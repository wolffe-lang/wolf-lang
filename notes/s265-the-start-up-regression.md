# s265 — the start-up regression (wolf-lang#918, yew#55)

Lane note. **Opus 5.5. Crawl 54.** Branch `s265` off wolf-lang trunk
`7b7b1753`, rebased onto `6480a27b` (s264's merge) before the PR; the
prediction is its first commit (`0186ddb8`). Contract: `sprints/compiler/s265-the-start-up-regression.md`
(planning repo). Measurements, witness builds and bisection builds on
kasumi CPUs 4-7 inside the lane's own rootless container; editing on
the Mac only.

## Prediction (written before the first change, 2026-10-24)

Read before predicting: wolf-lang#918, yew#55, yew `notes/yw07-evidence/
perf-attribution/` at `730115b` (the three-trial cells and the same-bits
path comparison), the 0.2.35 CHANGELOG section (s250-s259), and
`git diff v0.2.34 v0.2.35 -- crates/wolf_rt`.

- **The suspect commit: `75279748`** (s256, "Report native guard faults
  on every runtime thread"). It puts `__wolf_rt_stack_start` in every
  program's entry shim (`add_entry_shim`, unconditional `(true, ...)`),
  so every 0.2.35 program does, before user `main`:
  `pthread_getattr_np` on the main thread (glibc finds the main stack by
  reading `/proc/self/maps` through stdio: an `openat`, `read`s of the
  whole map text, a `close`, a `getrlimit`), a `sigaltstack` query, a
  32 KiB `mmap` and a `sigaltstack` install, two `sigaction` reads and
  two installs (`SIGSEGV`, `SIGBUS`), and the first touch of two
  thread-locals (one with a destructor, so a TLS-dtor registration). It
  also keeps the stack/handler code live past `--gc-sections`, which the
  0.2.34 comment says a non-spawning binary never paid for.
- **The mechanism:** runtime set-up before `main` (syscalls and one
  procfs read), not static initialisation and not page-in. I predict
  ~12 more syscalls on hello world and a binary a few KiB larger, the
  page-fault count up by a handful (the 32 KiB alternate stack is
  mapped, not touched).
- **The size:** tens of microseconds a process, not milliseconds:
  20-80 us on a release-tier hello world whose cold start is a few
  hundred microseconds. A program with a bigger map (yew: more mappings,
  a larger `/proc/self/maps` text) pays a little more, still well under
  0.2 ms.
- **yew's 3.8 ms:** I predict s256 explains at most ~0.1 ms of it. The
  attribution's cells are three trials each and its own same-bits
  control (one binary, two paths) ran 5.1-11.8 ms; I predict yew's
  50-start medians on the two toolchains differ by well under the
  reported 3.8 ms, and that any remainder over s256's share is on
  yew's start-up path through the bundled std (sc69's terminal-mode
  writes or sc70's time code), not in a compiler commit. If that is
  so, item 2's bisection names `75279748` for the per-process cost and
  the std delta is listed as an issue, not fixed here.
- **ms recovered:** 0.02-0.08 ms a process, by not reading
  `/proc/self/maps` at start (the main thread's bounds from the
  stack pointer, `RLIMIT_STACK` and the aux vector, or read on first
  need) while keeping the reporter's behaviour.
- **The gate:** kasumi's wall-time spread on a sub-millisecond start
  with other lanes on 0-3, 8-11 and 12-15 will be above 10 % of the
  delta, so a wall-time gate at the 0.2.34 number + 10 % will not be
  reliable; I predict a recorded-only wall row and a deterministic gate
  (the syscalls a release-tier hello world makes before `main`, counted
  by `strace`) that can go red on a planted eager init.

## Forbidden (what this lane did not do)

RULES §1-§10 held: no merge, tag, release or public step; no issue filed
or closed (listed in §7); no label on a PR; no `git config` on the Mac;
no build, witness or timing on the Mac (edits, `cargo check`, `cargo
fmt` and `cargo clippy` with the pinned 1.97.1, each behind the 8 GB
`vm_stat` floor); on kasumi nothing outside `~/lanes/s265` was written,
every job ran in the lane's own rootless container on CPUs 4-7 under the
lane lock with `ulimit -c 0`; no host package installed; no other lane's
file read beyond s264's taskset shim and validation script (the reference
the brief names); no kernel, libc or other implementation's source read —
the main stack's bounds were matched to `pthread_getattr_np(3)` black-box
(`tools/stackprobe.c`, the equivalence test), and sagitta (the
maintainer's own C yew, yew's oracle) was read and linked for its PTY
observer only. No attribution trailers.

## Inputs verified

- wolf-lang trunk `7b7b1753` (the base the lane measured from) and
  `6480a27b` (the base it was rebased onto: s264's six commits touch
  `wolf_backend`'s read lends, the two emitters and the corpus — no
  runtime and no entry shim).
- The release archives, digests from the release API read twice and the
  downloads hashed twice on kasumi (`archive-digests.txt`):
  - 0.2.34 `9da7bdcf`, `wolf-0.2.34-x86_64-unknown-linux-gnu.tar.gz`
    `88a9dd3da1c9a0e38c08eb6e2de9c1ef04d1ed93dcf51024d8dbbb76890c1821`
    (bundled std `e036bec`);
  - 0.2.35 `cf0c13d3`, `wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz`
    `4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a`
    (bundled std `2386522`);
  - lupin 0.1.56 (PAIRING),
    `ad703550ac4787e8e4bb26989d591a731410274d7332f2b4e8b9a64c878822cc`.
- The container: `ubuntu@sha256:534baea6a22c03a63003dbc8dbe78fe34bc0d7e595d9a9dc9834884ff530eb55`
  with clang 18.1.3, lld, strace, perf 6.8.12, gdb and qemu
  (`image/Containerfile`, image `a03d9a48`); the taskset shim (0-3 → 4-7,
  0 → 4; static C; `execv` of `taskset.real`) and a C `sigclean` (empty
  mask, four defaults) in place of s264's Python launcher; `--pids-limit
  16384 --ulimit nproc=32768:32768`; pid 1 is podman's init as container
  root, the command dropped to the lane uid by `setpriv` (`kill -0 1` is
  EPERM, checked); SigBlk and SigIgn zero (`image/ctr`).
- yew `280012fe` (cell a, its own pin 0.2.34) and `280012fe` + yw07's
  `compile-compat.patch`, read through `git show 730115b:` (cell b, 0.2.35;
  `git diff 280012fe 7a9ae6e -- src tools runtime` is empty, so this is the
  attribution's b-prime source); sagitta `9ace98c2` (`sagitta.pin`).
- kasumi: i9-11900K, 8 cores / 16 threads; CPUs 4-7 share their cores
  with 12-15, so every table names the load average and the busy share of
  4-7 and 12-15 (`tools/hostload.sh`, in each log); governor `powersave`;
  linux 7.2.8.

## 1. Reproduced without yew: hello world moves

`corpus/hello.lu`, the smallest program there is, moves. Built on the
release tier by each published archive in the container (clang 18), 50
cold starts each, interleaved (A B, B A, ...) after five unrecorded
warm-ups, fork to reaped exit (`tools/coldstart.c`); host load 1.62,
CPUs 4-7 and 12-15 at 0-2 % busy (`item1/hello/load-*.txt`):

| hello | median | p10 | p90 | (p90-p10)/median |
|---|---:|---:|---:|---:|
| 0.2.34 | 238.7 us | 231.3 | 249.3 | 7.5 % |
| 0.2.35 | 277.6 us | 269.3 | 296.5 | 9.8 % |
| **delta** | **+38.9 us (+16.3 %)** | | | |
| page cache evicted before every start (`-e`, posix_fadvise DONTNEED) | 1,802.9 → 1,927.5 us (+124.6, +6.9 %) | | | 27-29 % |

What changed (`item1/hello/`): seventeen more system calls (42 → 59,
`strace -f`), 86,248 more user-space instructions (173,380 → 259,628,
+50 %, `perf stat -r 50`), six more page faults (63 → 69), 7,322 bytes
more text and 696 more bss. The seventeen, all from one place: before
`main`, `openat("/proc/self/maps")`, `prlimit64(RLIMIT_STACK)`, `fstat`,
three 1 KiB `read`s, `close`, `sched_getaffinity` (that is
`pthread_getattr_np(3)` on the main thread), `sigaltstack` (query), a
32 KiB `mmap`, `sigaltstack` (install), four `rt_sigaction` (two reads,
two installs, SIGSEGV and SIGBUS); after `main`'s last write, a
`sigaltstack(SS_DISABLE)` and the `munmap` (a thread-local destructor on
the main thread). That is s256's `__wolf_rt_stack_start`, which the entry
shim calls unconditionally (`add_entry_shim`), so every 0.2.35 program
pays it.

## 2. The bisection: `75279748`

The compiler and runtime built at every landing commit of 0.2.35 (the
dist build's crates and stamp, one target directory, 5-55 s a build;
`tools/bisect.sh`, `bisect-commits.txt`), the hello witness rebuilt with
each, then all twenty-one binaries timed together, interleaved, three
rounds of 50 (host load 3.0-4.0, CPUs 4-7 and 12-15 at 0-2 %;
`item2/`). The `tl30` landing (`a10ff32b`) is CI files only and was not
built.

| build | lane | syscalls | page faults | user instructions | hello median, 3 rounds of 50 (us) |
|---|---|---:|---:|---:|---|
| v0.2.34 | 0.2.34 archive | 42 | 63 | 173,389 | 233.9 / 231.6 / 228.1 |
| `9da7bdcf` | v0.2.34 | 42 | 63 | 173,388 | 230.5 / 229.0 / 228.8 |
| `50e8d0f8` | s252 | 42 | 63 | 173,388 | 227.7 / 229.4 / 230.4 |
| `4732696d` | s250 | 42 | 63 | 173,388 | 229.2 / 233.4 / 228.2 |
| `0cb954a8` | s254 | 42 | 63 | 173,389 | 233.8 / 232.9 / 230.2 |
| `b308dafc` | s253 | 42 | 64 | 173,394 | 232.3 / 231.4 / 228.9 |
| `0477ce62` | s251 | 42 | 64 | 173,394 | 232.0 / 232.0 / 232.1 |
| `a49ddaa2` | s256 before the handler | 42 | 64 | 173,394 | 233.6 / 232.8 / 231.5 |
| `75279748` | **s256: the handler** | 59 | 69 | 259,543 | 267.1 / 267.6 / 269.7 |
| `94cd52bc` | s256 landed | 59 | 69 | 259,501 | 269.4 / 270.5 / 267.3 |
| `76a3c8b6` | s255 | 59 | 69 | 259,642 | 269.4 / 270.7 / 267.1 |
| `79550724` | s258 | 59 | 69 | 259,591 | 265.2 / 267.1 / 265.3 |
| `06cff722` | s257 | 59 | 69 | 259,556 | 267.6 / 264.9 / 267.2 |
| `e2ed1607` | s259 | 59 | 69 | 259,605 | 268.7 / 266.7 / 265.7 |
| `7ac8c981` | the musl patch | 59 | 69 | 259,624 | 270.3 / 270.7 / 268.2 |
| `cf0c13d3` | v0.2.35 | 59 | 69 | 259,601 | 272.4 / 269.0 / 268.4 |
| v0.2.35 | 0.2.35 archive | 59 | 69 | 259,625 | 269.4 / 268.6 / 266.5 |
| `7b7b1753` | trunk at s265 base | 59 | 69 | 259,557 | 267.8 / 269.2 / 266.8 |
| head | s265 (lazy guard) | 47 | 64 | 183,497 | 235.1 / 239.1 / 235.5 |
| V1 | no registration (floor) | 42 | 63 | 173,371 | 228.7 / 230.9 / 230.9 |
| c | C twin | 35 | 53 | 112,494 | 202.6 / 201.1 / 197.7 |

The step is one commit, **`75279748`** (s256, "Report native guard faults
on every runtime thread"): +17 calls, +86,149 instructions, +5 page
faults, +35.5 us (232.6 → 268.1 us, the three-round means). Nothing
before it moves the start (every hello through `a49ddaa2` is within
2 us of 0.2.34's); nothing after it moves it again, through 0.2.35 and
trunk. The prediction named this commit.

**yew** (the brief's question, `yew/`). The same three cells as
yw07's attribution, built on the release tier in the container (12:53,
14:06 and 14:54 a build), timed with sagitta's own PTY observer
(`tools/yewpaint.c` links `tests/support/live_pty.c` at `9ace98c2`;
first paint is `yew_live_pty_wait_frame`'s frame 0), three rounds of 50
interleaved, host load 3.0-3.8:

| row (sagitta 9ace98c2 observer) | yew a: 280012fe on 0.2.34 | yew b: + compat on 0.2.35 | yew fix: + compat on the s265 head | b - a | fix - b |
|---|---|---|---|---:|---:|
| startup.first_paint.clean (--clean empty.c) | 2.506 / 2.485 / 2.529 ms | 2.521 / 2.569 / 2.558 | 2.527 / 2.546 / 2.561 | +43 us | -5 us |
| cold open to first paint (10,000-line fixture) | 24.669 / 24.663 / 24.536 ms | 24.740 / 24.765 / 24.637 | 24.524 / 24.752 / 24.609 | +91 us | -86 us |

yew's start moves by **+43 us** on the clean first paint between 0.2.34
and 0.2.35 — about hello's 37 us — not 3.8 ms. yw07's 3.8 ms is the noise of a three-trial median: its own
same-bits control (one binary, two paths) ranged 5.1-11.8 ms, and its
closeout's port clean first paint read 2.57, 7.33 and 14.10 ms in three
trials; here 150 starts a cell sit within ±0.1 ms. The fix's 31 us is
below what yew's rows resolve (one round's median moves ±0.04 ms
against the next): fix - b is -5 us on the clean row and -86 us on the
cold one, both inside that. The cold row as this harness takes it (no
`--clean`, a 10,000-line `x` file) reads about 24.6 ms on every cell, so
it is not yew's `cold_open_to_first_paint` (yw07: 4.7-11.5 ms); only
the differences between cells are comparable.

## 3. The cause, the fix, and R-s265-1

**What each piece costs.** A C program doing one piece each
(`tools/piece.c`, 50 starts, host load 7: read the differences, not the
absolutes): `pthread_getattr_np` on the main thread +31.2 us and +63,188
instructions; a raw read of `/proc/self/maps` +11.7 us; the altstack and
the four `sigaction`s together under 1 us. And runtime variants linked
under one driver (the driver links the `libwolf_rt.a` beside it;
`tools/variants.sh`, three rounds of 50, host load 1.1-1.7, `item3/`):

| hello, release tier | calls | page faults | instructions | medians, three rounds of 50 |
|---|---:|---:|---:|---|
| 0.2.34 (published) | 42 | 63 | 173,381 | 239.3 / 241.3 / 244.7 us |
| 0.2.35 (published) | 59 | 69 | 259,616 | 279.6 / 282.7 / 276.5 |
| V1: no registration at all | 42 | 63 | 173,363 | 237.7 / 237.2 / 237.6 |
| V3: the handler only | 44 | 64 | 180,988 | 241.8 / 239.6 / 238.5 |
| V2: the handler and the altstack, no bounds query | 47 | 63 | 182,038 | 240.6 / 239.2 / 241.8 |
| the eager direct read (the runtime at `9f689e7d`) | 51 | 67 | 242,282 | 263.2 / 265.4 / 266.9 |
| C (`bench/startup/hello.c`) | 35 | 53 | 112,486 | 209.6 / 207.2 / 205.2 |

The reporter itself (the handler and its altstack) costs about 2.5 us;
the main thread's stack-bounds query costs the rest, and the bounds are
read only when a fault arrives. The hellos are linked BIND_NOW and the
new code adds 19 bound symbols (`item3/bindings.txt`); that is part of
every row from V3 down and is not separable here.

**The incidental part** (no behaviour change; the bounds are held equal
to `pthread_getattr_np`'s):

- `ce3b45eb` splits the registration into its bounds and altstack halves;
- `b94264d4` registers the main thread through `enter_main_thread`: the
  bounds read from `/proc/self/maps` (proc(5)) and `RLIMIT_STACK`
  directly — the mapping that holds the frame, grown down to the limit,
  never into the mapping beneath it, the guard the page below — instead
  of `pthread_getattr_np`'s stdio, scanf, `fstat` and CPU-set query; the
  main thread's altstack kept for the process's life (its end is the
  process's), so no thread-exit destructor and no `sigaltstack` +
  `munmap` at exit;
- `f540597e` holds the direct bounds to `pthread_getattr_np`'s
  (`crates/wolf_rt/tests/main_stack_bounds.rs`, on the main thread, no
  libtest): this process, then children under a 4 MiB, an 8191 KiB (not
  a page multiple), a 64 MiB and no stack limit, and under a 1 MiB
  environment — equal in all six (`item3/equivalence-eager.log`,
  `item3/short2.log`, and again in phase 2b's log); the black-box probe
  behind the formula is `item3/stackprobe.log`;
- `2e12015e` reads the replaced fault disposition in the installing
  `sigaction` (one call a signal, not two; the `SIG_IGN` rule unchanged);
- `9f689e7d` reads the map in one call; `9a762c4a` rustfmt; `afc2f008`
  names the guard range for clippy.

That alone leaves 51 calls and +25 us (+10 %): the eager query is still
a procfs read and a parse that the start does not need.

**R-s265-1 = B** (asked with these numbers; ruled B by the orchestrator,
2026-10-24; implemented `a50b724d`; `5627fdc1` makes the s256 handler
comment say the rule changed and why). What the cost buys:
`[conf.trap.stack]` — a main-thread stack overflow reported as
`wolf-trap: stack-overflow` with its PC and exit 134, not 0.2.34's bare
signal death (exit 139). The release intended it; the cost is the query,
not the safety.

- (A) Keep the eager query and document it: +38.9 us a start on 0.2.35
  as shipped (+16 % on hello); +25 us (+10 %) with the incidental fix
  alone.
- **(B) Make the bounds lazy — paid at first use.** The entry installs
  the altstack and the handler and records an address on the main stack
  and the page size; the handler resolves the main thread's guard on its
  first fault with the same direct read (`open`, `read`, `close`: on
  signal-safety(7)'s list; a 4 KiB buffer on the 32 KiB altstack;
  `getrlimit`). Start: 47 calls, about +5 us (+2 %) over 0.2.34 (the
  bisection's head row: 236.6 against 231.2 us); every overflow row
  unchanged: `corpus/faults/stack_overflow.lu` at depth 200,000 and
  300,000 exits 134 with `wolf-trap: stack-overflow` and its PC on native
  and release, as on 0.2.35 (`item3/short2.log`), and
  `tests/stack_overflow.rs` passes (phase 2b; `two_machines` passes too,
  but its fault rows need a nested container this one lacks, so CI's
  run is theirs). What it
  changes: the handler reads a file on the main thread's first fault,
  which the s256 comment had ruled out; the binaries are BIND_NOW, so no
  symbol lookup runs there; a map that cannot be read at that moment (no
  descriptor free) leaves no guard and the fault dies the ordinary way —
  the answer 0.2.35 gave when its query failed at entry.
- (C) A flag that skips the reporter: makes a spec'd report optional, and
  the program that needs it most is the one that would not set it.

Recommended and ruled: **B**. **Recovered: 31.3 us a start** (trunk
267.9 → head 236.6 us on hello, the bisection's three-round means),
0.031 ms of 0.2.35's 37.0 (0.2.34's archive 231.2, 0.2.35's 268.2); the
residual over 0.2.34 is about 5 us and 5 calls (the altstack and the
handler).

## 4. The gates

- **The start-up row** (`906ec9b9` the gate, `80516eab` and `642083eb`
  the record, `2b18fbe1` its row in `bench/protocol.md`): bench-gates'
  fourth gate builds `corpus/hello.lu` on the release tier with the
  gauntlet's own driver and `bench/startup/hello.c` with the host's
  clang, traces each once with `strace -qq`, and holds the difference in
  calls (wolf minus C: the runtime's own start-up and exit; the loader's
  calls cancel) at or below `bench/startup.json`: **12** (47 - 35 in the
  container; 0.2.34 was 7 over C, 0.2.35 24, the eager direct read 16).
  No `strace` is a loud skip, a failure under
  `WOLF_SYSCALL_REQUIRE_STRACE` (every CI job sets it).
- **The wall row is report-only.** The contract's gate (the 0.2.34
  number plus 10 %) needs a spread below 10 %; hello's 50-start spread on
  kasumi is 7.5-31 % ((p90-p10)/median, the rounds above), one round's
  median moves 1-3 % against the next, and CI's runners are not this
  host (the plant's run read 637 us for the wolf hello, 472 for C).
  bench-gates prints the medians and the ratio beside the count; D5 keeps
  them out of the merge gate.
- **The plant, red by run id: 40147037892** (`s265-plant-startup`,
  `c18ce4e8`, restoring the eager 0.2.35 registration —
  `__wolf_rt_stack_start` → `enter_runtime_thread`), dispatched
  `-f jobs=lint-test`: every gauntlet step green through `bench-gates`,
  which went red by name — "a release-tier hello world makes 26 more
  system calls than its C twin, recorded 12 (bench/startup.json)". The
  runner's raw counts are higher than the container's (wolf 93, C 67),
  and the plant's difference there is 26 against the 22 predicted here:
  the eager path reads `/proc/self/maps` in 1 KiB chunks and the
  runner's map is longer. The lazy head reads no map at start, so its
  count should be 12 on the runner too; the PR's `tier pr` run is that
  check. The first dispatch, 40146566459 at `58924a44`, went red at
  `fmt` on this branch's own unformatted code, not the gate; it is
  superseded. The plant branch is to be deleted after the merge.
- **The gauntlet.** The first run (phase 2b, at the pre-rebase head)
  went red in `test` at `crates/wolf_driver/tests/copy_write_lanes.rs:221`
  (`a_failed_copy_write_puts_the_source_back_on_every_lane`: "lupin 0.1.56
  (28547b4)'s answer on fs/copy_chunk_failed_write.lu: ("unsupported",
  ""), want the clause's or a pre-mirror pin [("0.1.55", "e642f26", …)]");
  `fmt`, `fmt-fixpoint`, `clippy` and `rt-none` were green before it
  (`gauntlet1/`). That phase's script ended 0 over the red; `bdb6fc89`
  makes it exit with its worst step. The attribution (the test at trunk
  `6480a27b` in the same container with the same lupin) and the whole
  gauntlet at the rebased head, every step past a red run alone, are
  `tools/gauntlet2.sh` — PENDING (§8).
- **The differ against 0.2.35** (`xtask differ <head's wolf> <0.2.35's
  wolf>`, the three tiers, `WOLF_STD` the std pin; `gauntlet1/differ-*.log`):
  checked 3, native 6, release 6 divergences of 1,221 files. Every moved
  row is trunk's, none s265's (no start-up output is compared):
  - `corpus/fs/copy_chunk_failed_write.lu` (all three tiers, exit 0 vs 1):
    s260's failed copy write (wolf-lang#642), not in 0.2.35;
  - `corpus/os/arg0.lu`, `corpus/os/args_bytes.lu` (all three tiers,
    exit 0 vs E0301): s261's `env_arg0`/`env_args_bytes` (wolf-lang#904),
    names 0.2.35 does not have;
  - `corpus/conc/atomic_ub_uaf.lu`, `corpus/memory/unsafe_ub_uaf.lu`,
    `corpus/memory/volatile_ub_uaf.lu` (native and release): the
    use-after-free witnesses, whose exit codes are garbage reads and differ
    from run to run (s257's note saw the same three).

## 5. What yew should expect

When yew re-pins on a wolf with this change, yew#55's two rows should
not move because of the toolchain: the toolchain's share of yew's start
was +0.04 ms at 0.2.35, and the fix returns about 0.03 ms of it. Rows
that read 6.90x and 2.07x at yw07's closeout are the host's noise on a
three- or five-trial median (the same binary read 2.57, 7.33 and
14.10 ms on the clean row there; 150 starts here hold it at 2.49-2.57
ms on every cell). yew's port reaches its clean first paint in about
2.5 ms on kasumi CPUs 4-7 when the host is quiet; yw07's C clean rows
read 1.96-2.88 ms. Expect the ratio near 1.2 under a measurement that
takes enough starts; a median of three will keep missing by chance.

## 6. Evidence index (`notes/s265-evidence/`)

- `archive-digests.txt` — both archives and lupin, hashed twice.
- `image/` — `Containerfile`, `ctr` (the launcher), `taskset-shim.c`,
  `sigclean.c`.
- `tools/` — `coldstart.c` (interleaved cold starts), `hostload.sh`,
  `summarize.py`, `piece.c`, `stackprobe.c`, `yewpaint.c`, `bisect.sh`,
  and the job scripts (`build-yew.sh`, `short1.sh`, `phase2a.sh`,
  `variants.sh`, `short2.sh`, `phase2b.sh`, `gauntlet2.sh`).
- `item1/` — the hello reproduction: rows, summaries, `strace`,
  `strace -c`, `perf stat`, loads; `phase2a.log` (the pieces, the first
  head timing).
- `item2/` — the bisection: `bisection-table.md`, three rounds' rows and
  summaries, one `strace` and one `perf stat` per build.
- `item3/` — `stackprobe.log`, `equivalence-eager.log`, `variants.log` and
  its rows, `short2.log` (the lazy head's counts, the equivalence test,
  the overflow rows on both tiers), `bindings.txt`,
  `profile-instructions.txt` (a `perf record` attempt that could not
  attribute the loader's samples; kept, not relied on).
- `yew/` — `yew-table.md`, three rounds of each row, the build times,
  `phase2b-run1.log` (the cells' builds and digests).
- `gauntlet1/` — phase 2b's log (bisection, timings, overflow rows, the
  first gauntlet and the differ), the gauntlet's tail and exit, the
  three differ logs.

## 7. Issues to file (not filed: RULES §3)

- **yew**: yew#55's rows are not wolf-lang#918's — measure the start-up
  rows over enough starts (here 150 a cell; R-yw07-12's median of five
  still swings 2x on the same binary) before re-pinning; the toolchain's
  share is +0.04 ms. wolf-lang#918 itself closes with this PR.
- **wolf-lang**: `copy_write_lanes`' lupin pin list stops at 0.1.55
  while PAIRING names 0.1.56, so the row is red wherever 0.1.56 answers
  `unsupported` on `fs/copy_chunk_failed_write.lu` (owner: the next lane
  on wolf-lang or is82's mirror) — conditional on the trunk run in §8.
- **wolf-lang**: bench-gates' start-up wall row (report-only) has no
  floor yet; a nightly row on a quiet rig would give it one (D5).

## 8. PENDING at the PR's opening

`tools/gauntlet2.sh` on kasumi: the copy_write test at trunk `6480a27b`
with lupin 0.1.56 (the attribution), then the gauntlet at the rebased
head `53b95d3a` with every step past a red one run alone; it exits with
the worst step. Its log and exits land in `gauntlet2/` when it ends.

## Done-when (the contract's)

- [x] Item 1: hello world reproduces the move (+38.9 us, +17 calls), with
  `strace -c` and `perf stat`, 50 interleaved cold starts, spreads and
  loads.
- [x] Item 2: every landing commit built and timed; the commit is
  `75279748`, as predicted.
- [x] Item 3: the incidental cost fixed (`ce3b45eb`..`9f689e7d`); the
  intended safety's cost asked as R-s265-1 with the numbers, ruled B,
  implemented (`a50b724d`, `5627fdc1`); 31.3 us recovered a start.
- [x] Item 4: the start-up row in bench-gates (deterministic, 12), the
  wall row report-only with the reason; the plant red by run id
  (40147037892); the differ against 0.2.35 with every moved row named.
- [ ] Item 4: the gauntlet at the head exit 0 — PENDING (§8).
- [x] Item 5: this note; `Footprint: cross-cutting` (the runtime moves).
- [ ] `tier pr` green at the head; `tier full` (the orchestrator's).

## README

README read against the pass: it states no start-up figure, gate list
or runtime set-up that this change moves (the gates are in
`bench/protocol.md`, updated in `2b18fbe1`); current.

## Prune list (kasumi, `~/lanes/s265`)

`target/` (14 GB after the first gauntlet; peak about 14 GB plus the
trunk attribution's separate target, removed by its own job),
`bisect/` (17 hellos), `tc/` (the unpacked archives and the head's
staged toolchain), `wolf-lang/`, `wolf-head/`, `wolf-var/`, `wolf-trunk/`,
`yew/`, `yew-a`, `yew-b`, `yew-fix` (the cells' binaries are hashed in
the log), `sagitta/`, `wolf-std/`, `archives/`, `scratch/`, the
`s265-ubuntu` image (`a03d9a48`); on the Mac `~/lanes/s265`,
`~/lanes/s265-plant`, `~/lanes/s265-target`, `~/lanes/s265-target197`.
The remote branch `s265-plant-startup` after the merge.
