//! # Lumi Core Process
//!
//! The central intelligence subsystem of the Lumi platform.
//! Manages AI inference, conversation, planning, memory, desktop awareness,
//! the emotion system, state machine, input system, security, privacy,
//! performance monitoring, logging, updates, and audio output.

use lumi_common::ipc::{Channel, LumiMessage, MessageType, ProcessId};
use lumi_common::ai::{AIState, AIStateEvent};
use lumi_common::character::CrystalState;
use lumi_common::emotion::{EmotionState, SentimentSignal, emotion_mapping_for_ai_state};
use lumi_common::state_machine::{default_transition_rules, StateEvent, StateCommand};
use lumi_common::input::{InputEvent, InputEventType, HotkeyAction, MouseButton};
use lumi_common::logging::{AuditEntry, AuditEventType, AuditOutcome, LogLevel};
use lumi_common::privacy::PIIAction;
use lumi_common::performance::FramePacerConfig;
use lumi_ipc::MessageBus;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// -- Module declarations for all subsystems --
mod ai_core;
mod conversation;
mod planning;
mod tool_framework;
mod memory;
mod desktop_awareness;
mod emotion;
mod state_machine;

// Volume III subsystem modules
mod input_system;
mod security;
mod privacy;
mod performance;
mod logging;
mod update;
mod audio;

// -- Use declarations for all subsystems --
use ai_core::AICore;
use conversation::ConversationSystem;
use planning::PlanningEngine;
use tool_framework::ToolFramework;
use memory::MemorySystem;
use desktop_awareness::DesktopAwareness;
use emotion::EmotionSystem;

use input_system::InputSystem;
use security::SecurityManager;
use privacy::PrivacyManager;
use performance::{FramePacer, ResponseCache};
use logging::LoggingManager;
use update::UpdateSystem;
use audio::AudioEngine;

/// Shared application state for the core process.
/// All subsystems are wrapped in Arc<RwLock<...>> for concurrent access
/// from the async event loop handlers.
pub struct CoreState {
    /// IPC message bus (thread-safe internally via DashMap + mpsc).
    pub bus: Arc<MessageBus>,

    // === Core subsystems (Volumes I–II) ===
    pub ai_core: Arc<RwLock<AICore>>,
    pub conversation: Arc<RwLock<ConversationSystem>>,
    pub planning: Arc<RwLock<PlanningEngine>>,
    pub tools: Arc<RwLock<ToolFramework>>,
    pub memory: Arc<RwLock<MemorySystem>>,
    pub desktop: Arc<RwLock<DesktopAwareness>>,
    pub emotion: Arc<RwLock<EmotionSystem>>,
    pub state_machine: Arc<RwLock<state_machine::StateMachine>>,

    // === Volume III subsystems ===
    /// Input System — click, drag, hotkey handling.
    pub input: Arc<RwLock<InputSystem>>,
    /// Security Manager — approval gates, secret store.
    pub security: Arc<RwLock<SecurityManager>>,
    /// Privacy Manager — PII detection, feature toggles, data inventory.
    pub privacy: Arc<RwLock<PrivacyManager>>,
    /// Frame pacer config reference (actual pacing happens in render process).
    pub frame_pacer: Arc<RwLock<FramePacer>>,
    /// Response cache — frequently repeated AI query caching.
    pub response_cache: Arc<RwLock<ResponseCache>>,
    /// Logging Manager — structured logs, audit log, telemetry.
    pub logging: Arc<RwLock<LoggingManager>>,
    /// Update System — version management, update checking.
    pub update: Arc<RwLock<UpdateSystem>>,
    /// Audio Engine — TTS playback, sound effects, mute/volume.
    pub audio: Arc<RwLock<AudioEngine>>,

    /// Whether the core process is running.
    pub running: Arc<AtomicBool>,
}

impl CoreState {
    pub fn new(bus: MessageBus) -> Self {
        Self {
            bus: Arc::new(bus),

            // Core subsystems
            ai_core: Arc::new(RwLock::new(AICore::new())),
            conversation: Arc::new(RwLock::new(ConversationSystem::new())),
            planning: Arc::new(RwLock::new(PlanningEngine::new())),
            tools: Arc::new(RwLock::new(ToolFramework::new())),
            memory: Arc::new(RwLock::new(MemorySystem::new())),
            desktop: Arc::new(RwLock::new(DesktopAwareness::new())),
            emotion: Arc::new(RwLock::new(EmotionSystem::new())),
            state_machine: Arc::new(RwLock::new(
                state_machine::StateMachine::new(default_transition_rules())
            )),

            // Volume III subsystems
            input: Arc::new(RwLock::new(InputSystem::new())),
            security: Arc::new(RwLock::new(SecurityManager::new())),
            privacy: Arc::new(RwLock::new(PrivacyManager::new())),
            frame_pacer: Arc::new(RwLock::new(FramePacer::new(FramePacerConfig::default()))),
            response_cache: Arc::new(RwLock::new(ResponseCache::new(
                lumi_common::performance::ResponseCacheConfig::default()
            ))),
            logging: Arc::new(RwLock::new(LoggingManager::new("core"))),
            update: Arc::new(RwLock::new(UpdateSystem::new())),
            audio: Arc::new(RwLock::new(AudioEngine::new())),

            running: Arc::new(AtomicBool::new(true)),
        }
    }
}

// =========================================================================
// Main entry point
// =========================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    // Subscribe to all IPC channels the core process handles
    let mut render_input_rx    = state.bus.subscribe(Channel::RenderInput);
    let mut voice_input_rx     = state.bus.subscribe(Channel::VoiceInput);
    let mut voice_output_rx    = state.bus.subscribe(Channel::VoiceOutput);
    let mut plugin_capability_rx = state.bus.subscribe(Channel::PluginCapability);
    let mut ai_command_rx      = state.bus.subscribe(Channel::AiCommand);
    let mut desktop_event_rx   = state.bus.subscribe(Channel::DesktopEvent);
    let mut config_operation_rx = state.bus.subscribe(Channel::ConfigOperation);

    // Initialize all subsystems
    state.initialize().await;

    info!("Lumi Core Process running");

    // =====================================================================
    // Main event loop — processes IPC messages from all channels
    // =====================================================================
    loop {
        tokio::select! {
            // -- User interactions from the render process --
            Ok(msg) = render_input_rx.recv() => {
                state.handle_render_input(msg).await;
            }

            // -- Transcribed speech from the voice process --
            Ok(msg) = voice_input_rx.recv() => {
                state.handle_voice_input(msg).await;
            }

            // -- TTS audio data from the voice process --
            Ok(msg) = voice_output_rx.recv() => {
                state.handle_voice_output(msg).await;
            }

            // -- Plugin capability registrations --
            Ok(msg) = plugin_capability_rx.recv() => {
                state.handle_plugin_capability(msg).await;
            }

            // -- Internal AI orchestration commands --
            Ok(msg) = ai_command_rx.recv() => {
                state.handle_ai_command(msg).await;
            }

            // -- Desktop environment events --
            Ok(msg) = desktop_event_rx.recv() => {
                state.handle_desktop_event(msg).await;
            }

            // -- Configuration read/write requests --
            Ok(msg) = config_operation_rx.recv() => {
                state.handle_config_operation(msg).await;
            }

            // -- Periodic ticks (every 50ms) --
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {
                state.on_tick().await;
            }
        }

        if !state.running.load(Ordering::Relaxed) {
            break;
        }
    }

    info!("Lumi Core Process shut down");
    Ok(())
}

// =========================================================================
// CoreState implementation
// =========================================================================

impl CoreState {
    /// Initialize all subsystems in dependency order.
    async fn initialize(&self) {
        info!("=== Initializing Core Subsystems ===");

        info!("[1/9] Initializing AI Core...");
        self.ai_core.write().await.initialize().await;

        info!("[2/9] Initializing Memory System...");
        self.memory.write().await.initialize().await;

        info!("[3/9] Initializing Tool Framework...");
        self.tools.write().await.register_builtin_tools();

        info!("[4/9] Initializing State Machine...");
        self.state_machine.write().await.initialize();

        info!("[5/9] Initializing Security Manager...");
        // Security is ready immediately

        info!("[6/9] Initializing Privacy Manager...");
        // Privacy is ready immediately

        info!("[7/9] Initializing Logging Manager...");
        self.logging.write().await.log(LogLevel::Info, "core", "Logging initialized");

        info!("[8/9] Initializing Update System...");
        // Update checker starts in background

        info!("[9/9] Initializing Audio Engine...");
        // Audio engine is ready immediately

        info!("=== All core subsystems initialized ===");

        // Emit startup event to state machine
        self.state_machine.write().await.handle_event(StateEvent::StartupComplete);

        // Log audit entry for startup
        self.logging.write().await.audit(
            AuditEntry::tool_executed("core.startup", AuditOutcome::Success, None),
        );
    }

    // -----------------------------------------------------------------
    // Periodic tick handler (called every 50ms)
    // -----------------------------------------------------------------
    async fn on_tick(&self) {
        // 1. Update desktop snapshot
        self.desktop.write().await.update_snapshot().await;

        // 2. Check focus mode — route to audio/emotion
        let focus_mode = self.desktop.read().await.is_focus_mode();
        if focus_mode {
            self.audio.write().await.set_focus_mode(true);
        } else {
            self.audio.write().await.set_focus_mode(false);
        }

        // 3. Route desktop idle events to state machine
        let idle_seconds = self.desktop.read().await.idle_seconds();
        if idle_seconds > 0 && idle_seconds % 300 == 0 {
            // Every 5 minutes of idle, emit idle event
            self.state_machine.write().await.handle_event(
                StateEvent::UserIdle { seconds: idle_seconds }
            );
        }

        // 4. Drain state machine commands and route them
        let commands = self.state_machine.write().await.drain_commands();
        for cmd in &commands {
            self.route_state_command(cmd).await;
        }
    }

    // -----------------------------------------------------------------
    // Route state machine commands to downstream subsystems
    // -----------------------------------------------------------------
    async fn route_state_command(&self, cmd: &StateCommand) {
        match cmd {
            StateCommand::PlayAnimation { clip, blend } => {
                debug!("Animation command: {:?} ({:?})", clip, blend);
                // Route to render process via IPC
                let msg = LumiMessage::new_event(
                    ProcessId::Core,
                    Channel::RenderCommand,
                    serde_json::json!({
                        "command": "play_animation",
                        "clip": clip,
                        "blend": blend,
                    }),
                );
                if let Ok(m) = msg {
                    let _ = self.bus.send(m).await;
                }
            }
            StateCommand::SetCrystalState(state) => {
                debug!("Crystal state: {:?}", state);
                // Update emotion system
                self.emotion.write().await.update_from_ai_state(&AIState::Thinking);
            }
            StateCommand::ShowWorkspacePanel(panel_type) => {
                debug!("Show panel: {:?}", panel_type);
                let msg = LumiMessage::new_event(
                    ProcessId::Core,
                    Channel::RenderCommand,
                    serde_json::json!({
                        "command": "show_panel",
                        "panel_type": panel_type,
                    }),
                );
                if let Ok(m) = msg {
                    let _ = self.bus.send(m).await;
                }
            }
            StateCommand::HideWorkspacePanel(panel_id) => {
                let msg = LumiMessage::new_event(
                    ProcessId::Core,
                    Channel::RenderCommand,
                    serde_json::json!({
                        "command": "hide_panel",
                        "panel_id": panel_id,
                    }),
                );
                if let Ok(m) = msg {
                    let _ = self.bus.send(m).await;
                }
            }
            StateCommand::PlaySound(sound) => {
                debug!("Play sound: {}", sound);
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------
    // IPC Message Handlers
    // -----------------------------------------------------------------

    /// Handle user interaction input from the render process.
    async fn handle_render_input(&self, msg: LumiMessage) {
        // Parse the input event from the message payload
        let input_event: Option<InputEvent> = serde_json::from_value(msg.payload.clone()).ok();

        if let Some(event) = input_event {
            match event.event_type {
                InputEventType::CharacterClick => {
                    let x = event.screen_position.map(|p| p.x as i32).unwrap_or(0);
                    let y = event.screen_position.map(|p| p.y as i32).unwrap_or(0);
                    let state_event = self.input.write().await.handle_click(x, y, MouseButton::Left);
                    if let Some(se) = state_event {
                        self.state_machine.write().await.handle_event(se);
                    }
                }
                InputEventType::CharacterDragStart => {
                    let x = event.screen_position.map(|p| p.x).unwrap_or(0.0);
                    let y = event.screen_position.map(|p| p.y).unwrap_or(0.0);
                    if self.input.write().await.handle_drag_start(x, y) {
                        // Emit drag start event to state machine
                        self.state_machine.write().await.handle_event(
                            StateEvent::UserDrag { new_position: (x, y) }
                        );
                    }
                }
                InputEventType::CharacterDragEnd => {
                    if let Some(target) = self.input.write().await.handle_drag_end() {
                        // Send position update to render process
                        let msg = LumiMessage::new_event(
                            ProcessId::Core,
                            Channel::RenderCommand,
                            serde_json::json!({
                                "command": "move_to",
                                "target": target,
                            }),
                        );
                        if let Ok(m) = msg {
                            let _ = self.bus.send(m).await;
                        }
                    }
                }
                InputEventType::TextSubmitted { ref text } => {
                    let state_event = self.input.read().await.handle_text_input(text);
                    self.state_machine.write().await.handle_event(state_event.clone());

                    // Route to conversation system
                    if let StateEvent::UserInput { ref content, .. } = state_event {
                        self.conversation.write().await.receive_message(content).await;
                    }
                }
                InputEventType::Hotkey { ref keys } => {
                    if let Some(action) = self.input.read().await.handle_hotkey(keys) {
                        self.handle_hotkey_action(action).await;
                    }
                }
                _ => {
                    debug!("Unhandled input event: {:?}", event.event_type);
                }
            }
        } else {
            debug!("Render input (raw): {:?}", msg.payload);
        }
    }

    /// Handle hotkey actions.
    async fn handle_hotkey_action(&self, action: HotkeyAction) {
        match action {
            HotkeyAction::ToggleConversation => {
                self.state_machine.write().await.handle_event(
                    StateEvent::UserInput {
                        source: lumi_common::state_machine::InputSource::Keyboard,
                        content: String::new(),
                    },
                );
            }
            HotkeyAction::ToggleVoice => {
                self.state_machine.write().await.handle_event(
                    StateEvent::WakeWord { confidence: 1.0 },
                );
            }
            HotkeyAction::ToggleVisibility => {
                let msg = LumiMessage::new_event(
                    ProcessId::Core,
                    Channel::RenderCommand,
                    serde_json::json!({ "command": "toggle_visibility" }),
                );
                if let Ok(m) = msg {
                    let _ = self.bus.send(m).await;
                }
            }
            HotkeyAction::ToggleFocusMode => {
                // Toggle focus mode via desktop awareness
                let mut desktop = self.desktop.write().await;
                if desktop.is_focus_mode() {
                    self.state_machine.write().await.handle_event(StateEvent::FocusEnded);
                    self.audio.write().await.set_focus_mode(false);
                } else {
                    self.state_machine.write().await.handle_event(StateEvent::FocusDetected);
                    self.audio.write().await.set_focus_mode(true);
                }
            }
            HotkeyAction::DismissPanel => {
                let msg = LumiMessage::new_event(
                    ProcessId::Core,
                    Channel::RenderCommand,
                    serde_json::json!({ "command": "dismiss_panel" }),
                );
                if let Ok(m) = msg {
                    let _ = self.bus.send(m).await;
                }
            }
            HotkeyAction::CancelVoice => {
                debug!("Voice cancelled");
            }
        }
    }

    /// Handle transcribed voice input from the voice process.
    async fn handle_voice_input(&self, msg: LumiMessage) {
        debug!("Voice input received");

        // Screen for PII before processing
        let transcript = msg.payload.get("transcript").and_then(|t| t.as_str()).map(|s| s.to_string());

        if let Some(ref text) = transcript {
            // Privacy check — apply PII detection before processing
            let processed_text = if let Some(action) = self.privacy.read().await.screen_content(text) {
                match action {
                    PIIAction::Block => {
                        warn!("Voice input blocked by PII detector");
                        return;
                    }
                    PIIAction::Redact { placeholder } => {
                        debug!("Voice input contains PII, redacting");
                        placeholder
                    }
                    PIIAction::Warn { message } => {
                        warn!("Voice input may contain sensitive data: {}", message);
                        text.clone()
                    }
                }
            } else {
                text.clone()
            };

            // Route to conversation system
            let mut conv = self.conversation.write().await;
            conv.receive_message(&processed_text).await;

            // Emit speech end event — triggers AI processing
            self.state_machine.write().await.handle_event(
                StateEvent::SpeechEnd { transcript: processed_text },
            );

            self.logging.write().await.log(LogLevel::Info, "voice", "Voice input processed");
        }

        // Handle wake word activation (separate from transcription)
        if let Some(confidence) = msg.payload.get("confidence").and_then(|c| c.as_f64()) {
            let state_event = self.input.read().await.handle_voice_activation(confidence as f32);
            self.state_machine.write().await.handle_event(state_event);
            debug!("Wake word detected (confidence: {:.2})", confidence);
        }
    }

    /// Handle TTS audio output from the voice process.
    async fn handle_voice_output(&self, msg: LumiMessage) {
        debug!("Voice output (TTS audio) received");

        // Forward audio to the audio engine for playback
        self.audio.write().await.play_tts();

        // Log telemetry
        let mut props = HashMap::new();
        if let Some(duration_ms) = msg.payload.get("lip_sync").and_then(|l| l.get("duration_ms")).and_then(|d| d.as_u64()) {
            props.insert("duration_ms".into(), duration_ms.to_string());
        }
        self.logging.write().await.record_telemetry("tts_playback", props);
    }

    /// Handle capability registrations from plugins.
    async fn handle_plugin_capability(&self, msg: LumiMessage) {
        debug!("Plugin capability registration: {:?}", msg.payload);

        if let Some(plugin_name) = msg.payload.get("plugin").and_then(|p| p.as_str()) {
            if let Some(caps) = msg.payload.get("capabilities").and_then(|c| c.as_array()) {
                for cap_val in caps {
                    if let Some(cap_str) = cap_val.as_str() {
                        // Parse capability string to Capability enum
                        let capability = serde_json::from_value(serde_json::json!(cap_str)).ok();
                        if let Some(cap) = capability {
                            self.tools.write().await.grant_capability(cap);
                        }
                    }
                }

                // Log audit entry
                self.logging.write().await.audit(
                    AuditEntry::tool_executed(
                        "plugin.register",
                        AuditOutcome::Success,
                        Some(true),
                    ),
                );
            }
        }
    }

    /// Handle internal AI orchestration commands.
    async fn handle_ai_command(&self, msg: LumiMessage) {
        debug!("AI command: {:?}", msg.payload);

        if let Some(command) = msg.payload.get("command").and_then(|c| c.as_str()) {
            match command {
                "set_ai_state" => {
                    if let Some(state_val) = msg.payload.get("state") {
                        if let Ok(ai_state) = serde_json::from_value::<AIState>(state_val.clone()) {
                            // Route to emotion system
                            self.emotion.write().await.update_from_ai_state(&ai_state);

                            // Route to state machine
                            self.state_machine.write().await.handle_event(
                                StateEvent::AIStateChanged(ai_state),
                            );
                        }
                    }
                }
                "check_approval" => {
                    if let Some(tool_name) = msg.payload.get("tool").and_then(|t| t.as_str()) {
                        let action = self.security.read().await.check_approval(tool_name);
                        // Send response back
                        let response = LumiMessage::new_response(
                            &msg,
                            serde_json::json!({ "approval": format!("{:?}", action) }),
                        );
                        if let Ok(r) = response {
                            let _ = self.bus.send(r).await;
                        }
                    }
                }
                "cache_response" => {
                    if let Some(query) = msg.payload.get("query").and_then(|q| q.as_str()) {
                        if let Some(response) = msg.payload.get("response").and_then(|r| r.as_str()) {
                            self.response_cache.write().await.put(query, response.to_string());
                        }
                    }
                }
                _ => {
                    debug!("Unknown AI command: {}", command);
                }
            }
        }
    }

    /// Handle desktop environment events.
    async fn handle_desktop_event(&self, msg: LumiMessage) {
        debug!("Desktop event: {:?}", msg.payload);

        if let Some(event_type) = msg.payload.get("event").and_then(|e| e.as_str()) {
            match event_type {
                "focus_detected" => {
                    self.state_machine.write().await.handle_event(StateEvent::FocusDetected);
                    self.audio.write().await.set_focus_mode(true);
                    // Enter calm state during focus mode
                    self.emotion.write().await.transition_to(
                        lumi_common::emotion::Emotion::Calm, 0.3
                    );
                }
                "focus_ended" => {
                    self.state_machine.write().await.handle_event(StateEvent::FocusEnded);
                    self.audio.write().await.set_focus_mode(false);
                }
                "user_active" => {
                    self.state_machine.write().await.handle_event(StateEvent::UserActive);
                    self.desktop.write().await.register_input(
                        lumi_common::desktop::InputType::Keyboard,
                    );
                }
                "user_idle" => {
                    if let Some(seconds) = msg.payload.get("seconds").and_then(|s| s.as_u64()) {
                        self.state_machine.write().await.handle_event(
                            StateEvent::UserIdle { seconds },
                        );
                    }
                }
                _ => {
                    debug!("Unknown desktop event: {}", event_type);
                }
            }
        }
    }

    /// Handle configuration read/write requests.
    async fn handle_config_operation(&self, msg: LumiMessage) {
        debug!("Config operation: {:?}", msg.payload);

        if msg.msg_type == MessageType::Request {
            // Read config
            if let Some(key) = msg.payload.get("key").and_then(|k| k.as_str()) {
                // Check privacy gate for sensitive config
                let is_sensitive = key.starts_with("privacy.") || key.starts_with("security.");
                if is_sensitive {
                    let action = self.security.read().await.check_approval("config.read");
                    match action {
                        lumi_common::security::ApprovalAction::Deny => {
                            let response = LumiMessage::new_error(&msg, "Access denied to sensitive config");
                            let _ = self.bus.send(response).await;
                            return;
                        }
                        _ => {}
                    }
                }

                let response = LumiMessage::new_response(
                    &msg,
                    serde_json::json!({
                        "key": key,
                        "value": serde_json::Value::Null,
                        "source": "core",
                    }),
                );
                if let Ok(r) = response {
                    let _ = self.bus.send(r).await;
                }
            }
        }
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_state_initialization() {
        let bus = MessageBus::new(ProcessId::Core);
        let state = CoreState::new(bus);

        // Verify all subsystems are initialized
        assert!(state.running.load(Ordering::Relaxed));
        assert!(!state.state_machine.blocking_read().is_idle()); // Starts in Initializing
        assert!(!state.privacy.blocking_read().is_feature_enabled("screen_capture"));
        assert_eq!(state.update.blocking_read().current_version(), "1.0.0");
        assert!((state.audio.blocking_read().master_volume() - 0.7).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_initialize_does_not_panic() {
        let bus = MessageBus::new(ProcessId::Core);
        let state = CoreState::new(bus);
        state.initialize().await;
        // After startup, state machine should have received StartupComplete
    }

    #[test]
    fn test_input_routes_to_state_machine() {
        let bus = MessageBus::new(ProcessId::Core);
        let state = CoreState::new(bus);

        // Verify input system can handle clicks
        let click = state.input.blocking_write().handle_click(50, 50, MouseButton::Left);
        assert!(click.is_some());
    }

    #[test]
    fn test_security_approval_gate() {
        let bus = MessageBus::new(ProcessId::Core);
        let state = CoreState::new(bus);

        let action = state.security.blocking_read().check_approval("fs.read_file");
        match action {
            lumi_common::security::ApprovalAction::AutoApprove => {}
            _ => panic!("Expected auto-approve for fs.read_file"),
        }
    }

    #[test]
    fn test_response_cache() {
        let bus = MessageBus::new(ProcessId::Core);
        let state = CoreState::new(bus);

        state.response_cache.blocking_write().put("hello", "Hi there!".into());
        let cached = state.response_cache.blocking_read().get("hello");
        assert_eq!(cached, Some("Hi there!"));
    }

    #[test]
    fn test_privacy_defaults() {
        let bus = MessageBus::new(ProcessId::Core);
        let state = CoreState::new(bus);

        assert!(!state.privacy.blocking_read().is_feature_enabled("clipboard_access"));
        assert!(state.privacy.blocking_read().is_feature_enabled("active_window_tracking"));
    }
}
