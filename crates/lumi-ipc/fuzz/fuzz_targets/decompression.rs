// ── Fuzz Target: Decompression ─────────────────────────────────────────────────
// Fuzzes the Decompressor with arbitrary byte sequences as zstd input.
// Invariant: Never panics, including on adversarial input.

#![no_main]

use libfuzzer_sys::fuzz_target;

use lumi_ipc::wire::compression::Decompressor;

fuzz_target!(|data: &[u8]| {
    let mut decompressor = match Decompressor::new() {
        Ok(d) => d,
        Err(_) => return, // skip if decompressor can't be created
    };

    // Attempt decompression with a reasonable max_output_size
    // The invariant is: it must never panic, even on adversarial input
    let max_output = 512 * 1024; // MAX_FRAME_SIZE
    let _ = decompressor.decompress(data, max_output);
});
