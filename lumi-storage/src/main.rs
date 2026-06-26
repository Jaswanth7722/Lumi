//! # Lumi Storage Process
//!
//! Dedicated persistent storage process managing the memory store,
//! configuration store, and asset cache. Uses SQLite + sqlite-vec
//! for memory storage with vector embedding support.

#![allow(unused_results)]

use lumi_common::ipc::{Channel, LumiMessage, MessageType, ProcessId};
use lumi_common::memory::{
    MemoryQuery, MemoryQueryResult, QueryMemoryRequest, RetentionConfig, RetrieverConfig,
    WriteMemoryRequest, WriteMemoryResult,
};
use lumi_ipc::MessageBus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

mod asset_cache;
mod config_store;
mod memory_store;

use asset_cache::AssetCache;
use config_store::ConfigStore;
use memory_store::MemoryStore;

/// Shared application state for the storage process.
pub struct StorageState {
    pub bus: Arc<RwLock<MessageBus>>,
    pub memory: Arc<RwLock<MemoryStore>>,
    pub config: Arc<RwLock<ConfigStore>>,
    pub cache: Arc<RwLock<AssetCache>>,
    pub running: Arc<AtomicBool>,
    pub retriever_config: RetrieverConfig,
    pub retention_config: RetentionConfig,
}

impl StorageState {
    pub fn new(bus: MessageBus) -> Self {
        Self {
            bus: Arc::new(RwLock::new(bus)),
            memory: Arc::new(RwLock::new(MemoryStore::new())),
            config: Arc::new(RwLock::new(ConfigStore::new())),
            cache: Arc::new(RwLock::new(AssetCache::new())),
            running: Arc::new(AtomicBool::new(true)),
            retriever_config: RetrieverConfig::default(),
            retention_config: RetentionConfig::default(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumi_storage=info".into()),
        )
        .init();

    info!("Starting Lumi Storage Process...");

    let bus = MessageBus::new(ProcessId::Storage);
    let state = StorageState::new(bus);

    // Subscribe to memory and config channels
    let mut memory_write_rx = state.bus.read().await.subscribe(Channel::MemoryWrite);
    let mut memory_query_rx = state.bus.read().await.subscribe(Channel::MemoryQuery);
    let bus_sender = state.bus.read().await.sender();

    info!("Storage Process running");

    loop {
        tokio::select! {
            Ok(msg) = memory_write_rx.recv() => {
                if msg.msg_type == MessageType::Request {
                    if let Ok(request) = serde_json::from_value::<WriteMemoryRequest>(msg.payload.clone()) {
                        let result = state.memory.write().await.write(request);
                        if let Ok(response) = LumiMessage::new_response(&msg, result) {
                            let _ = bus_sender.send(response).await;
                        }
                    }
                }
            }
            Ok(msg) = memory_query_rx.recv() => {
                if msg.msg_type == MessageType::Request {
                    if let Ok(request) = serde_json::from_value::<QueryMemoryRequest>(msg.payload.clone()) {
                        let result = state.memory.read().await.query(request);
                        if let Ok(response) = LumiMessage::new_response(&msg, result) {
                            let _ = bus_sender.send(response).await;
                        }
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)) => {
                // Periodic maintenance: cleanup expired memories
            }
        }

        if !state.running.load(Ordering::Relaxed) {
            break;
        }
    }

    info!("Storage Process shut down");
    Ok(())
}
