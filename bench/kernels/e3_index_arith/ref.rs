//! e3-index-arith (family E), Rust reference.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;

fn walk(n: i64, depth: i64) -> i64 {
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < n {
        acc = (acc + i * 8) & 1048575;
        i += 1;
    }
    if depth > 0 {
        acc = (acc + walk(n, depth - 1)) & 1048575;
    }
    acc
}

fn main() {
    let ops: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let inner: i64 = 100_000;
    let t0 = std::time::Instant::now();
    let mut sink: i64 = 0;
    for _ in 0..ops {
        sink = (sink + walk(black_box(inner), 1)) & 1048575;
    }
    let ns = t0.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops.wrapping_mul(inner).wrapping_mul(2)
    );
}
