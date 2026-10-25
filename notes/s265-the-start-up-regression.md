# s265 — the start-up regression (wolf-lang#918, yew#55)

Lane note. **Opus 5.5. Crawl 54.** Branch `s265` off wolf-lang trunk
`7b7b1753`. Contract: `sprints/compiler/s265-the-start-up-regression.md`
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
or closed (listed in §7); no label on a PR; no `git config` on the Mac; no
build, witness or timing on the Mac (only edits and `cargo check`); on
kasumi nothing outside `~/lanes/s265` was written, every job ran in the
lane's own rootless container on CPUs 4-7 under the lane lock with
`ulimit -c 0`; no host package installed; no other lane's file read
beyond s264's published shim and validation scripts (the reference the
brief names); no kernel, libc or other implementation's source read —
the main stack's bounds were matched to `pthread_getattr_np(3)`
black-box (`tools/stackprobe.c`, the equivalence test), and sagitta (the
maintainer's own C yew, the yew pin's oracle) was read and linked for
its PTY observer only. No attribution trailers.

## Inputs verified

- wolf-lang trunk `7b7b1753` (the base; s265 branched there).
- The release archives, digests from the release API read twice and the
  downloads hashed twice on kasumi (`archive-digests.txt`):
  - 0.2.34 `9da7bdcf`, `wolf-0.2.34-x86_64-unknown-linux-gnu.tar.gz`
    `88a9dd3da1c9a0e38c08eb6e2de9c1ef04d1ed93dcf51024d8dbbb76890c1821`
    (bundled std `e036bec`);
  - 0.2.35 `cf0c13d3`, `wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz`
    `4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a`
    (bundled std `2386522`);
  - lupin 0.1.56 (PAIRING), `ad703550ac4787e8e4bb26989d591a731410274d7332f2b4e8b9a64c878822cc`.
- The container: `ubuntu@sha256:534baea6a22c03a63003dbc8dbe78fe34bc0d7e595d9a9dc9834884ff530eb55`
  with clang 18.1.3, lld, strace, perf 6.8.12, gdb, qemu (`image/Containerfile`),
  image `a03d9a48`; the taskset shim (0-3 → 4-7, 0 → 4, static C,
  `execv` of `taskset.real`) and a C `sigclean` (empty mask, four
  defaults) in place of s264's Python; `--pids-limit 16384 --ulimit
  nproc=32768:32768`; pid 1 is podman's init as container root, the
  command dropped to the lane uid by `setpriv` (`kill -0 1` is EPERM,
  checked); SigBlk and SigIgn zero (`image/ctr`).
- yew `280012fe` (cell a, pin 0.2.34) and `280012fe` + yw07's
  `compile-compat.patch` read through `git show 730115b:` (cell b, 0.2.35;
  `git diff 280012fe 7a9ae6e -- src tools runtime` is empty, so this is
  the attribution's b-prime source); sagitta `9ace98c2` (yew's
  `sagitta.pin`).
- kasumi: i9-11900K, 8 cores / 16 threads; CPUs 4-7 share their cores
  with 12-15 (`thread_siblings_list`), so every table names the load
  average and the busy share of 4-7 and 12-15 (`tools/hostload.sh`);
  governor `powersave`; linux 7.2.8.

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
more text and 696 more bss. The seventeen, in order, all from one place:
before `main`, `openat("/proc/self/maps")`, `prlimit64(RLIMIT_STACK)`,
`fstat`, three 1 KiB `read`s, `close`, `sched_getaffinity` (that is
`pthread_getattr_np(3)` on the main thread), `sigaltstack` (query), a
32 KiB `mmap`, `sigaltstack` (install), four `rt_sigaction` (two reads,
two installs, SIGSEGV and SIGBUS); after `main`'s last write, a
`sigaltstack(SS_DISABLE)` and the `munmap` (a thread-local destructor
on the main thread). That is s256's `__wolf_rt_stack_start`, which the
entry shim calls unconditionally (`add_entry_shim`), so every 0.2.35
program pays it.

## 3. The cause, the fix, and R-s265-1

**What each piece costs.** Two instruments. A C program doing one piece
each (`tools/piece.c`, 50 starts, host load 7, so read the differences,
not the absolutes): `pthread_getattr_np` on the main thread +31.2 us and
+63,188 instructions; a raw read of `/proc/self/maps` +11.7 us; the
altstack and the four `sigaction`s together under 1 us. And runtime
variants linked under one driver (the driver links the `libwolf_rt.a`
beside it; `tools/variants.sh`, three rounds of 50, host load 1.1-1.7):

| hello, release tier | calls | page faults | instructions | medians, three rounds of 50 |
|---|---:|---:|---:|---|
| 0.2.34 (published) | 42 | 63 | 173,381 | 239.3 / 241.3 / 244.7 us |
| 0.2.35 (published) | 59 | 69 | 259,616 | 279.6 / 282.7 / 276.5 |
| V1: no registration at all | 42 | 63 | 173,363 | 237.7 / 237.2 / 237.6 |
| V3: the handler only | 44 | 64 | 180,988 | 241.8 / 239.6 / 238.5 |
| V2: the handler and the altstack, no bounds query | 47 | 63 | 182,038 | 240.6 / 239.2 / 241.8 |
| the eager direct read (runtime at `ac67a2a3`) | 51 | 67 | 242,282 | 263.2 / 265.4 / 266.9 |
| C (`bench/startup/hello.c`) | 35 | 53 | 112,486 | 209.6 / 207.2 / 205.2 |

So the reporter itself (handler and altstack) costs about 2.5 us; the
main thread's stack-bounds query costs the rest, about 37 us of the
0.2.35 delta — and the bounds are needed only when a fault arrives.

**The incidental part, fixed** (no behaviour change; the bounds are
held equal to `pthread_getattr_np`'s):

- `e52658ce` splits the registration into its bounds and altstack halves
  (a refactor);
- `b68d9afe` registers the main thread through `enter_main_thread`: the
  bounds read from `/proc/self/maps` (proc(5)) and `RLIMIT_STACK`
  directly — the mapping that holds the frame, grown down to the limit,
  never into the mapping beneath it, the guard the page below — instead
  of `pthread_getattr_np`'s stdio, scanf, `fstat` and CPU-set query; and
  the main thread's altstack kept for the process's life (its end is the
  process's), so no thread-exit destructor and no `sigaltstack` +
  `munmap` at exit;
- `fcd89f54` holds the direct bounds to `pthread_getattr_np`'s
  (`crates/wolf_rt/tests/main_stack_bounds.rs`, on the main thread, no
  libtest): this process, then children under a 4 MiB, an 8191 KiB (not
  a page multiple), a 64 MiB and no stack limit, and under a 1 MiB
  environment — equal in all six (`item3/equivalence-eager.log`,
  `item3/short2.log`); the black-box probe behind the formula is
  `item3/stackprobe.log`;
- `e49be6e6` reads the replaced fault disposition in the installing
  `sigaction` (one call a signal, not two; the `SIG_IGN` rule unchanged);
- `ac67a2a3` reads the map in one call.

That leaves 51 calls and +25 us (+10 %): the eager query is still a
procfs read the start does not need.

**R-s265-1 (asked; recommendation B implemented, `26d3a10d`).** What the
cost buys: `[conf.trap.stack]` — a main-thread stack overflow reported as
`wolf-trap: stack-overflow` with its PC and exit 134, not a bare signal
death (0.2.34's exit 139). The release intended it; the cost is the
query, not the safety.

- (A) Keep the eager query and document it: +38.9 us a start on 0.2.35
  as shipped (+16 % on hello); +25 us (+10 %) with the incidental fix
  alone (the runtime at `ac67a2a3`; `26d3a10d` reverted, `bench/startup.json` at 16).
- **(B) Make the bounds lazy — paid at first use.** The entry installs the
  altstack and the handler and records an address on the main stack and
  the page size; the handler resolves the main thread's guard on its
  first fault with the same direct read (`open`, `read`, `close`: all on
  signal-safety(7)'s list; a 4 KiB buffer on the 32 KiB altstack;
  `getrlimit`). Start: 47 calls, +2.5 us (+1 %) over 0.2.34 (V2's row);
  every overflow row unchanged: `corpus/faults/stack_overflow.lu` at
  depth 200,000 and 300,000 exits 134 with `wolf-trap: stack-overflow`
  and its PC on native and release, as on 0.2.35 (`item3/short2.log`);
  `tests/stack_overflow.rs` ok. What it changes: the handler now reads a
  file on the main thread's first fault, which the s256 comment ruled
  out ("no ... file I/O in a handler"; the comment is amended); the
  binaries are BIND_NOW (`item3/bindings.txt`), so no symbol lookup runs
  there; a map that cannot be read at that moment (no descriptor free)
  leaves no guard and the fault dies the ordinary way — the same answer
  0.2.35 gave when its query failed at entry.
- (C) A flag that skips the reporter: makes a spec'd report optional, and
  the program that needs it most (the deep recursion nobody expected)
  is the one that would not set it. Not recommended.

**Recommendation: B**, measured above: it returns 36 of the 38.9 us and
keeps every report. If the ruling is A: revert `26d3a10d` and
`df1bf08b` and record 16.

## 4. The gates

- **The start-up row** (`c2243e5c` the gate, `2c825949` and `df1bf08b`
  the record, `30009a38` its row in `bench/protocol.md`): bench-gates'
  fourth gate builds `corpus/hello.lu` on the release tier with the
  gauntlet's own driver and `bench/startup/hello.c` with the host's clang,
  traces each once with `strace -qq`, and holds the difference in calls
  (wolf minus C: the runtime's own start-up and exit; the loader's calls
  cancel) at or below `bench/startup.json`: **12** (47 - 35 in the
  container; 0.2.34 was 7 over C, 0.2.35 24, the eager direct read 16).
  No `strace` is a loud skip, a failure under
  `WOLF_SYSCALL_REQUIRE_STRACE` (every CI job sets it).
- **The wall row is report-only.** The gate the contract names (the
  0.2.34 number plus 10 %) needs a spread below 10 %; on kasumi the
  50-start spread of hello is 7.5-31 % ((p90-p10)/median, the rounds
  above), one round's median moves 2-5 % against the next, and CI's
  runners are not this host. bench-gates prints the medians and the ratio
  to C beside the count; D5 keeps them out of the merge gate.
- **The plant**: `s265-plant-startup` (`c18ce4e8`, on `643a967b`) restores
  the eager 0.2.35 registration (`__wolf_rt_stack_start` →
  `enter_runtime_thread`: `pthread_getattr_np` on the main thread and its
  altstack unmapped at exit); dispatched as `-f jobs=lint-test`: run
  40147037892 (PENDING: read at resume). The first dispatch, run
  40146566459 at `58924a44`, went red at `fmt` on this branch's own
  unformatted code — not the gate; superseded by `2e0d98c6` (rustfmt) and
  `643a967b` (clippy's type-complexity), and the plant re-cut.

## 2. The bisection, yew's rows, the gauntlet and the differ

PENDING: phase 2b on kasumi (`tools/phase2b.sh`, pid 2493612, log
`~/lanes/s265/evidence/phase2b/phase2b.log`, exit marker
`~/lanes/s265/evidence/phase2b/phase2b.exit`): yew's three cells built
(a 0.2.34, b 0.2.35, fix the head), the fifteen landing commits and
trunk built and their hellos timed, yew's cold-open and clean first
paint 3 x 50 each, the overflow rows at the head, the gauntlet at
`643a967b` (the code head; the head's toolchain for the timings was
built at `26d3a10d`, and the two commits since are rustfmt and a type
alias), the differ against 0.2.35 on all three tiers.
