
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
claim to LLVM as call-site `!noalias`.

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
