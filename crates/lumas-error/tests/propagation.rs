//! Integration tests for error propagation, context preservation, and causal chains.

use lumas_error::category::ErrorCategory;
use lumas_error::error::LumasError;
use lumas_error::error_code::ErrorCode;
use lumas_error::prelude::*;

#[tokio::test]
async fn test_propagation_via_question_mark() {
    fn inner() -> LumiResult<i32> {
        Err(LumasError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore {
                provider: Some("anthropic".into()),
            },
            "inner error",
        ))
    }

    fn outer() -> LumiResult<i32> {
        let val = inner()?;
        Ok(val + 1)
    }

    let result = outer();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), ErrorCode::AI_INFERENCE_FAILED);
}

#[tokio::test]
async fn test_context_preservation() {
    fn inner() -> LumiResult<()> {
        Err(LumasError::new(
            ErrorCode::RUNTIME_INTERNAL,
            ErrorCategory::Runtime,
            "original context",
        ))
    }

    let result: LumiResult<()> = inner().context("additional context");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.user_message()
            .to_string()
            .contains("additional context")
    );
}

#[tokio::test]
async fn test_causal_chain_creation() {
    let inner = LumasError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "inner",
    );
    let outer = LumasError::new(
        ErrorCode::RUNTIME_SERVICE_FAILED,
        ErrorCategory::Runtime,
        "outer",
    )
    .with_source(inner);

    assert!(outer.source().is_some());
}

#[tokio::test]
async fn test_causal_chain_depth() {
    // Build a chain of errors
    let mut current = LumasError::new(
        ErrorCode::INTERNAL_UNEXPECTED,
        ErrorCategory::Internal,
        "level 0",
    );

    for i in 1..=5 {
        current = LumasError::new(
            ErrorCode::RUNTIME_SERVICE_FAILED,
            ErrorCategory::Runtime,
            format!("level {}", i),
        )
        .with_source(current);
    }

    // Count the chain depth
    let mut depth = 0;
    let mut source = current.source();
    while source.is_some() {
        depth += 1;
        source = source.and_then(|s| s.source());
    }
    assert_eq!(depth, 5);
}

#[tokio::test]
async fn test_error_preserves_category_metadata() {
    let err = LumasError::new(
        ErrorCode::CONFIG_FILE_NOT_FOUND,
        ErrorCategory::Configuration {
            field: Some("logging.level".into()),
        },
        "config error",
    );

    match err.category() {
        ErrorCategory::Configuration { field } => {
            assert!(field.is_some());
            assert_eq!(field.as_ref().unwrap().as_ref(), "logging.level");
        }
        _ => panic!("Wrong category"),
    }
}
