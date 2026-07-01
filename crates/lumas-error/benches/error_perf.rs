//! Criterion benchmarks for error performance budgets.
//!
//! Measures:
//! - Happy-path overhead per `?` propagation (< 5 ns)
//! - Error construction (no stack trace) (< 200 ns)
//! - Error construction (with stack trace) (< 10 µs)
//! - Crash report generation (< 50 ms)

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lumas_error::category::ErrorCategory;
use lumas_error::crash::CrashReport;
use lumas_error::error::LumasError;
use lumas_error::error_code::ErrorCode;
use lumas_error::prelude::*;

fn bench_error_construction(c: &mut Criterion) {
    c.bench_function("error_construction_no_trace", |b| {
        b.iter(|| {
            LumasError::new(
                black_box(ErrorCode::AI_INFERENCE_FAILED),
                black_box(ErrorCategory::AiCore { provider: None }),
                black_box("test error"),
            )
        })
    });
}

fn bench_error_with_severity(c: &mut Criterion) {
    c.bench_function("error_with_severity", |b| {
        b.iter(|| {
            LumasError::new(
                black_box(ErrorCode::AI_INFERENCE_FAILED),
                black_box(ErrorCategory::AiCore { provider: None }),
                black_box("test error"),
            )
            .with_severity(black_box(Severity::Critical))
        })
    });
}

fn bench_result_context(c: &mut Criterion) {
    c.bench_function("result_context_propagation", |b| {
        b.iter(|| {
            fn inner() -> LumiResult<i32> {
                Err(LumasError::new(
                    black_box(ErrorCode::AI_INFERENCE_FAILED),
                    black_box(ErrorCategory::AiCore { provider: None }),
                    "inner error",
                ))
            }

            fn outer() -> LumiResult<i32> {
                let val = inner()?;
                Ok(val + 1)
            }

            black_box(outer())
        })
    });
}

fn bench_crash_report_generation(c: &mut Criterion) {
    c.bench_function("crash_report_generation", |b| {
        b.iter(|| {
            let report = CrashReport::new(black_box(lumas_error::crash::CrashType::FatalError));
            black_box(report)
        })
    });
}

fn bench_error_code_lookup(c: &mut Criterion) {
    c.bench_function("error_code_lookup", |b| {
        b.iter(|| {
            let entry = lumas_error::error_code::lookup_error_code(black_box(
                ErrorCode::AI_INFERENCE_FAILED.value(),
            ));
            black_box(entry)
        })
    });
}

fn bench_error_display(c: &mut Criterion) {
    let err = LumasError::new(
        ErrorCode::AI_INFERENCE_FAILED,
        ErrorCategory::AiCore { provider: None },
        "test error message",
    );

    c.bench_function("error_display", |b| {
        b.iter(|| black_box(format!("{}", err)))
    });
}

criterion_group!(
    benches,
    bench_error_construction,
    bench_error_with_severity,
    bench_result_context,
    bench_crash_report_generation,
    bench_error_code_lookup,
    bench_error_display,
);
criterion_main!(benches);
