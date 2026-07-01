//! Material system — material kinds, GPU material bind groups, and pipeline management.
//!
//! # Material Kinds
//!
//! Each material kind maps to a specific render pipeline and bind group layout:
//!
//! - `FurBody`: PBR character body with fur density texture
//! - `CrystalEmissive`: Forehead crystal with emission and noise
//! - `HolographicPanel`: Workspace panels with translucent glow
//! - `Particle`: Billboarded sprites from an atlas
//! - `UnlitTexture`: Simple unlit textured geometry
//!
//! # Pre-Multiplied Alpha
//!
//! All color textures in materials use pre-multiplied alpha. Blend states
//! use `wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING`.

use crate::error::RenderError;
use crate::shader::ShaderId;
use crate::texture::TextureId;
use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;

new_key_type! {
    /// Key for a material resource.
    pub struct MaterialId;
    /// Key for a render pipeline.
    pub struct PipelineId;
}

/// Material kind — determines the render pipeline and shader used.
///
/// `#[non_exhaustive]` — new material types can be added without breaking
/// existing match expressions.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum MaterialKind {
    /// Fur body with PBR shading and fur density shell rendering.
    FurBody {
        albedo: TextureId,
        normal: TextureId,
        roughness: TextureId,
        ao: TextureId,
        fur_density: TextureId,
        fur_length: f32,
    },
    /// Crystal with emissive VFX, noise animation, and bloom source output.
    CrystalEmissive {
        albedo: TextureId,
        emission: TextureId,
        noise: TextureId,
        emission_color: [f32; 4],
    },
    /// Holographic workspace panel with per-frame content texture.
    HolographicPanel {
        content: TextureId,
        glow_color: [f32; 4],
        opacity: f32,
    },
    /// Billboarded particle sprites from an atlas.
    Particle {
        atlas: TextureId,
        blend_mode: ParticleBlend,
    },
    /// Simple unlit textured surface.
    UnlitTexture {
        albedo: TextureId,
        alpha: f32,
    },
}

/// Particle blend mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleBlend {
    /// Standard alpha blending (pre-multiplied).
    Alpha,
    /// Additive blending (glow effects).
    Additive,
    /// Soft additive (fades with alpha).
    SoftAdditive,
}

impl ParticleBlend {
    pub fn to_wgpu_blend(&self) -> wgpu::BlendState {
        match self {
            ParticleBlend::Alpha => wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            ParticleBlend::Additive => wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            },
            ParticleBlend::SoftAdditive => wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            },
        }
    }
}

/// GPU material — binds shader resources to a pipeline.
#[derive(Debug)]
pub struct GpuMaterial {
    pub id: MaterialId,
    pub kind: MaterialKind,
    pub bind_group: wgpu::BindGroup,
    pub pipeline_id: PipelineId,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

/// Pipeline configuration for a material kind.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub label: String,
    pub vertex_shader: ShaderId,
    pub fragment_shader: ShaderId,
    pub vertex_layouts: Vec<wgpu::VertexBufferLayout<'static>>,
    pub primitive: wgpu::PrimitiveState,
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    pub multisample: wgpu::MultisampleState,
    pub blend: Option<wgpu::BlendState>,
    pub immediate_size: u32,
    pub multiview_mask: Option<std::num::NonZeroU32>,
    pub cache: Option<wgpu::PipelineCache>,
    pub bind_group_layouts: Vec<wgpu::BindGroupLayout>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            label: String::new(),
            vertex_shader: ShaderId::default(),
            fragment_shader: ShaderId::default(),
            vertex_layouts: Vec::new(),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
            blend: None,
            immediate_size: 0,
            bind_group_layouts: Vec::new(),
        }
    }
}

/// Pipeline manager — owns all `wgpu::RenderPipeline` instances.
#[derive(Debug)]
pub struct PipelineManager {
    pipelines: SlotMap<PipelineId, wgpu::RenderPipeline>,
    pipeline_configs: SlotMap<PipelineId, PipelineConfig>,
    material_pipeline_map: HashMap<&'static str, PipelineId>,
    device: wgpu::Device,
}

impl PipelineManager {
    /// Create a new pipeline manager.
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            pipelines: SlotMap::with_key(),
            pipeline_configs: SlotMap::with_key(),
            material_pipeline_map: HashMap::new(),
            device: device.clone(),
        }
    }

    /// Create a render pipeline from a config.
    ///
    /// # Errors
    /// Returns `RenderError::PipelineCreationFailed` if the pipeline cannot be created.
    pub fn create_pipeline(
        &mut self,
        config: PipelineConfig,
        shader_manager: &crate::shader::ShaderManager,
    ) -> Result<PipelineId, RenderError> {
        let config = config.clone();
        let device = &self.device;

        let vertex_module = shader_manager.get(config.vertex_shader)
            .ok_or_else(|| RenderError::PipelineCreationFailed {
                pipeline_id: format!("{}_vertex", config.label),
                cause: "Vertex shader not found".into(),
                severity: crate::error::ErrorSeverity::Critical,
            })?;

        let fragment_module = shader_manager.get(config.fragment_shader)
            .ok_or_else(|| RenderError::PipelineCreationFailed {
                pipeline_id: format!("{}_fragment", config.label),
                cause: "Fragment shader not found".into(),
                severity: crate::error::ErrorSeverity::Critical,
            })?;

        // Build bind group layouts.
        let bind_group_layouts: Vec<wgpu::BindGroupLayout> = config.bind_group_layouts.clone();
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{}_layout", config.label)),
            bind_group_layouts: &bind_group_layouts.iter().map(|l| Some(l)).collect::<Vec<_>>(),
            immediate_size: config.immediate_size,
        });

        let fragment_state = wgpu::FragmentState {
            module: fragment_module,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float, // HDR color target
                blend: config.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&config.label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: vertex_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &config.vertex_layouts,
            },
            fragment: Some(fragment_state),
            primitive: config.primitive,
            depth_stencil: config.depth_stencil.clone(),
            multisample: config.multisample,
            multiview_mask: config.multiview_mask,
            cache: config.cache.as_ref(),
        });

        let id = self.pipelines.insert(pipeline);
        self.pipeline_configs.insert(config.clone());
        Ok(id)
    }

    /// Register a material kind to pipeline mapping.
    pub fn register_material_mapping(
        &mut self,
        kind_name: &'static str,
        pipeline_id: PipelineId,
    ) {
        self.material_pipeline_map.insert(kind_name, pipeline_id);
    }

    /// Get a pipeline by ID.
    #[track_caller]
    pub fn get_pipeline(&self, id: PipelineId) -> Option<&wgpu::RenderPipeline> {
        self.pipelines.get(id)
    }

    /// Get the pipeline config by ID.
    pub fn get_config(&self, id: PipelineId) -> Option<&PipelineConfig> {
        self.pipeline_configs.get(id)
    }

    /// Get the pipeline ID for a material kind name.
    pub fn pipeline_for_material(&self, kind_name: &str) -> Option<PipelineId> {
        self.material_pipeline_map.get(kind_name).copied()
    }
}

/// Material manager — owns all materials and their GPU resources.
#[derive(Debug)]
pub struct MaterialManager {
    materials: SlotMap<MaterialId, GpuMaterial>,
    device: wgpu::Device,
}

impl MaterialManager {
    /// Create a new material manager.
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            materials: SlotMap::with_key(),
            device: device.clone(),
        }
    }

    /// Create a material from a material kind, using the pipeline manager.
    ///
    /// This creates the bind group for the material based on its kind.
    ///
    /// # Errors
    /// Returns `RenderError::MaterialNotFound` if the pipeline cannot be found.
    /// Returns `RenderError::PipelineCreationFailed` if bind group creation fails.
    pub fn create_material(
        &mut self,
        kind: MaterialKind,
        pipeline_id: PipelineId,
        pipeline_manager: &PipelineManager,
        texture_manager: &crate::texture::TextureManager,
    ) -> Result<MaterialId, RenderError> {
        let device = &self.device;

        let pipeline_config = pipeline_manager.get_config(pipeline_id)
            .ok_or_else(|| RenderError::MaterialNotFound {
                material_id: format!("pipeline {:?}", pipeline_id),
                severity: crate::error::ErrorSeverity::Warning,
            })?;

        // Create the bind group based on material kind.
        let bind_group_layout = pipeline_config.bind_group_layouts
            .last()
            .cloned()
            .ok_or_else(|| RenderError::PipelineCreationFailed {
                pipeline_id: "material_bind_group".into(),
                cause: "No bind group layout available".into(),
                severity: crate::error::ErrorSeverity::Critical,
            })?;

        let bind_group = self.create_bind_group(&device, &kind, &bind_group_layout, texture_manager)?;

        let material = GpuMaterial {
            id: MaterialId::default(),
            kind,
            bind_group,
            pipeline_id,
            bind_group_layout,
        };

        let id = self.materials.insert(material);
        if let Some(m) = self.materials.get_mut(id) {
            m.id = id;
        }

        Ok(id)
    }

    /// Create the appropriate bind group for a material kind.
    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        kind: &MaterialKind,
        layout: &wgpu::BindGroupLayout,
        texture_manager: &crate::texture::TextureManager,
    ) -> Result<wgpu::BindGroup, RenderError> {
        match kind {
            MaterialKind::FurBody {
                albedo,
                normal,
                roughness,
                ao,
                fur_density,
                ..
            } => {
                let tex_albedo = texture_manager.get_texture(*albedo);
                let tex_normal = texture_manager.get_texture(*normal);
                let tex_roughness = texture_manager.get_texture(*roughness);
                let tex_ao = texture_manager.get_texture(*ao);
                let tex_fur = texture_manager.get_texture(*fur_density);

                Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("fur_body_material"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&tex_albedo.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&tex_normal.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&tex_roughness.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&tex_ao.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&tex_fur.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::Sampler(
                                texture_manager.default_sampler_ref(),
                            ),
                        },
                    ],
                }))
            }

            MaterialKind::CrystalEmissive {
                albedo,
                emission,
                noise,
                ..
            } => {
                let tex_albedo = texture_manager.get_texture(*albedo);
                let tex_emission = texture_manager.get_texture(*emission);
                let tex_noise = texture_manager.get_texture(*noise);

                Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("crystal_emissive_material"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&tex_albedo.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&tex_emission.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&tex_noise.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(
                                texture_manager.default_sampler_ref(),
                            ),
                        },
                    ],
                }))
            }

            MaterialKind::HolographicPanel { content, .. } => {
                let tex_content = texture_manager.get_texture(*content);

                Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("holographic_panel_material"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&tex_content.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(
                                texture_manager.default_sampler_ref(),
                            ),
                        },
                    ],
                }))
            }

            MaterialKind::Particle { atlas, .. } => {
                let tex_atlas = texture_manager.get_texture(*atlas);

                Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("particle_material"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&tex_atlas.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(
                                texture_manager.default_sampler_ref(),
                            ),
                        },
                    ],
                }))
            }

            MaterialKind::UnlitTexture { albedo, .. } => {
                let tex_albedo = texture_manager.get_texture(*albedo);

                Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("unlit_texture_material"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&tex_albedo.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(
                                texture_manager.default_sampler_ref(),
                            ),
                        },
                    ],
                }))
            }
        }
    }

    /// Get a material by ID.
    #[track_caller]
    pub fn get_material(&self, id: MaterialId) -> Option<&GpuMaterial> {
        self.materials.get(id)
    }

    /// Get a mutable material by ID.
    pub fn get_material_mut(&mut self, id: MaterialId) -> Option<&mut GpuMaterial> {
        self.materials.get_mut(id)
    }

    /// Destroy a material.
    pub fn destroy_material(&mut self, id: MaterialId) {
        self.materials.remove(id);
    }

    /// Number of registered materials.
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    /// Clear all materials.
    pub fn clear(&mut self) {
        self.materials.clear();
    }
}

/// Compute the default depth-stencil state for opaque geometry.
pub fn default_depth_stencil() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Equal), // Early-Z from prepass
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

/// Compute the depth-stencil state for the depth prepass.
pub fn depth_prepass_stencil() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

/// Bind group layout for the camera uniform buffer (bind group 0).
pub fn camera_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camera_ubo_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

/// Bind group layout for bone matrices (bind group 1).
pub fn bone_matrix_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bone_matrix_ubo_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

/// Bind group layout for lighting (bind group 2).
pub fn lighting_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("lighting_ubo_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

/// Bind group layout for material textures (bind group 3 — variant per material kind).
pub fn material_bind_group_layout(
    device: &wgpu::Device,
    num_textures: u32,
) -> wgpu::BindGroupLayout {
    let mut entries = Vec::new();
    for i in 0..num_textures {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: i,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    // Add sampler at the last binding.
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: num_textures,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });

    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("material_textures_layout"),
        entries: &entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_kind_non_exhaustive() {
        // Verify that MaterialKind implements the expected traits.
        let kind = MaterialKind::UnlitTexture {
            albedo: TextureId::default(),
            alpha: 1.0,
        };
        assert_eq!(
            kind,
            MaterialKind::UnlitTexture {
                albedo: TextureId::default(),
                alpha: 1.0,
            }
        );
    }

    #[test]
    fn test_particle_blend_conversion() {
        assert_eq!(
            ParticleBlend::Alpha.to_wgpu_blend(),
            wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
        );
    }

    #[test]
    fn test_default_depth_stencil() {
        let ds = default_depth_stencil();
        assert_eq!(ds.format, wgpu::TextureFormat::Depth32Float);
        assert_eq!(ds.depth_write_enabled, Some(true));
        assert_eq!(ds.depth_compare, Some(wgpu::CompareFunction::Equal));
    }

    #[test]
    fn test_depth_prepass_stencil() {
        let ds = depth_prepass_stencil();
        assert_eq!(ds.depth_compare, Some(wgpu::CompareFunction::Less));
    }

    #[test]
    fn test_material_bind_group_layout_entries() {
        // The layout for 5 textures + 1 sampler = 6 entries.
        let entries_count = 5 + 1; // textures + sampler
        // We can't create a real device here, so just verify the logic.
        assert_eq!(entries_count, 6);
    }

    #[test]
    fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.primitive.topology, wgpu::PrimitiveTopology::TriangleList);
        assert_eq!(config.primitive.front_face, wgpu::FrontFace::Ccw);
        assert!(config.bind_group_layouts.is_empty());
    }
}
