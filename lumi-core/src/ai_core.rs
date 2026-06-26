//! # AI Core — Inference Orchestration (Chapter 8)
//!
//! Manages the inference pipeline: context building, provider selection,
//! output routing, and AI state emission.

use lumi_common::ai::{
    AIOutput, AIState, AIStateEvent, ContextBudget, InferenceMode, InferenceRequest,
    InferenceResponse, LatencyProfile, ProviderCapabilities, ToolCall,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// The central AI Core orchestrating all inference activity.
pub struct AICore {
    /// Preferred inference execution mode.
    pub inference_mode: InferenceMode,
    /// Registered inference providers.
    providers: Vec<Box<dyn InferenceProvider + Send + Sync>>,
    /// Whether the network is available for cloud inference.
    network_available: bool,
    /// Current AI processing state.
    pub current_state: AIState,
    /// Context budget configuration.
    context_budget: ContextBudget,
}

impl AICore {
    pub fn new() -> Self {
        Self {
            inference_mode: InferenceMode::PreferLocal,
            providers: Vec::new(),
            network_available: false,
            current_state: AIState::Idle,
            context_budget: ContextBudget::default(),
        }
    }

    /// Initialize the AI Core with default providers.
    pub async fn initialize(&mut self) {
        info!("AI Core initializing with mode: {:?}", self.inference_mode);
        self.current_state = AIState::Idle;
    }

    /// Set the current AI state and return the corresponding event.
    pub fn set_state(&mut self, state: AIState) -> AIStateEvent {
        self.current_state = state.clone();
        AIStateEvent {
            state,
            metadata: None,
            duration_hint_ms: None,
        }
    }

    /// Build an inference request from conversation context.
    pub fn build_request(
        &self,
        system_prompt: &str,
        messages: &[String],
        tools: &[ToolDefinition],
    ) -> InferenceRequest {
        InferenceRequest {
            system_prompt: system_prompt.to_string(),
            messages: messages
                .iter()
                .map(|m| lumi_common::ai::Message {
                    role: lumi_common::ai::MessageRole::User,
                    content: m.clone(),
                })
                .collect(),
            tools: tools.to_vec(),
            max_tokens: 2048,
            temperature: 0.7,
            context_budget: self.context_budget.clone(),
        }
    }

    /// Route an AI output to the appropriate subsystem.
    pub fn route_output(&self, output: AIOutput) {
        match output {
            AIOutput::Conversation(response) => {
                debug!("Routing conversation response: {} chars", response.text.len());
            }
            AIOutput::PlanCreated(plan) => {
                info!("Plan created: {} with {} steps", plan.title, plan.steps.len());
            }
            AIOutput::ToolCall(request) => {
                debug!("Tool call requested: {}", request.name);
            }
            AIOutput::StateTransition(_) => {
                debug!("State transition requested");
            }
            AIOutput::MemoryWrite(entry) => {
                debug!("Memory write: {}", entry.content);
            }
            AIOutput::WorkspaceCommand(cmd) => {
                debug!("Workspace command: {:?}", cmd);
            }
            AIOutput::ClarificationRequest(req) => {
                debug!("Clarification needed: {}", req.question);
            }
        }
    }

    /// Get the context budget.
    pub fn context_budget(&self) -> &ContextBudget {
        &self.context_budget
    }
}

use lumi_common::tool::ToolDefinition;

/// Provider-agnostic inference interface.
/// Uses a manual vtable approach rather than async_trait for dyn compatibility.
pub trait InferenceProvider: Send + Sync {
    fn complete(&self, request: &InferenceRequest) -> anyhow::Result<InferenceResponse>;
    fn capabilities(&self) -> ProviderCapabilities;
    fn latency_profile(&self) -> LatencyProfile;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_core_default_state() {
        let core = AICore::new();
        assert_eq!(core.current_state, AIState::Idle);
        assert_eq!(core.inference_mode, InferenceMode::PreferLocal);
    }

    #[test]
    fn test_state_transition_event() {
        let mut core = AICore::new();
        let event = core.set_state(AIState::Thinking);
        assert_eq!(event.state, AIState::Thinking);
        assert_eq!(core.current_state, AIState::Thinking);
    }
}
