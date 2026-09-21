//! s174 — the `Scope`-handle-as-parameter deadlock (wolf-lang#431).
//!
//! # The prediction, written before the first measurement
//!
//! This header is committed BEFORE any run of the witness, so the
//! table below can be read against what was measured rather than
//! rewritten after it. Nothing in this file measures anything yet;
//! the gate lands in the next commit.
//!
//! The witness is the program in wolf-lang#431's body, verbatim — the
//! file trunk `2f8deb7f` removed from the corpus ("no header is true
//! on all three hosts"). It is `corpus/conc/scope_handle_param.lu` at
//! `2f8deb7f^`.
//!
//! Posture: kasumi, linux x86-64 (CachyOS), `cargo xtask dist` — the
//! shipping build with the D57 stamp, not a plain `cargo build`.
//! t03's reproduction carried exactly that caveat and this lane's
//! first act is to discharge it.
//!
//! | # | claim | falsified by |
//! |---|---|---|
//! | P1 | `conform-run --native --json` (Cranelift debug tier) **hangs**: rc 124 under `timeout 60`, 3/3 | any rc 0 |
//! | P2 | `conform-run --native --release --json` (LLVM tier) **runs**: rc 0, `exit(0)`, stdout `1`, 3/3 | any rc 124 |
//! | P3 | `conform-run --checked --json` answers `unsupported`, rc 0 | a verdict, or a hang |
//! | P4 | the dist posture does not change the answer — the D57 stamp caveat is discharged and the hang is in the build that ships | a dist-built native tier running clean 3/3 |
//! | P5 | the wait is at the **scope's join** — the `scope work { … }` exit waiting on a child that never reports done — not at `ch.recv()`, not in the parameter's move | a probe printing between the scope block and the `recv` reaches stdout under `timeout` |
//! | P6 | the mechanism is `task_env_arena` (`crates/wolf_wir/src/lower.rs:5956`) returning `Ok(None)` for any spawn receiver that is not a named `conc_scope` binding of the current function. A `Scope` **parameter** takes exactly that path, so `pack_task_env` puts the capture record in `fan_out`'s own frame and `fan_out` returns before the queued task reads it | a capture-free variant of the witness hanging just the same |
//!
//! P6 is the one that says what to fix; P5 is where to look. Both are
//! read from the source, not from a run.
