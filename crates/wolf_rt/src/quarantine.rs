//! The debug quarantine allocator (s23's specs; the s23-era plan said
//! "s32" for the real allocation path, but s32's contract is the task
//! scheduler — the c06 closeout routed the `--checked` runtime hooks,
//! and with them this allocator's bodies, to s54, which is here).
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
//! # Status
//!
//! The hook signatures are the frozen interface and the bodies are
//! now real: a granule store over `std::alloc`, a deterministic tag
//! PRNG, and the FIFO quarantine the budget bounds. `wolf_rt` stays
//! dependency-thin (D15) — the backing memory is `std::alloc`, there
//! is no new crate. The checker-side equivalent runs in
//! [`crate::super`]`::ubcheck` (in `wolf_mem`), so the model was
//! validated before the runtime grew around it (01 Q6).
//!
//! ## Seen red before it was trusted
//!
//! `crates/wolf_rt/tests/quarantine_alloc.rs` landed one commit
//! earlier, against a rename and nothing else. Run on kasumi
//! (linux x86-64) at that commit, with the bodies still
//! `unimplemented!`:
//!
//! ```text
//! $ cargo test -p wolf_rt --test quarantine_alloc --no-fail-fast
//! thread 'an_allocation_is_live_addressable_and_tagged' panicked at
//!   crates/wolf_rt/src/quarantine.rs:172:9:
//! not implemented: wolf_rt quarantine allocator is s54; the s23
//!   checker-side twin is wolf_mem::ubcheck
//! test result: FAILED. 0 passed; 14 failed; 0 ignored
//! EXIT=101
//! ```
//!
//! Fourteen of fourteen, at the stub's own line. The tests drive this
//! type through [`QuarantineHooks`]; none of them reimplements it.
//!
//! ## What is deliberately NOT here
//!
//! Nothing calls this allocator yet. It is a library with a proven
//! contract, not a wired `--checked` profile: the link-time profile
//! choice, the alloc/free backtraces the fault report promises, and
//! the region machinery that would drive [`QuarantineAllocator::set_region`]
//! are each their own work. Saying so is the point — a module that
//! passes its own tests and is reached by no program is a component,
//! and calling it a shipped feature would be the claim without the
//! artifact.

use std::alloc::{Layout, alloc as raw_alloc, dealloc};
use std::collections::VecDeque;

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

/// The runtime hooks a checked build calls. The signatures are the
/// frozen interface the `--checked` link path binds and s23's fact
/// docs reference; [`QuarantineAllocator`] implements them.
pub trait QuarantineHooks {
    /// Allocate `size` bytes, returning the base address and its fresh
    /// random tag: a nonzero tag drawn from the allocator's own
    /// stream and stamped on the granule's shadow.
    fn alloc(&mut self, size: usize) -> (usize, Tag);

    /// Free `addr`: retag the granule and move it to quarantine (no
    /// reuse until [`QuarantineBudget`] pressure). The alloc/free
    /// backtraces the fault report promises are not captured yet.
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
/// values). s54 assigns these as regions are created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionId(pub u32);

/// The granule state machine. A granule is LIVE from `alloc` until
/// `free`, QUARANTINED from `free` until the budget forces it out,
/// and then RELEASED — its backing returned to `std::alloc` and its
/// span no longer a span this allocator knows anything about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GranuleState {
    Live,
    Quarantined,
}

/// Why a quarantined granule is quarantined — the fault a later
/// access through a stale pointer reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Poison {
    Freed,
    RegionFreed,
    DoubleFreed,
}

#[derive(Debug)]
struct Granule {
    base: usize,
    /// The REQUESTED size. Bounds are judged against this, not
    /// against the rounded-up allocation, so an access one byte past
    /// a 3-byte request is out of bounds even though the granule's
    /// backing is 16 bytes wide.
    size: usize,
    /// The backing allocation's layout, kept for the `dealloc` that
    /// eventually releases it.
    layout: Layout,
    tag: Tag,
    state: GranuleState,
    poison: Option<Poison>,
    region: RegionId,
}

impl Granule {
    fn contains(&self, addr: usize) -> bool {
        // A zero-sized request still owns its base: one address, so a
        // pointer to it is live and a pointer past it is not.
        addr >= self.base && addr < self.base + self.size.max(1)
    }
}

/// The granule alignment. Every allocation is rounded up to this, so
/// two granules never share a machine word and a tag shadow is never
/// ambiguous about which object an address belongs to.
const GRANULE: usize = 16;

/// The quarantine allocator: MTE-style generational tags over a
/// granule store, with freed granules poisoned and unreused until the
/// budget forces the oldest out (FIFO).
///
/// # Determinism
///
/// The tag stream is SplitMix64 from a fixed seed, so the same
/// sequence of calls draws the same tags and reports the same fault
/// identities run after run — which is what D21's planted-defect
/// suite asserts. "Random" here means "unrelated to the address", not
/// "unpredictable": an attacker is not the threat model, a dangling
/// pointer is.
///
/// A tag is 8 bits, so two live granules can carry the same tag; that
/// is MTE's own bargain and it costs recall, never soundness. A stale
/// pointer whose tag collides with the granule's fresh one reads as
/// live — 1 chance in 255, and deterministic, because a fault that
/// appears only sometimes is worse than one that appears rarely.
#[derive(Debug)]
pub struct QuarantineAllocator {
    pub budget_bytes: usize,
    granules: Vec<Granule>,
    /// Indices into `granules`, oldest first: the FIFO the budget
    /// drains.
    quarantine: VecDeque<usize>,
    quarantined_bytes: usize,
    /// Indices freed back to the store once a granule is released —
    /// so a long-running program does not grow `granules` forever.
    vacant: Vec<usize>,
    region: RegionId,
    rng: u64,
}

impl Default for QuarantineAllocator {
    fn default() -> Self {
        QuarantineAllocator::new(QuarantineBudget::default())
    }
}

impl QuarantineAllocator {
    pub fn new(budget: QuarantineBudget) -> Self {
        QuarantineAllocator {
            budget_bytes: budget.bytes,
            granules: Vec::new(),
            quarantine: VecDeque::new(),
            quarantined_bytes: 0,
            vacant: Vec::new(),
            // The seed is fixed and stated, not hidden: see the
            // determinism note above.
            region: RegionId(0),
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// The shadow tag currently stamped on the granule containing
    /// `addr`, live or quarantined; `None` once the span is released.
    pub fn tag_at(&self, addr: usize) -> Option<Tag> {
        self.find(addr).map(|i| self.granules[i].tag)
    }

    /// Bytes held in quarantine — freed, retagged and not yet reused.
    pub fn quarantined_bytes(&self) -> usize {
        self.quarantined_bytes
    }

    /// Which region subsequent allocations belong to. The trait's
    /// `alloc` is the frozen interface and takes no region, so the
    /// owner is ambient state the region machinery sets as it opens
    /// and closes scopes.
    pub fn set_region(&mut self, region: RegionId) {
        self.region = region;
    }

    /// The region allocations are currently charged to.
    pub fn region(&self) -> RegionId {
        self.region
    }

    /// How many granules the store is tracking, live and quarantined.
    pub fn tracked(&self) -> usize {
        self.granules.len() - self.vacant.len()
    }

    /// The next tag: SplitMix64, forced nonzero so it can never
    /// collide with [`Tag::UNTAGGED`].
    fn next_tag(&mut self) -> Tag {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // 1..=255: the reserved value is never drawn.
        Tag((z % 255) as u8 + 1)
    }

    /// The granule whose span contains `addr`, if the store still
    /// knows one. Released spans are gone by construction — their
    /// entry is vacant.
    fn find(&self, addr: usize) -> Option<usize> {
        self.granules
            .iter()
            .position(|g| g.size != usize::MAX && g.contains(addr))
    }

    /// Does any LIVE granule carry this tag? The discriminator that
    /// separates "ran off a live object" from "used a dead one".
    fn live_with_tag(&self, tag: Tag) -> bool {
        self.granules
            .iter()
            .any(|g| g.size != usize::MAX && g.state == GranuleState::Live && g.tag == tag)
    }

    /// Put a granule in quarantine: retag it so every extant pointer
    /// goes stale, record why, and drain the FIFO if the budget is
    /// exceeded.
    fn quarantine_granule(&mut self, i: usize, why: Poison) {
        let fresh = self.next_tag();
        let g = &mut self.granules[i];
        g.state = GranuleState::Quarantined;
        g.poison = Some(why);
        g.tag = fresh;
        let held = g.size.max(1);
        self.quarantined_bytes += held;
        self.quarantine.push_back(i);
        self.drain_to_budget();
    }

    /// Release the oldest quarantined granules until the held bytes
    /// fit the budget. A released granule's backing goes back to the
    /// system and its span stops being a span this allocator knows —
    /// which is why a pointer into it then reads
    /// [`FaultKind::OutOfBounds`] rather than a use-after-free: the
    /// allocator has forgotten, and saying "use after free" would be
    /// a claim it can no longer support.
    fn drain_to_budget(&mut self) {
        while self.quarantined_bytes > self.budget_bytes {
            let Some(i) = self.quarantine.pop_front() else {
                break;
            };
            let (base, layout, held) = {
                let g = &self.granules[i];
                (g.base, g.layout, g.size.max(1))
            };
            self.quarantined_bytes -= held;
            // SAFETY: `base` came from `raw_alloc` with exactly this
            // layout, it is still owned by this granule, and the
            // entry is marked vacant immediately below so nothing
            // reads it again.
            unsafe { dealloc(base as *mut u8, layout) };
            let g = &mut self.granules[i];
            g.base = 0;
            g.size = usize::MAX;
            g.poison = None;
            self.vacant.push(i);
        }
    }
}

impl Drop for QuarantineAllocator {
    fn drop(&mut self) {
        for g in &mut self.granules {
            if g.size == usize::MAX || g.base == 0 {
                continue;
            }
            // SAFETY: as in `drain_to_budget` — every live entry's
            // base came from `raw_alloc` with this layout and is
            // owned here.
            unsafe { dealloc(g.base as *mut u8, g.layout) };
            g.base = 0;
            g.size = usize::MAX;
        }
    }
}

impl QuarantineHooks for QuarantineAllocator {
    fn alloc(&mut self, size: usize) -> (usize, Tag) {
        let rounded = size.max(1).div_ceil(GRANULE) * GRANULE;
        let layout = Layout::from_size_align(rounded, GRANULE).expect("granule layout");
        // SAFETY: `rounded` is nonzero and `GRANULE` is a power of
        // two, so the layout is valid for `alloc`.
        let p = unsafe { raw_alloc(layout) };
        assert!(!p.is_null(), "quarantine allocator: out of memory");
        let tag = self.next_tag();
        let g = Granule {
            base: p as usize,
            size,
            layout,
            tag,
            state: GranuleState::Live,
            poison: None,
            region: self.region,
        };
        match self.vacant.pop() {
            Some(i) => self.granules[i] = g,
            None => self.granules.push(g),
        }
        (p as usize, tag)
    }

    fn free(&mut self, addr: usize) {
        let Some(i) = self.find(addr) else { return };
        // A free of an INTERIOR pointer is not a free of the object.
        // The allocator does not guess; it declines, and the granule
        // stays live so the leak is visible rather than the wrong
        // object being poisoned.
        if self.granules[i].base != addr {
            return;
        }
        match self.granules[i].state {
            GranuleState::Live => self.quarantine_granule(i, Poison::Freed),
            // A second free of a quarantined granule is the D21 double
            // free. It does not fault here — the trait hands back
            // nothing — it upgrades the poison, so the next `check`
            // through any pointer into the span reports it.
            GranuleState::Quarantined => {
                self.granules[i].poison = Some(Poison::DoubleFreed);
            }
        }
    }

    fn free_region(&mut self, region: RegionId) {
        let doomed: Vec<usize> = self
            .granules
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                g.size != usize::MAX && g.state == GranuleState::Live && g.region == region
            })
            .map(|(i, _)| i)
            .collect();
        for i in doomed {
            self.quarantine_granule(i, Poison::RegionFreed);
        }
    }

    fn check(&self, addr: usize, tag: Tag) -> Result<(), FaultKind> {
        let Some(i) = self.find(addr) else {
            // No granule owns this address: off the end of everything
            // the allocator knows.
            return Err(FaultKind::OutOfBounds);
        };
        let g = &self.granules[i];
        match g.state {
            GranuleState::Live if g.tag == tag => Ok(()),
            // The address is inside a live granule, but the pointer
            // carries some other object's tag: it walked in from
            // outside.
            GranuleState::Live => Err(FaultKind::OutOfBounds),
            GranuleState::Quarantined => {
                // The pointer's tag still names a LIVE granule, so the
                // pointer itself is valid and the ACCESS ran off its
                // object into a poisoned neighbour — the checker's P3.
                if self.live_with_tag(tag) {
                    return Err(FaultKind::OutOfBounds);
                }
                Err(match g.poison {
                    Some(Poison::DoubleFreed) => FaultKind::DoubleFree,
                    Some(Poison::RegionFreed) => FaultKind::RegionFreed,
                    _ => FaultKind::UseAfterFree,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_shapes_exist() {
        // The shapes the checker-side twin (wolf_mem::ubcheck)
        // mirrors. Behaviour lives in tests/quarantine_alloc.rs,
        // which drives the type through its trait.
        let a = QuarantineAllocator::new(QuarantineBudget::default());
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
    fn no_hook_is_a_stub_any_more() {
        // The inverse of the test this replaces, and the reason it is
        // written as an assertion rather than deleted: the old
        // `#[should_panic(expected = "s54")]` would go green again the
        // moment anyone put the `unimplemented!` back.
        let mut a = QuarantineAllocator::new(QuarantineBudget::default());
        let (addr, tag) = a.alloc(8);
        assert_eq!(a.check(addr, tag), Ok(()));
        a.free(addr);
        assert_eq!(a.check(addr, tag), Err(FaultKind::UseAfterFree));
        a.free_region(RegionId(0));
        assert_eq!(a.quarantined_bytes(), 8);
        assert_eq!(a.tracked(), 1);
        assert_eq!(a.region(), RegionId(0));
    }

    #[test]
    fn the_tag_stream_never_draws_the_reserved_value() {
        let mut a = QuarantineAllocator::new(QuarantineBudget::default());
        for _ in 0..4096 {
            assert_ne!(a.next_tag(), Tag::UNTAGGED);
        }
    }

    #[test]
    fn a_released_granule_leaves_no_backing_behind() {
        // Every path that forgets a granule must have deallocated it
        // first; the leak is invisible to assertions, so the shape is
        // pinned instead: the store shrinks and the FIFO empties.
        let mut a = QuarantineAllocator::new(QuarantineBudget { bytes: 0 });
        let (p, _) = a.alloc(32);
        a.free(p);
        assert_eq!(a.quarantined_bytes(), 0, "a zero budget holds nothing");
        assert_eq!(a.tracked(), 0, "and the granule is forgotten");
    }
}
