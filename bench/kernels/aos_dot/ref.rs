//! aos_dot (family C), Rust reference: idiomatic Rust is AoS too — `repr`
//! is not the ABI, but `Vec<P3>` is what people write.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::time::Instant;

const N: usize = 100_000;

#[derive(Clone, Copy)]
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
        .unwrap_or(1500);
    let mut a = vec![
        P3 {
            x: 0.0,
            y: 1.0,
            z: 2.0
        };
        N
    ];
    let mut b = vec![
        P3 {
            x: 0.0,
            y: 3.0,
            z: 4.0
        };
        N
    ];
    let mut va = 0.0f64;
    let mut vb = N as f64;
    for i in 0..N {
        a[i].x = va;
        b[i].x = vb;
        va += 1.0;
        vb -= 1.0;
    }
    let t = Instant::now();
    let mut sink = 0.0f64;
    for _ in 0..ops {
        let mut acc = 0.0f64;
        for i in 0..N {
            acc += a[i].x * b[i].x;
        }
        sink = acc;
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink:?}}}",
        ops * N as u64
    );
}
