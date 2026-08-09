//! aos_dot Rust reference: same AoS layout and stride tax as the C side.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..}.

use std::hint::black_box;
use std::time::Instant;

const N: usize = 100_000;

#[derive(Clone, Copy, Default)]
struct P3 {
    x: f64,
    #[allow(dead_code)]
    y: f64,
    #[allow(dead_code)]
    z: f64,
}

fn main() {
    let ops: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let mut a = vec![P3::default(); N];
    let mut b = vec![P3::default(); N];
    for i in 0..N {
        a[i] = P3 { x: i as f64, y: 1.0, z: 2.0 };
        b[i] = P3 { x: (N - i) as f64, y: 3.0, z: 4.0 };
    }
    let t = Instant::now();
    for _ in 0..ops {
        let mut dot = 0.0f64;
        for i in 0..N {
            dot += a[i].x * b[i].x;
        }
        black_box(dot);
    }
    let ns = t.elapsed().as_nanos();
    println!("{{\"ns\":{ns},\"ops\":{ops}}}");
}
