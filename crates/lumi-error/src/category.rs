//! # Error Category Taxonomy
//!
//! Typed, structured error categories for every subsystem.
//! Each variant carries typed metadata — not free-form strings.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Error category with typed metadata for each subsystem.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Core runtime errors.
    Runtime,
    /// Configuration errors.
    Configuration {
        /// The specific field that caused the error, if known.
        field: Option<Cow<'static, str>>,
    },
    /// Logging system errors.
    Logging,
    /// IPC (inter-process communication) errors.
    Ipc {
        /// The channel name where the error occurred.
        channel: Cow<'static, str>,
    },
    /// Rendering engine errors.
    Rendering {
        /// The render pass that failed.
        pass: Option<RenderPass>,
    },
    /// Character engine errors.
    Character {
        /// The bone name, if applicable.
        bone: Option<Cow<'static, str>>,
    },
    /// Animation system errors.
    Animation {
        /// The animation clip name, if applicable.
        clip: Option<Cow<'static, str>>,
    },
    /// AI core errors.
    AiCore {
        /// The AI provider name, if applicable.
        provider: Option<Cow<'static, str>>,
    },
    /// Memory subsystem errors.
    Memory {
        /// The store that failed.
        store: MemoryStoreHint,
    },
    /// Workspace panel errors.
    WorkspacePanel,
    /// Plugin system errors.
    Plugin {
        /// The plugin ID that caused the error.
        plugin_id: Cow<'static, str>,
    },
    /// Voice system errors.
    Voice {
        /// The voice processing stage.
        stage: VoiceStage,
    },
    /// Audio subsystem errors.
    Audio,
    /// Storage subsystem errors.
    Storage {
        /// The storage path, if known.
        path: Option<std::path::PathBuf>,
    },
    /// Security violations.
    Security {
        /// The type of security violation.
        violation: SecurityViolation,
    },
    /// Network errors.
    Network {
        /// The URL, if known.
        url: Option<Cow<'static, str>>,
        /// The HTTP status code, if applicable.
        status: Option<u16>,
    },
    /// Filesystem errors.
    Filesystem {
        /// The path, if known.
        path: Option<std::path::PathBuf>,
        /// The filesystem operation.
        operation: FilesystemOp,
    },
    /// Validation errors.
    Validation {
        /// The field that failed validation.
        field: Cow<'static, str>,
        /// The constraint that was violated.
        constraint: Cow<'static, str>,
    },
    /// Permission errors.
    Permission {
        /// The resource that was denied.
        resource: Cow<'static, str>,
    },
    /// Internal invariant violations.
    Internal,
    /// Unknown/unclassified.
    Unknown,
}

impl ErrorCategory {
    /// Short code for use in error code formatting.
    pub fn short_code(&self) -> &'static str {
        match self {
            ErrorCategory::Runtime => "RTE",
            ErrorCategory::Configuration { .. } => "CFG",
            ErrorCategory::Logging => "LOG",
            ErrorCategory::Ipc { .. } => "IPC",
            ErrorCategory::Rendering { .. } => "RND",
            ErrorCategory::Character { .. } => "CHR",
            ErrorCategory::Animation { .. } => "ANM",
            ErrorCategory::AiCore { .. } => "AI",
            ErrorCategory::Memory { .. } => "MEM",
            ErrorCategory::WorkspacePanel => "WSP",
            ErrorCategory::Plugin { .. } => "PLG",
            ErrorCategory::Voice { .. } => "VOI",
            ErrorCategory::Audio => "AUD",
            ErrorCategory::Storage { .. } => "STR",
            ErrorCategory::Security { .. } => "SEC",
            ErrorCategory::Network { .. } => "NET",
            ErrorCategory::Filesystem { .. } => "FS",
            ErrorCategory::Validation { .. } => "VAL",
            ErrorCategory::Permission { .. } => "PER",
            ErrorCategory::Internal => "INT",
            ErrorCategory::Unknown => "UNK",
        }
    }

    /// Human-readable category name.
    pub fn display_name(&self) -> &'static str {
        match self {
            ErrorCategory::Runtime => "Runtime",
            ErrorCategory::Configuration { .. } => "Configuration",
            ErrorCategory::Logging => "Logging",
            ErrorCategory::Ipc { .. } => "IPC",
            ErrorCategory::Rendering { .. } => "Rendering",
            ErrorCategory::Character { .. } => "Character",
            ErrorCategory::Animation { .. } => "Animation",
            ErrorCategory::AiCore { .. } => "AI Core",
            ErrorCategory::Memory { .. } => "Memory",
            ErrorCategory::WorkspacePanel => "Workspace",
            ErrorCategory::Plugin { .. } => "Plugin",
            ErrorCategory::Voice { .. } => "Voice",
            ErrorCategory::Audio => "Audio",
            ErrorCategory::Storage { .. } => "Storage",
            ErrorCategory::Security { .. } => "Security",
            ErrorCategory::Network { .. } => "Network",
            ErrorCategory::Filesystem { .. } => "Filesystem",
            ErrorCategory::Validation { .. } => "Validation",
            ErrorCategory::Permission { .. } => "Permission",
            ErrorCategory::Internal => "Internal",
            ErrorCategory::Unknown => "Unknown",
        }
    }
}

impl Default for ErrorCategory {
    fn default() -> Self {
        ErrorCategory::Unknown
    }
}

// ---------------------------------------------------------------------------
// Sub-types used in category metadata
// ---------------------------------------------------------------------------

/// Render pass identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RenderPass {
    /// Main character rendering.
    Character,
    /// Post-processing effects.
    PostProcessing,
    /// UI overlay rendering.
    UiOverlay,
    /// Shadow rendering.
    Shadows,
    /// Particle effects.
    Particles,
}

/// Memory store hint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryStoreHint {
    /// Long-term memory store.
    LongTerm,
    /// Working/short-term memory store.
    Working,
    /// Episodic memory store.
    Episodic,
    /// Entity/relationship store.
    Entity,
}

/// Voice processing stage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VoiceStage {
    /// Wake word detection.
    WakeWord,
    /// Speech-to-text.
    Stt,
    /// Text-to-speech.
    Tts,
    /// Voice activity detection.
    Vad,
    /// Audio capture.
    Capture,
}

/// Security violation type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecurityViolation {
    /// Authentication failure.
    Authentication,
    /// Authorization failure.
    Authorization,
    /// Sandbox escape attempt.
    SandboxEscape,
    /// Plugin capability violation.
    CapabilityViolation,
    /// Network policy violation.
    NetworkPolicy,
    /// Filesystem policy violation.
    FilesystemPolicy,
    /// Secret access violation.
    SecretAccess,
}

/// Filesystem operation type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FilesystemOp {
    /// File read.
    Read,
    /// File write.
    Write,
    /// File delete.
    Delete,
    /// Directory creation.
    CreateDir,
    /// File rename.
    Rename,
    /// File metadata access.
    Metadata,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
