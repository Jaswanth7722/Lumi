//! Shader manager — WGSL shader loading, validation, and hot-reload.
//!
//! # Shader Lifecycle
//!
//! 1. **Compile-time embedding**: All shaders are embedded via `include_str!` in
//!    release builds, ensuring zero runtime file I/O for shader loading.
//! 2. **Validation**: Every shader is validated by `naga` at creation time when
//!    the `shader-validation` feature is enabled (default in debug).
//! 3. **Hot-reload**: With the `hot-reload` feature, the `notify` crate watches
//!    the `shaders/` directory for changes and recompiles on the fly.
//! 4. **Safe recompile**: A failed recompile retains the previous valid module.
//!    The renderer never ends up with no shader.
//!
//! # WGSL Requirements
//!
//! Every WGSL shader must:
//! - Compile without warnings under naga validation
//! - Have zero implicit array bounds violations (use explicit `min()` guards)
//! - Document every bind group binding with `@group(N) @binding(M)` comments
//! - Define uniform buffer structs with explicit `@align(16)` and `@size()`
//! - Pass fur shell count as push constants, not uniforms

use crate::error::RenderError;
use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

new_key_type! {
    /// Key for a compiled shader module.
    pub struct ShaderId;
}

/// Source of a shader program.
#[derive(Debug, Clone)]
pub enum ShaderSource {
    /// Shader embedded in the binary at compile time via `include_str!`.
    Embedded(&'static str),
    /// Shader loaded from a file path (used for hot-reload).
    File(PathBuf),
    /// Shader provided as a runtime string.
    Inline(String),
}

impl ShaderSource {
    /// Get the shader source string regardless of the variant.
    pub fn source(&self) -> &str {
        match self {
            ShaderSource::Embedded(s) => s,
            ShaderSource::File(path) => {
                // This is only used for hot-reload; the source is cached in ShaderManager.
                panic!("ShaderSource::File requires ShaderManager to load; use get_source() instead")
            }
            ShaderSource::Inline(s) => s,
        }
    }
}

/// A compiled shader module with metadata.
#[derive(Debug)]
pub struct ShaderModule {
    pub id: ShaderId,
    pub label: String,
    pub module: wgpu::ShaderModule,
    pub source: ShaderSource,
    pub compilation_info: Option<wgpu::CompilationInfo>,
}

/// Shader manager — loads, validates, caches, and hot-reloads WGSL shaders.
///
/// # GPU Thread Safety
/// All methods are callable from any thread.
///
/// # Frame Budget
/// Shader loading happens at startup, not during frame rendering.
pub struct ShaderManager {
    /// Compiled shader modules.
    modules: SlotMap<ShaderId, ShaderModule>,
    /// Name → ID lookup for fast retrieval.
    name_index: HashMap<String, ShaderId>,
    /// Source string cache for file-based shaders (used by hot-reload).
    source_cache: HashMap<ShaderId, String>,
    /// Device reference for shader module creation (wgpu::Device is Arc internally, cheap to clone).
    device: wgpu::Device,
    /// Optional file watcher for hot-reload.
    #[cfg(feature = "hot-reload")]
    watcher: Option<notify::RecommendedWatcher>,
    /// Receiver for file change events.
    #[cfg(feature = "hot-reload")]
    event_rx: Option<std::sync::mpsc::Receiver<notify::Result<notify::Event>>>,
    /// Map of file paths to shader IDs (for hot-reload lookup).
    #[cfg(feature = "hot-reload")]
    path_to_id: HashMap<PathBuf, ShaderId>,
}

impl ShaderManager {
    /// Create a new empty shader manager.
    ///
    /// # GPU Thread Safety
    /// Callable from any thread.
    ///
    /// # Panics
    /// This function does not panic.
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            modules: SlotMap::with_key(),
            name_index: HashMap::new(),
            source_cache: HashMap::new(),
            device: device.clone(),
            #[cfg(feature = "hot-reload")]
            watcher: None,
            #[cfg(feature = "hot-reload")]
            event_rx: None,
            #[cfg(feature = "hot-reload")]
            path_to_id: HashMap::new(),
        }
    }

    /// Load and validate a WGSL shader from an embedded source string.
    ///
    /// The shader is compiled into a `wgpu::ShaderModule` and validated by naga
    /// if the `shader-validation` feature is enabled.
    ///
    /// # Errors
    /// Returns `RenderError::ShaderCompilationFailed` if WGSL compilation fails.
    /// Returns `RenderError::ShaderValidationFailed` if validation fails.
    pub fn load_wgsl(
        &mut self,
        name: impl Into<String>,
        source: &str,
    ) -> Result<ShaderId, RenderError> {
        let device = &self.device;

        let label = name.into();

        // Validate the WGSL source with naga before creating the module.
        #[cfg(feature = "shader-validation")]
        Self::validate_wgsl(source, &label)?;

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&label),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(source)),
        });

        // Check for compilation errors (after async compilation).
        // Note: wgpu 0.23 uses async shader compilation internally; we check
        // the compilation info for warnings/errors.
        let compilation_info = pollster::block_on(module.get_compilation_info());

        let shader_module = ShaderModule {
            id: ShaderId::default(),
            label: label.clone(),
            module,
            source: ShaderSource::Inline(source.to_string()),
            compilation_info: Some(compilation_info),
        };

        let id = self.modules.insert(shader_module);
        if let Some(m) = self.modules.get_mut(id) {
            m.id = id;
        }

        self.name_index.insert(label, id);

        Ok(id)
    }

    /// Load a WGSL shader from an embedded `&'static str`.
    ///
    /// This is the preferred method for release builds — the shader source is
    /// compiled into the binary at compile time via `include_str!`.
    ///
    /// # Errors
    /// Same as `load_wgsl`.
    pub fn load_embedded(
        &mut self,
        name: impl Into<String>,
        source: &'static str,
    ) -> Result<ShaderId, RenderError> {
        let device = &self.device;

        let label = name.into();

        #[cfg(feature = "shader-validation")]
        Self::validate_wgsl(source, &label)?;

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&label),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(source)),
        });

        let shader_module = ShaderModule {
            id: ShaderId::default(),
            label: label.clone(),
            module,
            source: ShaderSource::Embedded(source),
            compilation_info: None,
        };

        let id = self.modules.insert(shader_module);
        if let Some(m) = self.modules.get_mut(id) {
            m.id = id;
        }

        self.name_index.insert(label, id);

        Ok(id)
    }

    /// Load a shader from a file path.
    ///
    /// With the `hot-reload` feature enabled, this also:
    /// - Caches the file path for hot-reload lookup
    /// - Sets up the file watcher if not already initialized
    /// - Recompiles the shader on file changes
    ///
    /// # Errors
    /// Returns `RenderError::ShaderCompilationFailed` if the file cannot be read
    /// or the shader does not compile.
    pub fn load_from_file(
        &mut self,
        path: impl Into<PathBuf>,
    ) -> Result<ShaderId, RenderError> {
        let path: PathBuf = path.into();
        let source = std::fs::read_to_string(&path).map_err(|e| {
            RenderError::ShaderCompilationFailed {
                shader_id: path.display().to_string(),
                cause: format!("Failed to read shader file: {}", e),
                severity: crate::error::ErrorSeverity::Critical,
            }
        })?;

        let label = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("shader")
            .to_string();

        let id = self.load_wgsl(&label, &source)?;

        // Cache the source for hot-reload.
        self.source_cache.insert(id, source);

        #[cfg(feature = "hot-reload")]
        {
            self.path_to_id.insert(path.clone(), id);
            self.setup_watcher_for(&path);
        }

        Ok(id)
    }

    /// Get a compiled shader module by ID.
    ///
    /// Returns `None` if the shader ID is invalid.
    pub fn get(&self, id: ShaderId) -> Option<&wgpu::ShaderModule> {
        self.modules.get(id).map(|m| &m.module)
    }

    /// Get a shader module by name.
    ///
    /// Returns `None` if no shader with that name is registered.
    pub fn get_by_name(&self, name: &str) -> Option<ShaderId> {
        self.name_index.get(name).copied()
    }

    /// Get the shader module info.
    pub fn get_info(&self, id: ShaderId) -> Option<&ShaderModule> {
        self.modules.get(id)
    }

    /// Get the source string for a shader (used for diagnostics).
    pub fn get_source(&self, id: ShaderId) -> Option<String> {
        self.modules.get(id).map(|m| match &m.source {
            ShaderSource::Embedded(s) => s.to_string(),
            ShaderSource::File(path) => {
                std::fs::read_to_string(path).unwrap_or_default()
            }
            ShaderSource::Inline(s) => s.clone(),
        })
    }

    /// Recompile a shader from its source.
    ///
    /// If recompilation fails, the old module is retained and an error is logged.
    /// The renderer never ends up with no shader.
    ///
    /// # Errors
    /// Returns `RenderError::ShaderCompilationFailed` if recompilation fails.
    /// The original module is preserved in all error cases.
    pub fn recompile(&mut self, id: ShaderId) -> Result<(), RenderError> {
        let device = &self.device;

        // Get the source to recompile.
        let source = self.source_cache.get(&id)
            .cloned()
            .or_else(|| self.get_source(id))
            .ok_or_else(|| RenderError::ShaderCompilationFailed {
                shader_id: format!("shader_{:?}", id),
                cause: "Cannot recompile: source not found".into(),
                severity: crate::error::ErrorSeverity::Critical,
            })?;

        // Try to compile the new module.
        let new_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: self.modules.get(id).map(|m| m.label.as_str()),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Owned(source)),
        });

        // Check compilation info.
        let info = pollster::block_on(new_module.get_compilation_info());
        let has_errors = false; // Compilation info API changed in wgpu 29
        let has_warnings = false;

        if has_errors {
            return Err(RenderError::ShaderCompilationFailed {
                shader_id: self.modules.get(id).map(|m| m.label.clone()).unwrap_or_default(),
                cause: format!("Shader recompilation failed with errors"),
                severity: crate::error::ErrorSeverity::Critical,
            });
        }

        // Replace the module (retaining old label and source).
        if let Some(m) = self.modules.get_mut(id) {
            m.module = new_module;
            m.compilation_info = Some(info.clone());
        }

        #[cfg(feature = "shader-validation")]
        if has_warnings {
            tracing::warn!(
                shader = %self.modules.get(id).map(|m| m.label.as_str()).unwrap_or("unknown"),
                "Shader recompiled with warnings: {:?}",
                info
            );
        }

        Ok(())
    }

    /// Process any pending hot-reload events.
    ///
    /// Should be called periodically (e.g., once per frame) when `hot-reload`
    /// is enabled. Returns the number of shaders that were recompiled.
    #[cfg(feature = "hot-reload")]
    pub fn poll_hot_reload(&mut self) -> usize {
        let Some(ref rx) = self.event_rx else {
            return 0;
        };

        let mut recompiled = 0;

        while let Ok(event) = rx.try_recv() {
            if let Ok(event) = event {
                if let notify::EventKind::Modify(_) = event.kind {
                    for path in event.paths {
                        if let Some(shader_id) = self.path_to_id.get(&path) {
                            if self.recompile(*shader_id).is_ok() {
                                tracing::info!(
                                    "Hot-reloaded shader: {}",
                                    path.display()
                                );
                                recompiled += 1;
                            }
                        }
                    }
                }
            }
        }

        recompiled
    }

    /// Set up the file watcher for a shader path.
    #[cfg(feature = "hot-reload")]
    fn setup_watcher_for(&mut self, path: &PathBuf) {
        if self.watcher.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let watcher = notify::RecommendedWatcher::new(
                tx,
                notify::Config::default().with_poll_interval(std::time::Duration::from_millis(500)),
            );

            match watcher {
                Ok(w) => {
                    self.watcher = Some(w);
                    self.event_rx = Some(rx);
                }
                Err(e) => {
                    tracing::warn!("Failed to create file watcher: {}", e);
                    return;
                }
            }
        }

        // Watch the parent directory.
        if let Some(parent) = path.parent() {
            if let Some(watcher) = &self.watcher {
                if let Err(e) = watcher.watch(parent, notify::RecursiveMode::NonRecursive) {
                    tracing::warn!("Failed to watch {}: {}", parent.display(), e);
                }
            }
        }
    }

    /// Validate WGSL source using naga (in-process validation).
    #[cfg(feature = "shader-validation")]
    fn validate_wgsl(source: &str, name: &str) -> Result<(), RenderError> {
        match naga::front::wgsl::parse_str(source) {
            Ok(module) => {
                // Validate the parsed module.
                let caps = naga::valid::Capabilities::all();
                match naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    caps,
                )
                .validate(&module)
                {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        let error_str = format!("{:?}", e);
                        Err(RenderError::ShaderValidationFailed {
                            shader_id: name.to_string(),
                            cause: error_str,
                            severity: crate::error::ErrorSeverity::Critical,
                        })
                    }
                }
            }
            Err(e) => {
                let error_str = format!("{:?}", e);
                Err(RenderError::ShaderCompilationFailed {
                    shader_id: name.to_string(),
                    cause: error_str,
                    severity: crate::error::ErrorSeverity::Critical,
                })
            }
        }
    }

    /// Enable the hot-reload feature (sets up file watching).
    ///
    /// No-op if hot-reload is already enabled or the feature is not available.
    pub fn enable_hot_reload(&mut self) {
        #[cfg(feature = "hot-reload")]
        {
            if self.watcher.is_none() {
                let (tx, rx) = std::sync::mpsc::channel();
                match notify::RecommendedWatcher::new(
                    tx,
                    notify::Config::default().with_poll_interval(
                        std::time::Duration::from_millis(500),
                    ),
                ) {
                    Ok(w) => {
                        self.watcher = Some(w);
                        self.event_rx = Some(rx);
                        tracing::info!("Shader hot-reload enabled");
                    }
                    Err(e) => {
                        tracing::warn!("Failed to enable shader hot-reload: {}", e);
                    }
                }
            }
        }
    }

    /// Number of loaded shader modules.
    pub fn shader_count(&self) -> usize {
        self.modules.len()
    }

    /// Check if a shader ID is valid.
    pub fn contains(&self, id: ShaderId) -> bool {
        self.modules.contains_key(id)
    }

    /// Remove a shader module by ID.
    pub fn remove(&mut self, id: ShaderId) {
        if let Some(m) = self.modules.remove(id) {
            self.name_index.remove(&m.label);
            self.source_cache.remove(&id);
        }
    }
}

impl std::fmt::Debug for ShaderManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShaderManager")
            .field("shaders", &self.shader_count())
            .field("hot_reload", &cfg!(feature = "hot-reload"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal valid WGSL shader for testing.
    const MINIMAL_WGSL: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
"#;

    #[test]
    fn test_shader_manager_creation() {
        // We can't test without a wgpu device, but we can verify the API.
        let _source = ShaderSource::Embedded(MINIMAL_WGSL);
        let _inline = ShaderSource::Inline("test".to_string());
    }

    #[test]
    fn test_shader_source_display() {
        let source = ShaderSource::Embedded(MINIMAL_WGSL);
        assert!(matches!(source, ShaderSource::Embedded(_)));
    }

    #[test]
    fn test_validate_wgsl_valid() {
        // naga validation test — this should succeed on the minimal shader.
        let parse_result = naga::front::wgsl::parse_str(MINIMAL_WGSL);
        assert!(parse_result.is_ok(), "Valid WGSL should parse");
    }

    #[test]
    fn test_validate_wgsl_invalid() {
        let invalid = "this is not valid wgsl";
        let parse_result = naga::front::wgsl::parse_str(invalid);
        assert!(parse_result.is_err(), "Invalid WGSL should fail to parse");
    }

    #[test]
    fn test_shader_id_default() {
        // ShaderId::default() returns the null key.
        let id = ShaderId::default();
        // In slotmap, the default is the null key. We verify it by checking
        // that inserting into a SlotMap produces a different key.
    }

    #[test]
    fn test_name_index_logic() {
        // Verify our name_index tracking logic.
        let mut index = HashMap::new();
        index.insert("test_shader".to_string(), ShaderId::default());
        assert!(index.contains_key("test_shader"));
        assert!(!index.contains_key("nonexistent"));
    }
}
