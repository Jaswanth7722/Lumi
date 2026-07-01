// ── Benchmark: Wire Encode/Decode ──────────────────────────────────────────────
// Benchmarks the full encode → decode roundtrip for various message sizes.
// Goal: encode+decode roundtrip < 10µs for 1KB message.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use lumas_ipc::wire::codec::{encode_message, decode_frame, WireCodec, WireCodecConfig};
use lumas_ipc::wire::frame::{LumiFramer, RawFrame};
use lumas_ipc::wire::metrics::WireMetrics;

fn bench_encode_small(c: &mut Criterion) {
    let codec = WireCodec::new(WireCodecConfig::default());
    let msg = lumas_ipc::message::LumiMessage::new(
        lumas_ipc::message::MessageKind::Data,
        lumas_ipc::message::MessagePayload::Empty,
    );

    c.bench_function("encode_roundtrip_small", |b| {
        b.iter(|| {
            let _ = black_box(encode_message(black_box(&msg), black_box(&codec)));
        })
    });
}

fn bench_decode_small(c: &mut Criterion) {
    let codec = WireCodec::new(WireCodecConfig::default());
    let metrics = codec.metrics();
    let raw = RawFrame::new(bytes::Bytes::from(&b"\x4C\x55\x4D\x49\x01\x01\x00\x00\x00\x00\x00\x68\x00\x00\x00\x00"[..]));

    c.bench_function("decode_small", |b| {
        b.iter(|| {
            let _ = black_box(decode_frame(black_box(raw.clone()), black_box(&codec)));
        })
    });
}

fn bench_framer_decode(c: &mut Criterion) {
    let metrics = std::sync::Arc::new(WireMetrics::new());
    let mut framer = LumiFramer::new(512 * 1024, metrics);

    // Build a valid frame
    let mut frame = bytes::BytesMut::new();
    frame.extend_from_slice(&[0x4C, 0x55, 0x4D, 0x49]); // magic
    frame.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]); // version + flags
    frame.extend_from_slice(&(104u32).to_le_bytes()); // total_length
    frame.extend_from_slice(&(0u32).to_le_bytes()); // payload_length
    frame.extend(std::iter::repeat(0u8).take(88)); // rest of header

    c.bench_function("framer_decode_valid_frame", |b| {
        b.iter(|| {
            let mut buf = frame.clone();
            let _ = black_box(framer.decode(&mut buf));
        })
    });
}

fn bench_header_parse(c: &mut Criterion) {
    let mut header_bytes = vec![0u8; 104];
    header_bytes[0..4].copy_from_slice(&[0x4C, 0x55, 0x4D, 0x49]);
    header_bytes[4] = 1;
    header_bytes[5] = 1;
    header_bytes[8..12].copy_from_slice(&104u32.to_le_bytes());

    c.bench_function("header_parse_valid", |b| {
        b.iter(|| {
            let h = lumas_ipc::wire::header::Header::parse(black_box(&header_bytes));
            black_box(h)
        })
    });
}

criterion_group!(
    name = encode_decode;
    config = Criterion::default().sample_size(100);
    targets =
        bench_encode_small,
        bench_decode_small,
        bench_framer_decode,
        bench_header_parse,
);
criterion_main!(encode_decode);
