//! # Lumas Plugin Host Process
//!
//! Manages WebAssembly plugin sandboxes using Wasmtime for capability-based
//! isolation. Handles plugin registration, capability declaration,
//! and tool execution within isolated sandboxes.

#![allow(unused_results)]

use lumas_common::ipc::{Channel, LumiMessage, MessageType, ProcessId};
use lumas_common::tool::{Capability, ToolDefinition, ToolError};
use lumas_ipc::MessageBus;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

mod capability_broker;
mod plugin_registry;
mod plugin_sandbox;

use capability_broker::CapabilityBroker;
use plugin_registry::PluginRegistry;
use plugin_sandbox::PluginSandbox;

/// Shared application state for the plugin host process.
pub struct PluginHostState {
    pub bus: Arc<RwLock<MessageBus>>,
    pub registry: Arc<RwLock<PluginRegistry>>,
    pub sandbox: Arc<RwLock<PluginSandbox>>,
    pub broker: Arc<RwLock<CapabilityBroker>>,
    pub running: Arc<AtomicBool>,
}

impl PluginHostState {
    pub fn new(bus: MessageBus) -> Self {
        Self {
            bus: Arc::new(RwLock::new(bus)),
            registry: Arc::new(RwLock::new(PluginRegistry::new())),
            sandbox: Arc::new(RwLock::new(PluginSandbox::new())),
            broker: Arc::new(RwLock::new(CapabilityBroker::new())),
            running: Arc::new(AtomicBool::new(true)),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumas_plugin_host=info".into()),
        )
        .init();

    info!("Starting Lumas Plugin Host Process...");

    let bus = MessageBus::new(ProcessId::PluginHost);
    let state = PluginHostState::new(bus);

    // Subscribe to plugin invocation requests from core
    let mut plugin_invoke_rx = state.bus.read().await.subscribe(Channel::PluginInvoke);
    let bus_sender = state.bus.read().await.sender();

    info!("Plugin Host Process running");

    loop {
        tokio::select! {
            Ok(msg) = plugin_invoke_rx.recv() => {
                if msg.msg_type == MessageType::Request {
                    // Execute the requested tool in the plugin sandbox
                    if let Some(tool_name) = msg.payload.get("tool").and_then(|t| t.as_str()) {
                        let capability = state.broker.read().await.check_tool(tool_name);
                        match capability {
                            Ok(_caps) => {
                                let result = state.sandbox.write().await.execute(tool_name, &msg.payload);
                                let response = match result {
                                    Ok(output) => {
                                        let res = LumiMessage::new_response(
                                            &msg,
                                            serde_json::json!({
                                                "status": "success",
                                                "output": output,
                                            }),
                                        );
                                        if let Ok(r) = res { r } else { continue; }
                                    }
                                    Err(e) => LumiMessage::new_error(&msg, e.to_string()),
                                };
                                let _ = bus_sender.send(response).await;
                            }
                            Err(e) => {
                                let response = LumiMessage::new_error(&msg, e);
                                let _ = bus_sender.send(response).await;
                            }
                        }
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
                // Periodic plugin health check
            }
        }

        if !state.running.load(Ordering::Relaxed) {
            break;
        }
    }

    info!("Plugin Host Process shut down");
    Ok(())
}
