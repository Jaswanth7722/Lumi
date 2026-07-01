//! Integration tests for crash reports, atomic writes, and index rotation.

use lumas_error::crash::*;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn test_crash_report_creation() {
    let report = CrashReport::new(CrashType::FatalError);
    assert_eq!(format!("{}", report.crash_type), "fatal_error");
    assert!(report.error.is_none());
}

#[tokio::test]
async fn test_crash_report_atomic_write() {
    let dir = tempdir().unwrap();
    let report = CrashReport::new(CrashType::Panic {
        message: "test panic".into(),
    });
    let path = report.write_to_dir(dir.path()).unwrap();
    assert!(path.exists());

    // Verify it's valid JSON
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["crash_type"], "panic");
}

#[tokio::test]
async fn test_crash_report_with_error() {
    let dir = tempdir().unwrap();
    let inner_err = lumas_error::LumasError::new(
        lumas_error::ErrorCode::AI_INFERENCE_FAILED,
        lumas_error::ErrorCategory::AiCore { provider: None },
        "inference failed",
    );

    let report = CrashReport::new(CrashType::FatalError).with_error(inner_err);
    let path = report.write_to_dir(dir.path()).unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(parsed.get("error").is_some());
}

#[tokio::test]
async fn test_crash_report_index_updated() {
    let dir = tempdir().unwrap();

    // Write multiple crash reports
    for i in 0..3 {
        let report = CrashReport::new(CrashType::FatalError);
        report.write_to_dir(dir.path()).unwrap();
    }

    // Check index exists and has entries
    let index_path = dir.path().join("crash_index.json");
    assert!(index_path.exists());

    let content = std::fs::read_to_string(&index_path).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed.len(), 3);
}

#[tokio::test]
async fn test_crash_report_fallback_write() {
    let id = Uuid::new_v4();
    let path = CrashReport::write_fallback(id, "emergency").unwrap();
    assert!(path.exists());
    assert!(path.to_string_lossy().contains("fallback"));

    // Verify it's valid JSON
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["panic_message"], "emergency");
    assert_eq!(parsed["fallback"], true);

    // Cleanup
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn test_multiple_crash_types() {
    let types = vec![
        CrashType::FatalError,
        CrashType::Panic {
            message: "test".into(),
        },
        CrashType::OomKill,
        CrashType::SignalReceived { signal: 11 },
        CrashType::Watchdog {
            component: "ai-core".into(),
            timeout_secs: 30,
        },
    ];

    for crash_type in types {
        let report = CrashReport::new(crash_type);
        let _ = format!("{}", report.id); // Just verify it doesn't panic
    }
}
