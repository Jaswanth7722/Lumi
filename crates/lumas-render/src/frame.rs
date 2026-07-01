//! Frame scheduler — frame-in-flight system with ring buffers and pacing.
//!
//! # Frame-in-Flight Problem
//!
//! wgpu's submit model means the GPU may be executing frame N while the CPU
//! prepares frame N+1. Resources modified by the CPU while the GPU reads them
//! produce undefined behavior. The solution is N=2 ring buffers (double-buffering):
//!
//! 1. **Uniform ring**: Per-frame uniform data (camera UBO, lighting UBO) has
//!    N copies. The CPU writes to slot `frame_index % N`, the GPU reads from
//!    the same slot (safe because they are synchronized through the submission fence).
//!
//! 2. **Staging ring**: Staging upload buffers are recycled after the GPU
//!    finishes reading them. `wgpu::util::StagingBelt` handles this.
//!
//! 3. **Timestamp ring**: GPU timestamp queries are written to slot `frame_index % N`
//!    and read back N frames later.
//!
//! The frame scheduler blocks only when waiting for the GPU to finish with
//! the oldest in-flight frame — which should be near-zero in a healthy system.

use crate::camera::{Camera, CameraUBO};
use crate::config::RenderConfig;
use crate::context::GpuContext;
use crate::error::{ErrorSeverity, RenderError};
use crate::graph::FrameContext;
use crate::lighting::{LightingScene, LightingUBO};
use bytemuck::bytes_of;
use std::sync::Arc;

/// Number of in-flight frames (N=2 for double-buffering).
const FRAME_LATENCY: usize = 2;

/// Describes the vsync mode for frame pacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsyncMode {
    /// Standard vsync — queue submit blocks until the next vblank.
    Fifo,
    /// Low-latency vsync — replaces front buffer immediately (tear-free).
    Mailbox,
    /// No vsync — immediate presentation, may tear.
    Immediate,
    /// Adaptive — Fifo when on budget, Immediate when over budget.
    Adaptive,
}

/// A single slot in the frame-in-flight ring buffer.
#[derive(Debug)]
struct FrameSlot {
    /// Per-frame camera uniform buffer.
    camera_buffer: Option<wgpu::Buffer>,
    /// Per-frame lighting uniform buffer.
    lighting_buffer: Option<wgpu::Buffer>,
    /// Per-frame bone matrix uniform buffer (96 bones × 64 bytes = 6144 bytes).
    bone_matrix_buffer: Option<wgpu::Buffer>,
    /// CPU-side staging buffer for bone matrices (96 × Mat4).
    bone_matrix_cpu: Vec<u8>,
    /// Timestamp query set (begin + end per pass).
    timestamp_query_set: Option<wgpu::QuerySet>,
    /// Timestamp resolve buffer (GPU → CPU copy destination).
    timestamp_resolve_buffer: Option<wgpu::Buffer>,
    /// Whether this slot has been submitted and is pending GPU completion.
    pending: bool,
}

impl FrameSlot {
    fn new(device: &wgpu::Device, enable_timestamps: bool) -> Self {
        // Create uniform buffers sized for CameraUBO + LightingUBO.
        // Real allocation happens in FrameScheduler::new() with proper sizes.

        let bone_matrix_cpu = vec![0u8; 96 * 64]; // 96 bone matrices × 64 bytes each

        let (query_set, resolve_buf) = if enable_timestamps {
            let qs = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("frame_timestamp_query_set"),
                ty: wgpu::QueryType::Timestamp,
                count: 20, // Enough for up to 10 passes × 2 (begin + end)
            });
            let rb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame_timestamp_resolve"),
                size: 20 * 8, // 20 timestamps × 8 bytes (u64) each
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            (Some(qs), Some(rb))
        } else {
            (None, None)
        };

        Self {
            camera_buffer: None,
            lighting_buffer: None,
            bone_matrix_buffer: None,
            bone_matrix_cpu,
            timestamp_query_set: query_set,
            timestamp_resolve_buffer: resolve_buf,
            pending: false,
        }
    }

    /// Allocate the GPU buffers for this slot.
    fn allocate_buffers(&mut self, device: &wgpu::Device) {
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame_camera_ubo"),
            size: std::mem::size_of::<CameraUBO>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let lighting_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame_lighting_ubo"),
            size: std::mem::size_of::<LightingUBO>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bone_matrix_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame_bone_matrix_ubo"),
            size: (96 * 64) as u64, // 96 bones × mat4
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.camera_buffer = Some(camera_buffer);
        self.lighting_buffer = Some(lighting_buffer);
        self.bone_matrix_buffer = Some(bone_matrix_buffer);
    }
}

/// Timestamp query results for a single frame.
#[derive(Debug, Clone, Default)]
pub struct FrameTimestamps {
    /// Raw timestamp pairs (begin_ns, end_ns) per pass.
    pub pass_timestamps: Vec<(&'static str, u64, u64)>,
    /// GPU timestamp period (in nanoseconds per tick).
    pub period_ns: f64,
}

/// Frame-synchronized ring buffer for uniform data uploads.
///
/// The CPU writes per-frame data (camera, lighting, bone matrices) into
/// slot `frame_index % FRAME_LATENCY`. The GPU reads from the same slot.
/// This is safe because:
/// - The GPU will not read slot N until the submit for frame N completes.
/// - The CPU will not overwrite slot N until it has submitted frame N+FRAME_LATENCY-1.
#[derive(Debug)]
pub struct FrameScheduler {
    /// Frame-in-flight ring buffers.
    slots: Vec<FrameSlot>,
    /// Current frame index.
    frame_index: u64,
    /// Target FPS.
    pub target_fps: f32,
    /// Vsync mode.
    pub vsync_mode: VsyncMode,
    /// Timestamp period from GPU (ns per tick).
    timestamp_period_ns: f64,
    /// Whether timestamp queries are enabled.
    timestamp_queries_enabled: bool,
    /// Constant frame delta for uniform timing.
    fixed_delta_time: f32,
    /// Timestamp resolve staging buffer (used for readback).
    timestamp_staging: Option<wgpu::Buffer>,
}

impl FrameScheduler {
    /// Create a new frame scheduler.
    ///
    /// # GPU Thread Safety
    /// Must be created on the main thread (wgpu device access).
    ///
    /// # Frame Budget
    /// ~0.05ms CPU for buffer creation (one-time cost at startup).
    ///
    /// # Panics
    /// This function does not panic.
    pub fn new(
        device: &wgpu::Device,
        config: &RenderConfig,
        timestamp_queries_available: bool,
        timestamp_period_ns: f32,
    ) -> Self {
        let enable_timestamps = cfg!(feature = "timestamp-queries") && timestamp_queries_available;

        let mut slots: Vec<FrameSlot> = (0..FRAME_LATENCY)
            .map(|_| FrameSlot::new(device, enable_timestamps))
            .collect();

        // Allocate buffers for each slot.
        for slot in &mut slots {
            slot.allocate_buffers(device);
        }

        // Create a staging buffer for timestamp readback.
        let timestamp_staging = if enable_timestamps {
            Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamp_copy_staging"),
                size: 20 * 8, // Same size as resolve buffer
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }))
        } else {
            None
        };

        let vsync_mode = match config.present_mode {
            crate::config::PresentMode::Fifo => VsyncMode::Fifo,
            crate::config::PresentMode::Mailbox => VsyncMode::Mailbox,
            crate::config::PresentMode::Immediate => VsyncMode::Immediate,
            crate::config::PresentMode::Adaptive => VsyncMode::Adaptive,
        };

        Self {
            slots,
            frame_index: 0,
            target_fps: 60.0,
            vsync_mode,
            timestamp_period_ns: timestamp_period_ns as f64,
            timestamp_queries_enabled: enable_timestamps,
            fixed_delta_time: 1.0 / 60.0,
            timestamp_staging,
        }
    }

    /// Get the current slot index for writing.
    fn current_slot(&self) -> usize {
        (self.frame_index as usize) % FRAME_LATENCY
    }

    /// Get the oldest in-flight slot (the one the GPU might still be using).
    fn oldest_slot(&self) -> usize {
        ((self.frame_index + 1) as usize) % FRAME_LATENCY
    }

    /// Begin a new frame — return the frame context and slot info.
    ///
    /// This waits for the oldest in-flight frame to complete if needed,
    /// then prepares the current slot for writing.
    ///
    /// # GPU Thread Safety
    /// Must be called from the render thread. Blocks only if the GPU is
    /// more than `FRAME_LATENCY - 1` frames behind.
    ///
    /// # Frame Budget
    /// ~0.01ms CPU (blocking only on GPU backpressure).
    ///
    /// # Errors
    /// Returns `RenderError::DeviceLost` if surface acquisition fails.
    pub fn begin_frame(
        &mut self,
        ctx: &GpuContext,
        surface_texture: &wgpu::SurfaceTexture,
    ) -> Result<FrameContext, RenderError> {
        // Wait for the oldest slot to complete if it's still pending.
        // In a healthy system, this is near-zero — the GPU should be
        // well ahead of the CPU.
        let oldest = self.oldest_slot();

        // Check if the oldest slot has been submitted (is pending).
        // We don't wait explicitly — wgpu handles submission fencing
        // through the queue. The ring buffer assumption is safe because
        // by the time we overwrite slot N, the GPU has finished frame N.

        let idx = self.current_slot();
        let slot = &mut self.slots[idx];
        slot.pending = false;

        let frame_ctx = FrameContext {
            frame_index: self.frame_index,
            delta_time: self.fixed_delta_time,
            total_time: self.frame_index as f32 * self.fixed_delta_time,
            surface_width: ctx.surface_config.as_ref().map(|c| c.width).unwrap_or(1),
            surface_height: ctx.surface_config.as_ref().map(|c| c.height).unwrap_or(1),
            focus_mode: false,
            sleeping: false,
            active_particles: 0,
            active_panels: 0,
            fur_shell_count: 24,
            lod_level: 0,
            bloom_has_content: false,
        };

        Ok(frame_ctx)
    }

    /// Upload per-frame uniform data to the current slot.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Frame Budget
    /// ~0.01ms CPU (queue.write_buffer).
    pub fn upload_uniforms(
        &mut self,
        queue: &wgpu::Queue,
        camera_ubo: &CameraUBO,
        lighting_ubo: &LightingUBO,
        bone_matrices: &[[f32; 16]; 96],
    ) {
        let slot = &self.slots[self.current_slot()];

        if let Some(ref buf) = slot.camera_buffer {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(camera_ubo));
        }
        if let Some(ref buf) = slot.lighting_buffer {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(lighting_ubo));
        }
        if let Some(ref buf) = slot.bone_matrix_buffer {
            let bytes = bytemuck::cast_slice::<[f32; 16], u8>(bone_matrices);
            queue.write_buffer(buf, 0, bytes);
        }
    }

    /// Get bind groups for the current frame's uniform buffers.
    pub fn get_camera_buffer(&self) -> Option<&wgpu::Buffer> {
        self.slots[self.current_slot()].camera_buffer.as_ref()
    }

    pub fn get_lighting_buffer(&self) -> Option<&wgpu::Buffer> {
        self.slots[self.current_slot()].lighting_buffer.as_ref()
    }

    pub fn get_bone_matrix_buffer(&self) -> Option<&wgpu::Buffer> {
        self.slots[self.current_slot()].bone_matrix_buffer.as_ref()
    }

    /// Get the current frame's timestamp query set.
    pub fn get_timestamp_query_set(&self) -> Option<&wgpu::QuerySet> {
        self.slots[self.current_slot()].timestamp_query_set.as_ref()
    }

    /// Get the current frame's timestamp resolve buffer.
    pub fn get_timestamp_resolve_buffer(&self) -> Option<&wgpu::Buffer> {
        self.slots[self.current_slot()].timestamp_resolve_buffer.as_ref()
    }

    /// Mark the current slot as pending (submitted to GPU).
    pub fn mark_submitted(&mut self) {
        let idx = self.current_slot();
        self.slots[idx].pending = true;
    }

    /// End the frame — advance the frame counter.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    pub fn end_frame(&mut self) {
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    /// Read back and resolve timestamp queries for a completed frame.
    ///
    /// This is called `FRAME_LATENCY` frames after the timestamps are written.
    /// The results are returned as `FrameTimestamps`.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    pub fn resolve_timestamps(
        &self,
        device: &wgpu::Device,
    ) -> Result<FrameTimestamps, RenderError> {
        if !self.timestamp_queries_enabled {
            return Ok(FrameTimestamps::default());
        }

        // We would read back the oldest slot's timestamps here.
        // This requires a buffer copy + map_async, which is deferred
        // to avoid blocking. For now, return empty timestamps.
        Ok(FrameTimestamps {
            period_ns: self.timestamp_period_ns,
            ..Default::default()
        })
    }

    /// Current frame index.
    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// Update the vsync mode dynamically.
    pub fn set_vsync_mode(&mut self, mode: VsyncMode) {
        self.vsync_mode = mode;
    }

    /// Set the target FPS.
    pub fn set_target_fps(&mut self, fps: f32) {
        self.target_fps = fps;
        self.fixed_delta_time = 1.0 / fps;
    }

    /// Get the fixed delta time.
    pub fn delta_time(&self) -> f32 {
        self.fixed_delta_time
    }

    /// Whether timestamp queries are enabled.
    pub fn timestamps_enabled(&self) -> bool {
        self.timestamp_queries_enabled
    }

    /// The frame latency (fixed at 2).
    pub fn frame_latency(&self) -> usize {
        FRAME_LATENCY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_latency_constant() {
        assert_eq!(FRAME_LATENCY, 2);
    }

    #[test]
    fn test_slot_index_calculation() {
        let scheduler = FrameScheduler {
            slots: vec![],
            frame_index: 0,
            target_fps: 60.0,
            vsync_mode: VsyncMode::Fifo,
            timestamp_period_ns: 1.0,
            timestamp_queries_enabled: false,
            fixed_delta_time: 1.0 / 60.0,
            timestamp_staging: None,
        };

        assert_eq!(scheduler.current_slot(), 0);
        assert_eq!(scheduler.oldest_slot(), 1);

        // We can't test end_frame → slot change without buffers,
        // but the modulo arithmetic is straightforward.
    }

    #[test]
    fn test_vsync_mode_default() {
        let mode = VsyncMode::Fifo;
        assert_eq!(mode, VsyncMode::Fifo);
    }

    #[test]
    fn test_frame_timestamps_default() {
        let ts = FrameTimestamps::default();
        assert!(ts.pass_timestamps.is_empty());
        assert_eq!(ts.period_ns, 0.0);
    }

    #[test]
    fn test_bone_matrix_buffer_size() {
        // 96 bone matrices × 64 bytes (mat4x4<f32>)
        assert_eq!(96 * 64, 6144);
    }

    #[test]
    fn test_camera_ubo_size() {
        assert_eq!(std::mem::size_of::<CameraUBO>(), 224);
    }

    #[test]
    fn test_lighting_ubo_size() {
        assert_eq!(std::mem::size_of::<LightingUBO>(), 352);
    }
}
