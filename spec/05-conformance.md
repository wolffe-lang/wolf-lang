# Wolf Language Specification — 05: Conformance

Status: normative, v0 (sprint s06). This document makes the spec testable:
it defines clause anchors, test tagging, the trap vocabulary, and coverage
reporting. Consumed by both implementation tracks; wolf-interp reads only
`spec/` + `corpus/` — this document is part of that sealed interface.

---

## §1 Clause anchors `[conf.anchor]`

- `[conf.anchor.grammar]` An anchor is a dotted, lowercase token
  `[ns.a.b…]` (letters, digits, `-`, `_`, `.`). The leading segment is its
  **namespace**; the owning document defines every anchor of its
  namespace.
- `[conf.anchor.ns]` Registered namespaces and owners:
  `gram` → 01-grammar.md · `diag` → 01-grammar.md (§9) ·
  `mem` → 02-memory-model.md · `conc` → 03-concurrency.md ·
  `abi` → 04-abi.md · `conf` → 05-conformance.md ·
  `proto` → 06-differential-protocol.md ·
  `sched` → 07-schedule-points.md · `pkg` → 08-package.md ·
  `ct` → 09-constant-time.md · `type` → 10-types.md ·
  `os` → 11-os.md · `exec` → 05-conformance.md.
  **Reserved forward namespaces** (owned by spec documents not yet
  written; tags in them are legal, reported as *forward*): `str`, `err`,
  `task`, `proc`, `sync`, `generics`, `arith`, `ffi`, `unsafe`,
  `comptime`, `perf`, `mod`, `std`, `ty`, `test`. A tag outside all
  registered and reserved namespaces is a CI failure. (`test` appended
  2026-08-11 by s39 for the built-in test framework's litmus tier —
  D34/D36 own the future spec document; the append is additive per
  this clause's own contract, nothing renumbered. `pkg` appended
  2026-08-27 by s115 for #120: 08-package.md's sixteen anchors were
  registered in the extractor index but never admitted by this clause's
  letter — the append reconciles the two, additive, nothing renumbered.
  `diag`, `ct`, `type` and `os` appended 2026-09-06 by r09 for #239 —
  the same reconciliation, four documents over. 01-grammar.md §9 (s67),
  09-constant-time.md (s112), 10-types.md (s113) and 11-os.md (s114)
  each went normative and entered the extractor's registered set with no
  append here, so **seventy-one published anchors stood outside this
  clause's letter** at the moment of this append while `[conf.tag.valid]`
  made citing any one of them a CI failure. That is not cosmetic: a rig
  that mirrors this letter rather than an extractor's output cannot name
  the clause its own witness holds, which is where the bill came due
  (wolf-std F-0099 — six `[os.net.unix]` witnesses at sc36 carried the
  forward tag `std.net.unix` and recorded the real clause in a comment).
  The anchors did not move, and could not: `[conf.anchor.stable]` forbids
  moving a published anchor, and the citations that would have had to
  move with these number 3,273 across nine repositories against the one
  paragraph an append costs. `sched` appended 2026-09-07 by s139 for
  #246, the MIRROR of that: 07-schedule-points.md declared seven
  `[sched.*]` anchors while standing in neither this list nor the
  extractor's document table, so the registry published NONE of them —
  and a document nothing reads raises no alarm, which is the restrictive
  half of the same silence. #239 let permissive tooling publish past the
  clause; this let a document declare past the tooling, leaving anchors
  that `[conf.tag.valid]` could only ever reject. It was not idle prose:
  the native runtime cites four of the seven by name from
  `wolf_rt::task::det`, `::reactor`, `::net` and `::task::hooks`, and
  `wolf_driver`'s `--schedules`/`--replay` surface implements §5
  (X12/D23, locked). Additive, nothing renumbered — the seven had never
  been published, so `[conf.anchor.stable]` had nothing to pin.
  `ty` stays reserved and unused —
  10-types.md chose `type`, and a reservation nothing cites is cheaper
  to leave standing than to withdraw. `exec` appended 2026-09-11 by
  s153 for wolf-lang#308: the checked tier's budgets went normative
  (`[exec.checked.budget]`, §5 below) and no execution document
  exists, so this document owns the namespace — a tier's budget
  decides what `unsupported` means on that lane, which is a
  conformance fact. Registered in the clause and in `NS_OWNERS` in the
  one change `[conf.anchor.ns.admit]` requires; additive, nothing
  renumbered.)
- `[conf.anchor.ns.admit]` **A namespace is admitted in one change or
  not at all.** When the document that owns a namespace becomes
  normative, that same change appends the namespace to the registered
  list above AND to the registered set of the anchor tooling on every
  implementation track (`[conf.anchor.index]`). A namespace present in
  one and absent from the other is the defect #120 and #239 each name,
  and it is **silent on whichever side is permissive** — a track whose
  tooling admits the namespace goes on publishing anchors and passing
  CI, so the gap only ever announces itself on a track that mirrors this
  letter, by rejecting a tag that ought to be legal. The reverse
  omission is quieter still: a document owning a namespace NEITHER side
  registers is read by no extractor, so its anchors are declared and
  never published and no gate anywhere holds an opinion (#246,
  07-schedule-points.md, seven anchors for a year). Admission is
  therefore checkable in BOTH directions and must be checked both ways:
  every registered namespace publishes anchors, and every anchor a spec
  document writes sits in a namespace this clause registers or reserves.
  Admission is additive and one-way: a namespace that has published
  anchors may be registered late, never un-registered, because
  `[conf.anchor.stable]` pins the anchors that carry it. One document
  may own several namespaces (01-grammar.md owns `gram` and `diag`); no
  namespace has two owners. A namespace whose document is not yet written is
  **reserved**, not registered, and its tags are *forward*.
- `[conf.anchor.stable]` Anchors are **stable once published**: never
  renumbered, never reused. A deleted clause leaves a tombstone (the
  anchor with the text "*tombstone — see <replacement or rationale>*").
  Amendments append new anchors; they do not edit the meaning of
  published ones beyond errata.
- `[conf.anchor.index]` `cargo xtask spec-extract` emits
  `spec/anchors.json` — the machine-readable registry
  `{ "version": 1, "anchors": { "<anchor>": "<owning file>" } }` — and
  CI fails if it is out of sync with the documents.

## §2 Test tagging `[conf.tag]`

- `[conf.tag.key]` The corpus directive key `conforms:` carries a
  comma-separated list of anchors (grammar of directives: s01, extended
  here — this is the one extension s01 anticipated).
- `[conf.tag.must]` Every file under `corpus/grammar/`, `corpus/memory/`,
  and `corpus/conc/` (the litmus tiers) **must** carry `conforms:`;
  other corpus files may.
- `[conf.tag.valid]` A tag in a registered namespace must name an anchor
  present in `spec/anchors.json` — an unknown anchor is a CI failure.
  A tag in a reserved forward namespace is counted as *forward* and
  becomes checkable when its document registers the namespace.

## §2a Corpus directives `[conf.directive]`

The complete directive language of corpus files (formerly split across
process docs; normative here so independent implementations share one
parser contract):

- `[conf.directive.block]` Directives live in the leading `//!` block;
  non-directive `//!` lines are prose. Keys, each at most once
  (duplicates are errors): `check:`, `phase:`, `conforms:`, `member:`.
- `[conf.directive.check]` `check: pass | fail(CODE) | run(exit=N |
  exit=trap | exit=trap(kind) [, stdout="…"])` — kinds from
  `[conf.trap.set]`; unknown kinds/phases are errors. A `fail(CODE)`
  expectation matches the failing code exactly. A `stdout="…"`
  expectation matches the program's stdout byte-exactly EXCEPT that one
  trailing newline in the observed output is ignored (`print` appends
  one; directives stay single-line). Cross-implementation stdout
  comparison ([proto.record]'s `stdout_sha256`) remains byte-exact —
  the newline allowance is the directive matcher's alone.
- `[conf.directive.phase]` `phase:` names the deepest rung of the
  canonical ladder (`none, lex, parse, resolve, typecheck, mem, wir,
  run`) that succeeds today — the truthful-ledger contract.
- `[conf.directive.member]` `member: true` marks a file compiled through
  its directory's entry file (directory = module; the package root is
  the entry file's directory) — never conform-run directly. A member
  file carries neither `check:` nor `phase:` (error if present); an
  entry file must carry both. `member: false` is legal and means entry.
- `[conf.directive.conforms]` As §2; duplicate anchors within one file
  are errors.
- `[conf.directive.standalone]` Module formation for a single-entry
  compilation (appended 2026-08-28 by s124, ruling D59): every `.lu`
  file in a directory is a member of that directory's module
  (directory = module) **except standalone entries** — files whose
  leading `//!` block carries `member: false` or both `check:` and
  `phase:` (the entry pair of `[conf.directive.member]`), files that
  announce script mode (a `#!` first line or a `pkg { … }` frontmatter
  block), and files whose name ends `_test.lu` (the test-discovery
  pattern). An explicit `member:` key always decides. The named entry
  of a compilation always belongs to its own root module, whatever its
  markers. A directory whose `.lu` files are all standalone entries
  forms no module. `member: true` remains legal and marks membership
  explicitly — it is the default for plain files, so the marker is
  needed only to override an entry-shaped header.

## §3 Trap & exit vocabulary `[conf.trap]`

- `[conf.trap.set]` `run(exit=…)` values are plain integer exit codes,
  `trap` (kind unspecified), or `trap(kind)` with kind from the closed
  set: `overflow`, `div-zero`, `bounds`, `use-after-move`, `exclusivity`,
  `region-fault`, `stale-handle`, `alloc-contract`, `assert`, `race`,
  `ub`, `deadlock`. The set is closed; extension requires revising this
  spec. (Revised 2026-08-10, deliberately: `deadlock` added by the
  spec/03 amendment `[conc.deadlock.trap]` — is06 finding S-3 showed
  the vocabulary had no spelling for an all-tasks-blocked outcome.)
- `[conf.trap.map]` Compiler, interpreter, and UB oracle map their
  runtime faults onto this single vocabulary — it is the comparison
  alphabet of spec 06. Sources: `overflow`/`div-zero`/`bounds` (s04
  defined-behavior table), `use-after-move`/`exclusivity` (s04 dynamic
  meanings of E1001/E1002; `exclusivity` is also the dynamic meaning
  of E1013's iteration claim — `[mem.iter.excl]`, D40 — and E1014's
read-mode write barrier, D39), `region-fault`
  (dynamic region-rule
  violations: the runtime meanings of E1004 — illegal cross-region
  edge — and E1005 — transfer of an open region — plus rule violations
  the static tier cannot see), `stale-handle`
  (`[mem.shared.handle.2]`), `alloc-contract` (I15 `#[noalloc]`-family
  violations in checked builds; region cap breaches and negative
  budgets — `[mem.region.cap.1]`/`[mem.region.cap.2]`, the s132/D68
  amendment: a byte budget is an allocation contract), `assert`
  (user assertions, ruled
  caller-contract violations of builtin surfaces —
  `[mem.str.repeat]`, `[os.random.fill]` — and the ruled
  runtime-refusal trap of the entropy surface, `[os.random.trap]`:
  the one builtin failure that is deliberately a trap rather than a
  row), `race`
  (`[conc.mm.race.3]` — detection permitted, not required), `ub`
  (oracle-detected UB; `[proto.record.ub]` gives it comparison
  semantics), `deadlock` (`[conc.deadlock.trap]` — every live task
  blocked with no pending timer or I/O, and the self-acquisition case
  `[conc.deadlock.self]`; detection required in deterministic test
  modes, permitted elsewhere).
- `[conf.trap.exit]` A trap terminates the process with a nonzero,
  implementation-specified exit status — unless it fires on a task
  inside a proc, where `[conc.proc.exit]`'s containment (D68, s132)
  turns it into the proc's `fault(kind)` reason and the process
  lives; this clause governs the root domain and every uncontained
  trap. **A trap runs no `defer` or `errdefer`, anywhere**: in the
  root domain death is immediate, so the pending scope-exit effects
  of the trapping scope and of every scope enclosing it are
  abandoned, exactly as `[conc.proc.exit]`'s killed-proc sequence
  abandons the ones below a proc boundary. A trap is not an error
  value and never unwinds (`[abi.native.nounwind]`); effects that had
  already run, ran. Conforming tools compare the
  *kind*, never the status number. The statuses in force are
  documented facts of each implementation, not comparison surface:
  the native tier exits 134 (`wolf_rt`'s `TRAP_EXIT_CODE`, 128+SIGABRT
  by convention, deterministic — never a real signal; the SAME number
  on windows-native, where it is a plain `ExitProcess(134)` with no
  signal arithmetic behind it — D70, s60a), the reference
  interpreter exits 3. (Appended 2026-08-28, s125: the divergence had
  been implicit since s28 and every "predict the exit code" exercise
  tripped over it — #150. The defer sentence added 2026-09-01, r05:
  the clause ruled the proc path and was silent about the root, so
  nothing pinned whether a root-domain trap flushed its pending
  defers — `faults/trap_skips_root_defers.lu` is the witness, and it
  records a measured divergence, lupin 0.1.22 running the root defer
  where every wolfc lane abandons it — #209.)
- `[conf.trap.report]` A compiled program reports its trap on stderr
  in a fixed shape. The FIRST line is the machine contract:
  `wolf-trap: <kind>` — everything after the prefix, trimmed, is the
  kind, one name from `[conf.trap.set]`, and nothing else may appear
  on this line (harness parsers take the remainder wholesale; one
  added byte corrupts every trap verdict). When the implementation
  knows the trap's source coordinates, a SECOND line names them:
  `  at <file>:<line>:<col>` — two leading spaces, the file's display
  path, then 1-based line and column of the trap SITE — the statement
  whose check fired, never the enclosing function declaration. The
  site line is additive and optional in both directions: conforming
  parsers recover the kind from the first line alone, tolerate the
  site line's presence or absence, and never require it. Further
  report information, if any tier ever adds it, goes on further
  lines — line one is closed. (Added 2026-08-28, s125.)
- `[conf.trap.render]` The reference interpreter renders the same
  fault as its one human diagnostic line — kind, message, clause,
  and the location spelled `line:col` in the SAME span grammar its
  static diagnostics use (one span spelling per tool; raw byte
  offsets remain available through `--json`, `[proto.record.ext]`'s
  trap span). Exit status per `[conf.trap.exit]`. (Added 2026-08-28,
  s125; the interpreter's line:col rendering lands in wolf-interp
  from this clause — the independence doctrine — as the sequenced
  follow-on after lupin 0.1.15.)
- `[conf.trap.assert]` `assert` is an **intrinsic** — one name in both
  tiers: comptime witness (a failing comptime `assert` is a compile
  error) and runtime user trap (the `assert` kind of
  `[conf.trap.set]`), silent and effect-free when the condition holds,
  trapping at its own span when not. It is not a library function and
  is never shadowed by one — the name cannot be both library surface
  and primitive (observed: a module-level `assert` severed callers from
  the trap; wolf-std F-0009). The two-argument form `assert(cond, msg)`
  is the intrinsic's own arity: `msg` is a `str` evaluated **only** on
  the failing path; rendering is one line to stdout before the trap
  once formatting lands — until then implementations may drop the
  message. (Appended 2026-08-10, wolf-std F-0009 / issue #9, contract
  F4.)

## §4 Coverage `[conf.cover]`

- `[conf.cover.report]` `cargo xtask conformance` reports: anchors with
  zero tests (the **debt list** — tracked and burned down across
  c02–c07 as phases become executable), corpus files citing no clause,
  forward-tag counts, and per-document coverage percentages.
- `[conf.cover.format]` Machine output is JSONL, one record per anchor:
  `{"clause": …, "tests": N, "status": "covered"|"debt"|"tombstone",
  "commit": …}` — the D5 shape, so nightly CI trends coverage like a
  benchmark.
- `[conf.cover.gate]` CI gates on tag *validity* (`[conf.tag.valid]`,
  `[conf.tag.must]`), never on coverage percentage — debt is visible,
  not blocking (c01 ships clauses faster than phases can test them; the
  ratchet arrives with the phases).

## §5 Execution tiers `[exec]`

(Appended 2026-09-11 by s153 — wolf-lang#308. The checked tier's
budget counted steps and not bytes, and one wolf-std row reached
16.6 GiB of resident set on a 16 GB runner before `step budget
exhausted` arrived — an OOM where an honest refusal was owed.)

- `[exec.checked]` The **checked tier** (`wolf conform-run --checked`,
  `wolf_mem::ubcheck`) runs `[mem.model.machine]`'s abstract machine
  as an interpreter: single-threaded, run-to-completion, every
  allocation modelled in shadow memory, every UB row of `[mem.ub]`
  detected rather than exploited. Its answers are `[conf.trap]`
  verdicts or an honest `unsupported` (`[proto.record.unsupported]`);
  it never guesses.
- `[exec.checked.budget]` **The tier keeps two budgets — steps AND
  bytes — and exhausting either is `unsupported`, never a verdict.**
  The *step* budget counts expression evaluations (20,000,000 at
  s153). The *byte* budget counts allocation volume in the machine's
  own units, cumulatively over the run (256 MiB at s153): every
  allocation the program performs — a container and its growth, a
  materialized `List[byte]`, a `str` built by `+`, `+=`, `repeat` or
  an interpolation with a hole — is charged when it is made and the
  charge is never refunded (the ledger is monotone, as a region's
  `[mem.region.account.1]` ledger is). Bytes are not a step cost and
  steps are not a byte cost: a loop that allocates nothing runs to its
  step budget, a single expression that allocates runs to its byte
  budget, and a program whose per-step cost is O(n) in memory is
  refused by the byte budget long before a step budget could see it.
  **What the byte budget buys is the invariant behind it: the machine
  retains on the host nothing the ledger has not charged.** A view
  (`[mem.str.view]` — a consumed `s.bytes()`, a `str` slice, `trim`,
  `get`, `strip_*`) charges zero AND retains zero: the machine reads
  the receiver's own bytes and mints nothing; whatever the program
  asks the machine to keep is charged. The tier's resident set is
  therefore bounded by the byte budget times the machine's per-unit
  overhead (one `Value` slot per charged unit), plus the program's own
  live sources — a constant, not a term that grows with steps — so a
  bounded host sees the refusal, not the kill. (Ruled 2026-09-11 by
  s153 for #308. Between s136 and s153 the consumed `bytes()` view was
  charged zero — correctly, per `[mem.str.view]` — and STILL
  materialized and retained a list per call: the ledger said 0 while
  the host paid O(len) per step for the rest of the run, ×3.8 per
  doubling of sc43's twelve-line reduction, 16.6 GiB on
  `cavp_sha384_long.lu`, flat on native and lupin. Weighed and
  rejected: bytes as a step cost, one ceiling — it prices a 64 KiB
  walk and a 64 KiB allocation the same, and the whole finding was
  that they are not. Witnesses: `corpus/strings/bytes_view_walk.lu`
  and the driver's `checked_budget` test, which asserts the peak
  resident set of the reduction as a subprocess.)
