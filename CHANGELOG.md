# Changelog

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
