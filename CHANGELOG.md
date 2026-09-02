# Changelog

## 0.2.3 — 2026-09-02

THE ARCHIVE RETURNS. **On Windows, `spawn` and scopes, `proc`, channels
and `select`, `sync`/`when`, `os.signal` and `net` deadlines now compile
and run.** v0.2.2 was the first archive that built and ran your program
on that host, and it refused twenty-one corpus rows by construct name —
"windows-native serves no `spawn`/scopes (the task layer) at the s60a
bring-up". That table is retired. The windows native lane measures at
macOS parity now — 261/278/0/295/0 (checked/native/release/union/
all-three), **zero rows refused by construct name** — and a stack
overflow inside a task reports in wolf's own voice,
`wolf-rt: stack overflow in task '<name>'`, exit **134**, where v0.2.2
died as `0xC00000FD` with no words at all. Two things this host still
does not serve, and says so by name: `wolf build --release` (the LLVM
tier is s60c's), and EXTERNAL `reload`/`upgrade` signal delivery, which
has no Windows analog — a program's own reload path works, and
wws-shaped programs use a control channel for the rest.

**And linux aarch64 has its archive back.** v0.2.2's release page
carried three archives, not four: the new learner-path smoke gated the
upload on a native tier that host does not serve, so the arm archive was
built and then thrown away (#213). The smoke reads the driver's
exit-code contract now — an exit-2 environment refusal is an unserved
host, not a broken archive — printing the refusal a learner would see
and then proving `wolf test corpus/hello.lu` runs from the unpacked
archive on the checked tier, which is how that host has served learners
since v0.2.1. Four archives at this tag. And the release page carries
this paragraph instead of a bare asset list, which is the third letter
below.

Underneath (s60b): workers are Win32 threads on the kernel's own
reserve-and-guard stacks — `CreateThread` with
`STACK_SIZE_PARAM_IS_A_RESERVATION` at `WOLF_TASK_STACK`, 8 MiB by
default: `VirtualAlloc(MEM_RESERVE)` plus the `PAGE_GUARD` page ntdll
walks down on first touch, done by the kernel rather than by hand. The
span is not ours to pool (Windows offers no thread on memory the runtime
mapped), so a worker keeps its stack for life, and idle trim is named
for s60c. `os.signal` rides `SetConsoleCtrlHandler`: Ctrl+C and the
console closing are `terminate`, Ctrl+Break is `quit` — the
`[os.signal.platform]` table — with `os_signal_raise` an in-process
loopback, because no self-targeted console event exists that would not
also reach every other process on the console. The reactor behind the
s35 interface is `WSAPoll`: `net` deadlines fire the `timeout` row,
accept/read/write park with the pool compensating, kill teardown reaches
them. Measured on the runner: 24 tasks parked on 200 ms read deadlines
resolved 24/24 `timeout` in 207 ms wall — the deadlines fire at the
deadline, not at the poll's cost. IOCP — completion ports, the async-fs
and many-socket rung — is s60c's, behind the same seam.
[`docs/platforms.md`](docs/platforms.md) is the per-host ledger.

It pairs with **lupin 0.1.23** at pin `8cda3aa` (D57: the pin is part of
this release's identity).

### The server annotates (s134)

`wolf lsp` serves `textDocument/signatureHelp`,
`textDocument/semanticTokens/full` and `/range`, and
`textDocument/inlayHint` — the three rungs s133's closeout named as
the binding table read three more ways. Nothing is a textual search:
**signature help** reads the checker's call record for the innermost
call whose argument list holds the cursor (`TypedBody::calls`, keyed
by the call expression's span) — the declared parameters as
`name: type` with a declared `mut`/`take` spelled, the receiver
omitted because the parentheses never spell it, the active parameter
counted by the commas before the cursor, the return type when the
callee is a declared item, and its `///` comment (the one doc model
hover and `wolf doc` read; markdown when the client lists it, plain
text otherwise); triggers on `(`, re-triggers on `,`. **Semantic
tokens** classify every identifier through what it bound to
(`Resolution::refs`, then `TypedBody::member_refs`): `parameter` when
the binder sits in a parameter list, `variable` otherwise (`readonly`
unless the binder is `var`'s), `function` / `type` / `variable` by an
item's kind, `namespace` for modules and std paths, `property` /
`enumMember` / `function` for fields, variants and methods, `keyword`
from the token kind, `type` for builtins and `Self`; a binder's own
token carries `declaration`; a name the compiler never bound gets no
token. The legend is closed and fixed: eight types in one order, two
modifiers. **Inlay hints** are the inferred type of an unascribed
`let`/`var` binder and the parameter name before a positional
argument that is not already that name — only at calls the checker
resolved to a declaration, so a fn-typed value and a prelude name
offer none; each class switches off through
`initializationOptions.inlayHints.{types, parameterNames}` and the
client's own toggle decides whether hints show at all. Positions
honor the negotiated encoding at every span (an astral character on
the line before a token moves its UTF-16 column, not its byte). No
delta tokens: a full answer is cheap here and a delta is a promise
about identity across edits this server has no reason to make; it
answers `-32601` by name like every other absence. `wolf_query`'s
contract moves to v5 (additive). Eighteen transcripts were recorded
in wolf-lsp against this build (one script per rung per maintained
client profile — fackr, facsimile, nvim, vscode, helix, emacs — the
answers differing by the profile's own declarations), the forty-seven
existing ones re-recorded with the initialize answer as their only
diff, and the unknown-method probe re-targeted at what is still
absent. Latency, s57's table before and after on the same machine:
`diagnostics-after-edit` p95 110.6 → 110.8 ms (p50 107.7 → 106.6),
hover p95 0.2 → 0.1 ms, cold first diagnostics p95 4.1 → 4.4 ms —
every class inside its budget, the number near perception unmoved.

### The proc leaves its module (s134 — #219 closes)

**A `spawn proc` in a non-entry module now builds on the release
tier under every partition.** lobo ws13 measured the gap while
adopting the region cap: its budget helper spawns a proc from a leaf
module, and `wolf build --release` answered “cannot compile this yet —
func.addr of `@work.run.task0.entry` outside this object's subset”
while `wolf run` executed the same program — #136's proc twin, one
partition over. s117's `refs=` edge keeps a spawner and its entry shim
in one CLUSTER; the per-module partition (`WOLF_MIDEND=0`, the
measurement mode lobo's gauntlet runs in while #146 is open) never
consulted it — the shim is synthetic, has no source file, and rode the
root module's object while the spawner sat in its own. The debug tier
had imported such a symbol across objects since #116; the LLVM tier
refused. The LLVM emitter now takes an out-of-subset referee's address
through the same mangled-symbol declaration a cross-object CALL uses —
a link-time constant in every object, resolved by the linker. Only a
name no module function carries is a refusal now. Witnesses:
`corpus/conc/proc_cross_module` (ws13's thirty-line reproducer, three
lanes plus lupin, `normal=0 breach=2`) and a driver test that pins the
per-module partition itself, because a thirty-line program is one
cluster under the whole-program phase and the refusal cannot fire
there. lobo's `ws13-cap` branch builds `--release` at this commit.

**A refusal names itself in the record.** `wolf conform-run --json`
answered `{"verdict":"unsupported","diagnostics":[]}` on every checked
proc spawn — by name on stderr, and nothing in the record a rig reads
over a pipe. The record now carries `x-unsupported-construct` and,
when the refusal has one, `x-unsupported-span` (`[proto.record.ext]`
extension keys, so they take no part in comparison and the
counterparty need not emit them) on every `unsupported` verdict at
every rung — typecheck, mem, wir, the checked machine, the native and
release lanes. `diagnostics` stays empty: an `unsupported` verdict
carries no partial diagnostics and a refusal is not a fault in the
program, so it has no E-code. `proc_cap_fault_join.lu`'s checked
verdict is UNCHANGED — `unsupported` at `mem`, now reading
`"x-unsupported-construct": "structured concurrency in checked
execution (C1 deferred)"` with the span of the spawn — because the
checked machine (`conform-run --checked`, the s23 UB machine) runs no
structured concurrency at all: `spawn`, scopes, `select` and `when`
are refused by name at the expression. A proc is refused where every
spawn is; running one there is the C1 sprint, not a fix. The other
`--checked` — `wolf run --checked` and `wolf build --checked` — is the
NATIVE build under the checked profile (the quarantine allocator and
the checked-tier runtime hooks), which is why ws13 saw the same file
run there and refuse under `conform-run`; the two flags name two
machines, and the record now says which one declined.

### The span is the offending token (s134 — D71, #220 closes)

**A parse refusal about a token now spans that token.** is34's first
full three-lane diff-run found DIV-2026-020: on eight grammar
witnesses wolfc's E0201 was a zero-width point at the offending
token's start while lupin spanned the token — the same byte, a
different width, invisible to every walk that compares codes and
visible to every editor, which highlights nothing at a zero-width
range. D71 ruled the width: "expected `}`, found identifier `y`"
points AT `y`, every byte of it. The parser's one primitive for a
refusal about the current token (`here()`) now answers the token's own
span, so E0201 and its siblings — E0203's "expected a struct name",
E0206's "expected a type", E0207's "expected a pattern", the missing
`=`/initializer reports — all moved together; a refusal at end of file
still lands on the zero-width `Eof` marker, and a suggestion's edit
keeps its own zero-width anchor (an insertion point IS zero-width; the
primary span and the fix's span were always different things). Seven of
the eight rows are now byte-identical to lupin's: `[550,551)` the `y`,
`[534,535)`, `[415,416)`, `[581,582)`, `[669,671)` the `..`,
`[332,333)`, `[896,897)`. The eighth, `let_group_bare_tuple.lu`, was
never a width question — wolfc reads the D63 let-group and refuses at
the end of the initializer list ("this value has no name", now
`[374,375)`), lupin refuses at the first comma (`[364,365)`) — and it
stays a locus divergence for its own triage, exactly as #220 said it
would.

Blast radius, measured before the change: wolf-lang, 37 snapshot files
(35 in `wolf_parse`, the LSP one-truth test's `[28,28]` → `[28,29]`,
and the eight `check: fail(E0201)` corpus pins unchanged — the walk
compares codes, and the directive grammar pins no spans); wolf-book, 5
diagnostic snapshots carrying 7 E0201 renderings whose carets widen
(read-only count — the book's lane re-records at its pin bump);
wolf-lsp, 0 transcripts (none carries an E0201; the two E0202s are at
the opener and the E0203 in the two smoke transcripts already spanned
its token). `cargo xtask differ` gains a **span-width class**: two
rejections with the same code at the same start byte that differ only
in width now classify as `SpanWidth` — a row that names itself — where
until now they were a `Diag` divergence spelled exactly like a wrong
locus, which is why is34 could only carry them as a waiver
(`differ::DIV_2026_020_FILES` in wolf-interp; it retires at the pin
bump that carries this, the interpreter lane's, as #177's did).

### The letters

**One archive per target, not a history** (#212). `cargo xtask dist`
runs in every gauntlet, and the archive, the staged tree and the
unpacked smoke tree it writes are all named for the version — so a
version bump renamed them and left the previous set behind forever. r05
measured the cost: five runs' worth plus the fuzz target dir took the
release rig to **0 bytes free** mid-release, which deadlocks a tool
harness that spools every output to a file first. `dist` now prunes
every prior artifact for its own target before it writes anything, and
names each removal, so a CI log can be audited. The second consequence
was worse than disk and closes with it: the release workflow uploads
`target/dist/*.tar.gz` by GLOB, so an archive left over from an older
version would have ridden a tag it did not belong to.

**The release body fills itself** (#214). `GET
/repos/wolffe-lang/wolf-lang/releases/tags/v0.2.2` answered with a
`body` of length **0**, and so did v0.2.1 — the workflow created the
release with `--title` and nothing else, so the paragraph written for a
newcomer lived only in this file, and a learner arriving from a download
link never read it. `cargo xtask release-notes <TAG>` cuts the entry for
the tag with fenced blocks transparent (the 0.2.2 entry quotes a
compiler refusal inside a fence, and a naive cut would have ended the
body two paragraphs in), and the release job passes it to `gh release
create --notes-file` and then to `gh release edit`, so the body is right
whether the workflow opened the release or someone else did. A tag with
no entry here fails the job by name. This page is the proof.

**The multiline, raw and generalized literals have productions** (#215).
`literal` named `MULTILINE_STRING`, `RAW_STRING` and
`GENERALIZED_STRING` on one line and defined none of them; `STR_TEXT`
and `CHAR_TEXT` were cited by the productions above them and defined
nowhere. A reader working from `spec/grammar.ebnf` alone derived nothing
at all for three of the six literal forms — the same class as #198, one
literal over. It bit le05, which wired an `invalid_escape` node for
tree-sitter-wolf and had to decide whether it belongs inside a `"""`
string: the productions could not answer, so the answer had to be read
off a CONTRAST between three prose bullets, two of which say "no
escapes" outright and the third of which says nothing about them. The
lexer was measured and the productions written from it —
`MULTI_PART ::= MULTI_TEXT | STR_ESC | '{{' | '}}' | INTERP`, the same
alternatives `STR_PART` has, because one routine scans both bodies. So
escapes and interpolation work inside a multiline, and that is a
derivation now rather than a silence. `RAW_TEXT` and `GEN_TEXT` derive
scalars and nothing else, which is where "no escapes, no interpolation"
is read from instead of the sentence beside them. Two corpus witnesses
make it measured: the escapes running inside a multiline, and the E0101
refusal of an unknown escape there — the first corpus entry anywhere to
pin one, which is how it turned up **#225**, a code collision 484
differential entries had never shown: lupin refuses a bad escape under
the code the catalogue spends on a multiline's opening line, in plain
strings and multilines alike.

**The pairing moved to lupin 0.1.23.** The sibling released while s60b
and s134 ran, which is why both waves carried `LUPIN=` overrides. The
ritual differ run over 484 corpus files found nothing new — checked 257
agreements / 2 soundness / 8 hard, native 278 / 0 / 5, every hard row a
standing named one (#167's warning-channel asymmetry, #168's float-cast
twins) — and one class GONE: not a single `SpanWidth` row, because
s134's D71 work made those seven E0201 spans byte-identical and this is
the pin bump at which the interpreter retires its waiver.

## 0.2.2 — 2026-09-02

THE LEARNERS' RELEASE. **On Windows, this is the first archive that
compiles and runs your program.** Unpack it, keep `wolf.exe` and
`wolf_rt.lib` together, and `wolf run hello.lu` produces a real
`hello.exe` and executes it — the native tier, on the host, not an
interpreter. One thing has to be installed beside it: **Visual Studio
Build Tools, "Desktop development with C++"**. The import libraries
every Windows link needs — `kernel32.lib`, `ws2_32.lib`, the UCRT,
`msvcrt.lib` — ship with the Windows SDK and the MSVC toolset, not with
Windows, and this is the same requirement Rust's own `windows-msvc`
toolchain carries; bundling them so no install is needed at all is
s47's, and the refusal that asks for them says so. With them present
wolf finds a linker by itself — `WOLF_LINKER`, then `lld-link`, then
rustup's bundled `rust-lld` (a learner with a Rust toolchain already
has one), then MSVC `link.exe` — and `wolf build --verbose` names the
choice.

What the bring-up does not serve, it refuses **by name**, before the
link, in these words:

```text
wolf build: cannot compile this yet — windows-native serves no
<construct> at the s60a bring-up (the runtime's task layer, io
reactor, and signal delivery are s60b's — the IOCP road);
`<symbol>` would not link (pipeline is honest through `wir`; the
conservatism ledger, not a bug in your program)
```

Twenty-one rows of the corpus take that refusal: the task layer
(`spawn` and scopes, `proc`, channels and `select`, `sync`/`when`,
`region_transfer`), `os.signal`, and `net` deadlines. `wolf build
--release` refuses in the same voice. Everything else serves — `print`,
strings, lists, json, `fs`, `os` (env, cwd, exe, exit, child
processes), `time`, `random`, regions and the region ledger, `net` in
its documented blocking posture, the C membrane for scalars and
pointers — and a trapping program prints `wolf-trap: <kind>` with its
site line and exits **134**, the same number as every other native
host. Nothing is a silent stub and nothing is a link error.
[`docs/platforms.md`](docs/platforms.md) is the per-host ledger and
names s60b and s60c as the road.

Underneath (s60a): cranelift emits COFF objects under the MSVC x64
convention, the driver links them against `wolf_rt.lib` (shipped in
every windows archive since v0.2.1) and the import libraries a Rust
staticlib needs, and the C runtime's console entry calls wolf's `main`
shim. Five linker rungs were proven — one of them turned up a real
`link.exe` bug with wolf's DWARF SECREL relocations. The 134 is not
signal arithmetic here: a trap is a call into the runtime ending in
`ExitProcess(134)` (D70), which is why no vectored handler is needed —
no sited trap is a fault at any tier. `[abi.c.targets]` gains the win64
bring-up contract: scalars and pointers direct, aggregates by value
refused by shape until the campaign's `cl.exe` differential.
`cargo xtask lane-coverage` measures windows on its OWN floor line —
259/255/0/274/0 (checked/native/release/union/all-three), checked at
full parity, native the macOS count minus the 21 refused rows, release
dark because that host's floor says so — and `cargo xtask dist` now
unpacks its own archive and builds and runs `corpus/hello.lu` from it
on every host: the learner's path, mechanized, in standing CI.

It pairs with **lupin 0.1.22** at pin `2bfbe5e` (D57: the pin is part
of this release's identity).

### The server navigates (s133 — #208 closes)

`wolf lsp` serves `textDocument/definition`, `textDocument/references`
and `textDocument/rename` (with `prepareRename`) — the three rungs s122
named and nobody had climbed, so F12 / Shift+F12 / F2 on a `.lu` file
did nothing in every editor. All three answer from a **binding table**,
never a textual search: the resolver keeps the decision it already
makes for every name (`Resolution::refs` — uses and binders alike, a
binder a ref to itself, an import's bound name a ref to what it
imports), and the checker keeps the type-dependent half
(`TypedBody::member_refs`: fields through `.`, literal and pattern
fields, variants in value/call/pattern position, methods and associated
fns). The two halves share one key — the declaration's name token span,
`FileId` included — so a cross-file answer is a lookup over the package
graph, not a scan. A name the compiler never bound answers `null`,
never a guess, and the lexical half keeps answering when typing
stopped.

The wire shapes follow the client's declarations, read once at
`initialize`: `LocationLink[]` with `originSelectionRange` when it
declares `linkSupport`, `Location[]` otherwise; a rename's
`WorkspaceEdit` as `documentChanges` when it declares them, the
`changes` map otherwise. References come back in (file, offset) order,
the declaration only when `includeDeclaration` asks, and the negotiated
position encoding holds at every span (the utf-16 astral case is
pinned). Rename refuses BY NAME — `RequestFailed` (-32803) naming the
token and the reason, never a partial edit — on keywords (`self` and
`Self` included), builtin types, prelude names, std and `import c`
symbols, modules, and a new name that is not a single identifier;
`prepareRename` refuses with the same reasons before the box opens. The
D59 `//!` member marker carries no identifier, so no rename ever
touches one. `wolf_query`'s contract moves to v4 (additive). Six client
profiles were transcribed against this build. Latency, measured before
and after on the same machine: `diagnostics-after-edit` p95 105.7 →
105.7 ms — the one number near perception is unchanged — and the three
new requests answer at ≈0.03 ms p95 in-process. Known residue, named:
the reachable set is the package around the ENTRY (the v0 single-entry
model); a workspace-root model is s57's.

### The region answers, and holds (s131, s132, D68 — #187 closes)

Region accounting became readable and then became a contract. Three
tiers gained `region_bytes(r)` — a named region's byte ledger, the
count `wolf_rt` has kept since s76, now surfaced — and
`live_region_bytes()`, the process-wide live total, with
`[mem.region.account]` pinning what every tier guarantees (zero at
creation, monotone within the lifetime, stable between allocations,
wholesale disappearance at free) and leaving the units as per-tier
measured facts.

Then the cap. A region takes a creation-time byte budget — `region
r(cap: n) { … }`, or `region(cap: n)` on the value form — and a charge
that would take its ledger PAST the budget is the deterministic trap
`alloc-contract` at the allocating site: the existing kind, no new trap
vocabulary, because a byte budget is an allocation contract. At-cap
exactly is not a breach; the next byte is; `cap: 0` is legal and
everything breaches it. Budgets are denominated in LEDGER units, not
payload bytes — the clause now says so plainly, after #203 measured a
64 KiB io chunk at ~1 MiB of ledger — so portable programs derive
budgets from measured `region_bytes` readings, which is exactly how the
witnesses pin the boundary on all three tiers at once.

D68's point is the fault half: a breach **inside a proc** no longer
kills the process. The trap is contained at the proc boundary — the
proc dies by the killed-proc sequence, so no `defer` below the boundary
runs — and `[conc.proc.exit]`'s closed reason set gains the mapping,
`fault(kind)` read at the join as `is_fault()` and
`is_alloc_contract()`. Teardown is free-then-deliver, measured and then
pinned: at the join, `live_region_bytes()` has already returned to its
pre-spawn reading, so the supervisor that answers 503 on this reason
was handed the memory back first. Containment is the no-unwinding law
made mechanism; the trapping worker parks, and its thread and stack
high-water are the measured per-breach cost.

### The comma insists everywhere (s131, s132 — D67, D69)

The separating comma is now required wherever the productions always
said it was, family by family and each with its blast radius measured
BEFORE the tightening. D67 took the pattern family: `Point { x .. }`,
`Point { x y }` and `(a b)` refuse at E0201 with a machine-applicable
"add the comma" fix (`wolf fix --apply` produces the canonical
spelling), and `..` follows a separator like one more member. D69 took
the rest of the unlicensed laxity: struct LITERAL fields (`Point { x: 7
y: 2 }`, and the newline-separated spelling lupin always refused),
closure parameters (`fn(a b)`), and inline-C capture lists (`unsafe c
[a b]`).

Blast radius across the world, measured: zero working code. The
wolf-lang corpus and fixtures (532 files), wolf-book (1,129), wolf-std
(887), wolf-web (1,976) and lobo (1,395) already write the comma; the
sole flagged file anywhere is a fuzz-minimized broken-input formatter
fixture, whose idempotence test still holds. The multi-line literal's
trailing layout is untouched — a terminator run before `}` is the
production's own — and the separator report latches once per list and
never into a reported wreck. Only lupin and the spec's letter were ever
this strict; the compiler now agrees with both.

Two more measurements rode along: `defer` runs at scope exit, not as
the frames return (D66/#193, now with a corpus witness pinning the
loop-turn interleaving on all three lanes), and two or-pattern
divergences got witnesses — an or-pattern OVER product alternatives
refuses on every wolfc lane, one INSIDE a product refuses natively
while the checked executor runs it; lupin runs both. Permissive-
direction divergences that were invisible until a file put them in the
differ's ledger (#196).

### The letters

**A bare entry name means `.`** (#206). `wolf conform-run hello.lu`
answered "the package root has no wolf source files" where
`./hello.lu` ran the program — `Path::parent()` on a bare relative name
is the EMPTY path, not `None`, and the anchoring that fixed it lived in
exactly one CLI parser, which is why `build`, `run` and `fmt` worked
and `conform-run`, `test`, `interface` and `doc` did not. It lives in
the loader now, root and entry anchored together so a headered entry
stays its own module's entry. One consequence is worth stating, because
it is visible in the machine record: `conform-run` reads its argument
through the same anchoring before interning it, so the record's `file`
field carries the anchored spelling and `wolf conform-run hello.lu` and
`wolf conform-run ./hello.lu` now produce BYTE-IDENTICAL records rather
than two `FileId`s for one file — identical programs, identical
records, whatever the command line typed. On Windows before this
release that one missing `./` stood between a learner and their first
program, because `conform-run --checked` was the only way to run one.

**`STR_PART` derives escapes** (#198). v0.2.1 bounded `\u{…}` at one to
six hex digits and said in prose that the bound "binds in string
literals too" — while the production derived no escape at all, so a
reader working from `spec/grammar.ebnf` got the bound for `'…'` and
nothing whatsoever for `"…"`. `STR_ESC` now carries the escape set,
`UNI_ESC` sits beside it, and `CHAR_ESC ::= STR_ESC | '\' "'"` — so
"the char set is the string set plus `\'`" is read off the productions
instead of asserted next to them. Two corpus witnesses make the string
half measured rather than assumed: the seven-digit refusal (shape, not
value — `0x0000041` IS `A`, and it is refused before anything asks) and
its in-bounds twin at one, four and six digits.

**A trap runs no defers, at the root too** (#209). `[conf.trap.exit]`
ruled the proc path and was silent about the root, so nothing pinned
whether a root-domain trap flushed its pending defers. The consistent
reading is written now — a trap is not an error value and runs no
`defer` or `errdefer` anywhere; at the root death is immediate — with
`faults/trap_skips_root_defers.lu` as the witness. It records a
measured divergence for the interpreter's next sprint: every wolfc lane
abandons the pending root defer, lupin 0.1.22 runs it. The differ could
never have found this on its own — on a trapping program the
interpreter's record carries no stdout, so the two machines are
verdict-identical whatever they print.

**Two nondeterministic verdicts retired.** The net refusal probes
dialed a just-released EPHEMERAL port and bet that nothing took it in
between; under `cargo test`'s full parallelism that bet lost, and it
reddened a trunk gauntlet while passing 3/3 in isolation (#205). They
dial a port from outside the host's ephemeral range now — one nobody's
`bind(0)` can be handed — so one dial is the whole story. And the
bare-entry suite's linking rung builds the runtime staticlib on demand
and skips loudly where a host cannot link, instead of reading an absent
toolchain as a regression.

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
