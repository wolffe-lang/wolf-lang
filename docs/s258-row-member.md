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
