//! wolf-lang#713 (s231): the pool holds a worker for every task parked
//! in a runtime-owned wait, however many there are. Its own test
//! binary, so the process-wide pool is this test's alone.
//!
//! A parked task holds its thread (`[conc.task.order]`) and blocking
//! compensation adds a worker so the pool's parallelism stays whole.
//! Until s231 compensation stopped at `max(8 t, t + 4)` workers: with
//! that many tasks parked, a task spawned next waited for a worker that
//! never came, and in a wolf program the task that would have woken the
//! parked ones was exactly that task (`waitwolf loop`, 2N + 1 > cap).

#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::time::{Duration, Instant};

use wolf_rt::task::{ExitReason, Gate, counters, scope};

#[test]
fn a_task_spawned_past_the_old_cap_of_parked_tasks_still_runs() {
    scope("warm", |s| s.spawn("warm", |_| ExitReason::Normal)).expect("warm");
    let (target, ..) = counters();
    assert!(target >= 2, "the pool's target is at least two: {target}");
    let old_cap = (target * 8).max(target + 4);
    // Twice the old cap parked at once.
    let parked = 2 * old_cap;
    let gate = Arc::new(Gate::new());
    let ran = Arc::new(AtomicBool::new(false));
    let mut ran_in_time = false;
    let mut waited = Duration::ZERO;
    let mut seen = (0, 0, 0);
    scope("grow", |s| {
        for i in 0..parked {
            let g = gate.clone();
            s.spawn(&format!("parked{i}"), move |ctx| {
                g.wait(ctx);
                ExitReason::Normal
            });
        }
        let r = ran.clone();
        s.spawn("late", move |_| {
            r.store(true, SeqCst);
            ExitReason::Normal
        });
        let t0 = Instant::now();
        while !ran.load(SeqCst) && t0.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(2));
        }
        waited = t0.elapsed();
        ran_in_time = ran.load(SeqCst);
        seen = counters();
        // Free the parked tasks either way, so the scope joins.
        gate.open();
    })
    .expect("the scope joins once the gate opens");
    eprintln!(
        "pool_growth: target {target}, old cap {old_cap}, {parked} parked; late task ran={} \
         after {waited:?}; (target, running, unblocked) at the deadline = {seen:?}",
        ran_in_time
    );
    assert!(
        ran_in_time,
        "a task spawned beside {parked} parked tasks (target {target}, old cap {old_cap}) \
         never ran in {waited:?}: compensation stopped at a cap"
    );
    assert!(
        seen.1 >= parked,
        "{} workers for {parked} parked tasks: one is owed to each",
        seen.1
    );
}
