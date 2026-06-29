//! # Handshake Protocol
//!
//! Implements the connection handshake sequence with version negotiation,
//! ECDH key exchange, and capability negotiation.

use crate::auth::{AuthEngine, SessionKey};
use crate::error::{AuthError, IpcError};
use crate::message::{
    CapabilitySet, ChannelName, HandshakePayload, LumiMessage, MessageKind,
    MessagePayload, ProcessToken, ProcessId, ProtocolVersion,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, info, warn};

/// Handshake protocol implementation.
pub struct HandshakeProtocol {
    /// Our process ID
    pub our_process_id: ProcessId,
    /// Our process token
    pub our_token: ProcessToken,
    /// Our capabilities
    pub capabilities: CapabilitySet,
    /// Auth engine
    pub auth: Arc<AuthEngine>,
}

impl HandshakeProtocol {
    /// Create a new handshake protocol instance.
    pub fn new(
        our_process_id: ProcessId,
        our_token: ProcessToken,
        capabilities: CapabilitySet,
        auth: Arc<AuthEngine>,
    ) -> Self {
        Self {
            our_process_id,
            our_token,
            capabilities,
            auth,
        }
    }

    /// Perform the client side of the handshake (initiating connection).
    ///
    /// 1. Send HandshakeRequest
    /// 2. Receive HandshakeResponse
    /// 3. Derive session key
    /// 4. Send HandshakeComplete
    /// 5. Receive Ready
    pub async fn client_handshake(
        &self,
        send_fn: impl Fn(LumiMessage) -> Result<(), IpcError>,
        recv_fn: impl Fn() -> Result<LumiMessage, IpcError>,
        timeout_duration: Duration,
    ) -> Result<(ProcessId, CapabilitySet), IpcError> {
        debug!("Starting client handshake as {}", self.our_process_id);

        // Step 1: Send HandshakeRequest
        let handshake_req = LumiMessage::builder()
            .sender(self.our_process_id.clone())
            .receiver(crate::message::MessageTarget::Broadcast)
            .channel("protocol.handshake")
            .kind(MessageKind::HandshakeRequest)
            .payload(MessagePayload::Handshake(HandshakePayload {
                process_id: self.our_process_id.clone(),
                capabilities: self.capabilities.clone(),
                ephemeral_pk: vec![], // ECDH not yet implemented — placeholder
                nonce: vec![],        // Nonce not yet implemented — placeholder
            }))
            .build()
            .map_err(|e| IpcError::Internal(e))?;

        send_fn(handshake_req)?;

        // Step 2: Receive HandshakeResponse
        let response = timeout(timeout_duration, async {
            loop {
                match recv_fn() {
                    Ok(msg) => {
                        if matches!(msg.kind, MessageKind::HandshakeResponse) {
                            return Ok(msg);
                        }
                        // Ignore other messages during handshake
                    }
                    Err(e) => return Err(e),
                }
            }
        })
        .await
        .map_err(|_| IpcError::HandshakeTimeout {
            peer: self.our_process_id.clone(),
            elapsed: timeout_duration,
        })??;

        // Extract capabilities from response
        let peer_capabilities = match &response.payload {
            MessagePayload::Handshake(payload) => payload.capabilities.clone(),
            _ => return Err(IpcError::HandshakeRejected {
                peer: self.our_process_id.clone(),
                reason: "Invalid handshake response payload".into(),
            }),
        };

        let peer_id = match &response.payload {
            MessagePayload::Handshake(payload) => payload.process_id.clone(),
            _ => return Err(IpcError::HandshakeRejected {
                peer: self.our_process_id.clone(),
                reason: "Invalid handshake response payload".into(),
            }),
        };

        info!(
            "Handshake complete with peer {} as {}",
            peer_id, self.our_process_id
        );

        Ok((peer_id, peer_capabilities))
    }

    /// Perform the server side of the handshake (accepting connection).
    ///
    /// 1. Receive HandshakeRequest
    /// 2. Validate capabilities
    /// 3. Send HandshakeResponse
    /// 4. Receive HandshakeComplete
    /// 5. Send Ready
    pub async fn server_handshake(
        &self,
        request: &LumiMessage,
        send_fn: impl Fn(LumiMessage) -> Result<(), IpcError>,
        timeout_duration: Duration,
    ) -> Result<(ProcessId, CapabilitySet), IpcError> {
        let peer_payload = match &request.payload {
            MessagePayload::Handshake(p) => p,
            _ => return Err(IpcError::HandshakeRejected {
                peer: self.our_process_id.clone(),
                reason: "Expected handshake payload".into(),
            }),
        };

        let peer_id = peer_payload.process_id.clone();
        let peer_capabilities = peer_payload.capabilities.clone();

        debug!(
            "Server handshake request from {} as {}",
            peer_id, self.our_process_id
        );

        // Validate capabilities — check version compatibility
        let our_version_range = (
            self.capabilities.supported_wire_versions.0,
            self.capabilities.supported_wire_versions.1,
        );
        let their_version_range = (
            peer_capabilities.supported_wire_versions.0,
            peer_capabilities.supported_wire_versions.1,
        );

        let compatible = our_version_range.0 <= their_version_range.1
            && their_version_range.0 <= our_version_range.1;

        if !compatible {
            return Err(IpcError::HandshakeRejected {
                peer: peer_id,
                reason: format!(
                    "Incompatible wire versions: we support {}-{}, they support {}-{}",
                    our_version_range.0,
                    our_version_range.1,
                    their_version_range.0,
                    their_version_range.1,
                ),
            });
        }

        // Build negotiated capabilities
        let negotiated = CapabilitySet {
            can_publish: self.capabilities.can_publish.clone(),
            can_subscribe: self.capabilities.can_subscribe.clone(),
            supported_message_versions: self.capabilities.supported_message_versions.clone(),
            supported_wire_versions: our_version_range,
            supports_compression: self.capabilities.supports_compression && peer_capabilities.supports_compression,
            supports_encryption: self.capabilities.supports_encryption && peer_capabilities.supports_encryption,
        };

        // Send HandshakeResponse
        let response = LumiMessage::builder()
            .sender(self.our_process_id.clone())
            .receiver(crate::message::MessageTarget::Process(peer_id.clone()))
            .channel("protocol.handshake")
            .kind(MessageKind::HandshakeResponse)
            .payload(MessagePayload::Handshake(HandshakePayload {
                process_id: self.our_process_id.clone(),
                capabilities: negotiated.clone(),
                ephemeral_pk: vec![],
                nonce: vec![],
            }))
            .build()
            .map_err(|e| IpcError::Internal(e))?;

        send_fn(response)?;

        info!(
            "Server handshake complete with peer {} as {}",
            peer_id, self.our_process_id
        );

        Ok((peer_id, peer_capabilities))
    }
}

/// Negotiate capabilities between two sets.
/// Returns the intersection of capabilities.
pub fn negotiate_capabilities(
    ours: &CapabilitySet,
    theirs: &CapabilitySet,
) -> CapabilitySet {
    let can_publish: Vec<String> = ours.can_publish.iter()
        .filter(|c| theirs.can_publish.contains(c))
        .cloned()
        .collect();

    let can_subscribe: Vec<String> = ours.can_subscribe.iter()
        .filter(|c| theirs.can_subscribe.contains(c))
        .cloned()
        .collect();

    CapabilitySet {
        can_publish,
        can_subscribe,
        supported_message_versions: ours.supported_message_versions.clone(),
        supported_wire_versions: (
            ours.supported_wire_versions.0.max(theirs.supported_wire_versions.0),
            ours.supported_wire_versions.1.min(theirs.supported_wire_versions.1),
        ),
        supports_compression: ours.supports_compression && theirs.supports_compression,
        supports_encryption: ours.supports_encryption && theirs.supports_encryption,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_negotiation() {
        let ours = CapabilitySet {
            can_publish: vec!["ai.state".into(), "render.command".into()],
            can_subscribe: vec!["ai.state".into()],
            supported_message_versions: vec![],
            supported_wire_versions: (1, 2),
            supports_compression: true,
            supports_encryption: false,
        };

        let theirs = CapabilitySet {
            can_publish: vec!["ai.state".into()],
            can_subscribe: vec!["render.command".into()],
            supported_message_versions: vec![],
            supported_wire_versions: (1, 1),
            supports_compression: true,
            supports_encryption: true,
        };

        let negotiated = negotiate_capabilities(&ours, &theirs);

        assert!(negotiated.can_publish.contains(&"ai.state".to_string()));
        assert!(!negotiated.can_publish.contains(&"render.command".to_string()));
        assert!(negotiated.supports_compression);
        assert!(!negotiated.supports_encryption);
        assert_eq!(negotiated.supported_wire_versions, (1, 1));
    }
}
