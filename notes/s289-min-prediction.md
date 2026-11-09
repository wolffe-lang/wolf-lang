# s289 prediction, third change: the prelude's `min` on every tier

The third of the sweep's three changes (`notes/s289-prediction.md` §2, R-s289-7 and R-s289-10),
from the second's head `353ee08c`. Written, with the base measured, before the first row. Its code
is the second change's checked commit's (`934e36ac`): nothing under `crates/`, `xtask/`, `spec/` or
`corpus/` differs.

## 1. What the base does

wolf-lang#708: the prelude names `min` and `zip` (`spec/prelude.json`, `prelude.rs`) and no clause
types either. Since the second change both are declined by name, on all three compiled tiers, as
"a prelude helper with no signature yet". lupin 0.1.60 runs `min` over any number of integers of
any widths, and declines `zip`. Its W0304 exempts `zip` and not `min`.

The probes (`notes/s289-evidence/probe-min/`, the runner `rig/min-probes.sh`, the log
`m3-base-353ee08c-air.log`). The compiled tiers are checked, native and release:

| probe | source | compiled tiers | lupin 0.1.60 |
|---|---|---|---|
| `m01_int` | `min(3, 4)`, `min(-5, 2)`, `min(7, 7)` | the decline; build exit 4 | `3 -5 7` |
| `m02_u8` | two `u8`s, both orders | the decline | `7 7` |
| `m03_i64_edges` | `i64::MIN`, `i64::MAX`, `0` | the decline | `-9223372036854775808 -9223372036854775808 0` |
| `m04_u64_top` | `u64::MAX` against `1`, both orders | the decline | `1 1` |
| `m05_one_arg` | `min(5)` | the decline | `5` |
| `m06_three_args` | `min(1, 2, 3)` | the decline | `1` |
| `m07_mixed` | a `u8` and an `i64` | the decline | `3` |
| `m08_str` | `min("b", "a")` | the decline | unsupported: "`min` takes integers, got str" |
| `m09_float` | `min(1.5, 2.0)` | the decline | unsupported: "… got f64" |
| `m10_literal_fit` | a `u8` and `300` | the decline | `3` |
| `m11_value` | `let f = min` | the decline | `1` |
| `m12_own_min` | the module's own `fn min` | W0304, runs its own (`4`) | W0304, `4` |
| `m13_typed_result` | `min` as a `u16` function's tail | the decline | `9` |
| `m14_effects_once` | `min(tick(2), tick(1))` | the decline | `tick 2`, `tick 1`, `1` |
| `z01_zip_call` | `zip(xs, ys)` | the decline | unsupported: "`zip` is in the ambient std stub but has no pinned semantics …" |
| `z02_own_zip` | the module's own `fn zip` | W0304, runs its own (`12`) | no warning, `12` |

`wolf prelude` lists both with W0304 "yes".

## 2. The change, predicted (R-s289-7 as ruled)

- **The clause.** It goes in `spec/10-types.md`, a new section `[type.min]`.
  - `min(a, b)` takes two values of one integer type `T` (the ten integer types; not `byte`,
    `char`, a float, `str` or `wrapping[T]`) and answers the smaller, as a `T`.
  - Both operands are evaluated once, left to right.
  - It is a compare and a select: signed for a signed `T` and unsigned for an unsigned one,
    as `<` is. It never traps.
  - `[type.min.zip]`: the free `zip` has no meaning. No clause gives it one, and `std.list`'s
    method `xs.zip(ys)` (`[type.comb.set]`) is the spelling. Declaring `zip` draws no W0304.
  - `spec/prelude.json` gives `min` its anchor and `zip` `w0304: false`, with `prelude.rs` in step
    (`cargo xtask prelude-diff`).
- **The checker.** `min(a, b)` is typed beside `print` and `fence`:
  - wrong arity is E0402, "`min` takes 2 arguments";
  - an operand outside the integers is E0401 at that operand;
  - the second operand is checked against the first's type, so a mismatch is E0401 at the second
    and a literal that does not fit is E0415.

  `min` as a value is "a builtin function used as a value (call it by name)", as every other
  builtin. `zip`, called, used as a value or as a receiver, is declined with the clause's reason.
- **Lowering.** In WIR, an `icmp` (`sle` or `ule`), a branch and a join with one parameter. On the
  checked machine, the two values compared as `<=` compares them, `u64` and `uint` by bit pattern.
- **What existing programs mean.** No corpus entry calls `min` or `zip` and reaches the decline
  (the first change's scan). `wordcount.lu` and `grammar/closure_extent.lu` spell them but stop at
  an earlier refusal. A program with its own `fn zip` loses its W0304.

## 3. The rows, predicted

Rows first, red at their commit. Corpus rows under `corpus/typecheck/` (`conforms: type.min`):

| row | at the rows' commit (compiled tiers) | at the fix | lupin 0.1.60 |
|---|---|---|---|
| `min_signed.lu`, `run(exit=0, stdout=…)` | the decline | `3 -5 7`, the `i64` edges | runs, equal |
| `min_unsigned.lu`, `run(exit=0, stdout=…)` | the decline | `7 7`, `1 1`, `9` | runs, equal |
| `min_evaluates_once.lu`, `run(exit=0, stdout=…)` | the decline | `tick 2`, `tick 1`, `1` | runs, equal |
| `min_one_argument.lu`, `fail(E0402)` | the decline | E0402 | runs (`5`) |
| `min_three_arguments.lu`, `fail(E0402)` | the decline | E0402 | runs (`1`) |
| `min_mixed_types.lu`, `fail(E0401)` | the decline | E0401 at the `i64` | runs (`3`) |
| `min_str.lu`, `fail(E0401)` | the decline | E0401 at `"b"` | unsupported |
| `min_literal_fit.lu`, `fail(E0415)` | the decline | E0415 at `300` | runs (`3`) |
| `lints/zip_own.lu`, `run(exit=0, stdout="12\n")`, no `warns:` | W0304 (the step's warning ledger) | warning-clean | equal |

- A four-machine test, `crates/wolf_driver/tests/prelude_min_lanes.rs`, holds each row on the
  checked machine, native and release, and lupin to its present answer. `zip` called is declined
  with the clause's reason on all three tiers, and lupin's decline is held too.
- `wolf_sema`'s census row moves `min` to typed and `zip` to its reason.
- The prelude table's test moves `zip` off W0304, and `wolf prelude` lists `zip` with W0304 "no".

Controls, unchanged: a module's own `fn min` keeps W0304 and runs its own (`lints/shadow_prelude*`
stay green); every other ambient name keeps its census answer.

## 4. For the lupin mirror

lupin's `min` takes any number of integers of any widths and checks no literal's fit. After this
change the rows `min_one_argument`, `min_three_arguments`, `min_mixed_types` and `min_literal_fit`
are divergences: the compiler rejects, lupin runs. lupin's mirror narrows `min` to the clause:
two operands of one integer type, the codes and spans above. Its `zip` decline and its W0304
exemption of `zip` already agree.

## 5. What could go wrong

- The mem tier (`wolf_mem`'s lowering) may refuse a call it does not know before the checked
  machine runs. The checked rows will say, and the fix then names the call there as it names
  `fence`.
- Constant folding: `min(3, 4)` with literal operands may reach the folder. The answer must be
  the same; the signed row has literal operands for that reason.
- Release optimizations may turn the branch into a select. That is permitted, and the stdout rows
  hold the answer.
- Not taken: a variadic `min`, and `min` over `char`, `byte`, `str` or floats. A generic
  `min[T: Ord]` would need `Ord` dispatch on every tier. The clause says the integer family is
  this edition's.
