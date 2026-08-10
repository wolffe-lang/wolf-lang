//! The D2 posture, mechanized: passes may rely on facts and may not
//! drop them. `run_pass` snapshots the fact table around a pass and
//! rejects any loss on a still-live value that lacks a justified
//! invalidation — exercised here with a deliberately lossy no-op pass
//! (s24 acceptance).

use wolf_wir::{ErrClass, Invalidation, entity::PrimaryMap, parse_module, run_pass};

const SRC: &str = "fn @f(mut ptr, read ptr) -> i64 {\n  fact noalias %p %q : excl.mut\n  fact frozen %q : frozen.read\nb0(%p: ptr, %q: ptr):\n  %z = iconst.i64 0\n  ret %z\n}\n";

#[test]
fn honest_pass_keeps_its_facts() {
    let mut m = parse_module(SRC).expect("parses");
    let func = m.funcs.keys().next().expect("one function");
    run_pass(&mut m, func, "honest-no-op", |_f, _ctx| {}).expect("facts intact");
}

#[test]
fn lossy_pass_is_rejected() {
    let mut m = parse_module(SRC).expect("parses");
    let func = m.funcs.keys().next().expect("one function");
    let err = run_pass(&mut m, func, "lossy-no-op", |f, _ctx| {
        // A "no-op" that quietly throws the fact table away — exactly
        // what LLVM metadata channels permit and WIR forbids.
        f.facts = PrimaryMap::new();
    })
    .expect_err("dropped facts must be rejected");
    assert_eq!(err.class, ErrClass::DroppedFact);
    assert!(err.msg.contains("lossy-no-op"), "{}", err.msg);
    assert!(err.msg.contains("noalias"), "names the fact: {}", err.msg);
}

#[test]
fn justified_invalidation_is_accepted() {
    let mut m = parse_module(SRC).expect("parses");
    let func = m.funcs.keys().next().expect("one function");
    let ids: Vec<_> = m.funcs[func].facts.keys().collect();
    run_pass(&mut m, func, "region-freeing", move |f, ctx| {
        f.facts = PrimaryMap::new();
        for id in &ids {
            ctx.invalidate(*id, Invalidation::RegionFreed);
        }
    })
    .expect("justified invalidation passes");
}

#[test]
fn bogus_value_deleted_claim_is_rejected() {
    let mut m = parse_module(SRC).expect("parses");
    let func = m.funcs.keys().next().expect("one function");
    let ids: Vec<_> = m.funcs[func].facts.keys().collect();
    let err = run_pass(&mut m, func, "liar", move |f, ctx| {
        f.facts = PrimaryMap::new();
        for id in &ids {
            // Claims the subject value is gone — it is not.
            ctx.invalidate(*id, Invalidation::ValueDeleted);
        }
    })
    .expect_err("unjustified value-deleted claim");
    assert_eq!(err.class, ErrClass::DroppedFact);
    assert!(err.msg.contains("still exists"), "{}", err.msg);
}
