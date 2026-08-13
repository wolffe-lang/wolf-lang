//! d2-substr-search (family D), Rust reference: sliding slice compare over
//! the same haystack (not `str::find`, per bench/protocol.md).
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;
use std::time::Instant;

const CHUNK: &str = "the pack moves at dusk and the wolf waits for nothing at all ";
const REPS: usize = 9000;
const M: usize = 5;

fn count_occurrences(hay: &[u8], needle: &[u8]) -> i64 {
    let mut hits: i64 = 0;
    let mut i = 0usize;
    while i + M <= hay.len() {
        if &hay[i..i + M] == needle {
            hits += 1;
        }
        i += 1;
    }
    hits
}

fn main() {
    let ops: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let hay = CHUNK.repeat(REPS);
    let bytes = hay.as_bytes();
    let t = Instant::now();
    let mut sink: i64 = 0;
    for _ in 0..ops {
        sink = count_occurrences(black_box(bytes), b"wolf ");
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops * bytes.len() as u64
    );
}
