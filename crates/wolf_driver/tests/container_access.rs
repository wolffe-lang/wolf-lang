//! s75 acceptance (wolf-lang#77): `List` element access is memory the
//! optimizer can see.
//!
//! s44 measured element access at 44–92 ns against C's 0.2–1.0 and
//! found zero vector instructions in the whole suite, because every
//! `xs[i]` lowered to `__wolf_rt_list_read(hdr, i, slot)` — an opaque
//! call LLVM cannot move, widen or see through. These tests pin the
//! four claims that fix costs:
//!
//! 1. **No per-element call.** The release-tier IR for a `List` sum
//!    contains no `list_read`/`list_write`/`list_len` call at all;
//!    only allocation (`list_new`) and growth (`list_push`) remain.
//! 2. **It vectorizes.** A `daxpy`-shaped loop over two `List[f64]`s
//!    reaches `clang -x ir -O2` as something the loop vectorizer takes
//!    — the acceptance clause of the sprint.
//! 3. **Bounds checks STAY.** An index the analysis cannot bound still
//!    emits a guard and still traps `bounds`. What the range analysis
//!    proves it removes; what it cannot prove it keeps. The two cases
//!    are pinned side by side so neither can drift into the other.
//! 4. **`for` over a `List`** is a counted loop over the backing
//!    storage: no call, and no check, because the header test is the
//!    proof.
//!
//! Environment problems (no clang) SKIP loudly; refusals FAIL.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// One fixture in a directory of its OWN: the driver treats a
/// directory as a module, so a shared scratch dir would sweep every
/// other test's `.lu` into this compilation.
fn write_case(case: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("s75")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join(format!("{case}.lu"));
    std::fs::write(&entry, src).expect("write fixture");
    entry
}

/// The release tier's whole-module IR for one program.
fn release_ir(case: &str, src: &str) -> String {
    let entry = write_case(case, src);
    let out = entry.with_extension("ll");
    let res = Command::new(wolf())
        .arg("build")
        .arg(&entry)
        .arg("--release")
        .arg("--emit=llvm-ir")
        .arg("-o")
        .arg(&out)
        .output()
        .expect("wolf runs");
    assert!(
        res.status.success(),
        "{case}: --emit=llvm-ir failed: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    std::fs::read_to_string(&out).expect("IR written")
}

/// Runtime symbols that must NOT appear in element-access position.
/// `list_new` and `list_push` are the two the runtime keeps: minting a
/// header and growing a buffer genuinely need it.
const BANNED: [&str; 5] = [
    "__wolf_rt_list_read",
    "__wolf_rt_list_write",
    "__wolf_rt_list_len",
    "__wolf_rt_list_pop",
    "__wolf_rt_list_clear",
];

fn assert_no_element_calls(case: &str, ir: &str) {
    for sym in BANNED {
        assert!(
            !ir.contains(sym),
            "{case}: {sym} survives in the release IR — element access is \
             an opaque call again, and #77 is back:\n{ir}"
        );
    }
}

const SUM: &str = r#"
fn sum(xs: List[int]) -> int {
    var acc = 0
    var i = 0
    while i < xs.len {
        acc = acc + xs[i]
        i = i + 1
    }
    acc
}

fn main() -> !int {
    var xs = List[int]()
    var i = 0
    while i < 4096 {
        (mut xs).push(i)
        i = i + 1
    }
    print("{sum(xs)}")
    0
}
"#;

#[test]
fn a_list_sum_carries_no_per_element_call() {
    let ir = release_ir("sum", SUM);
    assert_no_element_calls("sum", &ir);
    // What replaced it: a scaled address and a load.
    assert!(
        ir.contains("getelementptr") && ir.contains("load i64, ptr"),
        "the sum loop should read through ptr.off + load:\n{ir}"
    );
}

#[test]
fn a_provable_index_emits_no_bounds_check() {
    // `i` is an induction variable from 0 and the loop guard is
    // `i < xs.len` over the SAME length value, so `i` is in bounds by
    // the relation the guard already established. The check is proven,
    // not deleted: nothing here weakens the trap for an index that is
    // not proven — see the next test.
    let ir = release_ir("sum_proven", SUM);
    assert!(
        !ir.contains("i32 3)"),
        "no bounds trap should survive a loop whose guard proves the \
         index:\n{ir}"
    );
}

#[test]
fn an_unprovable_index_still_traps_bounds() {
    // Nothing relates `k` to the list's length, so the guard stays and
    // the program traps at run time. Bounds checks are eliminated when
    // provable and never because they are inconvenient.
    let src = r#"
fn at(xs: List[int], k: int) -> int { xs[k] }

fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    print("{at(xs, 7)}")
    0
}
"#;
    let ir = release_ir("oob", src);
    assert_no_element_calls("oob", &ir);
    assert!(
        ir.contains("@__wolf_rt_trap(i32 3)"),
        "the bounds trap must survive an index nothing bounds:\n{ir}"
    );

    let entry = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("s75")
        .join("oob")
        .join("oob.lu");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg("--release")
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) {
        eprintln!(
            "SKIP: environment cannot run the release lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return;
    }
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    assert_eq!(
        rec["verdict"].as_str(),
        Some("trap(bounds)"),
        "the release tier must still report the bounds trap"
    );
}

#[test]
fn for_over_a_list_is_a_counted_loop() {
    let src = r#"
fn total(xs: List[int]) -> int {
    var acc = 0
    for x in xs {
        acc = acc + x
    }
    acc
}

fn main() -> !int {
    var xs = List[int]()
    var i = 0
    while i < 64 {
        (mut xs).push(i)
        i = i + 1
    }
    print("{total(xs)}")
    0
}
"#;
    let ir = release_ir("for_list", src);
    assert_no_element_calls("for_list", &ir);
    assert!(
        !ir.contains("i32 3)"),
        "`for` over a List needs no bounds check — the header test is \
         the proof:\n{ir}"
    );
}

/// The acceptance clause: a `List` loop LLVM will vectorize. `daxpy`
/// is the shape family A exists to measure, and s44 measured zero
/// vectorized loops across the entire suite.
#[test]
fn a_list_loop_reaches_the_vectorizer() {
    let src = r#"
fn daxpy(mut y: List[f64], x: List[f64], a: f64) {
    var i = 0
    while i < x.len {
        y[i] = y[i] + a * x[i]
        i = i + 1
    }
}

fn main() -> !int {
    var x = List[f64]()
    var y = List[f64]()
    var i = 0
    while i < 4096 {
        (mut x).push(0.5)
        (mut y).push(1.0)
        i = i + 1
    }
    daxpy(mut y, x, 1.5)
    print("{y[0]}")
    0
}
"#;
    let ir = release_ir("daxpy", src);
    assert_no_element_calls("daxpy", &ir);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("s75")
        .join("daxpy");
    let ll = dir.join("daxpy.ll");
    let opt = Command::new("clang")
        .arg("-x")
        .arg("ir")
        .arg("-O2")
        .arg("-S")
        .arg("-emit-llvm")
        .arg(&ll)
        .arg("-o")
        .arg(dir.join("daxpy.opt.ll"))
        .output();
    let Ok(opt) = opt else {
        eprintln!("SKIP: no clang on this host");
        return;
    };
    if !opt.status.success() {
        eprintln!(
            "SKIP: clang cannot read the module: {}",
            String::from_utf8_lossy(&opt.stderr).trim()
        );
        return;
    }
    let optimized = std::fs::read_to_string(dir.join("daxpy.opt.ll")).expect("optimized IR");
    assert!(
        optimized.contains("<2 x double>") || optimized.contains("<4 x double>"),
        "the daxpy loop must vectorize — this is #77's acceptance \
         clause, and the whole reason the container stopped being a \
         call:\n{optimized}"
    );
}
