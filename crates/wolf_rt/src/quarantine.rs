//! The debug quarantine allocator — contract + hooks (s23 specs and
//! stubs; the real allocation path is s32, the `--checked` user
//! surface is s54).
//!
//! # What it is (D21)
//!
//! A debug/`--checked`-profile allocator for Tiers 1–3 that turns
//! latent use-after-free into a **deterministic fault**. It is the
//! runtime twin of s23's miri-lite: the checker treats freed-region
//! memory as poisoned and never reused (its shadow store keeps dead
//! allocations forever), and this allocator gives compiled `--checked`
//! builds the same guarantee against real memory.
//!
//! MTE-style **random generational tags** (Vale's "random generational
//! references" ≈ software MTE): every allocation granule carries a
//! tag; a free **retags** the granule and **quarantines** it (no reuse
//! until quarantine pressure forces it); a pointer deref in a checked
//! build validates the tag and **faults deterministically** — with
//! allocation and free backtraces — when it is stale. Release builds
//! pay nothing: they get the plain arena/pool paths, and the
//! quarantine allocator is a link-time profile choice.
//!
//! # Two layers that WILL confuse everyone, so: the distinction
//!
//! - The **pool's semantic generation** (`[mem.shared.handle.2]`, s21)
//!   is a *language-visible* value: `handle T` carries it, `pool[h]`
//!   re-validates it, and a stale handle is a **defined trap**
//!   (`trap(stale-handle)`) in *every* profile — it is part of the
//!   type's contract.
//! - The **debug tag** here is an *invisible* allocator-layer artifact
//!   present only in checked builds. It catches bugs the safe tier
//!   already forbids (raw-pointer UAF, region-free UAF) but that the
//!   unsafe tier permits (`[mem.unsafe.raw.1]`). A checked-build tag
//!   fault reports `ub` (the unsafe-tier UB became observable), never
//!   `stale-handle`.
//!
//! These are distinct layers by design. The semantic generation is
//! X5; the debug tag is D21.
//!
//! # Status this sprint
//!
//! This module is the **contract and the hook signatures**, stubbed.
//! `wolf_rt` stays dependency-thin (D15) and has no allocator today;
//! s32 implements the granule store, the tag PRNG, and the fault
//! path against a real backing allocator, and s31 wires the
//! `--checked` link-time profile that selects it. The signatures here
//! are the interface s32 fills and s23's fact/HIR docs reference — the
//! checker-side equivalent already runs in
//! [`crate::super`]`::ubcheck` (in `wolf_mem`), so the model is
//! validated before the runtime grows around it (01 Q6).

/// A software-MTE granule tag. `0` is the reserved "untagged" value;
/// live allocations carry a nonzero random tag, and a free rotates it
/// to a fresh nonzero value so the previous pointer's tag no longer
/// matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag(pub u8);

impl Tag {
    /// The reserved untagged value.
    pub const UNTAGGED: Tag = Tag(0);
}

/// The quarantine budget: freed granules stay poisoned and unreused
/// until total quarantined bytes exceed `bytes`, at which point the
/// oldest are released back for reuse (FIFO). A larger budget catches
/// longer-lived dangling pointers at higher memory cost — the
/// `--checked` profile's one tunable.
#[derive(Debug, Clone, Copy)]
pub struct QuarantineBudget {
    pub bytes: usize,
}

impl Default for QuarantineBudget {
    fn default() -> Self {
        // 64 MiB: enough to catch the planted-defect suite (UAF after
        // region free, pool-slot reuse, double free, OOB into a
        // quarantined span) with room, cheap enough for CI.
        QuarantineBudget { bytes: 64 << 20 }
    }
}

/// How a stale-tag fault was reached — the deterministic fault
/// identity the planted-defect suite asserts is identical across
/// repeated runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultKind {
    /// Deref of a granule whose region was wholesale-freed
    /// (`[mem.prov.region]`; the checker's P4).
    RegionFreed,
    /// Deref of a granule freed by `c.free`/pool remove and
    /// quarantined (the checker's P1/L2).
    UseAfterFree,
    /// A double free of a quarantined granule.
    DoubleFree,
    /// An access outside a live granule that lands in a quarantined
    /// neighbour (the checker's P3 into a poisoned span).
    OutOfBounds,
}

/// The runtime hooks a checked build calls. **All stubbed this
/// sprint** — s32 implements the bodies against a real backing
/// allocator; the signatures are the frozen interface s31/s54 wire and
/// s23's fact docs reference.
pub trait QuarantineHooks {
    /// Allocate `size` bytes, returning the base address and its fresh
    /// random tag. s32: draw a nonzero tag, stamp the granule shadow.
    fn alloc(&mut self, size: usize) -> (usize, Tag);

    /// Free `addr`: retag the granule, move it to quarantine (no
    /// reuse until [`QuarantineBudget`] pressure). s32: record the
    /// free backtrace for the fault report.
    fn free(&mut self, addr: usize);

    /// Retag every granule owned by `region` (cheap — one tag per
    /// region page run) and quarantine the whole span. The
    /// region-aware wholesale-free path (`[mem.region.intra.2]`).
    fn free_region(&mut self, region: RegionId);

    /// Validate `tag` against the granule at `addr`. `Ok(())` when
    /// live and matching; `Err(kind)` is the deterministic fault a
    /// checked build turns into `trap(ub)` with alloc/free backtraces.
    fn check(&self, addr: usize, tag: Tag) -> Result<(), FaultKind>;
}

/// A runtime region identity (the allocator layer's view; distinct
/// from the checker's region variables and the type-level region
/// values). s32 assigns these as regions are created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionId(pub u32);

/// The unimplemented stub every hook resolves to until s32. Present so
/// the `--checked` link path has a symbol to bind and so the contract
/// compiles and is documented from here on.
#[derive(Debug, Default)]
pub struct StubAllocator {
    pub budget_bytes: usize,
}

impl StubAllocator {
    pub fn new(budget: QuarantineBudget) -> Self {
        StubAllocator {
            budget_bytes: budget.bytes,
        }
    }
}

impl QuarantineHooks for StubAllocator {
    fn alloc(&mut self, _size: usize) -> (usize, Tag) {
        unimplemented!(
            "wolf_rt quarantine allocator is s32; the s23 checker-side twin is wolf_mem::ubcheck"
        )
    }
    fn free(&mut self, _addr: usize) {
        unimplemented!("wolf_rt quarantine allocator is s32")
    }
    fn free_region(&mut self, _region: RegionId) {
        unimplemented!("wolf_rt quarantine allocator is s32")
    }
    fn check(&self, _addr: usize, _tag: Tag) -> Result<(), FaultKind> {
        unimplemented!("wolf_rt quarantine allocator is s32")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_shapes_exist() {
        // The interface is a compile-time contract this sprint; the
        // bodies are s32. This test pins that the shapes are the ones
        // the checker-side twin (wolf_mem::ubcheck) mirrors.
        let a = StubAllocator::new(QuarantineBudget::default());
        assert_eq!(a.budget_bytes, 64 << 20);
        assert_eq!(Tag::UNTAGGED, Tag(0));
        // The four planted-defect fault identities the D21 acceptance
        // suite asserts are stable across runs.
        let kinds = [
            FaultKind::RegionFreed,
            FaultKind::UseAfterFree,
            FaultKind::DoubleFree,
            FaultKind::OutOfBounds,
        ];
        assert_eq!(kinds.len(), 4);
    }

    #[test]
    #[should_panic(expected = "s32")]
    fn hooks_are_stubbed_until_s32() {
        let mut a = StubAllocator::new(QuarantineBudget::default());
        let _ = a.alloc(8);
    }
}
