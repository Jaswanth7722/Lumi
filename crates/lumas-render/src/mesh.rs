//! GPU mesh resource management with LOD levels.
//!
//! The character mesh (Lumi) is stored as a single `GpuMesh` with three LOD levels
//! packed into the same vertex and index buffers. LOD selection is CPU-side using
//! screen size thresholds.

use crate::error::RenderError;
use glam::Vec3;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Unique mesh identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshId(pub u32);

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }
}

/// A single LOD level within a mesh.
#[derive(Debug, Clone, Copy)]
pub struct LodLevel {
    /// First index in the index buffer for this LOD.
    pub first_index: u32,
    /// Number of indices for this LOD.
    pub index_count: u32,
    /// Number of vertices for this LOD.
    pub vertex_count: u32,
    /// Switch to this LOD when the screen-space height (in pixels) is below this.
    pub screen_size_threshold: f32,
}

/// A vertex with skinning data.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CharacterVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    pub bone_indices: [u32; 4],
    pub bone_weights: [f32; 4],
}

/// A GPU-resident mesh with vertex and index buffers.
#[derive(Debug)]
pub struct GpuMesh {
    pub id: MeshId,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub vertex_count: u32,
    pub index_count: u32,
    pub index_format: wgpu::IndexFormat,
    pub aabb: Aabb,
    pub lod_levels: [LodLevel; 3],
}

impl GpuMesh {
    /// Create a new GPU mesh from CPU data.
    ///
    /// # Errors
    /// Returns `RenderError::ResourceNotFound` if buffer creation fails.
    pub fn new(
        id: MeshId,
        device: &wgpu::Device,
        vertices: &[CharacterVertex],
        indices: &[u32],
        lod_levels: [LodLevel; 3],
        label: &str,
    ) -> Result<Self, RenderError> {
        let vertex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{} vertices", label)),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        let index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{} indices", label)),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            }
        );

        // Compute AABB from vertices.
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in vertices {
            min = min.min(Vec3::from(v.position));
            max = max.max(Vec3::from(v.position));
        }

        Ok(Self {
            id,
            vertex_buffer,
            index_buffer,
            vertex_count: vertices.len() as u32,
            index_count: indices.len() as u32,
            index_format: wgpu::IndexFormat::Uint32,
            aabb: Aabb::new(min, max),
            lod_levels,
        })
    }

    /// Select the appropriate LOD level for a given screen-space height.
    pub fn select_lod(&self, screen_height_pixels: f32) -> &LodLevel {
        if screen_height_pixels < self.lod_levels[2].screen_size_threshold {
            &self.lod_levels[2]
        } else if screen_height_pixels < self.lod_levels[1].screen_size_threshold {
            &self.lod_levels[1]
        } else {
            &self.lod_levels[0]
        }
    }
}

/// Vertex buffer layout descriptor for character meshes.
pub fn character_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<CharacterVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            // Position (location 0)
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            // Normal (location 1)
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                shader_location: 1,
            },
            // Tangent (location 2)
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                shader_location: 2,
            },
            // UV (location 3)
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: (std::mem::size_of::<[f32; 3]>() * 2 + std::mem::size_of::<[f32; 4]>()) as wgpu::BufferAddress,
                shader_location: 3,
            },
            // Bone indices (location 4)
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32x4,
                offset: (std::mem::size_of::<[f32; 3]>() * 2 + std::mem::size_of::<[f32; 4]>() + std::mem::size_of::<[f32; 2]>()) as wgpu::BufferAddress,
                shader_location: 4,
            },
            // Bone weights (location 5)
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 3]>() * 2 + std::mem::size_of::<[f32; 4]>() + std::mem::size_of::<[f32; 2]>() + std::mem::size_of::<[u32; 4]>()) as wgpu::BufferAddress,
                shader_location: 5,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_id_creation() {
        let id = MeshId(42);
        assert_eq!(id.0, 42);
    }

    #[test]
    fn test_lod_level_screen_size_threshold() {
        let lod = LodLevel {
            first_index: 0,
            index_count: 100,
            vertex_count: 50,
            screen_size_threshold: 200.0,
        };
        assert_eq!(lod.screen_size_threshold, 200.0);
    }

    #[test]
    fn test_vertex_layout_size() {
        // position(12) + normal(12) + tangent(16) + uv(8) + bone_indices(16) + bone_weights(16) = 80
        assert_eq!(std::mem::size_of::<CharacterVertex>(), 80);
    }
}
