//! # Memory System — Memory Types and Retrieval (Chapter 14)
//!
//! Defines the memory entry schema, storage types, retrieval logic,
//! and retention policies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Memory Types
// ---------------------------------------------------------------------------

/// Categories of memory stored by the Memory System.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    /// User-declared preferences.
    Preference,
    /// Information about projects the user is working on.
    Project,
    /// Information about people in the user's life.
    Person,
    /// Objective facts about the user or environment.
    Fact,
    /// User-stated goals or targets.
    Goal,
    /// Significant events or milestones.
    Event,
    /// User skills and capabilities.
    Skill,
    /// Observed workflows or habits.
    Workflow,
    /// Lumi-inferred observations (lower retention priority).
    Observation,
}

/// How a memory was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemorySource {
    /// User explicitly said "remember that..."
    UserExplicit,
    /// Extracted from conversation.
    Conversation,
    /// Inferred from completed work.
    TaskCompletion,
    /// Inferred from desktop activity.
    Observation,
    /// Imported from backup file.
    UserImport,
}

/// A single memory entry stored in the Memory System.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub memory_type: MemoryType,
    /// Natural language statement of the memory.
    pub content: String,
    pub source: MemorySource,
    /// Confidence from 0.0 to 1.0.
    pub confidence: f32,
    /// Importance from 0.0 to 1.0 (affects retention priority).
    pub importance: f32,
    pub created_at: i64,
    pub last_accessed: i64,
    pub access_count: u32,
    pub tags: Vec<String>,
    pub related_ids: Vec<String>,
    /// 768-dim embedding vector (all-MiniLM-L6-v2).
    pub embedding: Option<Vec<f32>>,
    pub user_verified: bool,
    /// Optional expiration for time-bound memories.
    pub expires_at: Option<i64>,
}

// ---------------------------------------------------------------------------
// Memory Retrieval
// ---------------------------------------------------------------------------

/// A memory entry with its composite relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub memory: MemoryEntry,
    pub score: f32,
}

/// Configuration for the memory retriever.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrieverConfig {
    /// Number of top results to return.
    pub top_k: usize,
    /// Minimum similarity threshold (0.0 to 1.0).
    pub min_similarity: f32,
    /// Weight of recency in composite score.
    pub recency_weight: f32,
    /// Weight of importance in composite score.
    pub importance_weight: f32,
    /// Token budget for memory injection into context.
    pub max_tokens: u32,
}

impl Default for RetrieverConfig {
    fn default() -> Self {
        Self {
            top_k: 10,
            min_similarity: 0.65,
            recency_weight: 0.2,
            importance_weight: 0.3,
            max_tokens: 1000,
        }
    }
}

/// A query for memory retrieval with optional filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub query_text: String,
    pub memory_type: Option<MemoryType>,
    pub tags: Option<Vec<String>>,
    pub min_confidence: Option<f32>,
    pub limit: Option<usize>,
}

/// A candidate memory extracted from conversation or task results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub memory_type: MemoryType,
    pub content: String,
    pub importance: f32,
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// Retention Policies
// ---------------------------------------------------------------------------

/// Retention policy for a specific memory type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Number of days before expiration.
    pub expires_days: u32,
    /// Minimum confidence for storage.
    pub min_confidence: f32,
    /// Whether to auto-verify this type.
    pub auto_verify: bool,
    /// Whether user confirmation is required before storing.
    pub require_user_confirmation: bool,
}

/// All retention policies keyed by memory type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    pub policies: HashMap<String, RetentionPolicy>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        let mut policies = HashMap::new();

        policies.insert(
            "preference".into(),
            RetentionPolicy {
                expires_days: 365,
                min_confidence: 0.7,
                auto_verify: false,
                require_user_confirmation: false,
            },
        );

        policies.insert(
            "observation".into(),
            RetentionPolicy {
                expires_days: 30,
                min_confidence: 0.85,
                auto_verify: false,
                require_user_confirmation: true,
            },
        );

        policies.insert(
            "fact".into(),
            RetentionPolicy {
                expires_days: 730,
                min_confidence: 0.8,
                auto_verify: false,
                require_user_confirmation: false,
            },
        );

        policies.insert(
            "goal".into(),
            RetentionPolicy {
                expires_days: 90,
                min_confidence: 0.75,
                auto_verify: false,
                require_user_confirmation: false,
            },
        );

        policies.insert(
            "project".into(),
            RetentionPolicy {
                expires_days: 180,
                min_confidence: 0.7,
                auto_verify: false,
                require_user_confirmation: false,
            },
        );

        Self { policies }
    }
}

// ---------------------------------------------------------------------------
// Memory Operations
// ---------------------------------------------------------------------------

/// A request to write a memory to the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteMemoryRequest {
    pub entry: MemoryEntry,
    pub generate_embedding: bool,
}

/// A request to query memories from the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMemoryRequest {
    pub query: MemoryQuery,
    pub embedding: Option<Vec<f32>>,
}

/// Result of a memory query operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryResult {
    pub memories: Vec<ScoredMemory>,
    pub total_count: u64,
    pub query_time_ms: u64,
}

/// Result of a memory write operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteMemoryResult {
    pub id: String,
    pub stored: bool,
    pub embedding_generated: bool,
}

// ---------------------------------------------------------------------------
// Utility Functions
// ---------------------------------------------------------------------------

/// Calculate recency score based on exponential decay over a 30-day half-life.
pub fn recency_score(last_accessed: i64, now: i64) -> f32 {
    let age_days = (now - last_accessed) as f32 / 86400.0;
    (-age_days / 30.0).exp()
}

/// Calculate composite retrieval score combining similarity, recency, and importance.
pub fn composite_score(
    similarity: f32,
    recency_weight: f32,
    importance_weight: f32,
    last_accessed: i64,
    importance: f32,
    now: i64,
) -> f32 {
    let recency = recency_score(last_accessed, now);
    similarity * (1.0 - recency_weight - importance_weight)
        + recency * recency_weight
        + importance * importance_weight
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recency_score() {
        let now = chrono::Utc::now().timestamp();
        // Just accessed
        let score_now = recency_score(now, now);
        assert!((score_now - 1.0).abs() < 0.01);

        // 30 days ago
        let thirty_days_ago = now - 30 * 86400;
        let score_30d = recency_score(thirty_days_ago, now);
        assert!((score_30d - 0.367).abs() < 0.01);

        // 60 days ago (should be ~0.135)
        let sixty_days_ago = now - 60 * 86400;
        let score_60d = recency_score(sixty_days_ago, now);
        assert!((score_60d - 0.135).abs() < 0.01);
    }

    #[test]
    fn test_composite_score() {
        let now = chrono::Utc::now().timestamp();
        let score = composite_score(0.8, 0.2, 0.3, now, 0.9, now);
        // similarity * 0.5 + recency * 0.2 + importance * 0.3
        // = 0.8 * 0.5 + 1.0 * 0.2 + 0.9 * 0.3
        // = 0.4 + 0.2 + 0.27 = 0.87
        assert!((score - 0.87).abs() < 0.01);
    }

    #[test]
    fn test_default_retention_config() {
        let config = RetentionConfig::default();
        assert!(config.policies.contains_key("preference"));
        assert!(config.policies.contains_key("observation"));
        assert!(config.policies.contains_key("fact"));

        let obs = &config.policies["observation"];
        assert_eq!(obs.expires_days, 30);
        assert!(obs.require_user_confirmation);
    }

    #[test]
    fn test_memory_entry_serialization() {
        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            memory_type: MemoryType::Preference,
            content: "User prefers dark mode".into(),
            source: MemorySource::Conversation,
            confidence: 0.9,
            importance: 0.7,
            created_at: chrono::Utc::now().timestamp(),
            last_accessed: chrono::Utc::now().timestamp(),
            access_count: 1,
            tags: vec!["dark-mode".into(), "editor".into()],
            related_ids: vec![],
            embedding: None,
            user_verified: false,
            expires_at: None,
        };

        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["content"], "User prefers dark mode");
        assert_eq!(json["tags"], serde_json::json!(["dark-mode", "editor"]));
    }
}
