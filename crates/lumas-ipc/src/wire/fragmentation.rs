// ── Fragmentation and Reassembly ──────────────────────────────────────────────
// Handles splitting large payloads into fragments and reassembling them.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::wire::metrics::WireMetrics;

/// A single fragment of a fragmented message.
#[derive(Debug, Clone)]
pub struct Fragment {
    pub msg_id: Uuid,
    pub index: u16,
    pub total: u16,
    pub data: Vec<u8>,
    pub fragment_id: u16,
}

/// Configuration for the reassembler.
#[derive(Debug, Clone)]
pub struct ReassemblerConfig {
    pub fragment_timeout: Duration,
    pub max_pending_fragments: usize,
    pub metrics: Arc<WireMetrics>,
}

impl Default for ReassemblerConfig {
    fn default() -> Self {
        Self {
            fragment_timeout: Duration::from_secs(30),
            max_pending_fragments: 100,
            metrics: Arc::new(WireMetrics::new()),
        }
    }
}

/// Splits payloads into fragments based on MTU.
#[derive(Debug, Clone)]
pub struct Fragmenter {
    mtu: usize,
}

impl Fragmenter {
    /// Create a new fragmenter with the given MTU.
    pub fn new(mtu: usize) -> Self {
        Self { mtu }
    }

    /// Split a payload into fragments.
    pub fn fragment(&self, payload: &[u8], msg_id: Uuid, fragment_id: u16) -> Vec<Fragment> {
        if payload.len() <= self.mtu {
            return vec![Fragment {
                msg_id,
                index: 0,
                total: 1,
                data: payload.to_vec(),
                fragment_id,
            }];
        }

        let total = ((payload.len() + self.mtu - 1) / self.mtu) as u16;
        let mut fragments = Vec::with_capacity(total as usize);

        for i in 0..total as usize {
            let start = i * self.mtu;
            let end = (start + self.mtu).min(payload.len());
            fragments.push(Fragment {
                msg_id,
                index: i as u16,
                total,
                data: payload[start..end].to_vec(),
                fragment_id,
            });
        }

        fragments
    }
}

struct PendingReassembly {
    fragments: Vec<Option<Vec<u8>>>,
    total: u16,
    received: u16,
    last_activity: Instant,
}

/// Reassembles fragments back into complete payloads.
#[derive(Debug)]
pub struct Reassembler {
    pending: HashMap<Uuid, PendingReassembly>,
    config: ReassemblerConfig,
}

impl Reassembler {
    /// Create a new reassembler with the given config.
    pub fn new(config: ReassemblerConfig, _metrics: Arc<WireMetrics>) -> Self {
        Self {
            pending: HashMap::new(),
            config,
        }
    }

    /// Add a fragment to be reassembled.
    pub fn add_fragment(&mut self, fragment: Fragment) -> Result<(), crate::wire::error::WireError> {
        if self.pending.len() >= self.config.max_pending_fragments && !self.pending.contains_key(&fragment.msg_id) {
            return Ok(()); // silently drop if at capacity
        }

        let entry = self.pending.entry(fragment.msg_id).or_insert_with(|| {
            PendingReassembly {
                fragments: vec![None; fragment.total as usize],
                total: fragment.total,
                received: 0,
                last_activity: Instant::now(),
            }
        });

        if entry.total != fragment.total {
            return Ok(()); // ignore mismatched total
        }

        let idx = fragment.index as usize;
        if idx >= entry.fragments.len() {
            return Ok(()); // ignore out-of-bounds
        }

        if entry.fragments[idx].is_none() {
            entry.fragments[idx] = Some(fragment.data);
            entry.received += 1;
        }

        entry.last_activity = Instant::now();
        Ok(())
    }

    /// Take a fully reassembled payload for the given message ID.
    pub fn take_reassembled(&mut self, msg_id: Uuid) -> Option<Vec<u8>> {
        let entry = self.pending.get(&msg_id)?;
        if entry.received < entry.total {
            return None; // not all fragments received yet
        }

        let entry = self.pending.remove(&msg_id)?;
        let all_present = entry.fragments.iter().all(|f| f.is_some());
        if !all_present {
            return None;
        }

        let mut result = Vec::with_capacity(entry.fragments.len() * 1024);
        for frag in entry.fragments.into_iter().flatten() {
            result.extend_from_slice(&frag);
        }
        Some(result)
    }

    /// Run garbage collection: remove timed-out partial reassemblies.
    /// Returns the number of removed entries.
    pub fn gc(&mut self) -> usize {
        let now = Instant::now();
        let before = self.pending.len();
        self.pending.retain(|_, entry| {
            now.duration_since(entry.last_activity) < self.config.fragment_timeout
                || self.config.fragment_timeout.is_zero()
        });
        before - self.pending.len()
    }
}
