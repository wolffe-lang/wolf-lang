
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
3. `!invariant.load` detection is direct-def only. A frozen token that
   reaches a load through a block parameter is not traced, so those loads
   lose the fact. This is sound and conservative; a small token-origin
   dataflow closes it.

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

The cost is measurable and named. In `alias_daxpy` the two buffers are a
`mut` and a `read` parameter, which the c04 mode theorems ALREADY prove
disjoint — the fact exists (`FactKind::Noalias`, `Just::Theorem(ExclMut)`),
and the LLVM tier spends it on the parameter pointers. It cannot spend it on
the element buffers, because those are values LOADED from the headers, and
the noalias channel at v0 reaches LLVM only through parameter attributes and
region scopes. Closing it means propagating a proved-disjoint pair from the
container values to their buffer pointers, and giving those pointers their
own scopes. That is family A's remaining gap, and it is what the aliasing
kernels are actually measuring once G1 is closed.

## the packed spill layout claims natural alignment (2026-08-12, s75)

`flat_offsets` packs fields end to end while `Opcode::Load`/`Store` claim
`natural_align` at the LLVM tier, so an aggregate like `{i32, i64}` puts an
`i64` at offset 4 and loads it `align 8`. This predates s75 (it is the v0
`mut`-arg spill layout) and no corpus shape reaches it. s75 does not widen
it: `List` element access refuses a stride that does not tile at the
element's alignment rather than emit a misaligned access at `k*esize`. The
real fix is an aligned flat layout, or alignment on the WIR memory ops.
