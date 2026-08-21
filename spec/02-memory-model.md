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
- `[mem.model.order]` Evaluation is strict and **left-to-right**
  everywhere: operands before operators, arguments left-to-right before
  the call, receivers before arguments, struct-literal fields in written
  order. Nothing is unsequenced. `?` and error returns are ordinary
  control flow: `defer`/`errdefer` run as the frames return, LIFO,
  after the error value is formed.
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

### Iteration exclusivity `[mem.iter.excl]`

(Added 2026-08-12 by ruling D40, resolving the S-11 `for`-operand
question — wolf-lang#15 / wolf-interp#9.)

- `[mem.iter.excl.1]` `for x in xs` over a place: the loop holds a
  **read claim** on that place for the loop's whole extent. The claim
  is a read, not a move — the place stays live behind the walk and
  after the loop. A `Copy` iterable is copied at loop entry and
  carries no claim (the same instant-read model as `Copy` call
  arguments, `[mem.tier0.mode.mut]`'s leniency).
- `[mem.iter.excl.2]` While the claim is live, a mutating use of the
  claimed place or any conflicting path (`[mem.model.path.disjoint]`)
  — a write or element write, a `mut` lend, a move — is a **compile
  error** (E1013) in the safe tiers. Dynamic meaning (for the machine
  that checks at runtime what the compiler proves statically): the
  mutating operation traps with kind `exclusivity`.

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
  exit closes it (state Suspended). Re-entering a region that is already
  open in the current scope chain (`in a { … }` inside `region a { … }`)
  is **idempotent** — openness is depth-counted, not a violation
  (is03 model-checking repair, 2026-08-09).
- `[mem.region.open.2]` `[mem.region.multiopen]` **Multiple disjoint
  regions may be open simultaneously.** The disjointness obligation
  (repaired 2026-08-09 after is03's dynamic machine found the original
  wording incoherent): the open set must be an **antichain in the region
  forest** — no open region may be an ancestor (owner, transitively via
  iso edges) of another open region. Distinctness of affine region
  values is *not* sufficient: `[mem.region.edge.iso]` lets one region
  own another, and an owner's open window reaches its child's data.
  Distinct *sibling* subtrees are always safely co-openable, which is
  the pattern every corpus litmus exercises. Opening a region through
  anything other than its value (e.g., a Tier-3 raw path) is outside
  this clause and lands in §7. **Model-checking priority**: is03's
  executable machine enforces the antichain; is07 explores schedules
  over it exhaustively; the s20 fallback is single-open-window.
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
- `[mem.region.freeze.4]` **Reads are never the fault.** Every read
  through frozen data — field and index projections to any depth, and
  a method call that only reads its receiver — is an ordinary read;
  `[mem.region.edge.imm]` is the affirmative permission, forever and
  from anywhere. Only a *write* into frozen data is a rejection
  (E1012; dynamically `region-fault`, `[mem.region.freeze.1]`). An
  implementation whose value semantics write a method receiver back
  must not count writing back an unmodified receiver as a write:
  `frozen[0].body.words()` is a legal read. (Appended 2026-08-11 —
  DRAFT, the bs06 ledger's frozen-read question: the book expected
  the read, the reference machine trapped `region-fault` on the
  receiver write-back. Ruling: the read is legal; the trap is an
  implementation defect to repair, not a spec ambiguity — this
  clause exists so the next machine cannot make the same choice.)

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
  | Reserved | created, unused (two-phase window) | ok (stays Reserved) | → Active | ok | → Disabled |
  | Active | live unique/mutable | ok | ok | → Frozen | → Disabled |
  | Frozen | shared/read-only | ok | **UB §7/P2** | ok | → Disabled |
  | Disabled | dead | **UB §7/P1** | **UB §7/P1** | ok | ok |

  ("child" = access through this tag or a descendant; "foreign" = through
  a non-descendant. Reserved surviving child *reads* is what makes the
  two-phase window real — activation happens at the first child write
  (published Tree Borrows; repaired 2026-08-09, the original table
  activated on reads by transcription error). Protected tags escalate the foreign-write transition
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
| P1 | Access through a Disabled tag (use-after-free, use of an invalidated borrow), or a foreign write to a **protected** tag | O1: `mut` params lower to `noalias` + `dereferenceable`; unique-tag stores forward without memory checks | O, Q |
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

### Iteration — `for`, ranges, and `Iter[T]` `[mem.iter]`

(Appended 2026-08-10, wolf-std F-0008 / issue #8: the protocol was
prototyped executing in std.iter — range_iter/list_cursor driven by
`while` + `else`, exhaustion-stays-exhausted proven — and these clauses
adopt that design and give `for` its desugar.)

- `[mem.iter.trait]` `Iter[T]` is a **nominal prelude trait** with a
  single method `next(mut self) -> T ! {done}`. Yielded values are the
  ok payload; exhaustion is the payload-free lowercase tag `done`.
  Exhaustion is **stable**: after `next` raises `done`, every later
  `next` raises `done`. Weighed and rejected: an absence tag (`none`)
  as the end signal — iteration's end is its own noun, and `done` keeps
  absence and exhaustion separable in rows carrying both.
- `[mem.iter.for]` `for pat in e { body }` desugars by cases on `e`'s
  type. Builtin ranges iterate directly, with no trait machinery (D25;
  `[mem.iter.range]`). Otherwise `e`'s type must implement `Iter[T]`
  and the loop desugars to the explicit drive loop:

  ```text
  var it = e
  loop {
      let pat = (mut it).next() else { break }
      body
  }
  ```

  `break`/`continue` in `body` target the desugared `loop`; evaluation
  and `defer`/`errdefer` order per `[mem.model.order]`.
- `[mem.iter.range]` `a..b` / `a..=b` are a **closed builtin family**:
  `for` iterates ascending, `+1` steps, checked arithmetic (X3); both
  endpoints are evaluated exactly once, left-to-right, before the first
  test. An owned range value implements `Iter[int]` with identical
  semantics.
- `[mem.iter.impl]` `List[T]` and `Pool[T]` adopt `Iter` builtin-side
  (std surface); user types implement the trait **by name** — no
  structural conformance.

### `str` ordering `[mem.str]`

(Appended 2026-08-10, wolf-std F-0006 / issue #7. Interim home — a
future strings document may take the namespace over. This ruling adopts
lupin's executed byte-order behavior at the sc01 pins.)

- `[mem.str.order]` The relational family `< <= > >= <=>` is
  **defined** on `str` × `str`: byte-lexicographic over the UTF-8
  bytes, unsigned byte compare, shorter string first on a shared
  prefix. The order is total on all `str` values and consistent with
  the byte-offset commitment of D25 (`[gram.lex.source]`). `==`/`!=`
  remain byte equality. `<=>` on `str` yields the same ordering value
  as on integers (the v0 `int` read).
- `[mem.str.impl]` Consequence: `impl Ord for str` is shippable
  in-library with **no bytes accessor**; the operator family and the
  impl agree by definition.

(Appended 2026-08-10, s37 core types — wolf-std F-0018 / issue #17.
The boundary primitive: a byte-offset string library cannot ask
whether an offset is a code-point boundary without slicing at it, and
slicing at a non-boundary is a fault. `get` is the recoverable twin —
the one primitive that cannot be written in library code, because
writing it requires itself.)

- `[mem.str.get]` `s.get(a..b) -> str ! {none}` is defined for every
  `str` and every pair of `int` byte offsets. It answers the same
  question as the checked slice `s[a..b]` with the same domain:
  **exactly** the inputs on which `s[a..b]` faults `bounds` — an
  offset outside `0..=s.len`, `b < a`, or an offset that splits a
  UTF-8 code point — answer the tag `none`, and every other input
  answers the slice value `s[a..b]` would produce. No third outcome
  exists: `get` never faults on any input, and a hit is bit-identical
  to the checked slice. End-relative endpoints (`^n`) and open ends
  resolve exactly as in `s[a..b]` before the domain question is
  asked.

(Appended 2026-08-11, s71 — wolf-std F-0055/F-0056, issues #56/#57.
Two lanes refused what one answered; a primitive whose meaning depends
on which rung ran it cannot be delegated to. Both rulings adopt total
definitions so `std.str` drops its guards and delegates.)

- `[mem.str.empty]` The searching family is **defined** on an empty
  needle, on every lane: an empty needle matches nothing.
  `s.count("") == 0`; `s.split("")` yields the whole string as one
  piece; `s.replace("", t) == s`. No lane refuses, no lane traps —
  the three answers above are the only conforming ones. (These are
  the answers the native runtime always gave; the checked lane and
  the interpreter move to them.)
- `[mem.str.repeat]` `s.repeat(n)` with `n < 0` is a caller contract
  violation: the deterministic trap **`assert`** (`[conf.trap.map]`),
  on every lane. It is not `bounds` — no access is out of range — and
  it is not the empty string (the sc03-era interpreter answer, retired
  deliberately here; the trap was already the executed behavior on all
  three lanes, previously spelled `bounds` and cited against
  `[mem.ub.defined]`, which never defined it). `n == 0` answers `""`.

(Appended 2026-08-13, s84 — wolf-lang#95. `words`, `lines` and `split`
had no ruling: the two lanes agreed only because one was written from
the other, and "whatever Rust's `split_whitespace` does" is not a
definition a third implementation can be held to. Lazy iteration forced
the question — a compiler cannot inline a predicate nobody has written
down — so the clauses come first and both implementations follow them.
Everything below was already the executed behaviour on both lanes: this
is a ruling, not a behaviour change, and the corpus now witnesses it on
the inputs that would have decided it either way.)

- `[mem.str.ws]` The **separator set is Unicode `White_Space`**: the 25
  scalars carrying that property, and no others.

  ```text
  U+0009..U+000D   U+0020   U+0085   U+00A0   U+1680
  U+2000..U+200A   U+2028   U+2029   U+202F   U+205F   U+3000
  ```

  This is the one set `words`, `trim`, `trim_start` and `trim_end` test
  against. A position in a `str` is a separator when the scalar
  **encoded** there is in the set — never when a byte of a longer
  encoding happens to resemble one. A UTF-8 continuation byte is
  therefore never a separator, so every boundary these operations
  produce is a code-point boundary and every `str` they yield is one
  `[mem.str.get]` would have handed back. U+180E is deliberately absent
  (it left `White_Space` in Unicode 6.3). The set is **frozen at v1**:
  a later Unicode revision does not silently re-split a program's text,
  and a lane whose host library grows a 26th scalar has diverged, which
  `[proto.*]` is entitled to catch. Weighed and rejected: "whatever the
  host's `is_whitespace` says" — that is a lane-dependent primitive,
  exactly what `[mem.str.empty]` was written to stop.

- `[mem.str.words]` `s.words()` yields the **maximal non-empty runs of
  non-separator scalars**, left to right. That single sentence settles
  the boundary cases, and they are the ruling: a yielded word is never
  empty; leading and trailing separators yield nothing; a RUN of
  separators is one boundary, not several; `"".words()` and
  `"   ".words()` yield nothing at all. Consequently `s.words()` and
  `s.trim().words()` agree for every `s`, and a word counter is a
  counter rather than a counter plus a non-empty filter. Weighed and
  rejected: the field reading, under which `" a ".words()` would be
  `["", "a", ""]`. `words` answers *which words are in this text*, and
  the empty string is never an answer to that; a caller who wants
  fields has `split`, which gives them exactly and keeps the two
  spellings meaning different things — which is the point of having
  both.

- `[mem.str.lines]` `s.lines()` splits on **U+000A (LF) only**, and
  absorbs one U+000D (CR) immediately preceding an LF. Precisely: `s`
  decomposes uniquely into segments, each either ending in LF or being
  a non-empty final segment containing no LF; each segment yields one
  line, being that segment minus its trailing LF, minus one further CR
  if a CR immediately preceded that LF. So `"".lines()` yields nothing;
  `"a"` and `"a\n"` both yield `["a"]`; `"\n"` yields `[""]`;
  `"a\n\nb"` yields `["a", "", "b"]`; and `"a\r"` yields `["a\r"]` —
  the CR is absorbed only when an LF followed it, never on its own.
  Empty lines are REAL (a blank line in a file is an empty yield); only
  a trailing LF at the very end produces none, because it terminates
  the line before it rather than opening one after it. U+2028, U+2029
  and U+0085 are separators for `words` and are **not** line
  terminators here. Weighed and rejected: the UAX #14 line-boundary
  set. `lines` is the operation that reads a FILE, and a file's lines
  are delimited by the bytes a writer wrote; splitting U+2028 out of a
  string literal that merely contains one loses data in the direction
  nobody asked for. The Unicode reading is writable in library code on
  top of this one; the file reading is not recoverable from it.

- `[mem.str.split]` `s.split(sep)` splits at every **leftmost,
  non-overlapping** occurrence of `sep`, scanning left to right, and
  yields the text between occurrences — **every** field, empty ones
  included. The count is exact, and it ties the two operations
  together: `s.split(sep)` yields exactly `s.count(sep) + 1` fields,
  for every `s` and every `sep` **including the empty one**
  (`[mem.str.empty]` makes an empty needle match nothing, so `count` is
  0 and `split` yields the one whole-string field — the rule and its
  apparent exception are the same rule). Hence a leading separator
  yields a leading empty field, a trailing separator a trailing empty
  field, a run of `k` adjacent separators the `k-1` empty fields
  between them, and `"".split(sep)` yields one empty field, never zero.
  Weighed and rejected: collapsing empty fields. `split` is the
  parser's primitive — `"a,,b"` has three fields and the middle one is
  empty, and a reader that cannot see that cannot round-trip.

- `[mem.str.view]` All three yield **views**: every yielded `str` is a
  subslice of the receiver's own storage — the same `{ptr, len}`
  representation `bytes()` and the `trim`/`get`/`strip_*` family
  already hand back — so iterating them allocates **nothing**, on every
  lane. This is a commitment, not an optimization note:
  `[mem.region.create.3]` charges every materialization to the ambient
  region and these operations perform none, which is what makes
  `for w in s.words()` legal under `#[noalloc]`. A `List[str]` in a
  non-iterating position is still an allocation — but the LIST is,
  never the strings inside it.

- `[mem.str.view.lend]` `s.bytes()` in an **argument position** is
  OFFERED as a lend, not a copy, and the lend is an optimization with a
  defined fallback — never a rule a program can violate. Which positions
  materialize is a rule of the language and not a property of a
  compiler: `s.bytes()` yields a view when it is *consumed on the spot*
  — iterated, indexed, asked for `len`/`count`/`is_empty`/`get`/
  `first`/`last`, or **passed as an argument to a function that only
  does those things with it** — and materializes a `List[int]` in every
  other position, `let` bindings and returns included. Three cases at a
  call, decided by the callee's body: (1) a callee that only READS its
  parameter, in the positions above, is lent the receiver's own
  `{ptr, len}` and the call allocates nothing; the lend's deal is the
  region checker's, one scale down — the callee may read the bytes for
  the call's duration and may not keep them, and the caller's string
  stays borrowed across the call (a `str` is immutable at every tier,
  so nothing may change under the callee). (2) A callee whose use of
  the parameter this analysis cannot classify is not lent: the caller
  materializes for it, exactly as every caller did before views crossed
  calls. (3) A callee that provably KEEPS the parameter past the call —
  returns it, stores it, hands it on — is not lent either, for the same
  reason a region may not outlive its scope: the caller materializes,
  and the compiler says so once (W1004), naming the escaping use and the
  one-word fix (binding first, `let bs = s.bytes()`, if the copy was
  the intent; changing the callee, if the lend was). Cases (2) and (3)
  compile to the same thing — a copy — and differ only in what the
  compiler can prove and therefore say. **The copy is observable only
  in cost**: in every case the program means what it meant when views
  materialized everywhere, and no program has a meaning under a lend
  that it lacks under the copy. There is deliberately no way to spell
  "keep these bytes and do not copy them", because that spelling has
  no meaning that is not a dangling `{ptr, len}`. A byte view has no
  write path in any position (`[mem.str.get]`'s UTF-8 guarantee
  survives construction: nothing may forge a `str` by writing one's
  bytes). *History:* s89–s91 refused case (3) as E1015; s92 retired
  the refusal in favour of the copy-and-warn (#107, #108).

---

- `[mem.dyn.unsize]` A trait object is constructed by an **explicit
  cast, from a place** (D47): `v as dyn Trait`, where `v` denotes a
  binding, a field, or an index, and the concrete type carries the
  coherence-unique impl of the dyn-safe trait. There is no implicit
  coercion — the coercion table grows by addition, never by a new
  implicit mechanism — and a temporary is refused (E0810): the pair's
  data half points AT the operand, and a temporary has no home to
  point into. The cast is a **shared lend of the place** for as long
  as the pair is needed: writes to and moves of the place while the
  pair lives are refused, the same deal as every other borrow in this
  chapter, one object at a time. A pair lives in a local or crosses a
  call as an argument; it does not (yet) cross a return, enter a
  container, a field, or a row — each of those is a borrow story not
  yet written, refused by name rather than guessed. The vtable the
  cast builds is `[abi.native.dyn]`'s: one content-interned table per
  (trait, impl) pair, slots in the dyn report's canonical order,
  shims owned by the table.

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
