# s287 prediction: the front-end rulings on the second compiler's spec silences

Committed before the first change. Base: wolf-lang trunk `d848fbed`. The base was measured first
(`notes/s287-evidence/base-d848fbed.log`, the programs under `notes/s287-evidence/probe/`), on the
checked machine, native and release (`wolf conform-run --json`, a clean debug build of the base,
sha256 `4781f945…`) and lupin 0.1.60 (the `PAIRING` stamp; archive sha256 `f3cae16f…6bb8`, read
twice, equal to the release page).

Eight questions, six issues: wolf-lang#1064, #1059, #1060, #1062 (three cases), #1065 and #1061's two
refusals, plus #1073 (a parameter declared twice) folded into #1062's first case. Each lands as its
own clause with its own anchor, its rows red first, and its own commits, cheapest first.

## What the base does

Every case the issues and the ruling page record answers the same at `d848fbed`:

- **#1064:** `error E = {E}` unused exits 0 on all three tiers; used (`-> int ! E`) it is E0610 at
  1:12 on all three. A two-alias loop unused exits 0. lupin gives E0610 at the declaration either way.
- **#1059:** `let Done` then `Done => { Done = 9 }` exits 9 on all four machines (W0801 at the
  `let`). Reading `Done` in the arm gives 8; `var Done` gives 9. Two neighbours the page did not
  probe answer the same way: a row-tag arm (`gone => { gone = 9 }` over `int ! {gone}`, exit 9) and a
  field write through the name (`Done.a = 9`, exit 9). The qualified spelling (`E.Done => { Done = 9
  }`) is already E0410 on all four.
- **#1060:** `enum E { A, A }` exits 3, `{ A(int), A(str) }` exits 5, a struct field declared twice
  exits 0, and so do the `type E = enum {…}` and `type P = struct {…}` forms. `enum E { a, A }` is two
  variants (exit 2).
- **#1062a:** the later binding wins everywhere (exit 2): two struct fields, at two types, `let (x,
  x)`, a `for` header, `x @ x` (exit 5), nested (exit 5). `fn f(a: int, a: int)` and
  `fn(a: int, a: int)` exit 2 (#1073). `let a = 1, a = a + 1` exits 2. `(Red, Red)` over two
  variants is two tests, not two bindings (exit 4).
- **#1065:** `{A, x}` is E0601 at `x`, `{x, A}` exits 1, `{A, B}` sharing `x` exits 1,
  `{A, Bad(str)}` is E0601 at `Bad`, `{Bad(str), A}` is E0609 at the spelling, and two aliases whose
  `Bad` payloads disagree are E0609 at the later alias. `{x, x}` and `{Io(str), Io}` are E0601. lupin
  accepts all of them. An alias whose own row spells `x` twice exits 0 while unused.
- **#1061:** `A(x) | B(x) => x` passes the checker on the compiled tiers and refuses at `mem`
  ("cannot compile this yet", two ledger constructs); lupin exits 3. `A(x) | B(_)`, `A(_) | B(x)`
  and `x` at `int` and `bool` all pass the checker; lupin runs two of them and refuses one as
  unsupported. Or-patterns that bind nothing run everywhere.
- **#1062b:** `enum L { Nil, Cons(int, L) }` runs on the checked machine and lupin (exit 1) and
  refuses natively ("deeply nested aggregate types"); the tuple and struct cycles split the same
  way; every unused shape exits 0, `Box[L]` through a generic struct included. Through `List`, `Map`
  and `handle` every tier runs it. `struct S { next: shared S }` is E1006 at `mem`.
- **#1062c:** every machine lets the binding be assigned: a match arm (exit 4), a `for` header
  (12), `else |Bad(n)|` (5), a `select` receive (5), a field of an arm binding (9), an `@` name (6);
  the `bump` trap exits 3 with only W1002. A `read` parameter is E1014 at `mem` (lupin traps). A
  closure parameter is assignable (exit 2), which no clause here touches.

## The rulings, as ruled, and the lane's own recommendations

- **R-s287-1 (the contract's): #1062c lands as an error unless the census moves downstream code.**
  The threshold: if the census moves any file outside this repository's corpus (a downstream
  package a user builds), the rule lands as a warning for one release, with a machine-applicable
  `var x = x` fix that `wolf fix` applies, and becomes E0410 in the release after; if only corpus
  rows move, it lands as E0410 now. Recommended after the census, below.
- **R-s287-2: #1059's E0410 is decided by type, in the checker.** Whether a bare arm name binds is
  the scrutinee's type's question (`[gram.pat.binder]`: "a bare name that spells a case of the
  scrutinee's type tests it"). The resolve rung decides by scope, and scope gets this wrong both ways:
  it misses a row tag (`gone`) and a variant reached only through a module path, and it would refuse a
  bare name that spells an in-scope variant the scrutinee's type does not have, which binds. The
  checker already decides it exactly, so the E0410 is reported there, at the assignment, with the
  same text and fix as the resolve rung's. The phase reached is `typecheck` where E0410 is otherwise
  `resolve`; `[proto.cmp.rung]` makes the records agree on code and span. Not taken: the in-scope
  approximation at the resolve rung.
- **R-s287-3: E0827 and E0828 live in the checker, for the same reason.** `(Red, Red)` is two tests
  and must stay legal, while `(x, x)` is two bindings; only the type tells them apart in an arm.
  Binder positions (`let`, `var`, `for`, `else |…|`, `select`) and parameter lists go through the same
  check, so one code has one rung.
- **R-s287-4: #1073 folds in.** A parameter list (a function's or a closure's) binds each name once,
  E0827 at the second. The clause takes one sentence for it, and the code's title becomes "a name bound
  twice in one pattern or parameter list". wolf-lang#1074 (a generic parameter declared twice) waits:
  generic parameters are a type namespace with its own codes, and the clause would need a different
  sentence.
- **R-s287-5: #1064's walk is signature elaboration's, at each `error` declaration.** Today's E0610
  is emitted where a row expands an alias; the fix elaborates each alias's own row once, at its
  declaration, in declaration order, and the expansion at a use reports nothing the declaration
  already reported. E0610 keeps its rung (`typecheck` on the compiled tiers), so
  `rows/negative/error_alias_cycle.lu` keeps its header and its span. "The resolver only" in the
  contract is read as: name resolution of alias rows, no checker or backend change.
- **R-s287-6: a loop is reported once, at the entry that closes it in declaration order.** Two
  aliases that name each other are one E0610, at the first-declared alias's name inside the
  second's row, as the used case reports today.
- **R-s287-7: an alias's own row is checked where it is declared, used or not.** The same walk
  finds the alias's own duplicate spellings (`error A = {x, x}`, E0601) and its layers' payload
  conflicts (E0609) while it is unused. This follows from the amended clause's "an alias is checked
  where it is declared, as every item is"; nothing in the corpus spells one (the census will say).
- **R-s287-8: #1065's E0609 site:** at the row's own spelling of the tag when one source is the row's
  own, at the later alias entry when two aliases disagree. A tag the row spells twice stays E0601
  at the second spelling, also after an alias that brings it (`{A, x, x}`).
- **R-s287-9: #1062b's walk:** a cycle through struct fields, enum payloads, tuple elements, the ok
  type and the tag payloads of an error union, `wrapping`, `distinct` and range types, with a generic
  struct's or enum's arguments substituted for its parameters (`Box[L]` with `struct Box[T] { v: T }`
  is a cycle; `Bag[L]` with `struct Bag[T] { items: List[T] }` is not). `List`, `Map`, `Pool`,
  `handle`, `weak`, raw pointers, channels, mutexes, function types and `shared` are not inline;
  `shared` cycles stay E1006. One E0404 per cycle, at the member that closes it, the cycle named in
  full. A recursion that only grows its generic arguments (`L[T]` holding `L[(T, T)]`) is not
  detected; the walk stops at a fixed depth and says nothing.

## The clauses and their rows, predicted

The rows go through the corpus step (`cargo xtask corpus`), conformance (`cargo xtask conformance`)
and a new driver test, `crates/wolf_driver/tests/front_end_rulings_lanes.rs`, which runs each row on
the checked machine, native, release and the pinned lupin, s285's shape. At the rows' commits (the
base's code), each new `fail` row is red by name in the corpus step and in the test, and each new
anchor is red in conformance until it is indexed. `cargo xtask spec-extract --check` is red at each
clause until the index is regenerated; each regeneration adds that clause's anchors and removes
none (the key sets diffed both ways).

The phases below are the corpus's (`phase:` is the deepest phase that passes): `parse` for a resolve
rung refusal, `resolve` for the checker's.

1. **#1064, `[type.err.alias.cycle]` amended in `spec/10-types.md`** (anchor kept): "…whether or
   not any row names it: an alias is checked where it is declared, as every item is."
   - `rows/negative/error_alias_cycle_unused.lu` (`error E = {E}`, unused): `fail(E0610)`, `phase:
     resolve`. Base: `exit(0)` on all three. Head: E0610 at 11..12 on all three; lupin E0610 there.
   - `rows/negative/error_alias_cycle_pair_unused.lu` (`error A = {B}`, `error B = {A}`): one
     E0610, at `A` inside `B`'s row. Base `exit(0)`.
   - `rows/negative/error_alias_own_dup_unused.lu` (`error A = {x, x}`, unused): `fail(E0601)` at
     the second `x` (R-s287-7). Base `exit(0)`; lupin 0.1.60 `exit(0)` (it has no own-duplicate rule,
     #1065).
   - The existing `rows/negative/error_alias_cycle.lu` keeps one E0610 at its span.
2. **#1059, `[gram.pat.nullary.scope]`, new, appended to `[gram.pat.nullary]`.**
   - `typecheck/nullary_arm_let_assign.lu`: `fail(E0410)`, `phase: resolve`, at `Done` in
     `Done = 9`. Base `exit(9)` on all four.
   - `typecheck/nullary_arm_var_assign.lu`: `run(exit=9)`, `warns: W0801`. Green at the base too: the
     control.
   - `typecheck/nullary_arm_tag_assign.lu` (a row-tag arm): `fail(E0410)`. Base `exit(9)`.
   - `typecheck/nullary_arm_field_assign.lu` (`Done.a = 9`): `fail(E0410)` over the whole place.
     Base `exit(9)`.
   - lupin 0.1.60 answers the three `fail` rows `exit(9)`; the rows hold it to that until the mirror.
3. **#1060, `[gram.item.type.names]`, new, in §2.5.** The item collector (`graph.rs`), so E0302
   stays a resolve-rung code (`phase: parse`).
   - `resolve/enum_variant_twice.lu`, `resolve/enum_variant_twice_payload.lu`,
     `resolve/struct_field_twice.lu`: `fail(E0302)` at the second name, with the first as the
     secondary label. Base `exit(3)`, `exit(5)`, `exit(0)`.
   - `resolve/type_decl_member_twice.lu` (the `type … = enum {…}` and `= struct {…}` forms): two
     E0302s. Base `exit(0)`.
   - `resolve/enum_variant_case_distinct.lu` (`enum E { a, A }`): `run(exit=2)`, green at the base.
   - E0302's entry in the registry gains the member paragraph; `docs/diagnostics.md` regenerated.
4. **#1062a and #1073, `[gram.pat.names]`, new, in §5, and the new E0827** (free at the base:
   E0826 is the last E08xx). The checker (R-s287-3), `phase: resolve`.
   - `typecheck/pattern_name_twice_struct.lu`, `…_types.lu` (bool and int), `…_let.lu`,
     `…_at.lu`, `…_nested.lu`, `…_for.lu`: `fail(E0827)` at the second binding, the first as the
     secondary label. Base: `exit(2)`, `exit(2)`, `exit(2)`, `exit(5)`, `exit(5)`, `exit(2)`.
   - `typecheck/param_name_twice.lu` and `typecheck/param_name_twice_closure.lu`: `fail(E0827)` at
     the second `a`. Base `exit(2)`.
   - `typecheck/let_group_shadow.lu` (`let a = 1, a = a + 1`): `run(exit=2)`, green at the base.
   - `typecheck/pattern_variant_tests_twice.lu` (`(Red, Red)`): `run(exit=4)`, green at the base.
5. **#1065, `[type.err.alias.union]` amended, and `[gram.type.row.dup]`, new, beside
   `[gram.type.row.flatten]`.** Signature elaboration's row lowering: the duplicate check looks only
   at the entries the row spells; alias tags merge in any order.
   - `rows/error_alias_union_own_after.lu` (`{A, x}`): `run(exit=1)`. Base `fail(E0601)`.
   - `rows/negative/error_alias_union_own_after_payload.lu` (`{A, Bad(str)}`): `fail(E0609)` at the
     spelling `Bad(str)`. Base `fail(E0601)` at `Bad`.
   - `rows/negative/error_alias_union_two_aliases_payload.lu`: `fail(E0609)` at the later alias
     entry. Green at the base (the control for R-s287-8).
   - `rows/negative/error_alias_union_own_twice.lu` (`{A, x, x}`): `fail(E0601)` at the second `x`.
     Base: E0601 at the first `x` (a span move).
   - `rows/negative/error_row_own_dup_bare.lu` (`{x, x}`): `fail(E0601)`, green at the base.
   - The existing `rows/negative/dup_tags.lu` gains `conforms: gram.type.row.dup`.
   - lupin accepts every one of them today (`exit(1)`, `dup_tags.lu` `exit(7)`).
6. **#1061's two refusals, `[gram.pat.or]`, new, in §5, and the new E0828.** The checker,
   `phase: resolve`. The join lowering is s288's: the machines' "cannot compile this yet" stays.
   - `typecheck/or_pattern_binds_missing.lu` (`A(x) | B(_)`): `fail(E0828)` at `B(_)`, naming `x`.
     Base: the checker passes it (both machines refuse at `mem`).
   - `typecheck/or_pattern_binds_missing_left.lu` (`A(_) | B(x)`): `fail(E0828)` at `A(_)`.
   - `typecheck/or_pattern_binds_types.lu` (`x` at `int` and `bool`): `fail(E0401)` at the later `x`.
   - `typecheck/or_pattern_binds.lu`, `…_nested.lu` (`W(A(x) | B(x))`), `…_at.lu` (`n @ 1..=9 | n @
     20..=29`): `run(exit=…)`, `phase: mem`, green at the base: the compiled tiers' refusal at `mem`
     stays the honest ledger entry, and lupin runs them.
   - The existing `grammar/match_arm_or_over_product.lu` gains `conforms: gram.pat.or`.
7. **#1062b, `[type.rec]`, a new section in `spec/10-types.md`, after its census.** The checker,
   once per declared type, `phase: resolve`.
   - `typecheck/rec_by_value_enum.lu`, `…_tuple.lu`, `…_mutual.lu` (a struct and an enum),
     `…_unused.lu` (`struct N { v: int, next: N }`), `…_generic.lu` (`Box[L]`), `…_errunion.lu`:
     `fail(E0404)`, once each, at the member that closes the cycle. Base: `exit(1)`/`exit(7)`/`exit(5)`
     on the checked machine and lupin, "cannot compile this yet" natively; the unused ones `exit(0)`.
   - `typecheck/rec_through_containers.lu` (`List`, `Map`, `handle`, `Bag[L]`): `run`, green at the
     base; `typecheck/enum_union_tree.lu` stays the legal counterpart.
   - E0404's explanation says "through a container".
8. **#1062c, `[gram.pat.let]`, new, in §5, after its census.** The checker marks every binding a
   pattern makes outside `let` and `var` (a match arm, a `for` header, an `else |…|` handler, a
   `select` receive); assigning one, or a field of one, is E0410 (or the warning, R-s287-1),
   `phase: resolve`.
   - `typecheck/match_binding_assign.lu`, `for_binding_assign.lu`, `else_binding_assign.lu`,
     `select_binding_assign.lu`, `match_binding_field_assign.lu`, `match_binding_at_assign.lu`,
     `match_binding_mut_param.lu` (the `bump` trap): `fail(E0410)`. Base: every one runs.
   - `typecheck/match_binding_var_shadow.lu` (`var x = x`): `run(exit=4)`, green at the base.
   - E0410's sentence about match bindings is rewritten.
9. **Anchors:** seven new keys (`gram.pat.nullary.scope`, `gram.item.type.names`, `gram.pat.names`,
   `gram.type.row.dup`, `gram.pat.or`, `type.rec`, `gram.pat.let`), two amended clauses that keep
   theirs. 726 keys at the base; 733 at the end, none removed.

## The censuses' plan

One measurement rig for both (`rig/measure.py`, s285's per-module runner, generalized to a rung and
every error code): every `.lu` of the corpus and of 38 org checkouts at their trunks (the public
wolf trees, wolf-std at its trunk and at the `STD-PIN`, wolf-interp, and the private downstream
trees, whose results stay in the lane), one `conform-run --json --phase=mem` per directory module or
entry file, on the base binary and on a head's, compared run by run.

- **The base pass** runs before any code (launched with this prediction).
- **#1062b's census:** the head with `[type.rec]`'s check, against the base. Predicted: no file
  outside this lane's rows gains an E0404. Every by-value recursive type that runs today runs only
  on the checked machine or lupin, and no downstream tree declares one.
- **#1062c's census:** the head with the rule, against the base. Predicted: a handful of moves, all
  `for` headers or arm bindings that update a loop variable or a payload in place, most in the
  corpus, possibly in the book. R-s287-1 then decides error or warning, and the hand-back says which
  files moved.
- **What must not move, at every head:** no verdict or diagnostic outside the rows each item adds
  (and the census's named moves for #1062c), in the corpus or any tree.

## Commits, the plant, the PRs

- Per item: the clause (the anchors gate red), the rows (red at the base's code), the fix, the
  anchor indexed, the registry text, the regenerated catalog, snapshots.
- One plant, `s287-plant-alias-cycle`: the declaration walk for #1064 removed, one hunk in
  `crates/wolf_sema/src/sig.rs`, red by name at `rows/negative/error_alias_cycle_unused.lu` in the
  corpus step and in `front_end_rulings_lanes`.
- Two PRs if the censuses run long: items 1–6, then 7 and 8 from the first PR's green head.
