//! The cancellation architecture, tested at the contract surface
//! (clause 3, frozen for s57): a write cancels in-flight snapshot
//! reads, blocks until they drop, and the cancelled read surfaces as
//! `Err(Cancelled)` — never a hang, never a panic escaping the crate.
//!
//! Own test binary: the slow-query env knob is process-wide.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use wolf_query::{Change, QueryHost};

#[test]
fn write_cancels_readers_and_blocks_until_they_drop() {
    // SAFETY: set before any threads spawn in this test binary.
    unsafe { std::env::set_var(wolf_query::TEST_SLOW_ENV, "30000") };

    let dir = std::env::temp_dir().join(format!("wolf_query_cancel_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.lu");

    let host = Arc::new(QueryHost::new());
    host.apply_change(Change::Open {
        path: path.clone(),
        text: b"fn main() -> int {\n    0\n}\n".to_vec(),
    });
    assert_eq!(host.revision(), 1);

    // Reader: starts a (test-slowed) query on its own snapshot.
    let reader_done = Arc::new(AtomicBool::new(false));
    let reader = {
        let host = Arc::clone(&host);
        let path = path.clone();
        let done = Arc::clone(&reader_done);
        std::thread::spawn(move || {
            let snapshot = host.snapshot();
            let result = snapshot.diagnostics(&path);
            done.store(true, Ordering::SeqCst);
            result
        })
    };
    // Give the reader time to enter its slow first checkpoint.
    std::thread::sleep(Duration::from_millis(100));
    assert!(!reader_done.load(Ordering::SeqCst), "reader is in flight");

    // Writer: must cancel the reader, block until its snapshot drops,
    // and return long before the 30 s the query would have dawdled.
    let started = Instant::now();
    host.apply_change(Change::Edit {
        path: path.clone(),
        text: b"fn main() -> int {\n    1\n}\n".to_vec(),
    });
    let write_latency = started.elapsed();
    assert!(
        reader_done.load(Ordering::SeqCst),
        "apply_change returned only after the reader's snapshot dropped"
    );
    assert!(
        write_latency < Duration::from_secs(10),
        "the write did not wait out the slow query: {write_latency:?}"
    );

    let result = reader
        .join()
        .expect("reader thread survives (no panic escapes)");
    assert!(
        result.is_err(),
        "the cancelled read surfaced Err(Cancelled)"
    );

    // The pipeline is intact: a fresh snapshot answers on the new text.
    unsafe { std::env::set_var(wolf_query::TEST_SLOW_ENV, "0") };
    let snapshot = host.snapshot();
    let batch = snapshot
        .diagnostics(&path)
        .expect("not cancelled")
        .expect("readable");
    assert!(batch.diagnostics.is_empty(), "{:?}", batch.diagnostics);
    assert_eq!(host.revision(), 2);

    std::fs::remove_dir_all(&dir).ok();
}
