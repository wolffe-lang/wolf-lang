
## lsp-windows-stdio (2026-08-10)

The three `lsp_one_truth` publish-waiting tests wedge on Windows CI. The
server receives `didOpen` over piped stdio and never publishes: no panic on
inherited stderr, alive until the 60s watchdog kills it. URIs are verified
parseable (`file:///D:/…`), macOS and Linux are green, and the in-process
wolf_lsp transcript suite is green on Windows too, so the protocol logic is
answering and the stdio/threading layer is where to look. Quarantined with
`#[cfg_attr(windows, ignore)]` until there is real signal (an env-gated
server trace, or a Windows box). s57 owns it; that sprint rebuilds this
layer.

## release-tier fact channels owed to s42 (2026-08-12)

Three gaps in the s41 fact emission. Each waits on an analysis s41 does not
own.

1. Call purity and termination bits (`memory(...)`, `willreturn`, `nosync`).
   WIR carries no purity facts at v0, so the LLVM tier emits none. The
   lowering rule is a one-liner once s42's analyses mint them.
2. `deref` facts with scaled sizes (`8x%n`). `dereferenceable(n)` wants a
   constant, so scaled claims are dropped. Revisit when s42's range analysis
   can bound the count.
3. ~~`!invariant.load` detection is direct-def only. A frozen token that
   reaches a load through a block parameter is not traced, so those loads
   lose the fact.~~ **Not a gap (s78).** A block argument is a consuming
   position and the verifier rejects a frozen token in one
   (`ErrClass::FrozenToken`), so a frozen token cannot reach a load
   through a block parameter at all. The direct-def test is complete for
   every input WIR admits; the token-origin dataflow would be dead code.

All three are performance-only by construction (D2: dropping facts is always
sound).

## container disjointness owed to the checker (2026-08-12, s75)

s75 made `List` element access ordinary memory the optimizer can see. Two
regions carry it: container headers and container element buffers, which the
runtime always allocates separately, so the pair is a theorem rather than a
convenience. WITHIN each region nothing is claimed, because nothing is
proved: `let b = a` copies a header pointer, so two `List` values may share
one buffer, and a region per container would be a `!noalias` pair no theorem
backs.

The cost is measurable and named. In `alias_daxpy` the two buffers arrive as
a `mut` and a `read` parameter. **Corrected at s78 (wolf-lang#82):** the c04
mode theorems do NOT prove those two BUFFERS disjoint. `wolf_mem`'s place
language has no dereference projection (`place.rs`: `Field | Opaque`), so
`[mem.model.path.disjoint]` is a claim about the argument places — the header
pointers — and `excl.rs` proves it pairwise over one call surface. The
existing `Noalias(p,q) : excl.mut` is about those pointers (for a `mut` arg,
a fresh per-call spill slot), and propagating it to the loaded buffer
pointers is a NEW theorem, not a mechanical transfer. Until c04 proves
container-payload disjointness, emitting the pair would be an unsound
`noalias` — the class of bug the whole fact rig exists to prevent.

What s78 established instead:

- The scope channel already reaches loaded pointers. Every load and store
  through a region-tagged pointer carries `!alias.scope`/`!noalias`,
  including element accesses whose address came out of a header
  (`every_region_access_carries_scopes` pins it; the `loaded_pointer` fuzz
  shape covers it). #82's candidate fix — "emit scopes on loads, not just
  attributes on params" — was already in place since s41; the gap is the
  theorem, not the channel.
- The channel is REDUNDANT, not empty. With the mid-end off
  (`WOLF_MIDEND=0`, the s44 measurement hatch), stripping the facts costs
  +22% on `alias_daxpy` and +57% on `a2_stencil1d`; with the mid-end on it
  is 0 within the layout floor. `memopt`'s token-keyed availability and
  `licm` already perform the motion the metadata would license, so the
  sentinel's +0.0% says "already cashed upstream", not "nothing to spend".
- The sentinel's resolution is the other half of the +0.0%. At the
  manifest's `ops` the family-A kernels run 2-6 ms on a 1 ms clock
  (`time_now_ms` is the only clock a kernel has), so the metric is
  quantized to 17-50%. Raising the sweep counts ~100x is what made the
  readings above possible; the manifest sizes date from before s75 made the
  wolf lane two orders of magnitude faster.

A hazard recorded while declining it: the mid-end's "a call that does not
consume a region's token cannot touch that region" (memopt's header claim)
does not survive `region.foreign`. Foreign roots are minted per function
over storage the RUNTIME owns, so a callee can write a caller's foreign
region without receiving a token — `@stencil` does exactly that. No dynamic
witness was produced (the shapes that would show it inline first), so this
is a hazard to audit, not a filed miscompile; s78 refused to hand the same
claim to LLVM as call-site `!noalias`. **Audited at s80 — see below.**

## the foreign-root audit and what it turned up (2026-08-13, s80, #83)

s80 went looking for the #83 miscompile and found a DIFFERENT one, live and
reachable from source, in a fact this file never listed because nobody had
doubted it: **one `!alias.scope` per region id**.

`region.foreign` roots are per function. The inliner freshens every
callee-internal region id — correct for `region.new`/`stack.alloc`, where
the callee's arena really is its own — and a foreign root is neither. So
inlining any container-touching callee left the caller holding two roots
over the SAME element buffers, and one-scope-per-region declared them
`!noalias`. LLVM cashed it. The witness, `wolf run --release` against `wolf
run`:

```
fn peek(l: List[int], i: int) -> int { l[i] }
fn main() -> int {
    var a = List[int]()
    (mut a).push(0)  (mut a).push(0)  (mut a).push(5)
    let i = a[0] + 2          // 2, opaquely
    let j = a[1] + 2          // 2, opaquely
    let x = peek(a, i)        // inlined: loads under a FRESH foreign root
    a[j] = 7                  // stores under main's own foreign root
    let y = peek(a, i)        // inlined again, another fresh root
    print("x={x} y={y}")
    0
}
```

Checked lane and debug native: `x=5 y=7`. Release: `x=5 y=5`. In the -O3 IR
the second `peek`'s load is gone and `%t33` — the value loaded BEFORE the
store — is printed twice:

```
%t33.i = load i64, ptr %t32.i, !alias.scope !21, !noalias !22   ; x
store i64 7, ptr %t38.i,       !alias.scope !18, !noalias !19   ; a[j] = 7
call void @__wolf_rt_print_i64(i64 %t33.i)                      ; x
call void @__wolf_rt_print_i64(i64 %t33.i)                      ; y, stale
```

The two indices have to be opaque-but-equal or basic AA answers MustAlias
and never consults the metadata — which is why the earlier attempts at a
witness kept coming out correct.

**Fix.** Region identity is not the aliasing unit for runtime-owned
storage; the ROLE is. `region.foreign` carries a role immediate now
(`0` header, `1` buffer, a closed set the verifier enforces), and every
consumer that claims disjointness keys on the class:

- the LLVM emitter gives all same-role foreign regions ONE `!alias.scope`;
  header-vs-buffer separation is untouched, which is the whole D46 theorem
  and all s75 ever needed.
- `memopt` drops availability entries whose token names a foreign region at
  every call, at every store through a same-role foreign token, and on
  entry to any loop header whose body does either — the last one because
  the pass's kill is a linear RPO scan and a back edge is not linear.
- `licm` will not hoist a foreign-token load out of a loop that contains a
  call or a same-role foreign store. A loop-invariant TOKEN is not
  loop-invariant MEMORY once the token stops being exhaustive.

**Why conservatism and not token propagation.** Propagating the foreign
tokens through signatures is the stronger answer and it would also unlock
s78's declined call-site `!noalias`: the call would consume and re-mint the
caller's foreign chain, the inliner's existing signature binding would map
the callee's roots onto the caller's with no freshening, and memopt/licm
would need no special case at all. It is not a mid-end change. It puts two
token parameters on every wolf function signature and has to answer for
`@main`, exported functions, `c_call`, task entries, and the backends' entry
shims — a lowering-wide ABI change (free at the machine level, since tokens
erase) that an audit sprint should not land alongside a live miscompile
fix. It stays open as the way to CLAIM the fact rather than merely stop
relying on it.

**#83's own hazard: real, and still not reachable from source.** The
non-inlining shape — an opaque callee writing the caller's foreign storage
with none of its tokens — is admissible WIR and now has a differential
witness (`fuzzgen::shape_foreign_cross_call`, plus the LICM variant). What
stops today's LOWERING from emitting it is worth naming precisely, because
it is an accident and not a theorem:

- a `mut List` argument spills its header pointer to a stack slot and
  RELOADS it after the call, so the caller's post-call element address is a
  fresh SSA value and memopt's `(token, address)` key misses. Re-lending a
  `mut` parameter passes the slot through instead, and then the slot's own
  token is what changes. Either way the address, not the token, is what
  saves it.
- two live `List` values cannot share a buffer at source level: `let b = a`
  moves, and `copy a` is a deep copy. The IR-level sharing the s78 note
  describes is real, but the move checker keeps two readable paths to one
  buffer out of a single frame.

Neither is a claim about tokens. Both would evaporate if the `mut` spill
got smarter or a sharing container landed, and the pass comments asserted
the opposite of what the code relied on. The conservatism costs nothing
today (below), so there is no reason to keep resting on them.

**Cost, per kernel.** The mid-end rules cost EXACTLY ZERO: optimized WIR is
byte-identical with them on and off across all thirteen kernels
(`a2_stencil1d`, `a5_hoist_call`, `alias_daxpy`, `aos_dot`, `b3_churn`,
`c2_ecs_sweep`, `d1_utf8_validate`, `d2_substr_search`, `e1_sum_reduce`,
`e2_checksum`, `e3_index_arith`, `list_alloc`, `word_count`) — the shapes
they decline are the shapes the lowering does not currently produce, which
is the same fact as "no source witness". The scope-class change removes 0-2
`!alias.scope` nodes per kernel, and every claim it removes was a false one
(the duplicate roots inlining minted); the header/buffer pair survives in
every kernel that has one. Self-timed medians over 7 alternating runs, this
worktree, this box — `a2_stencil1d` 161 vs 159 ms, `alias_daxpy` 46 vs 49
ms, `list_alloc` 6805 vs 7000 ms — are inside the noise in both directions.
s79 is re-measuring the suite concurrently on its own baseline; these
numbers are a cost check, not a suite reading.

## the ABI answer #91 wanted, and why s83 did not land it (2026-08-13, s83)

s83 wrote the inventory s80 owed — what token propagation does at each of
ordinary calls, `main`, exports, `c_call`, task entry shims, task/proc
bodies, runtime shims, and indirect calls (none exist). Two things fell
out of writing it, and together they are why the propagation is a
successor sprint and not this one.

**The mechanism is not the problem.** Token params have been ordinary
signature params since s26: `verify.rs` checks a formal→actual region
substitution per call site, `ins_call_regions` threads them and mints
successors, `FuncBuilder::new` turns a `mem.rN` entry param into a live
chain, and `rt_call_foreign` ALREADY passes both foreign tokens to every
container shim. Appending two more params to `wir_sig_of` is a small
edit. Tokens erase at both backends, so the change is free at the
machine level.

**1. Uniform propagation restores no optimization by itself.** If every
wolf signature carries both foreign tokens then every wolf call consumes
and re-mints them, so availability keyed on the pre-call token is
unnameable afterwards — memopt loses container CSE across calls by token
VERSIONING instead of by s80's explicit rule, which is the same
conservatism relocated. licm likewise: the token stops being
loop-invariant. Restoring the motion needs a transitive per-callee
effect summary ("never stores through its role-R token, mints no role-R
root, reaches no tokenless seam that could") used to RE-KEY availability
across the call. `midend/summary.rs` is the home and its schema is
**frozen at v1**, so that is a version bump with c12 and the tooling
track downstream of it. The blocker for memopt/licm is the SUMMARY, not
the signatures — naming that correctly is the most useful line here.

**2. The propagation has a trapdoor that must land with it.**
`build_scopes` (`emit.rs`) learns which regions are foreign by scanning
for `region.foreign` INSTRUCTIONS. The moment a foreign root arrives as
an entry PARAMETER instead, the emitter classifies it `Local(r)` — one
scope per region id — and two same-role foreign regions in one function
are declared `!noalias`. That is the s80 miscompile verbatim.
`memopt::foreign_roles` has the same shape and the same hole. So the
role has to become a field on `ir::Param` and travel with the signature
first, with every role consumer reading both places, and only then the
signatures. Any other order ships a miscompile. #91 stays open with this
as its plan.

**What s83 landed instead.** s78's declined call-site fact, in the half
that has a theorem. The emitter's own note already said it was
emittable: local regions' tokens ARE exhaustive. A call now carries
`!noalias` over every region `wolf_wir::midend::exhaustive_regions`
admits — rooted here by `region.new`/`stack.alloc` or lent by the
caller, with no pointer escaped and no handle handed out — whose token
the call does not take. Foreign roots are never listed; that decline
stands, now for a stated reason. The predicate is IMPORTED by the
emitter rather than re-derived, so the fact rig and the passes cannot
drift.

**#92, and it was real.** `rle_and_forward` rests on "no token ⇒ no
effect" and had no escape guard, while `dse_dying_regions` has had one
since s42. A local region whose pointer reached a tokenless seam
(`__wolf_rt_print_str`-class shims take raw pointers) is writable
without its token, and rle forwarded across it. Same guard now, same
predicate, plus the loop-header form of it — a call in a loop body runs
before the header's second visit, and the pass's kill is a linear RPO
scan (the same back-edge subtlety s80 hit). licm gets the matching rule.
Witnessed by `fuzzgen::shape_call_escaped_pointer` on BOTH axes.

**#93, pinned.** The two accidents that make #83's original shape
unreachable from source are now tests
(`lower_shapes.rs`): the `mut`-arg spill's post-call reload, and `let b
= a` moving. Neither is a claim about tokens; both are now claims that
fail loudly if they go.

**Cost, per kernel: EXACTLY ZERO.** Optimized WIR is byte-identical with
the #92 guard on and off across all thirteen kernels (`a2_stencil1d`,
`a5_hoist_call`, `alias_daxpy`, `aos_dot`, `b3_churn`, `c2_ecs_sweep`,
`d1_utf8_validate`, `d2_substr_search`, `e1_sum_reduce`, `e2_checksum`,
`e3_index_arith`, `list_alloc`, `word_count`) — same hashes, same
`rle`/`fwd`/`dse`/`hoist` counters. The shapes it declines are shapes
the lowering does not currently produce, which is the same fact as "no
source witness", which is why the fuzz shape is the witness.

**And the new fact buys nothing on those kernels either, which is the
finding.** Emitted call-site `!noalias` count at `--release`: 0 of 27-31
calls, on every one of the thirteen. The kernels' region traffic is
container traffic, and container storage is exactly the FOREIGN case the
fact declines. The theorem is right, the channel works
(`llvm_goldens::a_call_claims_only_the_regions_it_cannot_reach` pins
both directions), and it will stay unspent on this suite until #91
lands. That is the honest measurement of what the ABI change is worth,
taken before paying for it.

## a2_stencil1d's early exits are two thirds ARITHMETIC (2026-08-13, s78)

s78's affine relational channel discharges three of the stencil's four
bounds guards (`bounds: 0/10 → 3/7` on that file). The wall clock did not
move — 246 ms before, 250 ms after over 11 alternating runs, inside a
236-281 ms spread — and the loop still does not vectorize. The remark is
unchanged: *"Cannot vectorize early exit loop with more than one early
exit"*, because the surviving exits are the ONE remaining bounds guard plus
the TWO `iadd.chk` overflow traps of `src[i-1] + src[i] + src[i+1]`. The
s75 note that "four surviving guards means four early exits" undercounted:
the arithmetic contributes two more, and they are the harder pair.

Closing it needs both halves:

- the last bounds guard (`i <u out.len`) — the guard relates `i` to
  `src.len`, and nothing relates `out.len` to `src.len`. This is the
  loop-versioning generalization s75 named: guard on `src.len <= out.len`
  (a limit against another loop-invariant VALUE, not a constant `K`). Note
  that the affine channel cannot chain it for free — it reasons about one
  base PAIR, and this needs transitivity across two pairs (a
  difference-bound closure).
- the two overflow checks — the loaded elements have no range, so nothing
  bounds `src[i-1] + src[i]`. A `range` fact on container element loads
  (the values were stored masked) or versioning on the loaded values is
  what X3's claw-back would need here.

## the packed spill layout claims natural alignment (2026-08-12, s75)

`flat_offsets` packs fields end to end while `Opcode::Load`/`Store` claim
`natural_align` at the LLVM tier, so an aggregate like `{i32, i64}` puts an
`i64` at offset 4 and loads it `align 8`. This predates s75 (it is the v0
`mut`-arg spill layout) and no corpus shape reaches it. s75 does not widen
it: `List` element access refuses a stride that does not tile at the
element's alignment rather than emit a misaligned access at `k*esize`. The
real fix is an aligned flat layout, or alignment on the WIR memory ops.
