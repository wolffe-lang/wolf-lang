# Changelog

## Unreleased

### The server navigates (s133 — #208 closes)

`wolf lsp` serves `textDocument/definition`, `textDocument/references`
and `textDocument/rename` (with `prepareRename`) — the three rungs
s122 named and nobody had climbed, so F12 / Shift+F12 / F2 on a `.lu`
file did nothing in every editor. All three answer from a **binding
table**, never a textual search: the resolver now KEEPS the decision
it already makes for every name (`wolf_sema::Resolution::refs`, one
list per file — uses and binders alike, a binder being a ref to
itself, an item declaration an `Item` ref to itself, an import's
bound name a ref to what it imports), and the checker keeps the
type-dependent half it already resolves (`TypedBody::member_refs`:
fields through `.`, struct-literal and pattern fields, enum variants
in value, call and pattern position, methods and associated
functions — with the declaration's name token, which may sit in
another file). The two halves share one key — the declaration's name
token span, `FileId` included — so a cross-file answer is a lookup
over the package graph (D32 modules, `//! member` files, `use`
targets), not a scan. Locals, parameters, generic parameters, pattern
binders, block-level items, scope and region names, import aliases
(`use m.x as y`: uses of `y` bind to the alias, the path segment binds
to the item, so an item rename never rewrites the alias) all
navigate; a name the compiler never bound — a deferred error-row
tag, a member on an untyped receiver, anything inside a body that did
not reach typecheck — answers `null`, never a guess, and the lexical
half keeps answering when typing stopped.

The wire shapes follow the client's declarations, read once at
`initialize`: `LocationLink[]` (with the asking token as
`originSelectionRange`) when it declares
`textDocument.definition.linkSupport`, `Location[]` otherwise;
a rename's `WorkspaceEdit` as `documentChanges` (one
`TextDocumentEdit` per file, `version: null`) when it declares
`workspace.workspaceEdit.documentChanges`, the `changes` map
otherwise. References come back in (file, offset) order, the
declaration only when `includeDeclaration` asks. The negotiated
position encoding holds at every span — the utf-16 astral case is
pinned. Builtins and prelude names answer `null` for definition and
their uses for references.

Rename refuses BY NAME — a `RequestFailed` (-32803) `ResponseError`
whose message names the token and the reason, never a partial edit —
when the cursor is on a keyword (`self` and `Self` included), a
builtin type, a prelude name (D31), a std stub or `import c` symbol
(cross-package), or a module (a directory, D32); and when the new
name is not a single identifier or is a keyword. `prepareRename`
refuses with the same reasons before the rename box opens. The D59
`//!` member marker carries no identifier (`member:` is a boolean),
so no rename ever touches a `//!` line — the question the contract
asked, answered in `wolf_query::navigate`'s docs. Known residue,
named: the reachable set is the package around the ENTRY (the v0
single-entry model) — asked from a `member: true` sibling, references
see that module alone; a workspace-root model is s57's.

`wolf_query`'s contract moves to v4 (additive: `definition`,
`references`, `prepare_rename`, `rename`; `DefResult` gained
`origin`; `def_of` is the s52 name for `definition`). Protocol tests
land in `crates/wolf_lsp/tests/navigate.rs` over a two-file fixture;
query tests in `crates/wolf_query/tests/navigate.rs`. wolf-lsp's
transcript library gains one script per rung per maintained client
profile on its `s133-transcripts` branch (recorded against this
branch's binary; le05 re-pins at the tag), and the still-absent set
is now signature help, semantic tokens, inlay hints, range
formatting, workspace symbols and pull diagnostics (s134's rungs).
Latency, measured before and after on the same machine (wolf-lsp's
`lspconf bench`, `regions.lu`, 20 fresh processes): `diagnostics-
after-edit` p50 104.9 → 104.6 ms, p95 105.7 → 105.7 ms — the one
number near perception holds (the binding table rides the resolve
walk the ladder already runs); hover/documentSymbol/formatting/
codeAction unchanged at their sub-millisecond floors; the new
requests answer in ≈0.03 ms p95 in-process (`lsp_bench`: definition
0.032, references 0.035, rename 0.033) — a lookup on the memoized
analysis, no re-resolve.

### The region holds (s132, D68 — #187 closes)

The cap half of #187 lands whole, in D68's ruled direction. A region
takes a creation-time byte budget — `region r(cap: n) { … }` on the
named sugar block, `region(cap: n)` / `region(rc, cap: n)` on the
value form — and a charge that would take its ledger PAST the budget
is the deterministic trap `alloc-contract` at the allocating site:
the existing kind (`[conf.trap.map]` — a byte budget is an allocation
contract), no new trap vocabulary. At-cap-exactly is not a breach;
the next byte is; `cap: 0` is legal and everything breaches it; a
negative budget is the same contract violation at the creating site.
The cap bounds `[mem.region.account.1]`'s ledger in each tier's own
units — and the clause now says plainly that budgets are denominated
in LEDGER units, not payload bytes (the ledger is cumulative and
high-water; #203 measured a 64 KiB io chunk at ~1 MiB of ledger), so
portable programs derive budgets from measured `region_bytes`
readings — which is exactly how the witnesses pin the boundary on
all three tiers at once. New clauses `[mem.region.cap.1/.2/.3]`.

The fault half is D68's point: a breach **inside a proc** no longer
kills the process. The trap is contained at the proc boundary
(`[conc.proc.1]`'s failure domain): the proc dies by the killed-proc
sequence — no further user code, so no `defer` below the boundary
runs — and `[conc.proc.exit]`'s closed reason set gains the mapping,
`fault(kind)` with `kind` from the closed trap vocabulary, read at
the join as `is_fault()` and, for the budget breach specifically,
`is_alloc_contract()`. Fault teardown is free-then-deliver, measured
and then pinned as `[mem.region.cap.3]`: at the join,
`live_region_bytes()` has already returned to its pre-spawn reading —
the supervisor that answers 503 on this reason was handed the memory
back first. Containment is the no-unwinding law made mechanism
(`[abi.native.nounwind]` holds: the trapping worker parks; its thread
and stack high-water are the measured per-breach cost, the same class
as a task parked on a silent channel). In the root domain a trap
remains process death (`[conf.trap.exit]`, amended to say so).
Witnesses: `faults/region_cap_breach.lu` (three-tier breach at
measured-minus-one), `memory/region_cap_boundary.lu` (at-cap /
cap-0 / query interplay), `conc/proc_cap_fault_join.lu` (the
join-reason read, both defers, the reclaimed-at-join pin — the shape
lobo's ws12 consumes).

### The comma insists everywhere (s132, D69 — D67 completed)

The remaining unlicensed comma laxity is gone: struct LITERAL fields
(`Point { x: 7 y: 2 }` — and the newline-separated spelling, which
lupin always refused), closure parameters (`fn(a b)`), and inline-C
capture lists (`unsafe c [a b]`) now refuse at E0201 with the same
machine-applicable "add the comma" insertion D67 gave the pattern
family (`wolf fix --apply` produces the canonical spelling; the
newline form is reported at the field the missing comma should
precede, byte-for-byte where lupin points). The multi-line literal's
trailing layout is untouched: a terminator run before `}` is the
production's own. Blast radius, measured before the tightening, is
zero working code anywhere: the wolf-lang corpus and fixtures (532
files), wolf-book (1,129), wolf-std (887), wolf-web (1,976 —
counting two vendored copies of the one flagged file), and lobo
(1,395) — the sole flagged file in the world is a fuzz-minimized
broken-input formatter fixture (`idem_comment_vs_comma.lu`), whose
idempotence test still holds. The separator report latches once per
list and never into a reported wreck; MUTATE_BUDGET=300 swept green.
Two corpus refusal witnesses pin the family (the newline form is
pinned in the parser suite instead — the formatter's canonical
multi-line layout regenerates the separator, and a corpus file must
fully format); the spec sentences land with the fix under
`[gram.expr.primary]`, `[gram.expr.closure]`, and
`[gram.expr.unsafe]`.

### The region answers (s131)

The region accounting queries land three-tier (#187, the wolf-web
`memory_budget` customer): `region_bytes(r)` reads a named region's
byte ledger — the count `wolf_rt` has kept since s76, now surfaced —
and `live_region_bytes()` reads the process-wide live-region total.
The new clause `[mem.region.account]` pins what every tier guarantees
(zero at creation, monotone within the lifetime, stable between
allocations, wholesale disappearance at free) and leaves the units as
per-tier measured facts: the native arena charges alignment-rounded
container storage, the checked machine its shadow-memory model. The
str gap stays #191's (string bytes charge no named region on the
native tier until the c09 seam closes — recorded in the clause).
#187's second half — the creation-time cap and its fault semantics —
is deliberately not here: the honest designs all need a ruling
(catchable-row-at-the-boundary wants a mechanism `[abi.err.repr]`
forbids), and the r04 lesson says the clause must not outrun the
differential.

### The comma insists (s131, D67)

The pattern family's separating comma is now required, as the
production always said (#190): `Point { x .. }`, `Point { x y }` and
`(a b)` refuse at E0201 with a machine-applicable "add the comma" fix
(`wolf fix --apply` produces the canonical spelling), and `..` follows
a separator like one more member. Blast radius, measured before the
tightening: zero — the wolf-lang corpus, fmt's output, and every one
of wolf-book's `.lu` files (the 248-file exercise corpus included)
already write the comma; only lupin and the spec's letter were ever
this strict, and the compiler now agrees with both. The r04 spec
sentence and witness that were backed out of 0.2.1 land WITH the fix:
`[gram.pat.struct]`'s production tightens to `(',' '..'?)?` and three
refusal witnesses pin the family. The struct-literal laxity
(`Point { x: 7 y: 2 }` still parses) is outside D67's letter and
stays measured residue on the tracker.

### The or-pattern divergence, measured (s131, #196)

Two witnesses pin the c06 residue's or-pattern halves: an or-pattern
OVER product alternatives (`Left(A { n }) | Right(B { n })`) refuses
by name on every wolfc lane, and an or-pattern INSIDE a product
(`Pair(1 | 2, b)`) refuses natively while the checked executor runs
it — both shapes lupin runs today (is31's measurement), a
permissive-direction divergence that was invisible until these files
put it in the differ's ledger. The join-params lowering itself stays
with the c06-residue sprint beside deep trees and str/float literals
in products.

## 0.2.1 — 2026-09-01

THE LETTER AND THE ARCHIVE. A patch release: no new features, and
nothing runs here that did not run at v0.2.0. What it carries is a tag
the Windows archive can finally reach, the pattern work that landed
between the tags, and four places where the written language and the
built one had drifted apart — each one measured on the tools before a
word of it was rewritten. It pairs with **lupin 0.1.20** at pin
`b80d239` (D57: the pin is part of this release's identity).

### Patterns take the product domain (s129, s130)

Struct patterns went through the whole pipe (`[gram.pat.struct]`,
#179): `Point { x, y: p, .. }` takes a struct apart by field name,
fields move field-wise so an unnamed field stays live, a pattern
without `..` must name every field (E0814 — the same lean E0408 takes
at construction), and the formatter has a canonical form for them.
The lent-view slice gap closed alongside it (#184): `b[lo..hi]` over
a lent byte view resolves per `[mem.list.slice]` instead of meeting a
misattributed c06 refusal.

Match arms then took the full product domain (s130, retiring that c06
arm): tuple and struct patterns, `@`-bindings, literals at product
depth, and products nested through enum/row payloads (`Pair(a, 0)`,
`Dot(Point { x, y: 0 })`) compile on both native tiers and execute on
the checked lane, with exhaustiveness and the redundant-arm warning —
which already reasoned over products — finally carrying running
witnesses. Two shapes stay refused by name on the native pipe: an
enum/row test inside a product, and a `str` literal inside one.

An arm still takes the WHOLE scrutinee when it binds a non-`Copy`
piece. The field-wise partial-move story remains a `let`-binder rule;
the boundary is pinned by an `E1001` witness and the diagnostic's
`copy` suggestion is the sanctioned idiom. Two checked-lane fixes rode
along — arm guards evaluate their condition rather than their wrapper
node, and a qualified constructor's dotted tag (`Pairs.Pair`) matches
a bare-name arm the way the native tiers compare tag ids — and one
release-tier fix: a type-blind peephole could fold a bool `bxor x, x`
to an integer constant and ICE the verifier, so boolean results now
fold to `bconst`.

### Four letters, each measured first

**`defer` runs at scope exit, not as the frames return** (D66, #193).
Every implementation already did: a `defer` in a loop body fires at
the end of each turn, and `[mem.shared.drop.1]` had implied it all
along, since a drop that runs "at scope exit, LIFO with defer" cannot
be LIFO with something frame-timed. `[mem.model.order]`'s frame
wording is amended and a corpus witness now pins the interleaving on
all three lanes.

**`\u{…}` takes one to six hex digits** (#189). The clause said so in
prose while its production said `HEX_DIGIT+`, and nothing said which
was normative. The lexer bounds it at six, so the prose was the
letter and the production amended. The bound is on the escape's
shape, not on the value it names: leading zeros count, and
`'\u{0000041}'` is refused before anything asks that it spells `A`.

**Two region diagnostics stopped lying** (#192). `W1001` claimed a
region "never allocates — delete the region" on blocks whose callees
allocate through it; the fact it read was the in-frame site list, and
D12 charges a callee's allocations to its caller's ambient, so the
advice was measured at +82 MB on the reporting program. It now
requires no call in the region's extent either. And `E1010` refused a
region block whose tail was a unit-typed raising call: an error union
is never `Copy`, so every raising call got a phantom ambient
allocation, which in tail position read as the block's value
outliving the region. That judgement now reads through the error row
the way the region-return judgement beside it already did — a row tag
carrying a real payload still allocates and still gets its site.

### The archive

v0.2.0's release workflow produced three of four tier-1 archives and
failed on Windows: `cargo xtask dist` hunted the unix staticlib name
on MSVC, the first tag since s59 added the guard. Fixed forward at
`10c2bf8` — the staticlib is named for its target, and the Windows
archive stages `wolf_rt.lib` against the day s60 teaches the driver to
link there. This tag is what proves it (#183).

## 0.2.0 — 2026-08-30

WOLFGANG TELLS THE TRUTH. v0.1.0 was an identity claim that shipped
the debug tier and named the rest not built. Nineteen days later the
rest is largely built, and this release's own contribution is honesty
about exactly that: `wolf --version` now names the build's commit and
never claims a release it is not (D57 — a build made exactly at the
`v0.2.0` tag prints the bare version; every other build answers
`0.2.0+dev.<commit>`), the observation record's `impl_version` carries
the same identity, and the release pairs with **lupin 0.1.17** as the
reference interpreter at pin `addcd7f` — the pin is part of this
release's identity (D57 again). Everything below landed on trunk
between the two tags, told by campaign.

### The release tier, and the M2 declaration

Campaign c09 made `wolf build --release` real: the backend emits
textual LLVM IR handed to the system clang — no llvm-sys, no inkwell,
zero new dependencies (D33) — with `--emit=llvm-ir`, dominator-scoped
GVN, check elimination, clustering with byte-identical output at 1, 4
and 8 threads, and PGO's instrumented loop (`.wprof`, `wolf profile
show|merge`, branch weights). The campaign also found and killed a
real miscompile: a false `!noalias` pair from region-freshened
inlining let LLVM delete a load; the fix made the role the aliasing
unit, at measured zero cost. The perf campaigns that followed (c23
range facts, c24 the six mechanisms, c29 the hot header, plus the a2,
a5, b3, d2 and fmt lanes) drove the thirteen-kernel T1 suite from
0.476x to over 1.0 — and on 2026-08-24, after three consecutive
nightly holds under `cargo xtask bench ritual` (geomean 1.099, 1.066,
1.080 vs naive `clang -O3`, two documented exceptions renounced by X3
and D25), **M2 was declared** (#89). The ritual is a program whose
refuted gate is a result, not a failure; its ledger is in the repo.

### The platform matrix

The compiler came home (s59, c13): the macOS/aarch64 native tier
lights — the Apple-arm64 C ABI (`wolf-abi-2`, differentially green
against Apple clang), Mach-O emission with dSYM and lldb parity, the
kqueue reactor, deterministic links, and all 21 linux-only test
headers flipped to runtime skips that distinguish environment from
breakage. s127 brought the release tier along: triple and datalayout
are host-derived from clang's own emission (never hand-composed),
and macOS holds three-tier parity at full linux floors. A nine-commit
Windows sweep then hardened what the s59 flip exposed: environment
refusals became exit 2 (so Windows stopped reading every native suite
as a program refusal), fixtures learned RFC 8089 file URLs, and a
real product bug fell — `wolf add`'s git fetch now pins line-ending
config, so an `autocrlf` host no longer manufactures a false E1506.
CI runs six jobs across linux x86-64, macOS aarch64 and windows
x86-64; linux/aarch64 waits on a runner (#166).

### The toolchain

`wolf_pkg` (c11) is D33's covenant as running code: declarative
manifests with no expressions anywhere, MVS resolution, blake3
content identity, capability audit where a dependency acquiring
`exec` fails CI before the ledger moves, the transparency log's
Merkle half, `wolf publish`, `wolf vendor`, `replace`/`exclude`, and
spec §8. The C importer (c10, s46) landed as a versioned interface
artifact produced by an out-of-process libclang worker — the compiler
binary links no C frontend, and the importer refuses by name. `wolf
doc` and script mode arrived with s53, whose rider created the
PAIRING file this release stamps.

### The claims become native

What v0.1.0 could only interpret or check now compiles. Concurrency
(c19): scope, spawn, channels and procs lower to machine code with
the defer law pinned on the release tier and a proc's argument record
copied at spawn (D14, D15, D16, D30). The io surface (c20): sixteen
fs builtins, sorted `read_dir`, and no false promise of atomic rename.
The crossing (c26) closed the checked-native split for builtins
entirely — net (with `error: timeout` firing for the first time on
any lane), JSON without an invented DOM surface, process control with
zombie discipline. Generics (c21) monomorphize on one worklist with
D8 dedup; dispatch (c22) landed trait methods, `call.ind`, and the
two-word trait object ABI, completing the story from D28 to D47; the
value tier (c25) made regions and closures values, with capturing
closures as an (entry, env) pair through `call.ind`. The consumer
findings campaign (c17) turned wolf-std's friction into fixes,
including the byte-view lend analysis and the first code retirement
(E1015 → W1004).

### c27 — small debts

Five reopenings of the same discipline: front-end debts paid without
inventing syntax. Raw-literal decoding, `main`'s legal shapes (E0414),
scoped bottom for literal `assert(false)`, nested error rows
flattening (D51), declared-row-first widening (D52), the numeric
literal chapter `[type.numlit]` with int-adopts-float and trapping
float→int casts (D54), value-preserving `wrapping[T] as int` (D56),
and `List[mod.Type]` reading an imported type in bracket position.

### c28 — the constant-time tier

`#[consttime]` is a verified contract (D53): secret-tainted data
cannot decide a branch, index memory, pick a call target, or feed a
variable-time instruction — refused fail-closed at the WIR rung on
every lane in every build mode, E1601–E1607, with spec/09 written
before the pass and assembly witnesses whose bodies contain zero
conditional branches.

### c29 — the hot header

Loop-carried header promotion, licensed only by existing proofs:
`_Wmain` on the b3 kernel gave back 8.1% of its instructions, ten
other kernels stayed byte-identical, and the campaign's real product
was a correction — the number it chased was an ops count misread as
an instruction count, and the instrument outvoted the narrative.

### c30 — signals

The program hears the signal: reception via a self-pipe trampoline,
`os_signal_listen/wait/raise` on every lane, and `os_random` over
getrandom/getentropy/BCryptGenRandom with no fallback and no seeding
— failure traps, honestly. The `[os.signal]` and `[os.random]`
clauses wrote the platform matrix down, which is what pre-authorized
the macOS crossings.

### c31 — clustering

The shim travels with its spawner: summary v3 records `func.addr` as
a reachability edge, so a spawn shim or closure entry can never be
split away from the body that references it.

### c32 — codegen debts

The loop and the layout: the versioning pass routes live-outs against
the current CFG, token linearity refined to the edge target, and List
element stride rounds up to the element's alignment — witnessed on
session-shaped and mixed-width layouts.

### c33 — strings

`chars()` landed on every lane, and then the scalar got its type:
`char` is a Unicode scalar value (D58) — four bytes, scalar-value
order, no arithmetic, `char as int` total and `int as char` trapping
on the surrogate gap and out of range — with `chars()` re-typed
`List[char]` and every caller migrated.

### c34 — the server

`wolf lsp` serves completion — keywords, scope names, typed members —
from a query that answers incomplete and broken buffers, measured
under the keystroke budget.

### c35 — the crashes

The compiler does not panic: str-match constant-folding builds no
orphan blocks, GVN scopes per continuation, and an integer literal
that cannot fit its type is E0415 at the front end, not a verifier
ICE. The one suspected oracle hole (#152) was ruled both-correct —
X3 has no hole at list-element provenance, only a width.

### c36 — the module explains itself

D59: directory = module by default, membership is the default and
standalone is a normative opt-out, E0301 explains both of its
situations, a silently unparseable sibling is E0202, and E0302 is
reachable from `wolf build`.

### c37 — the trap names its site

A trap's second stderr line says where (`at file:line:col`) while the
first line stays the byte-identical parsed ABI; per-site cold blocks
cost ~0.2% of .text; `[conf.trap.report]` and `[conf.trap.render]`
rule the shape, and D60 rules that the kind, not the exit status, is
the contract. The release cache learned that site coordinates are
part of a cluster's key.

### c38 — the origin

D61: the index chooses its origin — `#![index(0|1)]` as a lexical
marker, 1-mode coupling inclusive ranges, the shift landing as one
checked subtraction on both executing lanes, and zero cost when the
marker is absent: 422 files, zero verdict flips.

### c39 — the book writes

The walls the language showed its first real user came down in one
sprint: comma-grouped binders (D63), tuple destructuring with
element-wise moves live on three lanes, `str + str` as
interpolation-append (D62) with mixes still refused by name, and list
slices as fresh-List copies with `for` over a slice. The acceptance
was the human's own scratch programs running unmodified on all three
tiers, byte-identical to lupin.

### The spec, the corpus, and this release's own fix

The conformance machinery hardened alongside: lane coverage became a
gated ratchet, `cargo xtask peel` reads the fail-fast ledger, and the
anchor registry now holds 403 anchors — including `gram.lex.ident`,
which the spec-extract bracket scanner had silently dropped twice by
pairing a bare `[` literal in prose with the next anchor's `]`
(F-0100, #170/#177; fixed this release, with the s126 shebang prose
as the pinned regression witness). At the tag the lane-coverage
floors stand at checked 235 / native 257 / release 257 / union 271 /
all-three 221.

v0.1.0's notes said the release tier, macros and the registry were
not built. The release tier is built and declared against its gate;
the registry protocol is built with its hosted half still waiting
(X7); macros remain the CTFE tier's future. The corpus ledger, not
this document, stays the authority on completeness.
