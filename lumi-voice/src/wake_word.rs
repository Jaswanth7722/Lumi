//! # Wake Word Detection Engine (Chapter 13.3)
//!
//! Local, low-power neural network-based wake word detection
//! that runs continuously on audio input without streaming to external services.

use lumi_common::voice::WakeWordResult;

/// Adaptive audio ring buffer for sliding window wake word detection.
pub struct RingBuffer {
    buffer: Vec<f32>,
    capacity: usize,
    write_pos: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity],
            capacity,
            write_pos: 0,
        }
    }

    /// Push audio samples into the ring buffer.
    pub fn push(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.buffer[self.write_pos] = sample;
            self.write_pos = (self.write_pos + 1) % self.capacity;
        }
    }

    /// Get the full buffer as a slice.
    pub fn as_slice(&self) -> &[f32] {
        &self.buffer
    }

    /// Check if the buffer has been filled at least once.
    pub fn full(&self) -> bool {
        // After one full cycle, the buffer is considered "full"
        self.write_pos % self.capacity == 0 || self.buffer[self.capacity - 1] != 0.0
    }

    /// Reset the buffer.
    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

/// Local wake word detection engine.
pub struct WakeWordEngine {
    model_loaded: bool,
    ring_buffer: RingBuffer,
    threshold: f32,
    cooldown_remaining_ms: u64,
}

impl WakeWordEngine {
    pub fn new() -> Self {
        Self {
            model_loaded: false,
            ring_buffer: RingBuffer::new(16000), // 1 second at 16kHz
            threshold: 0.85,
            cooldown_remaining_ms: 0,
        }
    }

    /// Load the wake word model (in production, a <2MB ONNX model).
    pub fn load_model(&mut self) {
        self.model_loaded = true;
    }

    /// Process an audio chunk and check for wake word.
    pub fn process_chunk(&mut self, samples: &[f32]) -> WakeWordResult {
        if !self.model_loaded {
            return WakeWordResult::NotDetected;
        }

        if self.cooldown_remaining_ms > 0 {
            self.cooldown_remaining_ms = self.cooldown_remaining_ms.saturating_sub(
                (samples.len() as u64 * 1000) / 16000,
            );
            return WakeWordResult::NotDetected;
        }

        self.ring_buffer.push(samples);

        if self.ring_buffer.full() {
            // In production, extract MFCC features and run ONNX inference
            let confidence = self.run_inference();
            if confidence >= self.threshold {
                self.cooldown_remaining_ms = 2000; // 2 second cooldown
                return WakeWordResult::Detected { confidence };
            }
        }

        WakeWordResult::NotDetected
    }

    /// Run inference on the current audio buffer.
    /// In production, this uses an ONNX runtime to run the wake word model.
    fn run_inference(&self) -> f32 {
        // Placeholder: simulate inference with noise
        0.0
    }

    /// Check if the model is loaded.
    pub fn is_loaded(&self) -> bool {
        self.model_loaded
    }

    /// Set the detection threshold.
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_push_and_full() {
        let mut buffer = RingBuffer::new(4);
        assert!(!buffer.full());
        buffer.push(&[1.0, 2.0, 3.0, 4.0]);
        assert!(buffer.full());
        assert_eq!(buffer.as_slice(), &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_ring_buffer_wraparound() {
        let mut buffer = RingBuffer::new(3);
        buffer.push(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        // After wraparound: positions 0,1,2 contain 4,5,3
        assert!(buffer.full());
    }

    #[test]
    fn test_wake_word_not_detected_without_model() {
        let mut engine = WakeWordEngine::new();
        assert_eq!(
            engine.process_chunk(&[0.0; 16000]),
            WakeWordResult::NotDetected
        );
    }

    #[test]
    fn test_wake_word_model_loading() {
        let mut engine = WakeWordEngine::new();
        assert!(!engine.is_loaded());
        engine.load_model();
        assert!(engine.is_loaded());
    }

    #[test]
    fn test_threshold_setting() {
        let mut engine = WakeWordEngine::new();
        engine.set_threshold(0.9);
        assert!((engine.threshold - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_ring_buffer_reset() {
        let mut buffer = RingBuffer::new(4);
        buffer.push(&[1.0, 2.0, 3.0, 4.0]);
        buffer.reset();
        assert!(!buffer.full());
        assert_eq!(buffer.as_slice(), &[0.0, 0.0, 0.0, 0.0]);
    }
}
