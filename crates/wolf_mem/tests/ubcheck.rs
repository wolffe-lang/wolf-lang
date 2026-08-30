//! The s23 UB-coverage gate: every `[mem.ub]` row this machine can
//! reach (P1–P6, L1, L2, T1) gets a **flag** test (the checker detects
//! the row) and a **near-miss** test (the closest legal program, which
//! the checker passes — the SB spurious-UB lesson, enforced as test
//! discipline). T2 and C1 are out of this machine's single-threaded
//! scope and are asserted so, never silently absent.
//!
//! Each program is unsafe-tier and statically ACCEPTED (the static
//! tier gates only the surface; UB is dynamic by design,
//! `[mem.unsafe.raw.1]`) — the harness asserts `mem` is clean before
//! executing, so a row that fires is a genuine dynamic verdict, not a
//! static rejection leaking through.

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_mem::ubcheck::{self, Budget, UbRow, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

/// Render the E1401 diagnostic for a program's UB finding (the s10
/// reviewed-artifact rule: every diagnostic ships a snapshot fixture).
/// Panics unless the program reaches a UB verdict.
fn render_ub(src: &str) -> String {
    let mut ml = MemoryLoader::new("ub");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    let out = ubcheck::run_checked(&res.package, &tc, Budget::default()).expect("within surface");
    let Verdict::Ub(f) = out.verdict else {
        panic!("expected a UB verdict for the snapshot input");
    };
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    render_human(
        &ubcheck::ub_diagnostic(&f),
        &sources,
        &RenderOptions::default(),
    )
}

/// Run a single-file program under the UB machine. Panics if the file
/// does not pass every rung up to and including `mem` (the inputs must
/// be statically accepted — the point is dynamic detection).
fn run(src: &str) -> Verdict {
    let mut ml = MemoryLoader::new("ub");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input resolves clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty(),
        "input typechecks fully: {:?}",
        tc.not_yet
    );
    assert!(
        !tc.has_errors(),
        "input typechecks clean: {:?}",
        tc.diagnostics
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(
        mem.not_yet.is_empty(),
        "input stays inside the mem surface: {:?}",
        mem.not_yet
    );
    assert!(
        mem.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input is statically ACCEPTED (UB is dynamic): {:?}",
        mem.diagnostics
    );
    ubcheck::run_checked(&res.package, &tc, Budget::default())
        .expect("the program is within the executable surface")
        .verdict
}

fn assert_row(src: &str, row: UbRow) {
    match run(src) {
        Verdict::Ub(f) => assert_eq!(
            f.row,
            row,
            "expected row {}, got {} ({})",
            row.as_str(),
            f.row.as_str(),
            f.message
        ),
        other => panic!("expected ub({}), got {other:?}", row.as_str()),
    }
}

fn assert_exit(src: &str, code: u8) {
    match run(src) {
        Verdict::Exit(n) => assert_eq!(n, code, "exit code"),
        other => panic!("expected exit({code}), got {other:?}"),
    }
}

const IMPORT: &str = "import c \"stdlib.h\"\n";

// ------------------------------------- F-0048: verdict determinism --

/// wolf-lang#42 (wolf-std F-0048): the checked lane's verdict is a
/// pure function of the program. The sensitive shape is two modules
/// exporting the same fn name — std.str and std.strbuf both export
/// `len` — where the machine's name-only fallback once walked a
/// `HashMap` in hash order: `alpha.len("wolf")` sometimes reached
/// beta's `len(b: Buf)` instead and refused ("place projection
/// outside the modelled surface"), so the SAME program answered `run`
/// or `unsupported` at random. Resolution now goes through the
/// checker's declaration locus (`CallSig::decl_span`); this rebuilds
/// the package per iteration (fresh hash seeds each time) and demands
/// one identical, CORRECT verdict every time.
#[test]
fn f0048_same_named_fns_across_modules_verdict_is_stable() {
    let files: &[(&[&str], &str, &str)] = &[
        (
            &[],
            "main.lu",
            "use alpha\n\nfn main() -> !int {\n    \
             if alpha.len(\"wolf\") == 4 { 0 } else { 1 }\n}\n",
        ),
        (
            &["alpha"],
            "alpha.lu",
            "use beta\n\npub fn len(s: str) -> int {\n    s.len\n}\n\n\
             pub fn blank_len() -> int {\n    beta.len(beta.mk())\n}\n",
        ),
        (
            &["beta"],
            "beta.lu",
            "pub struct Buf {\n    s: str,\n}\n\n\
             pub fn mk() -> Buf {\n    Buf { s: \"\" }\n}\n\n\
             pub fn len(b: Buf) -> int {\n    b.s.len\n}\n",
        ),
    ];
    let mut verdicts = Vec::new();
    for _ in 0..24 {
        let mut ml = MemoryLoader::new("f0048");
        for (m, n, s) in files {
            ml.add_file(m, n, s);
        }
        let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
        assert!(
            res.diagnostics
                .iter()
                .all(|d| d.severity != wolf_diag::Severity::Error),
            "input resolves clean: {:?}",
            res.diagnostics
        );
        let tc = typecheck_package_with(&res.package, true);
        assert!(
            !tc.has_errors(),
            "input typechecks clean: {:?}",
            tc.diagnostics
        );
        verdicts.push(
            match ubcheck::run_checked(&res.package, &tc, Budget::default()) {
                Ok(out) => match out.verdict {
                    Verdict::Exit(c) => format!("exit({c})"),
                    other => format!("{other:?}"),
                },
                Err(ny) => format!("unsupported: {}", ny.construct),
            },
        );
    }
    assert!(
        verdicts.iter().all(|v| v == "exit(0)"),
        "checked verdicts must be deterministic and correct: {verdicts:?}"
    );
}

// ------------------------------------------------------------- P1 UAF --

#[test]
fn e1401_uaf_diagnostic() {
    // The reviewed-artifact snapshot for E1401: the row, the
    // responsible operation, and the licensed optimization it breaks.
    let rendered = render_ub(&format!(
        "{IMPORT}fn main() -> !int {{\n\
         unsafe {{\n\
         let p = c.malloc(8) as *u8\n\
         p[0] = 7\n\
         c.free(p)\n\
         let v = p[0]\n\
         v as int\n\
         }}\n}}\n"
    ));
    insta::assert_snapshot!("e1401_uaf", rendered);
}

#[test]
fn p1_use_after_free_flags() {
    // The corpus's own litmus shape: read through a freed pointer.
    assert_row(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             p[0] = 7\n\
             c.free(p)\n\
             let v = p[0]\n\
             v as int\n\
             }}\n}}\n"
        ),
        UbRow::P1,
    );
}

#[test]
fn p1_near_miss_read_before_free_passes() {
    assert_exit(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             p[0] = 7\n\
             let v = p[0]\n\
             c.free(p)\n\
             v as int\n\
             }}\n}}\n"
        ),
        7,
    );
}

// ---------------------------------------------------------- P2 Frozen --

#[test]
fn p2_write_through_frozen_flags() {
    // A frozen region's backing carries Frozen tags; a raw write
    // through it is P2.
    assert_row(
        "fn main() -> !int {\n\
         let r = region()\n\
         let f = freeze r\n\
         unsafe {\n\
         let p = f as *u8\n\
         p[0] = 1\n\
         0\n\
         }\n}\n",
        UbRow::P2,
    );
}

#[test]
fn p2_near_miss_read_through_frozen_passes() {
    assert_exit(
        "fn main() -> !int {\n\
         let r = region()\n\
         let f = freeze r\n\
         unsafe {\n\
         let p = f as *u8\n\
         let v = p[0]\n\
         v as int\n\
         }\n}\n",
        0,
    );
}

// ------------------------------------------------------------ P3 OOB --

#[test]
fn p3_out_of_bounds_flags() {
    assert_row(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             let v = p[16]\n\
             v as int\n\
             }}\n}}\n"
        ),
        UbRow::P3,
    );
}

#[test]
fn p3_near_miss_in_bounds_passes() {
    assert_exit(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             p[7] = 3\n\
             let v = p[7]\n\
             v as int\n\
             }}\n}}\n"
        ),
        3,
    );
}

// ---------------------------------------------------- P4 region freed --

#[test]
fn p4_access_after_region_free_flags() {
    // A pointer with provenance into a region's backing, accessed
    // after that region's binding scope frees it.
    assert_row(
        "fn main() -> !int {\n\
         var out = 0\n\
         unsafe {\n\
         let p = {\n\
         let inner = region()\n\
         inner as *u8\n\
         }\n\
         let v = p[0]\n\
         out = v as int\n\
         }\n\
         out\n}\n",
        UbRow::P4,
    );
}

#[test]
fn p4_near_miss_region_alive_passes() {
    assert_exit(
        "fn main() -> !int {\n\
         let inner = region()\n\
         unsafe {\n\
         let p = inner as *u8\n\
         let v = p[0]\n\
         v as int\n\
         }\n}\n",
        0,
    );
}

// ----------------------------------------------------- P5 false noalias --

#[test]
fn p5_false_noalias_flags() {
    assert_row(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             let q = p\n\
             assume noalias p, q\n\
             0\n\
             }}\n}}\n"
        ),
        UbRow::P5,
    );
}

#[test]
fn p5_near_miss_distinct_allocations_passes() {
    // The corpus's unsafe_noalias shape: two distinct allocations.
    assert_exit(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             let q = c.malloc(8) as *u8\n\
             assume noalias p, q\n\
             p[0] = 1\n\
             q[0] = 2\n\
             c.free(p)\n\
             c.free(q)\n\
             0\n\
             }}\n}}\n"
        ),
        0,
    );
}

// -------------------------------------------------------- P6 false door --

#[test]
fn p6_false_door_flags() {
    assert_row(
        "fn main() -> !int {\n\
         let r = region()\n\
         let s = region()\n\
         unsafe {\n\
         let p = s as *u8\n\
         let v = borrow r from p\n\
         v as int\n\
         }\n}\n",
        UbRow::P6,
    );
}

#[test]
fn p6_near_miss_true_door_passes() {
    // The corpus unsafe_door_borrow shape: p is r's own base.
    assert_exit(
        "fn main() -> !int {\n\
         let r = region()\n\
         unsafe {\n\
         let p = r as *u8\n\
         let v = borrow r from p\n\
         v as int\n\
         }\n}\n",
        0,
    );
}

// ------------------------------------------------------------ L1 uninit --

#[test]
fn l1_uninitialized_read_flags() {
    assert_row(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             let v = p[0]\n\
             v as int\n\
             }}\n}}\n"
        ),
        UbRow::L1,
    );
}

#[test]
fn l1_near_miss_written_first_passes() {
    assert_exit(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             p[0] = 4\n\
             let v = p[0]\n\
             v as int\n\
             }}\n}}\n"
        ),
        4,
    );
}

// ---------------------------------------------------------- L2 dangling --

#[test]
fn l2_double_free_flags() {
    assert_row(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             c.free(p)\n\
             c.free(p)\n\
             0\n\
             }}\n}}\n"
        ),
        UbRow::L2,
    );
}

#[test]
fn l2_near_miss_single_free_passes() {
    assert_exit(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let p = c.malloc(8) as *u8\n\
             p[0] = 1\n\
             c.free(p)\n\
             0\n\
             }}\n}}\n"
        ),
        0,
    );
}

// -------------------------------------------------------- T1 invalid bool --

#[test]
fn t1_invalid_bool_flags() {
    assert_row(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let raw = c.malloc(1) as *u8\n\
             raw[0] = 5\n\
             let b = raw as *bool\n\
             let v = b[0]\n\
             if v {{ 1 }} else {{ 0 }}\n\
             }}\n}}\n"
        ),
        UbRow::T1,
    );
}

#[test]
fn t1_near_miss_valid_bool_passes() {
    assert_exit(
        &format!(
            "{IMPORT}fn main() -> !int {{\n\
             unsafe {{\n\
             let raw = c.malloc(1) as *u8\n\
             raw[0] = 1\n\
             let b = raw as *bool\n\
             let v = b[0]\n\
             if v {{ 0 }} else {{ 1 }}\n\
             }}\n}}\n"
        ),
        0,
    );
}

// ---------------------------------------------- out-of-scope rows, stated --

/// T2 (torn writes) and C1 (data races) are unreachable in this
/// single-threaded machine — the sprint's honest-scope contract. This
/// is asserted structurally (the `UbRow` enum has no T2/C1 constructor)
/// rather than left as an unexplained absence: the concurrency campaign
/// (ic03/C1) makes them reachable.
#[test]
fn t2_and_c1_are_out_of_single_threaded_scope() {
    // Enumerate the reachable rows: exactly the nine this machine
    // detects. T2/T-tearing and C1/race need a second observer.
    let reachable = [
        UbRow::P1,
        UbRow::P2,
        UbRow::P3,
        UbRow::P4,
        UbRow::P5,
        UbRow::P6,
        UbRow::L1,
        UbRow::L2,
        UbRow::T1,
    ];
    assert_eq!(reachable.len(), 9);
    // Each row names a licensed optimization (the D2 pairing) and a
    // clause — the executable half of the spec's §7 table.
    for r in reachable {
        assert!(!r.licensed().is_empty(), "row {} names its O#", r.as_str());
        assert!(
            !r.clause().is_empty(),
            "row {} names its clause",
            r.as_str()
        );
    }
}

// ------------------------------------- s71: the fold reaches the lane --

/// A module holding `comptime fn`s executes on the checked lane, and
/// each fold's value arrives as an ordinary constant: an int fold, a
/// reflection fold (`typeinfo(T).fields.len` — the intrinsic never
/// reaches the machine), and a str fold built with comptime
/// interpolation, `str.get` behind an `else` handler, and `str.len`.
#[test]
fn comptime_folds_execute_checked() {
    assert_exit(
        "struct Howl {\n    pitch: int,\n    length: int,\n    at_moon: bool,\n}\n\
         comptime fn sum_squares(n: int) -> int {\n    var acc = 0\n    var i = 1\n    \
         while i <= n {\n        acc += i * i\n        i += 1\n    }\n    acc\n}\n\
         comptime fn field_count(T: type) -> int {\n    typeinfo(T).fields.len\n}\n\
         comptime fn expand(axiom: str, steps: int) -> str {\n    var cur = axiom\n    \
         var step = 0\n    while step < steps {\n        var next = \"\"\n        var i = 0\n        \
         while i < cur.len {\n            let ch = cur.get(i..i + 1) else |_| { \"\" }\n            \
         if ch == \"A\" {\n                next = \"{next}A-B\"\n            } else {\n                \
         next = \"{next}{ch}\"\n            }\n            i += 1\n        }\n        \
         cur = next\n        step += 1\n    }\n    cur\n}\n\
         fn main() -> !int {\n    const T = sum_squares(9)\n    const N = field_count(Howl)\n    \
         const CURVE = expand(\"A\", 2)\n    \
         if T != 285 { return 1 }\n    if N != 3 { return 2 }\n    \
         if CURVE != \"A-B-B\" { return 3 }\n    0\n}\n",
        0,
    );
}

// ------------------- s72 posture: the D39/D40 mode-rule shapes ----
//
// The three s72 shapes are rejected STATICALLY (E1014, E1002's
// overlap half, E1013), so the driver's checked lane never executes
// them; these tests drive the machine directly to pin what it models
// today — the contract's "verify, don't assume". Verified: this
// machine does NOT model the mode claims dynamically. `read`
// arguments copy scalars and structs (a callee write lands on the
// copy), a `mut` lend coexists with a later argument read of the same
// place, and `for` iterates a loop-entry snapshot of the list. (The
// verification also caught a real disagreement: eval_for consumed a
// place iterable through reads-as-moves — the machine-side twin of
// the #15 accident — which would have refused the newly-legal
// mutate-AFTER-the-loop shape; it reads without consuming now, and
// `iterate_then_mutate_agrees_checked` below pins the repair.) All
// three shapes run clean here while lupin 0.1.1 runs two of them
// clean (it traps f(mut a, a.x) already); the v0.1.8 mirrors bring
// the traps. When this machine grows the claims, these pins fail
// loudly and move deliberately.

/// Run WITHOUT the static-acceptance gate: the s72 shapes fail the
/// mem rung by design, and the dynamic machine itself is the subject.
fn run_past_static(src: &str) -> Verdict {
    let mut ml = MemoryLoader::new("ub");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty() && !tc.has_errors(),
        "input typechecks fully (only the MEM rung rejects it)"
    );
    ubcheck::run_checked(&res.package, &tc, Budget::default())
        .expect("the program is within the executable surface")
        .verdict
}

#[test]
fn posture_write_through_read_param_lands_on_the_copy() {
    // E1014's shape (D39). Dynamically the machine passes `read`
    // structs by value: the callee's write mutates its own copy (the
    // caller never observes it), so the run exits clean — no trap.
    match run_past_static(
        "struct P { x: int, y: int }\n\
         fn poke(p: P) -> int {\n    p.x = 7\n    p.x\n}\n\
         fn main() -> !int {\n    \
             var v = P { x: 1, y: 2 }\n    \
             let n = poke(v)\n    \
             if n == 7 && v.x == 1 { 0 } else { 1 }\n\
         }\n",
    ) {
        Verdict::Exit(0) => {}
        other => panic!("posture moved — update the s72 record: {other:?}"),
    }
}

#[test]
fn posture_copy_read_after_mut_runs_clean() {
    // E1002's overlap shape (D39, f(mut a, a.x)). The machine reads
    // `p.x` from the intact allocation while `p` is lent — no claim
    // model, no trap. (lupin 0.1.1 already traps this one; the static
    // rejection is what makes the pair agree under [proto.cmp.rung].)
    match run_past_static(
        "struct P { x: int, y: int }\n\
         fn bump(mut a: P, n: int) { a.x += n }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2 }\n    \
             bump(mut p, p.x)\n    \
             p.x - 2\n\
         }\n",
    ) {
        Verdict::Exit(0) => {}
        other => panic!("posture moved — update the s72 record: {other:?}"),
    }
}

#[test]
fn iterate_then_mutate_agrees_checked() {
    // The newly-legal shape under [mem.iter.excl.1]: the container is
    // live after the walk. Statically accepted, so this runs through
    // the gated harness — checked lane and static tier agree.
    assert_exit(
        "fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             (mut xs).push(1)\n    \
             (mut xs).push(2)\n    \
             var total = 0\n    \
             for x in xs {\n        \
                 total += x\n    \
             }\n    \
             (mut xs).push(total)\n    \
             if xs.len == 3 && total == 3 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn posture_mutate_while_iterating_walks_the_snapshot() {
    // E1013's shape (D40, the F-0014 program). The machine clones the
    // list at loop entry, so the pushes never feed the walk: the loop
    // terminates and the run exits clean — the same loop-entry-copy
    // reading lupin 0.1.1 exhibits, and exactly what [mem.iter.excl]
    // now forbids ahead of it.
    match run_past_static(
        "fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             (mut xs).push(1)\n    \
             (mut xs).push(2)\n    \
             for x in xs {\n        \
                 (mut xs).push(x)\n    \
             }\n    \
             if xs.len == 4 { 0 } else { 1 }\n\
         }\n",
    ) {
        Verdict::Exit(0) => {}
        other => panic!("posture moved — update the s72 record: {other:?}"),
    }
}

// ------------------------- #130: wrapping shifts/bitwise (s111) --

/// The checked tier executes `wrapping[T]` shifts and bitwise mixes
/// with the native rung's semantics (wolf-lang#130 / F-0091): the
/// FIPS 180-4 Sigma0{256} rotation of SHA-256's initial `a`, the
/// `ch` mix over the initial `e f g`, and a full-width logical
/// shift — every value cross-checked against the sc16 vector
/// corpus's two executing lanes.
#[test]
fn wrapping_shift_bitwise_mirror_native() {
    assert_exit(
        "fn mask32() -> wrapping[u64] { 0xffffffff }\n\
         fn bsig0(x: wrapping[u64]) -> wrapping[u64] {\n    \
             ((x >> 2 | x << 30) ^ (x >> 13 | x << 19) ^ (x >> 22 | x << 10)) & mask32()\n\
         }\n\
         fn ch(x: wrapping[u64], y: wrapping[u64], z: wrapping[u64]) -> wrapping[u64] {\n    \
             (x & y) ^ ((x ^ mask32()) & z)\n\
         }\n\
         fn main() -> !int {\n    \
             let s = bsig0(0x6a09e667) as int\n    \
             let c = ch(0x510e527f, 0x9b05688c, 0x1f83d9ab) as int\n    \
             if s == 3458249854 && c == 528861580 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// `>>` on an unsigned wrapping type is a LOGICAL shift (zero-fill),
/// exactly the native rung's `lshr` — the top bit must not smear.
#[test]
fn wrapping_u64_shr_is_logical() {
    assert_exit(
        "fn main() -> !int {\n    \
             let big: wrapping[u64] = 0x8000000000000000\n    \
             if (big >> 63) as int == 1 && (big >> 1) as int == 4611686018427387904 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// Shift amounts mask to the bit width (the WIR `shl`/`lshr`
/// contract both backends implement): a shift by 64 on a 64-bit
/// wrapping value is a shift by zero.
#[test]
fn wrapping_shift_amount_masks_to_width() {
    assert_exit(
        "fn main() -> !int {\n    \
             let x: wrapping[u64] = 5\n    \
             if (x << 64) as int == 5 && (x >> 64) as int == 5 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// Compound bitwise/shift assignment reuses the same arms (the
/// native rung maps `<<=` to `shl` and friends).
#[test]
fn wrapping_compound_bitwise_assign() {
    assert_exit(
        "fn main() -> !int {\n    \
             var x: wrapping[u64] = 5\n    \
             x <<= 3\n    \
             x |= 1\n    \
             x ^= 0xf\n    \
             x &= 0xff\n    \
             x >>= 1\n    \
             if x as int == 19 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// A `wrapping[u64]` literal above `2^63 - 1` is a bit pattern, not
/// an overflow (#130's literal half): SHA-512's `K[15]` constant
/// round-trips through hi/lo halves.
#[test]
fn wrapping_u64_full_range_literal() {
    assert_exit(
        "fn main() -> !int {\n    \
             let k: wrapping[u64] = 0xc19bf174cf692694\n    \
             let hi = (k >> 32) as int\n    \
             let lo = (k & 0xffffffff) as int\n    \
             if hi == 3248222580 && lo == 3479774868 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// #131's checked twin: a cast to a WRAPPING target masks to the
/// width (never traps) — both directions witnessed, plus the sign
/// bit-pattern round-trip through the unsigned zero-extension.
#[test]
fn wrapping_narrow_cast_masks_to_width() {
    assert_exit(
        "fn main() -> !int {\n    \
             let a: int = 300\n    \
             let big: int = 0x1_0000_002c\n    \
             let neg: int = 0 - 1\n    \
             let in_range = (a as wrapping[u32]) as int\n    \
             let masked = (big as wrapping[u32]) as int\n    \
             let bits = (neg as wrapping[u32]) as int\n    \
             if in_range == 300 && masked == 44 && bits == 4294967295 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

// ------------------------------ #122: rows bind at let (s111) --

/// A RAW row value at `let` BINDS (D52's declared-row-first reading:
/// rows are values) — the s108-found divergence where this machine
/// answered `error: none`/exit 1 while native ran the handler.
#[test]
fn raw_row_at_let_binds_and_reaches_its_handler() {
    assert_exit(
        "fn main() -> !int {\n    \
             let v: int ! {none} = none\n    \
             let w = v else 5\n    \
             if w == 5 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// The hoisted shape from `rows/handler_diverge_trap.lu`'s header:
/// `let p = pick(0)` binds the row, and the bound value reaches the
/// payload handler through a parameter — the fallback runs.
#[test]
fn bound_row_crosses_a_call_to_its_handler() {
    assert_exit(
        "fn expect(v: int ! {none}, d: int) -> int {\n    \
             let hit = v else d\n    \
             hit\n\
         }\n\
         fn pick(x: int) -> int ! {none} {\n    \
             if x < 1 { return none }\n    \
             x\n\
         }\n\
         fn main() -> !int {\n    \
             let p = pick(0)\n    \
             let a = expect(p, 41)\n    \
             let q = pick(7)\n    \
             let b = expect(q, 0)\n    \
             if a == 41 && b == 7 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// `?` stays the one PROPAGATING consumer: through a `let`, a raw
/// error under `?` unwinds to the caller instead of binding.
#[test]
fn qmark_still_propagates_through_let() {
    assert_exit(
        "fn pick(x: int) -> int ! {none} {\n    \
             if x < 1 { return none }\n    \
             x\n\
         }\n\
         fn tryit(x: int) -> int ! {none} {\n    \
             let v = pick(x)?\n    \
             v + 100\n\
         }\n\
         fn main() -> !int {\n    \
             let ok = tryit(5) else 0 - 1\n    \
             let bad = tryit(0) else 0 - 1\n    \
             if ok == 105 && bad == 0 - 1 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// The assignment sibling, measured in the same s111 sweep: a raw
/// row value assigned to a row-typed `var` binds exactly as at
/// `let`.
#[test]
fn raw_row_at_assignment_binds() {
    assert_exit(
        "fn pick(x: int) -> int ! {none} {\n    \
             if x < 1 { return none }\n    \
             x\n\
         }\n\
         fn main() -> !int {\n    \
             var w: int ! {none} = none\n    \
             w = pick(0)\n    \
             let z = w else 0 - 7\n    \
             if z == 0 - 7 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// s128 (#173): a tuple pattern over a place moves each BOUND element
/// out of its own sub-place — `_` leaves its element live, so the
/// checked executor reads it after the destructure without a trap.
#[test]
fn tuple_destructure_moves_elements_not_the_whole() {
    assert_exit(
        "struct Inner { n: int }\n\
         fn main() -> !int {\n    \
             var p = (Inner { n: 1 }, Inner { n: 2 })\n    \
             let (x, _) = p\n    \
             let b = p.1.n\n    \
             if x.n + b == 3 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// Nested tuples and `_` discards bind element-wise (s128, #173).
#[test]
fn tuple_destructure_nested_and_wildcards() {
    assert_exit(
        "fn main() -> !int {\n    \
             let (a, (b, _), c) = (1, (2, 3), 4)\n    \
             if a + b + c == 7 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}
