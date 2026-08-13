//! b3-request-churn (family B), Rust reference: a fresh `Vec` per request.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;
use std::time::Instant;

struct Req {
    #[allow(dead_code)]
    id: i64,
    size: i64,
}

fn handle(id: i64) -> i64 {
    let mut buf: Vec<Req> = Vec::new();
    for j in 0..16i64 {
        buf.push(Req {
            id: id + j,
            size: (id + j) & 31,
        });
    }
    let mut out: i64 = 0;
    for r in &buf {
        out = (out + r.size) & 65535;
    }
    out
}

fn main() {
    let ops: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20000);
    let t = Instant::now();
    let mut sink: i64 = 0;
    for i in 0..ops {
        sink = (sink + handle(black_box(i))) & 1048575;
    }
    let ns = t.elapsed().as_nanos();
    println!("{{\"ns\":{ns},\"ops\":{ops},\"sink\":{sink}}}");
}
