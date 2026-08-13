//! e1-sum-reduce (family E), Rust reference. Release Rust checks nothing
//! (wrapping in debug is a debug_assert; `-O` uses plain `add`), so this
//! lane sits between naive C and wolf's always-checked arithmetic.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;

fn sum_reduce(n: i64) -> i64 {
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < n {
        acc = acc + (i & 1023);
        i += 1;
    }
    acc
}

fn main() {
    let ops: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4000);
    let inner: i64 = 100_000;
    let t0 = std::time::Instant::now();
    let mut sink: i64 = 0;
    for _ in 0..ops {
        sink = sink % 4096 + sum_reduce(black_box(inner)) % 4096;
    }
    let ns = t0.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops.wrapping_mul(inner)
    );
}
