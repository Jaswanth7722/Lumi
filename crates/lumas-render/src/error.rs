//! Render error types with severity classification.
//!
//! Errors are classified by severity to guide recovery behavior:
//! - `Fatal`: Unrecoverable; process must be terminated and restarted.
//! - `Critical`: Recoverable with full device/context recreation.
//! - `Warning`: Non-fatal; operation failed but rendering can continue.
//! - `Recoverable`: Operation can be retried on the next frame.

use std::fmt;
use thiserror::Error;

/// Severity level for render errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorSeverity {
    /// Fatal: process must terminate.
    Fatal,
    /// Critical: device/context must be recreated.
    Critical,
    /// Recoverable: operation can be retried next frame.
    Recoverable,
    /// Warning: non-fatal, rendering continues.
    Warning,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Fatal => write!(f, "FATAL"),
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
            ErrorSeverity::Recoverable => write!(f, "RECOVERABLE"),
            ErrorSeverity::Warning => write!(f, "WARNING"),
        }
    }
}

/// Render engine errors.
///
/// Each variant carries a severity that guides the recovery strategy.
#[derive(Debug, Error)]
pub enum RenderError {
    #[error("[LUMI-REND-0001] Adapter not found: {requirements}")]
    AdapterNotFound {
        requirements: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0002] Device lost: {reason}")]
    DeviceLost {
        reason: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0003] Surface outdated")]
    SurfaceOutdated { severity: ErrorSeverity },

    #[error("[LUMI-REND-0004] Surface acquisition timeout")]
    SurfaceTimeout { severity: ErrorSeverity },

    #[error("[LUMI-REND-0005] Shader compilation failed for '{shader_id}': {cause}")]
    ShaderCompilationFailed {
        shader_id: String,
        cause: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0006] Shader validation failed for '{shader_id}': {cause}")]
    ShaderValidationFailed {
        shader_id: String,
        cause: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0007] Pipeline creation failed for '{pipeline_id}': {cause}")]
    PipelineCreationFailed {
        pipeline_id: String,
        cause: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0008] Texture upload failed for '{texture_id}': {cause}")]
    TextureUploadFailed {
        texture_id: String,
        cause: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0009] Texture format '{format}' not supported on this device")]
    TextureFormatUnsupported {
        format: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0010] Mesh '{mesh_id}' not found")]
    MeshNotFound {
        mesh_id: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0011] Material '{material_id}' not found")]
    MaterialNotFound {
        material_id: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0012] Render pass '{pass}' failed: {cause}")]
    RenderPassFailed {
        pass: &'static str,
        cause: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0013] Graph compilation failed (cycle: {cycle_detected})")]
    GraphCompilationFailed {
        cycle_detected: bool,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0014] Frame budget exceeded for pass '{pass}': {actual_us}us > {budget_us}us")]
    FrameBudgetExceeded {
        pass: &'static str,
        actual_us: u64,
        budget_us: u64,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0015] Buffer map failed for '{buffer_id}': {cause}")]
    BufferMapFailed {
        buffer_id: String,
        cause: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0016] Out of video memory: required {required_bytes} bytes, available {available_bytes}")]
    OutOfVideoMemory {
        required_bytes: u64,
        available_bytes: u64,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0017] Feature '{feature}' not supported on backend '{backend}'")]
    FeatureNotSupported {
        feature: String,
        backend: String,
        severity: ErrorSeverity,
    },

    #[error("[LUMI-REND-0018] Resource '{resource_id}' not found in pool")]
    ResourceNotFound {
        resource_id: String,
        severity: ErrorSeverity,
    },
}

impl RenderError {
    /// Returns the severity of this error.
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            RenderError::AdapterNotFound { .. } => ErrorSeverity::Fatal,
            RenderError::DeviceLost { .. } => ErrorSeverity::Critical,
            RenderError::SurfaceOutdated { .. } => ErrorSeverity::Recoverable,
            RenderError::SurfaceTimeout { .. } => ErrorSeverity::Warning,
            RenderError::ShaderCompilationFailed { .. } => ErrorSeverity::Critical,
            RenderError::ShaderValidationFailed { .. } => ErrorSeverity::Critical,
            RenderError::PipelineCreationFailed { .. } => ErrorSeverity::Critical,
            RenderError::TextureUploadFailed { .. } => ErrorSeverity::Warning,
            RenderError::TextureFormatUnsupported { .. } => ErrorSeverity::Warning,
            RenderError::MeshNotFound { .. } => ErrorSeverity::Warning,
            RenderError::MaterialNotFound { .. } => ErrorSeverity::Warning,
            RenderError::RenderPassFailed { .. } => ErrorSeverity::Critical,
            RenderError::GraphCompilationFailed { .. } => ErrorSeverity::Fatal,
            RenderError::FrameBudgetExceeded { .. } => ErrorSeverity::Warning,
            RenderError::BufferMapFailed { .. } => ErrorSeverity::Warning,
            RenderError::OutOfVideoMemory { .. } => ErrorSeverity::Critical,
            RenderError::FeatureNotSupported { .. } => ErrorSeverity::Warning,
            RenderError::ResourceNotFound { .. } => ErrorSeverity::Warning,
        }
    }

    /// Returns `true` if this error requires device recreation.
    pub fn requires_device_recreation(&self) -> bool {
        matches!(self.severity(), ErrorSeverity::Critical | ErrorSeverity::Fatal)
    }

    /// Returns the Lumas error code (e.g., "LUMI-REND-0001").
    pub fn error_code(&self) -> &'static str {
        match self {
            RenderError::AdapterNotFound { .. } => "LUMI-REND-0001",
            RenderError::DeviceLost { .. } => "LUMI-REND-0002",
            RenderError::SurfaceOutdated { .. } => "LUMI-REND-0003",
            RenderError::SurfaceTimeout { .. } => "LUMI-REND-0004",
            RenderError::ShaderCompilationFailed { .. } => "LUMI-REND-0005",
            RenderError::ShaderValidationFailed { .. } => "LUMI-REND-0006",
            RenderError::PipelineCreationFailed { .. } => "LUMI-REND-0007",
            RenderError::TextureUploadFailed { .. } => "LUMI-REND-0008",
            RenderError::TextureFormatUnsupported { .. } => "LUMI-REND-0009",
            RenderError::MeshNotFound { .. } => "LUMI-REND-0010",
            RenderError::MaterialNotFound { .. } => "LUMI-REND-0011",
            RenderError::RenderPassFailed { .. } => "LUMI-REND-0012",
            RenderError::GraphCompilationFailed { .. } => "LUMI-REND-0013",
            RenderError::FrameBudgetExceeded { .. } => "LUMI-REND-0014",
            RenderError::BufferMapFailed { .. } => "LUMI-REND-0015",
            RenderError::OutOfVideoMemory { .. } => "LUMI-REND-0016",
            RenderError::FeatureNotSupported { .. } => "LUMI-REND-0017",
            RenderError::ResourceNotFound { .. } => "LUMI-REND-0018",
        }
    }
}

// Convenience constructors to reduce boilerplate.
impl RenderError {
    pub fn adapter_not_found(requirements: impl Into<String>) -> Self {
        RenderError::AdapterNotFound {
            requirements: requirements.into(),
            severity: ErrorSeverity::Fatal,
        }
    }

    pub fn device_lost(reason: impl Into<String>) -> Self {
        RenderError::DeviceLost {
            reason: reason.into(),
            severity: ErrorSeverity::Critical,
        }
    }

    pub fn shader_compilation_failed(shader_id: impl Into<String>, cause: impl Into<String>) -> Self {
        RenderError::ShaderCompilationFailed {
            shader_id: shader_id.into(),
            cause: cause.into(),
            severity: ErrorSeverity::Critical,
        }
    }

    pub fn pipeline_creation_failed(pipeline_id: impl Into<String>, cause: impl Into<String>) -> Self {
        RenderError::PipelineCreationFailed {
            pipeline_id: pipeline_id.into(),
            cause: cause.into(),
            severity: ErrorSeverity::Critical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_classification() {
        assert_eq!(
            RenderError::adapter_not_found("no GPU").severity(),
            ErrorSeverity::Fatal
        );
        assert_eq!(
            RenderError::device_lost("driver crash").severity(),
            ErrorSeverity::Critical
        );
        assert_eq!(
            RenderError::SurfaceOutdated {
                severity: ErrorSeverity::Recoverable
            }
            .severity(),
            ErrorSeverity::Recoverable
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(RenderError::adapter_not_found("").error_code(), "LUMI-REND-0001");
        assert_eq!(RenderError::device_lost("").error_code(), "LUMI-REND-0002");
    }

    #[test]
    fn test_device_recreation_required() {
        assert!(RenderError::adapter_not_found("").requires_device_recreation());
        assert!(RenderError::device_lost("").requires_device_recreation());
        assert!(!RenderError::SurfaceOutdated { severity: ErrorSeverity::Recoverable }.requires_device_recreation());
    }
}
