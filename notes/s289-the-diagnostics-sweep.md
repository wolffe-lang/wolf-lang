# s289: the diagnostics sweep, first change: two messages that blamed the compiler

A reader typing in a tutorial met two compiler messages that blamed the compiler for a mistake in
the program. Both read as "wolf can't", where the truth was "you forgot X". This change makes each
say what is wrong, where, and what to do:

- **wolf-lang#1085:** a capitalized name that nothing declares is read as an error tag. Where no
  error row takes it, every compiled tier declined the program as "an error-row tag outside `!T`
  context … not a bug in your program", and `wolf test` counted the file as unsupported. It is
  now **E0301** at the name, as `[gram.expr.tagident]` and `[gram.expr.variant]` already said.
  The message names the readings the position allows:
  - declare or import the constant;
  - at a `return` or the tail of a function that cannot fail, `-> !T`, if an error was meant;
  - inside a fallible function, `return Name`;
  - `module.NAME`, offered as the edit, when an imported module defines the name.

  A tag the function does spell in some row, used where the function cannot fail, is **E0604**
  with the `! {tag}` fix `?` gets there.
- **wolf-lang#1083:** a module of the package that the file does not import now says so, with
  `use helper` offered after the file's imports. A module named like one of the prelude's
  provisional stand-ins (`worker`) used to resolve to the stand-in and was declined as "a
  std/prelude stub without a signature". It now misses the same way as any other unimported
  module. A stand-in that nothing defines is E0301 ("nothing defines `worker`").

The plan (`notes/s289-prediction.md`) splits the sweep into three changes, each from the one
before's head:
1. these two messages;
2. the census of every prelude stub, #1082 (which lld is picked from PATH) and #175 (`wolf add`
   writes a manifest it reads back);
3. #708, the prelude's `min` on every tier.

lupin's mirror is the wolf-interp change of the same name ("For the lupin mirror", below).

## Forbidden

Footprint: cross-cutting

The corpus rows add twelve lines to the lowering ledger's snapshot
(`crates/wolf_wir/tests/snapshots/lower_corpus__corpus_lowering_ledger.snap`), a path the
lane-audit map names codegen. The rows stay in the corpus, where lupin reads them (R-s289-11). No
WIR, backend or runtime code changes. The rest of the diff:
- the checker and the resolver (`wolf_sema`): the two reports, the import note, and the
  package's set of importable module names (`graph.rs`);
- the code registry's prose for E0301 and E0604, and the regenerated catalog;
- a four-machine test, three rendered snapshots, corpus rows and their lexer and parser
  snapshots, the changelog and notes.

There is no spec, CI, pin or toolchain change. Clean room: the spec, the compiler's own source,
and black-box runs of the published lupin 0.1.60, never another compiler's source. #1085 was
filed by this lane, as the contract asked, before the prediction. No label, merge, tag, release or
issue closed, and no attribution trailer. The plant is pushed without a PR. Nothing ran under
`/tmp` or `/var/folders`, and no commit was made on a Linux host.

What existing programs mean, by decision: the scan of every corpus entry on the checked machine
at the base (`notes/s289-evidence/corpus-scan-base-a57276f2.tsv`) found one entry that meets
either refusal. That is `errors.lu`, a seed program that calls an `acquire()` it never defines. It
moves from unsupported to fail(E0301) at typecheck. Its header (`phase: resolve`,
`check: run(exit=0)`) still holds under the corpus step, so the file is not edited.

## Inputs, verified

- **Base:** wolf-lang trunk `a57276f2` (0.2.38). The base binary is a debug build of that commit
  in its own worktree, copied by value (sha256 `dedaa6d8…5971e`), with its runtime beside it.
  Every later build ran in a worktree pinned to a commit (`cargo build --locked -p wolf_driver -p
  wolf_rt`, Rust 1.97.1) on one of the two arm64 macOS machines, behind that machine's build lock.
- **lupin 0.1.60**, the `PAIRING` stamp: `aarch64-apple-darwin` (sha256 `f3cae16f…6bb8`) and
  `x86_64-unknown-linux-gnu` (`e7d4c4f1…637e`), each read twice and equal to the release page.
  lupin's source was read at wolf-interp trunk `725d86da`; nothing between 0.1.60 and that commit
  touches the unresolved-tag reading or the ambient table.
- **The issues** wolf-lang#1083, #708, #1082 and #175, whole; #1085 as filed.
- **The spec:** `[gram.expr.tagident]`, `[gram.expr.variant]`, `[type.unit.discard]`,
  `[type.fn.ret]`, `[conf.resolve.ambient]`, `[os.host.sigs]`'s list of untyped ambient names.
- **The Linux record's inputs** (the musl link bundle, clef and ligature at their pins, the wolf
  0.2.36 toolchain, wolf-std at the `STD-PIN`) are staged once, outside the locks, and read twice
  against the head's pins in the prep take (`notes/s289-evidence/rig-linux/`).

## Prediction

`notes/s289-prediction.md`, committed (`878f23b3`) before the first change, with the base's
measurements (`e9343e3a`). It held for this change, with two corrections:
- **The fallible `let` row's span.** The test was written before the row's header gained eleven
  bytes. The row was red for the right reason at the rows' commit, and its span is corrected in
  `e8f8c086`.
- **The offered `use`.** A `use` declaration's span ends past its newline, so the edit was
  inserted on a blank line. It now goes on its own line (`14b94889`), and the snapshot shows it.

What each row answers, at the base and at this change, on the checked machine, native and release
(lupin 0.1.60 in the last column):

| row | base | now | lupin 0.1.60 |
|---|---|---|---|
| `typecheck/tag_undeclared_tail.lu` | unsupported | E0301, `-> !int` offered | exit 1 |
| `typecheck/tag_undeclared_assign.lu` | unsupported | E0301, no `!T` advice | exit 1 |
| `typecheck/tag_undeclared_return.lu` | unsupported | E0301, `-> !int` offered | exit 1 |
| `typecheck/tag_undeclared_let.lu` | unsupported | E0301 | exit 1 |
| `typecheck/tag_undeclared_payload.lu` | unsupported | E0301 at `Bad` | exit 1 |
| `typecheck/tag_undeclared_fallible_let.lu` | unsupported | E0301, `return Bad` offered | exit 1 |
| `typecheck/tag_undeclared_module_item/` | unsupported | E0301, `dialog.OPEN` the edit | exit 0 |
| `typecheck/tag_known_cannot_fail.lu` | unsupported | E0604, `! {bad}` the fix | unsupported |
| `resolve/stand_in_unimported/` | unsupported | E0301 at resolve, `use worker` | unsupported |
| `resolve/module_unimported/` | E0301, no note | E0301, `use helper` | unsupported |
| `typecheck/stand_in_undefined_call.lu` | unsupported | E0301 "nothing defines `worker`" | unsupported |
| `typecheck/stand_in_undefined_receiver.lu` | unsupported | the same | unsupported |
| `wolf test` over a module that forgot `OPEN` | 1 unsupported | 1 rejected, E0301 shown | — |

Controls, unchanged:
- a bare variant value keeps its E0301;
- `[type.unit.discard]`'s recorded decline keeps its words;
- `?` in a function that cannot fail keeps its E0604;
- a raise in `-> !int` keeps exit 7.

## Rulings

As recommended in the prediction and ruled:
- **R-s289-1:** an undeclared capitalized name where no row takes it is E0301. The notes give the
  readings the position allows, and the edit `module.NAME` when an import defines the name.
- **R-s289-2:** a tag the function spells, where the function cannot fail, is E0604.
- **R-s289-3:** `[type.unit.discard]`'s decline keeps its words.
- **R-s289-4:** a stand-in yields to an unimported module of its name. One that nothing defines
  is E0301. The ambient table and `wolf prelude` are unchanged.
- **R-s289-5:** every E0301 for a name that is a module of the package (a root directory of `.lu`
  files, or a dependency alias) carries the import note and `use`.
- **R-s289-10:** three changes, each from the one before's head.
- **R-s289-11:** the rows stay in the corpus, so this change is cross-cutting.

## For the lupin mirror

lupin 0.1.60 reads an undeclared capitalized name as a tag value wherever it stands, so every #1085
row runs there and exits 1 with `error: OPEN`; the compiler's E0301 is now a divergence on those
rows. The mirror is a static check in lupin's resolve rung:
- For every function, find each bare capitalized expression path that nothing resolves (no local,
  item, import, module or variant).
- It is legal only as the operand of `return` or the tail of a function whose declared result can
  fail. Elsewhere it is E0301 at the name.
- A tag the function spells in some row, used in a function that cannot fail, is E0604.

The `stand_in_*` and `module_unimported` rows are "unsupported" on lupin today, which is no
divergence; the mirror may give them E0301 too.

## Evidence index

`notes/s289-evidence/`:
- `base-a57276f2.log`: every probe under `probe/` on the four machines at the base.
- `census-base-a57276f2.tsv`: every ambient name, three ways.
- `corpus-scan-base-a57276f2.tsv`: every corpus entry's checked record at the base.
- `m1-red-f8efb6a1-air.log`: the rows' red take, at the rows' commit.
  - The four-machine test: 13 FAILED by name, the 2 controls ok.
  - The corpus step: 12 bad, each a new row by name.
  - The lexer snapshots and the lowering ledger written there.
- `m1-checks-72852da9-air.log`: the first check at the fix. 14 of 15 pass, and the fallible
  `let` row's span was wrong (above).
- `m1-checks-b93a0fe7-air.log`: the check at the fix, after the corrections.
  - The four-machine test 15 of 15; `wolf_sema`'s suites; clippy.
  - The corpus: 1,342 files, 0 bad; the fail-pin ledger 432 to 443 rules.
  - Conformance and the catalog regenerated.
  - fmt red, fixed in `63868e8a` and `87976df7`.
- `rig/`, `rig-linux/`: the scripts that ran them, and the Linux gauntlet's rig.

The Linux record of this head and the plant's record are cited in the PR.

## Done-when

Commits:
- `e9343e3a`: the base measured; `878f23b3`: the prediction, before the first change.
- The rows:
  - #1085: `f711e6cf`, `d7845f71`, `1fc94216`, `15098da7`, `9a7e52aa`, `d3e895a7`, `1ead5657`,
    `6ada31cd`;
  - #1083: `cf264165`, `b905f9dc`, `61678a51`, `72c748d1`;
  - `93e216ce` (a header's lupin note), `ee970684` (`wolf test`'s fixture);
  - `0cd0114d` (the rows on four machines);
  - snapshots and the ledger: `f8efb6a1`, `111348d4`, `b93a0fe7`.
- The fix:
  - `b4a9838c` (the package's importable names and the import note);
  - `e35a2a16` (a stand-in yields to an unimported module);
  - `2a8dc84f` (E0301 and E0604 where no row takes a tag; the undefined stand-in);
  - `14b94889` (the `use` on its own line).
- `80952746`: the registry's prose; `d906116f`: the catalog regenerated.
- `edd97748`, `1e52fe6c`: the rendered snapshots' tests and snapshots.
- `e8f8c086`: the fallible `let` row's span.
- `63868e8a`, `87976df7`: formatting.
- `72852da9`: the changelog.
- `56abd5df` and this note: the evidence.

Test targets:
- [x] the prediction committed before the first change (`878f23b3`)
- [x] the rows red at the rows' commit, by name, in the four-machine test and the corpus step
      (`m1-red-f8efb6a1-air.log`)
- [x] the rows green at the fix on the checked machine, native and release, spans exact; lupin
      held to its present answers (`diagnostics_sweep_lanes`, 15 tests)
- [x] the corpus `0 bad` (1,342 files); conformance green; the catalog regenerated
- [x] clippy (`wolf_sema`, `wolf_driver`, `wolf_diag`, `wolf_pkg`, all targets, `-D warnings`), the
      `wolf_sema` suites with the three new snapshots
- [ ] the gauntlet (`cargo xtask ci`, every step's `END`) at this head on Linux x86-64
- [ ] the plant red by name at `an_unimported_module_named_like_a_stand_in_is_e0301_with_its_use`,
      its controls green
- [ ] tier full on CI (`full-matrix`)
- [x] README read, current: it states nothing these messages change
