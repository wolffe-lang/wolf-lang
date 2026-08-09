//! list_alloc Rust reference: Box-per-node, the idiomatic-Rust baseline.
//! Protocol: argv[1]=ops; prints {"ns":..,"ops":..}.

use std::hint::black_box;
use std::time::Instant;

const NODES: u64 = 10_000;

struct Node {
    v: u64,
    next: Option<Box<Node>>,
}

fn main() {
    let ops: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let t = Instant::now();
    for _ in 0..ops {
        let mut head: Option<Box<Node>> = None;
        for i in 0..NODES {
            head = Some(Box::new(Node { v: i, next: head }));
        }
        let mut sum = 0u64;
        let mut cur = head.as_deref();
        while let Some(n) = cur {
            sum += n.v;
            cur = n.next.as_deref();
        }
        black_box(sum);
        // iterative drop: don't blow the stack on a 10k-deep recursive Drop
        let mut cur = head;
        while let Some(mut n) = cur {
            cur = n.next.take();
        }
    }
    let ns = t.elapsed().as_nanos();
    println!("{{\"ns\":{ns},\"ops\":{ops}}}");
}
