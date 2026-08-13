//! d1-utf8-validate (family D), Rust reference: the same hand-written
//! structural scan (NOT `str::from_utf8`, which is a hand-vectorized
//! library routine — comparing a hand loop against a tuned library would
//! be a benchmarking crime, see bench/protocol.md).
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;
use std::time::Instant;

const CHUNK: &str = "wolf pack ranges wide: é and € and 🐺 keep the scan honest. ";
const REPS: usize = 15000;

fn validate(p: &[u8]) -> i64 {
    let mut want: i64 = 0;
    let mut chars: i64 = 0;
    for &b in p {
        if want > 0 {
            if b < 128 || b > 191 {
                return -1;
            }
            want -= 1;
        } else if b < 128 {
            chars += 1;
        } else if b < 194 {
            return -1;
        } else if b < 224 {
            want = 1;
            chars += 1;
        } else if b < 240 {
            want = 2;
            chars += 1;
        } else if b < 245 {
            want = 3;
            chars += 1;
        } else {
            return -1;
        }
    }
    if want == 0 { chars } else { -1 }
}

fn main() {
    let ops: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let text = CHUNK.repeat(REPS);
    let bytes = text.as_bytes();
    let t = Instant::now();
    let mut sink: i64 = 0;
    for _ in 0..ops {
        sink = validate(black_box(bytes));
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops * bytes.len() as u64
    );
}
