# `wolf test --json` — the wolf-test/0 event stream

The machine half of the built-in test framework (D34/D36, sprint s39).
One JSON object per line on **stdout**; stderr stays the rich human
channel (diagnostics, warnings). Every object carries
`"schema": "wolf-test/0"`.

Versioning: the schema is versioned from day one. `wolf-test/0` is
**pre-stable**: it may change only by bumping the version string, and
the D36 fossilization point is s51 (`wolf test --json` consumed by CI
tooling), where the then-current version freezes. Within a version,
additions are new optional keys only — consumers must ignore unknown
keys, and the conformance test
(`crates/wolf_driver/tests/test_cmd.rs::json_stream_conforms_to_wolf_test_0`)
rejects unknown *events*.

## Events

### `suite` — one per test file that yields tests

```json
{"schema":"wolf-test/0","event":"suite","file":"corpus/test/assert_test.lu","tests":2}
```

- `file` (string): the test file, forward-slashed.
- `tests` (integer): tests discovered in it (before filtering).

### `test` — one per test

```json
{"schema":"wolf-test/0","event":"test","file":"corpus/test/assert_test.lu",
 "name":"test_arithmetic_holds","status":"pass","detail":"exit(0)"}
```

- `name` (string): the `test_*` fn; `"main"` for a black-box file;
  `"<file>"` for a file-level outcome (compile failure, ladder
  refusal).
- `status` (string): `pass` | `fail` | `unsupported`. `unsupported`
  is the conservatism ledger — the checked machine refused the
  construct; it fails the run, because green must mean everything
  discovered actually ran.
- `detail` (string): the verdict — `exit(N)`, `trap(kind)`,
  `ub(mem.ub)`, a refusal construct, or `does not compile`.
- `stdout`, `stderr` (strings): present when `status != "pass"` — what
  the test printed.

### `summary` — exactly one, always the last line

```json
{"schema":"wolf-test/0","event":"summary","passed":3,"failed":1,
 "unsupported":0,"filtered_out":0,"stopped_early":false}
```

- Counters are integers; `stopped_early` is true under `--fail-fast`.

## Exit codes

`0` — every discovered, unfiltered test passed (including zero tests).
`1` — any failure, unsupported test, or compile error — schedule
divergence under `--schedules=N` included (the finding's `detail`
carries the diverging seeds and the `--replay=` line; spec/07
`[sched.flags]`).
`2` — usage or environment error (a malformed `--replay` schedule
spec, `--schedules` combined with `--replay`, or `--chaos`, whose
injection engine is a parked c07-closeout handoff).
