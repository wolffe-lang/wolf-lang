# s259 — the refusal site

## Prediction

Written from trunk `06cff722` before the first implementation change.

1. `report_build_stop` prints the build/run refusal, but `compile_native`
   has already flattened `NotYet` into `construct @lo..hi`. The offsets
   are file-relative: `Span` carries a separate file id, discarded by
   that flattening. Keeping the span and rendering it against `Sources`
   should identify the second file without a byte hunt.
2. The new unresolved-name witness changes from `unsupported` to
   `fail(E0301)` on checked, native and release. No existing corpus row
   should change class. The qualified witness should run everywhere.
   The three operator rows and s258's two member rows should not move.
3. Checked/static `conform-run --json` already records
   `x-unsupported-construct` and `x-unsupported-span: [lo, hi]`, but no
   file. Native/release flatten the offsets into the construct and lack
   the separate span. Preserve existing fields and add extension keys
   for the source file and 1-based line/column.
4. The contract's tuple/generic examples predate their implementations:
   `type_from_bracket_arg` already constructs tuples and rigid generic
   types, and `[gram.amb.brackets]` explicitly admits tuple arguments.
   Preserve those successful paths, plus all actual downstream declines;
   do not reintroduce the closed #349 gap. Actual ambiguous bracket
   arguments (value-bound heads, non-type expressions, unsupported
   applications) remain refusals, named by their shape.
5. The anchor file should remain byte-identical (no spec edit). Adding
   the two rows should add two lowering-ledger entries and require
   reviewed lexer/parser snapshots; E0301's fixture index may grow.

## Recommendations

R-s259-1: retain the existing refusal headline and ledger explanation,
remove byte offsets from human refusal headlines, and render the real
source span with the same file/line/column and caret machinery as errors.
No error code is invented for a conservatism refusal.

R-s259-2: preserve tuple and generic-parameter handling already present
at the base. The contract's real-decline examples are stale; only actual
remaining declines should be renamed, not recreated.

## Inputs, verified

| input | identity |
|---|---|
| base | wolf-lang `06cff722` |
| wolf control | 0.2.34, pin/record `9da7bdc` |
| wolf linux archive SHA-256, read twice | `88a9dd3da1c9a0e38c08eb6e2de9c1ef04d1ed93dcf51024d8dbbb76890c1821` |
| lupin reference | 0.1.55, record `e642f26`, version-line pin `294d626` |
| lupin linux archive SHA-256, read twice | `1152abc0bef65d334b94bc881a26463fac345919cc2ca07a291796e6048a8a38` |

The inputs are the driver, `call_container_ctor`, `type_from_bracket_arg`,
the diagnostic renderer and E0301 registry entry, `[conf.exit.refused]`,
`[gram.amb.brackets]` (in `spec/01-grammar.md`, not the contract's
`02-grammar.md`), and wolf-lang issues #889, #890, #879 and #349.
The last two issues are closed. No interpreter implementation was read.
All builds and black-box measurements run on kasumi under `taskset -c
4-7`, with `CARGO_INCREMENTAL=0`; nothing is built on the Mac.

## The motivating transcripts

Read-only source:
`~/LocalLLM/Projects/logs/spell/pool-wollf-v7sf3/complete00.md`,
lines 1556 and 1734–1808. The model is building a directory containing
several `.lu` files:

```text
wolf build: cannot compile this yet — methods on builtin types (the std surface) @605..617 (pipeline is honest through `resolve`; the conservatism ledger, not a bug in your program)
```

The model first looks in `main.lu`; byte 605 lands in a comment. It then
copies `dict.lu` alone into `/tmp/wolf_test_dict/main.lu`, gets the same
refusal, and only then knows which file to inspect. The byte offset was
file-relative all along; its file identity had been discarded.

Read-only source:
`~/LocalLLM/Projects/logs/mk/mlx-wolllf-v7sf3/completed00.md`,
lines 22380–22485. The project imports `std.process` but writes
`Map[str, Child]()` rather than `Map[str, process.Child]()`:

```text
wolf build: cannot compile this yet — this prelude container instantiation (generic data) @8612..8629 (pipeline is honest through `resolve`; the conservatism ledger, not a bug in your program)
```

After three repetitions, the model concludes that `Child` cannot be a
map value. The missing qualifier, not the container's capabilities, is
the problem. The new headline for this case is:

```text
error[E0301]: nothing named `Child` is in scope
```

Its caret points at `Child`, and its note says:

```text
`Map[…]` is generic application here; each argument must name a type — import and qualify the type you mean
```

## What changed

`BuildStop::Refused` retains an optional `Span` instead of flattening a
`NotYet` into text. Build and run print the existing headline and ledger
sentence, followed by `wolf_diag::render_source_span`: the same
`render_group` machinery errors use, without inventing an error code.
The typecheck, memory, WIR and checked-executor refusal paths use that
renderer too. Backend refusals without source identity remain honest
about its absence; no file or `(1, 1)` is guessed. Dump forms retain their
byte offsets.

`conform-run` retains its existing verdict, phase, empty diagnostic
array, construct string and `[lo, hi]` span extension. Registered sites
add `x-unsupported-file`, `x-unsupported-line` and `x-unsupported-col`.
The line and column are 1-based; the offsets remain byte offsets. The
record's top-level `file` is still the entry, not the refusal's file.

The bracket reader separates a resolved type, an unresolved name and a
shape still declined. Container constructors report the resolver's
E0301 wording at the unresolved argument, return the error type, and
avoid a secondary key-type or mismatch error. Nested builtins and tuples
propagate the unresolved leaf; both `Map` arguments, `List` and `Pool`
are covered. Other bracket consumers retain their own reporting policy.
Tuple and rigid generic types already admitted at the base stay admitted.
Actual remaining refusals name a value-bound name/head, a non-type or
ambiguous expression, a wrong type-argument count, or an unsupported
nested type application. No declined construct is enabled by this pass.

R-s259-1 and R-s259-2 were accepted by the orchestrator. R-s259-3 was
also accepted: the qualified witness lives in
`corpus/rows/ctor_arg_qualified/main.lu` with its own
`std/process/process.lu`, rather than adding `rows/std` and changing
unrelated rows. The fixture declares only `pub struct Child { slot:
int }`; it starts no process. A plain dev build has no bundled std:
the flat positive row otherwise fails at `use std.process` before it
can reach the constructor. The 0.2.34 archive's real bundled std runs
the qualified program too. The directory-is-the-module rule requires
`std/process/process.lu`, not `std/process.lu`.

The driver witness copies the checked-in two-file module under
`crates/wolf_driver/tests/fixtures/refusal_site/` to a unique temporary
directory. The refusal is in the second file, line 3, column 5; the
human output and every machine's record must agree. A separate driver
test holds both the missing-name corpus row and the mk spelling to
E0301 on checked, native and release, and holds `wolf build` to exit 2.
The oracle test requires lupin when CI's strict sibling flag is set.

README read, current: command list, tier and archive descriptions,
installation and build paragraphs need no change for these diagnostics.

## Witness measurements

`ev/witness-matrix.jsonl` on kasumi holds 79 records (all tool exits 0).
Each wolf column below is checked / native / release, obtained with
`conform-run --json --checked|--native|--release`; `wolf run --checked`
is not the checked executor.

| witness | 0.2.34 | base `06cff722` | implementation | lupin 0.1.55 |
|---|---|---|---|---|
| `ctor_arg_unresolved` | unsupported, all three | unsupported, all three | fail(E0301), all three | fail(E0301) |
| `ctor_arg_qualified/main` | exit(0), all three | exit(0), all three | exit(0), all three | exit(0) |
| qualified program, real archive std | exit(0), all three | exit(0), all three | exit(0), all three | — |
| `row_operand_add`, `compare`, `rhs` | fail(E0409), all three | fail(E0409), all three | fail(E0409), all three | fail(E0409) |
| s258 `row_operand_member`, `method` | unsupported, all three | fail(E0409), all three | fail(E0409), all three | exit(0) |

The last line is s258's pre-existing divergence, not a move in this pass.
No toolchain leg skipped. The driver test also verifies the name's exact
span and the absence of a conservatism extension on the E0301 records.

The two-file module, measured with `wolf build` before and after (both
exit 4), is:

```text
# before
wolf build: cannot compile this yet — methods on builtin types (the std surface) @46..66 (pipeline is honest through `resolve`; the conservatism ledger, not a bug in your program)
# after
wolf build: cannot compile this yet — methods on builtin types (the std surface) (pipeline is honest through `resolve`; the conservatism ledger, not a bug in your program)
 --> crates/wolf_driver/tests/fixtures/refusal_site/second.lu:3:5
  |
3 |     value.not_compiled()
  |     ^^^^^^^^^^^^^^^^^^^^
  |
```

The original construct and `[46, 66]` span remain in every lane's JSON;
the added location names `second.lu`, line 3, column 5. The entry remains
`main.lu`. The tabbed renderer snapshot separately distinguishes byte
column 2 from the caret's expanded display column; an unregistered-file
test proves the renderer does not invent a location.

`spec-extract` is byte-identical to the base: 693 anchor keys before and
after, no additions and no removals. SHA-256:
`47e5e3c8a7334183c275469872fd34e6e74e7847157ff5a4bb2e9b98edf93398`.
The lowering ledger gains exactly two content lines: the qualified row
`lowers`, the unresolved row `not mem-clean (pre-wir rungs own it)`.
There are six new reviewed lexer/parser snapshots (the two entries and
the fixture member), one source-site renderer snapshot and one E0301
snapshot. `diag-catalog` adds two E0301 fixtures. No spec sentence or
anchor changed.


## The prediction, scored

1. Right: `report_build_stop` prints the flattened refusal and the byte
   offsets are file-relative; the span still knew the second file.
2. Right: the witness measurements match the predicted verdicts, and
   both complete same-corpus comparisons move exactly one row's class.
3. Partly wrong: checked/static records have the two existing extension
   keys and no location, as predicted. But semantic refusals in native
   and release `conform-run` also stop in that same static ladder before
   `compile_native`; their existing fields were already the same. Only
   a later native compilation refusal passed through the flattening
   path. Every spanned path now retains its source identity.
4. Right: tuple and rigid generic type arguments are already admitted.
   The known-name checks matter too: `List[never]` and `List[Scope]`
   remain declines, not false claims that those names are out of scope.
5. Right: anchors are byte-identical, two lowering-ledger entries were
   added, and the E0301 fixture list grew.

## Differ and conservatism ledgers

Same corpus on both sides: 1,217 files, 1,155 non-member entries. The
implementation is the code through `5429021d`; subsequent commits are
prose only. Every tool is used without a bundled std beside its binary
except where the command explicitly supplies a corpus fixture root.

R-s259-4 was accepted: `xtask differ` does not pass the fixture std root
that `xtask corpus`, lowering tests and lane coverage pass. A small
executable wrapper in `~/lanes/s259/fixture-tool.sh` adds
`--std-root <entry-parent>/std` when that directory exists, for both
wolf and lupin. No harness code changed in this PR. The measured command
is `cargo xtask differ "<wrapper> <wolf>" "<wrapper> <lupin>" --triage
--checked|--native` (the already-built `target/debug/xtask` executable
was used for these passes). The raw command without the wrapper is
reported separately below. The missing fixture-root dispatch is a
follow-up issue to file, not a silent configuration difference.

Fixture-aware ledger:

| tier | subject | agreements | completeness | SOUNDNESS | unsupported | hard divergences |
|---|---|---:|---:|---:|---:|---:|
| checked | 0.2.34 control | 822 | 257 | 0 | 107 | 21 |
| checked | base | 819 | 250 | 9 | 105 | 29 |
| checked | implementation | 819 | 251 | 9 | 104 | 29 |
| native | 0.2.34 control | 801 | 257 | 14 | 115 | 34 |
| native | base | 804 | 250 | 23 | 106 | 43 |
| native | implementation | 804 | 251 | 23 | 105 | 43 |

Raw ledger (no fixture-root dispatch):

| tier | subject | agreements | completeness | SOUNDNESS | unsupported | hard divergences |
|---|---|---:|---:|---:|---:|---:|
| checked | 0.2.34 control | 815 | 265 | 0 | 106 | 21 |
| checked | base | 812 | 258 | 9 | 104 | 29 |
| checked | implementation | 812 | 259 | 9 | 103 | 29 |
| native | 0.2.34 control | 793 | 265 | 14 | 115 | 34 |
| native | base | 796 | 258 | 23 | 106 | 43 |
| native | implementation | 796 | 259 | 23 | 105 | 43 |

All twelve triage passes exit 1 because of the pre-existing hard
findings in the last column. That is a measured red, not a green test
claim. The control differs from the base by trunk's intervening work;
only base versus implementation isolates this pass.

In both configurations and both tiers, the class delta is exactly one
file: `rows/negative/ctor_arg_unresolved.lu`, `unsupported` to
`Completeness` with `a: fail(E0301), b: fail(E0301)`. The triage ritual
calls a fail-pinned row completeness even when the reference agrees.
No other class moves; agreements, SOUNDNESS and hard divergences hold.
Checked JSON ledger lines differ only by that new line. Native also
prints different exit numbers for the three existing use-after-free
UB rows (`atomic_ub_uaf`, `unsafe_ub_uaf`, `volatile_ub_uaf`), inside the
same SOUNDNESS class; that is UB, not a new verdict class.

| coverage, A / B / both | fixture-aware base and implementation | raw base and implementation |
|---|---|---|
| checked | 743 / 930 / 714 | 736 / 930 / 707 |
| native | 748 / 930 / 713 | 740 / 930 / 705 |

The default static `differ-self` shape was measured before and after by
passing the same binary as both arguments, with no execution-tier flag.
Both the raw and fixture-aware commands give:

| subject | divergences | conservatism ledger | forward pins | entries executed |
|---|---:|---:|---:|---:|
| base against itself | 0 | 22 | 3 | 0 |
| implementation against itself | 0 | 21 | 3 | 0 |

No run coverage is claimed by that static self-check. The gauntlet's
literal `cargo xtask differ --self` is in its final log too.

## Plant and final verification

`s259-plant`, `f7566ced`, reverts sema commit `cfebde49` but retains the
independent corpus and driver witnesses. CI run **39883926644**, job
**120187593494**, `lint + test + corpus (linux x86-64)`, dispatched with
`workflow_dispatch -f jobs=lint-test` (`tier partial`), is red at the
`test` step on exactly
`an_unresolved_container_argument_is_e0301_on_every_lane`: the checked
record says `unsupported`, not `fail(E0301)`. Its three sibling tests
(source site, qualified argument, lupin) pass. No other test failed.

Cargo stops at that binary, so the corpus step is measured separately
on kasumi at the same plant: exit 1, `1217 file(s), 2 bad`, both
complaints on `ctor_arg_unresolved.lu` only (an unbacked fail pin and a
header claiming `typecheck` when the refusal stops at `resolve`).
The source-site test also distinguishes the base's byte-only output
from the implementation's file/line/column frame, as shown above.

One initial preflight is explicitly excluded: all workspace tests
passed, then rustdoc could not find a dependency after a concurrent
rebuild in the same target directory invalidated its crate metadata.
The final-head gauntlet runs with stable sources and no other cargo
process in that target directory, under `taskset -c 4-7`,
`CARGO_INCREMENTAL=0`, `LUPIN` set to the verified reference,
`WOLF_PAIRING_REQUIRE_SIBLING=1`, `WOLF_RT_NONE_REQUIRE=1` and
`RUST_TEST_NOCAPTURE=1`. Its complete output and direct exit status are
`~/lanes/s259/ev/final-ci.log` and `final-ci.rc`; no pipe determines the
exit status. The four new driver tests have no skipped leg in the
witness matrix or the targeted `--nocapture` run.

Evidence on kasumi, under `~/lanes/s259/ev/`:

| evidence | files |
|---|---|
| all witness tiers, including real std | `witness-matrix.jsonl`, `witness-matrix.rc` |
| human before/after and mk case | `base-refusal-screen.log`, `head-refusal-screen.log`, `head-mk-screen.log`, adjacent `.rc` files |
| raw triage passes | `rows-base-differ-*.log`, `rows-strip-differ-*.log`, `head-differ-*.log`, adjacent `.rc` files |
| fixture-aware triage passes | `fixture-base-differ-*.log`, `fixture-strip-differ-*.log`, `fixture-head-differ-*.log`, adjacent `.rc` files |
| static self-check, before/after | `raw-base-self.log`, `raw-head-self.log`, `fixture-base-self.log`, `fixture-head-self.log` |
| CI and local plant | `plant-ci.log`, `plant-driver.log`, `plant-corpus.log`, adjacent `.rc` files for local runs |
| final-head gauntlet | `final-ci.log`, `final-ci.rc` |

Close wolf-lang#889 and #890 at merge. File the separate wolf-lang issue
for `xtask differ`'s missing fixture-root dispatch. Nothing was filed
or closed by this lane.
