# s281 — a bare variant nested in a payload pattern: prediction, before the first change

Contract: the s281 lane contract and the lane rules it cites. Written 2026-11-04 on nomad,
before any change to the tree.

## Inputs, read
- Branch `s281` cut at origin/trunk `5f1f1c94` (s278).
- wolf-lang#1043 whole: `K(Drink)` over `enum W { K(Kind) }` takes the `K(Drink)` arm for a
  `K(Kind.Food)` on `wolf run` (0.2.36), lupin 0.1.58 prints `other`; the checker reads the arm
  as a test (E0801 names `K(Food)`, `K(Gratuity)` without `K(_)`; no E0802 on `K(_)`).
- Spec: `[gram.pat]` (payload positions take `pattern`, recursively), `[gram.pat.nullary]` (a
  bare `path` matches a constructor that carries nothing), `[gram.expr.variant]` ("a `match` arm
  is the other position and keeps the bare name: there the scrutinee's type already says which
  enum"), `[type.row.match]`, E0806's registry text (an enum variant is refutable in a binder),
  W0801.
- The checker: `match_pattern` in `crates/wolf_sema/src/check.rs` decides a bare name by the
  type of the value it matches (`scrut_cases`, the #4 rule), at every depth, because the payload,
  tuple and field arms recurse with the sub-value's type. `bind_pattern` (binders: `let`, `var`,
  `for`) binds every bare name and asks no type.
- The checked machine: `match_pattern` in `crates/wolf_mem/src/ubcheck.rs`; nested positions
  replay the checker's record (`tb.locals`): a name the checker did not bind is a case test.
- The lowering, `crates/wolf_wir/src/lower.rs`: `pattern_shape` classifies an arm; a top-level
  bare name asks `domain_test`; `product_top`/`product_sub` (s130, s168) decide a bare name in a
  product by `names_sema_case` on the field's own type and lower a case to
  `product_nested_case`'s tag test.

## Base, measured
`5f1f1c94` built on nomad (`cargo build --locked -p wolf_driver -p wolf_rt`); each probe on the
checked machine, native and release (`wolf conform-run --json --<tier>`) and lupin 0.1.58
(`notes/s281-evidence/nomad/mac-job1-5f1f1c94.log`; the published 0.2.36 answers the same for
the issue's two programs):

| shape | checked | native, release | lupin |
|---|---|---|---|
| `K(Drink)` (Food, Drink, Gratuity) | `other drink other` | `drink drink drink` | as checked |
| `Item(Drink, _)`, `Item(Food, n)`, `Item(_, n)` | `275 1 30` | `1 1 1` | as checked |
| `Item(n, Drink)` (the second slot) | `30 3` | `3 3` | as checked |
| `K(Drink, n) if n > 1` | `other big drink` | `big drink big drink` | as checked |
| row arm `bad(Drink)` over `int ! {bad(Kind)}` | `other drink` | `drink drink` | as checked |
| `else \|e\| match e { bad(Drink) => … }` | `8 7` | `7 7` | as checked |
| `M(fast)` over `enum Mode { fast, slow }` | `other fast` | `fast fast` | refuses: `Mode.slow` does not resolve |
| `Out(In(Drink))` (two levels) | right | right | right |
| `K(Drink \| Food)` | right | right | right |
| `K(Kind.Drink)`, `K(d)`, `x @ K(Drink)`, `K((Drink, n))`, `(Drink, _)`, `Drink` | right | right | right |
| `P { k: Drink, .. }`, `K(P { k: Drink, n })` | right | right | right |
| `K(Drink) \| K(Food)` | runs | refused: "or-patterns with payload bindings (join params)" | runs |
| `let (Drink, n) = (Kind.Food, 3)`, `let P { k: Drink, n } = …`, `for (Drink, n) in xs` | binds `Drink` | binds | binds |
| `else \|bad(Drink)\| 7` | E0809 | E0809 | runs, then "did not match" |
| `Drink from ch` over `channel[Kind]` | binds | binds | "did not match the received value" |
| `K(Drink) from ch` | refused ("this select-arm pattern shape") | refused | refused at run |

## The code path, predicted
`pattern_shape`'s `PathPat` arm reads `Tag(sub, …)` and, when every sub is a bare name or `_`,
returns `Tests([Eq(tag)], binds)` with **every** bare name pushed as a slot binding; only a
sub that is not a plain name (`Kind.Drink`, `In(…)`, `Drink | Food`, a tuple, a struct, a
literal) sends the arm to the product walk. So `K(Drink)` lowers to "tag is `K`, bind slot 0
as `Drink`": it matches every `K`. The top level asks `domain_test`; the tuple and product walk
ask `names_sema_case`; only this fast path asks nothing. It is the s27 shape ("payload
subpatterns bind slots"), written before nested bare names were typed, kept when s130/s168
taught the product walk to decide by the field's type. Every red row above goes through it;
every green one does not (two levels and the payload or-pattern are green because their sub is
a `PathPat` or an `OrPat`). The guard row and both row-arm rows are the same path (the guarded
arm and the row half call the same `pattern_shape`); `K(Drink) | K(Food)` is the same misreading
seen through the or-pattern classifier, which refuses alternatives that bind.

## What each change will show
1. Rows first (a commit of their own): corpus rows with `check: run(exit=0, stdout=…)` and one
   test each in `crates/wolf_driver/tests/four_machine_regressions.rs` (the checked machine,
   native, release, lupin). At the base: `K(Drink)`, the two-field payload (both slots), the
   guard, and the row-arm row red on native and release by name, green on the checked machine
   and lupin; the two-level, payload or-pattern, qualified, lowercase-binding and struct-field
   rows green everywhere (controls: the contract expected two levels and the payload
   or-pattern red; measured green, the reason above).
2. The fix: the `PathPat` fast path asks the payload slot's own type
   (`payload_sema_tys` + `names_sema_case`, the product walk's mirror of the checker's rule);
   a bare name that spells a case sends the arm to the product walk, which lowers it to the
   tag test on the slot's discriminant. One function's classifier; no `spec/` change, no
   checker change, nothing on the checked machine (already right). Every row green on all four
   machines; the corpus and the release-parity row unchanged elsewhere.
3. Sweep, answered by measurement: struct fields green; binders (`let`, `for`) and `select` arms
   bind on every wolf tier, which is the checker's reading, not a lowering divergence (rulings
   below); `else |pat|` is refused at the checker (E0809) on every tier.
4. `K(Drink) | K(Food)`: after the fix the native refusal names the true reason
   ("or-patterns over product patterns (join params)"); still refused. A ruling below.
5. The plant: the fast path's bare-name branch binds again (one hunk in `lower.rs`); the rows
   red by name on native and release in its record.

## Rulings to ask (R-s281-N; recommendation in bold, implemented)
- **R-s281-1, the spec.** Not silent: `[gram.pat]` recurses, and `[gram.pat.nullary]` is a
  `closed_pattern` alternative at any depth, so the type at the position names the enum.
  **(A) no `spec/` change** (the checker and lupin already read it so); (B) one clarifying
  sentence for a spec lane: "At any depth — a payload slot, a tuple element, a struct field —
  the type of the value that position matches says which enum a bare name belongs to."
- **R-s281-2, binders.** `let (Drink, n) = …`, `let P { k: Drink, n } = …` and
  `for (Drink, n) in …` bind a name that spells a case of its own type, silently, on all four
  machines; E0806's text calls a variant refutable in a binder. (A) the checker refuses it
  (E0806) with a lupin mirror; (B) W0801 warns there too; **(C) no change in this lane**: the
  machines agree, and either change is a checker and lupin pair, not the lowering.
- **R-s281-3, `select` arms.** `Drink from ch` binds on every wolf tier; lupin reads it as a test
  and refuses at run. **(A) a follow-on checker+lupin lane refuses a bare case name in a select
  arm (E0806: a committed receive cannot fail to match)**; (B) no change.
- **R-s281-4, `K(Drink) | K(Food)`.** Refused on native and release, run by the checked machine
  and lupin, at the base and after the fix. (A) lower binding-free or-alternatives over
  products here; **(B) leave it with the join-params work** (`grammar/match_arm_or_over_product.lu`
  holds that line): loud, never a wrong answer, and a capability, not this bug.
