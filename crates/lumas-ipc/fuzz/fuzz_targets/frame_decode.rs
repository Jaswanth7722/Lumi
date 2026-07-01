// ── Fuzz Target: Frame Decode ──────────────────────────────────────────────────
// Fuzzes the LumiFramer::decode() method with arbitrary byte sequences.
// Invariant: Never panics, always returns Ok or structured Err.

#![no_main]

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use bytes::BytesMut;

use lumas_ipc::wire::frame::LumiFramer;
use lumas_ipc::wire::metrics::WireMetrics;

fuzz_target!(|data: &[u8]| {
    let metrics = Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(512 * 1024, metrics);
    let mut buf = BytesMut::from(data);

    // Decode may return None (need more data) or an error
    // The invariant is: it must never panic
    let _ = framer.decode(&mut buf);

    // Try decoding remaining bytes too (simulate multiple frames)
    let _ = framer.decode(&mut buf);
});
