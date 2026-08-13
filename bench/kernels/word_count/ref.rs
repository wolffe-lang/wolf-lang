//! word_count = d3-split-count (family D), Rust reference:
//! `split_whitespace` is Rust's zero-copy answer, the direct analogue of
//! wolf's `words()`.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::time::Instant;

const CHUNK: &str = "the quick brown fox jumped over the lazy dogs again now  ";
const REPS: usize = 18000;

fn main() {
    let ops: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let text = CHUNK.repeat(REPS);
    let t = Instant::now();
    let mut sink: u64 = 0;
    for _ in 0..ops {
        sink = text.split_whitespace().count() as u64;
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops * text.len() as u64
    );
}
