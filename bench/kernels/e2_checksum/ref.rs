//! e2-checksum (family E), Rust reference.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;

fn checksum(n: i64, seed: i64) -> i64 {
    let mut h: i64 = seed;
    let mut i: i64 = 0;
    while i < n {
        h = (h * 31 + (i & 255)) & 1048575;
        i += 1;
    }
    h
}

fn main() {
    let ops: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4000);
    let inner: i64 = 100_000;
    let t0 = std::time::Instant::now();
    let mut sink: i64 = 1;
    for _ in 0..ops {
        sink = checksum(black_box(inner), sink);
    }
    let ns = t0.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops.wrapping_mul(inner)
    );
}
