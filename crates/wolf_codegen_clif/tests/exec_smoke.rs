//! The first machine-code tests in the repo (s28 acceptance): WIR →
//! object → `cc` → a real process, asserted on exit codes and trap
//! identities.
//!
//! The tests link against a tiny C stub implementing the `wolf_rt`
//! SYMBOL CONTRACT (`__wolf_rt_trap` + region shims) so the crate's
//! tests stay hermetic — the real implementation lives in `wolf_rt`
//! and is what `wolf build` links; the stub pins the contract from the
//! consumer side.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::PathBuf;
use std::process::Command;

use wolf_backend::Backend;
use wolf_codegen_clif::{ClifBackend, add_entry_shim, compile_module};

const RT_STUB: &str = r#"
#include <stdio.h>
#include <stdlib.h>

void __wolf_rt_trap(int kind) {
    const char *name =
        kind == 1 ? "overflow" :
        kind == 2 ? "div-zero" :
        kind == 3 ? "bounds" :
        kind == 4 ? "assert" : "other";
    fprintf(stderr, "wolf-trap: %s\n", name);
    exit(134);
}

void __wolf_rt_main_err(long long tag, long long len,
                        long long w0, long long w1,
                        long long w2, long long w3) {
    char name[33] = {0};
    long long ws[4] = {w0, w1, w2, w3};
    if (len < 0) len = 0;
    if (len > 32) len = 32;
    for (long long i = 0; i < len; i++)
        name[i] = (char)((ws[i / 8] >> ((i % 8) * 8)) & 0xff);
    if (len == 0)
        printf("error: %lld\n", tag);
    else
        printf("error: %s\n", name);
    fflush(stdout);
    exit(1);
}

void *__wolf_rt_region_new(void) { return malloc(sizeof(void *)); }
void *__wolf_rt_region_alloc(void *h, long long size) {
    (void)h;
    return malloc(size < 16 ? 16 : (size_t)size);
}
void __wolf_rt_region_free(void *h) { free(h); }
void __wolf_rt_region_freeze(void *h) { (void)h; }
"#;

/// Compile WIR text to an executable and run it: (exit code, stderr).
fn run_wir(name: &str, text: &str) -> Option<(i32, String)> {
    let mut module = wolf_wir::parse_module(text).expect("wir parses");
    wolf_wir::verify_module(&module).expect("wir verifies");
    let shim = add_entry_shim(&mut module).expect("entry shim");
    // s28 targets linux/x86-64 (the M1 platform); elsewhere skip loudly.
    let mut backend = match ClifBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP: {e}");
            return None;
        }
    };
    compile_module(
        &mut backend,
        &module,
        Some(shim),
        &mut wolf_backend::NullDebugSink,
    )
    .expect("compiles");
    let product = Box::new(backend).finish().expect("object emits");

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("exec_smoke_{name}"));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let obj = dir.join("prog.o");
    let stub = dir.join("rt_stub.c");
    let exe = dir.join("prog");
    std::fs::write(&obj, &product.bytes).expect("write object");
    std::fs::write(&stub, RT_STUB).expect("write stub");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let st = Command::new(&cc)
        .arg("-o")
        .arg(&exe)
        .arg(&obj)
        .arg(&stub)
        .status()
        .expect("cc runs");
    assert!(st.success(), "link failed");
    let out = Command::new(&exe).output().expect("binary runs");
    Some((
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}.wir",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read fixture")
}

/// s97 (#112): the indirect call, executed — `func.addr` produces the
/// pointer, `call.ind` goes through it, and the exit code proves the
/// callee actually ran (42 = double(21), computed behind the pointer).
#[test]
fn call_ind_runs_through_the_pointer() {
    let Some((code, _)) = run_wir("call_ind", &fixture("call_ind")) else {
        return;
    };
    assert_eq!(code, 42);
}

/// s96 (`[abi.native.dyn]`): the dispatch chain, executed — a one-slot
/// vtable in a stack slot, the (data, vtable) pair packed and passed,
/// and the erased callee reached THROUGH the slot reads 40 through the
/// data pointer and adds 2. Exit 42 proves both pair halves flow.
#[test]
fn dyn_dispatch_runs_through_the_table() {
    let Some((code, _)) = run_wir("dyn_dispatch", &fixture("dyn_dispatch")) else {
        return;
    };
    assert_eq!(code, 42);
}

/// The contract's acceptance fixture: branches, calls, structs, and
/// checked arithmetic, through `cc`, exiting correctly.
#[test]
fn tree_transform_runs_to_zero() {
    let Some((code, _)) = run_wir("tree", &fixture("region_infer_tree_transform")) else {
        return;
    };
    assert_eq!(code, 0);
}

/// eu values, mut-pointer params, regions, multi-token calls.
#[test]
fn qmark_defer_runs_to_zero() {
    let Some((code, _)) = run_wir("qmark", &fixture("qmark_defer")) else {
        return;
    };
    assert_eq!(code, 0);
}

/// `overflow.lu`'s deterministic trap is born here (X3): the folded
/// `trap.overflow` terminator reports its identity through the
/// runtime.
#[test]
fn overflow_traps_with_identity() {
    let Some((code, stderr)) = run_wir("overflow", &fixture("overflow")) else {
        return;
    };
    assert_eq!(code, 134);
    assert!(
        stderr.contains("wolf-trap: overflow"),
        "stderr was: {stderr}"
    );
}

/// A DYNAMIC checked-add overflow (nothing folded at parse time): the
/// compare-and-branch sequence itself must fire, with the overflow
/// identity intact.
#[test]
fn dynamic_checked_add_traps() {
    let Some((code, stderr)) = run_wir(
        "dynadd",
        "fn @main() -> i64 {\n\
         b0:\n  %0 = iconst.i64 9223372036854775807\n  %1 = iconst.i64 1\n  \
         %2 = iadd.chk %0, %1\n  ret %2\n}\n",
    ) else {
        return;
    };
    assert_eq!(code, 134);
    assert!(
        stderr.contains("wolf-trap: overflow"),
        "stderr was: {stderr}"
    );
}

/// Dynamic division by zero through the explicit zero check.
#[test]
fn dynamic_div_zero_traps() {
    let Some((code, stderr)) = run_wir(
        "dyndiv",
        "fn @main() -> i64 {\n\
         b0:\n  %0 = iconst.i64 10\n  %1 = iconst.i64 0\n  \
         %2 = idiv.chk %0, %1\n  ret %2\n}\n",
    ) else {
        return;
    };
    assert_eq!(code, 134);
    assert!(
        stderr.contains("wolf-trap: div-zero"),
        "stderr was: {stderr}"
    );
}

/// Narrow-width checked arithmetic widens and range-checks: i32 MAX+1
/// traps, while the same bits under `iadd.wrap` pass through silently.
#[test]
fn narrow_checked_vs_wrap() {
    let Some((code, stderr)) = run_wir(
        "narrowchk",
        "fn @main() -> i64 {\n\
         b0:\n  %0 = iconst.i32 2147483647\n  %1 = iconst.i32 1\n  \
         %2 = iadd.wrap %0, %1\n  %3 = iadd.chk %0, %1\n  \
         %4 = sext.i64 %3\n  ret %4\n}\n",
    ) else {
        return;
    };
    assert_eq!(code, 134);
    assert!(
        stderr.contains("wolf-trap: overflow"),
        "stderr was: {stderr}"
    );
}

/// IEEE-754 float compares through machine code (issue #22, wolf-std
/// F-0027): `fcmp.ne` is UNORDERED (`nan != nan` is TRUE — the
/// portable NaN test), while eq/lt/le/gt/ge are ordered (false on
/// NaN). The program computes NaN dynamically (0.0/0.0), runs each
/// compare, and exits with the sum of the *wrong* answers — 0 iff
/// every compare matches the interpreter's IEEE semantics.
#[test]
fn float_nan_compare_semantics() {
    // n = 0.0/0.0 (NaN), one = 1.0. Each wrong compare exits with its
    // own code (1..=7); all-correct falls through to exit 0.
    let text = "fn @main() -> i64 {\n\
        b0:\n\
        \x20 %0 = fconst.f64 0x0\n\
        \x20 %1 = fdiv %0, %0\n\
        \x20 %2 = fconst.f64 0x3ff0000000000000\n\
        \x20 %3 = fcmp.eq %1, %1\n\
        \x20 br %3, b8, b1\n\
        b1:\n\
        \x20 %4 = fcmp.ne %1, %1\n\
        \x20 br %4, b2, b9\n\
        b2:\n\
        \x20 %5 = fcmp.lt %1, %2\n\
        \x20 br %5, b10, b3\n\
        b3:\n\
        \x20 %6 = fcmp.le %1, %2\n\
        \x20 br %6, b11, b4\n\
        b4:\n\
        \x20 %7 = fcmp.gt %1, %2\n\
        \x20 br %7, b12, b5\n\
        b5:\n\
        \x20 %8 = fcmp.ge %1, %2\n\
        \x20 br %8, b13, b6\n\
        b6:\n\
        \x20 %9 = fcmp.ne %2, %2\n\
        \x20 br %9, b14, b7\n\
        b7:\n\
        \x20 %10 = iconst.i64 0\n\
        \x20 ret %10\n\
        b8:\n\
        \x20 %11 = iconst.i64 1\n\
        \x20 ret %11\n\
        b9:\n\
        \x20 %12 = iconst.i64 2\n\
        \x20 ret %12\n\
        b10:\n\
        \x20 %13 = iconst.i64 3\n\
        \x20 ret %13\n\
        b11:\n\
        \x20 %14 = iconst.i64 4\n\
        \x20 ret %14\n\
        b12:\n\
        \x20 %15 = iconst.i64 5\n\
        \x20 ret %15\n\
        b13:\n\
        \x20 %16 = iconst.i64 6\n\
        \x20 ret %16\n\
        b14:\n\
        \x20 %17 = iconst.i64 7\n\
        \x20 ret %17\n\
        }\n";
    let Some((code, _)) = run_wir("nancmp", text) else {
        return;
    };
    assert_eq!(
        code, 0,
        "IEEE float-compare semantics: ordered eq/lt/le/gt/ge, unordered ne"
    );
}

/// Plain exit-code plumbing: `main`'s i64 becomes the process status
/// through the entry shim.
#[test]
fn exit_code_flows_through_shim() {
    let Some((code, _)) = run_wir(
        "exit42",
        "fn @main() -> i64 {\nb0:\n  %0 = iconst.i64 42\n  ret %0\n}\n",
    ) else {
        return;
    };
    assert_eq!(code, 42);
}

/// s88 (wolf-lang#103): `fn main()` — no result at all — is an entry
/// the shim builds around. It is called for its effects and the
/// process gets a 0, which is what the checked rung and lupin already
/// did; the native rung used to refuse it at `wir`.
#[test]
fn no_result_main_exits_zero() {
    let Some((code, _)) = run_wir("nomain", "fn @main() {\nb0:\n  ret\n}\n") else {
        return;
    };
    assert_eq!(code, 0);
}

/// An entry the shim genuinely cannot build around is still refused —
/// and the refusal speaks the SURFACE's type names, never `i64`.
#[test]
fn unsupported_entry_signature_names_surface_types() {
    let mut module =
        wolf_wir::parse_module("fn @main() -> f64 {\nb0:\n  %0 = fconst.f64 0x0\n  ret %0\n}\n")
            .expect("wir parses");
    let err = add_entry_shim(&mut module).expect_err("f64 entry is not buildable");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("`fn main()`") && msg.contains("`fn main() -> !int`"),
        "the refusal must name the spellings a programmer can write: {msg}"
    );
    assert!(
        !msg.contains("i64"),
        "`i64` is not a surface type — the message must not name one: {msg}"
    );
}

/// eu-main (s29): an error-union `main` returning ok exits with the
/// payload; returning an error reports `error: <tag name>` on stdout
/// and exits 1 (D30's documented process behavior — never a trap,
/// never an unwind). The entry shim's compile-time tag dispatch hands
/// the NAME to the runtime.
#[test]
fn eu_main_ok_and_err_paths() {
    // Ok path: `eu{i64}` with tag 0 — exit code 7.
    let Some((code, _)) = run_wir(
        "eumain_ok",
        "fn @main() -> eu{i64} {\nb0:\n  %0 = iconst.i64 7\n  \
         %1: eu{i64} = eu.make.ok %0\n  ret %1\n}\n",
    ) else {
        return;
    };
    assert_eq!(code, 7);
}

#[test]
fn eu_main_err_reports_tag_and_exits_one() {
    let mut module = wolf_wir::parse_module(
        "fn @main() -> eu{i64} {\nb0:\n  %0 = iconst.i64 1\n  \
         %1: eu{i64} = eu.make.err %0\n  ret %1\n}\n",
    )
    .expect("wir parses");
    // The interned tag table names id 1.
    module.tag_id("Boom");
    wolf_wir::verify_module(&module).expect("wir verifies");
    let shim = add_entry_shim(&mut module).expect("entry shim");
    let mut backend = match ClifBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP: {e}");
            return;
        }
    };
    compile_module(
        &mut backend,
        &module,
        Some(shim),
        &mut wolf_backend::NullDebugSink,
    )
    .expect("compiles");
    let product = Box::new(backend).finish().expect("object emits");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("exec_smoke_eumain_err");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let obj = dir.join("prog.o");
    let stub = dir.join("rt_stub.c");
    let exe = dir.join("prog");
    std::fs::write(&obj, &product.bytes).expect("write object");
    std::fs::write(&stub, RT_STUB).expect("write stub");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let st = Command::new(&cc)
        .arg("-o")
        .arg(&exe)
        .arg(&obj)
        .arg(&stub)
        .status()
        .expect("cc runs");
    assert!(st.success(), "link failed");
    let out = Command::new(&exe).output().expect("binary runs");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "error: Boom\n",
        "the tag NAME is the documented report"
    );
}
