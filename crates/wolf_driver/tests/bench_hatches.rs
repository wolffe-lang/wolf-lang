//! The two s44 measurement hatches, tested — because a benchmark lane
//! built on an env var that silently does nothing is worse than no lane.
//!
//! - `WOLF_MIDEND=0` in release skips the s42 mid-end and the s43
//!   whole-program phase, so the IR-volume lane (#70 metric 1) has a real
//!   NAIVE-s41 denominator instead of a guess.
//! - `WOLF_STRIP_FACTS=1` lowers with every fact channel silenced — the
//!   metadata-drop sentinel's control lane (report 10 delta 3), the thing
//!   that prices the bonus channel per commit.
//!
//! Both are MEASUREMENT modes, never supported build modes. These tests
//! assert they do what their consumers assume, and (for strip-facts) that
//! the stripped build still runs and still traps identically — metadata is
//! a bonus (D2), so removing it may cost speed and must never cost
//! correctness.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn out_dir() -> PathBuf {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench-hatch-tests");
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

/// A loop the mid-end has real work to do on, over values it CANNOT
/// fold away.
///
/// The arithmetic rides `List` elements behind `mut`/`read` params, so
/// the annotated lane carries the channels the sentinel exists to
/// price: `noalias` and `dereferenceable` from the param modes,
/// `!alias.scope` on the element traffic, and `!prof` on the overflow
/// check that an opaque element operand keeps. s85 is why this is not
/// the old constant-bounded counter: once the trip-scaled accumulator
/// bound landed, the mid-end proved that fixture's every check away
/// and the "annotated" lane had nothing left to strip — the sentinel
/// would have been measuring nothing against nothing, which is the one
/// thing this test exists to refuse.
const SRC: &str = "\
fn blend(mut out: List[int], src: List[int]) {
    var i = 0
    while i < src.len {
        out[i] = (src[i] * 3 + 7) & 1023
        i = i + 1
    }
}

fn main() -> !int {
    var src = List[int]()
    var out = List[int]()
    var i = 0
    while i < 64 {
        (mut src).push(i)
        (mut out).push(0)
        i = i + 1
    }
    blend(mut out, src)
    var acc = 0
    var k = 0
    while k < 64 {
        acc = (acc + out[k]) & 65535
        k = k + 1
    }
    if acc >= 0 { 0 } else { 1 }
}
";

/// Emit the release module, optionally with a hatch set. `None` means the
/// release tier is unavailable here (no clang) — skip loudly, never green.
fn emit(tag: &str, env: &[(&str, &str)]) -> Option<String> {
    let dir = out_dir();
    let src = dir.join(format!("{tag}.lu"));
    let ll = dir.join(format!("{tag}.ll"));
    std::fs::write(&src, SRC).expect("write source");
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&ll)
        .arg("--release")
        .arg("--no-cache")
        .arg("--emit=llvm-ir");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("wolf runs");
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("release tier requires clang") || err.contains("targets linux/x86-64"),
            "release emit failed for a reason that is not a missing toolchain:\n{err}"
        );
        eprintln!("bench_hatches: release tier unavailable — skipped");
        return None;
    }
    Some(std::fs::read_to_string(&ll).expect("read emitted IR"))
}

/// Count instructions the way `xtask::t1::count_ir_instructions` does.
fn insts(module: &str) -> usize {
    let mut n = 0;
    let mut depth = 0usize;
    for raw in module.lines() {
        let line = raw.trim();
        if line.starts_with("define") {
            depth += usize::from(line.ends_with('{'));
            continue;
        }
        if depth == 0 {
            continue;
        }
        if line == "}" {
            depth -= 1;
        } else if !line.is_empty()
            && !line.starts_with(';')
            && !(line.ends_with(':') && !line.contains(' '))
        {
            n += 1;
        }
    }
    n
}

#[test]
fn midend_off_really_turns_the_midend_off() {
    let (Some(on), Some(off)) = (
        emit("hatch_on", &[]),
        emit("hatch_off", &[("WOLF_MIDEND", "0")]),
    ) else {
        return;
    };
    let (a, b) = (insts(&on), insts(&off));
    assert!(a > 0 && b > 0, "no instructions were counted: {a} vs {b}");
    // The denominator #70's budget is stated against has to be a real
    // naive lowering. If the hatch stopped working, these would be equal
    // and the IR-volume metric would quietly report 1.00 forever.
    assert!(
        b > a,
        "WOLF_MIDEND=0 produced {b} instruction(s) against the mid-end's {a} — the naive \
         lowering is not naive, so the IR-volume ratio has no denominator"
    );
}

#[test]
fn strip_facts_removes_every_fact_channel() {
    let (Some(on), Some(off)) = (
        emit("facts_on", &[]),
        emit("facts_off", &[("WOLF_STRIP_FACTS", "1")]),
    ) else {
        return;
    };
    // The annotated lane must actually carry facts, or the sentinel would
    // be measuring nothing against nothing.
    assert!(
        on.contains("noalias") || on.contains("!range") || on.contains("!prof"),
        "the annotated lane carries no fact channels at all:\n{on}"
    );
    for channel in [
        "noalias",
        "readonly",
        "dereferenceable",
        "!alias.scope",
        "!invariant.load",
        "!range",
        "!prof",
    ] {
        assert!(
            !off.contains(channel),
            "WOLF_STRIP_FACTS=1 still emitted `{channel}` — the sentinel's control lane is not a \
             control"
        );
    }
}

#[test]
fn a_stripped_build_still_runs_and_still_traps() {
    // D2: metadata is the BONUS channel. Stripping it may cost speed; it
    // must not change a single verdict.
    let dir = out_dir();
    let src = dir.join("facts_run.lu");
    std::fs::write(&src, SRC).expect("write source");
    let mut verdicts = Vec::new();
    for env in [None, Some(("WOLF_STRIP_FACTS", "1"))] {
        let bin = dir.join(match env {
            None => "facts_run_on",
            Some(_) => "facts_run_off",
        });
        let mut cmd = Command::new(wolf());
        cmd.arg("build")
            .arg(&src)
            .arg("-o")
            .arg(&bin)
            .arg("--release")
            .arg("--no-cache");
        if let Some((k, v)) = env {
            cmd.env(k, v);
        }
        if !cmd.status().expect("wolf runs").success() {
            eprintln!("bench_hatches: release tier unavailable — skipped");
            return;
        }
        let out = Command::new(&bin).output().expect("run built program");
        verdicts.push((out.status.code(), out.stdout));
    }
    assert_eq!(
        verdicts[0], verdicts[1],
        "the stripped lowering changed the program's behaviour — metadata is a bonus channel, \
         never a correctness input"
    );
}
