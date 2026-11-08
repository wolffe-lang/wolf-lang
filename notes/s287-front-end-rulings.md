# s287: the front-end rulings on the second compiler's spec silences

The second wolf compiler is written from the specification alone. In six places it found the
specification silent, or the compiler departing from a clause that speaks. Each place is now a
clause with its own anchor, rows committed red before the fix, and a front-end fix, so one change
holds the checked machine, native and release:

- **wolf-lang#1064:** an error-set alias is checked where it is declared, used or not. An unused
  `error E = {E}` is E0610, an unused two-alias loop is one E0610, and an alias's own repeated tag
  is E0601 (`[type.err.alias.cycle]`, amended).
- **wolf-lang#1059:** a nullary arm pattern binds nothing (`[gram.pat.nullary.scope]`), so writing a
  `let` through its name is E0410. This covers a payload-less variant, a row tag and a field write.
- **wolf-lang#1060:** a variant or a field declared twice is E0302 (`[gram.item.type.names]`).
- **wolf-lang#1062's first case and #1073:** a name bound twice in one pattern, or a parameter
  declared twice, is the new E0827 (`[gram.pat.names]`).
- **wolf-lang#1065:** an alias's tags merge with the row's own in any order, and a payload that
  disagrees is E0609 (`[type.err.alias.union]`, amended). Only a row's own spellings are held to
  naming a tag once, now written down as `[gram.type.row.dup]`.
- **wolf-lang#1061, the two refusals:** an or-pattern's alternatives bind the same names (the new
  E0828) at one type (E0401) (`[gram.pat.or]`). The compiled machines' join of a binding
  or-pattern is a change of its own; until it lands they refuse such an arm at `mem` as "cannot
  compile this yet", which is the conservatism ledger and not a rule.

#1062's other two cases need a census first: a type that contains itself by value, and the
mutability of a pattern's bindings. They follow in a second change from this one's head. lupin's
mirror is the wolf-interp change of the same name ("For the lupin mirror", below).

## Forbidden

Footprint: cross-cutting

The diff touches `spec/` (`01-grammar.md`, `10-types.md`, `anchors.json`), which the lane-audit map
names cross-cutting. The rest of it:
- the checker, the item collector and signature elaboration (`wolf_sema`);
- the code registry's prose and two new codes (`wolf_diag`);
- a four-machine test (`wolf_driver`);
- corpus rows and their lexer, parser and lowering-ledger snapshots, and rendered diagnostics;
- the generated diagnostics catalog, the changelog and notes.

There is no runtime, backend, CI, pin or toolchain change. Clean room: the spec, the compiler's own
source, and black-box runs of the published lupin 0.1.60, and no other compiler's source. No label,
merge, tag, release, issue closed, and no attribution trailer. One neighbour found while probing
was filed apart, as asked: wolf-lang#1076, a closure's unmarked parameter that can be assigned.
The plant is pushed without a PR. Nothing ran under `/tmp` or `/var/folders`, and no commit was made
on a Linux host.

What existing programs mean, by decision: every refusal above is new on the compiled tiers. The
census below found no program in the corpus, in wolf-std or in the org's other wolf repositories
that any of them moves, other than this change's own rows.

## Inputs, verified

- **Base:** wolf-lang trunk `d848fbed`. The base binary is a clean debug build of that commit,
  copied by value (sha256 `4781f945…`). Every later build ran in a worktree pinned to a commit
  (`cargo build --locked -p wolf_driver -p wolf_rt`, Rust 1.97.1), on one of two arm64 macOS
  machines. One take was queued as one commit and built another,
  because a shared build worktree moved under it. Its log names the commit it built, and the rig
  now gives each queued job its own worktree (Evidence index).
- **The rebase:** the branch was developed and measured on `d848fbed`, then rebased once onto trunk
  `718fe47a`, which adds s286's change: the runtime settles glibc's arena limit (wolf-lang#1051).
  This change's first Linux record, at the first head, met that race at its test step
  (`signal_gaps_lanes`, #781's row) and was voided, since the fix had to land first. The rebase was
  clean but for the changelog, where this entry now sits above s286's (`9d79c7e2`). Every log but
  `m1-rebased-5fb13d74.log` ran before the rebase and is named by the commit it ran at, and
  `notes/s287-evidence/rebase-718fe47a.txt` gives each one's rebased commit, with the same subject
  and the same change. The note names rebased commits throughout. At the rebased head the checks,
  the four-machine test and every corpus-walking snapshot suite ran again.
- **lupin 0.1.60**, the `PAIRING` stamp: `aarch64-apple-darwin` (archive sha256 `f3cae16f…6bb8`)
  and `x86_64-unknown-linux-gnu` (`e7d4c4f1…637e`), each read twice and equal to the release page.
- **The issues** wolf-lang#1059, #1060, #1061, #1062, #1064, #1065 and #1073, whole, and s285's
  change as the precedent.
- **The spec:** `[gram.pat]`, `[gram.pat.nullary]`, `[gram.pat.binder]`, `[gram.pat.struct]`,
  `[gram.item.type]`, `[gram.item.let]`, `[gram.type.row.flatten]`, `[type.err.alias]` whole,
  `[proto.record.phase]` and `[proto.cmp.rung]`.
- **The compiler, as found:**
  - **E0410** is the resolve rung's (`letcheck.rs`), lexical. It reads a bare arm name as the arm's
    binding, so a write through an arm that tested the name was invisible to it. The checker, which
    decides test or binding by the scrutinee's type (`match_pattern`), never tracked `let`.
  - **The item table** (`graph.rs`) checked duplicate items only.
  - **The checker's `bind`** let a later binding of a name shadow an earlier one in the same
    pattern. Or-pattern alternatives bound into one scope with no comparison.
  - **Signature elaboration** expanded an alias only where a row named it (`lower_row_parts`),
    checked a later spelling against an earlier alias's tags (E0601), and reported a loop once per
    use.
- **README** read, current: it states nothing these rules change.

## Prediction

Committed before the first change (`2041a39c`, `notes/s287-prediction.md`), after the base was
measured (`5a04f01f`). Scored:

- **The base: hit.** Every case on the ruling page and every neighbour answers as predicted on all
  four machines (`notes/s287-evidence/base-d848fbed.log`).
- **#1064: hit.** E0610 at 11..12 for the unused self loop, one E0610 at `A` inside `B`'s row for
  the pair, E0601 for the unused alias's own duplicate. The used loop keeps its one report at its
  span.
- **#1059: hit,** with the row tag and the field write as rows. E0410 at the assignment on the three
  compiled tiers, at the typecheck rung (R-s287-2). The `var` control runs.
- **#1060: hit.** E0302 at the second declaration in all four spellings; `a` and `A` stay two.
- **#1062a and #1073: hit.** E0827 at the second binding in every position the prediction named,
  for both parameter lists. `(Red, Red)` and the comma group stay legal.
- **#1065: hit.** `{A, x}` runs. E0609 sits at the row's own spelling, and at the later alias entry
  for two aliases. `{A, x, x}` moves its E0601 to the second `x`. A miss of wording, fixed:
  E0609's text spoke of "both layers of this nested error row" for an alias, and an alias that
  meets its row now has its own wording (`164aa799`).
- **#1061's refusals: hit.** E0828 at the alternative that lacks the name, on either side, and E0401
  at the later binding. The agreeing, nested and `@` rows pass the checker and stay refused at
  `mem` by the ledger, as predicted.
- **The rows red first: hit.** At the rows' commits, on the base's code for each item
  (`notes/s287-evidence/m2-red-*.log`):
  - `04458e55`, #1064: `front_end_rulings_lanes` 3 failed by name, 1 passed (the used loop, the
    control); the corpus step `3 bad`.
  - `a0d0844d`, #1059: 3 failed by name, 5 passed (the four above and the `var` control); `3 bad`;
    the anchors gate `OUT OF SYNC`; conformance names the unknown anchor.
  - `a95fb217`, #1060, #1062a, #1065 and #1061 together: 18 failed by name, 14 passed (the eight
    above and the six controls); `20 bad` in 17 files, each by name; the anchors gate and
    conformance red.
- **The census: hit, nothing moves.** Every `.lu` file of the corpus and of 38 org checkouts at
  their trunks ran at the `mem` rung on the base binary and on this head's (`aa3c9baa`'s, built
  before the rebase: the code this head carries but for E0609's wording and trunk's runtime fix), and the two were compared run by run
  (`notes/s287-evidence/census-pr1.txt`). That is 5,080 runs over 6,839 files. Outside the corpus,
  3,086 of 3,771 runs reached the checker. The other 685 stop at resolve on both binaries: a module
  that is not a root, or a test whose sources a harness stages. Their code is measured through the
  roots that import it. No verdict and no error moved in any public tree, nor in the seven private
  downstream trees (58 files). In the corpus, the 33 new rows moved, and so did `dup_tags.lu`'s
  span, by the 19 bytes its new `conforms:` entry added.
- **Anchors: hit.** 726 keys at the base, 731 here (five added, none removed; the key sets diffed
  both ways at each regeneration). The two amended clauses keep theirs.
- **A miss of the rig:** the catalog's fixture rule wants a rendered snapshot for each new code. The
  first snapshot of the arm E0410 went through `let Done`, whose W0801 the helper's resolve-clean
  assertion refuses. It renders through a row tag instead (`cf501a14`).

## Rulings

Every question on the ruling page was ruled A. The lane's own:

- **R-s287-1** (the second change's): #1062c lands as E0410 unless the census moves downstream code.
- **R-s287-2:** #1059's E0410 is decided by type, in the checker, not by scope at the resolve rung.
  The page's "one resolver fix" was a sketch, not a constraint. Scope misses row tags and enums
  reached through a module path, and would refuse a bare name that spells an in-scope variant the
  scrutinee's type does not have. Same code, same span; `[proto.cmp.rung]` lets the records agree
  across rungs.
- **R-s287-3:** E0827 and E0828 live in the checker for the same reason: only the type tells
  `(x, x)` from `(Red, Red)` in an arm. Binder positions and parameter lists use the same check, so
  one code has one rung.
- **R-s287-4:** #1073 folds in under E0827, which covers function and closure parameters.
  wolf-lang#1074 (a generic parameter declared twice) waits: generics are a type namespace.
- **R-s287-5:** #1064's walk runs in signature elaboration, once per `error` declaration, before any
  row that names one. E0610 keeps its rung.
- **R-s287-6:** a loop is reported once, at the entry that closes it in declaration order.
- **R-s287-7:** an alias's own row is checked where it is declared, used or not.
- **R-s287-8:** E0609's site is the row's own spelling, or the later alias entry when two aliases
  disagree.
- **R-s287-9** (the second change's): the by-value cycle walk and its limits.

## For the lupin mirror

lupin 0.1.60 accepts, and the mirror must refuse, with the compiler's code at the compiler's span
(the rows in `crates/wolf_driver/tests/front_end_rulings_lanes.rs` hold lupin to its present
answer until the pin moves, and switch to the code by themselves when its record carries it):

- **E0410** for a write to a `let` through an arm whose bare name tested a variant or a row tag, the
  field write included (`typecheck/nullary_arm_*_assign.lu`; lupin runs them, exit 9);
- **E0302** for a variant or a field declared twice, in all four spellings
  (`resolve/enum_variant_twice*.lu`, `struct_field_twice.lu`, `type_decl_member_twice.lu`);
- **E0827** for a name bound twice in one pattern, in every position, and for a parameter declared
  twice in a function or a closure (`typecheck/pattern_name_twice_*.lu`, `param_name_twice*.lu`);
- **E0828** for an or-pattern alternative that lacks a name, and **E0401** for one name at two types
  (`typecheck/or_pattern_binds_missing*.lu`, `or_pattern_binds_types.lu`); lupin already runs the
  agreeing rows;
- **E0601** for a tag a row spells twice, after an alias too (`rows/negative/dup_tags.lu`,
  `error_row_own_dup_bare.lu`, `error_alias_union_own_twice.lu`), and for an unused alias's own
  repeated tag (`rows/negative/error_alias_own_dup_unused.lu`);
- **E0609** for a payload that disagrees with an alias's, at the row's own spelling, and between two
  aliases at the later entry (`rows/negative/error_alias_union_*_payload.lu`).

lupin already gives E0610 for an unused alias loop, and needs nothing for it.

## Evidence index

`notes/s287-evidence/index.txt` lists every file.

## Done-when

Commits:
- `5a04f01f`: the base measured; `2041a39c`: the prediction, before the first change.
- #1064: `4bff47f5` (the amended clause); `f3001a16`, `f0f8727f`, `4b8cd114` (the rows);
  `04458e55` (the rows on four machines); `80989e45` (the declaration walk); `dc0b7f79` (E0610's
  explanation).
- #1059: `3d44aeb8` (`[gram.pat.nullary.scope]`); `1d9f6bc2`, `9355a0e3`, `cca0fc55`, `35640dec`;
  `a0d0844d`; `faeb4ca7`, `25cade0b` (the shared E0410 report, the checker's E0410); `ca1e0d92` (the
  anchor); `ef4ac885` (E0410's explanation).
- #1060: `c7625b95` (`[gram.item.type.names]`); `3dade993`, `e67087e3`, `5d920d63`, `978c39c2`,
  `e408fb66`; `19c83dc6`; `7b392c6d` (the item collector's E0302).
- #1062a and #1073: `c749ecf1` (`[gram.pat.names]`); `30c01a60` (E0827); `80bd8258`, `bb99f03f`,
  `5fe5f93b`, `9bf48ef9`, `34f0ada2`, `b015231e`, `2ef6a95e`, `dbf586bc`, `5b3ee488`, `3918ba26`;
  `f5cccd21`; `92adfb43` (the checker's name frames).
- #1065: `f292bb00` (`[gram.type.row.dup]`), `c9b9df7d` (the amended `[type.err.alias.union]`);
  `756b69c9`, `649bc870`, `9c1c741d`, `df9adaf2`, `a31c50d4`, `31c5a7c8`; `d85cc413`; `c9527b67` (the
  merge in any order); `164aa799` (E0609's wording for an alias, and a loop a use meets first).
- #1061: `f96da717` (`[gram.pat.or]`); `be9d3290` (E0828); `dc0bc4c2`, `891be254`, `ce2dfb77`,
  `d74e9f39`, `672bc3bc`, `9403f8c9`, `4540e03b`; `a95fb217`; `1002baef` (E0828 and E0401).
- `bb7574a7`: four anchors indexed. `e15ee170`: E0302, E0601 and E0609 explained.
- `a2fc76b9`, `cf501a14`: the rendered snapshots' tests. `2c122a38`, `65dad0a9`: formatting and a
  named type. `9d79c7e2`: the changelog. `e14ac506`, `959929e9`, `1b2ad006`, `f0fc5c37`: the
  snapshots and the lowering ledger. `d154492a`: the catalog regenerated.
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`2041a39c`)
- [x] the rows red at the base's code for each item, by name, in the lanes test and the corpus
      step (`m2-red-023ae9cf.log`, `m2-red-d0f1652e.log`, `m2-red-392a98a7.log`)
- [x] the anchors gate red at each new clause, green at its index: 726 to 731 keys, none removed
- [x] the rows green at the fix on the checked machine, native and release, spans exact; lupin held
      to its present answers (`front_end_rulings_lanes`, 32 tests)
- [x] the corpus `0 bad` (1312 files; the fail-pin ledger 393 to 418 rules); conformance green
- [x] fmt, clippy (`wolf_diag`, `wolf_sema`, `wolf_driver`, all targets, `-D warnings`), the
      `wolf_diag` and `wolf_sema` suites, the catalogs' checks, the anchors gate, the corpus and
      conformance at `d154492a` (`m1-checks-49cbd28f.log`)
- [x] every corpus-walking snapshot suite: only the new rows' snapshots and the two re-tagged rows'
- [x] the census: every program of the corpus and 38 org checkouts on the base and head binaries,
      nothing moves but this change's rows
- [x] at the rebased head (`5fb13d74`, trunk `718fe47a`): the checks above, the lanes test, the
      corpus, conformance, and every corpus-walking snapshot suite with no update
      (`m1-rebased-5fb13d74.log`)
- [ ] the Linux gauntlet record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once
