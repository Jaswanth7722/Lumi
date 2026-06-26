//! # Wire Protocol — Framed MessagePack Transport Layer
//!
//! Defines the on-wire framing protocol for Lumi IPC messages.
//! Each message frame consists of:
//! - 4-byte little-endian length prefix (u32)
//! - MessagePack-serialized bytes of the LumiMessage
//!
//! This provides a self-delimiting framing protocol that works
//! over any streaming transport (Unix sockets, named pipes, TCP).

use anyhow::{Result, anyhow, bail};
use lumi_common::ipc::LumiMessage;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Maximum frame size: 16 MB.
pub const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

/// Magic bytes for protocol identification: "LUMI" as ASCII.
pub const PROTOCOL_MAGIC: &[u8; 4] = b"LUMI";

/// Protocol version.
pub const PROTOCOL_VERSION: u8 = 1;

// ---------------------------------------------------------------------------
// Frame Encoding / Decoding
// ---------------------------------------------------------------------------

/// A single framed message ready to be sent over the wire.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The serialized MessagePack bytes of the LumiMessage.
    pub payload: Vec<u8>,
}

impl Frame {
    /// Create a new frame from a LumiMessage by serializing to MessagePack.
    pub fn from_message(msg: &LumiMessage) -> Result<Self> {
        let payload = rmp_serde::to_vec(msg)?;
        Ok(Self { payload })
    }

    /// Deserialize the frame back into a LumiMessage.
    pub fn into_message(self) -> Result<LumiMessage> {
        Ok(rmp_serde::from_slice(&self.payload)?)
    }

    /// Encode the frame as wire bytes: [magic(4), version(1), length(4), payload(n)].
    pub fn encode(&self) -> Vec<u8> {
        let len = self.payload.len() as u32;
        let mut bytes = Vec::with_capacity(9 + self.payload.len());

        bytes.extend_from_slice(PROTOCOL_MAGIC); // 4 bytes: magic
        bytes.push(PROTOCOL_VERSION); // 1 byte: version
        bytes.extend_from_slice(&len.to_le_bytes()); // 4 bytes: length (LE)
        bytes.extend_from_slice(&self.payload); // n bytes: payload

        bytes
    }

    /// Decode a frame from wire bytes (assumes header has already been parsed).
    pub fn decode(payload: Vec<u8>) -> Self {
        Self { payload }
    }
}

// ---------------------------------------------------------------------------
// Streaming Reader / Writer
// ---------------------------------------------------------------------------

/// Reads framed messages from an async byte stream.
pub struct FrameReader {
    /// Buffer for accumulating incoming data.
    buffer: Vec<u8>,
    /// Current read offset into the buffer.
    offset: usize,
    /// Expected total frame size being read (0 = awaiting header).
    expected_size: usize,
    /// Read state machine.
    state: ReadState,
}

enum ReadState {
    /// Reading the 9-byte header (magic + version + length).
    Header,
    /// Reading the message payload.
    Payload { remaining: usize },
}

impl FrameReader {
    pub fn new() -> Self {
        Self {
            buffer: vec![0u8; 4096],
            offset: 0,
            expected_size: 0,
            state: ReadState::Header,
        }
    }

    /// Read one complete frame from the given async reader.
    /// Returns `None` when the stream is closed (EOF).
    pub async fn read_frame<R: AsyncReadExt + Unpin + ?Sized>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<Frame>> {
        loop {
            match &self.state {
                ReadState::Header => {
                    // Need 9 bytes: 4 magic + 1 version + 4 length
                    let header_size = 9;
                    if self.buffer.len() < header_size {
                        self.buffer.resize(header_size, 0);
                    }

                    while self.offset < header_size {
                        let n = reader
                            .read(&mut self.buffer[self.offset..header_size])
                            .await?;
                        if n == 0 {
                            // EOF
                            return if self.offset == 0 {
                                Ok(None) // Clean EOF
                            } else {
                                bail!("Unexpected EOF while reading frame header")
                            };
                        }
                        self.offset += n;
                    }

                    // Validate magic bytes
                    if &self.buffer[0..4] != PROTOCOL_MAGIC {
                        bail!(
                            "Invalid protocol magic: got {:02x?}, expected LUMI",
                            &self.buffer[0..4]
                        );
                    }

                    // Check version
                    let version = self.buffer[4];
                    if version != PROTOCOL_VERSION {
                        bail!(
                            "Unsupported protocol version: {version}, expected {PROTOCOL_VERSION}"
                        );
                    }

                    // Parse payload length (little-endian u32)
                    let len_bytes: [u8; 4] = self.buffer[5..9].try_into().unwrap();
                    let payload_len = u32::from_le_bytes(len_bytes);

                    if payload_len > MAX_FRAME_SIZE {
                        bail!("Frame too large: {payload_len} bytes (max {MAX_FRAME_SIZE})");
                    }

                    self.expected_size = 9 + payload_len as usize;
                    self.state = ReadState::Payload {
                        remaining: payload_len as usize,
                    };

                    // Fall through to read payload in the same iteration
                }

                ReadState::Payload { remaining } => {
                    if self.buffer.len() < self.expected_size {
                        self.buffer.resize(self.expected_size, 0);
                    }

                    if *remaining > 0 {
                        let n = reader
                            .read(&mut self.buffer[self.offset..self.expected_size])
                            .await?;
                        if n == 0 {
                            bail!("Unexpected EOF while reading frame payload");
                        }
                        self.offset += n;

                        let new_remaining = self.expected_size.saturating_sub(self.offset);
                        self.state = ReadState::Payload {
                            remaining: new_remaining,
                        };

                        if new_remaining > 0 {
                            continue; // Need more data
                        }
                    }

                    // Complete frame received
                    let payload = self.buffer[9..self.expected_size].to_vec();
                    let frame = Frame::decode(payload);

                    // Reset reader state
                    self.offset = 0;
                    self.state = ReadState::Header;

                    return Ok(Some(frame));
                }
            }
        }
    }
}

/// Writes framed messages to an async byte stream.
pub struct FrameWriter;

impl FrameWriter {
    /// Write a framed message to the given async writer.
    pub async fn write_frame<W: AsyncWriteExt + Unpin + ?Sized>(
        writer: &mut W,
        frame: &Frame,
    ) -> Result<()> {
        let bytes = frame.encode();
        writer.write_all(&bytes).await?;
        writer.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumi_common::ipc::{Channel, MessageType, ProcessId};

    #[test]
    fn test_frame_encode_decode() {
        let msg = LumiMessage::new_request(
            ProcessId::Core,
            ProcessId::Render,
            Channel::RenderCommand,
            serde_json::json!({"animation": "walk"}),
        )
        .unwrap();

        let frame = Frame::from_message(&msg).unwrap();
        let bytes = frame.encode();

        // Parse the header
        assert_eq!(&bytes[0..4], PROTOCOL_MAGIC);
        assert_eq!(bytes[4], PROTOCOL_VERSION);

        let len = u32::from_le_bytes(bytes[5..9].try_into().unwrap());
        assert_eq!(len as usize, bytes.len() - 9);

        // Decode the payload
        let payload = bytes[9..].to_vec();
        let decoded_frame = Frame::decode(payload);
        let decoded_msg = decoded_frame.into_message().unwrap();

        assert_eq!(msg.id, decoded_msg.id);
        assert_eq!(msg.source, decoded_msg.source);
        assert_eq!(msg.channel, decoded_msg.channel);
    }

    #[tokio::test]
    async fn test_frame_reader_writer() {
        use tokio::io::duplex;

        let msg = LumiMessage::new_event(
            ProcessId::Core,
            Channel::AiState,
            serde_json::json!({"state": "thinking"}),
        )
        .unwrap();

        let frame = Frame::from_message(&msg).unwrap();
        let encoded = frame.encode();

        // Create a duplex stream for testing
        let (mut writer, mut reader) = duplex(65536);

        // Write in a background task
        let write_handle = tokio::spawn(async move {
            writer.write_all(&encoded).await.unwrap();
            writer.flush().await.unwrap();
        });

        // Read on this task
        let mut frame_reader = FrameReader::new();
        let read_frame = frame_reader.read_frame(&mut reader).await.unwrap().unwrap();
        let read_msg = read_frame.into_message().unwrap();

        write_handle.await.unwrap();

        assert_eq!(msg.id, read_msg.id);
        assert_eq!(msg.payload, read_msg.payload);
    }

    #[tokio::test]
    async fn test_eof_returns_none() {
        use tokio::io::AsyncRead;

        let empty: &[u8] = &[];
        let mut reader = empty;
        let mut frame_reader = FrameReader::new();
        let result = frame_reader.read_frame(&mut reader).await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_max_frame_size_limit() {
        let oversized = MAX_FRAME_SIZE + 1;
        let bytes = oversized.to_le_bytes();
        // Just verify the constant is reasonable
        assert!(MAX_FRAME_SIZE > 1024 * 1024);
    }
}
