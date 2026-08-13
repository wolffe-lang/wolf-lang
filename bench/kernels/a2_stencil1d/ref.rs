//! a2-stencil1d (family A), Rust reference: two distinct `&mut`/`&`
//! slices are noalias by construction.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::time::Instant;

const N: usize = 4096;

fn stencil(out: &mut [i64; N], src: &[i64; N]) {
    for i in 1..N - 1 {
        out[i] = (src[i - 1] + src[i] + src[i + 1]) & 1048575;
    }
}

fn main() {
    let ops: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20000);
    let mut src = [0i64; N];
    let mut out = [0i64; N];
    for i in 0..N {
        src[i] = (i as i64) & 255;
    }
    let t = Instant::now();
    for _ in 0..ops {
        stencil(&mut out, &src);
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{}}}",
        ops * N as u64,
        out[2048]
    );
}
