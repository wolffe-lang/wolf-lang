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
