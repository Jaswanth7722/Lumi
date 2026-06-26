//! # Conversation System — Dialogue Management (Chapter 9)
//!
//! Manages conversation history, intent detection, summarization,
//! and streaming response processing.

use lumi_common::conversation::{
    ConversationMessage, DetectedIntent, MessageContent, MessageMetadata, MessageRole,
    SentenceDetector, TokenChunk,
};
use std::collections::VecDeque;
use tracing::{debug, info, warn};

/// Maximum number of conversation turns before summarization is triggered.
const MAX_HISTORY_TURNS: usize = 64;

/// The Conversation System manages dialogue history and streaming output.
pub struct ConversationSystem {
    /// Message history (most recent first).
    history: VecDeque<ConversationMessage>,
    /// Sentence detector for streaming response processing.
    sentence_detector: SentenceDetector,
    /// Token budget for conversation history.
    history_token_budget: u32,
    /// Current state of conversation.
    active: bool,
}

impl ConversationSystem {
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(MAX_HISTORY_TURNS),
            sentence_detector: SentenceDetector::new(10),
            history_token_budget: 4000,
            active: false,
        }
    }

    /// Receive a new message from the user.
    pub async fn receive_message(&mut self, text: &str) {
        debug!("Received message: {:.50}...", text);
        let msg = ConversationMessage::new_text(MessageRole::User, text);
        self.history.push_front(msg);
        self.active = true;

        // Check if summarization is needed
        if self.history.len() >= MAX_HISTORY_TURNS {
            self.summarize_history().await;
        }
    }

    /// Add an assistant message to the history.
    pub fn add_assistant_message(&mut self, text: &str) {
        let msg = ConversationMessage::new_text(MessageRole::Assistant, text);
        self.history.push_front(msg);
    }

    /// Get the conversation history for context building.
    pub fn get_history(&self) -> Vec<&ConversationMessage> {
        self.history.iter().collect()
    }

    /// Detect the intent of a user message by keyword matching.
    /// In production, this would use a lightweight local classifier.
    pub fn detect_intent(&self, text: &str) -> DetectedIntent {
        let lower = text.to_lowercase();

        // Task/command detection
        if lower.starts_with("create")
            || lower.starts_with("make")
            || lower.starts_with("build")
            || lower.starts_with("write")
            || lower.starts_with("delete")
            || lower.starts_with("install")
            || lower.contains("please")
                && (lower.contains("create") || lower.contains("write") || lower.contains("find"))
        {
            return DetectedIntent::TaskRequest;
        }

        // Memory queries
        if lower.contains("remember")
            || lower.starts_with("what do you know about")
            || lower.starts_with("do you remember")
        {
            return DetectedIntent::MemoryQuery;
        }

        // System commands
        if lower.starts_with("open ")
            || lower.starts_with("run ")
            || lower.starts_with("search for ")
            || lower.starts_with("play ")
        {
            return DetectedIntent::SystemCommand;
        }

        // Clarification questions
        if text.ends_with('?') && text.split_whitespace().count() <= 5 {
            return DetectedIntent::SimpleQuestion;
        }

        // Personal sharing
        if lower.contains("i am")
            || lower.contains("i'm")
            || lower.contains("my name")
            || lower.contains("i work")
            || lower.contains("i use")
        {
            return DetectedIntent::PersonalShare;
        }

        // Desktop-aware requests
        if lower.contains("what's on")
            || lower.contains("what am i doing")
            || lower.contains("current window")
            || lower.contains("active")
        {
            return DetectedIntent::DesktopAware;
        }

        // Default to simple question
        DetectedIntent::SimpleQuestion
    }

    /// Summarize older conversation history to free context budget.
    async fn summarize_history(&mut self) {
        info!(
            "Summarizing conversation history ({} turns)",
            self.history.len()
        );

        // Keep most recent 40%, summarize oldest 60%
        let keep_count = (self.history.len() as f32 * 0.4) as usize;
        let summarize_count = self.history.len() - keep_count;

        let to_summarize: Vec<String> = self
            .history
            .range(self.history.len() - summarize_count..)
            .map(|m| match &m.content[0] {
                MessageContent::Text { text } => text.clone(),
                _ => String::new(),
            })
            .collect();

        // Create a summary message
        let summary = format!(
            "[Summary of earlier conversation: {} turns about {} topics]",
            summarize_count,
            to_summarize.len().min(10)
        );

        // Remove old messages and add summary
        for _ in 0..summarize_count {
            self.history.pop_back();
        }

        let summary_msg = ConversationMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::System,
            content: vec![MessageContent::Text { text: summary }],
            timestamp: chrono::Utc::now().timestamp_millis(),
            token_count: 50,
            metadata: MessageMetadata {
                summarized: true,
                ..Default::default()
            },
        };
        self.history.push_back(summary_msg);

        info!("History summarized: {} turns remaining", self.history.len());
    }

    /// Process a streaming token from inference output.
    pub fn process_token(&mut self, token: &str) -> Option<String> {
        self.sentence_detector.detect(token)
    }

    /// Get remaining text in the sentence buffer.
    pub fn remaining_text(&self) -> &str {
        self.sentence_detector.remaining()
    }

    /// Check if the conversation is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// End the current conversation.
    pub fn end_conversation(&mut self) {
        self.active = false;
    }

    /// Get the number of messages in history.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_detection_task() {
        let system = ConversationSystem::new();
        assert_eq!(
            system.detect_intent("Create a new React project"),
            DetectedIntent::TaskRequest
        );
        assert_eq!(
            system.detect_intent("Build a dashboard UI"),
            DetectedIntent::TaskRequest
        );
    }

    #[test]
    fn test_intent_detection_memory() {
        let system = ConversationSystem::new();
        assert_eq!(
            system.detect_intent("Do you remember my name?"),
            DetectedIntent::MemoryQuery
        );
        assert_eq!(
            system.detect_intent("What do you know about TypeScript?"),
            DetectedIntent::MemoryQuery
        );
    }

    #[test]
    fn test_intent_detection_question() {
        let system = ConversationSystem::new();
        assert_eq!(
            system.detect_intent("What's the weather?"),
            DetectedIntent::SimpleQuestion
        );
    }

    #[test]
    fn test_intent_detection_personal() {
        let system = ConversationSystem::new();
        assert_eq!(
            system.detect_intent("I am a software engineer"),
            DetectedIntent::PersonalShare
        );
        assert_eq!(
            system.detect_intent("I use VSCode for coding"),
            DetectedIntent::PersonalShare
        );
    }

    #[test]
    fn test_message_history() {
        let mut system = ConversationSystem::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            system.receive_message("Hello Lumi").await;
            system.add_assistant_message("Hello! How can I help you today?");
            assert_eq!(system.history_len(), 2);
        });
    }

    #[test]
    fn test_sentence_detection() {
        let mut system = ConversationSystem::new();
        assert_eq!(system.process_token("Hello"), None);
        assert_eq!(
            system.process_token(" world."),
            Some("Hello world.".to_string())
        );
    }
}
