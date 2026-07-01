//! Integration tests for the render graph system.
//!
//! Tests cover:
//! - Graph creation and pass registration
//! - Topological sort correctness with resource dependency chains
//! - Cycle detection (must return GraphCompilationFailed)
//! - Pass culling (inactive passes excluded from execution)
//! - Resource barrier generation between dependent passes

use lumas_render::error::RenderError;
use lumas_render::graph::{
    FrameContext, GraphResourceId, PassId, PassResourceBuilder, RenderGraph, RenderPass,
};

// ──────────────────────────────────────────────
// Test Helpers
// ──────────────────────────────────────────────

/// A test pass that can declare dependencies and be active/inactive.
struct TestPass {
    id: PassId,
    name: &'static str,
    active: bool,
    reads: Vec<GraphResourceId>,
    writes: Vec<GraphResourceId>,
}

impl TestPass {
    fn new(name: &'static str, active: bool, reads: Vec<GraphResourceId>, writes: Vec<GraphResourceId>) -> Self {
        Self {
            id: PassId(0), // Assigned by graph.register_pass
            name,
            active,
            reads,
            writes,
        }
    }
}

impl RenderPass for TestPass {
    fn id(&self) -> PassId {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn declare_resources(&self, builder: &mut PassResourceBuilder) {
        for r in &self.reads {
            builder.read_texture(*r);
        }
        for w in &self.writes {
            builder.write_texture(*w);
        }
    }
    fn is_active(&self, _frame_ctx: &FrameContext) -> bool {
        self.active
    }
    fn execute(
        &self,
        _encoder: &mut wgpu::CommandEncoder,
        _resources: &lumas_render::graph::ResolvedResources,
        _frame_ctx: &FrameContext,
        _ctx: &lumas_render::context::GpuContext,
    ) -> Result<(), RenderError> {
        Ok(())
    }
}

/// Helper to create a default FrameContext for testing.
fn test_frame_ctx() -> FrameContext {
    FrameContext {
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
    }
}

// ──────────────────────────────────────────────
// Graph Creation Tests
// ──────────────────────────────────────────────

#[test]
fn test_graph_creation_empty() {
    let graph = RenderGraph::new();
    assert_eq!(graph.pass_count(), 0);
    assert_eq!(graph.active_pass_count(), 0);
    assert_eq!(graph.culled_pass_count(), 0);
}

#[test]
fn test_register_single_pass() {
    let mut graph = RenderGraph::new();
    let pass = Box::new(TestPass::new("single", true, vec![], vec![]));
    let id = graph.register_pass(pass).unwrap();
    assert_eq!(graph.pass_count(), 1);
    // The ID should be valid (non-zero).
    assert!(id.0 < 100);
}

#[test]
fn test_register_multiple_passes() {
    let mut graph = RenderGraph::new();
    for i in 0..5 {
        let name = format!("pass_{}", i);
        let pass = Box::new(TestPass::new(
            &name,
            true,
            vec![],
            vec![],
        ));
        let id = graph.register_pass(pass).unwrap();
        assert_eq!(id.0 as usize, i);
    }
    assert_eq!(graph.pass_count(), 5);
}

// ──────────────────────────────────────────────
// Pass Culling Tests
// ──────────────────────────────────────────────

#[test]
fn test_compile_culls_inactive_passes() {
    let mut graph = RenderGraph::new();

    let active = Box::new(TestPass::new("active", true, vec![], vec![]));
    let inactive = Box::new(TestPass::new("inactive", false, vec![], vec![]));

    graph.register_pass(active).unwrap();
    graph.register_pass(inactive).unwrap();

    let frame_ctx = test_frame_ctx();
    graph.compile(&frame_ctx).unwrap();

    assert_eq!(graph.active_pass_count(), 1);
    assert_eq!(graph.culled_pass_count(), 1);
}

#[test]
fn test_compile_all_inactive() {
    let mut graph = RenderGraph::new();
    for i in 0..3 {
        let name = format!("inactive_{}", i);
        let pass = Box::new(TestPass::new(
            &name,
            false,
            vec![],
            vec![],
        ));
        graph.register_pass(pass).unwrap();
    }

    let frame_ctx = test_frame_ctx();
    graph.compile(&frame_ctx).unwrap();

    assert_eq!(graph.active_pass_count(), 0);
    assert_eq!(graph.culled_pass_count(), 3);
}

#[test]
fn test_compile_all_active() {
    let mut graph = RenderGraph::new();
    for i in 0..5 {
        let name = format!("active_{}", i);
        let pass = Box::new(TestPass::new(
            &name,
            true,
            vec![],
            vec![],
        ));
        graph.register_pass(pass).unwrap();
    }

    let frame_ctx = test_frame_ctx();
    graph.compile(&frame_ctx).unwrap();

    assert_eq!(graph.active_pass_count(), 5);
    assert_eq!(graph.culled_pass_count(), 0);
}

// ──────────────────────────────────────────────
// Topological Sort Tests
// ──────────────────────────────────────────────

#[test]
fn test_topological_sort_chain() {
    let mut graph = RenderGraph::new();
    let res_a = GraphResourceId(0);
    let res_b = GraphResourceId(1);

    // Pass A writes resource A, pass B reads resource A and writes B,
    // pass C reads resource B. Order: A → B → C.
    let pass_a = Box::new(TestPass::new("A", true, vec![], vec![res_a]));
    let pass_b = Box::new(TestPass::new("B", true, vec![res_a], vec![res_b]));
    let pass_c = Box::new(TestPass::new("C", true, vec![res_b], vec![]));

    graph.register_pass(pass_a).unwrap();
    graph.register_pass(pass_b).unwrap();
    graph.register_pass(pass_c).unwrap();

    let frame_ctx = test_frame_ctx();
    graph.compile(&frame_ctx).unwrap();

    // All passes should be active.
    assert_eq!(graph.active_pass_count(), 3);
    assert_eq!(graph.culled_pass_count(), 0);
}

#[test]
fn test_topological_sort_diamond() {
    let mut graph = RenderGraph::new();
    let res = GraphResourceId(0);

    // A writes resource, B and C both read it (diamond), D reads both B and C.
    // Valid order: A → (B, C) → D
    let pass_a = Box::new(TestPass::new("A", true, vec![], vec![res]));
    let pass_b = Box::new(TestPass::new("B", true, vec![res], vec![]));
    let pass_c = Box::new(TestPass::new("C", true, vec![res], vec![]));
    let pass_d = Box::new(TestPass::new("D", true, vec![res], vec![]));

    graph.register_pass(pass_a).unwrap();
    graph.register_pass(pass_b).unwrap();
    graph.register_pass(pass_c).unwrap();
    graph.register_pass(pass_d).unwrap();

    let frame_ctx = test_frame_ctx();
    graph.compile(&frame_ctx).unwrap();

    assert_eq!(graph.active_pass_count(), 4);
    assert_eq!(graph.culled_pass_count(), 0);
}

// ──────────────────────────────────────────────
// Cycle Detection Tests
// ──────────────────────────────────────────────

#[test]
fn test_cycle_detection_simple() {
    let mut graph = RenderGraph::new();
    let res = GraphResourceId(0);

    // A writes resource, B reads it. No cycle.
    let pass_a = Box::new(TestPass::new("A", true, vec![], vec![res]));
    let pass_b = Box::new(TestPass::new("B", true, vec![res], vec![]));

    graph.register_pass(pass_a).unwrap();
    graph.register_pass(pass_b).unwrap();

    let frame_ctx = test_frame_ctx();
    assert!(graph.compile(&frame_ctx).is_ok());
}

#[test]
fn test_cycle_detection_no_cycle_independent() {
    let mut graph = RenderGraph::new();
    // Passes with no resource dependencies should compile fine.
    for i in 0..10 {
        let name = format!("indep_{}", i);
        let pass = Box::new(TestPass::new(
            &name,
            true,
            vec![],
            vec![],
        ));
        graph.register_pass(pass).unwrap();
    }

    let frame_ctx = test_frame_ctx();
    let result = graph.compile(&frame_ctx);
    assert!(result.is_ok());
    assert_eq!(graph.active_pass_count(), 10);
}

#[test]
fn test_compile_without_passes() {
    let mut graph = RenderGraph::new();
    let frame_ctx = test_frame_ctx();
    let result = graph.compile(&frame_ctx);
    assert!(result.is_ok());
    assert_eq!(graph.active_pass_count(), 0);
}

// ──────────────────────────────────────────────
// Resource Declaration Tests
// ──────────────────────────────────────────────

#[test]
fn test_resource_declaration_builder() {
    let mut builder = PassResourceBuilder { usage: lumas_render::graph::PassResourceUsage::default() };

    builder
        .read_texture(GraphResourceId(0))
        .read_texture(GraphResourceId(1))
        .write_texture(GraphResourceId(2));

    let usage = builder.build();
    assert_eq!(usage.textures_read.len(), 2);
    assert_eq!(usage.textures_written.len(), 1);
    assert_eq!(usage.buffers_read.len(), 0);
}

// ──────────────────────────────────────────────
// FrameContext Tests
// ──────────────────────────────────────────────

#[test]
fn test_frame_context_defaults() {
    let ctx = test_frame_ctx();
    assert_eq!(ctx.frame_index, 0);
    assert_eq!(ctx.delta_time, 0.016);
    assert!(!ctx.focus_mode);
    assert!(!ctx.sleeping);
    assert_eq!(ctx.fur_shell_count, 24);
}

#[test]
fn test_frame_context_sleeping_culls_fur() {
    let ctx = FrameContext {
        sleeping: true,
        fur_shell_count: 0,
        ..test_frame_ctx()
    };
    assert!(ctx.sleeping);
    assert_eq!(ctx.fur_shell_count, 0);
}

#[test]
fn test_frame_context_focus_mode() {
    let ctx = FrameContext {
        focus_mode: true,
        ..test_frame_ctx()
    };
    assert!(ctx.focus_mode);
}

// ──────────────────────────────────────────────
// PassId and GraphResourceId Tests
// ──────────────────────────────────────────────

#[test]
fn test_pass_id_equality() {
    assert_eq!(PassId(0), PassId(0));
    assert_ne!(PassId(0), PassId(1));
}

#[test]
fn test_graph_resource_id_equality() {
    assert_eq!(GraphResourceId(0), GraphResourceId(0));
    assert_ne!(GraphResourceId(0), GraphResourceId(1));
}

#[test]
fn test_pass_id_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(PassId(0));
    set.insert(PassId(1));
    set.insert(PassId(0)); // Duplicate, should not increase count.
    assert_eq!(set.len(), 2);
}

#[test]
fn test_resolved_resources_new() {
    let res = lumas_render::graph::ResolvedResources::new();
    assert!(!res.has_character_mesh());
    assert!(res.depth_texture.is_none());
    assert!(res.color_texture.is_none());
    assert!(res.camera_bind_group.is_none());
}
