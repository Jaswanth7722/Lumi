//! # Lumas Voice Process
//!
//! Dedicated audio process handling wake word detection, speech-to-text
//! transcription, text-to-speech synthesis, and lip sync extraction.
//! Operates in a separate process for isolation from render and AI workloads.

#![allow(unused_results)]

use lumas_common::ipc::{Channel, LumiMessage, ProcessId};
use lumas_common::voice::{
    LipSyncData, LipSyncFrame, SSMLConfig, VADConfig, Viseme, VoiceConfig, WakeWordConfig,
    WhisperConfig, WhisperModelSize,
};
use lumas_ipc::MessageBus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

mod ssml_processor;
mod stt_engine;
mod tts_engine;
mod wake_word;

use ssml_processor::SSMLProcessor;
use stt_engine::STTEngine;
use tts_engine::TTSEngine;
use wake_word::WakeWordEngine;

/// Shared application state for the voice process.
pub struct VoiceState {
    pub bus: Arc<RwLock<MessageBus>>,
    pub wake_word: Arc<RwLock<WakeWordEngine>>,
    pub stt: Arc<RwLock<STTEngine>>,
    pub tts: Arc<RwLock<TTSEngine>>,
    pub ssml: Arc<RwLock<SSMLProcessor>>,
    pub running: Arc<AtomicBool>,
    pub wake_word_config: WakeWordConfig,
    pub stt_config: WhisperConfig,
    pub voice_config: VoiceConfig,
    pub vad_config: VADConfig,
    pub ssml_config: SSMLConfig,
}

impl VoiceState {
    pub fn new(bus: MessageBus) -> Self {
        Self {
            bus: Arc::new(RwLock::new(bus)),
            wake_word: Arc::new(RwLock::new(WakeWordEngine::new())),
            stt: Arc::new(RwLock::new(STTEngine::new())),
            tts: Arc::new(RwLock::new(TTSEngine::new())),
            ssml: Arc::new(RwLock::new(SSMLProcessor::new())),
            running: Arc::new(AtomicBool::new(true)),
            wake_word_config: WakeWordConfig::default(),
            stt_config: WhisperConfig::default(),
            voice_config: VoiceConfig::default(),
            vad_config: VADConfig::default(),
            ssml_config: SSMLConfig::default(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumas_voice=info".into()),
        )
        .init();

    info!("Starting Lumas Voice Process...");

    let bus = MessageBus::new(ProcessId::Voice);
    let state = VoiceState::new(bus);

    // Subscribe to TTS requests from core
    let mut voice_output_rx = state.bus.read().await.subscribe(Channel::VoiceOutput);
    let bus_sender = state.bus.read().await.sender();

    // Initialize subsystems
    info!("Voice Process initializing...");
    info!(
        "Wake word: '{}' (threshold: {})",
        state.wake_word_config.primary_phrase, state.wake_word_config.threshold
    );
    info!(
        "STT model: {:?} ({})",
        state.stt_config.model_size,
        state.stt_config.model_size.size_mb()
    );
    info!(
        "Voice: '{}' (rate: {})",
        state.voice_config.voice_id, state.voice_config.speaking_rate
    );

    info!("Voice Process running");

    loop {
        tokio::select! {
            Ok(msg) = voice_output_rx.recv() => {
                // Process TTS request from core
                if let Some(text) = msg.payload.get("text").and_then(|t| t.as_str()) {
                    let processed = state.ssml.write().await.process(text);
                    let audio = state.tts.write().await.synthesize(&processed).await;
                    if let Ok(audio_data) = audio {
                        // Send audio back to render process for playback
                        let lip_sync = LipSyncData {
                            frames: vec![],
                            duration_ms: audio_data.duration_ms,
                            sample_rate: 24000,
                        };
                        let response = LumiMessage::new_event(
                            ProcessId::Voice,
                            Channel::VoiceOutput,
                            serde_json::json!({
                                "audio": audio_data.data,
                                "lip_sync": {
                                    "duration_ms": lip_sync.duration_ms,
                                    "sample_rate": lip_sync.sample_rate,
                                }
                            }),
                        );
                        if let Ok(r) = response {
                            let _ = bus_sender.send(r).await;
                        }
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                // Periodic wake word checking
                // In production, this processes audio chunks from the microphone
            }
        }

        if !state.running.load(Ordering::Relaxed) {
            break;
        }
    }

    info!("Voice Process shut down");
    Ok(())
}
