//! # Performance Engineering — Frame Pacing and Optimization (Chapter 25)

use lumi_common::performance::{FramePacerConfig, ResponseCacheConfig};
use std::time::Instant;

/// Frame pacer for stable 60 FPS rendering with adaptive timing.
pub struct FramePacer {
    config: FramePacerConfig,
    last_frame: Option<Instant>,
    frame_times: Vec<f64>,
}

impl FramePacer {
    pub fn new(config: FramePacerConfig) -> Self {
        Self {
            config,
            last_frame: None,
            frame_times: Vec::with_capacity(120),
        }
    }

    pub fn wait_for_next_frame(&mut self) {
        let budget_us = self.config.frame_budget_us();
        if let Some(last) = self.last_frame {
            let elapsed_us = last.elapsed().as_micros() as u64;
            if elapsed_us < budget_us {
                let sleep_us = budget_us.saturating_sub(elapsed_us).saturating_sub(500);
                if sleep_us > 0 {
                    std::thread::sleep(std::time::Duration::from_micros(sleep_us));
                }
            }
        }
        if let Some(last) = self.last_frame {
            let frame_time = last.elapsed().as_secs_f64();
            self.frame_times.push(frame_time);
            if self.frame_times.len() > 120 {
                self.frame_times.remove(0);
            }
        }
        self.last_frame = Some(Instant::now());
    }

    pub fn average_frame_time(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        self.frame_times.iter().sum::<f64>() / self.frame_times.len() as f64
    }

    pub fn current_fps(&self) -> f64 {
        let avg = self.average_frame_time();
        if avg > 0.0 { 1.0 / avg } else { 0.0 }
    }

    pub fn target_fps(&self) -> u32 {
        self.config.target_fps
    }

    pub fn set_target_fps(&mut self, fps: u32) {
        self.config.target_fps = fps;
    }
}

/// Response cache for frequently repeated queries.
pub struct ResponseCache {
    config: ResponseCacheConfig,
    entries: Vec<CachedEntry>,
}

struct CachedEntry {
    query: String,
    response: String,
    created_at: Instant,
}

impl ResponseCache {
    pub fn new(config: ResponseCacheConfig) -> Self {
        Self {
            config,
            entries: Vec::new(),
        }
    }

    pub fn get(&self, query: &str) -> Option<&str> {
        self.entries.iter()
            .find(|e| e.query == query && e.created_at.elapsed().as_secs() < self.config.max_age_secs)
            .map(|e| e.response.as_str())
    }

    pub fn put(&mut self, query: &str, response: String) {
        if self.entries.len() >= self.config.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(CachedEntry {
            query: query.to_string(),
            response,
            created_at: Instant::now(),
        });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_pacer_budget() {
        let config = FramePacerConfig::default();
        let pacer = FramePacer::new(config);
        assert_eq!(pacer.target_fps(), 60);
    }

    #[test]
    fn test_response_cache() {
        let config = ResponseCacheConfig::default();
        let mut cache = ResponseCache::new(config);
        cache.put("hello", "Hi there!".into());
        assert_eq!(cache.get("hello"), Some("Hi there!"));
        assert_eq!(cache.get("unknown"), None);
    }

    #[test]
    fn test_cache_eviction() {
        let config = ResponseCacheConfig { max_entries: 2, max_age_secs: 3600, enabled: true };
        let mut cache = ResponseCache::new(config);
        cache.put("a", "1".into());
        cache.put("b", "2".into());
        cache.put("c", "3".into());
        assert_eq!(cache.len(), 2);
    }
}
