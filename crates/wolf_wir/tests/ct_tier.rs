//! The constant-time tier's red/green suite (c28, spec/09).
//!
//! One red test per sink class of `[ct.taint.sink]` — E1601 through
//! E1606, each with its rendered diagnostic as a reviewed snapshot
//! (s10 catalog discipline; `cargo xtask diag-catalog` enforces the
//! pairing) — plus the green shapes: the honest branch-free kernel,
//! the `public(…)` exemption, secret-into-secret composition, and the
//! off-by-default proof (the same violating body without the
//! attribute verifies nothing and refuses nothing). The carried
//! contract round-trips the canonical text, and the mid-end's
//! inlining barrier holds.

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};
use wolf_wir::ct::{CtSink, CtViolation};

/// Lower one single-file program to WIR and run the ct verifier.
/// Panics on any rung failure before the tier — these fixtures are
/// mem-clean by construction.
fn ct_check(src: &str) -> (Vec<CtViolation>, Sources, wolf_wir::Module) {
    let mut ml = MemoryLoader::new("ct");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        !res.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error),
        "fixture must resolve: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty() && !tc.has_errors(),
        "fixture must typecheck"
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(mem.not_yet.is_empty(), "fixture must mem-check");
    let build = wolf_wir::lower_package(&res.package, &tc);
    assert!(build.not_yet.is_empty(), "fixture must lower");
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    (
        wolf_wir::ct::check_module(&build.module),
        sources,
        build.module,
    )
}

fn render(violations: &[CtViolation], sources: &Sources) -> String {
    let fallback = wolf_span::Span::new(wolf_span::FileId::from_index(0), 0, 0);
    let mut out = String::new();
    for v in violations {
        out.push_str(&render_human(
            &v.diagnostic(fallback),
            sources,
            &RenderOptions::default(),
        ));
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------- red --

#[test]
fn e1601_branch_on_secret_refused() {
    let (v, sources, _) = ct_check(
        "#[consttime]\nfn leak(k: int) -> int {\n    if k == 0 { 1 } else { 0 }\n}\n\nfn main() -> !int {\n    leak(3)\n}\n",
    );
    assert_eq!(v.first().map(|v| v.sink), Some(CtSink::Branch));
    insta::assert_snapshot!("e1601_branch_on_secret", render(&v, &sources));
}

#[test]
fn e1602_secret_index_refused() {
    let (v, sources, _) = ct_check(
        "#[consttime]\nfn pick(xs: List[int], k: int) -> int {\n    xs[k]\n}\n\nfn main() -> !int {\n    var xs = List[int]()\n    (mut xs).push(4)\n    pick(xs, 0)\n}\n",
    );
    assert_eq!(v.first().map(|v| v.sink), Some(CtSink::MemAddr));
    insta::assert_snapshot!("e1602_secret_index", render(&v, &sources));
}

#[test]
fn e1603_secret_call_target_refused() {
    let (v, sources, _) = ct_check(
        "#[consttime]\nfn dispatch(f: fn(int) -> int, x: int) -> int {\n    f(x)\n}\n\nfn id(v: int) -> int { v }\n\nfn main() -> !int {\n    dispatch(id, 1) - 1\n}\n",
    );
    assert_eq!(v.first().map(|v| v.sink), Some(CtSink::CallTarget));
    insta::assert_snapshot!("e1603_secret_call_target", render(&v, &sources));
}

#[test]
fn e1604_div_by_secret_refused() {
    let (v, sources, _) = ct_check(
        "#[consttime]\nfn residue(k: int) -> int {\n    k % 3\n}\n\nfn main() -> !int {\n    residue(5)\n}\n",
    );
    assert_eq!(v.first().map(|v| v.sink), Some(CtSink::DivRem));
    insta::assert_snapshot!("e1604_div_by_secret", render(&v, &sources));
}

#[test]
fn e1605_membrane_refused() {
    let (v, sources, _) = ct_check(
        "fn helper(v: wrapping[u64]) -> wrapping[u64] { v }\n\n#[consttime]\nfn leaky(k: wrapping[u64]) -> wrapping[u64] {\n    helper(k)\n}\n\nfn main() -> !int {\n    let r = leaky(3)\n    if r == 3 { 0 } else { 1 }\n}\n",
    );
    assert_eq!(v.first().map(|v| v.sink), Some(CtSink::Membrane));
    insta::assert_snapshot!("e1605_membrane", render(&v, &sources));
}

#[test]
fn e1605_secret_into_public_param_refused() {
    // The membrane's second face: a consttime callee's `public(…)`
    // parameter is a license to branch, so a secret argument may not
    // land there.
    let (v, _, _) = ct_check(
        "#[consttime(public(n))]\nfn fold(k: wrapping[u64], n: int) -> wrapping[u64] {\n    k | n as wrapping[u64]\n}\n\n#[consttime]\nfn outer(secret_len: int) -> wrapping[u64] {\n    fold(1, secret_len)\n}\n\nfn main() -> !int {\n    let r = outer(2)\n    if r == 3 { 0 } else { 1 }\n}\n",
    );
    assert_eq!(v.first().map(|v| v.sink), Some(CtSink::Membrane));
}

#[test]
fn e1606_checked_arith_on_secret_refused() {
    let (v, sources, _) = ct_check(
        "#[consttime]\nfn bump(k: int) -> int {\n    k + 1\n}\n\nfn main() -> !int {\n    bump(5) - 6\n}\n",
    );
    assert_eq!(v.first().map(|v| v.sink), Some(CtSink::CheckedArith));
    insta::assert_snapshot!("e1606_checked_on_secret", render(&v, &sources));
}

// -------------------------------------------------------------- green --

#[test]
fn honest_wrapping_kernel_verifies() {
    // The accumulate-then-single-check shape: XOR/OR fold in
    // wrapping[u64], the one deciding comparison in the (non-ct)
    // caller. Zero violations.
    let (v, _, _) = ct_check(
        "#[consttime]\nfn tag_diff(a0: wrapping[u64], a1: wrapping[u64], b0: wrapping[u64], b1: wrapping[u64]) -> wrapping[u64] {\n    (a0 ^ b0) | (a1 ^ b1)\n}\n\nfn main() -> !int {\n    let d = tag_diff(7, 9, 7, 9)\n    if d == 0 { 0 } else { 1 }\n}\n",
    );
    assert!(v.is_empty(), "honest kernel refused: {v:?}");
}

#[test]
fn public_param_licenses_control_flow() {
    // `public(n)`: loop bounds on the exempted parameter are licensed;
    // the secret rides arithmetic only.
    let (v, _, _) = ct_check(
        "#[consttime(public(n))]\nfn fold(k: wrapping[u64], n: int) -> wrapping[u64] {\n    var acc: wrapping[u64] = 0\n    var i = 0\n    while i < n {\n        acc = acc | k\n        i = i + 1\n    }\n    acc\n}\n\nfn main() -> !int {\n    let r = fold(5, 3)\n    if r == 5 { 0 } else { 1 }\n}\n",
    );
    assert!(v.is_empty(), "public-param kernel refused: {v:?}");
}

#[test]
fn secret_into_secret_param_composes() {
    // A consttime callee's SECRET parameter accepts secret arguments
    // freely — the contract composes ([ct.taint.membrane]).
    let (v, _, _) = ct_check(
        "#[consttime]\nfn mask(k: wrapping[u64]) -> wrapping[u64] {\n    k & 0xff\n}\n\n#[consttime]\nfn outer(k: wrapping[u64]) -> wrapping[u64] {\n    mask(k ^ 0x55)\n}\n\nfn main() -> !int {\n    let r = outer(0)\n    if r == 0x55 { 0 } else { 1 }\n}\n",
    );
    assert!(
        v.is_empty(),
        "secret-into-secret composition refused: {v:?}"
    );
}

#[test]
fn off_by_default_is_free() {
    // The SAME violating body without the attribute: the verifier
    // walks nothing, refuses nothing — the tier costs zero unless
    // asked for ([ct.attr.fn]).
    let (v, _, m) = ct_check(
        "fn leak(k: int) -> int {\n    if k == 0 { 1 } else { 0 }\n}\n\nfn main() -> !int {\n    leak(3)\n}\n",
    );
    assert!(v.is_empty());
    assert!(m.funcs.values().all(|f| f.consttime.is_none()));
}

// ----------------------------------------------------- the carried form --

#[test]
fn contract_rides_the_canonical_text() {
    let (_, _, m) = ct_check(
        "#[consttime(public(n))]\nfn fold(k: wrapping[u64], n: int) -> wrapping[u64] {\n    k | n as wrapping[u64]\n}\n\nfn main() -> !int {\n    let r = fold(1, 2)\n    if r == 3 { 0 } else { 1 }\n}\n",
    );
    let printed = wolf_wir::print_module(&m);
    assert!(
        printed.contains("consttime(0) fn @fold"),
        "the contract must print — param 0 secret, param 1 public:\n{printed}"
    );
    let reparsed = wolf_wir::parse_module(&printed).expect("canonical dump reparses");
    assert_eq!(
        wolf_wir::print_module(&reparsed),
        printed,
        "print -> parse -> print fixpoint with the contract"
    );
    let f = reparsed
        .funcs
        .values()
        .find(|f| f.name == "fold")
        .expect("fold survives");
    assert_eq!(
        f.consttime.as_ref().map(|c| c.secret_params.clone()),
        Some(vec![0]),
        "the parsed contract matches the printed one"
    );
}

#[test]
fn hand_written_text_accepts_the_contract() {
    let text = "consttime(0) fn @k(i64) -> i64 {\nb0(%0: i64):\n  ret %0\n}\n";
    let m = wolf_wir::parse_module(text).expect("hand-written consttime header parses");
    let f = m.funcs.values().next().expect("one fn");
    assert_eq!(
        f.consttime.as_ref().map(|c| c.secret_params.clone()),
        Some(vec![0])
    );
    // And an out-of-range index is refused, not absorbed.
    let bad = "consttime(3) fn @k(i64) -> i64 {\nb0(%0: i64):\n  ret %0\n}\n";
    assert!(wolf_wir::parse_module(bad).is_err());
}

// ----------------------------------------------------------- the barrier --

#[test]
fn midend_never_dissolves_a_consttime_fn() {
    // [ct.attr.barrier]: the mid-end neither inlines the marked fn
    // into its caller nor inlines callees into it. After the full
    // pipeline the marked fn is still present, still marked, and the
    // caller still calls it.
    let (v, _, mut m) = ct_check(
        "#[consttime]\nfn mask(k: wrapping[u64]) -> wrapping[u64] {\n    k & 0xff\n}\n\nfn main() -> !int {\n    let r = mask(0x155)\n    if r == 0x55 { 0 } else { 1 }\n}\n",
    );
    assert!(v.is_empty());
    wolf_wir::midend::optimize_module(&mut m, &wolf_wir::midend::Options::default())
        .expect("mid-end runs");
    let mask = m
        .funcs
        .values()
        .find(|f| f.name == "mask")
        .expect("the consttime fn survives the mid-end");
    assert!(
        mask.consttime.is_some(),
        "the contract survives the mid-end"
    );
    let main = m
        .funcs
        .values()
        .find(|f| f.name == "main")
        .expect("main survives");
    let calls_mask = main.layout.iter().any(|&b| {
        main.blocks[b].insts.iter().any(|&i| {
            main.insts[i].op == wolf_wir::Opcode::Call
                && matches!(main.insts[i].aux, wolf_wir::Aux::Callee(ef)
                    if main.ext_funcs[ef].name == "mask")
        })
    });
    assert!(calls_mask, "the call must stay a call — never inlined away");
    // And the optimized form still verifies clean under the tier.
    assert!(wolf_wir::ct::check_module(&m).is_empty());
}
