// ── Benchmark: Wire Checksum ───────────────────────────────────────────────────
// Benchmarks the BLAKE3-truncated checksum vs CRC32 on various payload sizes.
// Goal: BLAKE3 must be faster than CRC32 on payload > 1KB (on modern SIMD hardware).

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_blake3_1kb(c: &mut Criterion) {
    let data = vec![0xABu8; 1024];
    let header_prefix = vec![0u8; 92];

    c.bench_function("blake3_checksum_1KB", |b| {
        b.iter(|| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(black_box(&header_prefix));
            hasher.update(black_box(&data));
            let hash = hasher.finalize();
            let checksum = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
            black_box(checksum);
        })
    });
}

fn bench_blake3_4kb(c: &mut Criterion) {
    let data = vec![0xABu8; 4096];
    let header_prefix = vec![0u8; 92];

    c.bench_function("blake3_checksum_4KB", |b| {
        b.iter(|| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(black_box(&header_prefix));
            hasher.update(black_box(&data));
            let hash = hasher.finalize();
            let checksum = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
            black_box(checksum);
        })
    });
}

fn bench_blake3_64kb(c: &mut Criterion) {
    let data = vec![0xABu8; 65536];
    let header_prefix = vec![0u8; 92];

    c.bench_function("blake3_checksum_64KB", |b| {
        b.iter(|| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(black_box(&header_prefix));
            hasher.update(black_box(&data));
            let hash = hasher.finalize();
            let checksum = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
            black_box(checksum);
        })
    });
}

fn bench_crc32_1kb(c: &mut Criterion) {
    let data = vec![0xABu8; 1024];
    let header_prefix = vec![0u8; 92];

    c.bench_function("crc32_checksum_1KB", |b| {
        b.iter(|| {
            let mut crc = crc32fast::Hasher::new();
            crc.update(black_box(&header_prefix));
            crc.update(black_box(&data));
            let checksum = crc.finalize();
            black_box(checksum);
        })
    });
}

fn bench_crc32_64kb(c: &mut Criterion) {
    let data = vec![0xABu8; 65536];
    let header_prefix = vec![0u8; 92];

    c.bench_function("crc32_checksum_64KB", |b| {
        b.iter(|| {
            let mut crc = crc32fast::Hasher::new();
            crc.update(black_box(&header_prefix));
            crc.update(black_box(&data));
            let checksum = crc.finalize();
            black_box(checksum);
        })
    });
}

fn bench_compute_checksum_engine(c: &mut Criterion) {
    let data = vec![0xABu8; 4096];
    let header_prefix = vec![0u8; 92];

    c.bench_function("checksum_engine_compute_4KB", |b| {
        b.iter(|| {
            let cs = lumi_ipc::wire::checksum::ChecksumEngine::compute(
                black_box(&header_prefix),
                black_box(&data),
            );
            black_box(cs);
        })
    });
}

criterion_group!(
    name = checksum;
    config = Criterion::default().sample_size(100);
    targets =
        bench_blake3_1kb,
        bench_blake3_4kb,
        bench_blake3_64kb,
        bench_crc32_1kb,
        bench_crc32_64kb,
        bench_compute_checksum_engine,
);
criterion_main!(checksum);
