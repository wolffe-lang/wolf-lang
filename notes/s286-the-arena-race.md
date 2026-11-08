# s286 — glibc's CPU-count read on a closed descriptor 1: the arena limit, settled (wolf-lang#1051)

A native program started with descriptor 1 closed could see a descriptor of the C library's on 1.
On linux-gnu, glibc gives each new thread a malloc arena, and the first time a thread needs one
past glibc's test bound (8 on 64-bit) it reads the CPU count to set its arena limit. That read,
`get_nprocs()`, opens `/sys/devices/system/cpu/online` on the lowest free descriptor. A pool
worker's first allocation (Rust std's `Thread` handle) crossed the bound in any program given 8 or
more CPUs, so for a few microseconds, while main ran, descriptor 1 was that file and `fs_fstat(1)`
answered ok. #781's race row failed on such hosts and passed on smaller ones, CI's among them. The
runtime's own descriptors were already kept off 0–2 (`stdfd`'s `off_std`); this one was the C
library's.

The runtime now settles glibc's arena limit itself (`crates/wolf_rt/src/arena.rs`), in two steps.
`__wolf_rt_main_start`, the entry shim's first call, sets `M_ARENA_MAX` to glibc's test bound
before `main` and before any thread exists, so glibc never runs its test. The pool's one-time
initialization, on the one thread then running the program and before its first worker, raises it
to the limit glibc documents for itself, 8 per CPU online, from the same `get_nprocs()`. A limit
the user set is left alone. Two new rows: which thread reads the CPU count, under `strace -f`, red
at 0.2.38 on every linux-gnu host; and the arena count against glibc's own on the same host.

## Forbidden

Footprint: cross-cutting

The diff touches the runtime (`crates/wolf_rt/src/`), which the lane-audit map names
cross-cutting. The rest is two test targets and their fixtures, the CHANGELOG and notes. The entry
shim's emitted code is unchanged: the call is inside `__wolf_rt_main_start`, which the shim
already calls first. No CI, pin, spec, codegen or toolchain change. Clean room: glibc's documented
interfaces only (`mallopt(3)`, `get_nprocs(3)`, `malloc_stats(3)`, the glibc manual's Tunables
chapter and Memory Allocation Tunables page at 2.35, 2.39, 2.40, 2.41, 2.42, 2.43 and the
development manual) and black-box runs (`strace`, `malloc_stats`, and a probe program written for
this lane); no libc source was read. No label, merge, tag, release, issue filed or closed, and no
attribution trailer. The plant is pushed without a PR. Nothing ran under `/tmp` or
`/var/folders`. No commit was made on a Linux host.

Three behaviours change, decided for this lane (below, Decisions 1 to 3): a C library's threads
started before any task share 8 arenas until the pool starts; a test bound set alone
(`MALLOC_ARENA_TEST`, `glibc.malloc.arena_test`) no longer moves glibc's CPU read, because the read
no longer happens; and a set-user-ID program has its limit settled, since glibc ignores the
user's variables there.

## Inputs, verified

- Base: wolf-lang trunk `d848fbed`. Every build ran in a worktree pinned to a commit.
- Toolchains: Rust 1.97.1 everywhere. On the Linux hosts, the toolchain image used by the other
  lanes' gauntlets (Ubuntu's glibc 2.39, `ldd (Ubuntu GLIBC 2.39-0ubuntu8.9) 2.39`, with `strace`
  and clang). On the first Mac, rustup's pinned 1.97.1. On the second Mac, a lane-local rustup
  1.97.1 (`rustup-init` sha256 `63752a19…0f9f`, equal to the published digest).
- lupin 0.1.60, the `PAIRING` stamp: `aarch64-apple-darwin` (sha256 `f3cae16f…6bb8`) and
  `x86_64-unknown-linux-gnu` (`e7d4c4f1…637e`), each read twice and equal to the release page.
- wolf-std at the `STD-PIN`, `d4a4d4ca`.
- wolf-lang#1051 whole, including the comment that measured the cause with `strace -k` and the
  tunables table; #781 and its row.
- The documentation. `mallopt(3)`: `M_ARENA_TEST` is the arena count at which "the system
  configuration will be examined to determine a hard limit", 8 where `long` is 8 bytes and 2 where
  it is 4, and "the value of `M_ARENA_TEST` is not used when `M_ARENA_MAX` has a nonzero value";
  `MALLOC_ARENA_MAX` and `MALLOC_ARENA_TEST` "are ignored in set-user-ID and set-group-ID
  programs". The glibc manual, 2.35 and 2.39 to 2.43: `glibc.malloc.arena_max` defaults to 0,
  "meaning that the limit on the number of arenas is determined by the number of CPU cores online.
  For 32-bit systems the limit is twice the number of cores online and on 64-bit systems, it is 8
  times the number of cores online." The development manual words it as "the number of CPU cores
  online reported by get_nprocs()" with no multiple, which is why a row compares the settled limit
  with glibc's own on the host, rather than assuming the formula. `get_nprocs(3)`: the processors
  online; it opens and parses files in `/sys` on every call.
- The runtime: `__wolf_rt_main_start` (s267) is the entry shim's first call on both tiers, before
  `main` and before any thread. The threads it can make are the signal hub's drain thread and the
  reactor's thread (both before `main`, both after the entry call), and the pool's workers, from
  the first spawn. Before the pool exists, only the thread that starts it runs program code. The
  pool's initialization already reads the affinity and cgroup files through Rust std's
  `available_parallelism`, at the same point.
- README read, current: its glibc floor (2.35) is unchanged (`M_ARENA_MAX` dates from glibc 2.15,
  and the 2.35 manual documents the same limit), and it states nothing about descriptors or
  arenas.

## Prediction

The prediction (`notes/s286-prediction.md`, `f6b11785`) was committed before any change.

### Decisions, as taken

- **Decision 1, where the limit is set: two steps.** At entry, `M_ARENA_MAX` is set to the test bound,
  8 (2 on a 32-bit target), with no system call. At the pool's first start it is set to
  `8 * get_nprocs()`, clamped to at least 8. Not taken: one step at entry, `8 * get_nprocs()` for
  every program, which costs every linux-gnu program `get_nprocs()`'s calls at start and turns the
  start-up gate red (`bench/startup.json`: at most 12 calls over C's `hello`). Not taken: the pool's
  step alone, which leaves glibc's test reachable from a C library's threads before the pool.
- **Decision 2, a test bound is not a limit.** `MALLOC_ARENA_TEST` and `glibc.malloc.arena_test` do not
  skip the settle: they only move the moment glibc reads the CPU count. A test bound set above 8 per
  CPU once let glibc create that many arenas before it tested; a user who wants more arenas sets
  the limit, which is honoured.
- **Decision 3, a set-user-ID program settles.** With `AT_SECURE` set, the variables are not the user's
  setting, because glibc ignores them. Measured below: glibc 2.39 ignores the tunable there too.
- **Decision 4, a deterministic red row.** The CPU-count row, under `strace -f`, red on any
  linux-gnu host, CI's included, beside #781's race row, which is red only on 8 or more CPUs.

### Predicted, then measured

Hosts: H1 is a 20-CPU x86-64 Linux host (kernel 7.2.4), H2 a 16-thread x86-64 Linux laptop
(kernel 7.2.14, the host where #781's race row failed every run). Every Linux row ran in the
toolchain container (Ubuntu glibc 2.39), the workload as an unprivileged uid under `taskset`.

- **glibc's limit is 8 per CPU online, not per CPU of the affinity: as predicted.** An arena
  probe (`rig/linux-h2/arena-probe.c`, written for this lane) holds `8 * get_nprocs() + 8`
  threads that have each allocated, and counts `malloc_stats(3)`'s arenas. On H1, with 20 CPUs
  online and an affinity of 2: 160 arenas. On H2, with 16 online and an affinity of 8 or of 4: 128
  (`probe-H1.log`, `H2-red-c09042b8.log`). The prediction said 160, not 32, on 4 of 20; the
  measurement is the same claim on 2 of 20.
- **The settled limit is glibc's own: as predicted.** `arena_limit` at the fix on H2: control 128
  (`8 * get_nprocs` = 128), settled 128, the entry step alone 8 (`H2-green-8fd5a8ef.log`). At the
  base the entry-only child reached 128, so the row is red there (`H2-red-c09042b8.log`).
- **`get_nprocs()` is three system calls: as predicted** (`openat`, one `read` of `"0-15\n"`,
  `close`; the prediction allowed a fourth). At the fix they are main's, at the pool's start,
  before the cgroup reads of the same initialization and before any clone
  (`H2-green-8fd5a8ef.log`, `fix-calls`).
- **The user's limit stands: as predicted.** With `MALLOC_ARENA_MAX=2`, with
  `GLIBC_TUNABLES=glibc.malloc.arena_max=2`, and with that item after
  `glibc.malloc.trim_threshold=131072`, the settled child holds 2 arenas (`arena_limit`, H2).
  Under the CPU-count row's tracer, the same settings make nothing read the CPU count, and
  `MALLOC_ARENA_MAX=0`, the user asking for glibc's own policy, makes main read nothing: the
  row's legs, green at the head in the Linux gauntlet's test step.
- **A set-user-ID program: glibc ignores both settings, as predicted.** The probe's set-user-ID
  copy, run by another user (`AT_SECURE` 1, euid 0), holds 160 arenas on H1 and 128 on H2 with no
  setting, with `MALLOC_ARENA_MAX=2` and with the tunable at 2. The plain copy holds 2 with
  either. So the settle runs in such a program, where the user's variables have no effect.
- **#781's race row is red at the base: at the rate #1051 measured.** Unpinned, descriptor 1
  closed, on H2's 8 CPUs: 10 of 10 runs red, `ok` 6 to 14 in a run (the native sub-run fails
  first; `H2-red-c09042b8.log`). The prediction said at least 9 of 10 and `ok` 7 to 27; 6 is one
  under that range. **Not as predicted on 12 and 16 CPUs:** H2's whole package, the
  base's fixture run directly with descriptor 1 closed, 0 of 10 runs red per tier at each count,
  and the row itself green on 16 (`H2-full-c09042b8-8fd5a8ef.log`). The prediction said red at
  every count of 8 or more. A reading consistent with #1051's own account ("given 8 CPUs, the
  program crosses the bound as its workers start, just as main's loop begins"), not measured
  further here: with 12 or 16 workers the arena past glibc's bound, and with it glibc's one CPU
  read, comes while main is still starting them, before its `fstat(1)` loop begins. Either way the
  race row is red only on some CPU counts, which is why the CPU-count row exists: it is red at the
  base on any host.
- **#781's race row is green at the fix: as predicted.** On H2's 8 CPUs, the row 3 of 3, and the
  row's fixture built by the head's own compiler and run with descriptor 1 closed, the row's own
  check on each run: native 0 red of 100, release 0 red of 100 (`H2-green-8fd5a8ef.log`).
  On 12 and 16 CPUs, native and release each 0 red of 100 at each count, and the row 3 of 3
  (`H2-full-c09042b8-8fd5a8ef.log`); those two counts do not tell the fix from the base, which
  was green there too.
- **The CPU-count row is red at the base on any host and green at the fix.** At the base on H2 (`H2-base-sleepers.trace`),
  the trace of `sleepers1051.lu` on native shows four of the pool's extra workers each opening
  `/sys/devices/system/cpu/online` after the clones; main's own opens there are the cgroup files of
  the pool's start. At the fix (`H2-fix-sleepers.trace`), main opens it once, before those cgroup reads
  and before its first clone, and no other thread opens it. As a row: red by name at the base's
  runtime in the plant record (the final head with the settle disabled), and green at the head in
  the Linux gauntlet's test step, both quoted in the PR.
- **The cost, allocation throughput: a tie on every gated row; one report-only row and one pool
  row moved, both measured again.** On H1, one block of six CPUs with nothing beside it, the base's
  runtime (`c09042b8`) and the settle's (`8fd5a8ef`) in six rounds, ABBAAB (`perf-H1.log`,
  `perf-H1-ratios.txt`):
  - t1's family B, every run of every round, ns per op, after/before, against `bench/protocol.md`'s
    margin (3×MAD and the 2% floor): `list_alloc` wolf 1.0000, PGO 1.0000, naive C 1.0001, expert C
    1.0011, Rust 1.0008; `b3_churn` wolf 1.0000, naive C 0.9991, Rust 0.9959. All ties, and `bench
    diff` says "unchanged" for each pair.
  - **Outside its margin: `b3_churn`'s PGO lane, 1.0357** (35.443 → 36.709 ns per op), which
    `bench diff` reports as report-only. It reads two values only, 28 ms and 29 ms over the
    kernel's 790,000 operations: one tick of the kernel's 1 ms clock, which on H1 is 3.4% of the
    reading, above the 2% floor. Read again at ten times the operations, where a tick is 0.35%, 20
    interleaved rounds: 1.0068 by the kernel's clock and 1.0086 by user CPU time, a tie
    (`perf-H1-pgo.log`). The PGO profile was the same file at both heads (sha256 `3513b269…`).
  - The pool's benches, release, 9 runs a head: `chan_bench` rendezvous 1.002, bounded 1.024, stream
    1.050, select 1.000; `spawn_bench` spawn 0.978, wake and join 0.973, per-task join 0.917; ties.
    **Outside its margin: `worker_spawn_median`, 0.879, faster.** Read again with 30 runs a head:
    0.923 against a margin of 0.064, faster again, and every other pool row a tie (0.961 to 1.022).
    That gain is not explained here; a read of the base's runtime with the user's own
    `MALLOC_ARENA_MAX` set to the same limit, to see whether glibc alone gives it, is reported in
    the PR.
- **Start-up: unchanged.** The entry step makes no system call: `hello.lu`, which starts no pool, opens no
  CPU file under the CPU-count row's tracer, and the start-up gate (`cargo xtask bench-gates`: the
  runtime's own start and exit within 12 calls over C's `hello`) passes at the head in the Linux
  gauntlet record. The pool's first start pays `get_nprocs()`'s three calls once.


## Evidence index

`notes/s286-evidence/index.txt` lists every file. Paths in the logs are written `$LANE` (this
lane's directory on each host), `$LANES` and `$HOME`; the Linux hosts are H1 and H2, the Macs the
first and the second.

- `H2-red-c09042b8.log`: H2's 8 CPUs at the rows' commit (no settle): the probe (plain and
  set-user-ID, each setting, two affinities), `get_nprocs()`'s calls, `arena_limit` red, the
  CPU-count row red for a parser fault fixed in `5e333326` (below), #781's row 10 of 10 red.
- `H2-green-8fd5a8ef.log`: H2's 8 CPUs at the settle: `arena_limit` green, the settle's own calls,
  #781's row 3 of 3 green, 100 direct runs per tier green.
- `H2-full-c09042b8-8fd5a8ef.log`: H2's whole package: the base's direct runs on 12 and 16
  CPUs (0 of 10 red per tier), the fix's (0 of 100 per tier), the row at the base (green on 16)
  and at the fix (3 of 3).
- `H2-base-sleepers.trace`, `H2-fix-sleepers.trace`: `strace -f -e trace=openat,clone,clone3` of
  `sleepers1051.lu` on native, H2's 8 CPUs, at the base and at the fix.
- `H2-sensors-*.log`: H2's package temperature beside each take.
- `probe-H1.log`: the probe on H1 (20 online, an affinity of 2). `probe-H1-void.log` is its first
  run, void: the compiler removed each thread's malloc/free pair, so no thread allocated
  (1 arena everywhere); the probe now keeps each block in a global slot.
- `perf-H1.log`, `perf-H1-ratios.txt`: the cost take (t1's family B and the pool's benches,
  ABBAAB); `perf-H1-pgo.log`, `perf-H1-pgo-ratios.txt`: the second read (the PGO lane at ten times
  the operations, the pool's benches at 30 runs a head).
- `mac1-term-alone.log`, `mac1-term-suite.log`, `mac2-term-suite.log`, `mac1-early-e2928947.log`:
  the two terminal rows on both Macs at the base and the head (below).
- `rig/`: the scripts as run, each host's in its own directory.

**Two terminal rows on the first Mac.** `term::tests::the_three_raw_bits` and
`term::tests::the_pty_helper_answers_its_own_pair` failed at `posix_openpt` in this lane's first
Mac run of the runtime's suite. Each passes when run alone, at the base `d848fbed` and at the head
(`mac1-term-alone.log`). Run with the whole lib suite, both fail at the base exactly as at the
head, the same two tests with eight `posix_openpt` panics each (`mac1-term-suite.log`: base 372
passed and 2 failed, head 374 and 2). On the second Mac the whole suite passes at both (374 of 374
at the base, 376 of 376 at the head, `mac2-term-suite.log`). So it is the first Mac's
environment, not the change, and the macOS record was taken on the second Mac.

**One row fixed after its red.** The CPU-count row as first committed (`2ac288e0`) split each
`strace -f` line at its first space, but `strace` writes the thread's id and then two spaces, so
the row saw no `openat` and no clone and failed at the base for that reason
(`H2-red-c09042b8.log`), and would have failed at the fix the same way. `5e333326` trims the
separator. Its red at the base's code is shown on `s286-red-rows` (`f751d5b7`: the rows' commit
plus that one change, pushed without a PR, its take reported in the PR) and in the plant record.

The records at the final head live outside the tree, because a record cannot predate the head it
names. The PR body quotes every `END` line of the Linux gauntlet record, of the macOS record and of
the plant record.


## Done-when

Commits:
- `f6b11785`: the prediction, before the first change.
- `2ac288e0`, `c09042b8`: the two rows, red first (the CPU-count row and its fixture
  `sleepers1051.lu`; `arena_limit`).
- `e48c50ab`: the settle's module (`crates/wolf_rt/src/arena.rs`, its skip and the skip's unit
  rows) and the entry step in `__wolf_rt_main_start`.
- `8fd5a8ef`: the pool's step, in the pool's one-time initialization.
- `e2928947`: #781's row and fixture name the C library's read.
- `d018c1e2`: the changelog.
- `a2cd6a23`: the settle's documentation states the set-user-ID measurement.
- `5e333326`: the CPU-count row reads `strace`'s spaced thread ids.
- This note and its evidence.

Off the line: `s286-red-rows` (`f751d5b7`, the rows' commit plus `5e333326`'s change, the
corrected row's red at the base's code) and the plant `s286-plant-no-settle` (the final head with
one hunk that leaves glibc's limit alone, so neither `mallopt` call is made), each pushed without a
PR.

Test targets:
- [x] the prediction committed before the first change (`f6b11785`)
- [x] the rows red at the base's code: `arena_limit` (the entry step alone reached 128 arenas, not
      8), the CPU-count row (four workers read the count), #781's race row on 8 CPUs (10 of 10)
- [x] the rows green at the settle: `arena_limit`, the CPU-count row (main alone reads the count,
      before its first clone), #781's race row on 8 CPUs (3 of 3, and 100 direct runs per tier),
      12 and 16 CPUs (100 direct runs per tier, and the row)
- [x] the skip: `MALLOC_ARENA_MAX=2`, the tunable at 2, and the tunable after another item each hold
      2 arenas; a test bound alone is settled; the user's 0 leaves the read to glibc
- [x] glibc's limit measured against the formula: 160 on 20 CPUs online, 128 on 16, whatever the
      affinity; the set-user-ID measurement on both Linux hosts
- [x] the cost: t1's family B and the pool's benches before and after, interleaved, with ratios and
      the protocol's margins; the two rows outside a margin read again
- [x] the two terminal rows behave the same at the base and the head on both Macs
- [x] fmt; clippy `-D warnings` on the runtime for linux-gnu and macOS, and on the signal gaps
      test for linux-gnu
- [ ] the Linux gauntlet record at the head (quoted in the PR)
- [ ] the macOS record at the head, on the second Mac (quoted in the PR)
- [ ] the plant red by name at #781's race row on 8 CPUs, at the CPU-count row and at
      `arena_limit` (quoted in the PR)
- [ ] the PR-tier CI run at the head, and the audit, once

Decisions: four, above, each taken as recommended. Issues: none filed. #1051's text gains the
resolution and names the open descriptor question, for its close by hand.
