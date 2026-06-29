// ── Compression Tests ──────────────────────────────────────────────────────────
// Tests the Compressor/Decompressor: compress/decompress roundtrip,
// bomb detection (synthetic oversized decompression), no-op for tiny payloads.
#![cfg(test)]

use crate::wire::compression::{CompressionType, CompressionLevel, CompressionPolicy, Compressor, Decompressor};

#[test]
fn test_compress_decompress_roundtrip() {
    let mut compressor = Compressor::new(CompressionLevel::FAST).unwrap();
    let mut decompressor = Decompressor::new().unwrap();
    let data = b"Hello, Lumi Wire Protocol! This is a test payload for compression.";
    let compressed = compressor.compress(data).unwrap();
    let decompressed = decompressor.decompress(&compressed, 1024).unwrap();
    assert_eq!(&decompressed[..], &data[..]);
}

#[test]
fn test_compress_large_payload() {
    let mut compressor = Compressor::new(CompressionLevel::DEFAULT).unwrap();
    let mut decompressor = Decompressor::new().unwrap();
    let data = vec![0xABu8; 10_000];
    let compressed = compressor.compress(&data).unwrap();
    assert!(compressed.len() < data.len(), "Compression should reduce size for repetitive data");
    let decompressed = decompressor.decompress(&compressed, 100_000).unwrap();
    assert_eq!(decompressed.len(), data.len());
    assert_eq!(&decompressed[..], &data[..]);
}

#[test]
fn test_compress_small_payload_no_growth() {
    let mut compressor = Compressor::new(CompressionLevel::FAST).unwrap();
    let data = b"small";
    let result = compressor.compress(data);
    // Small payloads may be returned as error (caller should send uncompressed)
    if let Ok(compressed) = result {
        assert!(
            compressed.len() <= data.len() + 64,
            "Compression should not grow small payloads unreasonably"
        );
    }
}

#[test]
fn test_decompression_bomb_detected() {
    let mut compressor = Compressor::new(CompressionLevel::BEST).unwrap();
    let mut decompressor = Decompressor::new().unwrap();
    // Create highly compressible data that will expand significantly
    let data = vec![0x00u8; 100_000]; // 100KB of zeros compresses to very small
    let compressed = compressor.compress(&data).unwrap();
    // Set max_output_size to something smaller than the original
    let result = decompressor.decompress(&compressed, 1000);
    assert!(
        result.is_err(),
        "Decompression bomb should be detected when max_output_size is exceeded"
    );
}

#[test]
fn test_decompression_bomb_at_max_frame_size() {
    let mut compressor = Compressor::new(CompressionLevel::BEST).unwrap();
    let mut decompressor = Decompressor::new().unwrap();
    // Create data at MAX_FRAME_SIZE
    let data = vec![0x00u8; 512 * 1024]; // 512KB
    let compressed = compressor.compress(&data).unwrap();
    // Trying to decompress with limit < original should trigger bomb detection
    let result = decompressor.decompress(&compressed, 256 * 1024);
    assert!(result.is_err(), "Decompression beyond max_output_size should be rejected");
}

#[test]
fn test_compression_type_enum() {
    assert_eq!(CompressionType::None as u8, 0);
    assert_eq!(CompressionType::Zstd as u8, 1);
}

#[test]
fn test_compression_level_default() {
    let level = CompressionLevel::default();
    assert_eq!(level.0, 3);
}

#[test]
fn test_compression_level_constants() {
    assert_eq!(CompressionLevel::FAST.0, 1);
    assert_eq!(CompressionLevel::DEFAULT.0, 3);
    assert_eq!(CompressionLevel::BEST.0, 9);
}

#[test]
fn test_compression_policy_never() {
    let policy = CompressionPolicy::Never;
    assert!(!policy.should_compress(0));
    assert!(!policy.should_compress(1000));
    assert!(!policy.should_compress(100_000));
}

#[test]
fn test_compression_policy_always() {
    let policy = CompressionPolicy::Always(CompressionLevel::FAST);
    assert!(policy.should_compress(0));
    assert!(policy.should_compress(1000));
    assert!(policy.should_compress(100_000));
}

#[test]
fn test_compression_policy_threshold() {
    let policy = CompressionPolicy::ThresholdBytes(512, CompressionLevel::FAST);
    assert!(!policy.should_compress(10));
    assert!(!policy.should_compress(511));
    assert!(policy.should_compress(512));
    assert!(policy.should_compress(1000));
}

#[test]
fn test_compressor_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Compressor>();
}

#[test]
fn test_decompressor_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Decompressor>();
}

#[test]
fn test_compress_empty_input() {
    let mut compressor = Compressor::new(CompressionLevel::FAST).unwrap();
    let data = b"";
    let result = compressor.compress(data);
    assert!(result.is_ok(), "Empty input should be compressible");
    if let Ok(compressed) = result {
        let mut decompressor = Decompressor::new().unwrap();
        let decompressed = decompressor.decompress(&compressed, 1024).unwrap();
        assert!(decompressed.is_empty());
    }
}

#[test]
fn test_compress_random_data() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut compressor = Compressor::new(CompressionLevel::FAST).unwrap();
    // Random data should not compress well
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    let mut data = Vec::with_capacity(1024);
    let mut rng = seed;
    for _ in 0..1024 {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((rng >> 32) as u8);
    }
    let compressed = compressor.compress(&data).unwrap();
    // Random data may not compress, but should not error
    let mut decompressor = Decompressor::new().unwrap();
    let decompressed = decompressor.decompress(&compressed, 4096).unwrap();
    assert_eq!(decompressed.len(), data.len());
}
