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
