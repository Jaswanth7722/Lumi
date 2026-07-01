// ── Concurrency Tests ──────────────────────────────────────────────────────────
// Tests thread safety: multiple threads encoding/decoding simultaneously.
#![cfg(test)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::wire::codec::{WireCodec, WireCodecConfig};
use crate::wire::metrics::WireMetrics;

#[test]
fn test_concurrent_metrics_increment() {
    let metrics = Arc::new(WireMetrics::new());
    let thread_count = 10;
    let increments_per_thread = 1000;

    let handles: Vec<_> = (0..thread_count)
        .map(|_| {
            let m = metrics.clone();
            std::thread::spawn(move || {
                for _ in 0..increments_per_thread {
                    m.increment_frames_decoded();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let snap = metrics.snapshot();
    assert_eq!(
        snap.frames_decoded as usize,
        thread_count * increments_per_thread,
        "All concurrent increments should be visible"
    );
}

#[test]
fn test_concurrent_metrics_increment_multiple_counters() {
    let metrics = Arc::new(WireMetrics::new());
    let thread_count = 8;
    let increments_per_thread = 500;

    let handles: Vec<_> = (0..thread_count)
        .map(|i| {
            let m = metrics.clone();
            std::thread::spawn(move || {
                for _ in 0..increments_per_thread {
                    m.increment_frames_decoded();
                    if i % 2 == 0 {
                        m.increment_checksum_passes();
                    } else {
                        m.increment_checksum_failures();
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let snap = metrics.snapshot();
    assert_eq!(
        snap.frames_decoded,
        (thread_count * increments_per_thread) as u64
    );
    assert_eq!(
        snap.checksum_passes + snap.checksum_failures,
        (thread_count * increments_per_thread) as u64
    );
}

#[test]
fn test_codec_create_from_multiple_threads() {
    let thread_count = 10;
    let handles: Vec<_> = (0..thread_count)
        .map(|i| {
            std::thread::spawn(move || {
                let config = WireCodecConfig {
                    default_mtu: 1024 + i,
                    ..Default::default()
                };
                let codec = WireCodec::new(config);
                assert!(codec.config.default_mtu >= 1024);
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_metrics_clone_across_threads() {
    let metrics = Arc::new(WireMetrics::new());
    let m2 = metrics.clone();
    let m3 = metrics.clone();

    let h1 = std::thread::spawn(move || {
        m2.increment_frames_decoded();
        m2.increment_frames_decoded();
    });

    let h2 = std::thread::spawn(move || {
        m3.increment_frames_decoded();
    });

    h1.join().unwrap();
    h2.join().unwrap();

    assert!(
        metrics.snapshot().frames_decoded >= 3,
        "All threads' increments should be visible"
    );
}

#[test]
fn test_metrics_snapshot_consistency() {
    let metrics = Arc::new(WireMetrics::new());

    let h = std::thread::spawn(move || {
        metrics.increment_frames_decoded();
        let s1 = metrics.snapshot();
        metrics.increment_frames_decoded();
        let s2 = metrics.snapshot();
        assert_eq!(s2.frames_decoded, s1.frames_decoded + 1);
    });

    h.join().unwrap();
}

#[test]
fn test_metrics_no_data_races() {
    // Run this under TSAN/address sanitizer ideally,
    // but at minimum verify the concurrent access pattern.
    let metrics = Arc::new(WireMetrics::new());

    let writers: Vec<_> = (0..4)
        .map(|_| {
            let m = metrics.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    m.increment_frames_decoded();
                    m.increment_checksum_passes();
                    m.increment_fragmentation_events();
                    m.increment_reassembly_events();
                }
            })
        })
        .collect();

    let readers: Vec<_> = (0..4)
        .map(|_| {
            let m = metrics.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    let _ = m.snapshot();
                }
            })
        })
        .collect();

    for h in writers.into_iter().chain(readers) {
        h.join().unwrap();
    }
}

#[test]
fn test_metrics_high_contention() {
    let metrics = Arc::new(WireMetrics::new());
    let thread_count = 20;
    let iterations = 10_000;

    let handles: Vec<_> = (0..thread_count)
        .map(|_| {
            let m = metrics.clone();
            std::thread::spawn(move || {
                for _ in 0..iterations {
                    m.increment_frames_decoded();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let snap = metrics.snapshot();
    assert_eq!(
        snap.frames_decoded,
        (thread_count * iterations) as u64
    );
}

#[test]
fn test_metrics_all_counters_thread_safe() {
    let metrics = Arc::new(WireMetrics::new());

    let inc = |m: Arc<WireMetrics>| {
        std::thread::spawn(move || {
            m.increment_frames_decoded();
            m.increment_checksum_passes();
            m.increment_checksum_failures();
            m.increment_truncated_detected();
            m.increment_desync_events();
            m.increment_oversized_detected();
            m.increment_decompression_bombs();
            m.increment_decryption_failures();
            m.increment_fragmentation_events();
            m.increment_reassembly_events();
            m.increment_reassembly_timeouts();
        })
    };

    let handles: Vec<_> = (0..5).map(|_| inc(metrics.clone())).collect();
    for h in handles {
        h.join().unwrap();
    }

    let snap = metrics.snapshot();
    assert_eq!(snap.frames_decoded, 5);
    assert_eq!(snap.checksum_passes, 5);
    assert_eq!(snap.checksum_failures, 5);
    assert_eq!(snap.truncated_detected, 5);
    assert_eq!(snap.desync_events, 5);
    assert_eq!(snap.oversized_detected, 5);
    assert_eq!(snap.decompression_bombs, 5);
    assert_eq!(snap.decryption_failures, 5);
    assert_eq!(snap.fragmentation_events, 5);
    assert_eq!(snap.reassembly_events, 5);
    assert_eq!(snap.reassembly_timeouts, 5);
}
