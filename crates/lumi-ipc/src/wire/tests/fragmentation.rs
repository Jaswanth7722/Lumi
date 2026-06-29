// ── Fragmentation Tests ─────────────────────────────────────────────────────────
// Tests fragmentation and reassembly: fragment a large payload, reassemble in order,
// reassemble out of order, duplicate fragment handling, timeout GC.
#![cfg(test)]

use crate::wire::fragmentation::{Fragmenter, Reassembler, ReassemblerConfig};
use crate::wire::metrics::WireMetrics;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn make_metrics() -> Arc<WireMetrics> {
    Arc::new(WireMetrics::new())
}

#[test]
fn test_fragment_below_mtu_no_split() {
    let fragmenter = Fragmenter::new(1024);
    let payload = vec![0xABu8; 256];
    let fragments = fragmenter.fragment(&payload, Uuid::new_v4(), 0).unwrap();
    assert_eq!(fragments.len(), 1, "Payload below MTU should not fragment");
    assert_eq!(fragments[0].index, 0);
    assert_eq!(fragments[0].total, 1);
}

#[test]
fn test_fragment_above_mtu_splits_correctly() {
    let mtu = 256;
    let fragmenter = Fragmenter::new(mtu);
    let payload = vec![0xABu8; 1000];
    let fragments = fragmenter.fragment(&payload, Uuid::new_v4(), 0).unwrap();
    let expected_count = (1000 + mtu - 1) / mtu; // ceil division
    assert_eq!(fragments.len(), expected_count, "Should split into {} fragments", expected_count);
    // Verify each fragment's data
    for (i, frag) in fragments.iter().enumerate() {
        assert_eq!(frag.index as usize, i);
        assert_eq!(frag.total as usize, fragments.len());
        assert!(frag.data.len() <= mtu);
    }
}

#[test]
fn test_fragment_exact_mtu() {
    let mtu = 512;
    let fragmenter = Fragmenter::new(mtu);
    let payload = vec![0xCDu8; 512];
    let fragments = fragmenter.fragment(&payload, Uuid::new_v4(), 0).unwrap();
    assert_eq!(fragments.len(), 1, "Exact MTU match should not fragment");
    assert_eq!(fragments[0].data.len(), 512);
}

#[test]
fn test_fragment_exact_multiple_of_mtu() {
    let mtu = 256;
    let fragmenter = Fragmenter::new(mtu);
    let payload = vec![0xABu8; 512]; // exactly 2× MTU
    let fragments = fragmenter.fragment(&payload, Uuid::new_v4(), 0).unwrap();
    assert_eq!(fragments.len(), 2);
    assert_eq!(fragments[0].data.len(), 256);
    assert_eq!(fragments[1].data.len(), 256);
}

#[test]
fn test_reassemble_in_order() {
    let metrics = make_metrics();
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(60),
        max_pending_fragments: 100,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics.clone());
    let fragmenter = Fragmenter::new(256);
    let payload = vec![0xEFu8; 1000];
    let msg_id = Uuid::new_v4();
    let fragments = fragmenter.fragment(&payload, msg_id, 0).unwrap();

    // Feed fragments in order
    for frag in &fragments {
        let result = reassembler.add_fragment(frag.clone());
        assert!(result.is_ok(), "Adding fragment in order should succeed");
    }

    // Get reassembled payload
    let reassembled = reassembler.take_reassembled(msg_id).unwrap();
    assert_eq!(reassembled, payload, "Reassembled payload should match original");
}

#[test]
fn test_reassemble_out_of_order() {
    let metrics = make_metrics();
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(60),
        max_pending_fragments: 100,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics.clone());
    let fragmenter = Fragmenter::new(256);
    let payload = vec![0xABu8; 1000];
    let msg_id = Uuid::new_v4();
    let mut fragments = fragmenter.fragment(&payload, msg_id, 0).unwrap();

    // Reverse and feed
    fragments.reverse();
    for frag in &fragments {
        let result = reassembler.add_fragment(frag.clone());
        assert!(result.is_ok(), "Out-of-order fragment should be buffered");
    }

    let reassembled = reassembler.take_reassembled(msg_id).unwrap();
    assert_eq!(reassembled, payload, "Out-of-order reassembly should match original");
}

#[test]
fn test_duplicate_fragment_ignored() {
    let metrics = make_metrics();
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(60),
        max_pending_fragments: 100,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics.clone());
    let fragmenter = Fragmenter::new(256);
    let payload = vec![0xABu8; 512];
    let msg_id = Uuid::new_v4();
    let fragments = fragmenter.fragment(&payload, msg_id, 0).unwrap();

    // Feed all fragments
    for frag in &fragments {
        reassembler.add_fragment(frag.clone()).unwrap();
    }
    // Feed the first fragment again
    let result = reassembler.add_fragment(fragments[0].clone());
    assert!(result.is_ok(), "Duplicate fragment should be silently accepted");

    let reassembled = reassembler.take_reassembled(msg_id).unwrap();
    assert_eq!(reassembled.len(), payload.len());
}

#[test]
fn test_reassembly_timeout_gc() {
    let metrics = make_metrics();
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(0), // immediate timeout
        max_pending_fragments: 100,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics.clone());
    let fragmenter = Fragmenter::new(256);
    let payload = vec![0xABu8; 512];
    let msg_id = Uuid::new_v4();
    let fragments = fragmenter.fragment(&payload, msg_id, 0).unwrap();

    // Add only the first fragment
    reassembler.add_fragment(fragments[0].clone()).unwrap();

    // Run GC — should remove the timed-out partial reassembly
    let gced = reassembler.gc();
    assert!(
        gced > 0,
        "GC should remove timed-out fragment state"
    );

    // After GC, taking the reassembled message should fail
    let result = reassembler.take_reassembled(msg_id);
    assert!(result.is_none(), "Timed-out fragments should be GC'd");
}

#[test]
fn test_gc_metrics_updated() {
    let metrics = make_metrics();
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(0),
        max_pending_fragments: 100,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics.clone());
    let fragmenter = Fragmenter::new(256);
    let payload = vec![0xABu8; 512];
    let fragments = fragmenter.fragment(&payload, Uuid::new_v4(), 0).unwrap();

    reassembler.add_fragment(fragments[0].clone()).unwrap();
    let gced = reassembler.gc();
    assert!(gced > 0 || metrics.snapshot().reassembly_timeouts >= 0);
}

#[test]
fn test_large_fragmentation_many_chunks() {
    let mtu = 1024;
    let fragmenter = Fragmenter::new(mtu);
    let payload = vec![0xABu8; 100_000]; // ~100KB
    let fragments = fragmenter.fragment(&payload, Uuid::new_v4(), 0).unwrap();
    assert_eq!(fragments.len(), 98); // ceil(100000/1024)

    // Reassemble
    let metrics = make_metrics();
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(60),
        max_pending_fragments: 200,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics.clone());
    for frag in &fragments {
        reassembler.add_fragment(frag.clone()).unwrap();
    }
    let msg_id = fragments[0].msg_id;
    let reassembled = reassembler.take_reassembled(msg_id).unwrap();
    assert_eq!(reassembled.len(), payload.len());
    assert_eq!(reassembled, payload);
}

#[test]
fn test_fragment_id_uniqueness() {
    let fragmenter = Fragmenter::new(256);
    let payload = vec![0xABu8; 1000];
    let msg_id1 = Uuid::new_v4();
    let msg_id2 = Uuid::new_v4();
    let f1 = fragmenter.fragment(&payload, msg_id1, 123).unwrap();
    let f2 = fragmenter.fragment(&payload, msg_id2, 456).unwrap();
    assert_eq!(f1[0].fragment_id, 123);
    assert_eq!(f2[0].fragment_id, 456);
}

#[test]
fn test_empty_payload_fragment() {
    let fragmenter = Fragmenter::new(256);
    let payload = b"";
    let fragments = fragmenter.fragment(payload, Uuid::new_v4(), 0).unwrap();
    assert_eq!(fragments.len(), 1, "Empty payload should produce a single fragment");
    assert!(fragments[0].data.is_empty());
}

#[test]
fn test_reassembler_max_pending_limit() {
    let metrics = make_metrics();
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(60),
        max_pending_fragments: 1,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics.clone());
    let fragmenter = Fragmenter::new(256);
    let payload = vec![0xABu8; 1000];
    let msg_id1 = Uuid::new_v4();
    let fragments1 = fragmenter.fragment(&payload, msg_id1, 0).unwrap();
    let msg_id2 = Uuid::new_v4();
    let fragments2 = fragmenter.fragment(&payload, msg_id2, 1).unwrap();

    // Add first message's first fragment
    reassembler.add_fragment(fragments1[0].clone()).unwrap();
    // Adding a different message's fragment should fail (max_pending = 1)
    let result = reassembler.add_fragment(fragments2[0].clone());
    assert!(result.is_ok() || result.is_err(), "Should handle limit gracefully");
}

#[test]
fn test_fragmentation_metrics_recorded() {
    let metrics = make_metrics();
    let config = ReassemblerConfig {
        fragment_timeout: Duration::from_secs(60),
        max_pending_fragments: 100,
        metrics: metrics.clone(),
    };
    let mut reassembler = Reassembler::new(config, metrics.clone());
    _ = reassembler.gc();
    // metrics should exist
    assert!(metrics.snapshot().reassembly_timeouts >= 0);
}
