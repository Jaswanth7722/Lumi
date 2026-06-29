//! # AI Core — Inference and Output Types (Chapter 8)
//!
//! Defines the provider-agnostic inference interface, AI state signals,
//! output routing types, and provider selection logic.

use crate::memory::MemoryEntry;
use crate::plan::Plan;
use crate::state_machine::CharacterState;
use crate::tool::ToolDefinition;
use crate::workspace::WorkspaceCommand;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AI State Signals
// ---------------------------------------------------------------------------

/// AI processing state transmitted on the `ai.state` IPC channel.
/// Every state change produces a corresponding visual change on the character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AIState {
    /// No active AI processing.
    Idle,
    /// Receiving input from the user.
    ReceivingInput,
    /// LLM inference is in progress.
    Thinking,
    /// Planning Engine is constructing a task plan.
    Planning,
    /// A tool is being executed.
    ExecutingTool,
    /// Memory retrieval is in progress.
    RetrievingMemory,
    /// Response text is being generated.
    GeneratingResponse,
    /// TTS voice response is being played.
    Speaking,
    /// Waiting for voice input from the user.
    Listening,
    /// Waiting for user confirmation before proceeding.
    AwaitingConfirmation,
    /// An error has occurred.
    Error,
    /// A task completed successfully.
    Success,
}

/// An AI state event transmitted on the IPC bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIStateEvent {
    pub state: AIState,
    pub metadata: Option<serde_json::Value>,
    /// Expected duration in milliseconds for animation planning.
    pub duration_hint_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Inference Provider Abstraction
// ---------------------------------------------------------------------------

/// A single message in a conversation sent to an inference provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

/// The role of a message participant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A complete inference request to an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub system_prompt: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub context_budget: ContextBudget,
}

/// Budget allocations for fitting content into the provider's context window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    pub total_tokens: u32,
    pub reserved_output: u32,
    pub allocations: BudgetAllocations,
}

impl ContextBudget {
    /// How many tokens are available for conversation history after accounting
    /// for all other reserved allocations.
    pub fn history_available(&self) -> u32 {
        self.total_tokens
            .saturating_sub(self.reserved_output)
            .saturating_sub(self.allocations.system)
            .saturating_sub(self.allocations.memory)
            .saturating_sub(self.allocations.desktop)
            .saturating_sub(self.allocations.plan)
            .saturating_sub(self.allocations.tools)
    }
}

/// Per-category token budget allocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetAllocations {
    pub system: u32,
    pub memory: u32,
    pub desktop: u32,
    pub plan: u32,
    pub tools: u32,
    pub history: u32,
}

impl Default for BudgetAllocations {
    fn default() -> Self {
        Self {
            system: 800,
            memory: 1000,
            desktop: 300,
            plan: 500,
            tools: 2000,
            history: 4000,
        }
    }
}

/// Capabilities declared by an inference provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub max_context_tokens: u32,
    pub supports_tool_use: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub local: bool,
}

/// Latency profile for an inference provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyProfile {
    /// Estimated time to first token in milliseconds.
    pub ttft_ms: u32,
    /// Estimated tokens per second generation speed.
    pub tokens_per_second: f32,
}

/// The result of an inference call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
}

/// A tool call requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Reason the model stopped generating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinishReason {
    Stop,
    Length,
    ToolUse,
    Error,
}

/// Token usage statistics for an inference call.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

// ---------------------------------------------------------------------------
// Inference Mode and Provider Selection
// ---------------------------------------------------------------------------

/// The preferred inference execution mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum InferenceMode {
    /// Always use local inference (privacy mode).
    AlwaysLocal,
    /// Always use cloud inference.
    AlwaysCloud,
    /// Prefer local inference, fall back to cloud.
    #[default]
    PreferLocal,
    /// Prefer cloud inference, fall back to local.
    PreferCloud,
}

// ---------------------------------------------------------------------------
// AI Output Routing
// ---------------------------------------------------------------------------

/// Structured output types from the AI Core, routed to downstream subsystems.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AIOutput {
    /// A conversational response to the user.
    Conversation(ConversationResponse),
    /// A new task plan was created.
    PlanCreated(Plan),
    /// The AI is requesting a tool call.
    ToolCall(ToolCallRequest),
    /// A character state transition is requested.
    StateTransition(CharacterState),
    /// A memory entry should be persisted.
    MemoryWrite(MemoryEntry),
    /// A command to the workspace panel system.
    WorkspaceCommand(WorkspaceCommand),
    /// The AI needs clarification from the user.
    ClarificationRequest(ClarificationRequest),
}

/// A conversational response routed to the Conversation System.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationResponse {
    pub text: String,
    pub tool_results: Vec<ToolCallResult>,
}

/// Result of a completed tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub tool_call_id: String,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

/// A requested tool call from the AI Core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub tool_call_id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub requires_approval: bool,
}

/// A request for user clarification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationRequest {
    pub question: String,
    pub options: Vec<String>,
    pub context: Option<String>,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            total_tokens: 8192,
            reserved_output: 1024,
            allocations: BudgetAllocations::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_budget_history_available() {
        let budget = ContextBudget::default();
        let expected = 8192u32
            .saturating_sub(1024)
            .saturating_sub(800)
            .saturating_sub(1000)
            .saturating_sub(300)
            .saturating_sub(500)
            .saturating_sub(2000);
        assert_eq!(budget.history_available(), expected);
    }

    #[test]
    fn test_ai_state_serialization() {
        let event = AIStateEvent {
            state: AIState::Thinking,
            metadata: Some(serde_json::json!({"model": "claude-3"})),
            duration_hint_ms: Some(2000),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["state"], "Thinking");
        assert_eq!(json["duration_hint_ms"], 2000);
    }
}
