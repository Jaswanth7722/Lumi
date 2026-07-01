//! # Typed Identifiers
//!
//! Strongly-typed process and worker identifiers for the Lumas process
//! management system. Uses hierarchical dot-notation paths reflecting
//! the supervision tree structure, with UUID disambiguation for restarts.
//!
//! # Thread Safety
//!
//! Both `ProcessId` and `WorkerId` are `Send + Sync`, `Clone`, and
//! `Copy`-free (heap-allocated string). They are safe to use as
//! `HashMap`/`DashMap` keys.
//!
//! # Examples
//!
//! ```
//! use lumas_process::ProcessId;
//!
//! let root = ProcessId::root();
//! assert_eq!(root.path(), "lumi");
//!
//! let child = root.child("render");
//! assert_eq!(child.path(), "lumi.render");
//!
//! let nested = child.child("animation");
//! assert_eq!(nested.path(), "lumi.render.animation");
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::{Hash, Hasher};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ProcessId
// ---------------------------------------------------------------------------

/// A strongly-typed, globally unique process identifier.
///
/// Uses a hierarchical dot-notation path reflecting the supervision tree:
/// - `"lumi"` — root supervisor
/// - `"lumi.render"` — render subsystem
/// - `"lumi.plugin-host.plugin:github-tools"` — plugin sandbox
///
/// The UUID component disambiguates between restarts of the same process.
/// Display shows the short form: `lumi.render#a1b2c3d4`.
///
/// # Thread Safety
///
/// `ProcessId` is `Send + Sync`, `Clone`, and suitable as a map key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessId {
    /// Dot-notation path (e.g., "lumi.plugin-host.plugin:github-tools").
    pub path: String,
    /// UUID that disambiguates restarts of the same process.
    pub uuid: Uuid,
}

impl ProcessId {
    /// Create a new `ProcessId` with the given hierarchical path.
    ///
    /// A new UUIDv4 is generated for each call.
    ///
    /// # Panics
    ///
    /// Never panics.
    ///
    /// # Examples
    ///
    /// ```
    /// use lumas_process::ProcessId;
    /// let pid = ProcessId::new("lumi.render");
    /// assert_eq!(pid.path(), "lumi.render");
    /// ```
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            uuid: Uuid::new_v4(),
        }
    }

    /// Create the root supervisor process ID (`"lumi"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use lumas_process::ProcessId;
    /// assert_eq!(ProcessId::root().path(), "lumi");
    /// ```
    pub fn root() -> Self {
        Self {
            path: "lumi".to_string(),
            uuid: Uuid::new_v4(),
        }
    }

    /// Create a child process ID by appending `name` to this path.
    ///
    /// The child's path becomes `"{self.path}.{name}"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use lumas_process::ProcessId;
    /// let parent = ProcessId::new("lumi");
    /// let child = parent.child("render");
    /// assert_eq!(child.path(), "lumi.render");
    /// ```
    pub fn child(&self, name: &str) -> Self {
        let mut child_path = self.path.clone();
        child_path.push('.');
        child_path.push_str(name);
        Self {
            path: child_path,
            uuid: Uuid::new_v4(),
        }
    }

    /// Returns the hierarchical path string.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the UUID component.
    pub fn uuid(&self) -> &Uuid {
        &self.uuid
    }

    /// Returns the parent path, if any.
    ///
    /// The parent is everything before the last `.` in the path.
    /// Returns `None` for root-level processes (no `.` in path).
    ///
    /// # Examples
    ///
    /// ```
    /// use lumas_process::ProcessId;
    /// let pid = ProcessId::new("lumi.render.animation");
    /// assert_eq!(pid.parent_path(), Some("lumi.render"));
    ///
    /// let root = ProcessId::root();
    /// assert_eq!(root.parent_path(), None);
    /// ```
    pub fn parent_path(&self) -> Option<&str> {
        self.path.rfind('.').map(|pos| &self.path[..pos])
    }

    /// Returns the last component of the path (simplified name).
    ///
    /// # Examples
    ///
    /// ```
    /// use lumas_process::ProcessId;
    /// let pid = ProcessId::new("lumi.render.animation");
    /// assert_eq!(pid.short_name(), "animation");
    /// ```
    pub fn short_name(&self) -> &str {
        self.path.rsplit('.').next().unwrap_or(&self.path)
    }

    /// Creates a copy of this `ProcessId` with a new UUID.
    ///
    /// Used when restarting a process — the path remains the same but
    /// the UUID changes to distinguish the new instance.
    pub fn with_new_uuid(&self) -> Self {
        Self {
            path: self.path.clone(),
            uuid: Uuid::new_v4(),
        }
    }
}

impl PartialEq for ProcessId {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.uuid == other.uuid
    }
}

impl Eq for ProcessId {}

impl Hash for ProcessId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
        self.uuid.hash(state);
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show short UUID for readability: lumi.render#a1b2c3d4
        let short = &self.uuid.to_string()[..8];
        write!(f, "{}#{}", self.path, short)
    }
}

impl From<String> for ProcessId {
    fn from(path: String) -> Self {
        Self::new(path)
    }
}

impl From<&str> for ProcessId {
    fn from(path: &str) -> Self {
        Self::new(path.to_string())
    }
}

// ---------------------------------------------------------------------------
// WorkerId
// ---------------------------------------------------------------------------

/// Identifies a background worker within a specific process.
///
/// Workers are owned by a process and are stopped when the owning
/// process is stopped. The UUID disambiguates between worker restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerId {
    /// The process that owns this worker.
    pub owner: ProcessId,
    /// The worker's name (unique within the owner process).
    pub name: String,
    /// UUID that disambiguates between restarts.
    pub uuid: Uuid,
}

impl WorkerId {
    /// Create a new `WorkerId` for the given owner process and name.
    ///
    /// # Examples
    ///
    /// ```
    /// use lumas_process::{ProcessId, WorkerId};
    /// let owner = ProcessId::new("lumi.core");
    /// let wid = WorkerId::new(owner.clone(), "health-checker");
    /// assert_eq!(wid.owner.path(), "lumi.core");
    /// ```
    pub fn new(owner: ProcessId, name: impl Into<String>) -> Self {
        Self {
            owner,
            name: name.into(),
            uuid: Uuid::new_v4(),
        }
    }

    /// Creates a copy with a new UUID (for restarts).
    pub fn with_new_uuid(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            name: self.name.clone(),
            uuid: Uuid::new_v4(),
        }
    }
}

impl PartialEq for WorkerId {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner && self.name == other.name && self.uuid == other.uuid
    }
}

impl Eq for WorkerId {}

impl Hash for WorkerId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.owner.hash(state);
        self.name.hash(state);
        self.uuid.hash(state);
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let short = &self.uuid.to_string()[..8];
        write!(f, "{}.worker.{}#{}", self.owner.path, self.name, short)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_id_creation() {
        let pid = ProcessId::new("lumi.render");
        assert_eq!(pid.path(), "lumi.render");
        assert_eq!(pid.short_name(), "render");
    }

    #[test]
    fn test_root_process_id() {
        let root = ProcessId::root();
        assert_eq!(root.path(), "lumi");
        assert_eq!(root.short_name(), "lumi");
    }

    #[test]
    fn test_child_process_id() {
        let parent = ProcessId::new("lumi");
        let child = parent.child("plugin-host");
        let grandchild = child.child("plugin:editor");
        assert_eq!(child.path(), "lumi.plugin-host");
        assert_eq!(grandchild.path(), "lumi.plugin-host.plugin:editor");
    }

    #[test]
    fn test_parent_path() {
        let pid = ProcessId::new("lumi.render.animation");
        assert_eq!(pid.parent_path(), Some("lumi.render"));
        assert_eq!(ProcessId::root().parent_path(), None);
    }

    #[test]
    fn test_short_name() {
        assert_eq!(ProcessId::new("lumi").short_name(), "lumi");
        assert_eq!(ProcessId::new("lumi.render").short_name(), "render");
        assert_eq!(
            ProcessId::new("lumi.plugin-host.plugin:editor").short_name(),
            "plugin:editor"
        );
    }

    #[test]
    fn test_display() {
        let pid = ProcessId::new("lumi.render");
        let display = pid.to_string();
        assert!(display.starts_with("lumi.render#"));
        assert_eq!(display.len(), "lumi.render#".len() + 8);
    }

    #[test]
    fn test_process_id_equality() {
        let pid1 = ProcessId::new("test");
        let pid2 = ProcessId::new("test");
        assert_ne!(pid1, pid2); // Different UUIDs
        assert_eq!(pid1.path, pid2.path);
    }

    #[test]
    fn test_with_new_uuid() {
        let original = ProcessId::new("lumi.core");
        let renewed = original.with_new_uuid();
        assert_eq!(original.path, renewed.path);
        assert_ne!(original.uuid, renewed.uuid);
    }

    #[test]
    fn test_worker_id_display() {
        let owner = ProcessId::new("lumi.core");
        let wid = WorkerId::new(owner, "health");
        let display = wid.to_string();
        assert!(display.contains("lumi.core"));
        assert!(display.contains("health"));
    }

    #[test]
    fn test_process_id_from_str() {
        let pid: ProcessId = "custom.path".into();
        assert_eq!(pid.path(), "custom.path");
    }
}
