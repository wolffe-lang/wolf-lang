//! s99 acceptance at the driver: interprocedural range facts mint only
//! where the proof exists, and every kill in the contract's list is a
//! pinned refusal.
//!
//! The observable is the release tier's LLVM IR: a discharged
//! `iadd.chk` leaves no `llvm.sadd.with.overflow` behind, an
//! undischarged one keeps it. Each witness is a whole program on the
//! a2/stencil shape — `fill(mut out, src)` sums src's elements into
//! out — because a `mut List` param carries a memory token and a
//! multi-block token-param callee never inlines (the s42 v0 rule), so
//! the channel under test is genuinely interprocedural. A read-only
//! callee would inline and the intraprocedural half would silently
//! take over (measured while writing this file: `total(xs)` with no
//! mut param vanished into `_Wmain`).
//!
//! Behavior is pinned alongside every IR assertion: debug and release
//! must print the same answer (facts change codegen, never results).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn case(name: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("main.lu"), src).expect("write");
    dir
}

/// Release LLVM IR, or None on an environment skip (exit 2).
fn release_ir(dir: &Path) -> Option<String> {
    let out = dir.join("out.ll");
    let st = Command::new(wolf())
        .args([
            "build",
            dir.join("main.lu").to_str().unwrap(),
            "--release",
            "--emit=llvm-ir",
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn wolf");
    if st.status.code() == Some(2) {
        eprintln!(
            "SKIP (environment): {}",
            String::from_utf8_lossy(&st.stderr)
        );
        return None;
    }
    assert!(
        st.status.success(),
        "release build failed:\n{}",
        String::from_utf8_lossy(&st.stderr)
    );
    Some(std::fs::read_to_string(&out).expect("read ir"))
}

/// Run on a tier; (exit, stdout).
fn run(dir: &Path, release: bool) -> Option<(i32, String)> {
    let bin = dir.join(if release { "a_rel" } else { "a_dbg" });
    let mut args = vec![
        "build".to_string(),
        dir.join("main.lu").to_str().unwrap().to_string(),
        "-o".to_string(),
        bin.to_str().unwrap().to_string(),
    ];
    if release {
        args.insert(1, "--release".to_string());
    }
    let st = Command::new(wolf()).args(&args).output().expect("spawn");
    if st.status.code() == Some(2) {
        return None;
    }
    assert!(
        st.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&st.stderr)
    );
    let out = Command::new(&bin).output().expect("run");
    Some((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    ))
}

/// The body of the function whose DEFINE line names `name` (symbols
/// are `_W`-prefixed and a header `declare` must not match).
fn func_ir<'a>(ir: &'a str, name: &str) -> &'a str {
    for seg in ir.split("\ndefine") {
        let first = seg.lines().next().unwrap_or("");
        if first.contains(name) {
            return seg;
        }
    }
    panic!("no define for {name} in the IR");
}

fn overflow_intrinsics(ir: &str) -> usize {
    ir.matches("llvm.sadd.with.overflow").count()
        + ir.matches("llvm.ssub.with.overflow").count()
        + ir.matches("llvm.smul.with.overflow").count()
}

fn tiers_agree(dir: &Path) {
    let (Some(d), Some(r)) = (run(dir, false), run(dir, true)) else {
        return;
    };
    assert_eq!(d, r, "debug and release disagree — facts changed results");
}

/// The callee under test — the a2 shape: a mut out (token param, never
/// inlines) and a read src whose element loads want the caller's
/// proof. The element-sum `+` inside is the checked op that must
/// discharge exactly when the pushes prove it.
const FILL: &str = "fn fill(mut out: List[int], src: List[int]) {\n    var i = 1\n    let last = src.len - 1\n    while i < last {\n        out[i] = (src[i - 1] + src[i] + src[i + 1]) & 1048575\n        i = i + 1\n    }\n}\n";

/// A caller prologue: two lists, src pushed from `expr` (a function of
/// i), out zero-filled, then fill(mut out, src), then sink out[1].
fn program(push_expr: &str, extra_fns: &str, pre_call: &str) -> String {
    format!(
        "{FILL}\n{extra_fns}fn main() -> !int {{\n    var src = List[int]()\n    var out = List[int]()\n    var i = 0\n    while i < 64 {{\n        (mut src).push({push_expr})\n        (mut out).push(0)\n        i = i + 1\n    }}\n{pre_call}    fill(mut out, src)\n    if out[1] >= 0 {{ 0 }} else {{ 1 }}\n}}\n"
    )
}

#[test]
fn bounded_pushes_discharge_the_callee_sums() {
    let dir = case("s99_pos", &program("i & 255", "", ""));
    let Some(ir) = release_ir(&dir) else { return };
    let fir = func_ir(&ir, "fill");
    // The element-sum ADDS discharge; `src.len - 1` keeps its cold
    // isub.chk (len is unbounded) — the same residue the real a2
    // kernel carries.
    assert_eq!(
        fir.matches("llvm.sadd.with.overflow").count(),
        0,
        "bounded pushes should discharge fill's element sums:\n{fir}"
    );
    tiers_agree(&dir);
}

#[test]
fn an_unbounded_push_keeps_the_checks() {
    // env_args().len is a value no evaluator can bound — the honest
    // "no proof, no fact" baseline for every kill below.
    let dir = case(
        "s99_unbounded",
        &program(
            "i & 255",
            "",
            "    let n = env_args().len\n    (mut src).push(n * 1000000000000)\n",
        ),
    );
    let Some(ir) = release_ir(&dir) else { return };
    let fir = func_ir(&ir, "fill");
    assert!(
        overflow_intrinsics(fir) > 0,
        "an unbounded push must keep fill's checks:\n{fir}"
    );
    tiers_agree(&dir);
}

#[test]
fn a_wrapping_value_among_the_pushes_keeps_the_checks() {
    // The D44-addendum witness: a wrap-typed value's range is its type
    // bounds and nothing narrower, at every entry into the channel.
    let dir = case(
        "s99_wrap",
        &program(
            "i & 255",
            "",
            "    var w: wrapping[int] = 9223372036854775807\n    w = w * 1664525 + 1013904223\n    (mut src).push(w as int)\n",
        ),
    );
    let Some(ir) = release_ir(&dir) else { return };
    let fir = func_ir(&ir, "fill");
    assert!(
        overflow_intrinsics(fir) > 0,
        "a wrapping push must keep fill's checks:\n{fir}"
    );
    tiers_agree(&dir);
}

#[test]
fn the_same_list_as_both_params_cannot_be_spelled() {
    // The alias kill has a stronger guard than the channel: the mem
    // tier rejects `fill(mut src, src)` outright — E1002, the c04
    // exclusivity theorem ("the same place twice is never disjoint").
    // The channel's own frame check is defense in depth, pinned at
    // the WIR level in `wolf_wir/tests/interproc.rs` where a
    // hand-built module can express what the language refuses.
    let src_prog = format!(
        "{FILL}\nfn main() -> !int {{\n    var src = List[int]()\n    var i = 0\n    while i < 64 {{\n        (mut src).push(i & 255)\n        i = i + 1\n    }}\n    fill(mut src, src)\n    if src[1] >= 0 {{ 0 }} else {{ 1 }}\n}}\n"
    );
    let dir = case("s99_alias", &src_prog);
    let st = Command::new(wolf())
        .args(["build", dir.join("main.lu").to_str().unwrap(), "--release"])
        .output()
        .expect("spawn wolf");
    let err = String::from_utf8_lossy(&st.stderr);
    assert!(
        !st.status.success() && err.contains("E1002"),
        "the aliased call must be an E1002 rejection, got:\n{err}"
    );
}

#[test]
fn a_list_reaching_the_c_membrane_is_poisoned() {
    let dir = case(
        "s99_export",
        &program(
            "i & 255",
            "export fn poke(xs: List[int]) -> int {\n    xs.len\n}\n\n",
            "    let n = poke(src)\n    if n < 0 { return 1 }\n",
        ),
    );
    let Some(ir) = release_ir(&dir) else { return };
    let fir = func_ir(&ir, "fill");
    assert!(
        overflow_intrinsics(fir) > 0,
        "a list reaching an export must keep fill's checks:\n{fir}"
    );
    tiers_agree(&dir);
}

#[test]
fn a_list_crossing_a_fn_value_is_poisoned() {
    // s97: `call.ind` contributes no summary edge — so it kills.
    let dir = case(
        "s99_callind",
        &program(
            "i & 255",
            "fn feed(xs: List[int]) -> int {\n    xs.len\n}\n\n",
            "    let f = feed\n    let n = f(src)\n    if n < 0 { return 1 }\n",
        ),
    );
    let Some(ir) = release_ir(&dir) else { return };
    let fir = func_ir(&ir, "fill");
    assert!(
        overflow_intrinsics(fir) > 0,
        "a list crossing call.ind must keep fill's checks:\n{fir}"
    );
    tiers_agree(&dir);
}
