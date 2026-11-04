# s281 — a bare variant nested in a payload pattern tests its variant (wolf-lang#1043)

`K(Drink)` over `enum W { K(Kind) }` matched every `K` on the native and release tiers: the arm
classifier bound every bare name in a payload slot as a name, without asking the slot's type.
The checker, the checked machine and lupin read the arm as a test. Now the classifier asks the
slot's type, and a bare name that spells one of its cases lowers to the tag test the product
walk already emits for the same name inside a tuple or a struct field. One function in
`crates/wolf_wir/src/lower.rs` changed; nothing in `spec/`, the checker, the checked machine or
the runtime did.

## Forbidden

Footprint: cross-cutting

The fix is in `crates/wolf_wir/`, which lane-audit's wolf-lang map names codegen, so the PR is
`cross-cutting` (s268's and s270's precedent): the full-matrix label and its run are the
orchestrator's. The rest of the diff is corpus rows, their lexer and parser snapshots, the
lowering ledger, four-machine test rows and notes. No `spec/` change (R-s281-1), no checker,
runtime, CI or pin change. Clean room: the spec, the compiler's own source and black-box runs of
lupin 0.1.58 and the published wolf 0.2.36; no other compiler's source. No label, merge, tag,
release, issue filed or closed, and no attribution trailer; wolf-lang#1043 closes by hand at the
merge. The 0.2.37 release's branch and PRs are untouched. The plant is pushed without a PR.
Nothing ran under `/tmp` or `/var/folders`: TMPDIR is the lane's own. No commit was made on hasu.

## Inputs, verified

- Base `5f1f1c94` (trunk, s278). wolf built there on nomad (`cargo build --locked -p wolf_driver
  -p wolf_rt`, Rust 1.97.1); the published wolf 0.2.36 (`wolf --version`: pin `5adf3d9`) and
  lupin 0.1.58 (pin `a5378e3`, the `PAIRING` stamp). On hasu, lupin 0.1.58's linux archive
  (sha256 `34f52792…2193`) and wolf 0.2.35's (`4cc4045b…ad1a`, the clef and ligature builds'
  toolchain), each read twice and equal to the digest the release page reports.
- wolf-lang#1043 whole, reproduced at the base and on 0.2.36: `drink drink drink` natively for
  Food, Drink, Gratuity; `other drink other` on the checked machine and lupin. `Item(Drink, _)`
  over `Item(Kind, int)` is wrong the same way. `K(Kind.Drink)`, `(Drink, _)` and a top-level
  `Drink` are right.
- Spec: `[gram.pat]` (payload positions take `pattern`, recursively), `[gram.pat.nullary]` (a
  bare `path` matches a constructor that carries nothing), `[gram.expr.variant]` ("a `match` arm
  … keeps the bare name: there the scrutinee's type already says which enum"),
  `[type.row.match]`; E0806's and W0801's registry texts.
- The checker (`crates/wolf_sema/src/check.rs`, `match_pattern`): a bare name is a test when it
  spells a case of the type of the value it matches (`scrut_cases`, the #4 rule). The payload,
  tuple and field arms recurse with the sub-value's type, so the rule holds at every depth.
  `bind_pattern` (the binder positions) binds every bare name.
- The checked machine (`crates/wolf_mem/src/ubcheck.rs`, `match_pattern`): nested positions
  replay the checker's record; a name the checker did not bind is a case test. It was right at
  the base for every row.
- README read, current: "the two tiers agree on every corpus program that runs" held throughout
  (native and release were wrong together), and nothing else it states moves.

## Prediction

Committed before the first change (`3a7bc845`, `notes/s281-prediction.md`). Scored:

- **The code path: hit.** `Lowerer::pattern_shape`, the `PathPat` arm. When every payload sub is
  a bare name or `_`, it returned `Tests([Eq(tag)], binds)` and pushed every bare name as a slot
  binding. A sub that is anything else (`Kind.Drink`, `In(…)`, `Drink | Food`, a tuple, a
  struct, a literal) sent the arm to the product walk, where `product_sub` asks
  `names_sema_case` of the field's own type. The top level asks `domain_test`; only this fast
  path asked nothing.
- **Why it diverged.** The fast path is s27's ("payload subpatterns bind slots"), written before
  bare names nested in a payload were typed. s130 and s168 taught the product walk to decide a
  bare name by the field's type, and the fast path kept the s27 reading. Every red row goes
  through it; every green one does not. The two-level row (`Out(In(Drink))`) and the payload
  or-pattern (`K(Drink | Food)`) were expected red by the contract and are green at the base,
  because their sub is a `PathPat` or an `OrPat`; they are controls here.
- **The rows: hit.** At the rows' commit (`7afb039b`, the base's code), the corpus step is
  `1262 file(s), 4 bad`, the four rows by name, and `four_machine_regressions` fails the same
  four (the checked machine passes, native fails): `K(Drink)`, both slots of a two-field
  payload, a guarded arm, and a row tag's payload (a row arm of a `match` over a fallible value,
  and a `match` over the row value an `else |e|` caught). Release printed native's wrong answers
  (`nomad/mac-job2-7afb039b.log`). The five controls are green on all four machines.
- **The fix: hit** (`7818bfcf`). For a bare-name sub, the fast path reads the slot types once
  (`payload_sema_tys`, the product walk's own reading of a payload, rows and generic slots
  included) and asks `names_sema_case`, the product walk's mirror of the checker's rule. A name
  that spells a case sends the arm to the product walk, which lowers `K(Drink)` to the tag test
  on `K` and the tag test on the slot's discriminant, and the guard after both. A name that
  spells no case binds, as before. A slot type the lowering cannot name (a generic payload's
  compound type) admits no case there, so its names bind, as they always did. At the fix, all
  nine rows are green on all four machines and the corpus step is `1262 file(s), 0 bad`
  (`nomad/mac-job3-7818bfcf.log`).
- **The sweep: as predicted** (the probes at the head, `nomad/mac-job4-3972d9d3.log`):
  - struct fields (`P { k: Drink, .. }`, `K(P { k: Drink, n })`): green before and after;
  - the guard, the second slot, both row positions, and a lowercase variant (`M(fast)` over
    `enum Mode { fast, slow }`): red before, green on the checked machine, native and release
    after. lupin cannot construct `Mode.slow` ("does not resolve"), so that shape has no corpus
    row and stays in the probes;
  - binders (`let (Drink, n) = …`, `let P { k: Drink, n } = …`, `for (Drink, n) in …`): every
    machine binds `Drink`, before and after (R-s281-2);
  - `select`: `Drink from ch` binds on every wolf tier, before and after; lupin reads a test
    (R-s281-3). `K(Drink) from ch` is refused on every machine;
  - `else |bad(Drink)| 7`: E0809 at the checker on every tier, before and after (lupin runs it
    and refuses at run, "did not match the error value");
  - `K(Drink) | K(Food)`: refused on native and release, before ("or-patterns with payload
    bindings") and after ("or-patterns over product patterns (join params)"), and run by the
    checked machine and lupin (R-s281-4).
- **The plant: see the PR body.** `s281-plant-binds-again` is one commit, one file and one hunk
  on the final head: the fast path's case test is dropped, so the bare name binds again.
- **The snapshot sets: a miss, caught by the first linux take.** The prediction named the
  lexer's and parser's per-file snapshots and the lowering ledger; it missed `wolf_parse`'s
  `ambiguity_trees`, whose `ambiguity_corpus_tree_shapes` snapshots the parse tree of every file
  in `corpus/grammar/`. The first record at `0fa55804` (hasu block A) went red there and only
  there: `END test exit=101 secs=3077`, the one failure `snapshot assertion for
  'expr_tree__match_nested_bare_variant' failed in line 34` (`hasu/red-linux-0fa55804.log`; the
  `lane_exit` and `wolf_query` panics before it are passing tests' own expected panics). The
  same assertion fails on nomad at `0fa55804` before the update (`nomad/mac-job5-0fa55804.log`).
  `9d7cd68c` commits the eight snapshots, one per grammar row: each arm parses as a `PathPat`
  whose payload sub is an `IdentPat` (`K(Drink)`), a `PathPat` (`Out(In(Drink))`) or an `OrPat`
  (`K(Drink | Food)`), with no diagnostics, which is the point: the parser cannot tell a binding
  from a variant, and the type decides. Because `cargo test --workspace` stops at the first red
  binary, the crates after `wolf_parse` never ran in that take; `nomad/mac-job6-9d7cd68c.log`
  runs them with `--no-fail-fast` and insta's update off, beside the whole `wolf_parse` suite and
  `wolf_driver`'s corpus-walking binaries.

## Evidence index

`notes/s281-evidence/index.txt` lists every file:
- `probe/`: the 24 probe programs (`.lu.txt`; each its own directory, so each is its own module);
- `nomad/mac-job1-5f1f1c94.log`: the base; every probe on the checked machine, native, release
  and lupin, and the issue's two programs on the published 0.2.36;
- `nomad/mac-job2-7afb039b.log`: the rows at the base's code; the nine rows on every machine,
  `four_machine_regressions` 9 passed and 4 failed by name, the corpus step `4 bad` by name, and
  the snapshots generated;
- `nomad/mac-job3-7818bfcf.log`: the same at the fix; every row green, 13 passed, `0 bad`, and
  the snapshots identical to the base's;
- `nomad/mac-job4-3972d9d3.log`: every probe at the head, `cargo fmt --all --check`, clippy on
  `wolf_wir` and `wolf_driver` (all targets, `-D warnings`), the corpus snapshot tests, the
  `wolf_wir` suite and the release-parity row;
- `hasu/red-linux-0fa55804.log` (and its `.side.log`): the first linux record, at `0fa55804`,
  red at the test step by the missing parse-tree snapshots;
- `nomad/mac-job5-0fa55804.log`: `ambiguity_trees` red at `0fa55804` on nomad, then the eight
  snapshots made with insta's update;
- `nomad/mac-job6-9d7cd68c.log`: at `9d7cd68c`, insta's update off and `--no-fail-fast`: the whole
  `wolf_parse` suite, `wolf_driver`'s corpus-walking binaries with the four-machine rows and the
  release-parity row, and every crate the first take's test step never reached (`wolf_pkg`,
  `wolf_query`, `wolf_rt`, `wolf_sema`, `wolf_span`, `wolf_wir`, `xtask`);
- `rig/`: the scripts as run (the Mac lock, build-at, the jobs, the probe runner, the plant's
  record scripts, and hasu's gauntlet scripts, adapted by copy from s277's). The org's gauntlet
  recorder is copied in, not committed.

The records at the final head live outside the tree, under the lane's directory, because a
record cannot predate the head it names. The PR body quotes every `END` line:
- the linux record: `cargo xtask ci`'s steps in its order, in four chunks (prep, lint, test,
  rest), each one take of a hasu block's two locks, in `localhost/r41-toolchain:1` with a
  foreign pid 1 and the taskset shim, every `*_REQUIRE` set, and every `ci/*-test.py`;
- the plant record (nomad, one take of the Mac lock): the build and `four_machine_regressions`,
  red by name as the four rows that were red at the base.

## Done-when

Commits:
- `11391f86`: the base measured (`notes/s281-evidence`).
- `3a7bc845`: the prediction, before the first change.
- `43185b2d`: the four rows red at the base.
- `e3edd9f2`: the five control rows.
- `7afb039b`: the nine rows in `four_machine_regressions` (the checked machine, native, release,
  lupin).
- `7818bfcf`: the fix, `pattern_shape`.
- `da68bdc6`, `c029948f`, `3972d9d3`: the lexer's and parser's snapshots of the nine rows, and the
  lowering ledger (the nine rows lower).
- `d49fb950`, `0fa55804`: the evidence and this note (the first linux take's head).
- `9d7cd68c`: the parse-tree snapshots of the eight grammar rows (`ambiguity_trees`), red at
  `0fa55804` in the first linux record.
- The evidence of the red take and the note's account of it.

R-list, recommendations implemented:
- **R-s281-1, the spec: (A) no `spec/` change**, implemented. The spec is not silent:
  `[gram.pat]` recurses, `[gram.pat.nullary]` is a `closed_pattern` alternative at any depth, and
  the checker and lupin already read the nested bare name as a test. (B), for a spec lane that
  takes R-s281-2: one sentence after `[gram.pat.nullary]`'s, "At any depth (a payload slot, a
  tuple element, a struct field) the type of the value that position matches says which enum a
  bare name belongs to."
- **R-s281-2, binders: (C) no change in this lane.** `let (Drink, n) = …`, `let P { k: Drink, n }
  = …` and `for (Drink, n) in …` bind a name that spells a case of its own type, silently, on all
  four machines, while E0806's text calls a variant refutable in a binder. (A) the checker
  refuses it (E0806), with a lupin mirror; (B) W0801 warns there too. Either is a checker and
  lupin pair, not the lowering, and the machines agree today.
- **R-s281-3, `select` arms: (A) a follow-on checker and lupin lane** refuses a bare case name
  in a select arm (E0806: a committed receive cannot fail to match); (B) no change. Today
  `Drink from ch` binds on every wolf tier and lupin refuses it at run.
- **R-s281-4, `K(Drink) | K(Food)`: (B) leave it with the join-params work**
  (`grammar/match_arm_or_over_product.lu` holds that line). (A) would lower binding-free
  or-alternatives over products here. The refusal is loud, never a wrong answer, and the shape
  is a capability, not this bug.

Test targets:
- [x] the prediction committed before the first change (`3a7bc845`)
- [x] the four rows red at the base by name on native (the corpus step, `four_machine_regressions`)
      and release (the probes), green on the checked machine and lupin (`nomad/mac-job2-7afb039b.log`)
- [x] every row green on all four machines at the fix; the corpus step `0 bad` (`nomad/mac-job3-7818bfcf.log`)
- [x] the sweep's probes at the head; fmt, clippy, the snapshot tests, `wolf_wir`, release parity
      (`nomad/mac-job4-3972d9d3.log`)
- [x] the first linux record at `0fa55804`, red at the test step by `ambiguity_trees` alone
      (`hasu/red-linux-0fa55804.log`); the snapshots red first and made on nomad
      (`nomad/mac-job5-0fa55804.log`)
- [x] `wolf_parse` whole, `wolf_driver`'s corpus-walking binaries and the crates the red take
      never reached, at `9d7cd68c` (`nomad/mac-job6-9d7cd68c.log`)
- [ ] the linux record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once
- [x] README read, current

Issues: wolf-lang#1043 closes by hand at the merge (no closing keyword here). To file, for the
orchestrator: lupin 0.1.58 cannot construct a lowercase enum variant (`Mode.slow` "does not
resolve"; wolf runs it on every tier), and the follow-ons R-s281-2 and R-s281-3 name.
