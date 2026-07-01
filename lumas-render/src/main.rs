//! # Lumas Render Process
//!
//! GPU-side rendering subsystem for the Lumas character and workspace panels.
//! Uses wgpu for cross-platform GPU abstraction (Metal/DX12/Vulkan).

use lumas_common::character::{CharacterDrawCall, CrystalState};
use lumas_common::ipc::{Channel, LumiMessage, ProcessId};
use lumas_common::render::{LODSystemConfig, LightingConfig, RenderBudget, RenderPipelineConfig};
use lumas_common::state_machine::StateCommand;
use lumas_ipc::MessageBus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

mod animation_engine;
mod character_engine;
mod desktop_engine;
mod rendering_engine;
mod workspace_system;

use animation_engine::AnimationEngine;
use character_engine::CharacterEngine;
use desktop_engine::DesktopEngine;
use rendering_engine::RenderingEngine;
use workspace_system::WorkspaceSystem;

/// Shared application state for the render process.
pub struct RenderState {
    pub bus: Arc<MessageBus>,
    pub character: Arc<RwLock<CharacterEngine>>,
    pub animation: Arc<RwLock<AnimationEngine>>,
    pub rendering: Arc<RwLock<RenderingEngine>>,
    pub workspace: Arc<RwLock<WorkspaceSystem>>,
    pub desktop: Arc<RwLock<DesktopEngine>>,
    pub running: Arc<AtomicBool>,
}

impl RenderState {
    pub fn new(bus: MessageBus) -> Self {
        Self {
            bus: Arc::new(bus),
            character: Arc::new(RwLock::new(CharacterEngine::new())),
            animation: Arc::new(RwLock::new(AnimationEngine::new())),
            rendering: Arc::new(RwLock::new(RenderingEngine::new())),
            workspace: Arc::new(RwLock::new(WorkspaceSystem::new())),
            desktop: Arc::new(RwLock::new(DesktopEngine::new())),
            running: Arc::new(AtomicBool::new(true)),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumas_render=info".into()),
        )
        .init();

    info!("Starting Lumas Render Process...");

    let bus = MessageBus::new(ProcessId::Render);
    let state = RenderState::new(bus);

    // Subscribe to channels (bus is internally thread-safe, no RwLock needed)
    let mut render_command_rx = state.bus.subscribe(Channel::RenderCommand);
    let mut ai_state_rx = state.bus.subscribe(Channel::AiState);
    let mut state_event_rx = state.bus.subscribe(Channel::StateEvent);

    info!("Render Process running");

    loop {
        tokio::select! {
            Ok(msg) = render_command_rx.recv() => {
                if let Ok(crystal) = serde_json::from_value::<CrystalState>(msg.payload.clone()) {
                    state.character.write().await.update_crystal(crystal);
                }
            }
            Ok(msg) = ai_state_rx.recv() => {
                debug!("AI state update: {:?}", msg.payload);
            }
            Ok(msg) = state_event_rx.recv() => {
                if let Ok(cmd) = serde_json::from_value::<StateCommand>(msg.payload.clone()) {
                    match cmd {
                        StateCommand::SetCrystalState(crystal) => {
                            state.character.write().await.update_crystal(crystal);
                        }
                        StateCommand::PlayAnimation { clip, blend } => {
                            state.animation.write().await.play_clip(clip, blend);
                        }
                        StateCommand::MoveTo(target) => {
                            state.desktop.write().await.move_to(target);
                        }
                        StateCommand::ShowWorkspacePanel(panel) => {
                            state.workspace.write().await.show_panel(panel);
                        }
                        _ => {}
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(16)) => {
                state.animation.write().await.update(1.0 / 60.0);
                state.desktop.write().await.update(1.0 / 60.0);
            }
        }

        if !state.running.load(Ordering::Relaxed) {
            break;
        }
    }

    info!("Render Process shut down");
    Ok(())
}
