//! word_count Rust reference: same byte-scan over the same synthetic
//! buffer (identical LCG). Protocol: argv[1]=ops; prints {"ns":..,"ops":..}.

use std::hint::black_box;
use std::time::Instant;

const LEN: usize = 1 << 20;

fn main() {
    let ops: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let mut buf = vec![0u8; LEN];
    let mut seed: u32 = 0x9e37_79b9;
    for b in buf.iter_mut() {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *b = if (seed >> 28) == 0 {
            b' '
        } else {
            b'a' + (seed % 26) as u8
        };
    }
    let t = Instant::now();
    for _ in 0..ops {
        let mut words = 0u64;
        let mut in_word = false;
        for &c in &buf {
            let ws = c == b' ' || c == b'\n' || c == b'\t';
            words += u64::from(!ws && !in_word);
            in_word = !ws;
        }
        black_box(words);
    }
    let ns = t.elapsed().as_nanos();
    println!("{{\"ns\":{ns},\"ops\":{ops}}}");
}
