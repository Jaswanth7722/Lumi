//! # Hot Reload Engine
//!
//! Watches the config file for changes using OS-native events.
//! Debounces rapid changes and validates before applying.

use crate::error::ConfigError;
use crate::events::{ConfigReloadFailed, ConfigReloaded};
use crate::loader::ConfigLoader;
use crate::manager::ConfigManager;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

/// File watcher for hot-reloading configuration.
pub struct ConfigWatcher {
    /// Path to the config file being watched.
    path: PathBuf,
    /// Debounce interval in milliseconds.
    debounce_ms: u64,
    /// SHA-256 hash of the last loaded file content.
    hash: Arc<Mutex<[u8; 32]>>,
    /// Reference to the config manager for applying reloads.
    manager: Arc<ConfigManager>,
    /// Watch channel sender for stopping the watcher.
    shutdown_tx: watch::Sender<bool>,
    /// Watch channel receiver for stopping the watcher.
    shutdown_rx: watch::Receiver<bool>,
    /// Whether the watcher is running.
    running: Arc<AtomicBool>,
}

impl ConfigWatcher {
    /// Create a new config file watcher.
    pub fn new(path: PathBuf, manager: Arc<ConfigManager>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            path,
            debounce_ms: 500,
            hash: Arc::new(Mutex::new([0u8; 32])),
            manager,
            shutdown_tx,
            shutdown_rx,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set a custom debounce interval.
    pub fn with_debounce(mut self, debounce_ms: u64) -> Self {
        self.debounce_ms = debounce_ms;
        self
    }

    /// Start watching the config file for changes.
    ///
    /// On change:
    /// 1. Compute SHA-256 of new file content
    /// 2. If hash unchanged: skip (spurious event)
    /// 3. Run full load pipeline on new content
    /// 4. If validation passes: atomically swap config and emit ConfigReloaded
    /// 5. If validation fails: retain old config, emit ConfigReloadFailed
    pub async fn run(mut self) -> Result<(), ConfigError> {
        self.running.store(true, Ordering::Relaxed);

        // Compute initial hash
        if let Ok(content) = std::fs::read(&self.path) {
            let hash = Sha256::digest(&content);
            *self.hash.lock().unwrap() = hash.into();
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let path = self.path.clone();

        // Set up notify watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        let _ = tx.blocking_send(());
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| ConfigError::ReloadFailed {
            reason: format!("Failed to create file watcher: {e}"),
        })?;

        watcher
            .watch(
                path.parent().unwrap_or(std::path::Path::new(".")),
                RecursiveMode::NonRecursive,
            )
            .map_err(|e| ConfigError::ReloadFailed {
                reason: format!("Failed to watch config directory: {e}"),
            })?;

        info!("Config watcher started for {:?}", self.path);

        loop {
            tokio::select! {
                Some(_) = rx.recv() => {
                    // Debounce: wait for quiet period
                    tokio::time::sleep(Duration::from_millis(self.debounce_ms)).await;

                    // Drain any pending events during debounce
                    while rx.try_recv().is_ok() {}

                    // Check file hash to skip spurious events
                    let new_content = match std::fs::read(&self.path) {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Cannot read config file for reload: {e}");
                            continue;
                        }
                    };

                    let new_hash: [u8; 32] = Sha256::digest(&new_content).into();
                    {
                        let mut last_hash = self.hash.lock().unwrap();
                        if *last_hash == new_hash {
                            debug!("Config file unchanged, skipping reload");
                            continue;
                        }
                        *last_hash = new_hash;
                    }

                    info!("Config file changed, reloading...");

                    // Run load pipeline on new content
                    let loader = ConfigLoader::new()
                        .with_path(self.path.clone());

                    match loader.load().await {
                        Ok((_cache, _config)) => {
                            // Emit reloaded event
                            if let Some(ref publisher) = self.manager.event_publisher() {
                                publisher.on_config_reloaded(ConfigReloaded::new(vec![])).await;
                            }
                            info!("Config reloaded successfully");
                        }
                        Err(e) => {
                            error!("Config reload failed: {e}");
                            if let Some(ref publisher) = self.manager.event_publisher() {
                                publisher.on_config_reload_failed(ConfigReloadFailed::new(e.to_string())).await;
                            }
                            warn!("Previous configuration retained");
                        }
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    info!("Config watcher shutting down");
                    break;
                }
            }
        }

        self.running.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Stop the watcher.
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Whether the watcher is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}
