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
    compile_module(&mut backend, &module, Some(shim)).expect("compiles");
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
    compile_module(&mut backend, &module, Some(shim)).expect("compiles");
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
