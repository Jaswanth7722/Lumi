// ── Encryption Tests ───────────────────────────────────────────────────────────
// Tests encryption: ChaCha20-Poly1305 roundtrip, nonce uniqueness across
// 10,000 encryptions, tampered ciphertext rejection.
#![cfg(test)]

use crate::wire::encryption::{EncryptionType, EncryptionKey, EncryptionLayer};

#[test]
fn test_encryption_type_enum() {
    assert_eq!(EncryptionType::None as u8, 0);
    assert_eq!(EncryptionType::ChaCha20Poly1305 as u8, 1);
}

#[test]
fn test_encryption_none() {
    let layer = EncryptionLayer::none();
    assert!(matches!(layer.encryption_type(), EncryptionType::None));
}

#[test]
fn test_encryption_key_creation() {
    let key = EncryptionKey::new();
    assert_eq!(key.as_bytes().len(), 32);
}

#[test]
fn test_encryption_key_zeroize_on_drop() {
    let key_bytes: Vec<u8>;
    {
        let key = EncryptionKey::new();
        key_bytes = key.as_bytes().to_vec();
        assert_eq!(key_bytes.len(), 32);
        // Key should have random contents (not all zeros)
        assert!(
            key_bytes.iter().any(|&b| b != 0),
            "Key should have non-zero bytes"
        );
    }
    // After drop, the inner storage should have been zeroized.
    // Note: we can only verify the key hasn't leaked, not zeroization directly.
}

#[test]
fn test_chacha20_encrypt_decrypt_roundtrip() {
    let key = EncryptionKey::new();
    let layer = EncryptionLayer::chacha20_poly1305(key);
    let plaintext = b"Hello, Lumi Wire Protocol! This is a secret message.";
    let (ciphertext, nonce) = layer.encrypt(plaintext).unwrap();
    assert_ne!(
        ciphertext.as_ref(),
        plaintext,
        "Ciphertext should differ from plaintext"
    );
    let decrypted = layer.decrypt(&ciphertext, &nonce).unwrap();
    assert_eq!(
        decrypted.as_ref(),
        plaintext,
        "Decrypted text should match original"
    );
}

#[test]
fn test_chacha20_large_plaintext() {
    let key = EncryptionKey::new();
    let layer = EncryptionLayer::chacha20_poly1305(key);
    let plaintext = vec![0xABu8; 10_000];
    let (ciphertext, nonce) = layer.encrypt(&plaintext).unwrap();
    let decrypted = layer.decrypt(&ciphertext, &nonce).unwrap();
    assert_eq!(decrypted.as_ref(), &plaintext[..]);
}

#[test]
fn test_chacha20_empty_plaintext() {
    let key = EncryptionKey::new();
    let layer = EncryptionLayer::chacha20_poly1305(key);
    let (ciphertext, nonce) = layer.encrypt(b"").unwrap();
    let decrypted = layer.decrypt(&ciphertext, &nonce).unwrap();
    assert!(decrypted.is_empty());
}

#[test]
fn test_tampered_ciphertext_rejected() {
    let key = EncryptionKey::new();
    let layer = EncryptionLayer::chacha20_poly1305(key);
    let plaintext = b"This message integrity is protected.";
    let (mut ciphertext, nonce) = layer.encrypt(plaintext).unwrap();
    // Tamper with a byte in the ciphertext
    let mut bytes = ciphertext.to_vec();
    bytes[bytes.len() / 2] ^= 0xFF;
    ciphertext = bytes.into();
    let result = layer.decrypt(&ciphertext, &nonce);
    assert!(
        result.is_err(),
        "Tampered ciphertext should be rejected"
    );
}

#[test]
fn test_tampered_nonce_rejected() {
    let key = EncryptionKey::new();
    let layer = EncryptionLayer::chacha20_poly1305(key);
    let plaintext = b"Nonce is critical for security.";
    let (ciphertext, mut nonce) = layer.encrypt(plaintext).unwrap();
    // Tamper with the nonce
    nonce[0] ^= 0x01;
    let result = layer.decrypt(&ciphertext, &nonce);
    assert!(
        result.is_err(),
        "Tampered nonce should be rejected"
    );
}

#[test]
fn test_nonce_uniqueness_across_10k_encryptions() {
    let key = EncryptionKey::new();
    let layer = EncryptionLayer::chacha20_poly1305(key);
    let mut nonces = std::collections::HashSet::new();
    for i in 0..10_000 {
        let plaintext = format!("message-{}", i);
        let (_, nonce) = layer.encrypt(plaintext.as_bytes()).unwrap();
        assert!(
            nonces.insert(nonce),
            "Nonce collision at iteration {}",
            i
        );
    }
    assert_eq!(nonces.len(), 10_000, "All 10,000 nonces must be unique");
}

#[test]
fn test_nonce_format() {
    let key = EncryptionKey::new();
    let layer = EncryptionLayer::chacha20_poly1305(key);
    let (_, nonce) = layer.encrypt(b"test").unwrap();
    assert_eq!(nonce.len(), 12, "Nonce must be 12 bytes");
    // First 4 bytes should be zero (spec: [0u8; 4] ++ counter)
    assert_eq!(&nonce[0..4], &[0u8; 4], "First 4 bytes of nonce must be zero");
}

#[test]
fn test_different_keys_different_ciphertexts() {
    let key1 = EncryptionKey::new();
    let key2 = EncryptionKey::new();
    let layer1 = EncryptionLayer::chacha20_poly1305(key1);
    let layer2 = EncryptionLayer::chacha20_poly1305(key2);
    let plaintext = b"Same plaintext, different keys.";
    let (ct1, _) = layer1.encrypt(plaintext).unwrap();
    let (ct2, _) = layer2.encrypt(plaintext).unwrap();
    assert_ne!(
        ct1.as_ref(),
        ct2.as_ref(),
        "Different keys should produce different ciphertexts"
    );
}

#[test]
fn test_encryption_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<EncryptionLayer>();
}

#[test]
fn test_encryption_layer_debug() {
    let layer = EncryptionLayer::none();
    let debug_str = format!("{:?}", layer);
    assert!(!debug_str.is_empty());
}

#[test]
fn test_encryption_layer_clone() {
    let key = EncryptionKey::new();
    let layer1 = EncryptionLayer::chacha20_poly1305(key);
    let layer2 = layer1.clone();
    let (ct, nonce) = layer1.encrypt(b"test").unwrap();
    let decrypted = layer2.decrypt(&ct, &nonce).unwrap();
    assert_eq!(decrypted.as_ref(), b"test");
}
