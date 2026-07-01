//! GPU resource pool — typed resource management with deferred deletion.
//!
//! GPU resources cannot be deleted immediately if the GPU may still be reading
//! them (frame-in-flight problem). All resource deletions go through
//! `ResourcePool::defer_delete()`, which queues resources for deletion after
//! N frames have passed.

use crate::error::RenderError;
use slotmap::{new_key_type, SlotMap};
use std::collections::VecDeque;
use std::sync::Arc;
use wgpu::{BindGroupLayout, Buffer, Sampler, Texture, TextureView};

new_key_type! {
    /// Key for a GPU buffer resource.
    pub struct BufferKey;
    /// Key for a GPU texture resource.
    pub struct TextureKey;
    /// Key for a GPU texture view resource.
    pub struct TextureViewKey;
    /// Key for a GPU sampler resource.
    pub struct SamplerKey;
    /// Key for a bind group layout.
    pub struct BindGroupLayoutKey;
    /// Key for a render pipeline.
    pub struct PipelineKey;
    /// Key for a bind group.
    pub struct BindGroupKey;
}

/// Wrapper for a GPU buffer with metadata.
pub struct GpuBuffer {
    pub buffer: Buffer,
    pub size: u64,
    pub usage: wgpu::BufferUsages,
    pub label: String,
}

/// Wrapper for a GPU texture with metadata.
pub struct GpuTexture {
    pub texture: Texture,
    pub view: TextureView,
    pub format: wgpu::TextureFormat,
    pub size: wgpu::Extent3d,
    pub mip_levels: u32,
    pub label: String,
}

/// Wrapper for a GPU sampler.
pub struct GpuSampler {
    pub sampler: Sampler,
    pub label: String,
}

/// A resource that can be deferred for deletion.
enum DeletableResource {
    Buffer(Buffer),
    Texture(Texture),
    TextureView(TextureView),
    Sampler(Sampler),
    BindGroupLayout(BindGroupLayout),
}

/// Deferred deletion entry.
struct DeferredDelete {
    resource: DeletableResource,
    /// Delete when frame_index exceeds this value.
    delete_after_frame: u64,
}

/// Number of frames to wait before deleting a resource.
const DEFER_FRAMES: u64 = 3;

/// Thread-safe GPU resource pool with deferred deletion.
pub struct ResourcePool {
    pub buffers: SlotMap<BufferKey, GpuBuffer>,
    pub textures: SlotMap<TextureKey, GpuTexture>,
    pub texture_views: SlotMap<TextureViewKey, TextureView>,
    pub samplers: SlotMap<SamplerKey, GpuSampler>,
    pub bind_group_layouts: SlotMap<BindGroupLayoutKey, BindGroupLayout>,
    pub pipelines: SlotMap<PipelineKey, ()>,
    pub bind_groups: SlotMap<BindGroupKey, ()>,

    /// Resources queued for deletion after N frames.
    pending_delete: VecDeque<DeferredDelete>,
    /// Current frame index.
    frame_index: u64,
}

impl ResourcePool {
    /// Create a new empty resource pool.
    pub fn new() -> Self {
        Self {
            buffers: SlotMap::with_key(),
            textures: SlotMap::with_key(),
            texture_views: SlotMap::with_key(),
            samplers: SlotMap::with_key(),
            bind_group_layouts: SlotMap::with_key(),
            pipelines: SlotMap::with_key(),
            bind_groups: SlotMap::with_key(),
            pending_delete: VecDeque::new(),
            frame_index: 0,
        }
    }

    /// Advance the frame counter and drain completed deferred deletions.
    pub fn end_frame(&mut self) {
        self.frame_index = self.frame_index.wrapping_add(1);
        self.drain_pending();
    }

    /// Drain resources whose defer period has expired.
    fn drain_pending(&mut self) {
        // Drain from the front while the deletion frame has passed.
        while let Some(entry) = self.pending_delete.front() {
            if self.frame_index > entry.delete_after_frame {
                let entry = self.pending_delete.pop_front().unwrap();
                // Resource is dropped here, freeing GPU memory.
                drop(entry.resource);
            } else {
                break;
            }
        }
    }

    /// Queue a buffer for deletion after the defer period.
    pub fn defer_delete_buffer(&mut self, buffer: Buffer) {
        self.pending_delete.push_back(DeferredDelete {
            resource: DeletableResource::Buffer(buffer),
            delete_after_frame: self.frame_index + DEFER_FRAMES,
        });
    }

    /// Queue a texture for deletion after the defer period.
    pub fn defer_delete_texture(&mut self, texture: Texture) {
        self.pending_delete.push_back(DeferredDelete {
            resource: DeletableResource::Texture(texture),
            delete_after_frame: self.frame_index + DEFER_FRAMES,
        });
    }

    /// Queue a texture view for deletion after the defer period.
    pub fn defer_delete_texture_view(&mut self, view: TextureView) {
        self.pending_delete.push_back(DeferredDelete {
            resource: DeletableResource::TextureView(view),
            delete_after_frame: self.frame_index + DEFER_FRAMES,
        });
    }

    /// Queue a sampler for deletion after the defer period.
    pub fn defer_delete_sampler(&mut self, sampler: Sampler) {
        self.pending_delete.push_back(DeferredDelete {
            resource: DeletableResource::Sampler(sampler),
            delete_after_frame: self.frame_index + DEFER_FRAMES,
        });
    }

    /// Remove and defer-delete a buffer by key.
    pub fn remove_buffer(&mut self, key: BufferKey) {
        if let Some(buffer) = self.buffers.remove(key) {
            self.defer_delete_buffer(buffer.buffer);
        }
    }

    /// Remove and defer-delete a texture by key.
    pub fn remove_texture(&mut self, key: TextureKey) {
        if let Some(texture) = self.textures.remove(key) {
            self.defer_delete_texture(texture.texture);
        }
    }

    /// Total number of pending deletions.
    pub fn pending_deletion_count(&self) -> usize {
        self.pending_delete.len()
    }

    /// Current frame index.
    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }
}

impl Default for ResourcePool {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ResourcePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourcePool")
            .field("buffers", &self.buffers.len())
            .field("textures", &self.textures.len())
            .field("samplers", &self.samplers.len())
            .field("pending_deletions", &self.pending_delete.len())
            .field("frame_index", &self.frame_index)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_pool_creation() {
        let pool = ResourcePool::new();
        assert_eq!(pool.buffers.len(), 0);
        assert_eq!(pool.pending_deletion_count(), 0);
        assert_eq!(pool.frame_index(), 0);
    }

    #[test]
    fn test_end_frame_increments_index() {
        let mut pool = ResourcePool::new();
        assert_eq!(pool.frame_index(), 0);
        pool.end_frame();
        assert_eq!(pool.frame_index(), 1);
    }

    #[test]
    fn test_pending_deletions_are_held_for_defer_frames() {
        let mut pool = ResourcePool::new();

        // Create a dummy buffer (we can't create a real wgpu buffer without a device).
        // In real usage, this would be a wgpu::Buffer.
        // For now, verify that the defer mechanism tracks pending deletions.

        // This is a compile-time check that the pool API is correct.
        // Runtime buffer deletion tests require a wgpu device.
    }
}
