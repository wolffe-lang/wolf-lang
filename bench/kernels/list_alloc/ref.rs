//! list_alloc (family B), Rust reference: `Vec<Node>` is Rust's idiomatic
//! answer — one allocation, one free, which is closer to wolf's region
//! than to naive C's malloc-per-node.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}.

use std::hint::black_box;
use std::time::Instant;

struct Node {
    v: i64,
    next: i64,
}

fn build_walk(nodes: i64) -> i64 {
    let mut xs: Vec<Node> = Vec::new();
    for i in 0..nodes {
        xs.push(Node {
            v: i & 1023,
            next: i - 1,
        });
    }
    let mut sum: i64 = 0;
    let mut idx = nodes - 1;
    while idx >= 0 {
        let n = &xs[idx as usize];
        sum = (sum + n.v) & 1048575;
        idx = n.next;
    }
    sum
}

fn main() {
    let ops: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let nodes: i64 = 10000;
    let t = Instant::now();
    let mut sink: i64 = 0;
    for _ in 0..ops {
        sink = (sink + build_walk(black_box(nodes))) & 1048575;
    }
    let ns = t.elapsed().as_nanos();
    println!(
        "{{\"ns\":{ns},\"ops\":{},\"sink\":{sink}}}",
        ops.wrapping_mul(nodes)
    );
}
