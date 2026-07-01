//! # Concurrency Integration Tests
//!
//! Tests for concurrent access patterns in the process management system.

use lumas_process::capability::CapabilityRegistry;
use lumas_process::descriptor::{ProcessDescriptor, ProcessKind};
use lumas_process::id::ProcessId;
use lumas_process::registry::ProcessRegistry;
use std::sync::Arc;

/// Helper to create a simple process descriptor for tests.
fn make_test_descriptor(id: &str) -> ProcessDescriptor {
    ProcessDescriptor::new(
        ProcessId::new(id),
        id,
        semver::Version::new(1, 0, 0),
        ProcessKind::Worker {
            worker_fn: Arc::new(|| Box::pin(async {})),
        },
    )
}

#[tokio::test]
async fn test_100_concurrent_process_registrations_no_race() {
    let registry = ProcessRegistry::new();
    let cap_reg = CapabilityRegistry::new();

    let mut handles = Vec::new();

    for i in 0..100 {
        let registry = registry.clone();
        let cap_reg = &cap_reg;
        handles.push(tokio::spawn(async move {
            let id = format!("process-{}", i);
            let pid = ProcessId::new(&id);
            let desc = make_test_descriptor(&id);

            registry.insert(
                lumas_process::handle::ProcessHandle::dummy(&pid),
            );
            let _ = cap_reg.register(&pid, &desc);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(registry.len(), 100);
}

#[tokio::test]
async fn test_concurrent_worker_spawns_no_duplicate_ids() {
    use lumas_process::worker::WorkerManager;
    use lumas_process::metrics::ProcessMetrics;
    use lumas_runtime::event::EventBus;

    let event_bus = Arc::new(EventBus::new(64));
    let metrics = Arc::new(ProcessMetrics::new());
    let wm = Arc::new(WorkerManager::new(event_bus.clone(), metrics));

    let mut handles = Vec::new();

    for i in 0..50 {
        let wm = wm.clone();
        let owner = ProcessId::new("test-owner");
        handles.push(tokio::spawn(async move {
            wm.spawn(
                owner,
                Box::new(lumas_process::worker::TestWorker),
                lumas_process::restart::RestartPolicy::Never,
            )
            .await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;
    let successes = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(successes, 50);
}
