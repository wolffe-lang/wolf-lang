//! a5-hoist-call (family A), Rust reference (#97 redesign): `&[i64]` and
//! `&mut [i64]` are noalias, so rustc may hoist the src load across the
//! memory-writing call for the same reason wolf's `read`/`mut` modes do.
//! `black_box` launders the borrows so the disjointness the compiler
//! uses is the type-level fact, not const-propagated provenance.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;
use std::time::Instant;

fn bump(dst: &mut [i64], x: i64) -> i64 {
    dst[0] = (dst[0] + x) & 1023;
    dst[0]
}

fn probe(src: &[i64], scratch: &mut [i64], n: i64) -> i64 {
    let mut acc: i64 = 0;
    for i in 0..n {
        let a = src[0];
        let side = bump(scratch, i);
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
    let mut scr = [0i64, 0i64];
    let t = Instant::now();
    let mut sink: i64 = 0;
    for _ in 0..ops {
        sink = (sink + probe(black_box(&src), black_box(&mut scr), black_box(inner))) & 1048575;
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops.wrapping_mul(inner)
    );
}
