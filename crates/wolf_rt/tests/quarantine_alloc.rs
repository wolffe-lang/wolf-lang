//! The debug quarantine allocator's behaviour (D21).
//!
//! # Seen red before it was trusted
//!
//! Every test in this file was run against the UNIMPLEMENTED bodies
//! first — the commit that adds this file renames the type and
//! nothing else, so `QuarantineAllocator`'s four hooks are still
//! `unimplemented!(…)` when these assertions first execute. The
//! recorded red is quoted in the module that implements them
//! (`crates/wolf_rt/src/quarantine.rs`), together with the command
//! and the commit it ran at, so a reader can re-run it.
//!
//! These are not a model of the allocator. They drive the shipping
//! type through its public trait, which is what makes them a gate on
//! it rather than a description of it.

use wolf_rt::quarantine::{
    FaultKind, QuarantineAllocator, QuarantineBudget, QuarantineHooks, RegionId, Tag,
};

fn alloc() -> QuarantineAllocator {
    QuarantineAllocator::new(QuarantineBudget::default())
}

#[test]
fn an_allocation_is_live_addressable_and_tagged() {
    let mut a = alloc();
    let (addr, tag) = a.alloc(8);
    assert_ne!(addr, 0, "a live granule has an address");
    assert_ne!(tag, Tag::UNTAGGED, "a live granule carries a nonzero tag");
    assert_eq!(a.check(addr, tag), Ok(()), "its own tag validates");
}

#[test]
fn the_granule_is_real_memory() {
    let mut a = alloc();
    let (addr, tag) = a.alloc(4);
    // SAFETY: `addr` is the base of a live 4-byte granule this
    // allocator just handed out and nothing else addresses it.
    unsafe {
        let p = addr as *mut u8;
        p.write(0xAB);
        p.add(3).write(0xCD);
        assert_eq!(p.read(), 0xAB);
        assert_eq!(p.add(3).read(), 0xCD);
    }
    assert_eq!(a.check(addr, tag), Ok(()));
}

#[test]
fn two_live_granules_do_not_overlap() {
    let mut a = alloc();
    let (p, _) = a.alloc(64);
    let (q, _) = a.alloc(64);
    assert!(p + 64 <= q || q + 64 <= p, "{p:#x} and {q:#x} overlap");
}

#[test]
fn an_interior_pointer_into_its_own_granule_is_live() {
    let mut a = alloc();
    let (addr, tag) = a.alloc(32);
    assert_eq!(a.check(addr + 31, tag), Ok(()));
}

#[test]
fn a_freed_granule_answers_use_after_free() {
    let mut a = alloc();
    let (addr, tag) = a.alloc(8);
    a.free(addr);
    assert_eq!(a.check(addr, tag), Err(FaultKind::UseAfterFree));
}

#[test]
fn a_free_retags_so_a_stale_tag_never_matches_again() {
    let mut a = alloc();
    let (addr, tag) = a.alloc(8);
    a.free(addr);
    let fresh = a
        .tag_at(addr)
        .expect("the granule is quarantined, not gone");
    assert_ne!(fresh, tag, "a free rotates the granule's tag");
    assert_ne!(
        fresh,
        Tag::UNTAGGED,
        "and the fresh tag is not the reserved one"
    );
}

#[test]
fn a_second_free_is_a_double_free() {
    let mut a = alloc();
    let (addr, tag) = a.alloc(8);
    a.free(addr);
    a.free(addr);
    assert_eq!(a.check(addr, tag), Err(FaultKind::DoubleFree));
}

#[test]
fn a_wholesale_region_free_answers_region_freed() {
    let mut a = alloc();
    a.set_region(RegionId(7));
    let (addr, tag) = a.alloc(16);
    a.set_region(RegionId(0));
    let (other, otag) = a.alloc(16);
    a.free_region(RegionId(7));
    assert_eq!(a.check(addr, tag), Err(FaultKind::RegionFreed));
    assert_eq!(a.check(other, otag), Ok(()), "another region is untouched");
}

#[test]
fn running_off_a_live_object_into_a_poisoned_span_is_out_of_bounds() {
    let mut a = alloc();
    let (dead, _) = a.alloc(64);
    a.free(dead);
    let (live, ltag) = a.alloc(64);
    // The live object's own tag, presented at an address inside the
    // quarantined neighbour: the checker's P3 shape.
    assert_eq!(a.check(dead, ltag), Err(FaultKind::OutOfBounds));
    assert_eq!(a.check(live, ltag), Ok(()));
}

#[test]
fn an_address_in_no_granule_at_all_is_out_of_bounds() {
    let mut a = alloc();
    let (addr, tag) = a.alloc(8);
    assert_eq!(
        a.check(addr.wrapping_add(1 << 40), tag),
        Err(FaultKind::OutOfBounds)
    );
}

#[test]
fn quarantine_holds_a_freed_granule_until_the_budget_forces_reuse() {
    // A tiny budget: the first free fits, the second evicts it.
    let mut a = QuarantineAllocator::new(QuarantineBudget { bytes: 96 });
    let (first, ftag) = a.alloc(64);
    a.free(first);
    assert_eq!(
        a.check(first, ftag),
        Err(FaultKind::UseAfterFree),
        "inside the budget the granule stays poisoned"
    );
    let (second, _) = a.alloc(64);
    a.free(second);
    // 128 > 96: the oldest is released back, and its span is no
    // longer a granule this allocator knows.
    assert_eq!(a.quarantined_bytes(), 64, "only the newest is still held");
    assert_eq!(a.check(first, ftag), Err(FaultKind::OutOfBounds));
}

#[test]
fn the_fault_identity_is_stable_across_runs() {
    // D21's contract: the planted-defect suite asserts the SAME fault
    // for the same program, run after run. Two identical allocator
    // lifetimes must agree on tag and verdict.
    let run = || {
        let mut a = alloc();
        let (addr, tag) = a.alloc(24);
        let before = a.check(addr, tag);
        a.free(addr);
        (tag, before, a.check(addr, tag))
    };
    assert_eq!(run(), run());
}

#[test]
fn a_zero_sized_request_still_yields_a_distinct_live_address() {
    let mut a = alloc();
    let (p, ptag) = a.alloc(0);
    let (q, qtag) = a.alloc(0);
    assert_ne!(p, 0);
    assert_ne!(p, q);
    assert_eq!(a.check(p, ptag), Ok(()));
    assert_eq!(a.check(q, qtag), Ok(()));
}

#[test]
fn the_budget_is_the_profile_tunable_it_claims_to_be() {
    let a = QuarantineAllocator::new(QuarantineBudget::default());
    assert_eq!(a.budget_bytes, 64 << 20);
    assert_eq!(a.quarantined_bytes(), 0);
}
