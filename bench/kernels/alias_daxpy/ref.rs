//! alias_daxpy = a1-triad (family A), Rust reference: `&mut [f64]` is
//! noalias by construction, so Rust gets for free what C needs `restrict`
//! for — the same fact wolf's param modes carry.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;
use std::time::Instant;

const N: usize = 4096;

fn daxpy(y: &mut [f64; N], x: &[f64; N], a: f64) {
    for i in 0..N {
        y[i] += a * x[i];
    }
}

fn main() {
    let ops: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20000);
    let mut y = [0.0f64; N];
    let mut x = [0.0f64; N];
    let mut v = 0.0f64;
    let mut w = 0.0f64;
    for i in 0..N {
        x[i] = v;
        y[i] = w;
        v += 0.5;
        w += 1.0;
    }
    let t = Instant::now();
    for _ in 0..ops {
        daxpy(&mut y, &x, 1.000001);
    }
    let ns = t.elapsed().as_nanos();
    let sink = black_box(y[N - 1]);
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink:?}}}",
        ops * N as u64
    );
}
