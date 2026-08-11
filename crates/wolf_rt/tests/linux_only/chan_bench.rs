//! s33 acceptance — channel ping-pong microbench (D5): rendezvous and
//! bounded-16 round trips between two tasks. Prints the numbers the
//! campaign closeout records and asserts only loose sanity ceilings
//! (CI boxes vary; judgement against the s01 C/Rust channel kernels is
//! informal by contract — M2 gates arrive with s44).
//!
//! D5 JSONL wiring is a recorded delta, same as spawn_bench: `xtask
//! bench`'s runtime track times COMPILED corpus programs, and no
//! corpus program can use channels until c05 typing + lowering land —
//! this bench joins that track when they do. `harness = false` keeps
//! the harness's own threads out of the measurement.

use std::time::{Duration, Instant};
use wolf_rt::task::{self, Chan, ExitReason};

/// One ping-pong pair over `cap`: task echoes N balls back; returns
/// time per round trip.
fn ping_pong(cap: usize, rounds: u64) -> Duration {
    let ping = Chan::new(cap);
    let pong = Chan::new(cap);
    let (ping2, pong2) = (ping.clone(), pong.clone());
    let t0 = Instant::now();
    let r = task::scope("ping-pong", |s| {
        s.spawn("echo", move |_| {
            for _ in 0..rounds {
                let v = ping2.recv().unwrap();
                pong2.send(v).unwrap();
            }
            ExitReason::Normal
        });
        for k in 0..rounds {
            ping.send(k).unwrap();
            assert_eq!(pong.recv().unwrap(), k);
        }
    });
    assert!(r.is_ok());
    t0.elapsed() / u32::try_from(rounds).unwrap()
}

pub fn main() {
    // Warm the pool out of the measurement.
    let _ = task::scope("warm", |s| {
        for _ in 0..16 {
            s.spawn("w", |_| ExitReason::Normal);
        }
    });

    const ROUNDS: u64 = 50_000;
    let rendezvous = ping_pong(0, ROUNDS);
    let bounded16 = ping_pong(16, ROUNDS);

    // Bounded-16 streaming throughput: producer fills, consumer
    // drains, no lock-step round trip.
    let ch = Chan::new(16);
    let tx = ch.clone();
    let t0 = Instant::now();
    let r = task::scope("stream", |s| {
        s.spawn("producer", move |_| {
            for k in 0..ROUNDS {
                tx.send(k).unwrap();
            }
            ExitReason::Normal
        });
        let sum: u64 = (0..ROUNDS).map(|_| ch.recv().unwrap()).sum();
        assert_eq!(sum, ROUNDS * (ROUNDS - 1) / 2);
    });
    assert!(r.is_ok());
    let stream = t0.elapsed() / u32::try_from(ROUNDS).unwrap();

    // Uncontended select over two ready arms (the tie-break path).
    let a = Chan::new(1);
    let b = Chan::new(1);
    let n: u32 = 100_000;
    let t0 = Instant::now();
    for k in 0..u64::from(n) {
        a.send(k).unwrap();
        b.send(k).unwrap();
        let one = task::select(&[task::Arm::Recv(&a), task::Arm::Recv(&b)], None, false);
        let other = task::select(&[task::Arm::Recv(&a), task::Arm::Recv(&b)], None, false);
        assert!(matches!(one, task::Selected::Recv { .. }));
        assert!(matches!(other, task::Selected::Recv { .. }));
    }
    let select_2ready = t0.elapsed() / (2 * n);

    println!(
        "chan_bench: rendezvous_rt={rendezvous:?} bounded16_rt={bounded16:?} \
         bounded16_stream={stream:?} select_2ready={select_2ready:?}"
    );

    // Sanity ceilings only (orders above the informal C/Rust kernel
    // comparison; a regression to milliseconds is a bug).
    assert!(
        rendezvous < Duration::from_micros(500),
        "rendezvous round trip blew the sanity ceiling: {rendezvous:?}"
    );
    assert!(
        stream < Duration::from_micros(100),
        "bounded-16 streaming blew the sanity ceiling: {stream:?}"
    );
}
