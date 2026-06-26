//! # Conversation System — Message and Intent Types (Chapter 9)
//!
//! Defines conversation message structures, intent detection, and
//! the streaming response processor pipeline.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Message Types
// ---------------------------------------------------------------------------

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: Vec<MessageContent>,
    pub timestamp: i64,
    pub token_count: u32,
    pub metadata: MessageMetadata,
}

/// Participant role in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Content types within a single message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, media_type: String },
    #[serde(rename = "file")]
    File { name: String, content: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String, content: serde_json::Value },
}

/// Metadata attached to each conversation message.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    pub voice_input: bool,
    pub contains_pii: bool,
    pub summarized: bool,
    pub memory_written: bool,
}

// ---------------------------------------------------------------------------
// Intent Detection
// ---------------------------------------------------------------------------

/// Lightweight intent classifications for routing user input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectedIntent {
    /// Direct inference query, no tools needed.
    SimpleQuestion,
    /// Multi-step task requiring the Planning Engine.
    TaskRequest,
    /// Query about stored memories.
    MemoryQuery,
    /// OS or application automation command.
    SystemCommand,
    /// Personal information sharing (empathy + optional memory write).
    PersonalShare,
    /// Clarify ambiguity in the request.
    Clarification,
    /// Request that needs desktop context injected first.
    DesktopAware,
    /// Voice-only interaction requiring TTS-optimized response.
    VoiceOnly,
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

/// A single token chunk during streaming inference output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenChunk {
    pub token: String,
}

/// A complete sentence chunk dispatched to the TTS system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceChunk {
    pub text: String,
    pub is_final: bool,
}

/// Detects sentence boundaries in streaming text output.
#[derive(Debug, Default)]
pub struct SentenceDetector {
    buffer: String,
    min_sentence_length: usize,
}

impl SentenceDetector {
    pub fn new(min_sentence_length: usize) -> Self {
        Self {
            buffer: String::new(),
            min_sentence_length,
        }
    }

    /// Process a token and return a completed sentence if a boundary is detected.
    pub fn detect(&mut self, text: &str) -> Option<String> {
        self.buffer.push_str(text);

        if self.buffer.len() < self.min_sentence_length {
            return None;
        }

        // Detect sentence boundaries: . ! ? followed by space or end
        for (i, ch) in self.buffer.char_indices().rev() {
            if matches!(ch, '.' | '!' | '?') {
                let end = i + ch.len_utf8();
                let (sentence, rest) = self.buffer.split_at(end);
                let sentence = sentence.to_string();
                self.buffer = rest.trim_start().to_string();
                return Some(sentence);
            }
        }
        None
    }

    /// Get the remaining text buffer.
    pub fn remaining(&self) -> &str {
        &self.buffer
    }

    /// Reset the buffer (e.g., after response completion).
    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

// ---------------------------------------------------------------------------
// Message Utilities
// ---------------------------------------------------------------------------

impl ConversationMessage {
    /// Create a new user message with text content.
    pub fn new_text(role: MessageRole, text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            content: vec![MessageContent::Text { text: text.into() }],
            timestamp: chrono::Utc::now().timestamp_millis(),
            token_count: 0,
            metadata: MessageMetadata::default(),
        }
    }

    /// Estimate token count from text (rough: ~4 chars per token).
    pub fn estimate_tokens(text: &str) -> u32 {
        (text.len() / 4).max(1) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentence_detector() {
        let mut detector = SentenceDetector::new(5);

        // No sentence boundary yet
        assert_eq!(detector.detect("Hello"), None);

        // Sentence boundary detected
        let result = detector.detect(" world.");
        assert_eq!(result, Some("Hello world.".to_string()));

        // Buffer should contain remaining text after the sentence
        assert!(detector.remaining().is_empty());
    }

    #[test]
    fn test_message_creation() {
        let msg = ConversationMessage::new_text(MessageRole::User, "Hello Lumi");
        assert_eq!(msg.role, MessageRole::User);
        assert!(!msg.id.is_empty());
        assert_eq!(msg.content.len(), 1);
    }

    #[test]
    fn test_token_estimate() {
        let tokens = ConversationMessage::estimate_tokens("Hello, how are you today?");
        assert!(tokens > 0);
    }
}
