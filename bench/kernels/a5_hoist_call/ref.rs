//! a5-hoist-call (family A), Rust reference: `&[i64]` is noalias, so
//! rustc may hoist the load across the opaque call for the same reason
//! wolf's `read` mode does.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;
use std::time::Instant;

fn opaque(depth: i64, x: i64) -> i64 {
    if depth <= 0 {
        return x & 1023;
    }
    opaque(depth - 1, x + 1) & 1023
}

fn probe(src: &[i64], n: i64) -> i64 {
    let mut acc: i64 = 0;
    for i in 0..n {
        let a = src[0];
        let side = opaque(1, i);
        let b = src[0];
        acc = (acc + a + b + side) & 1048575;
    }
    acc
}

fn main() {
    let ops: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let inner: i64 = 10000;
    let src = [7i64, 9i64];
    let t = Instant::now();
    let mut sink: i64 = 0;
    for _ in 0..ops {
        sink = (sink + probe(black_box(&src), black_box(inner))) & 1048575;
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops.wrapping_mul(inner)
    );
}
