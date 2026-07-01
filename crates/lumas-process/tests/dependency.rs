//! # Dependency Graph Integration Tests
//!
//! Tests for cycle detection, topological sort, and dependency validation.

use lumas_process::dependency::{DependencyEdge, DependencyGraph};
use lumas_process::error::ProcessError;
use lumas_process::id::ProcessId;
use std::collections::HashMap;

fn make_id(name: &str) -> ProcessId {
    ProcessId::new(name)
}

#[test]
fn test_startup_order_respects_dependencies() {
    let mut graph = DependencyGraph::new();
    let a = make_id("a");
    let b = make_id("b");
    let c = make_id("c");

    graph.add_process(a.clone()).unwrap();
    graph.add_process(b.clone()).unwrap();
    graph.add_process(c.clone()).unwrap();

    // a depends on b, b depends on c
    graph
        .add_dependency(
            a.clone(),
            b.clone(),
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            },
        )
        .unwrap();

    graph
        .add_dependency(
            b.clone(),
            c.clone(),
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            },
        )
        .unwrap();

    let order = graph.startup_order().unwrap();
    let pos_c = order.iter().position(|id| id.path() == "c").unwrap();
    let pos_b = order.iter().position(|id| id.path() == "b").unwrap();
    let pos_a = order.iter().position(|id| id.path() == "a").unwrap();

    // c before b before a
    assert!(pos_c < pos_b);
    assert!(pos_b < pos_a);
}

#[test]
fn test_cycle_detection_returns_full_cycle_path() {
    let mut graph = DependencyGraph::new();
    let a = make_id("a");
    let b = make_id("b");
    let c = make_id("c");

    graph.add_process(a.clone()).unwrap();
    graph.add_process(b.clone()).unwrap();
    graph.add_process(c.clone()).unwrap();

    graph
        .add_dependency(
            a.clone(),
            b.clone(),
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            },
        )
        .unwrap();

    graph
        .add_dependency(
            b.clone(),
            c.clone(),
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            },
        )
        .unwrap();

    let result = graph.add_dependency(
        c.clone(),
        a.clone(),
        DependencyEdge {
            version_req: semver::VersionReq::STAR,
            required: true,
            startup_order: true,
        },
    );

    assert!(result.is_err());
    match result {
        Err(ProcessError::DependencyCycle { cycle }) => {
            assert!(cycle.len() >= 3, "Cycle path should include at least a,b,c");
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn test_missing_required_dependency_returns_error() {
    let mut graph = DependencyGraph::new();
    let a = make_id("a");
    // b is not added to the graph

    graph.add_process(a.clone()).unwrap();
    let result = graph.add_dependency(
        a.clone(),
        make_id("b"), // Not registered
        DependencyEdge {
            version_req: semver::VersionReq::STAR,
            required: true,
            startup_order: true,
        },
    );

    assert!(result.is_err());
    match result {
        Err(ProcessError::NotFound { .. }) => {} // Expected
        _ => panic!("Expected NotFound error"),
    }
}

#[test]
fn test_shutdown_order_is_reverse_of_startup() {
    let mut graph = DependencyGraph::new();
    let a = make_id("a");
    let b = make_id("b");

    graph.add_process(a.clone()).unwrap();
    graph.add_process(b.clone()).unwrap();

    graph
        .add_dependency(
            a.clone(),
            b.clone(),
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            },
        )
        .unwrap();

    let startup = graph.startup_order().unwrap();
    let shutdown = graph.shutdown_order().unwrap();

    assert_eq!(startup.len(), shutdown.len());
    let mut reversed = startup.clone();
    reversed.reverse();
    assert_eq!(shutdown, reversed);
}

#[test]
fn test_transitive_dependents_identified_correctly() {
    let mut graph = DependencyGraph::new();
    let a = make_id("a");
    let b = make_id("b");
    let c = make_id("c");

    graph.add_process(a.clone()).unwrap();
    graph.add_process(b.clone()).unwrap();
    graph.add_process(c.clone()).unwrap();

    // a -> b -> c (a depends on b, b depends on c)
    graph
        .add_dependency(
            a.clone(),
            b.clone(),
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            },
        )
        .unwrap();

    graph
        .add_dependency(
            b.clone(),
            c.clone(),
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            },
        )
        .unwrap();

    // c's dependents should include both b and a
    let deps = graph.transitive_dependents_of(&c);
    let paths: Vec<&str> = deps.iter().map(|id| id.path()).collect();
    assert!(paths.contains(&"a"));
    assert!(paths.contains(&"b"));
    assert_eq!(paths.len(), 2);
}

#[test]
fn test_mermaid_export_is_valid_syntax() {
    let mut graph = DependencyGraph::new();
    let a = make_id("lumi.render");
    let b = make_id("lumi.core");

    graph.add_process(a.clone()).unwrap();
    graph.add_process(b.clone()).unwrap();

    graph
        .add_dependency(
            a.clone(),
            b.clone(),
            DependencyEdge {
                version_req: semver::VersionReq::STAR,
                required: true,
                startup_order: true,
            },
        )
        .unwrap();

    let mermaid = graph.to_mermaid();
    assert!(mermaid.starts_with("graph TD;"));
}
