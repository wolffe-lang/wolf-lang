//! c2-ecs-sweep (family C), Rust reference: AoS `Vec<Ent>`, the idiomatic
//! shape.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::time::Instant;

const N: usize = 100_000;

#[derive(Clone, Copy, Default)]
struct Ent {
    hot: i64,
    #[allow(dead_code)]
    cold: [i64; 7],
}

fn main() {
    let ops: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1500);
    let mut es = vec![Ent::default(); N];
    for i in 0..N {
        es[i].hot = (i as i64) & 1023;
        es[i].cold = [1, 1, 1, 1, 2, 2, 2];
    }
    let t = Instant::now();
    let mut sink: i64 = 0;
    for _ in 0..ops {
        let mut acc: i64 = 0;
        for e in &es {
            acc = (acc + e.hot) & 1048575;
        }
        sink = acc;
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops.wrapping_mul(N as i64)
    );
}
