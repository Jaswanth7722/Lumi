//! Integration tests for error code uniqueness and registry completeness.

use lumi_error::error_code::{ERROR_CODE_REGISTRY, ErrorCode, lookup_error_code};
use std::collections::HashSet;

#[tokio::test]
async fn test_error_code_uniqueness() {
    let mut seen = HashSet::new();
    for (code, _entry) in ERROR_CODE_REGISTRY.iter() {
        assert!(seen.insert(code), "Duplicate error code: {}", code);
    }
}

#[tokio::test]
async fn test_error_code_format() {
    let code = ErrorCode::new(0x0401);
    let formatted = code.format(&lumi_error::ErrorCategory::AiCore { provider: None });
    assert_eq!(formatted, "LUMI-AI-0401");
}

#[tokio::test]
async fn test_error_code_from_u32() {
    let code: ErrorCode = 0x0101.into();
    assert_eq!(code.value(), 0x0101);
}

#[tokio::test]
async fn test_no_duplicate_runtime_codes() {
    let codes = [
        ErrorCode::RUNTIME_BOOTSTRAP_FAILED.value(),
        ErrorCode::RUNTIME_SHUTDOWN_FAILED.value(),
        ErrorCode::RUNTIME_INTERNAL.value(),
        ErrorCode::RUNTIME_SERVICE_FAILED.value(),
        ErrorCode::RUNTIME_RESOURCE_EXHAUSTED.value(),
        ErrorCode::RUNTIME_LIFECYCLE_INVALID.value(),
    ];

    let mut seen = HashSet::new();
    for code in codes {
        assert!(seen.insert(code), "Duplicate runtime code: {}", code);
    }
}

#[tokio::test]
async fn test_no_duplicate_ai_codes() {
    let codes = [
        ErrorCode::AI_INFERENCE_FAILED.value(),
        ErrorCode::AI_PROVIDER_UNREACHABLE.value(),
        ErrorCode::AI_MODEL_NOT_FOUND.value(),
        ErrorCode::AI_CONTEXT_OVERFLOW.value(),
        ErrorCode::AI_RATE_LIMITED.value(),
    ];

    let mut seen = HashSet::new();
    for code in codes {
        assert!(seen.insert(code), "Duplicate AI code: {}", code);
    }
}

#[tokio::test]
async fn test_lookup_existing_code() {
    let entry = lookup_error_code(ErrorCode::AI_INFERENCE_FAILED.value());
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().name, "ai_inference_failed");
}

#[tokio::test]
async fn test_lookup_nonexistent_code() {
    let entry = lookup_error_code(0x9999);
    assert!(entry.is_none());
}

#[tokio::test]
async fn test_all_error_codes_have_entries() {
    // Verify that all defined error code constants have corresponding
    // entries in the registry
    let codes = [
        ErrorCode::RUNTIME_BOOTSTRAP_FAILED,
        ErrorCode::RUNTIME_SHUTDOWN_FAILED,
        ErrorCode::CONFIG_FILE_NOT_FOUND,
        ErrorCode::IPC_CONNECTION_FAILED,
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCode::VOICE_STT_FAILED,
        ErrorCode::STORAGE_WRITE_FAILED,
        ErrorCode::SECURITY_ACCESS_DENIED,
        ErrorCode::INTERNAL_UNEXPECTED,
    ];

    for code in codes {
        assert!(
            lookup_error_code(code.value()).is_some(),
            "Missing registry entry for code: {}-{:04}",
            code,
            code.value()
        );
    }
}

#[tokio::test]
async fn test_error_code_display() {
    let code = ErrorCode::new(0x0101);
    let display = format!("{}", code);
    assert_eq!(display, "EC0101");
}

#[tokio::test]
async fn test_category_short_codes() {
    use lumi_error::ErrorCategory;
    assert_eq!(ErrorCategory::Runtime.short_code(), "RTE");
    assert_eq!(ErrorCategory::AiCore { provider: None }.short_code(), "AI");
    assert_eq!(ErrorCategory::Internal.short_code(), "INT");
}
