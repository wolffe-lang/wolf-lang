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
  `ub(mem.ub)`, a refusal construct, or `does not compile`. A verdict a
  native run gave ends ` [native]` (see `native`). Explicit `--native`
  and `--release` runs mark their tier here too; they are not fallbacks.
- `assertion` (object, optional): present for a failed assertion. Its
  `message`, `site`, `left` and `right` keys are strings or `null` when
  absent. `site` is `file:line:col`, with 1-based coordinates; an inlined
  release assertion can name its caller's site. An equality condition
  (`assert(left == right)`, including an `assert_eq` helper written in
  that form) retains the two already-evaluated values without evaluating
  either operand again. Comparand formatting uses reporter storage, not
  the program's region budget. Messages retain newlines and quotes as JSON
  string data. The default checked machine, native tests and release
  tests carry the same object; passing test events do not gain it.
  These diagnostics do not add a new `assert_eq` builtin or infer values
  from arbitrary helper-function names.
- `native` (string, optional; s250, wolf-lang#817): present when the
  checked machine refused a `main` test and the verdict is the one
  native run that stood in for it; the value is the checked machine's
  refusal, verbatim (`"step budget exhausted"`, or the construct it
  named). A `pass` with this key is green on the native tier, not
  checked. `--no-native-fallback` turns the refusal into the verdict
  (`unsupported`) instead, and the key is absent.
- `stdout`, `stderr` (strings): present when `status != "pass"`, and
  under `--nocapture` on every test, a passing one too (s250,
  wolf-lang#810: the record a passing test's `SKIP:` lines are counted
  from); they hold what the test printed.

### `summary`: exactly one, always the last line

```json
{"schema":"wolf-test/0","event":"summary","passed":3,"failed":1,
 "rejected":0,"unsupported":0,"filtered_out":0,"stopped_early":false,
 "native":0}
```

- Counters are integers; `stopped_early` is true under `--fail-fast`.
- `native` (s250, wolf-lang#817): how many of `passed` carry the
  `native` key — passes the checked machine refused and a native run
  gave. It counts toward nothing else: the exit status is the one the
  run would have without it (reporting, not a new failure). The human
  summary says `N passed, M of them native` when it is nonzero.

## Compiled test tiers

The default remains the checked machine with the existing native fallback.
`--native` and `--release` explicitly compile and run each discovered test
function in isolation, including a `test_*` function in a file with its
own `main`. They cannot combine with `--schedules`, `--replay` or
`--no-native-fallback`; such combinations exit 2. A tier unavailable on
the host remains an explicit failure or refusal, never a checked pass.
Extra assertion-reporting code is emitted only for test executables;
ordinary builds and freestanding trap hooks keep their previous ABI.

## `--nocapture`

s250 (wolf-lang#810). A test's standard output and standard error are
shown as the test writes them — the checked machine echoes each write,
a native run's pipes are pumped — instead of only on a failing row. In
the human output each stream goes to its own (and a failing row does not
repeat what was shown); under `--json` both go to stderr, since stdout
is this stream, and every `test` event carries `stdout` and `stderr`. A
test the checked machine refuses partway runs again natively, so under
`--nocapture` its output can appear twice; a stderr line says when.

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
