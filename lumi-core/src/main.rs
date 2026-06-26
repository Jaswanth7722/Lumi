//! # Lumi Core Process
//!
//! The central intelligence subsystem of the Lumi platform.
//! Manages AI inference, conversation, planning, memory, desktop awareness,
//! the emotion system, and the state machine.

use lumi_common::ipc::{Channel, LumiMessage, ProcessId};
use lumi_common::ai::{AIState, AIStateEvent};
use lumi_common::character::CrystalState;
use lumi_common::emotion::{EmotionState, emotion_mapping_for_ai_state};
use lumi_common::state_machine::{default_transition_rules};
use lumi_ipc::MessageBus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

mod ai_core;
mod conversation;
mod planning;
mod tool_framework;
mod memory;
mod desktop_awareness;
mod emotion;
mod state_machine;
mod input_system;
mod security;
mod privacy;
mod performance;
mod logging;
mod update;
mod audio;

use ai_core::AICore;
use conversation::ConversationSystem;
use planning::PlanningEngine;
use tool_framework::ToolFramework;
use memory::MemorySystem;
use desktop_awareness::DesktopAwareness;
use emotion::EmotionSystem;

/// Shared application state for the core process.
/// MessageBus is internally thread-safe (uses DashMap + mpsc channels),
/// so it doesn't need an RwLock wrapper.
pub struct CoreState {
    /// The IPC message bus (thread-safe internally).
    pub bus: Arc<MessageBus>,
    /// AI Core — inference orchestration.
    pub ai_core: Arc<RwLock<AICore>>,
    /// Conversation System — dialogue management.
    pub conversation: Arc<RwLock<ConversationSystem>>,
    /// Planning Engine — task decomposition and execution.
    pub planning: Arc<RwLock<PlanningEngine>>,
    /// Tool Framework — tool registration and invocation.
    pub tools: Arc<RwLock<ToolFramework>>,
    /// Memory System — persistent memory storage and retrieval.
    pub memory: Arc<RwLock<MemorySystem>>,
    /// Desktop Awareness — desktop context monitoring.
    pub desktop: Arc<RwLock<DesktopAwareness>>,
    /// Emotion System — emotional state and sentiment analysis.
    pub emotion: Arc<RwLock<EmotionSystem>>,
    /// State Machine — behavioral state coordinator.
    pub state_machine: Arc<RwLock<state_machine::StateMachine>>,
    /// Whether the core process is running.
    pub running: Arc<AtomicBool>,
}

impl CoreState {
    pub fn new(bus: MessageBus) -> Self {
        Self {
            bus: Arc::new(bus),
            ai_core: Arc::new(RwLock::new(AICore::new())),
            conversation: Arc::new(RwLock::new(ConversationSystem::new())),
            planning: Arc::new(RwLock::new(PlanningEngine::new())),
            tools: Arc::new(RwLock::new(ToolFramework::new())),
            memory: Arc::new(RwLock::new(MemorySystem::new())),
            desktop: Arc::new(RwLock::new(DesktopAwareness::new())),
            emotion: Arc::new(RwLock::new(EmotionSystem::new())),
            state_machine: Arc::new(RwLock::new(state_machine::StateMachine::new(default_transition_rules()))),
            running: Arc::new(AtomicBool::new(true)),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumi_core=info,lumi_ipc=info".into()),
        )
        .init();

    info!("Starting Lumi Core Process...");

    // Initialize IPC bus
    let bus = MessageBus::new(ProcessId::Core);
    let state = CoreState::new(bus);

    // Subscribe to IPC channels
    let mut render_input_rx = state.bus.subscribe(Channel::RenderInput);
    let mut voice_input_rx = state.bus.subscribe(Channel::VoiceInput);
    let mut plugin_capability_rx = state.bus.subscribe(Channel::PluginCapability);

    // Initialize all subsystems
    state.initialize().await;

    info!("Lumi Core Process running");

    // Main event loop (no RwLock on bus — it's internally thread-safe)
    loop {
        tokio::select! {
            Ok(msg) = render_input_rx.recv() => {
                state.handle_render_input(msg).await;
            }
            Ok(msg) = voice_input_rx.recv() => {
                state.handle_voice_input(msg).await;
            }
            Ok(msg) = plugin_capability_rx.recv() => {
                state.handle_plugin_capability(msg).await;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {
                state.desktop.write().await.update_snapshot().await;
            }
        }

        if !state.running.load(Ordering::Relaxed) {
            break;
        }
    }

    info!("Lumi Core Process shut down");
    Ok(())
}

impl CoreState {
    /// Initialize all subsystems.
    async fn initialize(&self) {
        info!("Initializing AI Core...");
        self.ai_core.write().await.initialize().await;

        info!("Initializing Memory System...");
        self.memory.write().await.initialize().await;

        info!("Initializing Tool Framework...");
        self.tools.write().await.register_builtin_tools();

        info!("Initializing State Machine...");
        let mut sm = self.state_machine.write().await;
        sm.initialize();

        info!("Core subsystems initialized");
    }

    /// Handle user interaction input from the render process.
    async fn handle_render_input(&self, msg: LumiMessage) {
        debug!("Render input: {:?}", msg.payload);
    }

    /// Handle transcribed voice input from the voice process.
    async fn handle_voice_input(&self, msg: LumiMessage) {
        debug!("Voice input: {:?}", msg.payload);
        if let Some(text) = msg.payload.get("transcript").and_then(|t| t.as_str()) {
            let mut conv = self.conversation.write().await;
            conv.receive_message(text).await;
        }
    }

    /// Handle capability registrations from plugins.
    async fn handle_plugin_capability(&self, msg: LumiMessage) {
        debug!("Plugin capability: {:?}", msg.payload);
    }
}
