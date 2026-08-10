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
  `gram` → 01-grammar.md · `mem` → 02-memory-model.md ·
  `conc` → 03-concurrency.md · `abi` → 04-abi.md ·
  `conf` → 05-conformance.md · `proto` → 06-differential-protocol.md.
  **Reserved forward namespaces** (owned by spec documents not yet
  written; tags in them are legal, reported as *forward*): `str`, `err`,
  `task`, `proc`, `sync`, `generics`, `arith`, `ffi`, `unsafe`,
  `comptime`, `perf`, `mod`, `std`, `ty`. A tag outside all registered and
  reserved namespaces is a CI failure.
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
  expectation matches the failing code exactly.
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
  meanings of E1001/E1002), `region-fault` (dynamic region-rule
  violations: the runtime meanings of E1004 — illegal cross-region
  edge — and E1005 — transfer of an open region — plus rule violations
  the static tier cannot see), `stale-handle`
  (`[mem.shared.handle.2]`), `alloc-contract` (I15 `#[noalloc]`-family
  violations in checked builds), `assert` (user assertions), `race`
  (`[conc.mm.race.3]` — detection permitted, not required), `ub`
  (oracle-detected UB; `[proto.record.ub]` gives it comparison
  semantics), `deadlock` (`[conc.deadlock.trap]` — every live task
  blocked with no pending timer or I/O, and the self-acquisition case
  `[conc.deadlock.self]`; detection required in deterministic test
  modes, permitted elsewhere).
- `[conf.trap.exit]` A trap terminates the process with a nonzero,
  implementation-specified exit status; conforming tools compare the
  *kind*, never the status number.

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
