//! # Memory Store — Persistent Memory Storage (Chapter 14.4)
//!
//! Wraps SQLite + sqlite-vec for memory persistence with vector search.
//! In production, uses rusqlite with the sqlite-vec extension.
//! For the skeleton, uses an in-memory HashMap.

use lumi_common::memory::{
    MemoryEntry, MemoryQuery, MemoryQueryResult, QueryMemoryRequest, RetrieverConfig,
    WriteMemoryRequest, WriteMemoryResult, composite_score,
};
use std::collections::HashMap;
use tracing::debug;

/// In-memory storage for memories.
/// In production, this wraps SQLite with the following schema:
/// ```sql
/// CREATE TABLE memories (
///     id TEXT PRIMARY KEY,
///     type TEXT NOT NULL,
///     content TEXT NOT NULL,
///     source TEXT NOT NULL,
///     confidence REAL NOT NULL,
///     importance REAL NOT NULL,
///     created_at INTEGER NOT NULL,
///     last_accessed INTEGER NOT NULL,
///     access_count INTEGER NOT NULL,
///     tags TEXT NOT NULL DEFAULT '[]',
///     user_verified INTEGER NOT NULL DEFAULT 0,
///     expires_at INTEGER
/// );
/// CREATE VIRTUAL TABLE memory_embeddings USING vec0(
///     memory_id TEXT,
///     embedding float[768]
/// );
/// ```
pub struct MemoryStore {
    memories: HashMap<String, MemoryEntry>,
    retriever_config: RetrieverConfig,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            memories: HashMap::new(),
            retriever_config: RetrieverConfig::default(),
        }
    }

    /// Write a memory entry to the store.
    pub fn write(&mut self, request: WriteMemoryRequest) -> WriteMemoryResult {
        let id = request.entry.id.clone();
        self.memories.insert(id.clone(), request.entry);
        debug!("Memory stored: {id}");
        WriteMemoryResult {
            id,
            stored: true,
            embedding_generated: request.generate_embedding,
        }
    }

    /// Query memories by text and optional filters.
    pub fn query(&self, request: QueryMemoryRequest) -> MemoryQueryResult {
        let start = std::time::Instant::now();
        let now = chrono::Utc::now().timestamp_millis();
        let query_lower = request.query.query_text.to_lowercase();

        let mut scored: Vec<_> = self
            .memories
            .values()
            .filter(|m| {
                if let Some(ref mem_type) = request.query.memory_type {
                    if m.memory_type != *mem_type {
                        return false;
                    }
                }
                if let Some(ref tags) = request.query.tags {
                    if !tags.iter().any(|t| m.tags.contains(t)) {
                        return false;
                    }
                }
                if let Some(min_conf) = request.query.min_confidence {
                    if m.confidence < min_conf {
                        return false;
                    }
                }
                true
            })
            .map(|m| {
                let content_lower = m.content.to_lowercase();
                let query_words: Vec<&str> = query_lower.split_whitespace().collect();
                let matches = query_words
                    .iter()
                    .filter(|w| content_lower.contains(*w))
                    .count();
                let similarity = if query_words.is_empty() {
                    0.0
                } else {
                    matches as f32 / query_words.len() as f32
                };

                let score = composite_score(
                    similarity * 0.8,
                    self.retriever_config.recency_weight,
                    self.retriever_config.importance_weight,
                    m.last_accessed,
                    m.importance,
                    now,
                );

                (m.clone(), score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let limit = request.query.limit.unwrap_or(self.retriever_config.top_k);
        scored.truncate(limit);

        MemoryQueryResult {
            memories: scored
                .into_iter()
                .map(|(memory, score)| lumi_common::memory::ScoredMemory { memory, score })
                .collect(),
            total_count: self.memories.len() as u64,
            query_time_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Delete a specific memory.
    pub fn delete(&mut self, id: &str) -> bool {
        self.memories.remove(id).is_some()
    }

    /// Get a memory by ID.
    pub fn get(&self, id: &str) -> Option<&MemoryEntry> {
        self.memories.get(id)
    }

    /// Get all memories (for export).
    pub fn all(&self) -> Vec<&MemoryEntry> {
        self.memories.values().collect()
    }

    /// Get the total count of stored memories.
    pub fn count(&self) -> usize {
        self.memories.len()
    }

    /// Purge all memories.
    pub fn purge(&mut self) -> u64 {
        let count = self.memories.len() as u64;
        self.memories.clear();
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumi_common::memory::MemoryType;

    fn test_entry(content: &str) -> MemoryEntry {
        MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            memory_type: MemoryType::Preference,
            content: content.to_string(),
            source: lumi_common::memory::MemorySource::Conversation,
            confidence: 0.9,
            importance: 0.7,
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
    fn test_write_and_query() {
        let mut store = MemoryStore::new();
        let entry = test_entry("User prefers dark mode");
        store.write(WriteMemoryRequest {
            entry,
            generate_embedding: false,
        });

        let result = store.query(QueryMemoryRequest {
            query: MemoryQuery {
                query_text: "dark mode".into(),
                memory_type: None,
                tags: None,
                min_confidence: None,
                limit: None,
            },
            embedding: None,
        });

        assert_eq!(result.memories.len(), 1);
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn test_delete() {
        let mut store = MemoryStore::new();
        let entry = test_entry("Test");
        let id = entry.id.clone();
        store.write(WriteMemoryRequest {
            entry,
            generate_embedding: false,
        });
        assert!(store.delete(&id));
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_purge() {
        let mut store = MemoryStore::new();
        store.write(WriteMemoryRequest {
            entry: test_entry("A"),
            generate_embedding: false,
        });
        store.write(WriteMemoryRequest {
            entry: test_entry("B"),
            generate_embedding: false,
        });
        assert_eq!(store.purge(), 2);
        assert_eq!(store.count(), 0);
    }
}
