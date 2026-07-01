//! # Capability Registry Integration Tests
//!
//! Tests for capability declaration, exclusivity, and deregistration.

use lumas_process::capability::CapabilityRegistry;
use lumas_process::descriptor::{ProcessDescriptor, ProcessKind};
use lumas_process::error::ProcessError;
use lumas_process::id::ProcessId;
use std::sync::Arc;

fn make_descriptor(id: &str, capabilities: Vec<String>) -> ProcessDescriptor {
    let pid = ProcessId::new(id);
    let mut desc = ProcessDescriptor::new(
        pid,
        id,
        semver::Version::new(1, 0, 0),
        ProcessKind::Worker {
            worker_fn: Arc::new(|| Box::pin(async {})),
        },
    );
    desc.capabilities = capabilities;
    desc
}

#[test]
fn test_declared_capability_registers_successfully() {
    let reg = CapabilityRegistry::new();
    let id = ProcessId::new("test");
    let desc = make_descriptor("test", vec!["file.read".into(), "network.connect".into()]);

    assert!(reg.register(&id, &desc).is_ok());
    assert!(reg.has_capability(&id, "file.read"));
    assert!(reg.has_capability(&id, "network.connect"));
}

#[test]
fn test_duplicate_exclusive_capability_returns_error() {
    let reg = CapabilityRegistry::new();
    let id1 = ProcessId::new("proc1");
    let id2 = ProcessId::new("proc2");

    let desc1 = make_descriptor("proc1", vec!["screen.capture".into()]);
    let desc2 = make_descriptor("proc2", vec!["screen.capture".into()]);

    assert!(reg.register(&id1, &desc1).is_ok());
    let result = reg.register(&id2, &desc2);
    assert!(result.is_err());
    match result {
        Err(ProcessError::DuplicateCapability { capability, .. }) => {
            assert_eq!(capability, "screen.capture");
        }
        _ => panic!("Expected DuplicateCapability error"),
    }
}

#[test]
fn test_deregister_releases_exclusive_capability() {
    let reg = CapabilityRegistry::new();
    let id = ProcessId::new("test");
    let desc = make_descriptor("test", vec!["screen.capture".into()]);

    reg.register(&id, &desc).unwrap();
    assert!(reg.exclusive_owner("screen.capture").is_some());

    reg.deregister(&id);
    assert!(reg.exclusive_owner("screen.capture").is_none());
}

#[test]
fn test_non_exclusive_capability_allows_multiple_owners() {
    let reg = CapabilityRegistry::new();
    let id1 = ProcessId::new("proc1");
    let id2 = ProcessId::new("proc2");

    let desc1 = make_descriptor("proc1", vec!["file.read".into()]);
    let desc2 = make_descriptor("proc2", vec!["file.read".into()]);

    assert!(reg.register(&id1, &desc1).is_ok());
    assert!(reg.register(&id2, &desc2).is_ok());

    assert!(reg.has_capability(&id1, "file.read"));
    assert!(reg.has_capability(&id2, "file.read"));
}
