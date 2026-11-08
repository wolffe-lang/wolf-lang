# s287: a type that contains itself by value, and a pattern's bindings

The second change of s287, from the first's head (`notes/s287-front-end-rulings.md`). It covers
the two cases of wolf-lang#1062 that waited for a census:

- **`[type.rec]`:** a type may contain itself only through storage that is not inline. A struct's
  fields, an enum's payloads, a tuple's elements and an error union's payloads are laid out
  inline, so a type that reaches itself through them alone would have no finite size.
  - It is **E0404** where the type is declared, used or not, once per cycle, with the cycle named
    in full, at the member through which the cycle's first-declared type reaches itself.
  - A generic struct or enum is walked at its arguments: `Box[L]` holds an `L` inline, and
    `Bag[L]` over a `List[T]` does not.
  - `List`, `Map`, `Pool`, `handle`, `weak`, pointers, channels, mutexes and function types hold
    their element elsewhere. `shared` cycles stay E1006.
  - Before, the checked machine and lupin ran `enum L { Nil, Cons(int, L) }`, native and release
    refused it at `mem` as "cannot compile this yet", and every implementation accepted it while
    unused.
- **`[gram.pat.let]`:** a name a pattern binds outside `let` and `var` binds as `let` does. That
  covers a `match` arm, a `for` header, an `else |…|` handler, a `select` receive, and either side
  of `@`.
  - Assigning such a name, or a field of it, is **E0410**, and the fix is `var x = x` at the start
    of the body.
  - Before, every machine let the body write it. Under `fn bump(mut v: E)`,
    `match v { A(x) => { x = x + 1 } }` read as an update to `v` and wrote the arm's own copy.
  - The checker reports it, since only the scrutinee's type tells a bare arm name that binds from
    one that tests a case, as the first change's E0410 through a nullary arm is.

lupin's mirror is the wolf-interp change of the same name.

## Forbidden

Footprint: cross-cutting

The diff touches `spec/` (`01-grammar.md`, `10-types.md`, `anchors.json`), which the lane-audit map
names cross-cutting. The rest of it:
- the checker and a new by-value cycle check beside signature elaboration (`wolf_sema`);
- the code registry's prose;
- the four-machine test;
- corpus rows and their snapshots, one adjusted corpus witness and one adjusted `wolf_mem`
  fixture, rendered diagnostics;
- the generated catalog, the changelog and notes.

There is no runtime, backend, CI, pin or toolchain change. The clean room, the forbidden actions and
the hosts are the first change's, unchanged. The plant is pushed without a PR.

What existing programs mean, by decision: a by-value recursive type and a write to a pattern's
binding are refused on every compiled tier. The census below found no program outside the corpus
that has either. The second rule ships as an error at once, with no warning release, because the
census moved no program outside the corpus.

## Inputs, verified

The first change's inputs, plus:
- **The base of this change:** the first change's head `d59086f7`, on wolf-lang trunk `718fe47a`.
  The branch was cut from the first change's code-final commit `4945073e` and rebased onto its
  head `aac7dee6`, which added only the changelog, snapshots, the catalog, the note and its
  evidence (`notes/s287-evidence/pr2-rebase-map.txt`). Then both changes were rebased onto trunk
  `718fe47a`, for the reason the first change's note gives (`pr2-rebase-718fe47a.txt`). Every
  commit kept its subject and its change through both, but the changelog's, whose context moved.
  The red takes and every log but `pr2-m2-rebased-a7078094.log` ran before the second rebase, and
  are named by the commit they ran at; the maps name the commit each became. The note names
  rebased commits throughout. At the rebased code (`a7078094`, whose code this head carries
  unchanged; only notes differ) the checks, the four-machine test, the adjusted fixture's suites and
  every corpus-walking snapshot suite ran again.
- **The spec:** `[abi.native.enum]`, `[mem.shared.rc.2]`, `[mem.tier0.move]`,
  `[mem.tier0.mode.read]`, `[gram.pat.binder]` and E0404's and E0410's explanations.
- **The compiler, as found:**
  - No check walked a type's own members for a by-value cycle. Native refused such a value at
    `mem` ("deeply nested aggregate types"), and the checked machine laid it out recursively.
  - The checker's pattern binding (`match_pattern`, `bind_pattern`, the `select` arm) recorded
    nothing about how a name was bound, and the resolve rung's E0410 walk treats every pattern name
    as assignable.

## Prediction

The first change's prediction file (`notes/s287-prediction.md`), items 7 and 8, and the rulings
R-s287-1 and R-s287-9. Scored:

- **#1062b at the base: hit.** At the rows' commit, on the base's code (`pr2-m1-red-e36a9b29.log`):
  the enum, tuple and struct-and-enum rows are declined at `mem` natively ("deeply nested aggregate
  types"), and the unused, generic and error-union rows pass every rung. lupin runs them with the
  exits the prediction named (1, 7, 5, and 0 for the unused shapes), and the four-machine test
  holds it to them. Through `List`, `Map`, `handle` and `Bag[L]`, every machine runs the control.
- **#1062b's census: hit.** No program outside this change's rows gains an E0404, in the corpus
  or in any tree (below).
- **#1062b's rows: hit.** E0404 once each, at the member through which the cycle's first-declared
  type reaches itself, the cycle named in full (`N -> L -> N`, `L -> Box -> L`). One miss of the
  prediction's report site, fixed in the clause before the code: it put the report at the member
  that closes the cycle. For `L -> Box -> L` that member is `Box`'s own `v: T`, inside a generic
  definition that is not recursive by itself. The clause now names the member through which the
  first-declared type reaches itself (`240a12de`), and the struct-and-enum row moved from `More`
  to `next` (`a00d45c4`).
- **#1062c at the base: hit.** Every machine lets the binding be written: a match arm (exit 4), a
  `for` header (12), an `else |Bad(n)|` handler (5), a `select` receive (5), a field of an arm's
  binding (9), an `@` name (6), and the `bump` trap answers 3 with only W1002
  (`pr2-m1-red-ddceedf8.log`, where those seven rows fail by name and the `var` control passes).
- **#1062c's census: a miss of the prediction, in the direction that settles the ruling.** A
  handful of moves was predicted, mostly in the corpus and possibly in the book. At the rule's
  first snapshot one file moved, in the corpus, and nothing outside it:
  `corpus/memory/elem_loop_induction_written_mut.lu`, a witness that wrote a `for` index to show an
  exclusivity refusal (E1002 became E0410). The syntactic scan below found no other write to a
  `for` header's name.
- **R-s287-1: as ruled,** E0410 now, no warning release.
- **The rows red first: hit.** At the rows' commits, on the base's code for each item (the red
  takes ran before both rebases, at `e36a9b29` and `ddceedf8`, which became `4383a2af` and
  `f80dee12`):
  - `e36a9b29`, #1062b: `front_end_rulings_lanes` 6 failed by name, 33 passed (the first change's
    32 and the container control); the corpus step `9 bad`, all in the six rows; the anchors gate
    `OUT OF SYNC`; conformance red at the unknown anchor.
  - `ddceedf8`, #1062c: 7 failed by name, 40 passed (the 39 above and the `var` control); the
    corpus step `8 bad`, all in the seven rows (one of them the miss below); the anchors gate and
    conformance red.
- **A miss of the rows, fixed:** the `bump` row fires W1002 (its `mut` parameter is never written,
  which is the trap), and its header did not declare it. The red take's corpus step names it
  beside the row's phase, and the checks at `99fab3e8` found it alone. The row now declares
  `warns: W1002` (`6ac44337`). Its snapshots and its span moved down the line that added
  (`255cefbe`, `9890f13b`), and the catalog lists its lexer snapshot under W1002 (`ba2b42f8`).
- **A miss of the rows, found by the first change's Linux record:** twelve of this change's rows
  were not as `wolf fmt` writes them, the same miss as the first change's (its note says how
  `wolf_fmt`'s corpus identity test found it, and why the rows were wrong and not the formatter).
  Red first: with the first change's rows formatted and these not yet (`ac71f6d3`), the identity
  test fails at this change's first such row, `typecheck/else_binding_assign.lu`
  (`pr2-m2-fmt-red-ac71f6d3.log`); before either fix, the corpus walkers failed on the second Mac
  at the first change's row, and every other walker passed (`pr2-m2-walkers-red-b0dcac64.log`).
  The rows are formatted now (`d77d9a9b`, `28146434`), and
  their spans in the four-machine test (`a4924dbe`) and their lexer and parser snapshots
  (`4f5e2f4e`, `de02b44c`) moved with them; the E0404 spans did not move.
- **Anchors: hit.** 731 keys at the first change's head, 733 here (`gram.pat.let`, `type.rec`
  added, none removed; the key sets diffed both ways, `pr2-m2-regen-59f80c28.log`).

## Rulings

- **R-s287-1, ruled:** #1062c lands as E0410 now, with no warning release, because the census
  moved no program outside the corpus. One corpus witness wrote a `for` index to show an
  exclusivity refusal (E1002). It was adjusted, not edited to fit: it now copies the index into a
  `var` first (`var i = i`), the rule's own fix, so the index the claim reads is still one the body
  writes and the E1002 still holds. Its comment says why the copy is there. The `wolf_mem` fixture
  with the same program got the same copy, and its snapshot moved down the line it added.
- **R-s287-9:** the by-value cycle walk, its inline set, its report site (the member through which
  the cycle's first-declared type reaches itself, so a cycle through a generic struct is not
  reported inside that struct's own definition) and its limit (a recursion that only grows its
  arguments is not detected; the walk stops at 32 inline nominal levels).

## For the lupin mirror

lupin 0.1.60 runs, and the mirror must refuse, with the compiler's code at the compiler's span:

- **E0404** for a type that contains itself by value, used or not: through a payload, a tuple, a
  struct and an enum, a generic struct and an error union (`typecheck/rec_by_value_*.lu`). lupin
  already runs the container cases (`typecheck/rec_through_containers.lu`).
- **E0410** for a write to a name a `match` arm, a `for` header, an `else |…|` handler, a `select`
  receive or an `@` binds, and to a field of one (`typecheck/match_binding_*.lu`,
  `for_binding_assign.lu`, `else_binding_assign.lu`, `select_binding_assign.lu`).
- The vendored copy of `corpus/memory/elem_loop_induction_written_mut.lu` in wolf-interp's corpus
  takes the same `var i = i`.

## The census

The first change's rig: every `.lu` file of the corpus and of 38 org checkouts at their trunks, one
`conform-run --json --phase=mem` per directory module or entry file, on the first change's binary
(`aa3c9baa`'s, its code-final head but for E0609's wording) and on this change's (`bf77af3f`'s, the
code this head carries), both built before the rebase onto trunk `718fe47a`, whose change is the
runtime's alone, compared run by run (`notes/s287-evidence/pr2-census.txt`).

That is 5,095 runs over 6,854 files. No verdict and no error moved in any public tree, nor in the
seven private downstream trees (58 files). In the corpus, the 15 new rows moved, and so did the
adjusted witness's E1002 span (`592..597` to `921..926`), by the bytes its comment and its
`var i = i` added; its verdict stayed `fail(E1002)`. The `bump` row's span in this census is
`394..395`, from before its `warns:` line and before the rows were formatted (below); at the head
it is `419..420`. The census ran before that formatting, so its spans are the unformatted rows'.

Before the witness was adjusted, the same comparison at an earlier work-in-progress snapshot moved
it from E1002 at `mem` to E0410 at the checker, and moved nothing outside the corpus. That is the
one move R-s287-1 was decided on.

A syntactic scan for a write to a `for` header's name, over 8,124 files (every checkout above,
vendored copies included, and the corpus; `rig/forassign-scan.py`, a superset with no scopes or
types), found three lines:
two match arms `_ => …` inside a `for _ in` loop, which it reads as a write to `_`, and the vendored
copy of the witness in wolf-interp's corpus, which the census skips as a copy of this one.

## Evidence index

`notes/s287-evidence/index.txt` lists every file. This change's files begin `pr2-`.

## Done-when

Commits:
- #1062b: `c640f7b9` (`[type.rec]`); `52b4e252`, `33741960`, `2487debf`, `56c204c6`, `b192d802`,
  `860a8195` (the six rows), `1fbeb0ce` (the container control); `240a12de`, `a00d45c4` (the
  report site, before the code); `4383a2af` (the rows on four machines); `30833adb` (the walk,
  E0404); `bd7ac235` (E0404's explanation); `1b48c453`, `e6f01462` (the rendered snapshot).
- #1062c: `a5961266` (`[gram.pat.let]`); `a7cf83d0`, `4dd3694f`, `dbb6f7b1`, `a3a5defe`,
  `091fb5fa`, `59991378`, `3f5220ba` (the seven rows), `b87c3932` (the `var` control); `f80dee12`
  (the rows on four machines); `3fdc9046`, `abd9ea09` (the witness and the fixture adjusted, before
  the code); `98675784` (the checker's E0410); `870a2b38` (the resolve rung's docs say which rung
  judges a pattern's name); `2115dd30` (E0410's explanation); `e06f4137`, `9b554592` (the rendered
  snapshot).
- `53cfb124`: the two anchors indexed.
- `0bd6e46d`, `ef6afa76`, `b5892354`, `e799b33c`: the snapshots, the fixture's snapshot and the
  lowering ledger. `81c3a342`: the changelog. `99fab3e8`: the catalog regenerated.
- `6ac44337`, `255cefbe`, `9890f13b`: the `bump` row's W1002, its snapshots and its span;
  `ba2b42f8`: the catalog lists that row's lexer snapshot under W1002.
- `d77d9a9b`, `28146434`: the rows as `wolf fmt` writes them; `a4924dbe`: their spans;
  `4f5e2f4e`, `de02b44c`: their snapshots.
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`2041a39c`), items 7 and 8
- [x] the rows red at the base's code for each item, by name, in the lanes test and the corpus
      step (`pr2-m1-red-e36a9b29.log`, `pr2-m1-red-ddceedf8.log`)
- [x] the anchors gate red at each new clause, green at its index: 731 to 733 keys, none removed
- [x] the rows green at the fix on the checked machine, native and release, spans exact; lupin held
      to its present answers (`front_end_rulings_lanes`, 47 tests)
- [x] the corpus `0 bad` (1327 files; the fail-pin ledger 418 to 431 rules); conformance green
- [x] fmt, clippy (`wolf_diag`, `wolf_sema`, `wolf_driver`, all targets, `-D warnings`), the
      `wolf_diag` and `wolf_sema` suites, the catalogs' checks, the anchors gate, the corpus and
      conformance at `ba2b42f8`, the code-final head (`pr2-m1-checks-cdc77a1f.log`)
- [x] the adjusted fixture: `wolf_mem`'s rendered diagnostics, the element-places four-machine
      test, clippy on `wolf_mem` (`pr2-m2-extra-2aeeb81b.log`)
- [x] every corpus-walking snapshot suite with no update at `9890f13b`, whose snapshots the head
      carries (`pr2-m2-snaps-2aeeb81b.log`)
- [x] the census: nothing moves outside the corpus
- [x] at the rebased code (`a7078094`, trunk `718fe47a`): the checks above, the lanes test, the
      corpus, conformance, the adjusted fixture's suites, and every corpus-walking snapshot suite
      with no update (`pr2-m2-rebased-a7078094.log`)
- [x] the corpus walkers (`wolf_fmt`'s corpus identity and properties; every test of `wolf_fmt`,
      `wolf_lex`, `wolf_parse` and `wolf_wir`): red before the fix at that one test, at this
      change's own rows (`ac71f6d3`), and green at
      the formatted code (`0c23228c`, whose code this head carries; only notes differ) with the
      checks above, the adjusted fixture's suites and every snapshot suite
      (`pr2-m2-fmt-red-ac71f6d3.log`, `pr2-m2-walkers-red-b0dcac64.log`,
      `pr2-m2-fixed-0c23228c.log`)
- [ ] the Linux gauntlet record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once
