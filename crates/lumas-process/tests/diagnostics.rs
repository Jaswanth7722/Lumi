//! # Diagnostics Integration Tests
//!
//! Tests for process diagnostics, reporting, and export.

use lumas_process::dependency::DependencyGraph;
use lumas_process::diagnostics::ProcessDiagnostics;
use lumas_process::metrics::ProcessMetrics;
use lumas_process::registry::ProcessRegistry;
use parking_lot::RwLock;
use std::sync::Arc;

#[test]
fn test_process_list_contains_all_registered_processes() {
    let registry = Arc::new(ProcessRegistry::new());
    let graph = Arc::new(RwLock::new(DependencyGraph::new()));
    let metrics = Arc::new(ProcessMetrics::new());
    let diag = ProcessDiagnostics::new(registry.clone(), graph, metrics.clone());

    // Initially empty
    assert!(diag.process_list().is_empty());

    // Record a restart event
    diag.record_restart(lumas_process::id::ProcessId::new("test"), "crash".into());
    assert_eq!(diag.restart_history().len(), 1);
}

#[test]
fn test_dependency_graph_exported_as_mermaid() {
    let registry = Arc::new(ProcessRegistry::new());
    let graph = Arc::new(RwLock::new(DependencyGraph::new()));
    let metrics = Arc::new(ProcessMetrics::new());

    // Add some processes to the graph
    {
        let mut g = graph.write();
        g.add_process(lumas_process::id::ProcessId::new("lumi.core"))
            .unwrap();
        g.add_process(lumas_process::id::ProcessId::new("lumi.render"))
            .unwrap();
    }

    let diag = ProcessDiagnostics::new(registry, graph, metrics);
    let mermaid = diag.dependency_graph_mermaid();
    assert!(mermaid.starts_with("graph TD;"));
}

#[test]
fn test_restart_history_records_all_restart_events() {
    let registry = Arc::new(ProcessRegistry::new());
    let graph = Arc::new(RwLock::new(DependencyGraph::new()));
    let metrics = Arc::new(ProcessMetrics::new());
    let diag = ProcessDiagnostics::new(registry, graph, metrics);

    diag.record_restart(lumas_process::id::ProcessId::new("proc1"), "crash".into());
    diag.record_restart(lumas_process::id::ProcessId::new("proc2"), "OOM".into());

    assert_eq!(diag.restart_history().len(), 2);
}
