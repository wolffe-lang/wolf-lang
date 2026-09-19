# `wolf test --json`: the wolf-test/0 event stream

The machine half of the built-in test framework (D34/D36, sprint s39).
One JSON object per line on stdout; stderr stays the human channel
and carries the diagnostics and warnings. Every object carries
`"schema": "wolf-test/0"`.

Versioning: the schema is versioned from day one. `wolf-test/0` is
pre-stable. It may change only by bumping the version string, and
the D36 fossilization point is s51 (`wolf test --json` consumed by CI
tooling), where the then-current version freezes. Within a version,
additions are new optional keys only. Consumers must ignore unknown
keys; the conformance test
(`crates/wolf_driver/tests/test_cmd.rs::json_stream_conforms_to_wolf_test_0`)
rejects unknown *events*.

## Events

### `suite`: one per test file that yields tests

```json
{"schema":"wolf-test/0","event":"suite","file":"corpus/test/assert_test.lu","tests":2}
```

- `file` (string): the test file, forward-slashed.
- `tests` (integer): tests discovered in it, before filtering.

### `test`: one per test

```json
{"schema":"wolf-test/0","event":"test","file":"corpus/test/assert_test.lu",
 "name":"test_arithmetic_holds","status":"pass","detail":"exit(0)"}
```

- `name` (string): the `test_*` fn. `"main"` for a black-box file;
  `"<file>"` for a file-level outcome (compile failure, ladder
  refusal).
- `status` (string): `pass`, `fail`, `rejected`, or `unsupported`.
  Three ways not to pass, and they are three different facts:
  - `fail` — the test RAN and found something.
  - `rejected` — the compiler rejected the file, so it never ran
    (added s169, wolf-lang#157: `wolf test` spelled this `fail` too,
    and "your test found a bug" and "your module does not build" were
    one column). A DOCTEST that does not compile stays `fail`:
    compiling is what a doctest asserts, and only a precondition for a
    test file.
  - `unsupported` — the conservatism ledger: the checked machine
    refused the construct.

  All three fail the run, since a green run means every discovered
  test ran and passed. A consumer that does not know `rejected` sees a
  status it cannot classify, never a silent pass — the value is new,
  the schema version is not, and every non-`pass` status has always
  been a red row.
- `detail` (string): the verdict. `exit(N)`, `trap(kind)`,
  `ub(mem.ub)`, a refusal construct, or `does not compile`.
- `stdout`, `stderr` (strings): present when `status != "pass"`, and
  they hold what the test printed.

### `summary`: exactly one, always the last line

```json
{"schema":"wolf-test/0","event":"summary","passed":3,"failed":1,
 "rejected":0,"unsupported":0,"filtered_out":0,"stopped_early":false}
```

- Counters are integers; `stopped_early` is true under `--fail-fast`.

## Exit codes

- `0`: every discovered, unfiltered test passed. Zero tests counts.
- `1`: any failure, rejected file, unsupported test, or compile error
  ([conf.exit.test]: a runner that PRODUCED a report with a
  non-passing row exits 1; it exits 2 only when it could not produce
  a report at all). A schedule
  divergence under `--schedules=N` lands here too, and the finding's
  `detail` carries the diverging seeds and the `--replay=` line
  (spec/07 `[sched.flags]`).

  On the native lane that `--replay=` line is measured before it is
  printed: the driver re-runs the diverging seed a few times, and
  `detail` either states that the replay was confirmed or states what
  the seed does not pin. A decimal seed reaches `wolf_rt`'s scheduler
  PRNG (steal victims, `select` tie-breaks), while cross-task arrival
  order on the worker pool is not derived from it, so an
  arrival-order divergence need not come back. `[sched.stable]`'s
  byte-identical guarantee is stated over a recorded decision stream
  (`w1-`/`ev:`), which the native lane does not yet emit.
- `2`: usage or environment error. A malformed `--replay` schedule
  spec, `--schedules` combined with `--replay`, or `--chaos`, whose
  injection engine is a parked c07-closeout handoff.
