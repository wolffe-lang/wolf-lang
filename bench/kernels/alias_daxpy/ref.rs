//! alias_daxpy Rust reference: &mut noalias gives the optimizer what C
//! needs `restrict` for. Protocol: argv[1]=ops; prints {"ns":..,"ops":..}.

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
        .unwrap_or(1000);
    let mut y = [0.0f64; N];
    let mut x = [0.0f64; N];
    for i in 0..N {
        x[i] = i as f64 * 0.5;
        y[i] = i as f64;
    }
    let t = Instant::now();
    for _ in 0..ops {
        daxpy(&mut y, &x, 1.000001);
    }
    let ns = t.elapsed().as_nanos();
    black_box(y[N - 1]);
    println!("{{\"ns\":{ns},\"ops\":{ops}}}");
}
