# s289: the diagnostics sweep, third change: the prelude's `min` on every tier

The third of the sweep's three changes (`notes/s289-prediction.md`), from the second's head
`743c6770` (`notes/s289-the-census-the-linker-and-the-manifest.md`). It covers wolf-lang#708 as
ruled (R-s289-7):

- **`[type.min]`:** `min(a, b)` takes two values of one integer type and answers the smaller, as
  that type.
  - It compares signed for a signed type and unsigned for an unsigned one, as `<` does.
  - Each operand is evaluated once, left to right. It never traps.
  - Arity is E0402. An operand outside the integers, or a second integer type, is E0401 at that
    operand. A literal that does not fit the other operand's type is E0415.
  - Until this change the checked machine, native and release declined every call ("a prelude
    helper with no signature yet", the second change's interim).
- **`[type.min.zip]`:** the free `zip` has no meaning; `std.list`'s `xs.zip(ys)` is the spelling.
  Every use is declined with that reason, and a module's own `zip` draws no W0304.

## Forbidden

Footprint: cross-cutting

The diff covers:
- a spec clause (`spec/10-types.md` §18, one line of `spec/11-os.md`, the regenerated
  `spec/anchors.json`, `spec/prelude.json`'s two rows);
- the checker and the prelude table (`wolf_sema`);
- the WIR lowering (`wolf_wir`) and the checked machine (`wolf_mem`'s `ubcheck`);
- corpus rows with their lexer and parser snapshots and nine lines of the lowering ledger;
- a four-machine test with two fixtures, the census and prelude-table rows, the changelog and
  notes.

There is no CI, pin or toolchain change, and no runtime or backend change. Clean room: the spec,
the compiler's own source, and black-box runs of the published lupin 0.1.60. No label, merge,
tag, release or issue closed, and no attribution trailer. The plant is pushed without a PR.
Nothing ran under `/tmp` or `/var/folders`, and no commit was made on a Linux host.

What existing programs mean, by decision:
- **A program that calls `min`** with two operands of one integer type now compiles and runs on
  every tier. With any other operands it is a compile error where it was declined.
- **A program with its own `fn zip`** loses its W0304.
- **The corpus:** no entry reaches either name's old decline (the first change's scan;
  `wordcount.lu` and `grammar/closure_extent.lu` stop earlier). Nothing moves but this change's
  rows.

## Inputs, verified

- **Base:** the second change's head `743c6770`.
  - The base was measured and the rows and fix were written on its earlier head `353ee08c`,
    whose code is identical to the second change's checked commit `934e36ac`.
  - The second change then gained one test fix (`e03952a3`, the refusal-site rows read past the
    linker note) and its note, and this change was rebased onto it with the same patches
    (`m3-rebase-map.txt`). The logs are named by the commits they ran at.
- **lupin 0.1.60**, as in the first change.
- **The spec:**
  - `[type.comb.set]` (`std.list`'s `zip`);
  - `[conf.resolve.ambient]` and W0304;
  - `[type.numlit]`;
  - `[os.host.sigs]`'s list of names typed by their own clauses.
- **Builds:** every build ran in a worktree pinned to a commit, on the second arm64 macOS
  machine, behind its build lock.

## Prediction

`notes/s289-min-prediction.md`, committed (`c6635ed5`) before the first row, with the base
measured (`feb3b5fc`). It held, with three corrections:
- **A folded compare.** The first take of the lowering (`7753e3c4`) branched on the compare
  whatever it was. When both operands are known, the builder folds the compare to a constant and
  the branch to one arm, which left the other arm a block with no predecessor beside the join.
  - Native and release then panicked in SSA construction at the next use of a local (`min_signed`
    and `min_unsigned` red, `m3-check-292bc30a-air.log`).
  - `54cc3875` answers a folded compare with its operand directly, and one value twice with
    itself.
- **Two consequences the prediction did not name**, found by the full take
  (`m3-full-71e97d82-air.log`):
  - `wolf_sema`'s `prelude_names` pins the table's counts. With `min` and `zip` anchored and `zip`
    off W0304, it reads 28 unanchored names (was 30) and 220 W0304 names (was 221)
    (`868b0ef7`).
  - The catalog lists every snapshot that quotes a code as that code's fixture, and the new rows'
    lexer snapshots quote E0401, E0402 and E0415. `docs/diagnostics.md` was regenerated
    (`62d4466e`).

| row | at the rows' commit `78511cf9` | at the fix | lupin 0.1.60 |
|---|---|---|---|
| `typecheck/min_signed.lu` | declined | `3 -5 7`, `i64`'s ends | equal |
| `typecheck/min_unsigned.lu` | declined | `7 7`, `1 1`, `9` | equal |
| `typecheck/min_evaluates_once.lu` | declined | `tick 2`, `tick 1`, `1` | equal |
| `typecheck/min_one_argument.lu` | declined | E0402 at the call | runs (`5`) |
| `typecheck/min_three_arguments.lu` | declined | E0402 at the call | runs (`1`) |
| `typecheck/min_mixed_types.lu` | declined | E0401 at the `i64` | runs (`3`) |
| `typecheck/min_str.lu` | declined | E0401 at `"b"` | declined |
| `typecheck/min_literal_fit.lu` | declined | E0415 at `300` | runs (`3`) |
| `lints/zip_own.lu` | W0304 (the warning ledger) | warning-clean, `12` | equal |
| the free `zip`, called | "a prelude helper with no signature yet" | declined, `xs.zip(ys)` named | declined |
| the census: `min`, `zip` | the interim decline | `min()` E0402, `min` a builtin value; `zip` declined, all three uses | — |
| `wolf prelude`: `zip` | W0304 yes | W0304 no | — |

Controls, unchanged:
- a module's own `fn min` keeps W0304 and runs its own;
- the first change's four-machine test, 15 of 15;
- `prelude_list`: `spec/prelude.json` is `wolf prelude --json`, byte for byte.

## Rulings

As recommended in the prediction and ruled:
- **R-s289-7:** `min(a: T, b: T) -> T` for one integer type `T`, in a new clause, lowered to a
  compare and a select in WIR and on the checked machine. `zip` is refused with lupin's reason and
  exempt from W0304.
- **R-s289-10:** the third change stacks on the second's head.

## For the lupin mirror

lupin 0.1.60's `min` takes any number of integers of any widths and checks no literal's fit. Four
rows are therefore divergences: `min_one_argument`, `min_three_arguments`, `min_mixed_types` and
`min_literal_fit` (the compiler rejects; lupin runs). The mirror narrows `min` to the clause: two
operands, one integer type, the codes and spans the four-machine test holds. Its `zip` decline and
W0304 exemption already agree.

## Evidence index

`notes/s289-evidence/`:
- `m3-rebase-map.txt`: the logged commits and the commits they are now, with the same patches.
- `m3-base-353ee08c-air.log`: every probe under `probe-min/` on the four machines at the base.
- `m3-red-c2e51c03-air.log`: the rows red at their commit.
  - The four-machine test: 10 FAILED by name, the own-`min` control ok.
  - The census and the prelude table red.
  - The corpus: 9 bad, each a new row by name.
  - The lexer and parser snapshots and the ledger written.
- `m3-check-292bc30a-air.log`: the fix's first take. The two run rows with a later local panicked
  natively (above). `spec/anchors.json` was regenerated there, and the rows' formatting was red.
- `m3-check-9f16bfdb-air.log`: the take at the fold. Every row and control green; spec-extract
  and conformance; the corpus 1,351 files, 0 bad; fmt and clippy; the ledger written.
- `m3-check-b53694c7-air.log`: the same checks at the ledger's commit, green, the tree clean.
- `m3-full-71e97d82-air.log`: the full take at the note's first commit.
  - Every step of the checks above green, and `wolf_wir`'s and `wolf_mem`'s library tests.
  - `wolf_sema`'s `prelude_names` and the catalog check red (the two consequences above).
- `m3-full-62d4466e-air.log`: the full take at the fixes, every step green, the catalog in sync,
  the tree clean.
- `rig/m3-red.sh`, `rig/m3-check.sh`: the scripts that ran them.

The Linux record of this head and the plant's record are cited in the PR.

## Done-when

Commits:
- `feb3b5fc`: the base measured; `c6635ed5`: the prediction.
- The rows:
  - corpus: `f1f757e1`, `1bbbc41e`, `df9e39b1`, `5b2c6168`, `3e14c08c`, `bdbb7fae`, `46000038`,
    `1ca487a7`, `25e80248`;
  - `c43a9e6e` (four machines), `c3328f15` (the census), `78511cf9` (the prelude table);
  - snapshots: `e186314a`, `f2557e09`.
- The fix:
  - `1f79b17c` (the clause);
  - `3add1ff8` (the table's anchors and `zip`'s exemption);
  - `24d747e5` (the checker);
  - `7753e3c4` and `54cc3875` (WIR);
  - `bc0f5376` (the checked machine).
- `ad6a914b`: formatting. `c6def8db`: the anchors. `3a12f035`: the ledger. `3a8b25c6`: the
  changelog. This note and its evidence.
- `868b0ef7`: the prelude's counts. `62d4466e`: the catalog.

Test targets:
- [x] the prediction committed before the first row (`c6635ed5`)
- [x] the rows red at their commit, by name (`m3-red-c2e51c03-air.log`)
- [x] the rows green at the fix on the checked machine, native and release, spans exact; lupin
      held to its present answers (`prelude_min_lanes`, 11 tests)
- [x] the census, the prelude table, `prelude_list`; the first change's four-machine test
- [x] spec-extract (two anchors added, none dropped), conformance, the corpus `0 bad` (1,351
      files), fmt, clippy (`wolf_sema`, `wolf_wir`, `wolf_mem`, `wolf_driver`, all targets,
      `-D warnings`)
- [x] the full take: `wolf_sema`'s suites, `wolf_wir`'s and `wolf_mem`'s library tests, the
      catalog (red at first, above; green at the fixes)
- [ ] the gauntlet (`cargo xtask ci`, every step's `END`) at this head on Linux x86-64
- [ ] the plant red by name, its controls green
- [ ] tier full on CI (`full-matrix`)
- [x] README read, current: it names neither `min` nor `zip`
