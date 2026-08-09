# Wolf Language Specification — 05: Conformance

Status: normative, v0 (sprint s06). This document makes the spec testable:
it defines clause anchors, test tagging, the trap vocabulary, and coverage
reporting. Consumed by both implementation tracks; wolf-interp reads only
`spec/` + `corpus/` — this document is part of that sealed interface.

---

## §1 Clause anchors `[conf.anchor]`

- `[conf.anchor.grammar]` An anchor is a dotted, lowercase token
  `[ns.a.b…]` (letters, digits, `-`, `.`). The leading segment is its
  **namespace**; the owning document defines every anchor of its
  namespace.
- `[conf.anchor.ns]` Registered namespaces and owners:
  `gram` → 01-grammar.md · `mem` → 02-memory-model.md ·
  `conc` → 03-concurrency.md · `abi` → 04-abi.md ·
  `conf` → 05-conformance.md · `proto` → 06-differential-protocol.md.
  **Reserved forward namespaces** (owned by spec documents not yet
  written; tags in them are legal, reported as *forward*): `str`, `err`,
  `task`, `proc`, `sync`, `generics`, `arith`, `ffi`, `unsafe`,
  `comptime`, `perf`, `mod`, `std`. A tag outside all registered and
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

## §3 Trap & exit vocabulary `[conf.trap]`

- `[conf.trap.set]` `run(exit=…)` values are plain integer exit codes,
  `trap` (kind unspecified), or `trap(kind)` with kind from the closed
  set: `overflow`, `div-zero`, `bounds`, `use-after-move`, `exclusivity`,
  `region-fault`, `stale-handle`, `alloc-contract`, `assert`, `race`,
  `ub`. The set is closed; extension requires revising this spec.
- `[conf.trap.map]` Compiler, interpreter, and UB oracle map their
  runtime faults onto this single vocabulary — it is the comparison
  alphabet of spec 06. Sources: `overflow`/`div-zero`/`bounds` (s04
  defined-behavior table), `use-after-move`/`exclusivity` (s04 dynamic
  meanings of E1001/E1002), `region-fault` (dynamic region-rule
  violations the static tier cannot see), `stale-handle`
  (`[mem.shared.handle.2]`), `alloc-contract` (I15 `#[noalloc]`-family
  violations in checked builds), `assert` (user assertions), `race`
  (`[conc.mm.race.3]` — detection permitted, not required), `ub`
  (oracle-detected UB; `[proto.record.ub]` gives it comparison
  semantics).
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
