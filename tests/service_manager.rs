//! Integration tests for ServiceManager.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use lumas_runtime::context::RuntimeContext;
use lumas_runtime::event::EventBus;
use lumas_runtime::resource::ResourceManager;
use lumas_runtime::service::*;
use lumas_runtime::version::FeatureFlags;

struct TestService {
    name: &'static str,
    deps: Vec<&'static str>,
    version: semver::Version,
    start_flag: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl Service for TestService {
    fn name(&self) -> &'static str { self.name }
    fn version(&self) -> &semver::Version { &self.version }
    fn dependencies(&self) -> &[&'static str] { &self.deps }
    async fn start(&self, _ctx: Arc<RuntimeContext>) -> Result<(), ServiceError> {
        self.start_flag.store(true, Ordering::Relaxed);
        Ok(())
    }
    async fn stop(&self) -> Result<(), ServiceError> {
        self.stop_flag.store(true, Ordering::Relaxed);
        Ok(())
    }
    async fn health_check(&self) -> ServiceHealth {
        ServiceHealth::healthy("ok")
    }
    fn metrics(&self) -> Vec<ServiceMetric> { vec![] }
}

#[tokio::test]
async fn test_services_start_in_dependency_order() {
    let mut mgr = ServiceManager::new();
    let flag1 = Arc::new(AtomicBool::new(false));
    let flag2 = Arc::new(AtomicBool::new(false));

    let dep = Arc::new(TestService {
        name: "dependency",
        deps: vec![],
        version: semver::Version::new(1, 0, 0),
        start_flag: flag1.clone(),
        stop_flag: Arc::new(AtomicBool::new(false)),
    });
    let main = Arc::new(TestService {
        name: "main",
        deps: vec!["dependency"],
        version: semver::Version::new(1, 0, 0),
        start_flag: flag2.clone(),
        stop_flag: Arc::new(AtomicBool::new(false)),
    });

    mgr.register(dep, 3, false).unwrap();
    mgr.register(main, 3, false).unwrap();

    let ctx = Arc::new(RuntimeContext::new(
        Arc::new(FeatureFlags::new()),
        Arc::new(EventBus::new(16)),
        Arc::new(ResourceManager::new()),
    ));

    assert!(mgr.start_all(ctx).await.is_ok());
    assert!(flag1.load(Ordering::Relaxed));
    assert!(flag2.load(Ordering::Relaxed));
}

#[tokio::test]
async fn test_cycle_detection_returns_error_with_cycle_path() {
    let mut mgr = ServiceManager::new();

    let a = Arc::new(TestService {
        name: "a",
        deps: vec!["b"],
        version: semver::Version::new(1, 0, 0),
        start_flag: Arc::new(AtomicBool::new(false)),
        stop_flag: Arc::new(AtomicBool::new(false)),
    });
    let b = Arc::new(TestService {
        name: "b",
        deps: vec!["a"],
        version: semver::Version::new(1, 0, 0),
        start_flag: Arc::new(AtomicBool::new(false)),
        stop_flag: Arc::new(AtomicBool::new(false)),
    });

    mgr.register(a, 3, false).unwrap();
    mgr.register(b, 3, false).unwrap();

    let result = mgr.resolve_startup_order();
    assert!(result.is_err());
    match result {
        Err(ServiceError::DependencyCycle { cycle }) => {
            assert!(!cycle.is_empty());
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[tokio::test]
async fn test_services_stop_in_reverse_dependency_order() {
    let mut mgr = ServiceManager::new();
    let stop_a = Arc::new(AtomicBool::new(false));
    let stop_b = Arc::new(AtomicBool::new(false));

    let svc_a = Arc::new(TestService {
        name: "a",
        deps: vec![],
        version: semver::Version::new(1, 0, 0),
        start_flag: Arc::new(AtomicBool::new(false)),
        stop_flag: stop_a.clone(),
    });
    let svc_b = Arc::new(TestService {
        name: "b",
        deps: vec!["a"],
        version: semver::Version::new(1, 0, 0),
        start_flag: Arc::new(AtomicBool::new(false)),
        stop_flag: stop_b.clone(),
    });

    mgr.register(svc_a, 3, false).unwrap();
    mgr.register(svc_b, 3, false).unwrap();

    let ctx = Arc::new(RuntimeContext::new(
        Arc::new(FeatureFlags::new()),
        Arc::new(EventBus::new(16)),
        Arc::new(ResourceManager::new()),
    ));

    mgr.start_all(ctx).await.unwrap();
    mgr.stop_all().await.unwrap();

    assert!(stop_a.load(Ordering::Relaxed));
    assert!(stop_b.load(Ordering::Relaxed));
}
