//! Rendered snapshots for the E07xx comptime family — every code ships
//! with at least one reviewed fixture (`cargo xtask diag-catalog`
//! enforces the pairing). The sandbox refusals name their category and
//! reason (D33), the budget faults carry comptime backtraces and
//! raise-the-budget fix-its, and the staged-diagnostics snapshot shows
//! an error inside generated code rendering its generator provenance
//! chain.

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::ctfe::derive::{Provenance, check_generated, derive_eq};
use wolf_sema::{AliasTable, MemoryLoader, TraitRef, resolve_package_with, typecheck_package_with};

fn render_ctfe(src: &str) -> String {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "snapshot inputs resolve clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in &tc.diagnostics {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    out
}

fn snap_one(name: &str, src: &str) {
    insta::assert_snapshot!(name, render_ctfe(src));
}

// ---------------------------------------------------------- E0701 -----

/// Filesystem reach: refused for confinement, with the s51
/// declared-build-inputs pointer in the explanation.
#[test]
fn e0701_sandbox_fs() {
    snap_one(
        "e0701_fs",
        "comptime fn embed(path: str) -> str {\n    read_text(path)\n}\n\n\
         fn main() -> !int {\n    const BANNER = embed(\"banner.txt\")\n    \
         if BANNER == \"\" { 1 } else { 0 }\n}\n",
    );
}

/// Clock reach: refused for determinism — the reason is named.
#[test]
fn e0701_sandbox_clock() {
    snap_one(
        "e0701_clock",
        "comptime fn build_stamp() -> int {\n    clock_ms()\n}\n\n\
         fn main() -> !int {\n    const STAMP = build_stamp()\n    \
         if STAMP == 0 { 1 } else { 0 }\n}\n",
    );
}

/// FFI reach: an `extern` fn has no comptime body by construction.
#[test]
fn e0701_sandbox_ffi() {
    snap_one(
        "e0701_ffi",
        "extern \"c\" fn c_abs(n: int) -> int\n\n\
         comptime fn magnitude(n: int) -> int {\n    c_abs(n)\n}\n\n\
         fn main() -> !int {\n    const M = magnitude(-3)\n    \
         if M == 3 { 0 } else { 1 }\n}\n",
    );
}

// ---------------------------------------------------------- E0702 -----

/// Fuel exhaustion: backtrace plus the raise-the-budget fix-it
/// (`#[budget(fuel = N)]` inserted at the enclosing statement).
#[test]
fn e0702_fuel_exhaustion_fixit() {
    snap_one(
        "e0702_fuel_fixit",
        "comptime fn spin() -> int {\n    while true {}\n    0\n}\n\n\
         fn main() -> !int {\n    const N = spin()\n    if N == 0 { 0 } else { 1 }\n}\n",
    );
}

// ---------------------------------------------------------- E0703 -----

/// Heap-cap crossing under a legally raised fuel budget: the heap cap
/// fires on its own, with its own raise-the-budget fix-it.
#[test]
fn e0703_heap_exhaustion() {
    snap_one(
        "e0703_heap",
        "comptime fn flood() -> int {\n    var n = 0\n    \
         while n < 100000000 {\n        n = n + 1\n    }\n    n\n}\n\n\
         fn main() -> !int {\n    #[budget(fuel = 100000000)]\n    \
         const N = flood()\n    if N == 0 { 1 } else { 0 }\n}\n",
    );
}

// ---------------------------------------------------------- E0704 -----

/// Depth exhaustion: the repeating frame folds into one counted line
/// instead of hundreds of identical secondaries.
#[test]
fn e0704_depth_folded_frames() {
    snap_one(
        "e0704_depth",
        "comptime fn dive(n: int) -> int {\n    dive(n + 1)\n}\n\n\
         fn main() -> !int {\n    const D = dive(0)\n    if D == 0 { 1 } else { 0 }\n}\n",
    );
}

// ---------------------------------------------------------- E0705 -----

/// A runtime local in comptime position: must be comptime-known.
#[test]
fn e0705_runtime_argument() {
    snap_one(
        "e0705_runtime_arg",
        "comptime fn double(n: int) -> int {\n    n + n\n}\n\n\
         fn main() -> !int {\n    let x = 21\n    const Y = double(x)\n    \
         if Y == 42 { 0 } else { 1 }\n}\n",
    );
}

// ---------------------------------------------------------- E0706 -----

/// X3 at comptime: overflow at the declared width faults; the host's
/// width is never observable.
#[test]
fn e0706_overflow_at_declared_width() {
    snap_one(
        "e0706_overflow",
        "comptime fn brim() -> i32 {\n    let big: i32 = 2147483647\n    big + 1\n}\n\n\
         fn main() -> !int {\n    const B = brim()\n    if B == 0 { 1 } else { 0 }\n}\n",
    );
}

// ---------------------------------------------------------- E0707 -----

/// The witness line: `N + N` vs `N * 2` sits beyond linear
/// normalization, and the note states the three-step ladder.
#[test]
fn e0707_witness_line_note() {
    snap_one(
        "e0707_witness",
        "struct Buf[N: type] {\n    len: int,\n}\n\n\
         fn sneak[N: type](b: Buf[N + N]) -> Buf[N * 2] {\n    b\n}\n\n\
         fn main() -> !int {\n    0\n}\n",
    );
}

// ---------------------------------------------------------- E0708 -----

/// Layout is c05's: `size_of` on an aggregate is staged, not refused
/// forever.
#[test]
fn e0708_layout_unresolved() {
    snap_one(
        "e0708_layout",
        "struct Vec2 {\n    x: f64,\n    y: f64,\n}\n\n\
         fn main() -> !int {\n    const S = size_of(Vec2)\n    \
         if S == 16 { 0 } else { 1 }\n}\n",
    );
}

// ---------------------------------------------------------- E0709 -----

/// Budgets are raised, never removed: `fuel = 0` is rejected.
#[test]
fn e0709_budget_cannot_be_zero() {
    snap_one(
        "e0709_budget_zero",
        "comptime fn ten() -> int {\n    10\n}\n\n\
         fn main() -> !int {\n    #[budget(fuel = 0)]\n    const N = ten()\n    \
         if N == 10 { 0 } else { 1 }\n}\n",
    );
}

// ---------------------------------------------------------- E0710 -----

/// A failed comptime assertion stops the build — the witness mechanism.
#[test]
fn e0710_comptime_assert_failed() {
    snap_one(
        "e0710_assert",
        "comptime fn expect_fields(T: type, n: int) -> bool {\n    \
         assert(typeinfo(T).fields.len == n)\n    true\n}\n\n\
         struct Pair {\n    a: int,\n    b: int,\n}\n\n\
         fn main() -> !int {\n    const OK = expect_fields(Pair, 3)\n    \
         if OK { 0 } else { 1 }\n}\n",
    );
}

/// The two-argument form (#9): the `str` message rides into the E0710
/// rendering.
#[test]
fn e0710_comptime_assert_failed_with_message() {
    snap_one(
        "e0710_assert_message",
        "comptime fn expect_fields(T: type, n: int) -> bool {\n    \
         assert(typeinfo(T).fields.len == n, \"field count drifted\")\n    true\n}\n\n\
         struct Pair {\n    a: int,\n    b: int,\n}\n\n\
         fn main() -> !int {\n    const OK = expect_fields(Pair, 3)\n    \
         if OK { 0 } else { 1 }\n}\n",
    );
}

// --------------------------------------- staged diagnostics (E0507) ----

/// An error inside *generated* code renders the generator's provenance
/// chain: who generated it, invoked from where — the reader debugs the
/// generator, not a ghost source line.
#[test]
fn generated_impl_error_carries_provenance_chain() {
    let src = "\
trait Eq {\n    fn eq(a: Self, b: Self) -> int\n}\n\n\
struct Pt {\n    x: int,\n    y: int,\n}\n\n\
fn main() -> !int { 0 }\n";
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    let m = tc.bodies[0].body.module;
    // The "call site" of the generator: the `Pt` in `struct Pt`.
    let lo = src.find("Pt").expect("Pt occurs") as u32;
    let site = wolf_span::Span::new(res.package.files[0].raw.file, lo, lo + 2);
    let prov = Provenance {
        generator: "derive_eq".to_string(),
        call_site: site,
    };
    let tr = TraitRef {
        module: m,
        name: "Eq".to_string(),
    };
    // `derive_eq` generates `eq(a, b) -> bool`; this trait wants
    // `-> int` — ordinary s14 conformance rejects the generated impl.
    let generated = derive_eq(&tc.sigs, m, "Pt", &tr, &prov).expect("Pt reflects fine");
    let diags = check_generated(&res.package, &tc.sigs, generated, &prov);
    assert!(!diags.is_empty(), "the generated impl must not conform");
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in &diags {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    insta::assert_snapshot!("staged_provenance_chain", out);
}
