// ── Fuzz Target: Packet Parse ──────────────────────────────────────────────────
// Fuzzes Header::parse() and packet validation with arbitrary byte sequences.
// Invariant: Never panics, always returns Ok or structured Err.

#![no_main]

use libfuzzer_sys::fuzz_target;

use lumas_ipc::wire::header::Header;
use lumas_ipc::wire::protocol::HEADER_V1_SIZE;

fuzz_target!(|data: &[u8]| {
    // If the data is at least as large as a header, try parsing it
    if data.len() >= HEADER_V1_SIZE {
        let _ = Header::parse(data);
    }

    // Also test truncation paths with smaller buffers
    if data.len() < HEADER_V1_SIZE {
        let _ = Header::parse(data);
    }
});
