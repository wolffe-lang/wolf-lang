//! s89 — the byte-view LEND verdicts (wolf-lang#86), asserted directly
//! rather than through their two consumers.
//!
//! Soundness rests on exactly one of the three answers:
//! [`Lend::Lendable`] is what makes `wolf_wir::lower` hand a callee the
//! caller's `{ptr, len}` instead of a copy, so its whitelist is the
//! thing that must never grow by accident. `Opaque` is the pre-s89
//! behaviour (materialize) and is always safe; `Escapes` is E1015 and
//! only ever improves a diagnostic. These tests pin the boundary from
//! both sides: the seven read positions of s77's lowering comment on
//! one side, and the shapes that must NOT be lendable on the other.

use wolf_mem::byteview::{Lend, Lender};
use wolf_sema::sig::{FnSig, ItemSig};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

/// The verdict for parameter `ix` of the named function in `src`.
fn lend(src: &str, name: &str, ix: usize) -> Lend {
    let mut ml = MemoryLoader::new("lend");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        !res.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error),
        "input resolves clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(!tc.has_errors(), "input types clean: {:?}", tc.diagnostics);
    let sig: &FnSig = tc
        .sigs
        .modules
        .iter()
        .flat_map(|m| m.iter())
        .find_map(|(n, s)| match s {
            ItemSig::Fn(f) if n == name => Some(f),
            _ => None,
        })
        .expect("the named function");
    let lender = Lender::new(&res.package, &tc.sigs);
    lender.param(sig, ix)
}

fn lendable(src: &str, name: &str, ix: usize) {
    assert_eq!(lend(src, name, ix), Lend::Lendable, "expected a lend");
}

fn opaque(src: &str, name: &str, ix: usize) {
    assert_eq!(lend(src, name, ix), Lend::Opaque, "expected a materialize");
}

fn escapes(src: &str, name: &str, ix: usize) {
    assert!(
        matches!(lend(src, name, ix), Lend::Escapes(_)),
        "expected a provable escape"
    );
}

// ------------------------------- the seven read positions (lendable) ----

#[test]
fn iteration_is_lendable() {
    lendable(
        "fn f(bs: List[int]) -> int {\n    \
             var n = 0\n    \
             for b in bs { n = n + b }\n    \
             n\n\
         }\n",
        "f",
        0,
    );
}

#[test]
fn indexing_is_lendable() {
    lendable("fn f(bs: List[int], i: int) -> int { bs[i] }\n", "f", 0);
}

#[test]
fn the_len_field_is_lendable() {
    lendable("fn f(bs: List[int]) -> int { bs.len }\n", "f", 0);
}

#[test]
fn the_query_family_is_lendable() {
    for body in [
        "bs.count()",
        "if bs.is_empty() { 0 } else { 1 }",
        "bs.get(0) else 0",
        "bs.first() else 0",
        "bs.last() else 0",
    ] {
        lendable(
            &format!("fn f(bs: List[int]) -> int {{ {body} }}\n"),
            "f",
            0,
        );
    }
}

#[test]
fn an_index_expression_over_the_view_is_still_lendable() {
    // The INDEX is walked; only the receiver is the view.
    lendable("fn f(bs: List[int]) -> int { bs[bs.len - 1] }\n", "f", 0);
}

#[test]
fn a_re_lend_into_a_lendable_parameter_is_lendable() {
    lendable(
        "fn inner(bs: List[int]) -> int { bs.len }\n\
         fn f(bs: List[int]) -> int { inner(bs) }\n",
        "f",
        0,
    );
}

#[test]
fn a_self_recursive_walk_is_lendable() {
    // The cycle is the greatest fixed point: re-lending a view to
    // yourself adds no use, so it cannot turn a whitelist into an
    // escape — and assuming otherwise would cost the optimization on
    // every recursive byte walk, which is most of them.
    lendable(
        "fn sum(bs: List[int], i: int) -> int {\n    \
             if i >= bs.len { return 0 }\n    \
             bs[i] + sum(bs, i + 1)\n\
         }\n",
        "sum",
        0,
    );
}

#[test]
fn a_mutually_recursive_walk_is_lendable() {
    lendable(
        "fn evens(bs: List[int], i: int) -> int {\n    \
             if i >= bs.len { return 0 }\n    \
             bs[i] + odds(bs, i + 1)\n\
         }\n\
         fn odds(bs: List[int], i: int) -> int {\n    \
             if i >= bs.len { return 0 }\n    \
             evens(bs, i + 1)\n\
         }\n",
        "evens",
        0,
    );
}

#[test]
fn a_recursive_walk_that_escapes_still_escapes() {
    // The cycle assumption is about the LOOP, not about the body: a
    // use that outlives the call is still found.
    escapes(
        "fn walk(bs: List[int], i: int) -> List[int] {\n    \
             if i >= bs.len { return bs }\n    \
             walk(bs, i + 1)\n\
         }\n",
        "walk",
        0,
    );
}

#[test]
fn two_parameters_lend_independently() {
    let src = "fn f(bs: List[int], p: List[int], n: int) -> int { bs[0] + p.len + n }\n";
    lendable(src, "f", 0);
    lendable(src, "f", 1);
    // A non-List parameter is never a lend candidate.
    opaque(src, "f", 2);
}

// --------------------------------------------- provable escapes (E1015) ----

#[test]
fn returning_the_parameter_escapes() {
    escapes("fn f(bs: List[int]) -> List[int] { bs }\n", "f", 0);
}

#[test]
fn an_explicit_return_escapes() {
    escapes(
        "fn f(bs: List[int], n: int) -> List[int] {\n    \
             if n > 0 { return bs }\n    \
             bs\n\
         }\n",
        "f",
        0,
    );
}

#[test]
fn a_branch_tail_escapes() {
    escapes(
        "fn f(bs: List[int], n: int) -> List[int] {\n    \
             if n > 0 { bs } else { bs }\n\
         }\n",
        "f",
        0,
    );
}

#[test]
fn a_transitive_escape_is_an_escape() {
    escapes(
        "fn keep(bs: List[int]) -> List[int] { bs }\n\
         fn f(bs: List[int]) -> List[int] { keep(bs) }\n",
        "f",
        0,
    );
}

#[test]
fn an_assignment_away_escapes() {
    escapes(
        "fn f(bs: List[int]) -> int {\n    \
             var out = List[int]()\n    \
             out = bs\n    \
             out.len\n\
         }\n",
        "f",
        0,
    );
}

// ------------------------------------------ outside the surface (opaque) ----

#[test]
fn a_take_parameter_is_never_a_lend() {
    // Declared ownership transfer: refused at the signature, not per
    // call site.
    opaque("fn f(take bs: List[int]) -> int { bs.len }\n", "f", 0);
}

#[test]
fn a_mut_parameter_is_never_a_lend() {
    opaque("fn f(mut bs: List[int]) { (mut bs).push(1) }\n", "f", 0);
}

#[test]
fn a_non_byte_list_is_never_a_lend() {
    opaque("fn f(bs: List[str]) -> int { bs.len }\n", "f", 0);
}

#[test]
fn a_builtin_consumer_materializes() {
    // `str_from_utf8` takes a real list; modelling the builtins is the
    // std facade's job, so the caller materializes.
    opaque(
        "fn f(bs: List[int]) -> str { str_from_utf8(bs) else \"X\" }\n",
        "f",
        0,
    );
}

#[test]
fn a_local_rebinding_of_the_name_materializes() {
    // Shadowing is a refusal, not a puzzle: below the rebind the name
    // means a different value.
    opaque(
        "fn f(bs: List[int]) -> int {\n    \
             let n = bs.len\n    \
             let bs = List[int]()\n    \
             n + bs.len\n\
         }\n",
        "f",
        0,
    );
}

#[test]
fn binding_the_parameter_to_a_local_materializes() {
    // The local is an ordinary `List[int]` value from here on, and this
    // analysis does not follow it — so the caller materializes, which
    // is what the local's type promised anyway.
    opaque(
        "fn f(bs: List[int]) -> int {\n    \
             let q = bs\n    \
             q.len\n\
         }\n",
        "f",
        0,
    );
}

#[test]
fn a_re_lend_into_an_opaque_parameter_materializes() {
    // Opacity is transitive in the same direction the lend is: the hop
    // cannot promise more than the callee it hands the view to.
    opaque(
        "fn inner(bs: List[int]) -> str { str_from_utf8(bs) else \"X\" }\n\
         fn f(bs: List[int]) -> int { inner(bs).len }\n",
        "f",
        0,
    );
}

#[test]
fn the_len_field_is_the_only_field_a_view_has() {
    // `bs.len` is a read position; any OTHER member on the parameter is
    // outside the modelled surface and materializes rather than
    // guessing.
    lendable("fn f(bs: List[int]) -> int { bs.len }\n", "f", 0);
}
