# The lived lint corpus — s68 mining triage

Every recorded case of "wolf let someone write something legal that was
wrong, confusing, or unidiomatic" from every mandated source, each with
a verdict. **Zero unexamined entries** is the acceptance bar; the
per-source coverage receipts are at each section head, the totals at
the bottom. Sources mined 2026-08-11 (the mining mandate: lints come
from what actually bit people, not from imagination).

Verdicts:

- **LINT → W####** — shipped in the first wave (registered, fixtured,
  `corpus/lints/` witness).
- **LINT (deferred)** — accepted as lint territory, held past the
  first wave; the rationale names what it waits on.
- **PROMOTE** — should be (or already is being) an error, not a
  warning; routed, never silently dropped.
- **ALREADY-ERROR** — the hazard closed as a hard error before s68;
  recorded so the class is never re-mined.
- **DECLINE** — not lint material, with the reason.

Analysis tags: **syn** (syntax/lexical), **name** (needs resolution),
**type**, **mem**, **conc**, **meta** (process/rig/spec — no program
shape). `syn`/`name` rows are shared-analysis (lupin can implement);
`type`/`mem`/`conc` rows are compiler-only until lupin grows the
analysis (honest-absent under `[proto.record.warn]`).

## The first wave (15 lints, all shipped)

| Code | Shape | Scar rows | Analysis | Fix-it |
|---|---|---|---|---|
| W0304 | declaration shadows a prelude/built-in name | STD L-04/L-19/F-0009, lang#9, INT B20 | name | none (rename) |
| W0305 | row tag shares a name with an item/import/prelude name | STD F-0036/SE-06/T-R6/T-T3, lang#30 | name | none (rename) |
| W0306 | bare prefix-operator statement (broken continuation) | STD G-03 | syn | none |
| W0307 | comparison binds to the `else` fallback | STD L-10 | syn | parenthesize (Maybe) |
| W0308 | `mut` argument inside a string interpolation | STD L-16/F-0013 | syn | none (hoist) |
| W0309 | interpolation-shaped braces in a raw literal | STD G-05/L-53, BOOK ch02:425 | syn | none (drop the `r`) |
| W0401 | literal outside the cast target's range | INT B19a/B16/B21, STD F-0025 | type | none (widen/`wrapping`) |
| W0402 | `0.0 - x` as negation (loses `-0.0`) | STD L-38 | type | `-x` (Maybe) |
| W0601 | fallible `!T` result discarded by a statement | BOOK ch01:476, STD G-15 | type | none (`?`/`else` change meaning) |
| W0602 | anonymous multi-tag row on a `pub` signature | INT D2 (s15's recorded lean), C23 | syn | none (no alias surface yet — lang#36) |
| W0801 | capitalized bare pattern name binds instead of matching | INT B17, STD F-0007, interp#5, BOOK ch06:767 | type | none (guard or lowercase) |
| W1001 | region that provably never allocates | contract's free-`region()` smell; INT B7-adjacent | mem | none (delete/move) |
| W1101 | write to a captured copy inside a `spawn` closure | INT S-10/C3/B37/A24, interp#4 | syn | none (channel/join) |
| W1102 | closure captured a `var` assigned after creation | BOOK ch04:447 | syn | none (reorder) |
| W1302 | `assume noalias` operand reassigned afterwards | INT B30 (AC §7.8) | syn | none (re-state after last write) |

Plus one lint on ourselves: the **catalog-text test**
(`wolf_diag::registry::tests::explanations_carry_no_internal_identifiers`)
— no sprint/campaign identifiers and no nonexistent commands in
`--explain` prose (BOOK ch07:950, ch08:1126, ch09:980/992/1001,
lang#39; five explanation texts were cleaned when it first ran).

---

## Source 1 — wolf-std (`AN_AGENTS_GUIDE_TO_WRITING_GOOD_WOLF.md`, `docs/findings.md`, `docs/error-taxonomy.md`)

Coverage: every GUIDE section bullet and sharp edge (G-01..G-36,
SE-01..SE-12), every dated learning (L-01..L-72, split letters where
one entry carried two hazards), every finding F-0001..F-0042 plus the
retirement blocks (FR-01..FR-06), every TAXONOMY measurement, tag row,
and rule (T-*). 132 entries, 132 examined.

### GUIDE — the shape of the language / modes / memory / errors / arithmetic / traits / concurrency / tools

| ID | Hazard | Verdict | Rationale | Tag |
|---|---|---|---|---|
| G-01 | lupin accepts `let` reassignment | ALREADY-ERROR | E0410 both sides since DIV-2026-010 closed | syn |
| G-02 | read of moved-from binding | ALREADY-ERROR | E1001/trap | mem |
| G-03 | operator-led line silently becomes a second statement when the operator can be unary | **LINT → W0306** | the legal sibling of E0001 | syn |
| G-04 | turbofish | DECLINE | parse error already | syn |
| G-05 | `{}`/`{x}` brace confusion in literals | **LINT → W0309** (raw-literal half) | empty-brace half declined: `{}` in a cooked literal is already E-diagnosed | syn |
| G-06 | `"""` closing-delimiter column silently re-strips content | LINT (deferred) | real but needs multiline-literal shape analysis; no recorded bite yet — wait for one | syn |
| G-07 | byte-offset slicing used as char indexing | LINT (deferred) | the F-0018 provenance cluster below | type |
| G-08 | omitted `mut` on a param meant to be mutated | DECLINE | callee body then fails to check; absence-is-syntax is the design | type |
| G-09 | bare mut-self receiver | ALREADY-ERROR | E0804/E1007 both sides since sc06 | type |
| G-10 | whole-value `mut self` where a view set would do | LINT (deferred) | s64's perf-contract territory (non-target here) | mem |
| G-11 | region annotations reproducing inference | LINT (deferred) | needs written-vs-inferred placement diff; no bite recorded | mem |
| G-12 | (syntax inventory) | DECLINE | no hazard | — |
| G-13 | region value used after move (trap-only) | LINT (deferred) | straight-line half only; E1001 machinery owns the rest | mem |
| G-14 | frozen-write under-enforced dynamically | PROMOTE | interp parity (interp#2 closed; residue is INT B11) | mem |
| G-15 | `upgrade`'s absence row discarded | **LINT → W0601** | the unhandled-row class | type |
| G-16 | pool handle deref after remove | LINT (deferred) | spec deliberately declines a static check; same-body heuristic is future work | mem |
| G-17 | multiopen ancestor pair | ALREADY-ERROR | E1011 | mem |
| G-18 | private inferred row widens silently until `pub` day | LINT (deferred) | E0605 guards the pub edge; the private-growth half needs row-history | type |
| G-19 | missing tags at `?` | ALREADY-ERROR | row diagnostic names the tags | type |
| G-20 | `else` forms | DECLINE | covered by L-10/L-59 rows | — |
| G-21 | `defer` cleanup in killable procs | LINT (deferred) | needs an effect notion (INT B38); refile s69/s64 | conc |
| G-22 | hand-rolled exception emulation | DECLINE | architecture; no crisp shape | type |
| G-23 | `wrapping` silencing a real overflow | DECLINE | intent not decidable; taught in the book instead | type |
| G-24 | inherent method silently beats in-scope trait method | LINT (deferred) | resolution data exists; no recorded bite — wait for one | type |
| G-25 | generic body relying on unbounded capability | PROMOTE | E0501 golden rule; lupin parity (interp side) | type |
| G-26 | `distinct` round-tripped so freely it separates nothing | DECLINE | explicit casts are the design | type |
| G-27 | comptime reaching for ambient IO | ALREADY-ERROR | sandbox refusals | type |
| G-28 | (scope/spawn inventory) | DECLINE | no hazard | — |
| G-29 | task writes captured enclosing `var` | **LINT → W1101** (static half) + PROMOTE (dynamic parity is interp's) | E1101 unimplemented; the lint is the shape short of it | conc |
| G-30 | donor region touched after `send(move r)` | LINT (deferred) | same family as G-13 | mem |
| G-31 | nested `when` | ALREADY-ERROR | E1103 | syn |
| G-32 | seed-dependent program | DECLINE | explorer territory, not static | conc |
| G-33..G-36 | tool inventory / fmt law / honesty rule | DECLINE | process, not code | meta |

### GUIDE — current sharp edges

| ID | Hazard | Verdict | Rationale | Tag |
|---|---|---|---|---|
| SE-01 | bare `use` binds a package dir where std was meant | LINT (deferred) | interim retired upstream (F-0010); revisit if it re-bites | name |
| SE-02 | bare-receiver container mutation | ALREADY-ERROR | E0804 with fix-it | type |
| SE-03 | hand-rolled `if !cond { fail(msg) }` post-intrinsic | DECLINE | works, retires naturally; std's conventions rig owns it | syn |
| SE-04 | guessed-offset string scans | LINT (deferred) | F-0018 cluster | type |
| SE-05 | tag spelling convention (CapCase marks) | DECLINE | taxonomy rule without a D-number; owned by wolf-std's `--lint-conventions` rig; s69 may promote | syn |
| SE-06 | tag/value name collision rides out silently | **LINT → W0305** | the compiler resolves tag-first now; the double reading remains | name |
| SE-07 | enum through an error row always misses | PROMOTE | representation bug (interp#16) — fix, not lint; signature-shape interim declined as noise once fixed | type |
| SE-08 | module identity = last path segment, silent drop | ALREADY-ERROR (wolfc E0306) + PROMOTE (lupin) | lang#29 | name |
| SE-09..SE-12 | NotYet ceilings / undecided op-set / missing os tier | DECLINE | honest refusals or policy | — |

### GUIDE — accumulated learnings (dated)

| ID | Hazard | Verdict | Rationale | Tag |
|---|---|---|---|---|
| L-01a | bare-ident patterns bound, first arm always matched | ALREADY-ERROR + **LINT → W0801** (the binder residue) | resolution is type-directed now; the capitalized binder that still binds is the wave's lint | type |
| L-01b | one enum `==` poisons every importer's lane | LINT (deferred) | module-budget lint (L-63) — needs lane-cost model; s69 | type |
| L-02/L-03 | rows second-class / lowercase tags unresolved | DECLINE | superseded — legal now | — |
| L-04 | `fn assert` severs the module from the trap | **LINT → W0304** | promote later if the ruling hardens; warn ships now | name |
| L-05 | imported module typechecked as freight poisons importers | DECLINE | architecture; actionable half is L-63 | type |
| L-06 | bare literal defaults `i32`, misses `impl … for int` | LINT (deferred) | E0502 already fires; annotation hint needs bound context | type |
| L-07 | no legal `push` spelling | DECLINE | retired sc03 | type |
| L-08 | capability map | DECLINE | informational | — |
| L-09a | `float` resolves on one lane only | DECLINE | E0301 in wolfc already; parity chore | syn |
| L-09b | float div-by-zero is IEEE, not a trap | LINT (deferred) | needs guarded-value analysis; book teaches it (bs04) | type |
| L-09c | `0 - x` cargo-cult negation | **LINT → W0402** (float half) | int half is harmless | syn |
| L-10 | `e else d == x` compares the fallback only | **LINT → W0307** | precedence scar with an exact parenthesization | syn |
| L-11/L-12 | unused import / use-list | ALREADY-ERROR / DECLINE | E0305 is a deliberate hard error (see INT A30) | name |
| L-13 | single-read rule bodies | DECLINE | rule dead | mem |
| L-14 | explicit instantiation costs a lane | DECLINE | wolfc refuses honestly (NotYet); not legal-but-wrong | syn |
| L-15 | `struct X[T]` | DECLINE | parse error | syn |
| L-16 | `mut` argument inside an f-string | **LINT → W0308** | the interp bug retired; the hidden write remains | syn |
| L-17 | module-boundary ceilings | DECLINE | honest refusals | type |
| L-18a | absent Map key reads `()` | PROMOTE | soundness (std surface); lang-side typing owns it | type |
| L-18b | `m[k] += 1` | DECLINE | refused everywhere | type |
| L-19 | prelude shadowing is silent | **LINT → W0304** | the fixed-list check; provisional stand-ins exempt | name |
| L-20 | fmt splits dotted call under a comment | DECLINE | formatter defect (lang#14) — fix the tool, don't lint around it | syn |
| L-21 | unused import / duplicate main | ALREADY-ERROR | E0305/E0302 | name |
| L-22 | frozen-write dynamic gap | PROMOTE | dup of G-14 | mem |
| L-23/L-24 | slicing at computed offsets traps on legal input | LINT (deferred) | F-0018 provenance cluster: highest-severity legal-but-wrong class; needs offset-provenance analysis and the byte type — first candidate for wave two | type |
| L-25 | `words`/`trim` are Unicode, tail-dropping | DECLINE | builtin contract surprise; docs own it | type |
| L-26 | `a + b` on str diverges | ALREADY-ERROR | E0409 in wolfc; parity chore | type |
| L-27 | slice-of-binding method call | DECLINE | retired sc05 | type |
| L-28 | tuples work | DECLINE | no hazard | — |
| L-29 | mixed int/float literals | ALREADY-ERROR | E0401 | type |
| L-30 | lazy tag resolution certifies cold raises | PROMOTE | retired — eager now; rule recorded | name |
| L-31 | tag-spelling interim | DECLINE | retired | — |
| L-32 | module named like a builtin type | **LINT → W0304** (builtin-type half) | `str.len(s)` vs `s: str` in one file | name |
| L-33 | `for c in "abc"` refused | DECLINE | honest refusal (D25) | type |
| L-34 | sibling-file rig nuance | DECLINE | rig | — |
| L-35 | same-named fns in two modules break the native lane | DECLINE | mangling bug (F-0026) — fix the compiler, not the program | name |
| L-36 | unannotated `var` accumulator is `i32` | DECLINE (compiler-side) | wolfc *infers* from later assignments; the scar is lupin's defaulting — filed there (interp#14 closed) | type |
| L-37 | `!=` on f64 lane-dependent | DECLINE | the codegen bug (lang#22) is FIXED; `x != x` is the idiomatic NaN probe and must not warn | type |
| L-38 | `0.0 - x` erases `-0.0` | **LINT → W0402** | wrong library results nothing but a zero can catch | type |
| L-39 | one-ulp constants | DECLINE | needs an external oracle (ulp harness) | type |
| L-40 | bit-by-bit float building | DECLINE | retired workaround | type |
| L-41 | exported enum importers cannot inspect | LINT (deferred) | real API lint; wait for the F-0029 refusals to settle | type |
| L-42 | module `const` costs a lane | DECLINE | lane-cost portability, not language hazard; retires with codegen coverage | syn |
| L-43 | bitops on `int` cost the checked lane | DECLINE | same class as L-42 | type |
| L-44 | rebuilding `swap` in a loop is quadratic | LINT (deferred) | s64's perf-contract territory (non-target) | type |
| L-45 | doc claims no test can witness | DECLINE | review matter | — |
| L-46 | range accessors missing | DECLINE | F-0030 owns it | type |
| L-47/L-48/L-49/L-50 | tag=value namespace / enum-through-row / last-segment identity / unknown cast target | PROMOTE | all four are interp-parity errors (wolfc already rejects or resolves; see F-0036/F-0037/F-0034/F-0032) — W0305 covers the surviving double-reading | name/type |
| L-51 | Map `for` yields the pair, not the key | LINT (deferred) | needs the std Map surface to settle (s37) | type |
| L-52 | text heuristics matching inside literals | DECLINE | rig lesson | syn |
| L-53 | `{{`/`}}` escapes vs literal braces | **LINT → W0309** (raw half) | cooked-literal half already diagnosed | syn |
| L-54 | `var bits = 0` then `bits = to_bits(x)` traps far away | DECLINE (compiler-side) | wolfc's inference retypes; lupin scar, filed | type |
| L-55/L-56 | exact-decimal printing / differential testing | DECLINE | technique, not shape | — |
| L-57 | copy/skip of unrecognized text | LINT (deferred) | F-0018 cluster | type |
| L-58 | row value has no literal | DECLINE | honest refusal (F-0038) | name |
| L-59 | `else \|e\|` binds the tag, not the payload | DECLINE | both mistakes are errors today; the ask is fix-it wording on E-codes | type |
| L-60 | nested rows split the implementations | DECLINE | wolfc refuses at parse (loud); grammar decision owns it (lang#34) | syn |
| L-61 | unreachable dummy after a diverging call | LINT (deferred) | wants the `never` type first (F-0040) | type |
| L-62 | enum error payloads poison importers | LINT (deferred) | same F-0029 dependency as L-41 | type |
| L-63 | one body downgrades a shared module's lane | LINT (deferred) | the module-budget lint; needs a lane-cost model — s69 material | type |
| L-64 | `Type.method(recv)` is variant lookup | ALREADY-ERROR | E0403; wording chore | type |
| L-65 | bare `==` on unbounded generic | PROMOTE | E0501 exists; lupin parity | type |
| L-66 | `for x in xs` moves under the mem tier | LINT (deferred) | S-11-adjacent; see INT A13 | mem |
| L-67 | `var k = 0` is `i32` by documented rule | DECLINE (compiler-side) | as L-36/L-54 | type |
| L-68 | omitted call-site `mut` dropped writeback | ALREADY-ERROR | E1007 both sides | type |
| L-69 | builtin `pop()` traps where std's raises | LINT (deferred) | needs the std surface split to settle (s37) | type |
| L-70 | conventions written and not followed | DECLINE | the sprint's own justification (precedent, not target) | meta |
| L-71/L-72 | fmt partial-format / fmt normalizations | DECLINE | W0301 exists; formatter is law | syn |

### findings.md (F-0001..F-0042 + retirement blocks)

| ID | Hazard | Verdict | Rationale | Tag |
|---|---|---|---|---|
| F-0001..F-0003 | std path / second-class rows / lowercase tags | DECLINE | retired; loud failures throughout | — |
| F-0004 | `<=>` result compared to `-1` as an int | LINT (deferred) | intent mismatch under the future operator-trait bridge (lang#5); wait for the bridge | type |
| F-0005/F-0006 | push spelling / str ordering | DECLINE | retired | type |
| F-0007 | binding patterns + non-exhaustive matches ran | PROMOTE + **LINT → W0801** | promotion done upstream; W0801 is the binder residue | name |
| F-0008 | iterator protocol | DECLINE | closed | type |
| F-0009 | `assert` shadowing severs the trap | **LINT → W0304** | see L-04 | name |
| F-0010 | lupin std root | DECLINE | retired; "an interim that works is an interim you cannot see" noted at SE-01 | name |
| F-0011 | absent Map key reads `()` | PROMOTE | see L-18a | type |
| F-0012 | untouched trait/enum poisons importers | DECLINE | honest ceiling (lang#12) | type |
| F-0013 | false `ub(mem.ub)` positives | DECLINE | oracle bug, retired (interp#7); L-16's readability lint survives | mem |
| F-0014 | mutate-while-iterating diverges three ways | LINT (gated on S-11) | see INT A13 — ruling still open; fixture exists upstream, implementation held | mem |
| F-0015 | miss paths unsupported | DECLINE | ceiling | type |
| F-0016 | fmt comment-split | DECLINE | tool defect (lang#14) | syn |
| F-0017 | `let` reassignment ran | ALREADY-ERROR | E0410 | syn |
| F-0018 | offset-guessing scans trap on legal input | LINT (deferred) | the flagship deferred cluster (with L-23/L-24/L-57/F-0035): needs offset-provenance analysis; wave-two lead | type |
| F-0019 | ASCII-documented, Unicode-behaving helpers | DECLINE | pinned agreement tests own it | type |
| F-0020 | two-arg assert trapped on pass | DECLINE | compiler bug, fixed | type |
| F-0021/F-0022 | slice-of-binding / non-converting cast | DECLINE | retired | type |
| F-0023 | lazy tag resolution false-certifies | PROMOTE | retired — eager now | name |
| F-0024 | red trunk at a green pin | DECLINE | CI hygiene | — |
| F-0025 | INT_MIN unwritable / `var` is i32 / cross-module operators | DECLINE (wolfc) + **LINT → W0401** (the narrowing-cast face) | wolfc's inference covers the binding half; the literal-vs-narrow-type face ships | type |
| F-0026 | duplicate fn names break native linking | DECLINE | mangling bug to fix | name |
| F-0027 | ordered `fcmp ne` | DECLINE | codegen bug fixed (lang#22); see L-37 | type |
| F-0028 | constant-splitting ulp errors | DECLINE | oracle territory | type |
| F-0029 | enum APIs importers cannot inspect | LINT (deferred) | see L-41/L-62 | type |
| F-0030 | O(len) range answers hang on legal input | LINT (deferred) | needs the range accessor first (lang#24) | type |
| F-0031/F-0033 | format specs honoured on one side, absorbed `0` | ALREADY-ERROR (wolfc) | E0412/E0413 landed with the executable spec grammar; lupin parity filed | syn |
| F-0032 | unknown cast target is a silent no-op | PROMOTE | lupin parity (interp#17); wolfc already rejects | name |
| F-0034 | last-segment module identity silently drops | PROMOTE | lupin parity; wolfc E0306 | name |
| F-0035 | copy/skip unwritable | LINT (deferred) | F-0018 cluster | type |
| F-0036 | tag collision returns the module | **LINT → W0305** + PROMOTE (interp resolution) | resolution fixed; double reading remains | name |
| F-0037 | enum through row always misses | PROMOTE | representation fix (interp#16) | type |
| F-0038 | absence has no literal | DECLINE | honest refusal | name |
| F-0039 | nested rows parse on one side | DECLINE | see L-60 | syn |
| F-0040 | no bottom type | DECLINE | language feature; L-61's dead-code lint deferred behind it | type |
| F-0041 | row-width measurement | DECLINE | evidence *for* W0602's threshold (≥2 tags), not a hazard | syn |
| F-0042 | `wolf test` requirements | DECLINE | s39's contract | — |
| FR-01..FR-06 | register hygiene / stale interim notes / lane drift | DECLINE (FR-02/FR-04 noted) | stale-workaround sweeps belong to std's conventions rig | meta |

### error-taxonomy.md

| ID | Hazard | Verdict | Rationale | Tag |
|---|---|---|---|---|
| T-M1 | rows past what callers branch on | **LINT → W0602** (threshold ≥2 on `pub`) | the measured 0-signatures-with-3+ fact sets the bar | syn |
| T-M2 | payload proliferation | DECLINE | review property | type |
| T-T1/T-R4 | `none` carrying a payload | DECLINE | convention without a D-number; std's rig + s69 | syn |
| T-T2 | parse-family payload drift | DECLINE | costed retrofit, deliberate | type |
| T-T3 | tag named like a same-module function | **LINT → W0305** | the F-0036 instance | name |
| T-T4/T-T9/T-R1 | CapCase marks / lowercase payload tags | DECLINE | see SE-05 — spelling conventions stay with std's rig until s69 arbitrates | syn |
| T-T5 | temporary tag outliving its finding | DECLINE | stale-interim sweep, std rig | syn |
| T-T6 | precondition folded into data-error tag | DECLINE | judgment | type |
| T-T7 | iterator raising `none` for exhaustion | LINT (deferred) | wants the settled iterator protocol | type |
| T-T8 | `deep` | DECLINE | conforming | syn |
| T-T10/T-T11 | reserved tags (`gone`, `eof`, `utf8`) reused | LINT (deferred) | fixed-list check; wait until the io tier makes misuse possible | name |
| T-C1..T-C3 | rename precedent / exemplar / costed retrofit | DECLINE | precedent, not hazard | — |
| T-R2 | payload carrying pre-rendered strings | LINT (deferred) | needs payload-type inspection; s69 | type |
| T-R3 | one tag per internal cause | DECLINE | not decidable; T-M1 is the proxy | syn |
| T-R5 | implicit coarsening | DECLINE | no implicit conversion exists | type |
| T-R6 | tag collides with anything in scope | **LINT → W0305** | "grep before naming", mechanized | name |
| T-R7 | `errdefer` conventions pre-io | DECLINE | unreachable in Phase A | mem |

**wolf-std totals: 132 examined — 11 rows land in shipped lints, 20
deferred-accepted, 13 promote/already-error routed, 88 declined.**

---

## Source 2 — the book's `ba:` ledgers (ch01–ch09)

Coverage: 74 real entries (83 grep hits minus 9 per-chapter headers),
74 examined. Only deviations from DECLINE and the shipped/deferred
lints are itemized here row-by-row; the full per-chapter tables live
in the mining record below.

| Row | Hazard | Verdict | Rationale | Tag |
|---|---|---|---|---|
| ch01:446/455/462/469/489/495/521/537 | version drift, packaging, K&R trivia, pin drift, REPL lane, gdb section, DWARF gaps | DECLINE | book rig / packaging / debug-info chores, no code shape | meta |
| ch01:476 | `… else 0` turns a parse failure into a valid-looking value | **LINT → W0601** (the discarded-row face) | the constant-else face needs row-payload flow — deferred behind W0601's evidence | type |
| ch01:483 | byte-offset spans with no line:col | DECLINE | renderer chore (lupin) | meta |
| ch01:506 | exit codes disagree; reserved-verdict collision | DECLINE | needs the `[conf.exit]` clause (lang#32); the `return 2` collision lint waits on the clause | meta |
| ch02:403 | format spec refused only at eval | ALREADY-ERROR | the executable spec grammar landed (E0412/E0413) | syn |
| ch02:411/419/430/436 | missing str surface, `s[..]`, trap-kind sharing, `s[i]` unsupported-channel | DECLINE / PROMOTE (ch02:436) | std surface + spec chores; `s[i]` deserves its own E-code — routed to the taxonomy backlog | — |
| ch02:425 | `r"{who}"` silently six bytes | **LINT → W0309** | highest-confidence row in the book | syn |
| ch03:641/656/662 | `let` reassign parity, range patterns, half-enums | DECLINE | already errors / grammar gaps | — |
| ch03:649 | else-less `if` in value position yields `()` silently | PROMOTE | E0401's vocabulary; routed to the sema backlog | type |
| ch03:670 | non-exhaustive match reported as `unsupported` | PROMOTE | E0801 exists; verdict-channel chore for the interp | type |
| ch04:447 | closure captures `var` by value; later writes invisible (100 vs 715) | **LINT → W1102** | the measured wrong answer | syn |
| ch04:456/467/476 | fn-value signature message, conform-run verdicts, extractor offsets | DECLINE | message/rig chores | meta |
| ch04:462 | body's tail value silently discarded when the signature omits `->` | LINT (deferred) | needs tail-type-vs-signature check with `()`-fn discipline settled | type |
| ch04:490 | nested `fn` answered as a resolve miss | PROMOTE | syntactically obvious; reject by name with a closure pointer | syn |
| ch05:557/565/577/595/599/606 | missing containers/combinators/list literals, generic unification gap, `for` over str | DECLINE / PROMOTE (ch05:599) | std surface; instantiation-site check routed | — |
| ch05:588 | absent Map key yields `()` | PROMOTE | see STD L-18a | type |
| ch05:611 | `.take()` vs `take` verb collision | DECLINE | position disambiguates by grammar; no wrong-code shape — a rename question for s69/spec, and the book's extractor already learned it | syn |
| ch06:747/776/783/788/799 | error traces, main args, errdefer spec sentence, rig | DECLINE | features/spec/rig | meta |
| ch06:756 | `?` from a wider row into a narrower accepted | PROMOTE | row subsumption is s15's core claim; routed (interp conformance) | type |
| ch06:767 | bare variant pattern binds, first-arm-wins | **LINT → W0801** + PROMOTE (dispatch fix, done) | see INT B17 | type |
| ch07:877/894/906 | read-param immutability, mut-vs-read overlap, E1003 | PROMOTE | spec-mandated checks (lang#27); error territory, not warnings | mem |
| ch07:913 | interp ran what wolfc rejects (X1) | ALREADY-ERROR | E1007 parity landed | type |
| ch07:922 | E0804's blast radius on Part-1 `push` | DECLINE (as a lint) | the ask is *demoting* an error or renegotiating the book's non-target — a ruling, not a new W-code; routed to the c16 arbiter (s69) | type |
| ch07:935/941 | pass verdicts, dump streams | DECLINE | driver chores | meta |
| ch07:950, ch08:1126, ch08:1136, ch09:980, ch09:992, ch09:1001, ch09:1046 | internal ids / nonexistent commands / future tense / 300-char lines in reader-facing text | **LINT → catalog-text test** (ids + commands) | length/tense caps deferred to s63's polish pass — the id/command test ships and cleaned five texts | meta |
| ch08:1085/1094/1105/1149/1161/1178/1193 | dynamic twins, place-writes, probes, verdicts, subsumption | DECLINE | parity/feature/emitter chores | — |
| ch08:1188 | `xs.len` field vs `pool.count()` method | LINT (deferred) | did-you-mean on len/count; wants the std naming decision first | type |
| ch09:941/952/962/972/1007/1017/1034/1058 | ring parity, cast-set, `*T` signatures, calloc typing, codegen/allocator gaps, probes, span drift | DECLINE | parity/feature chores | — |
| ch09:1026 | undeclared `#[trusted]` compiles without comment | LINT (deferred) | E1303 exists on the audit path; folding it into `wolf build` is a driver-wiring decision — routed to c10's owner, not a new code | syn |

**Book totals: 74 examined — 5 rows land in shipped lints (incl. the
catalog-text test closing 7 rows), 3 deferred-accepted, 10 routed as
promotions, 56 declined.**

---

## Source 3 — issues, wolf-lang (39) + wolf-interp (18)

Coverage: all 57 issues both repos, all states. Shipped-lint and
non-obvious rows:

| Issue | Hazard | Verdict | Rationale |
|---|---|---|---|
| lang#2 + interp#8 | `let` reassignment's pre-fix era | ALREADY-ERROR | E0410 both sides; the sprint's named closed scar |
| lang#9 | user `assert` shadows the intrinsic module-wide | **LINT → W0304** | with interp B20's sharper local-binding case |
| lang#15 + interp#9 | mutate-while-iterating | LINT (gated on S-11) | ruling open — see INT §E; wolfc's E1001-by-accident stands meanwhile |
| lang#22 | native `!=` ordered on f64 | DECLINE | codegen bug, fixed; `x != x` stays lintless (NaN probe) |
| lang#27 | read-mode immutability unenforced both sides | PROMOTE | soundness; a lint would under-sell a spec mandate |
| lang#28 | format specs silently ignored | ALREADY-ERROR | E0412/E0413 landed |
| lang#29 | duplicate module leaf | ALREADY-ERROR (wolfc) + PROMOTE (lupin) | E0306 |
| lang#30 | row tag resolving to a module and riding out | ALREADY-ERROR + **LINT → W0305** | raise-position resolves row-first now; the collision lint is the residue |
| lang#39 | diagnostic text carries sprint ids / nonexistent tools | **LINT → catalog-text test** | the issue asks for exactly this fold-in |
| interp#4 | cross-task captured writes lost | ALREADY-FIXED (interp) + **LINT → W1101** | the static shape short of E1101 |
| interp#5 | bare-ident patterns bound; first arm always matched | ALREADY-FIXED + **LINT → W0801** | binder residue |
| interp#11/#14 | non-converting casts / context-free literals | ALREADY-FIXED; W0401 covers the narrowing face | |
| interp#17 | `s as nonsense` silent no-op | PROMOTE | lupin parity; wolfc rejects |
| interp#18 | unsafe ring unenforced dynamically (4 gaps) | PROMOTE | soundness parity |
| all others (lang#1,3–8,10–14,16–21,23–26,31–38; interp#1–3,6,7,10,12,13,15,16) | build/plumbing/feature/spec/oracle chores, honest refusals, or already-errors | DECLINE / ALREADY-ERROR | per-issue rationale in the mining record |

**Issue totals: 57 examined — 5 feed shipped lints, 1 gated (S-11), 6
promotions routed, 12 already-error, 38 declined.**

---

## Source 4 — divergence log, approximation contract, conservatism ledgers, lint-later spec notes

Coverage: 31 divergence-log entries (DIV-*, S-1..S-11, round records),
42 approximation-contract clauses, 48 conservatism-ledger rows (4
classes + 40 corpus files + 4 prose classes), 7 lint-later notes — 128
examined. Rows that shaped the wave:

| Row | Hazard | Verdict | Rationale |
|---|---|---|---|
| S-10 / C3 (`store_buffer.lu`) / B37 / A24 | task writes its captured copy; enclosing state never sees it | **LINT → W1101** | named in four places; `store_buffer.lu` now declares `warns: W1101, W1102` and is the corpus witness |
| S-11 / A13 / B18 (`for` operand semantics unstated) | `for x in xs { xs.push(x) }`: three readings, three verdicts | **LINT (gated on S-11 — ruling OPEN)** | verified open: listed under "remain open" in the divergence log, absent from spec/01 and spec/02, tracked as interp#9/F-0014/lang#15. Under loop-entry copy the lint is the answer and E1001 must demote; under move or extent-hold the lint is redundant. Fixture exists (wolf-std `mutate_while_iterating.lu`); implementation held per the contract's "coordinate" |
| B17 / B20 | capitalized binding over a case-less scrutinee binds silently; local `assert` rebinding | **LINT → W0801, W0304** | the checker resolves case-names type-directed already; the binder residue ships |
| B30 (AC §7.8) | `assume noalias` checked where written; reassignment voids it silently | **LINT → W1302** | sharp, cheap, UB-adjacent. The clause's anchor mismatch (`[ub.assume.noalias]` unregistered; the real clause is `[mem.unsafe.raw.2]`) noted for the spec owner |
| B19a / B16 / B21 | narrowing casts of known literals; struct-position literal adoption | **LINT → W0401** | the wide-literal narrowing member, confirmed |
| B8 / C14 (`shared_cycle.lu`) | E1006 has no dynamic counterpart; suspicious near-cycles | LINT (deferred) | E1006 owns provable cycles; the near-cycle heuristic needs an RC-graph reachability pass — wave two |
| B11 | frozen non-struct composites take writes on a copy | LINT (deferred) | value-home tracking; also an interp repair |
| B12 | in-place growth creates unchecked cross-region edges | LINT (deferred) | compiler-only; needs store-site analysis |
| B31 | modelled C set hides real `malloc` failure / overlapping `memcpy` | LINT (deferred) | unsafe-tier wave two, pairs with W1301 |
| B28 | code depending on `ptr as int` numeric values | LINT (deferred) | rare; behind the first wave |
| B38 / B39 | kill-skipped `defer`s; blocking-point-free loops in cancellable scopes | LINT (deferred) | need effect/termination notions; refiled s69/s36 |
| D1 (E0004 float-hint) | `1.e5` parses as member access; the interp's `CHOICES` note says "belongs to a lint" | **PROMOTE (stays an error)** | `int` has no member `e5`, so the program has no meaning — `[diag.sev.error]` fits exactly; demotion refused. Correction to file against lupin's `gram.lex.number` note; lupin can honestly produce E0004 at its resolve rung |
| D2 (s15 anonymous rows) | inline multi-tag rows on `pub` signatures | **LINT → W0602** | the lean s15 recorded as "allowed but linted", executed; single-tag rows exempt; the alias-extraction fix-it waits on lang#36's surface |
| D3 (`gram.amb.structlit`) | `{ x }` in condition position is silently a block | DECLINE | comparison-to-a-type errors downstream in practice; the truly-silent case needs field-coincidence machinery with zero recorded bites |
| D6 (grandfathered severities) | E0802 is a warning under an E-number | DECLINE (housekeeping done) | severity-not-letter is normative (spec §9.2); W0801 covers the beyond-reach adjacency, and E0802 keeps the dead arms |
| D7 (`[proto.record.warn]`) | one-sided lints invisible to the differential | adopted as the classification rule | every wave lint tagged shared vs compiler-only below |
| A30 (`resolve/unused/main.lu`) | unused import | DECLINE, deliberately | E0305 is a hard error by recorded ruling; re-opening as a lint would contradict the spec — the canonical declined-with-reason row |
| A16 (S-2) | region touched after channel send | PROMOTE | affine-move soundness, E1001 machinery |
| C19 (`errdefer_infallible.lu`) | dead `errdefer` | ALREADY-ERROR | E0607 with a `defer` fix-it |
| C40 (`receiver_bare_mut.lu`) | bare mut receiver runs in lupin | DECLINE | E0804 owns it; ledgered conservatism by design |
| C41 | `1.0 == 1` legal-false in lupin | DECLINE (wolfc) | E0401 already; lupin parity filed |
| all remaining A/B/C rows | oracle internals, spec repairs landed, protocol bookkeeping, already-errors | DECLINE / ALREADY-ERROR | per-row rationale in the mining record |

**Interp/spec totals: 128 examined — 6 rows land in shipped lints, 1
gated (S-11), 8 deferred-accepted, 4 promotions routed, 109
declined/already-error.**

---

## Cross-implementation classification (target 3)

**Shared-analysis (lupin can implement — filed):** W0304, W0305,
W0306, W0307, W0308, W0309, W0602, W1101, W1102, W1302 (syntax +
name-resolution only), and W0401's trigger (literal + spelled target
type). Filed against wolf-interp with spans and fixture pointers —
warning parity per `[proto.cmp.warn]` grows lint-by-lint; `#[allow]`
is part of the program and suppresses identically on both sides.

**Compiler-only (conformance posture: honest-absent):** W0601 (row
typing), W0402 (float typing of the subtraction), W0801 (scrutinee
case tables), W1001 (region inference). Lupin reports no warnings
array entry for these until it grows the analysis; absence is never a
divergence.

## Grand totals

| Source | Candidates | → shipped lint | deferred-accepted | promote/already-error | declined |
|---|---|---|---|---|---|
| wolf-std docs | 132 | 11 | 20 | 13 | 88 |
| book `ba:` ledgers | 74 | 5 | 3 | 10 | 56 |
| issues (both repos) | 57 | 5 | 1 (S-11) | 18 | 33 |
| interp divergences + spec notes | 128 | 6 | 9 (1 S-11) | 4 | 109 |
| **total** | **391** | 15 distinct codes + the catalog-text test | 31 | 45 | 286 |

(Shipped-lint counts overlap across sources by design — a lint needed
scars, and the strongest members have several.)

## Standing coordination

- **S-11** — open; the `for`-operand ruling decides whether the
  mutate-while-iterating lint ships, demotes E1001, or dies. This
  document is the coordination record the contract asked for.
- **E0004** — stays an error; correction filed against lupin's
  `CHOICES["gram.lex.number"]` note.
- **wolf-std / book posture** — the wave was tuned against wolf-lang's
  own corpus (`--deny-warnings` green, one reviewed `#[allow(w1001)]`
  with its reason in `corpus/rows/qmark_defer.lu`); std trees are
  exempt from the wave when compiled as libraries (`use std.*`), so
  std's own `--deny-warnings` posture is owned by its rig at its next
  toolchain pin, with `#[allow]` available where its wrappers are
  deliberate.
