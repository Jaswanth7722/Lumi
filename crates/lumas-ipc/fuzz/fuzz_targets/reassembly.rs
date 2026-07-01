// ── Fuzz Target: Reassembly ────────────────────────────────────────────────────
// Fuzzes the Reassembler with random fragment sequences.
// Invariant: Never panics, no unbounded memory growth.

#![no_main]

use std::sync::Arc;
use std::time::Duration;

use libfuzzer_sys::fuzz_target;
use uuid::Uuid;

use lumas_ipc::wire::fragmentation::{Reassembler, ReassemblerConfig, Fragment};
use lumas_ipc::wire::metrics::WireMetrics;

fuzz_target!(|data: &[u8]| {
    let metrics = Arc::new(WireMetrics::new());
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(60),
        max_pending_fragments: 100,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics);

    // Interpret the input as a sequence of fragment-like chunks
    let chunk_size = if data.len() > 32 { 32 } else { data.len().max(1) };
    let mut offset = 0;
    let mut index: u16 = 0;
    let total: u16 = (data.len() / chunk_size).min(u16::MAX as usize) as u16;
    let msg_id = Uuid::from_u128(0x12345678_1234_7123_8000_000000000000);

    while offset + chunk_size <= data.len() {
        let chunk = &data[offset..offset + chunk_size];
        let fragment = Fragment {
            msg_id,
            index,
            total,
            data: chunk.to_vec(),
            fragment_id: 0,
        };
        let _ = reassembler.add_fragment(fragment);

        index += 1;
        offset += chunk_size;
        if index >= total || index >= 200 {
            break; // safety limit
        }
    }

    // Run GC to clean up any stale state
    let _ = reassembler.gc();
});
