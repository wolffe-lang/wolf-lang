# s258 — a member read on a `!T` is E0409 (wolf-lang#879)

Lane note. **Opus 5.5. Crawl 54.** Branch `s258` off the orchestrator's
`o1-row-member` (`8bc97f65`: the spec sentence, the E0409 explain text
and the two corpus witnesses, three commits on trunk `94cd52bc`).
Contract: `sprints/compiler/s258-the-row-member.md`. Toolchains: wolf
0.2.34 (`9da7bdcf`) the control, lupin 0.1.55 the reference.

## Prediction (committed before the first change)

Read from `crates/wolf_sema/src/check.rs` at `8bc97f65`, nothing built
or run yet.

1. **Which arm each `row_operand_*` row exercises.**
   - `row_operand_add` (`n + 1` inside a hole), `row_operand_compare`
     (`n <= 5`) and `row_operand_rhs` (`1 + n`): `synth_bin` →
     `row_operand` → `report_bad_operand`, the existing operator arm.
     None of the three reaches either new arm; their diagnostics are
     byte-identical before and after.
   - `row_operand_member` (`rest.len`): `synth_member`, the
     `match self.kind_of(base_ty)` — today the `_ =>` fallback
     ("members on this type (the std surface)", exit 4); after item 1
     the new `TyKind::ErrUnion(..)` arm.
   - `row_operand_method` (`rest.starts_with("4")`): `synth_call` →
     `method_call` (an `ErrUnion` receiver falls to its `_ =>` arm) →
     `concrete_method_call`: no inherent impl, `home_of(ErrUnion)` is
     `None` so no home module, no trait impl, then the "methods on
     builtin types (the std surface)" NotYet. It never passes through
     `synth_member`, so **reverting item 1's arm alone reddens
     `row_operand_member` only**; the plant that reddens both rows
     reverts both arms.
2. **The method arm needs `synth_args_loosely`.** The arguments reach
   `concrete_method_call` as an untyped `ArgList`; nothing has synthed
   them. The neighbouring refusals (`home_miss`, the out-of-scope
   trait, the unknown method) each call `synth_args_loosely` before
   answering `error_ty`, and the new arm must too, so an error inside
   an argument is still reported and every argument expression still
   gets a recorded type.
3. **No existing corpus row changes verdict.** A row the compiler
   declines today is a `fail(CODE)` row carrying `forward:` (the corpus
   gate enforces it), and none of the three `forward:` rows reads a
   member of a `!T`. The differ's conservatism count moves by exactly
   2 — the two witnesses, `unsupported` → `fail(E0409)` — and by
   nothing else; the new positive row (`rows/row_member_handled.lu`)
   is one new entry that every machine runs alike.
4. **`Shared(ErrUnion)` is reachable** at the checker: prefix `shared`
   wraps whatever its operand synthesises (`TyKind::Shared(t)`, no row
   check), so `let c = shared strip("-j4")` then `c.len` reaches the
   `Shared` arm of `synth_member` with an `ErrUnion` inner and answers
   that arm's own NotYet ("member access on this type"). This lane
   does not touch it (the contract's one construct); a scratch probe
   measures it and the report carries it as a follow-up.
5. **The ledgers.** `spec/anchors.json` maps each tag to its document
   and the o1 sentence adds no anchor, so the key sets are equal both
   ways (688 = 688) and `spec-extract --check` is already green. The
   lowering ledger gains three lines: the two witnesses as `not
   mem-clean (pre-wir rungs own it)` (the same line before and after
   the arms — a NotYet and an E0409 both stop before the wir rung, so
   the snapshot is not the plant's red) and the positive row as
   `lowers`. At `8bc97f65` the gauntlet is red before any arm: the
   witnesses have no lexer snapshots, the ledger lacks them,
   `docs/diagnostics.md` predates the explain text (`diag-catalog
   --check`), and the corpus gate refuses two `fail(E0409)` rows the
   compiler declines without `forward:`.
6. **Against lupin 0.1.55** both witnesses run (`n: 1`, `yes`, exit 0),
   so in the `--triage` ledger they move from `unsupported` to
   *completeness* (static stricter), and in a plain differ pass to a
   `Verdict` divergence, until wolf-interp mirrors the clause.

## What moved (measured on kasumi, `taskset -c 4-7`)

**The arms** (`crates/wolf_sema/src/check.rs`). `report_row_member(span,
found, what)` sits beside `row_operand` and is the one text for both
sites: E0409 "`.len` cannot be applied to `str ! {none}`" at the base,
the label "this is `str ! {none}`", and the clause's note spelled for the
dot, naming the base when it is a name or a field path (`rest?.len`,
`(rest else "").len`) and `r` otherwise, with the `else` fallback the
value half takes (`""`, `0`, `0.0`, `false`, else `…`). `synth_member`
answers it in a `TyKind::ErrUnion(..)` arm before `Nominal`;
`concrete_method_call` answers it at its head, ahead of the inherent
impls, the home module and the traits, and types the arguments loosely.
The head placement is the clause's reading (R-s258-1): a blanket impl
would otherwise take the row as its `T`. No other arm moved; `Shared`,
`Meta` and `Var` are untouched.

**The rows on every machine** (`conform-run --json`, wolf 0.2.34 =
`9da7bdc`, head = `694d7025`, lupin 0.1.55 = record commit `e642f26`):

| row | 0.2.34 checked / native / release | head, all three | lupin 0.1.55 |
|---|---|---|---|
| `rows/negative/row_operand_member.lu` | `unsupported` (members on this type) | `fail(E0409)` at `rest` | `exit(0)`, `n: 1` |
| `rows/negative/row_operand_method.lu` | `unsupported` (methods on builtin types) | `fail(E0409)` at `rest` | `exit(0)`, `yes` |
| `rows/row_member_handled.lu` | `exit(0)` | `exit(0)` | `exit(0)` |
| `row_operand_add`, `_compare`, `_rhs` | `fail(E0409)` | `fail(E0409)` | `fail(E0409)` |

**Other shapes** (scratch probes, not rows): a call base
(`strip("-j4").len`), a struct field on `P ! {bad}`, `xs.len` on a
`List[int] ! {none}`, `t.0` on `(int, int) ! {none}`, a method through a
blanket `impl[T] Probe for T`, and `n.to_str()` on a `!int` were each
`unsupported` at 0.2.34 (the blanket one as "a `Probe` method on a
non-nominal receiver") and are each E0409 at the base on all three
lanes. lupin 0.1.55 runs the first four and refuses the last two.

**`Shared(ErrUnion)` is reachable**, as predicted: `let c = shared
strip("-j4")` then `c.len` is `unsupported` ("member access on this
type (the std surface)") at 0.2.34 and at the head, and lupin runs it
(`n: 1`); `c.starts_with("4")` is E0403 on wolf (the shared cell's
method surface) and `unsupported` on lupin. Not touched here (the
contract's one construct); listed for the orchestrator to file.

**The ledgers.** `spec/anchors.json` regenerated is byte-identical to
trunk's: 688 keys on both sides, nothing in either difference.
`spec-extract --check` was green before and after. The lowering ledger
gained exactly three lines (the witnesses `not mem-clean (pre-wir rungs
own it)`, `row_member_handled.lu: lowers`). `docs/diagnostics.md` was
out of sync at `8bc97f65` (`diag-catalog --check` red: the explain text)
and regenerated carries the text and six new fixtures (the two
witnesses' token streams, four sema snapshots). Every snapshot was
written as `.snap.new` and read before it was moved into place.

**The differ.** Every wolf in one configuration, the corpus's own (no
`std/` beside the binary: with the release archive's `std/` in reach,
twelve method rows that answer E0301 without one answer otherwise, which
is packaging, not this change, so 0.2.34's archive files were copied
without it). `cargo xtask differ <wolf>
<lupin 0.1.55> --triage --<tier>`, 1,193 files, 1,132 entries:

    checked  before 8bc97f6  801 agreement(s), 254 completeness note(s), 3 SOUNDNESS finding(s), 102 unsupported (3 forward pin(s)); 23 hard divergence(s)
    checked  head   694d702  801 agreement(s), 256 completeness note(s), 3 SOUNDNESS finding(s), 100 unsupported (3 forward pin(s)); 23 hard divergence(s)
    native   before 8bc97f6  785 agreement(s), 254 completeness note(s), 17 SOUNDNESS finding(s), 104 unsupported (3 forward pin(s)); 37 hard divergence(s)
    native   head   694d702  785 agreement(s), 256 completeness note(s), 17 SOUNDNESS finding(s), 102 unsupported (3 forward pin(s)); 37 hard divergence(s)
    checked  0.2.34 9da7bdc  802 agreement(s), 256 completeness note(s), 0 SOUNDNESS finding(s), 102 unsupported (3 forward pin(s)); 21 hard divergence(s)
    native   0.2.34 9da7bdc  786 agreement(s), 256 completeness note(s), 14 SOUNDNESS finding(s), 105 unsupported (3 forward pin(s)); 34 hard divergence(s)

Coverage is unchanged by the arms (checked A 717, BOTH 691; native A
721, BOTH 689). Between before and head the ledgers differ by exactly
two lines on each tier, the two witnesses, `unsupported` → `Completeness`
(`a: fail(E0409)`, `b: exit(0)`). On native the three use-after-free UB
rows (`unsafe_ub_uaf`, `atomic_ub_uaf`, `volatile_ub_uaf`) print a
different exit status on every run inside the same SOUNDNESS class: UB,
not a move. The plain pass head against before reports 0 divergences.
Head against 0.2.34 and before against 0.2.34 report the same seven
divergences, all trunk's since the tag (s252's region-scalar rows,
`distinct_copy_scalar`, `region_os_cwd_return`, `core_limit_rows`), so
the 0.2.34 control's counts carry trunk's deltas and the before build
at the lane's base is the one that isolates this change.

**The lupin lag, recorded.** `differ-self` runs the debug `wolf` on both
sides, so a lupin-lagging row is invisible to it (both answer
`fail(E0409)`, no class). The pairing ritual's `--triage` files a
`fail`-pinned row as a completeness note whatever lupin answers, so the
two witnesses reach no hard count; when wolf-interp mirrors the clause
they will move *below* the ledger (`B exit(0) -> fail(E0409)`), as
`row_operand_add` did at the 0.1.31 re-stamp. A `forward:` pin does not
apply: it marks a construct *wolf* declines, and wolf enforces the rule
now. The record is `crates/wolf_driver/tests/row_member_lanes.rs`: each
witness pins lupin 0.1.55 by version and record commit (`e642f26`) to
its measured run, so a newer lupin that still runs them reds by name and
a dev build carrying the mirror is held to the clause.

## The plant

Branch `s258-plant` (`65e7222d`) = `694d7025` with both arm commits
reverted (`6acd741c`, `0c85fb33`). Reverting item 1's commit alone does
not build: the shared helper lives in it and item 2 calls it. CI run
39837610858 (`workflow_dispatch -f jobs=lint-test`, `tier partial`),
job `lint + test + corpus (linux x86-64)`: red at the gauntlet's `test`
step on exactly the two witness tests of `row_member_lanes.rs`
(`a_member_read_on_a_row_is_e0409_on_every_lane`,
`a_method_call_on_a_row_is_e0409_on_every_lane`: the checked lane
answers `unsupported`, "members on this type (the std surface)" and
"methods on builtin types (the std surface)"); the handled row and the
operator rows stay green. Cargo stops at the first red test binary, so
the rest was read on kasumi at the same head with `--no-fail-fast`:
`typecheck_diagnostics` red on `e0409_row_member_and_method` only,
`typecheck` red on the two `method_on_a_row_*` tests only, and
`cargo xtask corpus` red on exactly the two witnesses ("pins a
rejection the compiler cannot make — it declines this file …"; `1193
file(s), 2 bad`). At the head each of these is green.

## The prediction, scored

1. **Right.** The witnesses took the arms named, the control refusing
   each by the construct predicted; the operator rows answer
   `fail(E0409)` on every lane and lupin, and their snapshots did not
   move. The sub-claim that reverting item 1 alone reddens one row was
   not measured (that revert does not build; see the plant).
2. **Right.** The method arm needs `synth_args_loosely`:
   `method_on_a_row_still_types_its_arguments` sees the E0401 inside
   the argument beside the E0409.
3. **Right.** Against the lane's base the ledgers moved by exactly the
   two witnesses on both tiers and nothing else.
4. **Right.** `Shared(ErrUnion)` is reachable and answers the `Shared`
   arm's own refusal.
5. **Right.** Anchors 688 = 688, byte-identical; three ledger lines; at
   `8bc97f65` `diag-catalog --check` was red, the witnesses had no lexer
   snapshots and no ledger lines, and the corpus gate refuses them
   without `forward:` (measured on the plant, the same compiler).
6. **Right for the triage ledger** (`unsupported` → completeness).
   The plain-pass claim was not measured: the plain passes compared
   wolf with wolf.
