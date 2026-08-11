//! s32 acceptance — spawn/join microbench (D5). Prints the numbers
//! the campaign closeout records and asserts only a loose sanity
//! ceiling (CI boxes vary; the <1µs spawn-median gate binds on the
//! reference linux box, judged from the printed figures).
//!
//! D5 JSONL wiring is a recorded delta: `xtask bench`'s runtime track
//! times COMPILED corpus programs, and no corpus program can spawn
//! until the frontend surface lands — the bench joins that track when
//! spawn lowering does. `harness = false` keeps the harness's own
//! threads out of the measurement.

use std::time::{Duration, Instant};
use wolf_rt::task::{self, ExitReason};

fn median(mut xs: Vec<Duration>) -> Duration {
    xs.sort();
    xs[xs.len() / 2]
}

pub fn main() {
    // Warm the pool out of the measurement.
    let _ = task::scope("warm", |s| {
        for _ in 0..64 {
            s.spawn("w", |_| ExitReason::Normal);
        }
    });

    // Spawn-call latency: the push-side cost of `s.spawn` (the
    // contract's sub-microsecond path), median over 10k.
    const N: usize = 10_000;
    let mut spawn_lat = Vec::with_capacity(N);
    let r = task::scope("bench-spawn", |s| {
        for _ in 0..N {
            let t0 = Instant::now();
            s.spawn("b", |_| ExitReason::Normal);
            spawn_lat.push(t0.elapsed());
        }
    });
    assert!(r.is_ok());
    let spawn_median = median(spawn_lat);

    // The same, from ON a worker (the LIFO fast path — a task
    // spawning siblings, the common compiled shape).
    let worker_median = std::sync::Arc::new(std::sync::Mutex::new(Duration::ZERO));
    let wm = worker_median.clone();
    let r = task::scope("bench-worker-spawn", |s| {
        s.spawn("spawner", move |_| {
            let r = task::scope("inner", |s2| {
                let mut lat = Vec::with_capacity(N);
                for _ in 0..N {
                    let t0 = Instant::now();
                    s2.spawn("b", |_| ExitReason::Normal);
                    lat.push(t0.elapsed());
                }
                *wm.lock().unwrap() = median(lat);
            });
            assert!(r.is_ok());
            ExitReason::Normal
        });
    });
    assert!(r.is_ok());
    let worker_spawn_median = *worker_median.lock().unwrap();

    // Spawn+run+join throughput: whole-scope round trip per task.
    let t0 = Instant::now();
    let r = task::scope("bench-join", |s| {
        for _ in 0..N {
            s.spawn("b", |_| ExitReason::Normal);
        }
    });
    assert!(r.is_ok());
    let per_task = t0.elapsed() / N as u32;

    // Park/unpark ping: workers are parked (idle pool); one spawn
    // must wake one — time spawn → first instruction of the task.
    std::thread::sleep(Duration::from_millis(100)); // let workers park
    let mut wake_lat = Vec::new();
    for _ in 0..100 {
        let t0 = Instant::now();
        let r = task::scope("bench-wake", |s| {
            s.spawn("ping", move |_| ExitReason::Normal);
        });
        assert!(r.is_ok());
        wake_lat.push(t0.elapsed());
        std::thread::sleep(Duration::from_millis(2)); // re-park
    }
    let wake_median = median(wake_lat);

    println!(
        "spawn_bench: spawn_median={spawn_median:?} \
         worker_spawn_median={worker_spawn_median:?} \
         per_task_join={per_task:?} spawn_wake_join_median={wake_median:?}"
    );

    // Sanity ceilings only (two orders above the real gates): a
    // regression to milliseconds is a bug, tighter judgement is the
    // reference box's.
    assert!(
        spawn_median < Duration::from_micros(100),
        "spawn median blew the sanity ceiling: {spawn_median:?}"
    );
    assert!(
        per_task < Duration::from_micros(500),
        "per-task round trip blew the sanity ceiling: {per_task:?}"
    );
}
