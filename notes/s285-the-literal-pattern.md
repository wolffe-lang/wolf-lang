# s285 — a literal pattern outside its scrutinee's type is E0415 (wolf-lang#1058)

`match v { 300 => 0, _ => 2 }` with `v: u8` ran on every tier with no diagnostic, and the tiers
disagreed. Native and release wrapped the arm constant into eight bits (300 mod 256 = 44) and
took the arm for a `u8` holding 44; the checked machine and lupin never matched. A literal pattern
is a literal, and its context is the scrutinee, so it is now **E0415** at the literal, on the
checked machine, native and release, as `v == 300` already was. The rule holds on every integer
primitive and `wrapping[T]`, at every depth a pattern admits, in every spelling, and at each end
of a range (`0..256` over a `u8` included; every `u8` is `0..=255`). Two neighbours were fixed
with it. A `u64` pattern above `i64::MAX` never matched on the checked machine, and native refused
its decimal spelling; both now match it. A signed literal pattern, which the grammar has never
admitted, is now refused by name ("a literal pattern has no sign"). The native lowering refuses an
arm constant beyond its type instead of wrapping it, as a backstop. The spec gains
`[gram.pat.lit]`. lupin's mirror is the wolf-interp change of the same name.

## Forbidden

Footprint: cross-cutting

The diff touches `spec/` (`01-grammar.md`, `anchors.json`) and the WIR lowering, both of which
the lane-audit map names cross-cutting. The rest is the checker, the parser, the checked machine,
the registry's prose, corpus rows and their snapshots, the generated diagnostics catalog, tests,
the changelog and notes. No runtime, CI, pin or toolchain change. Clean room: the spec, the
compiler's own source, and black-box runs of the published lupin 0.1.59 and 0.1.60; no other
compiler's source. No label, merge, tag, release, issue filed or closed, and no attribution trailer. The plant
is pushed without a PR. Nothing ran under `/tmp` or `/var/folders`. No commit was made on the Linux
host.

One change to what existing programs mean, decided for this lane: a literal pattern outside its
scrutinee's type, a range end outside it, and a pattern outside a defaulted `i32` are now
refused. No program in the corpus, in wolf-std or in the org's other wolf repositories writes one
(below).

## Inputs, verified

- Base: wolf-lang trunk `a4d2a9c9`, where the lane was developed and measured. The base binary is
  a clean debug build of that commit, copied by value (sha256 `a00fd0c3…`); every later build ran in
  a worktree pinned to a commit (`cargo build --locked -p wolf_driver -p wolf_rt`, Rust 1.97.1,
  arm64 macOS). The branch was then rebased once onto trunk `9a304544` (wolf 0.2.38, `95ae0827`,
  plus one notes commit), below.
- lupin 0.1.59, the release archives: `aarch64-apple-darwin` (sha256 `9f091af8…f5f5`) and
  `x86_64-unknown-linux-gnu` (`12121909…3a18`), each read twice and equal to the release page;
  `lupin --version` names pin `b16353e`, the `PAIRING` stamp at the base.
- lupin 0.1.60, the `PAIRING` stamp at the rebased trunk: `aarch64-apple-darwin` (sha256
  `f3cae16f…6bb8`) and `x86_64-unknown-linux-gnu` (`e7d4c4f1…637e`), each read twice and equal to
  the release page; `lupin --version` names pin `5f1f1c9`. It answers every row of this lane as
  0.1.59 does: the five E0415 rows with the never-matching exit, the signed row by running it, the
  edge row correctly.
- wolf-std at the base's `STD-PIN`, `5b3a5994`, and at its trunk, `d4a4d4ca`, which is the rebased
  trunk's `STD-PIN`.
- wolf-lang#1058 whole, and #1059–#1062 (the same family, not fixed here).
- The spec: `[type.numlit]` (its kind, adoption, default and value clauses), `[gram.pat]`,
  `[gram.pat.range]`, `[gram.lex]`'s `INT`, and E0415's explanation.
- The compiler: the checker's `literal_pattern` and `range_endpoint` unify a pattern's
  `{integer}` var with the scrutinee (`Reason::Pattern`), but `check_literal_fit` walked only
  `LiteralExpr` and `PrefixExpr` nodes, so a pattern's literal was never ranged. The native
  lowering wraps every arm constant into the discriminant's width (`wrap_test`, s74), so an
  unfitting one tested a different number. The checked machine compared a pattern's text read as
  an `i64` against the value (`parse_int_literal`), so it never matched an unfitting one, nor a
  `u64` above `i64::MAX`. The parser's `pattern_atom` has no arm for `-`, so a signed literal fell to
  the arm's generic "expected a pattern".
- README read, current: it states nothing about patterns, E0415 or the counts this change moves.

## Prediction

Committed before the first change (`d3337284`, `notes/s285-prediction.md`), after the base was
measured (`4a64fd54`). Scored:

- **The base: hit.** wolf-lang#1058 reproduces on all four machines, and so does every narrow
  primitive at max+1, `wrapping[T]`, every position and spelling; the 64-bit max+1 is refused
  natively; range ends diverge (`notes/s285-evidence/base-a4d2a9c9.log`).
- **The census: hit, none written.** 470 candidates in 7,899 public files (and one private
  consumer's 34 files, none), each read with its scrutinee: no literal pattern outside its type,
  no signed literal pattern, no exclusive range end at a type's max+1.
- **R-s285-1, (A) with E0415: as recommended.** E0415 at the literal on the three compiled tiers,
  with the type's range as the label and `[gram.pat.lit]` in the note. The rendered texts are the
  reviewed snapshots
  (`crates/wolf_sema/tests/snapshots/pattern_diagnostics__e0415_*.snap`).
- **R-s285-2, range ends: as recommended.** Each out-of-type end is reported at the end;
  `0..256` over a `u8` names `0..=255` in its note.
- **R-s285-3, the 64-bit unsigned edge: hit.** The edge row (every primitive's maximum and zero;
  `u64` and `uint` above `i64::MAX` in decimal, hex, at `2^63`, over `wrapping[u64]`, nested) is
  green on all four machines. Ranges with ends above `i64::MAX` stay refused as unsupported on both
  compiled machines (probe `q03`), unchanged; an issue to file.
- **R-s285-4, the signed literal: hit.** E0207 "a literal pattern has no sign" at the signed
  literal (`-1`, `-129`, in a tuple, as a range end), the guard spelling in the note, and the arm's
  `=>` still found: one report per signed literal, no cascade.
- **R-s285-5, a literal scrutinee: hit.** Both shapes are E0415 "does not fit `i32`", and the note
  says the `i32` is the default's. To say so, the checker now records the var roots its defaulting
  pass types (`defaulted_ints`).
- **R-s285-6, lupin here: as recommended.** The pinned lupin answers each E0415 row with the
  never-matching exit, the signed row by running it (`exit(2)`; it matches `-1` against an `i8`
  holding -1), and the edge row correctly. The rows hold it to those answers until the mirror
  moves the pin.
- **R-s285-7, the backstop: a miss, then fixed.** The first backstop ranged the parsed `i64`. Over
  an `i64` or `int` scrutinee, it would have passed `9223372036854775808`, read as u64 bits, and
  a hex `0xffffffffffffffff`, and wrapped them. No program reaches the backstop while the
  checker holds, so no row could show it; it was found by tracing the plant through the
  lowering, after the takes at `dce304f1` and `ece73727` had run the first version and before
  the Linux record. The backstop now ranges the literal's exact value from its text against the
  scrutinee's whole type (`2927ac88`), with a unit test of every edge
  (`pattern_literal_tests`, `e340bf25`).
- **The rows: hit.** At the rows' commit (`242a7d66`, the base's code): the anchors gate `OUT OF
  SYNC`; `literal_pattern_lanes` 0 passed, 7 failed by name; the three rendered snapshots missing;
  the corpus step `1279 file(s), 7 bad`, six files by name (the signed row passes there, its code
  and phase unchanged, as predicted); conformance names the unknown anchor in all seven rows
  (`mac-red-ad926cb6.log`). At the fix: 7 passed, spans exact on the checked machine, native and
  release; the corpus step `0 bad`, the fail-pin ledger 388 → 393 rules (the five E0415 rows);
  conformance green (`mac-fix-62ed1d00.log`, `mac-fix-ad66c02d.log`).
- **What must not move: hit.** Every test that walks the corpus with snapshots was run with
  updates on (`mac-sweep-06a6364a.log`). Only the new rows' snapshots appeared, and the lowering
  ledger gained exactly the seven new rows. Two counts moved by design: the diagnostics catalog's
  E0415 prose and fixtures, and `corpus_parse`'s syntax-tier fail count, 38 → 39 (the signed row).
- **The corpus, wolf-std and the org's repositories: hit, nothing moves but this lane's rows.**
  Each `.lu` file of the corpus and of 31 public checkouts was run at the typecheck rung on the
  base binary and on the head's, and the two runs compared file by file
  (`mac-measure-ceb23098.log`). The checkouts are the org's wolf repositories at their trunks,
  wolf-std at the `STD-PIN` and at its trunk, wolf-interp and yew: 7,899 files in 4,926 runs, one
  per directory module plus each file with its own `check:` header. Five verdicts moved, all of
  them this lane's E0415 rows, `pass` to `fail(E0415)` at the predicted spans. Six diagnostic sets
  moved: those five, and the signed row's four E0207s, which now span the whole signed literal.
  No other file in any tree moved, and no tree outside the corpus gained an E0415 or an E0207. One
  private downstream consumer was measured the same way: 34 files in 23 runs, nothing moved.
- **A miss, of formatting:** two `rustfmt` lines (`676b607a`, `b1390f5a`).
- **A miss, of the rig:** the first org measurement ran every file at the typecheck rung. Every
  member of a directory module re-typechecks the whole module (about 40 s a file in yew), so one
  pass would have held the Mac build lock for about an hour. It was stopped by its own pids
  part-way through the base pass, and its output is void. It ran again with one run per directory
  module, plus each file that has its own `check:` header (`rig/measure-org.py`).

## Evidence index

`notes/s285-evidence/index.txt` lists every file:
- `base-a4d2a9c9.log`, `probe/`: the base, every probe on four machines;
- `census-litpat.txt`, `census-scrutinee.txt`: the census before any code, and each candidate's
  scrutinee;
- `mac-red-ad926cb6.log`, `mac-regen-ad926cb6.log`: the rows and the anchors gate red at the
  base's code, and the anchor indexed;
- `mac-fix-62ed1d00.log`, `mac-fix-ad66c02d.log`: the fix, before and after the backstop's
  rework: the rows, the tests, the corpus, conformance, every probe, fmt, clippy, the suites;
- `mac-sweep-06a6364a.log`: every corpus-walking snapshot suite with updates on, and the
  catalogs;
- `mac-measure-ceb23098.log`: the org-wide measurement on both binaries, and the gates shard as
  a macOS shakedown;
- `mac-rebased-744d7a9b.log`: the rebased head's code with lupin 0.1.60: the rows, the tests, the
  corpus, conformance, every probe, fmt, clippy, the suites, and every corpus-walking snapshot
  suite against the committed snapshots;
- `rebase-map.txt`: each commit before the rebase and the commit it became;
- `rig/`, `rig/linux/`: the scripts as run.

The branch was rebased once, onto trunk `9a304544`: wolf 0.2.38 (`95ae0827`), which moved the
`PAIRING` stamp to lupin 0.1.60 and the `STD-PIN` to `d4a4d4ca`, plus one notes commit. The rebase
was clean, with one exception: 0.2.38 had cut the changelog's Unreleased section, so git put this
lane's entry inside 0.2.38's section, and `c765d5e7` moves it under a new Unreleased heading.
Every log before `mac-rebased-744d7a9b.log` ran before the rebase and is named by the commit it
ran at; `rebase-map.txt` gives each one's rebased commit, which has the same subject and the same
change. The note names rebased commits throughout.

The records at the final head live outside the tree, because a record cannot predate the head
it names. The PR body quotes every `END` line of the Linux gauntlet record and of the plant
record.

## Done-when

Commits:
- `4a64fd54`: the base measured; `d3337284`: the prediction, before the first change.
- `e423806c`: `[gram.pat.lit]` and the range sentence (the anchors gate red here).
- `ee94084a`, `d7a54b31`, `bbf5ddf7`, `548b8375`, `1398a66a`, `b0b25614`, `0245e15a`: the seven rows,
  red first.
- `4d51ec67`: the rows on four machines (`literal_pattern_lanes`); `242a7d66`: the rendered E0415
  snapshots' tests.
- `8c4e86a5`: the checker ranges literal patterns and range ends (E0415).
- `39efc6fc`: the parser refuses a signed literal pattern by name (E0207).
- `df8ccc08`, `b4871ee7`: a `u64` pattern above `i64::MAX` on the checked machine and natively.
- `5844b922`, `2927ac88`, `e340bf25`: the lowering backstop, its exact-value ranging, its test.
- `893d745c`: E0415's explanation. `dce304f1`: the anchor indexed.
- `86745f99`: the changelog.
- `676b607a`, `b1390f5a`: formatting. `c1d3a4b7`, `07a211e2`, `3d592f27`, `ece73727`, `61bd988e`:
  the snapshots and the lowering ledger. `ef52b6e9`: the catalog regenerated. `d8f746c7`: the
  syntax-tier count.
- `c765d5e7`: the changelog entry under Unreleased after the rebase; `6fdd17f2`, `744d7a9b`: the
  rows and the changelog name lupin 0.1.60, the new pin.
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`d3337284`)
- [x] the anchors gate red at the clause alone, green at the index (`mac-red-ad926cb6.log`,
      `mac-regen-ad926cb6.log`: one anchor added, none removed)
- [x] the rows red at the base's code, by name, in the corpus step, in conformance and on every
      machine (`mac-red-ad926cb6.log`)
- [x] the rows green at the fix, spans exact on the checked machine, native and release; the
      corpus `0 bad`; conformance green
- [x] fmt, clippy (`wolf_sema`, `wolf_parse`, `wolf_wir`, `wolf_mem`, `wolf_driver`, all targets,
      `-D warnings`), the `wolf_sema` and `wolf_parse` suites, the backstop's unit test
- [x] every corpus-walking snapshot suite: only the new rows' snapshots
- [x] the corpus, wolf-std and the org's wolf repositories on the base and head binaries: only
      this lane's rows move (`mac-measure-ceb23098.log`)
- [x] the gates shard on macOS, a shakedown, not a record: every step exit 0
      (`mac-measure-ceb23098.log`)
- [x] at the rebased head's code, with lupin 0.1.60: the rows, the tests, the corpus, conformance,
      fmt, clippy, the suites, every corpus-walking snapshot suite with no update
      (`mac-rebased-744d7a9b.log`)
- [ ] the Linux gauntlet record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once

Issues filed, each with its probe and output (linked from the PR, no closing keyword):
- wolf-lang#1066: a range pattern whose end lies above `i64::MAX` over a `u64` or `uint` is refused
  as unsupported on the checked machine ("this range pattern's end") and, for decimal ends,
  natively ("this literal range end"), where lupin matches (probes `q03`, `r03`–`r05`).
- wolf-interp#349: lupin accepts a signed literal pattern, which the grammar does not admit, and
  matches it (`match v { -1 => 0 }` over an `i8` holding -1 exits 0, probe `q01`); the mirror
  refuses it by name.
- wolf-interp#350: lupin lets an integer literal adopt a `distinct` integer type
  (`let c: Col = 300` with `type Col = distinct u8` runs, and so does a literal pattern over a
  `Col`), where the compiler refuses both as E0401 (probes `q08`, `q09`).
