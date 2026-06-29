//! # Authentication Engine
//!
//! Process authentication uses HMAC-SHA256 over the message header + payload,
//! keyed with a process-pair shared secret established during the ECDH handshake.
//!
//! Every authenticated message includes a monotonic sequence number for replay
//! attack prevention. The replay window tracks recently seen sequence numbers
//! and rejects duplicates.

use crate::error::AuthError;
use crate::message::{LumiMessage, MessageAuth, ProcessToken, next_sequence};
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use zeroize::Zeroizing;

/// HMAC-SHA256 type alias.
type HmacSha256 = Hmac<Sha256>;

/// Session key — zeroized on drop.
#[derive(Clone)]
pub struct SessionKey(Arc<Zeroizing<[u8; 32]>>);

impl SessionKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self(Arc::new(Zeroizing::new(key)))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionKey").finish_non_exhaustive()
    }
}

/// Replay attack prevention window.
/// Tracks recently seen sequence numbers per sender using a bitset.
pub struct ReplayWindow {
    sender: super::message::ProcessId,
    last_seq: AtomicU64,
    window_size: usize,
    /// BitVec tracking which sequence numbers have been seen
    /// within the sliding window [last_seq - window_size, last_seq]
    seen: std::sync::Mutex<bitvec::vec::BitVec>,
}

impl ReplayWindow {
    /// Create a new replay window for a sender.
    pub fn new(sender: super::message::ProcessId, window_size: usize) -> Self {
        Self {
            sender,
            last_seq: AtomicU64::new(0),
            window_size,
            seen: std::sync::Mutex::new(bitvec::vec::BitVec::repeat(false, window_size)),
        }
    }

    /// Check if a sequence number is valid (not a replay).
    /// Returns true if the sequence is acceptable, false if it's a replay.
    pub fn check_and_update(&self, sequence: u64) -> Result<(), AuthError> {
        let last = self.last_seq.load(Ordering::SeqCst);

        if sequence <= last.saturating_sub(self.window_size as u64) {
            return Err(AuthError::ReplayDetected { sequence });
        }

        let mut seen = self.seen.lock().unwrap();

        if sequence > last {
            // Advance the window
            let advance = (sequence - last) as usize;
            if advance < self.window_size {
                // Shift bits: rotate left by advance
                seen.shift_left(advance);
                // Mark the new sequence as seen (at position window_size - 1 from the right)
                let pos = self.window_size - 1;
                if pos < seen.len() {
                    seen.set(pos, true);
                }
            } else {
                // Window advanced beyond window_size, reset
                seen.fill(false);
                seen.set(self.window_size - 1, true);
            }
            self.last_seq.store(sequence, Ordering::SeqCst);
            Ok(())
        } else {
            // Sequence <= last — check within window
            let offset = (last - sequence) as usize;
            if offset < self.window_size {
                let pos = self.window_size - 1 - offset;
                if pos < seen.len() && seen[pos] {
                    return Err(AuthError::ReplayDetected { sequence });
                }
                seen.set(pos, true);
                Ok(())
            } else {
                // Outside window — treat as valid (stale but not replay)
                Ok(())
            }
        }
    }
}

/// Authentication engine for the IPC framework.
pub struct AuthEngine {
    /// Our process identity
    process_id: super::message::ProcessId,
    /// Our process token
    process_token: ProcessToken,
    /// Session keys per peer
    session_keys: DashMap<super::message::ProcessId, SessionKey>,
    /// Replay windows per peer
    replay_windows: DashMap<super::message::ProcessId, Arc<ReplayWindow>>,
    /// Next key ID
    next_key_id: AtomicU64,
    /// Replay window size
    replay_window_size: usize,
}

impl AuthEngine {
    /// Create a new authentication engine.
    pub fn new(
        process_id: super::message::ProcessId,
        process_token: ProcessToken,
        replay_window_size: usize,
    ) -> Self {
        Self {
            process_id,
            process_token,
            session_keys: DashMap::new(),
            replay_windows: DashMap::new(),
            next_key_id: AtomicU64::new(1),
            replay_window_size,
        }
    }

    /// Sign an outbound message with a MAC.
    pub fn sign(&self, msg: &mut LumiMessage) -> Result<(), AuthError> {
        let peer = match &msg.receiver {
            super::message::MessageTarget::Process(p) => p,
            _ => return Ok(()), // Broadcast messages are not signed individually
        };

        let key = self.session_keys.get(peer)
            .ok_or_else(|| AuthError::NoSessionKey { peer: peer.clone() })?;

        // Ensure sequence is set
        if msg.sequence == 0 {
            msg.sequence = next_sequence();
        }

        // Serialize header + payload for MAC computation
        let mut mac = HmacSha256::new_from_slice(key.as_bytes())
            .map_err(|e| AuthError::Protocol(e.to_string()))?;

        // Include sender, receiver, channel, sequence in the MAC
        mac.update(msg.sender.to_string().as_bytes());
        mac.update(msg.receiver.as_process().map(|p| p.to_string()).unwrap_or_default().as_bytes());
        mac.update(msg.channel.0.as_bytes());
        mac.update(&msg.sequence.to_le_bytes());
        mac.update(&msg.timestamp.to_le_bytes());

        // Include payload bytes
        if let Ok(payload_bytes) = rmp_serde::to_vec(&msg.payload) {
            mac.update(&payload_bytes);
        }

        let result = mac.finalize();
        let mac_bytes = result.into_bytes();

        msg.auth = Some(MessageAuth {
            process_token: self.process_token.clone(),
            mac: mac_bytes.into(),
            key_id: self.next_key_id.load(Ordering::Relaxed),
        });

        Ok(())
    }

    /// Verify the MAC on an inbound message.
    /// Updates the replay window for this sender.
    pub fn verify(&self, msg: &LumiMessage) -> Result<(), AuthError> {
        let auth = msg.auth.as_ref()
            .ok_or(AuthError::MacVerificationFailed)?;

        // Check replay window
        let window = self.replay_windows
            .entry(msg.sender.clone())
            .or_insert_with(|| {
                Arc::new(ReplayWindow::new(
                    msg.sender.clone(),
                    self.replay_window_size,
                ))
            });

        window.check_and_update(msg.sequence)?;

        // Verify MAC
        let key = self.session_keys.get(&msg.sender)
            .ok_or_else(|| AuthError::NoSessionKey { peer: msg.sender.clone() })?;

        let mut mac = HmacSha256::new_from_slice(key.as_bytes())
            .map_err(|e| AuthError::Protocol(e.to_string()))?;

        mac.update(msg.sender.to_string().as_bytes());
        mac.update(msg.receiver.as_process().map(|p| p.to_string()).unwrap_or_default().as_bytes());
        mac.update(msg.channel.0.as_bytes());
        mac.update(&msg.sequence.to_le_bytes());
        mac.update(&msg.timestamp.to_le_bytes());

        if let Ok(payload_bytes) = rmp_serde::to_vec(&msg.payload) {
            mac.update(&payload_bytes);
        }

        mac.verify_slice(&auth.mac)
            .map_err(|_| AuthError::MacVerificationFailed)
    }

    /// Register a session key for a peer.
    pub fn register_session_key(
        &self,
        peer: super::message::ProcessId,
        key: SessionKey,
    ) {
        self.session_keys.insert(peer, key);
        self.replay_windows.insert(
            peer.clone(),
            Arc::new(ReplayWindow::new(peer, self.replay_window_size)),
        );
    }

    /// Remove a session key for a peer.
    pub fn remove_session_key(&self, peer: &super::message::ProcessId) {
        self.session_keys.remove(peer);
        self.replay_windows.remove(peer);
    }

    /// Check if a session key exists for a peer.
    pub fn has_session_key(&self, peer: &super::message::ProcessId) -> bool {
        self.session_keys.contains_key(peer)
    }

    /// Get our process token.
    pub fn process_token(&self) -> &ProcessToken {
        &self.process_token
    }

    /// Get our process ID.
    pub fn process_id(&self) -> &super::message::ProcessId {
        &self.process_id
    }
}
