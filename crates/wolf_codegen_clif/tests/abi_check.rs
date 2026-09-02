//! The s29 differential ABI harness — s49's embryo (`xtask abi-check`
//! runs exactly this test in PR CI).
//!
//! A fixed table of signatures is compiled TWICE: the wolf side as
//! `export`ed C-membrane functions through the real backend, the C
//! side by the host `cc` — then values round-trip through both and
//! every bit is asserted. An echo function proves BOTH crossings of
//! one signature (C classifies the argument in; wolf re-reads it from
//! its own layout and returns it under the same classification); the
//! field probes prove wolf reads the same offsets C wrote.
//!
//! Coverage: scalars of every width and class; one- and two-eightbyte
//! aggregates in every class mix, including packed `{f32, f32}`,
//! sub-eightbyte `{i8, i8, i8}`, interior-padding `{i16, i32}`, and
//! nesting; MEMORY-class arguments (byval under SysV, indirect under
//! Apple-arm64); sret returns (`%rdi`/`%rax` under SysV, `x8` under
//! Apple-arm64); register exhaustion in both classes; the psABI
//! wholesale-reversion of an aggregate that no longer fits; HFAs
//! (`{f32, f32}`, `{f64, f64}`, `{f64 × 4}` — FP registers
//! member-wise under AAPCS64, past the 16-byte cap); and mixed
//! positions after both. The same table compiles against whatever C
//! compiler the HOST provides — gcc/clang speaking SysV on linux,
//! Apple clang speaking Apple-arm64 here — which is exactly what
//! makes it the `[abi.c.targets]` acceptance test on every ported
//! host (the runtime-SKIP pattern below covers the rest).
//! The wolf-CALLS-C direction rides the `c.*` import membrane
//! (scalars — the modelled five; aggregate imports are c10's header
//! importer). s49 replaces this fixed table with generative fuzzing;
//! the harness CONTRACT starts here.

use std::path::PathBuf;
use std::process::Command;

use wolf_backend::Backend;
use wolf_codegen_clif::{ClifBackend, compile_module};

/// The wolf side: every exported signature in the table, bodies that
/// echo or probe. `export fn` = unmangled symbol + SysV plan (s29).
const WOLF_SIDE: &str = r#"
decl @c.malloc(i64) -> ptr
decl @c.memset(ptr, i64, i64) -> ptr
decl @c.free(ptr)

export fn @w_echo_i8(i8) -> i8 {
b0(%0: i8):
  ret %0
}
export fn @w_echo_i16(i16) -> i16 {
b0(%0: i16):
  ret %0
}
export fn @w_echo_i32(i32) -> i32 {
b0(%0: i32):
  ret %0
}
export fn @w_echo_i64(i64) -> i64 {
b0(%0: i64):
  ret %0
}
export fn @w_echo_f32(f32) -> f32 {
b0(%0: f32):
  ret %0
}
export fn @w_echo_f64(f64) -> f64 {
b0(%0: f64):
  ret %0
}
export fn @w_echo_ptr(ptr) -> ptr {
b0(%0: ptr):
  ret %0
}
export fn @w_echo_ii({i32, i32}) -> {i32, i32} {
b0(%0: {i32, i32}):
  ret %0
}
export fn @w_echo_ll({i64, i64}) -> {i64, i64} {
b0(%0: {i64, i64}):
  ret %0
}
export fn @w_echo_ff({f32, f32}) -> {f32, f32} {
b0(%0: {f32, f32}):
  ret %0
}
export fn @w_echo_dd({f64, f64}) -> {f64, f64} {
b0(%0: {f64, f64}):
  ret %0
}
export fn @w_echo_ld({i64, f64}) -> {i64, f64} {
b0(%0: {i64, f64}):
  ret %0
}
export fn @w_echo_dl({f64, i64}) -> {f64, i64} {
b0(%0: {f64, i64}):
  ret %0
}
export fn @w_echo_bbb({i8, i8, i8}) -> {i8, i8, i8} {
b0(%0: {i8, i8, i8}):
  ret %0
}
export fn @w_echo_sw({i16, i32}) -> {i16, i32} {
b0(%0: {i16, i32}):
  ret %0
}
export fn @w_echo_nest({{i32, i32}, f64}) -> {{i32, i32}, f64} {
b0(%0: {{i32, i32}, f64}):
  ret %0
}
export fn @w_echo_lll({i64, i64, i64}) -> {i64, i64, i64} {
b0(%0: {i64, i64, i64}):
  ret %0
}
export fn @w_echo_d4({f64, f64, f64, f64}) -> {f64, f64, f64, f64} {
b0(%0: {f64, f64, f64, f64}):
  ret %0
}
export fn @w_mid_lll({i64, i64, i64}) -> i64 {
b0(%0: {i64, i64, i64}):
  %1 = agg.get %0, 1
  ret %1
}
export fn @w_snd_sw({i16, i32}) -> i32 {
b0(%0: {i16, i32}):
  %1 = agg.get %0, 1
  ret %1
}
export fn @w_sum8(i64, i64, i64, i64, i64, i64, i64, i64) -> i64 {
b0(%0: i64, %1: i64, %2: i64, %3: i64, %4: i64, %5: i64, %6: i64, %7: i64):
  %8 = iadd.wrap %0, %1
  %9 = iadd.wrap %8, %2
  %10 = iadd.wrap %9, %3
  %11 = iadd.wrap %10, %4
  %12 = iadd.wrap %11, %5
  %13 = iadd.wrap %12, %6
  %14 = iadd.wrap %13, %7
  ret %14
}
export fn @w_sumf9(f64, f64, f64, f64, f64, f64, f64, f64, f64) -> f64 {
b0(%0: f64, %1: f64, %2: f64, %3: f64, %4: f64, %5: f64, %6: f64, %7: f64, %8: f64):
  %9 = fadd %0, %1
  %10 = fadd %9, %2
  %11 = fadd %10, %3
  %12 = fadd %11, %4
  %13 = fadd %12, %5
  %14 = fadd %13, %6
  %15 = fadd %14, %7
  %16 = fadd %15, %8
  ret %16
}
export fn @w_spill(i64, i64, i64, i64, i64, {i64, i64}, i64) -> i64 {
b0(%0: i64, %1: i64, %2: i64, %3: i64, %4: i64, %5: {i64, i64}, %6: i64):
  %7 = agg.get %5, 0
  %8 = agg.get %5, 1
  %9 = iadd.wrap %7, %8
  %10 = iadd.wrap %9, %4
  %11 = iadd.wrap %10, %6
  ret %11
}
export fn @w_mix(i64, {f64, i64}, f32, {i64, i64, i64}) -> i64 {
b0(%0: i64, %1: {f64, i64}, %2: f32, %3: {i64, i64, i64}):
  %4 = agg.get %1, 1
  %5 = agg.get %3, 2
  %6 = iadd.wrap %0, %4
  %7 = iadd.wrap %6, %5
  ret %7
}
export fn @w_bounce_scalars() -> i64 {
b0:
  %0 = iconst.i64 24
  %1 = call @c.malloc(%0)
  %2 = iconst.i64 7
  %3 = iconst.i64 24
  %4 = call @c.memset(%1, %2, %3)
  call @c.free(%1)
  %5 = iconst.i64 0
  ret %5
}
"#;

/// The C side: matching struct definitions per the shared layout rules
/// and one check per table entry. Exit code = failing entry id.
const C_DRIVER: &str = r#"
#include <stdint.h>
#include <string.h>

struct ii { int32_t a, b; };
struct ll { int64_t a, b; };
struct ff { float a, b; };
struct dd { double a, b; };
struct ld { int64_t a; double b; };
struct dl { double a; int64_t b; };
struct bbb { int8_t a, b, c; };
struct sw { int16_t a; int32_t b; };
struct nest { struct ii p; double q; };
struct lll { int64_t a, b, c; };
struct d4 { double a, b, c, d; };
/* {f64, i64} again, under w_mix's parameter shape */
struct ld2 { double a; int64_t b; };

extern int8_t  w_echo_i8(int8_t);
extern int16_t w_echo_i16(int16_t);
extern int32_t w_echo_i32(int32_t);
extern int64_t w_echo_i64(int64_t);
extern float   w_echo_f32(float);
extern double  w_echo_f64(double);
extern void   *w_echo_ptr(void *);
extern struct ii  w_echo_ii(struct ii);
extern struct ll  w_echo_ll(struct ll);
extern struct ff  w_echo_ff(struct ff);
extern struct dd  w_echo_dd(struct dd);
extern struct ld  w_echo_ld(struct ld);
extern struct dl  w_echo_dl(struct dl);
extern struct bbb w_echo_bbb(struct bbb);
extern struct sw  w_echo_sw(struct sw);
extern struct nest w_echo_nest(struct nest);
extern struct lll w_echo_lll(struct lll);
extern struct d4  w_echo_d4(struct d4);
extern int64_t w_mid_lll(struct lll);
extern int32_t w_snd_sw(struct sw);
extern int64_t w_sum8(int64_t, int64_t, int64_t, int64_t,
                      int64_t, int64_t, int64_t, int64_t);
extern double  w_sumf9(double, double, double, double, double,
                       double, double, double, double);
extern int64_t w_spill(int64_t, int64_t, int64_t, int64_t, int64_t,
                       struct ll, int64_t);
extern int64_t w_mix(int64_t, struct ld2, float, struct lll);
extern int64_t w_bounce_scalars(void);

int main(void) {
    if (w_echo_i8(-7) != -7) return 1;
    if (w_echo_i16(-3000) != -3000) return 2;
    if (w_echo_i32(-2000000000) != -2000000000) return 3;
    if (w_echo_i64(0x0123456789abcdefLL) != 0x0123456789abcdefLL) return 4;
    if (w_echo_f32(1.5f) != 1.5f) return 5;
    if (w_echo_f64(-2.25) != -2.25) return 6;
    int probe = 0;
    if (w_echo_ptr(&probe) != &probe) return 7;

    struct ii a = { 11, -22 };
    struct ii ra = w_echo_ii(a);
    if (ra.a != a.a || ra.b != a.b) return 8;

    struct ll b = { 0x1111222233334444LL, -5 };
    struct ll rb = w_echo_ll(b);
    if (rb.a != b.a || rb.b != b.b) return 9;

    struct ff c = { 0.5f, -0.25f };
    struct ff rc = w_echo_ff(c);
    if (rc.a != c.a || rc.b != c.b) return 10;

    struct dd d = { 3.5, -4.75 };
    struct dd rd = w_echo_dd(d);
    if (rd.a != d.a || rd.b != d.b) return 11;

    struct ld e = { 77, 8.125 };
    struct ld re = w_echo_ld(e);
    if (re.a != e.a || re.b != e.b) return 12;

    struct dl f = { -9.5, 88 };
    struct dl rf = w_echo_dl(f);
    if (rf.a != f.a || rf.b != f.b) return 13;

    struct bbb g = { 1, -2, 3 };
    struct bbb rg = w_echo_bbb(g);
    if (rg.a != g.a || rg.b != g.b || rg.c != g.c) return 14;

    struct sw h = { -100, 123456 };
    struct sw rh = w_echo_sw(h);
    if (rh.a != h.a || rh.b != h.b) return 15;

    struct nest n = { { 5, 6 }, 7.5 };
    struct nest rn = w_echo_nest(n);
    if (rn.p.a != n.p.a || rn.p.b != n.p.b || rn.q != n.q) return 16;

    struct lll m = { 101, 202, 303 };
    struct lll rm = w_echo_lll(m);
    if (rm.a != m.a || rm.b != m.b || rm.c != m.c) return 17;

    struct d4 q = { 1.0, 2.0, 3.0, 4.0 };
    struct d4 rq = w_echo_d4(q);
    if (rq.a != q.a || rq.b != q.b || rq.c != q.c || rq.d != q.d) return 18;

    if (w_mid_lll(m) != 202) return 19;
    if (w_snd_sw(h) != 123456) return 20;
    if (w_sum8(1, 2, 3, 4, 5, 6, 7, 8) != 36) return 21;
    if (w_sumf9(1, 2, 3, 4, 5, 6, 7, 8, 9) != 45.0) return 22;
    if (w_spill(9, 9, 9, 9, 100, b, 1000) != b.a + b.b + 1100) return 23;

    struct ld2 x = { 2.5, 40 };
    if (w_mix(1, x, 9.0f, m) != 1 + 40 + 303) return 24;
    if (w_bounce_scalars() != 0) return 25;
    return 0;
}
"#;

#[test]
fn abi_table_round_trips_against_host_cc() {
    let module = wolf_wir::parse_module(WOLF_SIDE).expect("wolf side parses");
    wolf_wir::verify_module(&module).expect("wolf side verifies");
    let mut backend = match ClifBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP: {e}");
            return;
        }
    };
    // s60a: a C target that refuses this table BY SHAPE (win64 refuses
    // every aggregate by value until the campaign's cl.exe differential)
    // is a loud skip — the shape refusal is the contract there, and the
    // differential against the platform compiler is s49/s60's.
    if let Err(e) = compile_module(
        &mut backend,
        &module,
        None,
        &mut wolf_backend::NullDebugSink,
    ) {
        if matches!(e, wolf_backend::BackendError::Unsupported(_)) {
            eprintln!("SKIP: this host's C target refuses the table by shape: {e}");
            return;
        }
        panic!("compiles: {e}");
    }
    let product = Box::new(backend).finish().expect("object emits");

    // Every export must be visible under its UNMANGLED name.
    for f in module.funcs.values().filter(|f| f.export) {
        assert!(
            product.symbols.iter().any(|s| s.name == f.name),
            "export `{}` not in the symbol table",
            f.name
        );
    }

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("abi_check");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let obj = dir.join("wolf_side.o");
    let drv = dir.join("driver.c");
    let exe = dir.join("abi_check");
    std::fs::write(&obj, &product.bytes).expect("write object");
    std::fs::write(&drv, C_DRIVER).expect("write driver");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    if Command::new(&cc).arg("--version").output().is_err() {
        eprintln!(
            "SKIP: no `{cc}` on this host — the differential against the platform C compiler needs one"
        );
        return;
    }
    let out = Command::new(&cc)
        .arg("-o")
        .arg(&exe)
        .arg(&drv)
        .arg(&obj)
        .output()
        .expect("cc runs");
    assert!(
        out.status.success(),
        "link failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("binary runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "ABI mismatch at table entry {:?}",
        run.status.code()
    );
}

/// `[abi.c.panic]`: a wolf trap behind an `export` ABORTS the process
/// (deterministically, through the trap reporter) — it never returns
/// garbage into the C caller, and no unwinding exists to cross the
/// membrane.
#[test]
fn wolf_trap_behind_export_aborts() {
    let wolf_side = "export fn @w_overflow(i64) -> i64 {\n\
                     b0(%0: i64):\n  %1 = iconst.i64 9223372036854775807\n  \
                     %2 = iadd.chk %0, %1\n  ret %2\n}\n";
    let c_driver = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
extern int64_t w_overflow(int64_t);
void __wolf_rt_trap(int kind) {
    fprintf(stderr, "wolf-trap: %d\n", kind);
    exit(134);
}
int main(void) {
    int64_t r = w_overflow(2);
    /* Unreachable: the trap must abort before any value returns. */
    (void)r;
    return 42;
}
"#;
    let module = wolf_wir::parse_module(wolf_side).expect("parses");
    wolf_wir::verify_module(&module).expect("verifies");
    let mut backend = match ClifBackend::new() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SKIP: {e}");
            return;
        }
    };
    // s60a: a C target that refuses this table BY SHAPE (win64 refuses
    // every aggregate by value until the campaign's cl.exe differential)
    // is a loud skip — the shape refusal is the contract there, and the
    // differential against the platform compiler is s49/s60's.
    if let Err(e) = compile_module(
        &mut backend,
        &module,
        None,
        &mut wolf_backend::NullDebugSink,
    ) {
        if matches!(e, wolf_backend::BackendError::Unsupported(_)) {
            eprintln!("SKIP: this host's C target refuses the table by shape: {e}");
            return;
        }
        panic!("compiles: {e}");
    }
    let product = Box::new(backend).finish().expect("object emits");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("abi_check_trap");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let obj = dir.join("wolf_side.o");
    let drv = dir.join("driver.c");
    let exe = dir.join("trap_check");
    std::fs::write(&obj, &product.bytes).expect("write object");
    std::fs::write(&drv, c_driver).expect("write driver");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    if Command::new(&cc).arg("--version").output().is_err() {
        eprintln!(
            "SKIP: no `{cc}` on this host — the differential against the platform C compiler needs one"
        );
        return;
    }
    let out = Command::new(&cc)
        .arg("-o")
        .arg(&exe)
        .arg(&drv)
        .arg(&obj)
        .output()
        .expect("cc runs");
    assert!(
        out.status.success(),
        "link failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("binary runs");
    assert_eq!(run.status.code(), Some(134), "the trap must abort");
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("wolf-trap: 1"),
        "the overflow identity must reach the boundary"
    );
}
