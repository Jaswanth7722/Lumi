//! Render graph — directed acyclic graph of render passes.
//!
//! A render graph organizes rendering as a DAG where:
//! - **Nodes** are render passes
//! - **Edges** are resource dependencies
//! - **Compilation** resolves execution order, infers barriers, and culls unused passes
//!
//! This design allows the set of active passes to change per-frame without
//! a rats-nest of conditional logic in a sequential render function.

use crate::context::GpuContext;
use crate::error::RenderError;
use crate::resource::ResourcePool;
use crate::scene::ShadowInstanceGPU;
use indexmap::IndexMap;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Unique identifier for a render pass within the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId(pub u32);

/// Resource identifier within the render graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphResourceId(pub u32);

/// Declared resource usage for a pass.
#[derive(Debug, Clone)]
pub struct PassResourceUsage {
    /// Textures read by this pass.
    pub textures_read: Vec<GraphResourceId>,
    /// Textures written by this pass.
    pub textures_written: Vec<GraphResourceId>,
    /// Buffers read by this pass.
    pub buffers_read: Vec<GraphResourceId>,
    /// Buffers written by this pass.
    pub buffers_written: Vec<GraphResourceId>,
}

impl Default for PassResourceUsage {
    fn default() -> Self {
        Self {
            textures_read: Vec::new(),
            textures_written: Vec::new(),
            buffers_read: Vec::new(),
            buffers_written: Vec::new(),
        }
    }
}

/// Builder for declaring pass resource usage.
pub struct PassResourceBuilder {
    usage: PassResourceUsage,
}

impl PassResourceBuilder {
    /// Declare that this pass reads a texture.
    pub fn read_texture(&mut self, id: GraphResourceId) -> &mut Self {
        self.usage.textures_read.push(id);
        self
    }

    /// Declare that this pass writes to a texture.
    pub fn write_texture(&mut self, id: GraphResourceId) -> &mut Self {
        self.usage.textures_written.push(id);
        self
    }

    /// Build the resource usage declaration.
    pub fn build(&self) -> PassResourceUsage {
        self.usage.clone()
    }
}

/// Per-frame context passed to every render pass.
#[derive(Debug, Clone, Default)]
pub struct FrameContext {
    /// Current frame index.
    pub frame_index: u64,
    /// Frame delta time in seconds.
    pub delta_time: f32,
    /// Total elapsed time in seconds.
    pub total_time: f32,
    /// Surface width in physical pixels.
    pub surface_width: u32,
    /// Surface height in physical pixels.
    pub surface_height: u32,
    /// Whether the character is in focus mode.
    pub focus_mode: bool,
    /// Whether the character is sleeping.
    pub sleeping: bool,
    /// Active particle count.
    pub active_particles: u32,
    /// Number of active workspace panels.
    pub active_panels: u32,
    /// Current fur shell count (LOD).
    pub fur_shell_count: u32,
    /// Current LOD level (0=high, 1=medium, 2=low).
    pub lod_level: u8,
    /// Whether bloom source has content this frame.
    pub bloom_has_content: bool,
}

/// Resolved GPU resources for a compiled pass.
///
/// Populated by the graph compiler from resource declarations. Each pass
/// reads the resources it declared via `declare_resources()`.
pub struct ResolvedResources {
    /// Depth texture for the depth prepass (read by all geometry passes).
    pub depth_texture: Option<wgpu::TextureView>,
    /// HDR color texture (the main framebuffer).
    pub color_texture: Option<wgpu::TextureView>,
    /// Bloom source texture (written by CrystalVFX, read by Bloom).
    pub bloom_source: Option<wgpu::TextureView>,
    /// LDR output buffer (after tone mapping).
    pub output_texture: Option<wgpu::TextureView>,
    /// Surface texture (the swapchain).
    pub surface_texture: Option<wgpu::TextureView>,
    /// Character mesh vertex/index buffers (set by the scene).
    pub character_vertex_buffer: Option<wgpu::Buffer>,
    pub character_index_buffer: Option<wgpu::Buffer>,
    pub character_index_count: u32,
    /// Camera UBO (bind group 0).
    pub camera_bind_group: Option<wgpu::BindGroup>,
    /// Bone matrix buffer (bind group 1).
    pub bone_matrix_buffer: Option<wgpu::Buffer>,
    pub bone_matrix_bind_group: Option<wgpu::BindGroup>,
    /// Lighting UBO (bind group 2).
    pub lighting_bind_group: Option<wgpu::BindGroup>,
    /// Material bind group (bind group 3, per-material).
    pub material_bind_group: Option<wgpu::BindGroup>,
    /// Shadow instance data (written to GPU storage buffer before shadow pass).
    pub shadow_instance: Option<ShadowInstanceGPU>,
}

impl ResolvedResources {
    pub fn new() -> Self {
        Self {
            depth_texture: None,
            color_texture: None,
            bloom_source: None,
            output_texture: None,
            surface_texture: None,
            character_vertex_buffer: None,
            character_index_buffer: None,
            character_index_count: 0,
            camera_bind_group: None,
            bone_matrix_buffer: None,
            bone_matrix_bind_group: None,
            lighting_bind_group: None,
            material_bind_group: None,
            shadow_instance: None,
        }
    }

    /// Returns `true` if the character mesh is available for rendering.
    pub fn has_character_mesh(&self) -> bool {
        self.character_vertex_buffer.is_some() && self.character_index_buffer.is_some()
    }
}

/// A single pass in the render graph.
pub trait RenderPass: Send + Sync {
    /// Unique pass identifier.
    fn id(&self) -> PassId;

    /// Human-readable pass name (for debugging and metrics).
    fn name(&self) -> &'static str;

    /// Declare resource reads and writes for dependency resolution.
    fn declare_resources(&self, builder: &mut PassResourceBuilder);

    /// Returns `false` if this pass can be culled this frame (no output consumers).
    fn is_active(&self, frame_ctx: &FrameContext) -> bool;

    /// Execute the pass using the provided command encoder.
    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResolvedResources,
        frame_ctx: &FrameContext,
        ctx: &GpuContext,
    ) -> Result<(), RenderError>;
}

/// A resource barrier inserted at a dependency edge between passes.
#[derive(Debug, Clone)]
pub struct ResourceBarrier {
    pub from_pass: PassId,
    pub to_pass: PassId,
    pub resource_id: GraphResourceId,
    pub barrier_type: BarrierType,
}

/// Type of resource barrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarrierType {
    TextureReadToWrite,
    TextureWriteToRead,
    BufferReadToWrite,
    BufferWriteToRead,
}

/// Compiled graph — the result of graph compilation.
pub struct CompiledGraph {
    /// Passes in execution order (dependencies first).
    pub execution_order: Vec<PassId>,
    /// Passes that were culled (not executed).
    pub culled_passes: Vec<PassId>,
    /// Resource barriers to insert between passes.
    pub resource_barriers: Vec<ResourceBarrier>,
}

/// The render graph — a DAG of render passes.
pub struct RenderGraph {
    /// Registered passes, keyed by PassId.
    passes: IndexMap<PassId, Box<dyn RenderPass>>,
    /// Resource pool for GPU resources.
    pool: ResourcePool,
    /// Compiled graph (None until compile() is called).
    topology: Option<CompiledGraph>,
    /// Next pass ID.
    next_pass_id: u32,
}

impl RenderGraph {
    /// Create a new empty render graph.
    pub fn new() -> Self {
        Self {
            passes: IndexMap::new(),
            pool: ResourcePool::new(),
            topology: None,
            next_pass_id: 0,
        }
    }

    /// Register a render pass in the graph.
    ///
    /// # Errors
    /// Returns `RenderError::RenderPassFailed` if a pass with the same ID is already registered.
    pub fn register_pass(&mut self, mut pass: Box<dyn RenderPass>) -> Result<PassId, RenderError> {
        let id = PassId(self.next_pass_id);
        self.next_pass_id += 1;

        if self.passes.contains_key(&id) {
            return Err(RenderError::RenderPassFailed {
                pass: pass.name(),
                cause: "Pass already registered".into(),
                severity: crate::error::ErrorSeverity::Warning,
            });
        }

        self.passes.insert(id, pass);
        Ok(id)
    }

    /// Compile the graph: topological sort, barrier insertion, cull analysis.
    ///
    /// Must be called after all passes are registered and when the set of active
    /// passes changes (e.g., on scene change).
    ///
    /// # Errors
    /// Returns `RenderError::GraphCompilationFailed` if a cycle is detected.
    pub fn compile(&mut self, frame_ctx: &FrameContext) -> Result<(), RenderError> {
        // Determine active passes.
        let active_passes: Vec<PassId> = self
            .passes
            .iter()
            .filter(|(_, pass)| pass.is_active(frame_ctx))
            .map(|(id, _)| *id)
        .collect();

        let all_pass_ids: HashSet<PassId> = self.passes.keys().copied().collect();
        let active_set: HashSet<PassId> = active_passes.iter().copied().collect();
        let culled_passes: Vec<PassId> = all_pass_ids
            .difference(&active_set)
            .copied()
            .collect();

        // Build resource dependency graph for active passes.
        let mut resource_readers: HashMap<GraphResourceId, Vec<PassId>> = HashMap::new();
        let mut resource_writers: HashMap<GraphResourceId, Vec<PassId>> = HashMap::new();

        for pass_id in &active_passes {
            if let Some(pass) = self.passes.get(pass_id) {
                let mut builder = PassResourceBuilder { usage: PassResourceUsage::default() };
                pass.declare_resources(&mut builder);
                let usage = builder.build();

                for res in &usage.textures_read {
                    resource_readers.entry(*res).or_default().push(*pass_id);
                }
                for res in &usage.textures_written {
                    resource_writers.entry(*res).or_default().push(*pass_id);
                }
                for res in &usage.buffers_read {
                    resource_readers.entry(*res).or_default().push(*pass_id);
                }
                for res in &usage.buffers_written {
                    resource_writers.entry(*res).or_default().push(*pass_id);
                }
            }
        }

        // Topological sort using Kahn's algorithm.
        let execution_order = topological_sort(&active_passes, &resource_readers, &resource_writers)?;

        // Build resource barriers.
        let mut resource_barriers = Vec::new();
        for (res_id, writers) in &resource_writers {
            for writer in writers {
                if let Some(readers) = resource_readers.get(res_id) {
                    for reader in readers {
                        if execution_order.iter().position(|p| p == writer).unwrap()
                            < execution_order.iter().position(|p| p == reader).unwrap()
                        {
                            // Writer before reader: insert barrier.
                            resource_barriers.push(ResourceBarrier {
                                from_pass: *writer,
                                to_pass: *reader,
                                resource_id: *res_id,
                                barrier_type: BarrierType::TextureWriteToRead,
                            });
                        }
                    }
                }
            }
        }

        self.topology = Some(CompiledGraph {
            execution_order,
            culled_passes,
            resource_barriers,
        });

        Ok(())
    }

    /// Execute all active passes in compiled order.
    ///
    /// The caller is responsible for creating the command encoder and providing
    /// resolved resources (bind groups, textures, buffers) via `resources`.
    ///
    /// # Errors
    /// Returns `RenderError::RenderPassFailed` if any pass fails.
    /// Returns `RenderError::GraphCompilationFailed` if the graph has not been compiled.
    pub fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        resources: &ResolvedResources,
        frame_ctx: &FrameContext,
        ctx: &GpuContext,
    ) -> Result<(), RenderError> {
        let compiled = self
            .topology
            .as_ref()
            .ok_or(RenderError::GraphCompilationFailed {
                cycle_detected: false,
                severity: crate::error::ErrorSeverity::Fatal,
            })?;

        for pass_id in &compiled.execution_order {
            if let Some(pass) = self.passes.get(pass_id) {
                pass.execute(encoder, resources, frame_ctx, ctx)?;
            }
        }

        Ok(())
    }

    /// Get the compiled graph topology (for diagnostics).
    pub fn topology(&self) -> Option<&CompiledGraph> {
        self.topology.as_ref()
    }

    /// Get the resource pool.
    pub fn resource_pool(&self) -> &ResourcePool {
        &self.pool
    }

    /// Get the resource pool (mutable).
    pub fn resource_pool_mut(&mut self) -> &mut ResourcePool {
        &mut self.pool
    }

    /// Number of registered passes.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Number of active passes in the last compilation.
    pub fn active_pass_count(&self) -> usize {
        self.topology
            .as_ref()
            .map(|c| c.execution_order.len())
            .unwrap_or(0)
    }

    /// Number of culled passes in the last compilation.
    pub fn culled_pass_count(&self) -> usize {
        self.topology
            .as_ref()
            .map(|c| c.culled_passes.len())
            .unwrap_or(0)
    }
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Topological sort using Kahn's algorithm.
///
/// Returns passes in dependency order (dependencies before dependents).
/// Detects cycles and returns an error if one is found.
fn topological_sort(
    passes: &[PassId],
    readers: &HashMap<GraphResourceId, Vec<PassId>>,
    writers: &HashMap<GraphResourceId, Vec<PassId>>,
) -> Result<Vec<PassId>, RenderError> {
    // Build in-degree map and adjacency list.
    let mut in_degree: HashMap<PassId, usize> = HashMap::new();
    let mut adjacency: HashMap<PassId, Vec<PassId>> = HashMap::new();

    for pass in passes {
        in_degree.entry(*pass).or_insert(0);
        adjacency.entry(*pass).or_default();
    }

    // For each resource, if pass A writes and pass B reads, add edge A -> B.
    for (res_id, writers_list) in writers {
        for writer in writers_list {
            if let Some(readers_list) = readers.get(res_id) {
                for reader in readers_list {
                    if writer != reader && passes.contains(writer) && passes.contains(reader) {
                        adjacency.entry(*writer).or_default().push(*reader);
                        *in_degree.entry(*reader).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // Kahn's algorithm: start with nodes that have in-degree 0.
    let mut queue: Vec<PassId> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(pass, _)| *pass)
        .collect();

    let mut sorted = Vec::new();

    while let Some(pass) = queue.pop() {
        sorted.push(pass);
        if let Some(neighbors) = adjacency.get(&pass) {
            for neighbor in neighbors {
                if let Some(degree) = in_degree.get_mut(neighbor) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        queue.push(*neighbor);
                    }
                }
            }
        }
    }

    if sorted.len() != passes.len() {
        return Err(RenderError::GraphCompilationFailed {
            cycle_detected: true,
            severity: crate::error::ErrorSeverity::Fatal,
        });
    }

    Ok(sorted)
}

impl std::fmt::Debug for RenderGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderGraph")
            .field("passes", &self.passes.len())
            .field("active", &self.active_pass_count())
            .field("culled", &self.culled_pass_count())
            .field("compiled", &self.topology.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPass {
        id: PassId,
        name: &'static str,
        active: bool,
    }

    impl RenderPass for TestPass {
        fn id(&self) -> PassId {
            self.id
        }
        fn name(&self) -> &'static str {
            self.name
        }
        fn declare_resources(&self, _builder: &mut PassResourceBuilder) {}
        fn is_active(&self, _frame_ctx: &FrameContext) -> bool {
            self.active
        }
        fn execute(
            &self,
            _encoder: &mut wgpu::CommandEncoder,
            _resources: &ResolvedResources,
            _frame_ctx: &FrameContext,
            _ctx: &GpuContext,
        ) -> Result<(), RenderError> {
            Ok(())
        }
    }

    #[test]
    fn test_graph_creation() {
        let graph = RenderGraph::new();
        assert_eq!(graph.pass_count(), 0);
        assert_eq!(graph.active_pass_count(), 0);
    }

    #[test]
    fn test_register_pass() {
        let mut graph = RenderGraph::new();
        let pass = Box::new(TestPass {
            id: PassId(0),
            name: "test",
            active: true,
        });
        let id = graph.register_pass(pass).unwrap();
        assert_eq!(graph.pass_count(), 1);
    }

    #[test]
    fn test_compile_culls_inactive_passes() {
        let mut graph = RenderGraph::new();
        let active_pass = Box::new(TestPass {
            id: PassId(0),
            name: "active",
            active: true,
        });
        let inactive_pass = Box::new(TestPass {
            id: PassId(1),
            name: "inactive",
            active: false,
        });
        graph.register_pass(active_pass).unwrap();
        graph.register_pass(inactive_pass).unwrap();

        let frame_ctx = FrameContext {
            frame_index: 0,
            delta_time: 0.016,
            total_time: 0.0,
            surface_width: 1920,
            surface_height: 1080,
            focus_mode: false,
            sleeping: false,
            active_particles: 0,
            active_panels: 0,
            fur_shell_count: 24,
            lod_level: 0,
            bloom_has_content: false,
        };

        graph.compile(&frame_ctx).unwrap();
        assert_eq!(graph.active_pass_count(), 1);
        assert_eq!(graph.culled_pass_count(), 1);
    }

    #[test]
    fn test_topological_sort_empty() {
        let result = topological_sort(&[], &HashMap::new(), &HashMap::new());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_topological_sort_no_deps() {
        let passes = vec![PassId(0), PassId(1), PassId(2)];
        let result = topological_sort(&passes, &HashMap::new(), &HashMap::new()).unwrap();
        // With no deps, order is arbitrary but all passes present.
        assert_eq!(result.len(), 3);
    }
}
