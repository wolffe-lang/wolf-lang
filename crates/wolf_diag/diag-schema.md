# wolf diagnostic JSON schema — version 1

The machine format emitted by `wolf … --error-format=json`: **one JSON
object per diagnostic, one object per line**, on stderr. It mirrors the
full structured `Diagnostic` value and is a *superset* of the
differential protocol's diagnostic record (spec/06 `[proto.record.diag]`
— `{code, span, severity}`), which stays minimal on stdout.

Compatibility contract: within schema version 1, fields are never
removed or re-typed; new fields may be added (consumers must ignore
unknown keys). A breaking change increments `diag_schema`. The
compatibility test lives in `tests/json_schema.rs` next to the fixture
`tests/fixtures/diag-schema-v1.jsonl`.

## Top level

| field         | type     | meaning                                          |
|---------------|----------|--------------------------------------------------|
| `diag_schema` | int      | schema version; `1`                              |
| `code`        | string   | registered code, `E####`/`W####` (registry.rs)   |
| `severity`    | string   | `"error"` or `"warning"`                         |
| `message`     | string   | the headline sentence (VOICE.md); never matched by tools — codes and spans are the stable surface |
| `primary`     | span obj | where it went wrong                              |
| `secondary`   | array of span obj | why (may reference other files)         |
| `notes`       | array of string | free-standing prose                        |
| `suggestions` | array of suggestion obj | concrete fixes                     |
| `row_diff`    | row-diff obj | *optional* (present on E06xx error-row diagnostics only) |

## Row-diff object (s15, D30)

The structural error-row delta — tags only, never whole rows:

| field     | type            | meaning                                     |
|-----------|-----------------|---------------------------------------------|
| `missing` | array of string | tags the target row lacks, source-rendered (`Io(IoError)`) |
| `extra`   | array of string | tags present but not required               |

## Span object

| field   | type          | meaning                                        |
|---------|---------------|------------------------------------------------|
| `file`  | int           | file index within the run's source map         |
| `span`  | `[int, int]`  | byte-exact half-open `[lo, hi)` (s07 contract) |
| `label` | string        | may be empty                                   |

## Suggestion object

| field           | type   | meaning                                        |
|-----------------|--------|------------------------------------------------|
| `message`       | string | the fix as prose                               |
| `applicability` | string | `"machine-applicable"` \| `"maybe"` \| `"has-placeholders"` |
| `edits`         | array  | edit objects, applied together                 |

Edit object: `{"file": int, "span": [lo, hi], "replacement": string}` —
replace the span's bytes with `replacement` (zero-width span =
insertion). Only `machine-applicable` suggestions may be applied without
a human (`wolf fix`, D34; LSP quick-fix, s52).

## Example line (wrapped here for readability; emitted as one line)

```json
{"diag_schema":1,"code":"E0209","severity":"error",
 "message":"wolf has no negative indexing — indexes count from the end with `^`",
 "primary":{"file":0,"span":[24,26],"label":"this index is negative"},
 "secondary":[],"notes":[],
 "suggestions":[{"message":"index from the end: `^1`",
   "applicability":"machine-applicable",
   "edits":[{"file":0,"span":[24,25],"replacement":"^"}]}]}
```
