//! Shared harness for the release tier's tests: WIR module → LLVM IR
//! → clang → a real process, hermetic against a C stub that pins the
//! `wolf_rt` symbol contract from the consumer side (same posture as
//! the debug tier's exec_smoke).

// Each test binary uses a subset of this harness.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

use wolf_backend::{Backend, Linkage, mangle};
use wolf_codegen_llvm::{EmitOptions, LlvmBackend};
use wolf_wir::ir::{Aux, FuncId, Function, Module};
use wolf_wir::ops::Opcode;
use wolf_wir::types;

pub const RT_STUB: &str = r#"
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

/* s80: an identity the optimizer cannot see through (a separate TU, no
   LTO). A witness needs two pointers LLVM cannot prove equal — with one
   SSA address, basic AA answers MustAlias and never consults the scope
   metadata under test. */
long long __wolf_rt_test_opaque(long long x) { return x; }

void *__wolf_rt_region_new(void) { return malloc(sizeof(void *)); }
void *__wolf_rt_region_alloc(void *h, long long size) {
    (void)h;
    void *p = NULL;
    /* 16-aligned like the real bump allocator (the align-16 fact). */
    if (posix_memalign(&p, 16, size < 16 ? 16 : (size_t)size) != 0) abort();
    return p;
}
void __wolf_rt_region_free(void *h) { free(h); }
void __wolf_rt_region_freeze(void *h) { (void)h; }
"#;

/// Append a plain-`i64` C-entry shim (`main` calling wolf's `@main`,
/// truncating to the exit code). The crate's test fixtures all use
/// plain mains; error-union mains are exercised through the driver.
pub fn add_plain_entry_shim(m: &mut Module) -> FuncId {
    let entry = m
        .funcs
        .iter()
        .find(|(_, f)| f.name == "main")
        .map(|(_, f)| f.sig)
        .expect("fixture has a main");
    let shim_sig = m.make_sig(vec![], vec![types::I32]);
    let mut f = Function::new("__wolf_main_shim", shim_sig);
    let b0 = f.make_block(&[]);
    let callee = f.import_func("main", entry);
    let (_, res) = f.append_inst(b0, Opcode::Call, &[], &[types::I64], Aux::Callee(callee));
    let (_, tr) = f.append_inst(b0, Opcode::Itrunc, &[res[0]], &[types::I32], Aux::None);
    f.append_inst(b0, Opcode::Ret, &[tr[0]], &[], Aux::None);
    m.add_func(f)
}

/// Emit the whole module's IR: every function defined (mangled local
/// symbols), the shim exported as `main`.
pub fn module_ir(m: &Module, shim: Option<FuncId>, opts: EmitOptions) -> Option<String> {
    let mut backend = match LlvmBackend::with_options(opts) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP: {e}");
            return None;
        }
    };
    for (id, f) in m.funcs.iter() {
        let (symbol, linkage) = if Some(id) == shim {
            ("main".to_string(), Linkage::Export)
        } else if f.export {
            (f.name.clone(), Linkage::Export)
        } else {
            (mangle(m, &f.name, f.sig), Linkage::Local)
        };
        backend
            .declare_function(m, id, &f.name, &symbol, f.sig, linkage)
            .expect("declare");
    }
    for (id, f) in m.funcs.iter() {
        backend
            .define_function(m, id, f, &mut wolf_backend::NullDebugSink)
            .expect("define");
    }
    Some(backend.module_ir())
}

/// Compile IR + the RT stub to an executable and run it:
/// (exit code, stdout, stderr). `opt` is the clang -O level.
pub fn run_ir(dir_tag: &str, ir: &str, opt: &str) -> (i32, String, String) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(dir_tag);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let ll = dir.join("prog.ll");
    let stub = dir.join("rt_stub.c");
    let exe = dir.join("prog");
    std::fs::write(&ll, ir).expect("write ll");
    std::fs::write(&stub, RT_STUB).expect("write stub");
    let clang = std::env::var("WOLF_CLANG").unwrap_or_else(|_| "clang".to_string());
    let out = Command::new(&clang)
        .arg(opt)
        .arg("-Wno-override-module")
        .arg("-o")
        .arg(&exe)
        .arg(&ll)
        .arg(&stub)
        .output()
        .expect("clang runs");
    assert!(
        out.status.success(),
        "clang rejected the IR at {opt}:\n{}\n--- IR ---\n{ir}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("binary runs");
    (
        run.status.code().expect("exit code"),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
    )
}

/// Parse + verify a `.wir` fixture.
pub fn fixture(name: &str) -> Module {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}.wir",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read fixture");
    let m = wolf_wir::parse_module(&text).expect("fixture parses");
    wolf_wir::verify_module(&m).expect("fixture verifies");
    m
}
