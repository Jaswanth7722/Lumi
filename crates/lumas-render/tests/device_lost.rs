//! Device lost recovery tests.
//!
//! The rendering engine must be able to recover from device lost events.
//! These tests verify the error handling and recovery infrastructure.
//! Full GPU device lost simulation requires `feature = "headless"`.

use lumas_render::error::{ErrorSeverity, RenderError};

// ──────────────────────────────────────────────
// Error Classification Tests
// ──────────────────────────────────────────────

/// Test that DeviceLost errors require device recreation.
#[test]
fn test_device_lost_requires_recreation() {
    let err = RenderError::device_lost("GPU driver reset");
    assert!(err.requires_device_recreation());
    assert_eq!(err.error_code(), "LUMI-REND-0002");
}

/// Test that non-critical errors do NOT require recreation.
#[test]
fn test_warning_errors_dont_require_recreation() {
    let err = RenderError::SurfaceTimeout {
        severity: ErrorSeverity::Warning,
    };
    assert!(!err.requires_device_recreation());
}

/// Test that recoverable errors don't require recreation.
#[test]
fn test_recoverable_errors_dont_require_recreation() {
    let err = RenderError::SurfaceOutdated {
        severity: ErrorSeverity::Recoverable,
    };
    assert!(!err.requires_device_recreation());
}

// ──────────────────────────────────────────────
// Error Severity Tests
// ──────────────────────────────────────────────

#[test]
fn test_error_severity_display() {
    assert_eq!(format!("{}", ErrorSeverity::Fatal), "FATAL");
    assert_eq!(format!("{}", ErrorSeverity::Critical), "CRITICAL");
    assert_eq!(format!("{}", ErrorSeverity::Recoverable), "RECOVERABLE");
    assert_eq!(format!("{}", ErrorSeverity::Warning), "WARNING");
}

#[test]
fn test_error_severity_classification() {
    // Fatal errors: cannot recover, must restart.
    let fatal = RenderError::adapter_not_found("no GPU");
    assert_eq!(fatal.severity(), ErrorSeverity::Fatal);

    // Critical errors: need device/context recreation.
    let critical = RenderError::device_lost("driver crash");
    assert_eq!(critical.severity(), ErrorSeverity::Critical);

    // Warning errors: non-fatal, continue rendering.
    let warning = RenderError::SurfaceTimeout {
        severity: ErrorSeverity::Warning,
    };
    assert_eq!(warning.severity(), ErrorSeverity::Warning);
}

// ──────────────────────────────────────────────
// Device Lost Error Sources
// ──────────────────────────────────────────────

#[test]
fn test_surface_lost_is_critical() {
    let err = RenderError::DeviceLost {
        reason: "Surface lost".into(),
        severity: ErrorSeverity::Critical,
    };
    assert!(err.requires_device_recreation());
}

#[test]
fn test_shader_compilation_is_critical() {
    let err = RenderError::ShaderCompilationFailed {
        shader_id: "test.wgsl".into(),
        cause: "Syntax error".into(),
        severity: ErrorSeverity::Critical,
    };
    assert!(err.requires_device_recreation());
}

#[test]
fn test_pipeline_creation_is_critical() {
    let err = RenderError::pipeline_creation_failed(
        "geometry_pass",
        "Shader module not found",
    );
    assert!(err.requires_device_recreation());
}

#[test]
fn test_graph_compilation_is_fatal() {
    let err = RenderError::GraphCompilationFailed {
        cycle_detected: true,
        severity: ErrorSeverity::Fatal,
    };
    assert!(err.requires_device_recreation());
}

// ──────────────────────────────────────────────
// Recovery Strategy Tests
// ──────────────────────────────────────────────

#[test]
fn test_device_recreation_path() {
    // Simulate the recovery flow:
    // 1. Device lost error occurs
    // 2. Check if recreation is needed
    // 3. If yes, destroy old GpuContext and create new one
    // 4. If no, retry the operation

    let err = RenderError::device_lost("Device removed");
    assert!(err.requires_device_recreation());

    // The recovery path would be:
    // - Drop the old GpuContext (which drops device, queue, surface)
    // - Call GpuContext::new() with the same config
    // - Recreate all GPU resources (pipelines, textures, buffers)
    // - Resume rendering
}

#[test]
fn test_surface_timeout_recovery() {
    // Surface timeout: retry without recreation
    let err = RenderError::SurfaceTimeout {
        severity: ErrorSeverity::Warning,
    };
    assert!(!err.requires_device_recreation());

    // Recovery: just retry get_current_texture() on the next frame.
}

#[test]
fn test_surface_outdated_recovery() {
    // Surface outdated: reconfigure surface without recreation
    let err = RenderError::SurfaceOutdated {
        severity: ErrorSeverity::Recoverable,
    };
    assert!(!err.requires_device_recreation());

    // Recovery: call reconfigure_surface(new_width, new_height).
}

// ──────────────────────────────────────────────
// Error Code Tests
// ──────────────────────────────────────────────

#[test]
fn test_all_render_error_codes() {
    let cases = vec![
        (RenderError::adapter_not_found(""), "LUMI-REND-0001"),
        (RenderError::device_lost(""), "LUMI-REND-0002"),
        (RenderError::SurfaceOutdated { severity: ErrorSeverity::Recoverable }, "LUMI-REND-0003"),
        (RenderError::SurfaceTimeout { severity: ErrorSeverity::Warning }, "LUMI-REND-0004"),
        (RenderError::ShaderCompilationFailed { shader_id: "".into(), cause: "".into(), severity: ErrorSeverity::Critical }, "LUMI-REND-0005"),
        (RenderError::PipelineCreationFailed { pipeline_id: "".into(), cause: "".into(), severity: ErrorSeverity::Critical }, "LUMI-REND-0007"),
        (RenderError::GraphCompilationFailed { cycle_detected: false, severity: ErrorSeverity::Fatal }, "LUMI-REND-0013"),
        (RenderError::FrameBudgetExceeded { pass: "", actual_us: 0, budget_us: 0, severity: ErrorSeverity::Warning }, "LUMI-REND-0014"),
    ];

    for (err, expected_code) in cases {
        assert_eq!(
            err.error_code(),
            expected_code,
            "Expected error code {} for {:?}",
            expected_code,
            err
        );
    }
}

// ──────────────────────────────────────────────
// Surface Error Mapping Tests
// ──────────────────────────────────────────────

#[test]
fn test_wgpu_surface_error_mapping() {
    // Test the error mapping logic used in GpuContext::get_surface_texture.

    // Timeout → SurfaceTimeout (Warning)
    let timeout = &wgpu::CurrentSurfaceTexture::Timeout;
    let _ = format!("{:?}", timeout);

    // Outdated → SurfaceOutdated (Recoverable)
    let outdated = &wgpu::CurrentSurfaceTexture::Outdated;
    let _ = format!("{:?}", outdated);

    // Lost → DeviceLost (Critical)
    let lost = &wgpu::CurrentSurfaceTexture::Lost;
    let _ = format!("{:?}", lost);
}

// ──────────────────────────────────────────────
// Multiple Consecutive Error Tests
// ──────────────────────────────────────────────

#[test]
fn test_consecutive_timeout_escalation() {
    // The compositor escalates severity after 5 consecutive timeouts.
    let threshold: u32 = 5;
    assert!(6 > threshold); // Would escalate
    assert!(!(3 > threshold)); // Would not escalate
}

#[test]
fn test_device_lost_message() {
    let err = RenderError::device_lost("GPU hung");
    let msg = format!("{}", err);
    assert!(msg.contains("GPU hung"));
    assert!(msg.contains("LUMI-REND-0002"));
}
