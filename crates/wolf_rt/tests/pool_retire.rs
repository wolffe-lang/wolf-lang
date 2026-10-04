//! wolf-lang#570 (s210): workers blocked in runtime-owned waits must not
//! strand the pool. Its own test binary, so the process-wide pool is
//! this test's alone.
//!
//! Blocking compensation adds an extra worker for each one that blocks.
//! At 0.2.23 an idle extra retired after 50 ms whenever `running >
//! target`, and `running` counts the BLOCKED workers. So once the
//! extras idled out, the pool held only blocked workers and no
//! free one. A task spawned then waited for a worker that never came.
//! A wolf program met this as procs parked in `net_write` starving the
//! next proc (lobo#46 and ws49; on 1 cpu one parked proc was enough,
//! because each proc holds two workers, proc-main and its reaper).

#![cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::time::{Duration, Instant};

use wolf_rt::task::{ExitReason, Gate, counters, scope};

#[test]
fn a_task_spawned_after_the_extras_idle_still_runs() {
    // Bring the pool up so `counters()` names its target.
    scope("warm", |s| s.spawn("warm", |_| ExitReason::Normal)).expect("warm");
    let (target, ..) = counters();
    assert!(target >= 2, "the pool's target is at least two: {target}");
    let blocked = 3 * target;
    let gate = Arc::new(Gate::new());
    let ran = Arc::new(AtomicBool::new(false));
    let mut waited = Duration::ZERO;
    // Read at the deadline, BEFORE the gate opens: once it opens the
    // freed workers run the late task anyway, and `ran` alone would
    // pass a starved pool.
    let mut ran_in_time = false;
    scope("retire", |s| {
        // Three times the target, well under the 8x cap: with that many
        // queued, every core worker takes one (core workers never
        // retire, so one left free would hide the defect), and the
        // compensating extras take the rest.
        for i in 0..blocked {
            let g = gate.clone();
            s.spawn(&format!("blocked{i}"), move |ctx| {
                g.wait(ctx);
                ExitReason::Normal
            });
        }
        // Let the idle extras pass their 50 ms retire window many times.
        std::thread::sleep(Duration::from_millis(400));
        let r = ran.clone();
        s.spawn("late", move |_| {
            r.store(true, SeqCst);
            ExitReason::Normal
        });
        let t0 = Instant::now();
        while !ran.load(SeqCst) && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(2));
        }
        waited = t0.elapsed();
        ran_in_time = ran.load(SeqCst);
        let seen = counters();
        // Free the blocked tasks either way, so the scope joins.
        gate.open();
        eprintln!(
            "pool_retire: target {target}, {} blocked; late task ran={} after {waited:?}; \
             (target, running, unblocked) at the deadline = {seen:?}",
            blocked, ran_in_time
        );
    })
    .expect("the scope joins once the gate opens");
    assert!(
        ran_in_time,
        "a task spawned beside {blocked} blocked workers (target {target}) never ran in \
         {waited:?}: the idle extras retired with every remaining worker blocked"
    );
}
