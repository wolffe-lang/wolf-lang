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
//!
//! # Why this is a driver test and not a corpus entry
//!
//! Because no corpus posture is true on all three hosts, and a
//! `phase:` header that is false on any of them is worse than none.
//! `xtask corpus` compares the declared phase for EXACT equality
//! against the deepest passing phase: `phase: run` hangs the native
//! lane, and anything shallower is false on the hosts where the file
//! does reach `run`. `member: true` collides `main` with every other
//! `corpus/conc/*.lu` under D59. That is why trunk `2f8deb7f` removed
//! the file rather than re-homing it, and why the gate lives here.
//!
//! # And why the gate is structural rather than behavioural
//!
//! **The hang is not reproducible under `cargo test`.** s174 measured
//! the same fixture on one host (kasumi, linux x86-64) with three
//! builds of the same commit:
//!
//! | build of `wolf` | `conform-run --native` |
//! |---|---|
//! | `target/debug` (what every `cargo test` uses) | `exit(0)`, stdout `1` — 3/3 |
//! | `target/release` | rc 124 at 30 s — 3/3 |
//! | `cargo xtask dist` (the archive that ships) | rc 124 at 30 s — 3/3 |
//!
//! The defect is a dangling stack slot, so whether it is *observable*
//! depends on whether anything has overwritten the dead frame before
//! the task reads it — which changes with the runtime's own frame
//! sizes, and therefore with the profile `libwolf_rt.a` was built in,
//! and with the host. On linux the debug profile is lucky and the
//! shipping profile is not; on windows s170 saw it hang under the
//! debug profile, which is how wolf-lang#431 was filed at all.
//!
//! This has a consequence worth stating plainly: **`xtask
//! lane-coverage` builds `target/debug/wolf`**, so the corpus entry
//! trunk removed could never have gated this on linux either. It
//! would have been green there while the shipping compiler
//! deadlocked.
//!
//! So the load-bearing gate here is
//! [`the_capture_record_outlives_the_scope_it_is_spawned_into`], which
//! reads the compiler's own `--emit=wir` output and cannot be lucky.
//! The two behavioural tests below refuse to run at all unless a
//! release-profile `wolf` is present, because a green from the debug
//! profile is not evidence about this program and must not be
//! recorded as one.
//!
//! The fixtures are `tests/fixtures/scope_handle_param/main.lu` (the
//! issue's program, as the corpus carried it) and
//! `tests/fixtures/scope_handle_param_capture/main.lu` (the same
//! defect with a value to read instead of a deadlock to wait out).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .join("main.lu")
}

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("wolf-431-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// The native lanes link `libwolf_rt.a` found next to the `wolf`
/// binary, and `cargo test` alone does not produce the staticlib on a
/// fresh target — `conc_native.rs` carries the same helper for the
/// same reason (CI's release-parity lane once skipped silently on it).
fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

// ---- the gate: the compiler's own output ------------------------------

/// The function bodies of a `--emit=wir` dump, in order, as
/// `(name, body)`.
fn wir_functions(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, String)> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("fn @") {
            if let Some(f) = cur.take() {
                out.push(f);
            }
            let name = rest
                .split(['(', ' '])
                .next()
                .unwrap_or_default()
                .to_string();
            cur = Some((name, String::new()));
        } else if line == "}" {
            if let Some(f) = cur.take() {
                out.push(f);
            }
        } else if let Some((_, body)) = cur.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(f) = cur.take() {
        out.push(f);
    }
    out
}

/// Is `val` defined by a `stack.alloc` in this body? Matched on the
/// DEFINING line, so `%3` never matches `%30`.
fn defined_by_stack_alloc(body: &str, val: &str) -> bool {
    body.lines().any(|l| {
        let l = l.trim_start();
        l.contains("stack.alloc")
            && (l.starts_with(&format!("{val}:"))
                || l.starts_with(&format!("{val} "))
                || l.starts_with(&format!("{val},")))
    })
}

/// The env operand of `__wolf_rt_scope_spawn`: the frozen ABI's third
/// argument, `(scope, entry, env, name, name_len)`.
fn spawn_env_operands(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|l| l.split_once("call @__wolf_rt_scope_spawn(").map(|(_, a)| a))
        .filter_map(|a| a.split(')').next())
        .filter_map(|a| a.split(',').nth(2).map(|v| v.trim().to_string()))
        .collect()
}

/// **wolf-lang#431.** A task's capture record must live in a frame
/// that outlives the scope it is spawned into.
///
/// `pack_task_env` puts the record in a frame slot when the spawn is
/// straight-line, on a premise written into `crates/wolf_wir/src/
/// lower.rs` at s86: *"a frame slot when the spawn is straight-line
/// (the scope joins before the frame dies)"*. That premise holds when
/// the spawning function is the one that opened the scope. It is false
/// when the handle arrives as a PARAMETER — `[conc.task.scope]`'s
/// normative sentence, which s170 (#316) made writable for the first
/// time — because the scope joins in the caller and the callee has
/// already returned.
///
/// So the invariant, read straight off `--emit=wir`: a function that
/// hands `__wolf_rt_scope_spawn` an env it allocated with
/// `stack.alloc` must be a function that also joins that scope. At
/// trunk `2f8deb7f` the dump reads
///
/// ```text
/// fn @fan_out(ptr, i64) {
///   %3: ptr, %4: mem.r0 = stack.alloc %2
///   %5 = store.i64 %1, %3, %4
///   %9 = call @__wolf_rt_scope_spawn(%0, %6, %3, %7, %8, %5)
///   ret
/// }
/// ```
///
/// — the record is `%3`, in `@fan_out`'s frame, and the matching
/// `__wolf_rt_scope_join_free` is over in `@main`.
///
/// This is the load-bearing gate because it is the only one that
/// cannot be lucky: it is the same text on every host and in every
/// profile, and it answers in milliseconds.
#[test]
fn the_capture_record_outlives_the_scope_it_is_spawned_into() {
    let out = scratch().join("capture.wir");
    let st = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(fixture("scope_handle_param_capture"))
        .arg("--emit=wir")
        .arg("-o")
        .arg(&out)
        .status()
        .expect("wolf build --emit=wir runs");
    assert!(
        st.success(),
        "the fixture must lower (it is a legal program)"
    );
    let text = std::fs::read_to_string(&out).expect("the wir dump is written");

    let fns = wir_functions(&text);
    assert!(
        fns.iter().any(|(_, b)| b.contains("__wolf_rt_scope_spawn")),
        "the dump must contain the spawn this gate is about — it does not, \
         so the gate is reading the wrong thing:\n{text}"
    );

    let mut dangling = Vec::new();
    let mut checked = 0usize;
    for (name, body) in &fns {
        for env in spawn_env_operands(body) {
            checked += 1;
            if defined_by_stack_alloc(body, &env) && !body.contains("__wolf_rt_scope_join_free") {
                dangling.push(format!(
                    "@{name}: env {env} is a stack.alloc in a frame that does not join the scope"
                ));
            }
        }
    }
    assert!(checked > 0, "no spawn env operand was read out of the dump");
    assert!(
        dangling.is_empty(),
        "wolf-lang#431 — the task capture record dies with the frame that \
         packed it: {dangling:?}\n\nThe fix must put the record somewhere \
         the scope outlives — the scope's own arena, reached through the \
         handle — which the five-parameter `__wolf_rt_scope_spawn` ABI \
         cannot express today because it carries no env size.\n\n{text}"
    );
}

// ---- the behavioural witnesses, on the profile that ships -------------

/// A release-profile `wolf` with its runtime beside it, or `None`.
///
/// Deliberately NOT `CARGO_BIN_EXE_wolf`. The debug profile answers
/// `exit(0)` on this program on linux and would record a green that
/// says nothing — the module header has the three-build table. These
/// tests run after `cargo build --release -p wolf_driver -p wolf_rt`
/// or `cargo xtask dist`, and skip loudly otherwise.
fn release_wolf() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let exe = root.join(format!(
        "target/release/wolf{}",
        std::env::consts::EXE_SUFFIX
    ));
    let rt = root.join(if cfg!(windows) {
        "target/release/wolf_rt.lib"
    } else {
        "target/release/libwolf_rt.a"
    });
    if exe.is_file() && rt.is_file() {
        Some(exe)
    } else {
        eprintln!(
            "SKIP: no release-profile wolf at {} — a debug-profile green is \
             not evidence about wolf-lang#431 (see the module header), so \
             this witness does not run. `cargo xtask dist` produces it.",
            exe.display()
        );
        None
    }
}

/// The cap. `cargo xtask lane-coverage` kills a corpus observation
/// that exceeds 60 s and names the file and the lane; this is the same
/// number deliberately, so a lane that is slow-but-alive reads the
/// same here as it does there.
const CAP: Duration = Duration::from_secs(60);

struct Bounded {
    hung: bool,
    elapsed: Duration,
    stdout: String,
    stderr: String,
}

/// One `wolf` invocation, bounded by [`CAP`], reporting whether it
/// reached its own exit.
///
/// Two details are load-bearing. Output goes to FILES, never to pipes:
/// a test that read a pipe while polling could block on a full buffer
/// and report a hang that was its own. And the child gets its own
/// process group, because `conform-run` runs the compiled program as a
/// CHILD of `wolf` — killing only `wolf` would leave the deadlocked
/// program behind as an orphan, which is the very thing under test.
fn bounded(wolf: &Path, args: &[&str]) -> Bounded {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = scratch();
    let (op, ep) = (dir.join(format!("{n}.out")), dir.join(format!("{n}.err")));
    let mut cmd = Command::new(wolf);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            std::fs::File::create(&op).expect("stdout file"),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&ep).expect("stderr file"),
        ));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().expect("wolf runs");
    let t0 = Instant::now();
    let mut hung = true;
    while t0.elapsed() < CAP {
        if child.try_wait().expect("try_wait").is_some() {
            hung = false;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let elapsed = t0.elapsed();
    if hung {
        kill_tree(&mut child);
        let _ = child.wait();
    }
    Bounded {
        hung,
        elapsed,
        stdout: std::fs::read_to_string(&op).unwrap_or_default(),
        stderr: std::fs::read_to_string(&ep).unwrap_or_default(),
    }
}

fn kill_tree(child: &mut Child) {
    let pid = child.id();
    #[cfg(unix)]
    {
        // SAFETY: a negative pid signals the whole process group, and
        // `process_group(0)` made this child that group's leader.
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// A lane this HOST does not serve refuses BY NAME and is a loud skip,
/// not a verdict — the s59 pattern every conc test uses, so this gate
/// starts passing the moment a port lands rather than needing an edit.
/// A skip is only ever read off a record that ARRIVED: a lane that hung
/// is a failure whatever its stderr would have said.
fn refused_by_name(b: &Bounded) -> bool {
    [
        "native codegen targets",
        "windows-native serves no",
        "release tier targets linux/x86-64",
    ]
    .iter()
    .any(|m| b.stderr.contains(m))
}

/// **wolf-lang#431**, as the program behaves: every lane REACHES a
/// verdict, and the lanes that run the program answer what the
/// reference machine pinned.
///
/// A lane that never returns is named here with the seconds it burned
/// instead of taking the whole job down with it — before s170's
/// per-observation kill this program consumed an entire windows CI job
/// and left a 24-minute gap in the log with no file name in it.
///
/// The expected answers are the ones the REMOVED corpus header carried
/// (`run(exit=0, stdout="1")`), never another lane's answer: equality
/// between two lanes passes when both are wrong together, and on this
/// program the LLVM tier is right by luck about frame reuse rather
/// than by construction.
#[test]
fn every_lane_bounds_a_scope_handle_passed_as_a_parameter() {
    let Some(wolf) = release_wolf() else { return };
    ensure_rt_staticlib();
    let f = fixture("scope_handle_param");
    let f = f.to_str().expect("utf-8 fixture path");

    // (lane, flags, expected verdict, expected stdout_inline)
    let lanes: [(&str, &[&str], &str, Option<&str>); 4] = [
        ("default", &[], "pass", None),
        ("checked", &["--checked"], "unsupported", None),
        ("native", &["--native"], "exit(0)", Some("1\n")),
        (
            "release",
            &["--native", "--release"],
            "exit(0)",
            Some("1\n"),
        ),
    ];

    let mut hung: Vec<String> = Vec::new();
    let mut wrong: Vec<String> = Vec::new();
    let (mut answered, mut skipped) = (0usize, 0usize);

    for (lane, flags, want_verdict, want_stdout) in lanes {
        let mut args: Vec<&str> = vec!["conform-run", f, "--json"];
        args.extend_from_slice(flags);
        let b = bounded(&wolf, &args);
        if b.hung {
            hung.push(format!("{lane} (killed at {}s)", b.elapsed.as_secs()));
            continue;
        }
        if refused_by_name(&b) {
            eprintln!(
                "SKIP: the {lane} lane refuses this host by name: {}",
                b.stderr.trim()
            );
            skipped += 1;
            continue;
        }
        let rec: serde_json::Value = match serde_json::from_str(&b.stdout) {
            Ok(r) => r,
            Err(e) => {
                wrong.push(format!(
                    "{lane}: no observation record ({e}): {}",
                    b.stdout.trim()
                ));
                continue;
            }
        };
        let got = rec["verdict"].as_str().unwrap_or("<none>").to_string();
        if got != want_verdict {
            wrong.push(format!("{lane}: verdict {got}, want {want_verdict}"));
        }
        if let Some(want) = want_stdout {
            let out = rec["stdout_inline"].as_str().unwrap_or("<none>");
            if out != want {
                wrong.push(format!("{lane}: stdout {out:?}, want {want:?}"));
            }
        }
        answered += 1;
    }

    assert!(
        hung.is_empty(),
        "wolf-lang#431 — a `Scope` handle passed as a parameter never \
         returns on: {hung:?}. The task's capture record is read after the \
         frame that packed it is gone; the sibling WIR gate shows which \
         frame."
    );
    assert!(wrong.is_empty(), "wolf-lang#431 — wrong answers: {wrong:?}");
    assert_eq!(
        answered + skipped,
        4,
        "every lane was observed (answered {answered}, skipped {skipped})"
    );
}

/// The same defect with a value to read instead of a deadlock to wait
/// out: the task must print `task-7`, not an address.
///
/// Where the deadlock costs 60 s and says only that something never
/// finished, this answers in milliseconds and says WHAT — at trunk the
/// native tier prints `task-94521888960640`, a different number every
/// run, because the slot has been reused by then.
#[test]
fn a_task_capture_survives_a_scope_handle_passed_as_a_parameter() {
    let Some(wolf) = release_wolf() else { return };
    ensure_rt_staticlib();
    let f = fixture("scope_handle_param_capture");
    let f = f.to_str().expect("utf-8 fixture path");
    let mut checked = 0usize;
    for (lane, flags) in [
        ("native", &["--native"][..]),
        ("release", &["--native", "--release"][..]),
    ] {
        let mut args: Vec<&str> = vec!["conform-run", f, "--json"];
        args.extend_from_slice(flags);
        let b = bounded(&wolf, &args);
        assert!(
            !b.hung,
            "{lane}: the capture witness must not hang (wolf-lang#431)"
        );
        if refused_by_name(&b) {
            eprintln!("SKIP: the {lane} lane refuses this host by name");
            continue;
        }
        let rec: serde_json::Value =
            serde_json::from_str(&b.stdout).expect("observation record parses");
        assert_eq!(rec["verdict"].as_str(), Some("exit(0)"), "{lane}: verdict");
        assert_eq!(
            rec["stdout_inline"].as_str(),
            Some("task-7\n"),
            "wolf-lang#431 — {lane}: the task read its capture after \
             `fan_out`'s frame died. An address here instead of `7` is the \
             defect itself, and it varies run to run."
        );
        checked += 1;
    }
    eprintln!("capture-across-a-parameter: {checked} tier(s) checked");
}

/// The control, so the gate above can be seen green as well as red.
///
/// `tests/fixtures/scope_handle_local/main.lu` spawns from the
/// function that opened the scope, which is the case
/// `pack_task_env`'s frame-slot arm was written for and the case where
/// its premise holds. The same checker, over the same dump shape, must
/// report nothing — otherwise it is flagging `stack.alloc` rather than
/// flagging the defect, and its red on the sibling fixture would mean
/// nothing.
#[test]
fn the_gate_passes_a_scope_spawned_into_by_its_own_frame() {
    let out = scratch().join("local.wir");
    let st = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(fixture("scope_handle_local"))
        .arg("--emit=wir")
        .arg("-o")
        .arg(&out)
        .status()
        .expect("wolf build --emit=wir runs");
    assert!(st.success(), "the control must lower");
    let text = std::fs::read_to_string(&out).expect("the wir dump is written");
    let fns = wir_functions(&text);
    let mut seen = 0usize;
    for (name, body) in &fns {
        for env in spawn_env_operands(body) {
            seen += 1;
            assert!(
                !(defined_by_stack_alloc(body, &env)
                    && !body.contains("__wolf_rt_scope_join_free")),
                "the control must pass — @{name} flagged env {env}, so the \
                 checker is reading `stack.alloc` and not the defect:\n{text}"
            );
        }
    }
    assert_eq!(
        seen, 1,
        "the control must carry exactly one spawn for the check to mean \
         anything (saw {seen})"
    );
}
