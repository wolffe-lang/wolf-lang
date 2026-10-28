# Build output and timings

`wolf build` compiles an entry file; `wolf run` compiles it and executes the
result. `wolf test` compiles each test file before running its tests. Build
flags for `run` go **before the entry file**; later words belong to the
program. `wolf build --help` lists the complete flag surface.

## Progress

On a terminal, stderr shows a single updating `Building` line: completed /
planned codegen units, the current unit, tier, phase, elapsed seconds in that
operation, and the number of reused object units. Before codegen the unit
count is not yet known. Release codegen units are whole-program clusters;
native codegen units are source modules. Parallel workers may finish in any
order. The displayed operation is the most recently started active pass or
unit (otherwise the package phase), not a sum of all workers' progress.
Narrow terminals shorten the line. Diagnostics clear it before printing.
The line is removed on completion, rejection, refusal, and terminal interrupt.

When stderr is not a terminal, fast builds add **no output**. After 30 seconds
without compiler or tool output, stderr receives a heartbeat:

```text
wolf build: still in midend/licm of edit (30 s)
```

While the build remains silent, another heartbeat follows every 60 seconds.
Actual compiler/tool output resets the initial 30-second idle deadline; phase
changes do not. Once a build has emitted a heartbeat, it ends with a line such
as `wolf build: finished release in 894.506s`; failure adds ` (failed)`.
`TERM=dumb` uses this non-interactive form, never cursor controls.

`WOLF_STATUS_AFTER=<seconds>` overrides the initial idle deadline, not the
60-second repeat interval. It accepts a finite decimal from 0 through 86400;
invalid values use 30. This is primarily a testing override.

`-q` / `--quiet` disables progress, heartbeats, completion lines, and text
 timing tables. It does **not** suppress diagnostics, `--verbose` output,
test results, or the executed program's output. An explicitly requested JSON
file is still written. Build/run/test carry these options; fmt/fix/doc do not.
`run` and `test` stop compilation reporting before program/test execution.
Native test fallback compilation is reported as a separate build. Script
cache hits that skip compilation have no build to report.

## Timing options

- `--timings`: a table on stderr when compilation ends, including an ordinary
  rejection/refusal. It labels scope, unit, phase, function detail, invocation
  count and elapsed seconds. It never writes stdout.
- `--timings=json:<file>`: writes the following versioned JSON document instead
  of the text table. Quote the whole flag when the path contains spaces. An
  empty/malformed option is a usage error; a file-write failure is a tool
  error, not a successful build. An existing file is replaced.

The options are mutually selected: if repeated, the last timing option wins.
No timing data is wired into performance gates by these flags.

## JSON schema: `wolf-build-timings/1`

```json
{
  "schema": "wolf-build-timings/1",
  "builds": [
    {
      "command": "build",
      "tier": "native",
      "success": true,
      "seconds": 0.15,
      "units": 1,
      "reused": 0,
      "rows": [
        {
          "scope": "unit",
          "unit": "root",
          "phase": "codegen",
          "detail": "",
          "count": 1,
          "seconds": 0.03
        }
      ]
    }
  ]
}
```

`builds` is in compilation order. A build/run invocation normally has one
record; test may have several checked compilations and native fallbacks.
The file is refreshed after each compilation in that command; a new command
starts a new document, not an append-only log. `seconds` is nonnegative elapsed
wall time for that compilation, excluding subsequent program execution.
`success` records compilation success, not a program/test result. Process
termination by a signal cannot promise a completed JSON file.

`units`/`reused` count actual codegen objects/clusters, not parsed files. Checked
test compilation, frontend failure, and WIR-only emission have zero codegen
units. `rows` is deterministically sorted by `(scope, unit, phase, detail)`.
`count` sums completed invocations and `seconds` sums their elapsed durations.
Empty source-module names become `root`. Package operations use the entry
stem or `package`; a test file's package operations use its display path.
Function detail is present where the operation has a function, and empty
otherwise; it is not a stable compiler-symbol identity across versions.

Scopes:

- `package`: load+resolve, typecheck, mem, WIR lowering, midend, codegen
  preparation, codegen, and link, when reached. Nested `load/lex/parse` and
  `resolve` refine load+resolve; they are **not additional wall time**.
- `unit`: lex/parse and resolve per source module; typecheck, mem/lower,
  mem/check and WIR lowering contributions attributed to a module/function;
  codegen per actual object unit. Cache hits include object-read time.
- `pass`: each executed midend pass, attributed to its source home and
  function; repeated visits/revisits accumulate. Native builds normally run
  no midend passes. `WOLF_MIDEND=1` retains package attribution where no
  whole-program homes are available.

**Do not add nested scopes or parallel contributions to infer wall time.**
Typecheck bodies and release codegen can overlap, so their elapsed sums can
exceed wall time. Global signature building, graph/item work, mem solving,
lowering setup, whole-program orchestration and verification remain package
work; the compiler does not invent per-module allocations for them. Pass
elapsed time includes the pass's own verification, not just its transform.
This is an elapsed-time instrument, not CPU profiling or loop attribution.
