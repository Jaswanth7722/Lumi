//! # Memory System — Persistent Memory and Retrieval (Chapter 14)
//!
//! Manages memory entries, vector embeddings for semantic search,
//! memory extraction from conversations, and retention policies.

use lumi_common::memory::{
    MemoryCandidate, MemoryEntry, MemoryQuery, MemoryQueryResult, MemorySource, MemoryType,
    RetrieverConfig, RetentionConfig, RetentionPolicy, ScoredMemory, WriteMemoryRequest,
    WriteMemoryResult, composite_score, recency_score,
};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// In-memory storage for the Memory System.
/// In production, this wraps SQLite + sqlite-vec.
pub struct MemorySystem {
    /// All stored memories by ID.
    memories: HashMap<String, MemoryEntry>,
    /// Retrieval configuration.
    retriever_config: RetrieverConfig,
    /// Retention policies by memory type.
    retention_config: RetentionConfig,
    /// Whether the system has been initialized.
    initialized: bool,
}

impl MemorySystem {
    pub fn new() -> Self {
        Self {
            memories: HashMap::new(),
            retriever_config: RetrieverConfig::default(),
            retention_config: RetentionConfig::default(),
            initialized: false,
        }
    }

    /// Initialize the memory system.
    pub async fn initialize(&mut self) {
        info!("Memory System initializing...");
        self.initialized = true;
        info!("Memory System ready ({} memories loaded)", self.memories.len());
    }

    /// Write a new memory entry.
    pub fn write_memory(&mut self, request: WriteMemoryRequest) -> WriteMemoryResult {
        let id = request.entry.id.clone();

        // Check retention policy
        let type_key = format!("{:?}", request.entry.memory_type).to_lowercase();
        if let Some(policy) = self.retention_config.policies.get(&type_key) {
            if request.entry.confidence < policy.min_confidence {
                debug!("Memory rejected: confidence {} below threshold {} for type {}",
                    request.entry.confidence, policy.min_confidence, type_key);
                return WriteMemoryResult {
                    id,
                    stored: false,
                    embedding_generated: false,
                };
            }
        }

        let mem_type = format!("{:?}", request.entry.memory_type).to_lowercase();
        self.memories.insert(id.clone(), request.entry);
        debug!("Memory stored: {} (type: {})", id, mem_type);
        WriteMemoryResult {
            id,
            stored: true,
            embedding_generated: request.generate_embedding,
        }
    }

    /// Query memories by semantic similarity.
    pub fn query_memories(&self, query: &MemoryQuery) -> MemoryQueryResult {
        let start = std::time::Instant::now();

        let now = chrono::Utc::now().timestamp_millis();
        let query_lower = query.query_text.to_lowercase();

        // Simple keyword-based scoring (in production, use vector similarity)
        let mut scored: Vec<ScoredMemory> = self.memories
            .values()
            .filter(|m| {
                // Apply type filter
                if let Some(ref mem_type) = query.memory_type {
                    if m.memory_type != *mem_type {
                        return false;
                    }
                }
                // Apply confidence filter
                if let Some(min_conf) = query.min_confidence {
                    if m.confidence < min_conf {
                        return false;
                    }
                }
                true
            })
            .map(|m| {
                // Simple keyword overlap score as a stand-in for embedding similarity
                let content_lower = m.content.to_lowercase();
                let query_words: Vec<&str> = query_lower.split_whitespace().collect();
                let matches = query_words.iter()
                    .filter(|w| content_lower.contains(*w))
                    .count();
                let similarity = if query_words.is_empty() {
                    0.0
                } else {
                    matches as f32 / query_words.len() as f32
                };

                let score = composite_score(
                    similarity * 0.8, // scale keyword score
                    self.retriever_config.recency_weight,
                    self.retriever_config.importance_weight,
                    m.last_accessed,
                    m.importance,
                    now,
                );

                ScoredMemory {
                    memory: m.clone(),
                    score,
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        // Apply limit
        let limit = query.limit.unwrap_or(self.retriever_config.top_k);
        scored.truncate(limit.min(self.retriever_config.top_k));

        MemoryQueryResult {
            total_count: self.memories.len() as u64,
            memories: scored,
            query_time_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Extract memory candidates from conversation messages.
    pub fn extract_memories(&self, messages: &[String]) -> Vec<MemoryCandidate> {
        let mut candidates = Vec::new();

        for msg in messages {
            let lower = msg.to_lowercase();

            // Detect preference statements
            if lower.contains("prefer") || lower.contains("like") || lower.contains("favorite") {
                candidates.push(MemoryCandidate {
                    memory_type: MemoryType::Preference,
                    content: msg.clone(),
                    importance: 0.6,
                    tags: vec!["preference".into()],
                });
            }

            // Detect project mentions
            if lower.contains("project") || lower.contains("working on") {
                candidates.push(MemoryCandidate {
                    memory_type: MemoryType::Project,
                    content: msg.clone(),
                    importance: 0.7,
                    tags: vec!["project".into()],
                });
            }

            // Detect goals
            if lower.contains("goal") || lower.contains("want to") || lower.contains("plan to") {
                candidates.push(MemoryCandidate {
                    memory_type: MemoryType::Goal,
                    content: msg.clone(),
                    importance: 0.8,
                    tags: vec!["goal".into()],
                });
            }
        }

        candidates
    }

    /// Delete a specific memory by ID.
    pub fn delete_memory(&mut self, id: &str) -> bool {
        self.memories.remove(id).is_some()
    }

    /// Delete all memories.
    pub fn purge_all(&mut self) -> u64 {
        let count = self.memories.len() as u64;
        self.memories.clear();
        info!("All memories purged ({} entries)", count);
        count
    }

    /// Get all memories (for export).
    pub fn all_memories(&self) -> Vec<&MemoryEntry> {
        self.memories.values().collect()
    }

    /// Get total memory count.
    pub fn memory_count(&self) -> usize {
        self.memories.len()
    }

    /// Update the last accessed time and access count for a memory.
    pub fn touch_memory(&mut self, id: &str) {
        if let Some(memory) = self.memories.get_mut(id) {
            memory.last_accessed = chrono::Utc::now().timestamp_millis();
            memory.access_count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_memory(content: &str, mem_type: MemoryType, importance: f32) -> MemoryEntry {
        MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            memory_type: mem_type,
            content: content.to_string(),
            source: MemorySource::Conversation,
            confidence: 0.9,
            importance,
            created_at: chrono::Utc::now().timestamp_millis(),
            last_accessed: chrono::Utc::now().timestamp_millis(),
            access_count: 0,
            tags: vec![],
            related_ids: vec![],
            embedding: None,
            user_verified: false,
            expires_at: None,
        }
    }

    #[test]
    fn test_write_and_query_memories() {
        let mut system = MemorySystem::new();

        system.write_memory(WriteMemoryRequest {
            entry: create_test_memory("User prefers TypeScript", MemoryType::Preference, 0.8),
            generate_embedding: false,
        });

        system.write_memory(WriteMemoryRequest {
            entry: create_test_memory("Working on Lumi project", MemoryType::Project, 0.7),
            generate_embedding: false,
        });

        assert_eq!(system.memory_count(), 2);

        // Query by keyword
        let result = system.query_memories(&MemoryQuery {
            query_text: "TypeScript preference".into(),
            memory_type: None,
            tags: None,
            min_confidence: None,
            limit: None,
        });

        assert!(!result.memories.is_empty());
        // The TypeScript memory should rank higher
        assert!(result.memories[0].memory.content.contains("TypeScript"));
    }

    #[test]
    fn test_type_filter() {
        let mut system = MemorySystem::new();

        system.write_memory(WriteMemoryRequest {
            entry: create_test_memory("User likes Rust", MemoryType::Preference, 0.6),
            generate_embedding: false,
        });

        let result = system.query_memories(&MemoryQuery {
            query_text: "Rust".into(),
            memory_type: Some(MemoryType::Preference),
            tags: None,
            min_confidence: None,
            limit: None,
        });

        assert_eq!(result.memories.len(), 1);
    }

    #[test]
    fn test_delete_memory() {
        let mut system = MemorySystem::new();
        let entry = create_test_memory("Test", MemoryType::Fact, 0.5);
        let id = entry.id.clone();

        system.write_memory(WriteMemoryRequest {
            entry,
            generate_embedding: false,
        });

        assert!(system.delete_memory(&id));
        assert!(!system.delete_memory("non-existent"));
    }

    #[test]
    fn test_purge_all() {
        let mut system = MemorySystem::new();
        system.write_memory(WriteMemoryRequest {
            entry: create_test_memory("Mem 1", MemoryType::Fact, 0.5),
            generate_embedding: false,
        });
        system.write_memory(WriteMemoryRequest {
            entry: create_test_memory("Mem 2", MemoryType::Fact, 0.5),
            generate_embedding: false,
        });

        assert_eq!(system.purge_all(), 2);
        assert_eq!(system.memory_count(), 0);
    }

    #[test]
    fn test_extract_memories() {
        let system = MemorySystem::new();
        let messages = vec![
            "I prefer dark mode in all my editors".into(),
            "I'm working on a new React project".into(),
            "My goal is to launch by Q4".into(),
            "The weather is nice today".into(), // should not match
        ];

        let candidates = system.extract_memories(&messages);
        assert!(candidates.iter().any(|c| matches!(c.memory_type, MemoryType::Preference)));
        assert!(candidates.iter().any(|c| matches!(c.memory_type, MemoryType::Project)));
        assert!(candidates.iter().any(|c| matches!(c.memory_type, MemoryType::Goal)));
    }

    #[test]
    fn test_confidence_threshold() {
        let mut system = MemorySystem::new();

        // This should be stored
        let result = system.write_memory(WriteMemoryRequest {
            entry: create_test_memory("High confidence", MemoryType::Preference, 0.8),
            generate_embedding: false,
        });
        assert!(result.stored);

        // Override confidence to be too low
        let mut low_conf_entry = create_test_memory("Low confidence", MemoryType::Observation, 0.5);
        low_conf_entry.confidence = 0.3; // below observation threshold of 0.85

        let result = system.write_memory(WriteMemoryRequest {
            entry: low_conf_entry,
            generate_embedding: false,
        });
        assert!(!result.stored); // observation requires min_confidence 0.85
    }
}
