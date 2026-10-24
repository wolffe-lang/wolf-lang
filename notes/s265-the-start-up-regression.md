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
