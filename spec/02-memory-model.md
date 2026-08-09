# Wolf Language Specification — 02: Memory Model

Status: normative, v0 (sprint s04). Clause anchors `[mem.*]` are stable and
cited by conformance tags, diagnostics, and the checkers (s18–s23). The
reference interpreter (wolf-interp, is02–is04) implements this document as a
dynamic machine; the compiler implements a static approximation. Divergence
between them is, by definition, a bug in one of the two — the differential
protocol (spec 06) decides which.

Companion grammar: spec/01-grammar.md. Concurrency ordering: spec 03 (this
document stops at region transfer). Evidence: `.docs/refs/reports/01`,
papers cited per section.

Diagnostic codes: memory-tier static errors use the **E1xxx** family
(post-parse; corpus files carrying them must still parse — see the code
discipline in `[mem.codes]`). Trap kinds are drawn from the closed s06
vocabulary.

---

## §1 Model vocabulary & the abstract machine `[mem.model]`

- `[mem.model.value]` A **value** is data with MVS semantics: assignment
  and argument passing transfer or copy the *whole* value; values have no
  identity beyond their current place.
- `[mem.model.place]` A **place** is a storage location denoted by a
  **path**: a base binding followed by field/index projections (`a.x.y`,
  `xs[i]`). Paths are field-granular: `a.x` and `a.y` are disjoint places.
  `[mem.model.path.disjoint]` Two paths conflict iff one is a prefix of
  the other (after identical projections); otherwise they are disjoint.
- `[mem.model.granule]` A **granule** is the unit of ownership reasoning:
  a value (Tier 0), a region (Tier 1), or a shared/handle cell (Tier 2).
- `[mem.model.machine]` The abstract machine state comprises:
  1. **Memory**: a set of allocations, each with base address, size,
     liveness bit, and owning region (or *stack* for locals).
  2. **Provenance forest**: per allocation, a tree of tags (§6).
  3. **Region forest**: regions with parent edges (≤1 each), state
     (Open/Suspended/Frozen/Freed), and strategy (arena/rc/pool).
  4. **Scope stack**: the currently-open regions and active borrows.
  Programs whose executions the machine defines are exactly the defined
  programs; §7 enumerates the machine's stuck states (UB) — reachable only
  through Tier 3.
- `[mem.model.alloc]` There is no `new` keyword. Allocation sites are
  struct literals, collection constructors, and closures whose values the
  implementation places in memory; whether a given value lives in a
  register, on the stack, or in its region is unobservable except as §3
  and §7 permit.
- `[mem.codes]` Diagnostic-code families by tier: `E000x` (spec-01 §9)
  + `E01xx` (lexer) + `E02xx` (parser) are syntax-tier — the file fails
  to lex or parse. `E03xx` (resolution), `E04xx` (types), `E1xxx`
  (memory), `E11xx` (concurrency), and `E12xx` (ABI) are post-parse
  static rejections. Traps are runtime outcomes of *defined* behavior.
  Nothing in this document is both an error and a trap.

## §2 Tier 0 — values `[mem.tier0]`

MVS lineage: `.docs/refs/papers/mutable-value-semantics.pdf`; exclusivity
law: `.docs/refs/papers/swift-ownership-manifesto.md`.

### Moves `[mem.tier0.move]`

- `[mem.tier0.move.1]` Assignment, initialization, `take` arguments, and
  `return` **move** non-`Copy` values. The source place becomes
  *uninitialized*.
- `[mem.tier0.move.2]` Use of an uninitialized or moved-from place is a
  **compile error** (E1001) in the safe tiers. Dynamic meaning (for the
  interpreter, which checks at runtime what the compiler proves statically):
  the read traps with kind `use-after-move`.
- `[mem.tier0.move.3]` `copy x` produces an independent value from any
  type; types opting into `Copy` copy implicitly on what would otherwise
  move. POD-shaped types only (no destructor, no Tier-2 members).
- `[mem.tier0.move.4]` A moved-from place may be re-initialized by
  assignment; it is then live again.

### Parameter modes `[mem.tier0.mode]`

- `[mem.tier0.mode.read]` Default (unwritten) mode: the callee reads a
  value that is **immutable for the whole call**; the caller retains it.
  No syntax exists to name this mode — absence is the syntax.
- `[mem.tier0.mode.mut]` `mut` parameters are **exclusive inout**: for the
  duration of the call no other access (read or write) to the argument
  place or any conflicting path may occur. Call sites must write `mut`
  (X1, grammar `[gram.item.fn]`).
- `[mem.tier0.mode.take]` `take` consumes: the argument moves into the
  callee (`[mem.tier0.move.1]` applies at the call site).

### Exclusivity `[mem.tier0.excl]`

- `[mem.tier0.excl.1]` At every program point, a place accessed through a
  `mut` binding/parameter/borrow has **no other live access path**.
  Violations are compile errors (E1002); the dynamic machine traps with
  kind `exclusivity`.
- `[mem.tier0.excl.2]` Disjoint paths may be `mut` simultaneously:
  `f(mut a.x, mut a.y)` is legal by `[mem.model.path.disjoint]`;
  `f(mut a, mut a.x)` is E1002.
- `[mem.tier0.excl.3]` View sets declare a callee's path footprint:
  `fn norm(mut self.{x, y})` may touch only `self.x`/`self.y`; the caller
  may concurrently use `self.z`. The footprint is part of the signature.

### Local borrows `[mem.tier0.borrow]`

- `[mem.tier0.borrow.1]` `&path` / `&mut path` create **local borrows**,
  legal only within a function body (01 Q4). The observable rule: **no
  borrow outlives its function activation** — returning, storing (in any
  heap tier), or capturing a borrow past the activation is a compile error
  (E1003). There is no reference type in signatures; parameter modes are
  the cross-function story.
- `[mem.tier0.borrow.2]` While an `&mut` borrow of a path is live, that
  path is exclusively held (`[mem.tier0.excl.1]`); while `&` borrows are
  live, the path is read-frozen. Liveness is last-use (NLL-grade), not
  scope-exit; the spec constrains observations, not the inference
  algorithm.

## §3 Tier 1 — regions `[mem.region]`

Lineage: `.docs/refs/papers/cyclone-regions.pdf` (identity as a static
fact, polymorphism defaults), `.docs/refs/papers/verona-refcaps.pdf`
(granule ownership, imm).

### Creation & allocation `[mem.region.create]`

- `[mem.region.create.1]` `region name { … }` (sugar) and `region(…)`
  (first-class value) create a region. Strategy: arena (default), `rc`,
  or `pool(T)` — strategy changes cost and §4 availability, never safety.
- `[mem.region.create.2]` Region values are **affine**: they move, are
  never copied; a region variable denotes a distinct region from every
  other live region variable. `[mem.region.multiopen]` builds on this.
- `[mem.region.create.3]` **Ambient allocation**: every function executes
  with a *current region*; heap allocations land there. The default
  current region of a function body is its **caller's** current region
  (D12 — "caller decides where memory goes" is the language default).
  `in r { … }` sets the current region to `r` for the block.
- `[mem.region.create.4]` Region identity is a static type fact with
  **zero runtime representation**; the dynamic machine tracks it, compiled
  code need not.

### Intra-region freedom `[mem.region.intra]`

- `[mem.region.intra.1]` Within one region, references are unrestricted:
  cycles, back-edges, intrusive structures are safe. Nothing dangles while
  the region lives.
- `[mem.region.intra.2]` A region dies as a unit: when a region is freed
  (sugar-block exit, or last use of the region value), every allocation in
  it is freed **wholesale**. Per-allocation frees do not exist in safe
  code.

### Cross-region edges `[mem.region.edge]`

Edge legality (source stores a reference to target):

| from \ to | same region | child region (iso) | other region | `imm` | Tier-2 cell |
|-----------|-------------|--------------------|--------------|-------|-------------|
| region data | ✅ `[mem.region.intra.1]` | ✅ via owning handle `[mem.region.edge.iso]` | ❌ E1004 | ✅ `[mem.region.edge.imm]` | ✅ (§4 rules) |

- `[mem.region.edge.iso]` A region has **at most one owning edge**
  (parent): the region forest invariant. The owning handle (the region
  value or the iso field holding it) is affine like the region itself.
- `[mem.region.edge.imm]` Frozen (`imm`) data may be referenced from
  anywhere, forever.
- `[mem.region.edge.raw]` Cross-region raw edges exist only in Tier 3 and
  carry §7 obligations.

### Open/close discipline `[mem.region.open]`

- `[mem.region.open.1]` A region is **Open** (mutable) in at most one
  scope at a time; entering `in r { … }` or the sugar block opens it;
  exit closes it (state Suspended).
- `[mem.region.open.2]` `[mem.region.multiopen]` **Multiple disjoint
  regions may be open simultaneously.** The disjointness obligation:
  the open set is a set of *distinct region values*; since region values
  are affine (`[mem.region.create.2]`), two open handles are two regions
  by construction. Opening a region through anything other than its value
  (e.g., a Tier-3 raw path) is outside this clause and lands in §7.
  **Model-checking priority**: this clause is the unproven extension past
  Verona's single-window rule; is07 explores it exhaustively; the s20
  fallback is single-open-window.
- `[mem.region.open.3]` A Suspended region's contents are unreachable for
  writing (no live path can write into it — its handle is busy being
  somewhere); the optimizer entitlement this creates is §7/O4.

### Freeze & transfer `[mem.region.freeze]`

- `[mem.region.freeze.1]` `freeze r` consumes the region value and
  promotes the entire graph to `imm` — deep, in place, no copy. Frozen
  data is immutable **forever** and shareable across threads (s05).
- `[mem.region.freeze.2]` `move r` transfers the region value (affine
  move); cross-thread semantics in spec 03. After transfer, any use of
  the old binding is `[mem.tier0.move.2]`.
- `[mem.region.freeze.3]` Freezing or transferring a region containing an
  open child region is a compile error (E1005): the forest transfers as
  closed subtrees only.

### Unobservable placement `[mem.region.promote]`

- `[mem.region.promote.1]` Stack promotion / escape analysis must not
  change any program observation: allocation placement is unobservable
  except through §7-licensed reasoning and address inspection in Tier 3
  (which carries no placement guarantees — `[mem.ub]` row T2 layout freedom).

## §4 Tier 2 — `shared` and `handle` `[mem.shared]`

Lineage: `.docs/refs/papers/perceus.pdf` (RC insertion, drop timing).
Two escape types, two failure contracts (X5): `shared` can never dangle;
`handle` faults deterministically.

### `shared T` `[mem.shared.rc]`

- `[mem.shared.rc.1]` `shared T` is a reference-counted cell; clones share
  ownership; the value drops when the last strong reference drops.
- `[mem.shared.rc.2]` **Strong edges are acyclic at the type level**: a
  type participating in a `shared` graph may not contain a strong path
  back to itself; back-edges must be `weak` or `handle`. A strong cycle is
  a compile error (E1006) — wolf has no cycle collector, and a leak is not
  the answer (01 Q3).
- `[mem.shared.rc.3]` `weak T` does not keep the value alive; upgrading
  yields an option-shaped result the caller must handle.
- `[mem.shared.rc.4]` RC operations are unobservable (elision, fusing,
  non-atomic implementation when thread-exclusivity is statically known —
  s05) **except** through destructor timing, which is normative:
  `[mem.shared.drop]`.

### Destructor timing `[mem.shared.drop]`

- `[mem.shared.drop.1]` Types with a user-defined destructor have an
  implicit *use* at scope exit: their drop runs at scope exit, LIFO with
  `defer`/`errdefer`, exactly once.
- `[mem.shared.drop.2]` Destructor-free values may be reclaimed any time
  after their last use (drop-at-last-use; Perceus): reclamation of plain
  data is unobservable.
- `[mem.shared.drop.3]` A `shared` cell whose payload has a destructor
  runs it when the last strong count drops, at that release point.

### `handle T` `[mem.shared.handle]`

- `[mem.shared.handle.1]` `handle T` is a generational index into a
  `pool(T)` region. Pools are two-phase: `reserve()` yields a handle;
  `init(h, v)` fills it (grammar/corpus lock — no null handles exist).
- `[mem.shared.handle.2]` Accessing a handle whose slot was freed or
  re-generationed is a **deterministic fault**: trap kind `stale-handle`,
  in every profile. This is defined behavior, not UB.
- `[mem.shared.handle.3]` `pool[h]` / `pool[mut h]` access the slot under
  Tier-0 exclusivity rules; the pool is the place base.

## §5 Tier 3 — unsafe `[mem.unsafe]`

Simpler than the safe tier, not stricter (anti-Stacked-Borrows lesson).

- `[mem.unsafe.raw.1]` Raw pointers `*T` carry **no aliasing assumptions**:
  the implementation treats them like C `char*` unless an `assume noalias`
  assertion says otherwise. Arithmetic, casts, and copies of raw pointers
  are unrestricted.
- `[mem.unsafe.raw.2]` `assume noalias p, q` asserts the pointed-to
  ranges do not overlap for the assertion's scope. A false assertion is
  UB (§7/P5) — the only *assertion-created* UB in the language.
- `[mem.unsafe.raw.3]` Volatile reads/writes (std intrinsics) are
  side-effecting and never elided/reordered against each other.
- `[mem.unsafe.door]` Exactly two doors re-enter the safe world:
  1. `borrow r from ptr` — produces a region-scoped reference from a raw
     pointer. Obligation: `ptr` addresses a live allocation wholly inside
     region `r`'s footprint, correctly typed, for the borrow's extent.
  2. Checked `handle` — a raw index laundered through a pool lookup,
     which re-validates generation (`[mem.shared.handle.2]`).
  Discharging a door's obligation falsely is UB at the *door* (§7/P6),
  not later — the safe tier stays safe by construction.
- `[mem.unsafe.scope]` `unsafe { }` blocks appear only inside functions
  whose signatures are fully safe; the enclosing **module** is the audit
  granule (§8).

## §6 Provenance `[mem.prov]`

Machine style: Stacked Borrows lineage
(`.docs/refs/papers/stacked-borrows.pdf`), tree-structured per Tree
Borrows (`.docs/refs/articles/tree-borrows-paper.txt`) — trees tolerate
the C-style pointer arithmetic and two-phase patterns SB rejects.

- `[mem.prov.tag]` Every pointer value carries a **tag**; every allocation
  carries a tree of tags. New tags are children of the tag they derive
  from. Retag points: borrow creation, `mut`/`read` parameter entry
  (parameter entry is protector-equivalent: the tag is protected for the
  whole call), Tier-2 cell access, and `borrow r from ptr`.
- `[mem.prov.state]` Each tag, per allocation range, is in one state:

  | state | meaning | child read | child write | foreign read | foreign write |
  |-------|---------|-----------|-------------|--------------|---------------|
  | Reserved | created, unused (two-phase window) | → Active | → Active | ok | → Disabled |
  | Active | live unique/mutable | ok | ok | → Frozen | → Disabled |
  | Frozen | shared/read-only | ok | **UB §7/P2** | ok | → Disabled |
  | Disabled | dead | **UB §7/P1** | **UB §7/P1** | ok | ok |

  ("child" = access through this tag or a descendant; "foreign" = through
  a non-descendant. Protected tags escalate the foreign-write transition
  to immediate UB for the protection's duration.)
- `[mem.prov.expose]` Int→ptr casts produce a pointer with **exposed**
  provenance resolved angelically among exposed tags (a defined execution
  is chosen if one exists); ptr→int casts expose the tag. Wildcard
  pointers from FFI behave as exposed.
- `[mem.prov.region]` Region composition: freeing a region **Disables
  every tag tree** of every allocation it owns; `freeze` transitions all
  its tags to Frozen. Region identity partitions provenance: tags rooted
  in different regions never alias (§7/O3's ground truth).

## §7 UB enumeration ⇄ licensed optimizations `[mem.ub]`

**The D2 contract.** This enumeration is **closed**: behavior not listed
here is defined (possibly as a trap) or implementation-specified and
documented. Adding UB requires a spec amendment naming its licensed
optimization. Safe-tier programs cannot reach any row — every row requires
Tier 3 (or an FFI boundary) in the execution.

Detection legend: **S** static checker (s18–s23) · **O** is04 oracle ·
**Q** debug quarantine allocator (D21).

| # | UB (unsafe-tier reachable only) | Licensed optimization | Detected |
|---|--------------------------------|----------------------|----------|
| P1 | Access through a Disabled tag (use-after-free, use of invalidated borrow) | O1: `mut` params lower to `noalias` + `dereferenceable`; unique-tag stores forward without memory checks | O, Q |
| P2 | Write through a Frozen tag | O2: `read` params are immutable-for-the-call — loads hoist/CSE across opaque calls (the SB "holy grail") ; `imm` data const-propagates and needs no sync | O |
| P3 | Access outside an allocation's bounds | O3a: `dereferenceable(n)` on known-size accesses; bounds-based alias disproof between distinct allocations | O, Q |
| P4 | Access to an allocation whose region was freed | O3b: one alias-scope domain **per region** — pointers into distinct regions never alias (inter-procedural strength, unavailable to C/Rust); O4: regions not open in the current scope yield `invariant.load` | O, Q |
| P5 | Violated `assume noalias p, q` | O5: the asserted ranges get `noalias` treatment in Tier-3 code — vectorization/reordering as if proven | O (checks the assertion dynamically) |
| P6 | False discharge of a re-entry door (`borrow r from ptr` obligations, forged handle index laundering) | O6: safe-tier code after the door keeps **all** safe-tier entitlements (O1–O4) — the door is where trust concentrates, so safe code never re-checks | O, Q |
| L1 | Read of uninitialized or moved-from memory via raw pointers | O7: moves lower to memcpy-and-forget; dead-store elimination on moved-from places; no zero-init of locals | O, Q |
| L2 | Deref of a dangling raw pointer (freed C allocation, escaped stack address) | O8: escape analysis / stack promotion (`[mem.region.promote.1]`) without conservatively pinning addresses | O, Q |
| T1 | Producing an invalid value of a restricted type in unsafe code (bool ∉ {0,1}, out-of-range enum discriminant, non-UTF-8 `str` bytes) | O9: niche packing (`Option[handle T]` is one word); match jump tables without default arms; UTF-8 fast paths without re-validation | O |
| T2 | Torn write producing a partially-updated wide value observed through another tag | O10: layout freedom — field reorder, no address identity for value fields outside `#[repr(c)]` (I9); wide stores split freely | O |
| C1 | Data race on non-atomic memory reachable only from unsafe/FFI code (safe code cannot race — spec 03) | Licensing pairing lives in spec 03 §(DRF-SC): sync-free stretches permit store motion/combining | O (schedule-bounded), race detector |

**Deliberately defined (not UB), with their licensed recovery**
`[mem.ub.defined]`:

| behavior | outcome | licensed recovery |
|----------|---------|-------------------|
| Integer overflow (X3) | trap `overflow`, every profile | checked ops feed **value-range facts** the optimizer exploits (range-based CSE, bound-check elision) |
| Division by zero | trap `div-zero` | non-zero ranges elide the check |
| OOB index / split-code-point slice (D25) | trap `bounds` | range facts elide checks; slices carry `dereferenceable` |
| Stale handle | trap `stale-handle` | generation checks batch/hoist within provably-unfreed windows |
| Strong `shared` cycle | compile error E1006 | acyclicity ⇒ RC drops need no cycle detection ever |
| Memory leak (`shared` kept alive, region never freed) | defined, safe | — (leaks are safe; `wolf dbg` surfaces them) |

- `[mem.ub.closed]` Closing rule as above: **zero rows without a named
  optimization** is an invariant of this table; CI's spec review enforces
  it structurally (the D2 ratchet).

## §8 The boundary `[mem.boundary]`

- `[mem.boundary.module]` The **module** is the audit granule: an `unsafe`
  block's soundness argument may rely on invariants maintained by its
  module's private items, and nothing wider.
- `[mem.boundary.doc]` Every `unsafe` block carries a comment stating the
  safe-tier invariant it maintains; `wolf audit` (I13) lists modules with
  unsafe blocks, `#[trusted]` marks, and FFI imports as the three
  greppable rings (D11).
- `[mem.boundary.trusted]` `#[trusted]` modules appear in the package
  manifest; a dependency adding one is a `wolf audit` diff event.
- `[mem.boundary.ffi]` A C call executes against an implicit region
  borrowed for the call's extent. **C-held pointers**: any pointer a C
  callee may retain past the call must reference a `handle` (revalidated
  on wolf-side reuse) or an allocation in a pinned `#[trusted]` region
  (01 Q7); mechanism in c10. Everything else a C function may do with a
  wolf pointer ends at the call's return.

---

## Appendix A — `corpus/regions.lu`, clause by clause `[mem.appendix]`

```text
region r {                       -- [mem.region.create.1] sugar: create+open
    var pool = Pool[Node]()      -- [mem.region.create.3] allocates in r (ambient)
    var hs = List[handle Node]() -- [mem.shared.handle.1] handles; list in r
    for _ in 0..n { hs.push(pool.reserve()) }
                                 -- [mem.shared.handle.1] two-phase: reserve
    for i in 0..n {
        pool.init(hs[i], Node {  -- [mem.shared.handle.1] init fills the slot
            value: i,
            next: hs[(i + 1) % n],      -- [mem.region.intra.1] cycle: SAFE,
            prev: hs[(i + n - 1) % n],  --   handles within one pool region
        })
    }
    var sum = 0
    var cur = hs[0]              -- [mem.tier0.move.3] handle is Copy-shaped
    for _ in 0..n {
        sum += pool[cur].value   -- [mem.shared.handle.3] checked slot access
        cur = pool[cur].next     --   (generation valid: nothing freed)
    }
    sum                          -- [mem.tier0.move.1] value moves out of block
}                                -- [mem.region.intra.2] r freed WHOLESALE:
                                 --   pool, list, all nodes — one operation

let r2 = region(rc)              -- [mem.region.create.1] rc strategy
let config = in r2 { build_config() }
                                 -- [mem.region.create.3] `in` sets ambient
let frozen = freeze r2           -- [mem.region.freeze.1] deep imm, no copy;
                                 --   r2 consumed [mem.region.create.2]
```

Every allocation in the program traces to `[mem.region.create.3]`; every
free to `[mem.region.intra.2]` or `[mem.shared.drop]`. No lifetime is
written anywhere — that is the point.
