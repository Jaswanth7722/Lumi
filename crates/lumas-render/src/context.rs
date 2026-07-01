//! GPU context — adapter, device, queue, and surface management.
//!
//! The `GpuContext` is the root of all GPU interaction. It is created once
//! during initialization and holds the wgpu instance, adapter, device, queue,
//! and surface configuration.
//!
//! # Adapter Selection
//!
//! Adapter selection uses a scoring system that prefers integrated GPUs on
//! laptops (shared memory = better for frequent small uploads) and Metal on
//! macOS (lower driver overhead than MoltenVK).

use crate::config::{GpuBackend, RenderConfig};
use crate::error::RenderError;
use std::sync::Arc;

/// GPU context — the root of all GPU interaction.
pub struct GpuContext {
    /// The wgpu instance.
    pub instance: wgpu::Instance,
    /// The selected GPU adapter.
    pub adapter: wgpu::Adapter,
    /// The GPU device.
    pub device: wgpu::Device,
    /// The GPU command queue.
    pub queue: wgpu::Queue,
    /// The surface to render to.
    pub surface: Option<wgpu::Surface<'static>>,
    /// Surface configuration.
    pub surface_config: Option<wgpu::SurfaceConfiguration>,
    /// Adapter information.
    pub adapter_info: wgpu::AdapterInfo,
    /// Device limits.
    pub limits: wgpu::Limits,
    /// Enabled device features.
    pub features: wgpu::Features,
    /// Detected GPU backend.
    pub backend: GpuBackend,
    /// Timestamp period in nanoseconds (from adapter).
    pub timestamp_period_ns: f32,
    /// Whether timestamp queries are available.
    pub timestamp_queries_available: bool,
}

impl GpuContext {
    /// Create a new GPU context, selecting the best adapter and creating the device.
    ///
    /// # Errors
    /// Returns `RenderError::AdapterNotFound` if no suitable adapter is found.
    /// Returns `RenderError::FeatureNotSupported` if required features are missing.
    pub async fn new(
        raw_handle: Option<&raw_window_handle::RawWindowHandle>,
        config: &RenderConfig,
    ) -> Result<Self, RenderError> {
        let instance = create_instance(config);

        // Request adapter with scoring.
        let adapter = select_adapter(&instance, config).await
            .ok_or_else(|| RenderError::adapter_not_found(
                "No GPU adapter matching requirements found"
            ))?;

        let adapter_info = adapter.get_info();
        let backend = match adapter_info.backend {
            wgpu::Backend::Metal => GpuBackend::Metal,
            wgpu::Backend::Dx12 => GpuBackend::Dx12,
            wgpu::Backend::Vulkan => GpuBackend::Vulkan,
            wgpu::Backend::Gl => GpuBackend::Gl,
            _ => GpuBackend::Vulkan,
        };

        // Request device features.
        let required_features = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let timestamp_features = wgpu::Features::TIMESTAMP_QUERY;

        let adapter_features = adapter.features();
        let timestamp_queries_available = adapter_features.contains(timestamp_features);

        let mut requested_features = required_features;
        if cfg!(feature = "timestamp-queries") && timestamp_queries_available {
            requested_features |= timestamp_features;
        }
        if cfg!(feature = "gpu-debug") {
            requested_features |= wgpu::Features::CLEAR_TEXTURE;
        }

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Lumas Render Device"),
                    required_features: requested_features,
                    required_limits: wgpu::Limits::default(),
                    experimental_features: wgpu::ExperimentalFeatures::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                    trace: wgpu::Trace::Off,
                },
            )
            .await
            .map_err(|e| RenderError::device_lost(format!("Device creation failed: {}", e)))?;

        // Create surface if a window handle is provided.
        // In production, the lumi-desktop crate holds the window and provides
        // a `RawWindowHandle` with a stable address. The surface is recreated
        // if the window is destroyed and recreated.
        let (surface, surface_config) = if let Some(handle) = raw_handle {
            // wgpu 29.0 uses `SurfaceTargetUnsafe::RawHandle` for raw handle surface creation.
            // SAFETY: The caller (DesktopManager) guarantees the window outlives GpuContext.
            //
            // The display handle is platform-specific. Use the correct constructor
            // for each platform instead of std::mem::zeroed() which is unsound for
            // non-zeroed enum variants.
            let raw_display = create_display_handle();
            let target = wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_window_handle: *handle,
                raw_display_handle: Some(raw_display),
            };
            let surface = unsafe {
                instance.create_surface_unsafe(target)
                    .map_err(|e| RenderError::device_lost(format!("Surface creation failed: {}", e)))?
            };

            let caps = surface.get_capabilities(&adapter);
            let format = config.surface_format.unwrap_or_else(|| {
                *caps.formats.first().unwrap_or(&wgpu::TextureFormat::Bgra8UnormSrgb)
            });

            let composite_alpha = if caps
                .alpha_modes
                .contains(&config.composite_alpha.to_wgpu())
            {
                config.composite_alpha.to_wgpu()
            } else {
                wgpu::CompositeAlphaMode::Opaque
            };

            let sc_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: config.surface_width.max(1),
                height: config.surface_height.max(1),
                present_mode: config.present_mode.to_wgpu(),
                alpha_mode: composite_alpha,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };

            surface.configure(&device, &sc_config);
            (Some(surface), Some(sc_config))
        } else {
            (None, None)
        };

        // Get timestamp period.
        let timestamp_period_ns = if timestamp_queries_available {
            queue.get_timestamp_period()
        } else {
            1.0
        };

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surface,
            surface_config,
            adapter_info,
            limits: wgpu::Limits::default(),
            features: requested_features,
            backend,
            timestamp_period_ns,
            timestamp_queries_available,
        })
    }

    /// Reconfigure the surface after a window resize or DPI change.
    ///
    /// # Errors
    /// Returns `RenderError::SurfaceOutdated` if surface configuration fails.
    pub fn reconfigure_surface(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if let Some(config) = &mut self.surface_config {
            config.width = width.max(1);
            config.height = height.max(1);
            if let Some(ref surface) = self.surface {
                surface.configure(&self.device, config);
            }
        }
        Ok(())
    }

    /// Update the surface configuration with a new present mode.
    pub fn set_present_mode(&mut self, present_mode: wgpu::PresentMode) {
        if let Some(config) = &mut self.surface_config {
            config.present_mode = present_mode;
            if let Some(ref surface) = self.surface {
                surface.configure(&self.device, config);
            }
        }
    }

    /// Get the current surface texture for rendering.
    ///
    /// # Errors
    /// Returns `RenderError::SurfaceTimeout` if the surface texture cannot be acquired.
    pub fn get_surface_texture(&self) -> Result<wgpu::SurfaceTexture, RenderError> {
        let surface = self.surface.as_ref()
            .ok_or(RenderError::SurfaceOutdated { severity: crate::error::ErrorSeverity::Recoverable })?;

        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Ok(texture),
            wgpu::CurrentSurfaceTexture::Timeout => Err(RenderError::SurfaceTimeout {
                severity: crate::error::ErrorSeverity::Warning,
            }),
            wgpu::CurrentSurfaceTexture::Outdated => Err(RenderError::SurfaceOutdated {
                severity: crate::error::ErrorSeverity::Recoverable,
            }),
            wgpu::CurrentSurfaceTexture::Lost => Err(RenderError::DeviceLost {
                reason: "Surface lost".into(),
                severity: crate::error::ErrorSeverity::Critical,
            }),
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Validation => {
                Err(RenderError::SurfaceTimeout {
                    severity: crate::error::ErrorSeverity::Warning,
                })
            }
        }
    }

    /// Resize the surface to new dimensions.
    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(config) = &mut self.surface_config {
            config.width = width.max(1);
            config.height = height.max(1);
            if let Some(ref surface) = self.surface {
                surface.configure(&self.device, config);
            }
        }
    }
}

/// Create the wgpu instance based on configuration.
fn create_instance(config: &RenderConfig) -> wgpu::Instance {
    let backends = match config.preferred_backend {
        Some(b) => wgpu::Backends::from(b.to_wgpu()),
        None => wgpu::Backends::all(),
    };

    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: if config.gpu_debug {
            wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::DEBUG
        } else {
            wgpu::InstanceFlags::empty()
        },
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    })
}

/// Score a GPU adapter for selection.
///
/// Scoring prioritizes:
/// 1. Integrated GPUs (shared memory = better for frequent small uploads)
/// 2. Metal on macOS (lower driver overhead than MoltenVK)
/// 3. Native backends over translation layers
fn score_adapter(info: &wgpu::AdapterInfo) -> u32 {
    let mut score = 0u32;

    // Prefer integrated GPU for Lumi's use case (frequent small uploads).
    score += match info.device_type {
        wgpu::DeviceType::IntegratedGpu => 300,
        wgpu::DeviceType::DiscreteGpu => 200,
        wgpu::DeviceType::VirtualGpu => 100,
        wgpu::DeviceType::Cpu => 50,
        _ => 0,
    };

    // Prefer native backends with lower driver overhead.
    score += match info.backend {
        wgpu::Backend::Metal => 100,
        wgpu::Backend::Dx12 => 80,
        wgpu::Backend::Vulkan => 60,
        wgpu::Backend::Gl => 10,
        _ => 0,
    };

    // Bonus for known good hardware.
    if info.name.contains("Apple M") || info.name.contains("Intel") {
        score += 50;
    }

    score
}

/// Select the best GPU adapter from all available backends.
async fn select_adapter(instance: &wgpu::Instance, config: &RenderConfig) -> Option<wgpu::Adapter> {
    let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;

    if adapters.is_empty() {
        return None;
    }

    // Score all adapters and pick the best.
    let mut scored: Vec<(u32, wgpu::Adapter)> = adapters
        .into_iter()
        .map(|adapter| {
            let info = adapter.get_info();
            let score = score_adapter(&info);

            // Apply preference for integrated GPU.
            let score = if config.prefer_integrated_gpu
                && info.device_type == wgpu::DeviceType::IntegratedGpu
            {
                score + 100
            } else {
                score
            };

            (score, adapter)
        })
        .collect();

    // Sort by score descending.
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    scored.into_iter().next().map(|(_, adapter)| adapter)
}

/// Create a platform-appropriate display handle for wgpu surface creation.
///
/// # Platform Notes
///
/// - **Windows**: `WindowsDisplayHandle` is a zero-sized marker — `::new()` is perfectly safe.
/// - **macOS / iOS**: `AppKitDisplayHandle` / `UiKitDisplayHandle` are zero-sized markers.
/// - **Linux / Unix**: Falls back to Xlib with `None` display, which tells wgpu to use
///   the default X11 display connection. On Wayland-only systems, a proper display
///   connection handle must be passed from the windowing layer instead.
fn create_display_handle() -> raw_window_handle::RawDisplayHandle {
    #[cfg(target_os = "windows")]
    {
        raw_window_handle::RawDisplayHandle::Windows(raw_window_handle::WindowsDisplayHandle::new())
    }
    #[cfg(target_os = "macos")]
    {
        raw_window_handle::RawDisplayHandle::AppKit(raw_window_handle::AppKitDisplayHandle::new())
    }
    #[cfg(target_os = "ios")]
    {
        raw_window_handle::RawDisplayHandle::UiKit(raw_window_handle::UiKitDisplayHandle::new())
    }
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    {
        // X11 fallback with a null display pointer. wgpu will attempt to open
        // the default X11 display ($DISPLAY). On pure Wayland systems, this will
        // fail — the caller should pass a real display handle instead.
        raw_window_handle::RawDisplayHandle::Xlib(
            raw_window_handle::XlibDisplayHandle::new(None, 0),
        )
    }
    #[cfg(target_os = "android")]
    {
        raw_window_handle::RawDisplayHandle::Android(
            raw_window_handle::AndroidDisplayHandle::new(),
        )
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        unix,
    )))]
    {
        compile_error!("Unsupported platform: no display handle constructor available.");
    }
}

/// Create a headless GPU context for testing without a display.
#[cfg(feature = "headless")]
pub async fn create_headless_context(config: &RenderConfig) -> Result<GpuContext, RenderError> {
    let instance = create_instance(config);
    let adapter = select_adapter(&instance, config)
        .await
        .ok_or_else(|| RenderError::adapter_not_found("No adapter for headless rendering"))?;

    let adapter_info = adapter.get_info();
    let backend = match adapter_info.backend {
        wgpu::Backend::Metal => GpuBackend::Metal,
        wgpu::Backend::Dx12 => GpuBackend::Dx12,
        wgpu::Backend::Vulkan => GpuBackend::Vulkan,
        wgpu::Backend::Gl => GpuBackend::Gl,
        _ => GpuBackend::Vulkan,
    };

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Lumas Render Device (Headless)"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            },
            None,
        )
        .await
        .map_err(|e| RenderError::device_lost(format!("Headless device creation failed: {}", e)))?;

    Ok(GpuContext {
        instance,
        adapter,
        device,
        queue,
        surface: None,
        surface_config: None,
        adapter_info,
        limits: wgpu::Limits::default(),
        features: wgpu::Features::empty(),
        backend,
        timestamp_period_ns: 1.0,
        timestamp_queries_available: false,
    })
}

impl std::fmt::Debug for GpuContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuContext")
            .field("adapter", &self.adapter_info.name)
            .field("backend", &format!("{:?}", self.backend))
            .field("surface", &self.surface.is_some())
            .finish()
    }
}
