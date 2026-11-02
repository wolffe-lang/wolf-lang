# s275 — the drain thread survives a fork (wolf-lang#1006, pelt#98)

Contract: `wolf/sprints/compiler/s275-the-drain-thread.md` (crawl 54, a follow-on of pelt lane sh34). Base: wolf-lang
trunk `7332772f` at launch, rebased once onto `bb728d66` (s274's #1007, tests-only). Branch `s275`, PR #1011; the
plant `s275-plant-errno`, draft PR #1012. Every red and proof run below is linux: hasu, block B, in
`localhost/r41-toolchain:1` (rootless podman, kernel 7.2.4), pinned with `taskset` inside the container to four of the
block's CPUs (6-9).

## Prediction

Committed before any change, `98641479` (`notes/s275-prediction.txt`). It said:
- the witness would be red in most runs on four CPUs at trunk;
- three deterministic rows would be red at trunk and green at the fix;
- the audit would find no other site of the shape;
- neither the checked machine nor lupin has it;
- the head would show 0 red in 200 witness runs and 0 F0 stalls in 100;
- the plant would be red by name.

How it held:
- **Held, and stronger than predicted.**
  - The witness was bad in every trunk run, not most: 230 of 230 native and 30 of 30 release. Of those, 242 runs lost the thread (exit 1) and 18 stalled in the reap (exit 124).
  - The three rows were red at trunk, green at the head.
  - The head showed 0 red in 400 witness runs and 0 F0 stalls in 200.
- **Missed, and found by the plant on the Mac.** My first fix retried EAGAIN on every host as a transient poll failure. That masked the bug: with the errno read moved back after the park, the futex's EAGAIN was simply retried, and the plant read green (`logs/mac-turn-2-red-head-plant.log`).
  - Now the drain thread retries only the one failure the host's poll(2) names a shortage: ENOMEM on linux, EAGAIN on macOS (`ff7a6265`). Linux's poll never answers EAGAIN; there it is the futex's errno.
  - The hold row now asks two clobbers, the futex's EAGAIN and an EINVAL no host retries (`cc3984bd`), so it reads the errno's order on macOS too.
- **Not predicted: a row that was wrong on macOS.** macOS hands a gone thread's `pthread_t` to the next thread. The recovery row compared ids and went red at a correct plant (`logs/mac-turn-2-red-head-plant.log`); it now counts thread starts (`d8747e1a`).
- **Not predicted: an old row's flake at the labelled head.** The labelled tier full at `abd9482f` (run 41488742290) went red on macOS in `wait_fd_wakes_for_a_signal_while_parked`, "1 of 200 waits took over 50 ms". It is not mine: the wake latency is the same at trunk and the head, and trunk failed the same row on 2026-10-23. The row's assertion changed by ruling (R-s275-6, R-s275-7); see "The macOS row" below.

## Inputs, verified

- **Base**: wolf-lang trunk `7332772f` at launch (`git rev-parse origin/trunk`, read twice). It was rebased once onto `bb728d66`, as directed. The red commit, the head and the plant have empty diffs in `crates/wolf_rt`, `fork_lanes.rs`, the fixtures and `spec/` across the rebase (`git diff --stat` on each pair).
- **Trunk carries sh34's bug exactly**: `crates/wolf_rt/src/signal.rs` and `crates/wolf_rt/src/fork.rs` are byte-identical between pelt's pin 0.2.36 (`5adf3d91`) and trunk.
- **sh34's evidence**: wolf-lang#1006, read whole; pelt branch `sh34` at `97d23ea`: `notes/sh34/witness/drain_after_fork.lu`, `strace-red-drain-thread.log`, `runs.log` (9 of 10 red).
- **The replay tool**: `tools/wait-replay`, copied by the job from the branch at `7908f94`. Its sha256 at both commits is `cd7476566cca1b32e57ec59153cd817836720961384ac03aaf15f0050ac491eb`.
- **pelt**: trunk `81562fd` (read in the job), and its pinned wolf-std `78b9d3cb` (`STD-REV` checked in the job), copied from sh34's staged pin.
- **Toolchains**: the Mac's turns 3 and 4 ran rustc 1.97.1 (`8bab26f4f`) and clippy 0.1.97, the pin. Turn 2 ran Homebrew's 1.99.1 by mistake (first on PATH); it is kept and labelled. hasu ran `/opt/s263-rust` 1.97.1.

## The rows (red first)

1. **The witness**: `crates/wolf_driver/tests/fixtures/fork/drain.lu`. It is sh34's `drain_after_fork.lu` on the intrinsics (a checkout's `wolf` has no std beside it): CHILD listened, eight forks back to back whose children end at once, `Threads:` read from `/proc/self/status` before each reap.
   - `fork_lanes::the_drain_thread_outlives_every_fork` runs it on both tiers, and under four CPUs where the fork witnesses run (CI's linux container, CI's macOS runner). That is 1,000 rounds a row, under a 120 s cap: `timeout` in the container, perl's `alarm` on the macOS runner.
   - hasu's container has no nested engine, so there the binary runs directly (`scripts/hasu/witness-runs.sh`, 3,000 rounds a run).
   - Red at trunk: the red commit `fafb1a06` = trunk + the rows. The runtime a program links is trunk's, since the hooks are `cfg(test)`.
2. **Three deterministic rows** in `crates/wolf_rt/src/signal.rs`, each run alone in a child of the test binary and killed by its pid at a 30 s cap. A red row leaves the process-global hub without a drain thread, which would hang every later signal row in the same process. Test-only hooks (`sys::hook`, `cfg(test)`) stand in for the race: the park for a fork's hold leaves an errno (the futex's), the drain thread's next poll answers a chosen errno, and a signal is aimed at the drain thread with `pthread_kill`.
   - `s275_a_poll_interrupted_in_a_forks_hold_keeps_the_drain_thread`: a fork's hold (`sys::fork_hold`), a SIGHUP on the drain thread, its poll answering EINTR, the park leaving EAGAIN and then EINVAL, the release. The thread must move the signal's byte onto the queue.
   - `s275_a_transient_poll_failure_keeps_the_drain_thread`: the poll answers the host's shortage; the thread must go on.
   - `s275_a_drain_thread_that_ends_is_made_again`: a wait parked on the condvar (SIGHUP blocked on its thread and the test's), and the poll answering EBADF, so the thread ends. The wait must wake, make the thread again, and take a SIGHUP that only the new thread can move.

Why hooks and not a real fork loop in the unit test (R-s275-2): the race is the futex's, a few instructions wide, and a fork storm inside `cargo test` would run on the Mac's process table (RULES §5). The witness row is the real race, in a container.

## The fix

- `reader_loop` reads the poll's errno right after `poll` returns, before `back_from_poll()` can park the thread (`f5b6d366`).
- **The retry set** (`ff7a6265`, R-s275-3):
  - EINTR goes on at once.
  - The one failure the host's poll(2) names a moment's shortage (`POLL_SHORT`: ENOMEM on linux, EAGAIN on macOS and the BSDs) goes on after 1 ms.
  - Anything else, and one of the hub's own pipes gone, ends the loop.
- **The end is never silent** (`reader_gone`, R-s275-1):
  - The state reads ending. A new `READER_ENDING` is waited out by `fork_hold` as it waits out a moment of work, since the exit path takes three locks.
  - The thread's own unblocks are forgotten, and the next thread unblocks the armed meanings again (`UNBLOCK_AT_START`, s247's flag renamed).
  - READER_UP is cleared under the hub lock, and every parked wait is woken: `wake_all`, factored out of `drain`, covers the condvar and the descriptor waits' shared pipe.
  - The state reads gone unless a new thread has already started.
- **The waits**: `os_signal_wait` and `os_signal_wait_fds` read `running()` under the hub lock before they park. A wait that finds the thread gone makes it again (`try_hub`), once a call, and answers `io` if it is gone again (R-s275-4): never a park for ever, never a spawn loop.
- **The spec** says so in `[os.signal.hub]` (`df56bf91`, `62887ed5`). No anchor moved: `spec/anchors.json` maps a clause to its file only.

## The audit: every errno read in the hub and the fork path

| site | the call | what runs between it and the read | verdict |
|---|---|---|---|
| `signal.rs` `reader_loop` | `poll` | `back_from_poll()`: a park, a futex | **the bug; fixed** |
| `signal.rs` `cloexec_pipe` | `pipe2`; macOS `pipe` | nothing (macOS's `fcntl`s run only on success) | right |
| `signal.rs` `drain` | `read` | nothing | right |
| `signal.rs` `wait_many` (`errno()`) | `poll`; `select` | nothing | right |
| `signal.rs` `unblock` | `write` | nothing | right |
| `signal.rs` `kill_code` | `kill` | nothing | right |
| `signal.rs` `trampoline` | its `write` | (a handler between any call and its read) | right: errno saved and put back |
| `fork.rs` `fork` | `fork` | the `pid == 0` test | right |
| `stdfd.rs` `off_std` | `fcntl(F_DUPFD_CLOEXEC)` | nothing (read before the `close`) | right |
| `reactor.rs` `Poller::new`, `arm`, macOS `new` | `epoll_create1`, `eventfd`, `epoll_ctl`, `kqueue`, `kevent` | nothing | right |
| `reactor.rs` `Poller::wait` (linux, macOS) | `epoll_wait`; `kevent` | `leave_kernel_wait()`: the same park | right: no errno is read; any error loops, so the reactor thread cannot end |

The pool's hold point parks too and reads no errno. Only `reader_loop` had the shape.

## The checked machine and lupin (item 4)

- **The checked machine** (`crates/wolf_mem/src/ubcheck.rs`, `checked_sig`) has none of the shape:
  - it has no drain thread: its handler sets a bit in `CHECKED_PENDING`, and the machine's one thread polls;
  - it reads errno straight after its own `poll` and `select`;
  - `os_fork` answers `unsupported` there (ruling #95 R4 C), so no hold exists.
- **lupin** (wolf-interp trunk `ed71184`) has `unsafe_code = "forbid"` and refuses `os_signal_listen`, `os_signal_wait` and `os_fork` by name (`src/eval/builtin.rs`). No hub, so no mirror is filed.

## Proof (hasu, block B, CPUs 6-9, one rig)

| what | where | runs | green | red (thread gone) | stalled (reap parked, `timeout` 124) |
|---|---|---|---|---|---|
| the three `s275_` rows at trunk + rows `fafb1a06` | `logs/hasu-red.log` | 3 | 0 | 3 FAILED | — |
| witness, native, trunk (`fafb1a06`) | `logs/hasu-red.log` | 30 | 0 | 29 | 1 |
| witness, release, trunk (`fafb1a06`) | `logs/hasu-red.log` | 30 | 0 | 27 | 3 |
| `signal::tests` at the head `dd289a60` | `logs/hasu-proof-witness.log` | 29 | 29 | 0 | — |
| witness, native, head `dd289a60` | `logs/hasu-proof-witness.log` | 200 | **200** | 0 | 0 |
| witness, release, head `dd289a60` | `logs/hasu-proof-witness.log` | 200 | **200** | 0 | 0 |
| witness, native, trunk (`fafb1a06`), the same rig | `logs/hasu-proof-witness.log` | 200 | 0 | 186 | 14 |
| witness, native, the plant `1c7a73ba` | `logs/hasu-proof-witness.log` | 30 | 0 | 30 | 0 |
| pelt F0 (`wait-replay`), native, pelt `81562fd` + the head's wolf | `logs/hasu-proof-f0.log` | 100 | **100** | — | **0** |
| pelt F0, static (PAX's shape) | `logs/hasu-proof-f0.log` | 100 | **100** | — | **0** |

- **Timing**:
  - At trunk the thread was gone at round 0 to 44 of 3,000 (median 4), every run.
  - At the head every 3,000-round run took 2.69 to 2.95 s (native median 2.76 s, release 2.78 s).
  - Every run's exit and time is in `logs/hasu-witness-every-run.txt`.
- **F0**: every one of the 200 replays kept two tasks at every prompt (the watch). sh34 measured the F0 rate at trunk in its own rig (pelt's PR); this lane did not re-run trunk's F0.
- **CI**: `tier pr` run 41362666151 at `dd289a60` is green. Its four `drain.lu` rows in the linux container all read `drain: 1000 rounds of 8 forks, the drain thread at every one true | rc=0`, and the three `s275_` rows are ok (`logs/ci-41362666151-head-rows.log`).

## The plant

`s275-plant-errno` (`1c7a73ba`, draft #1012) = the head `dd289a60` with the errno read moved back after `back_from_poll()`. The retry set, the recovery and the rows are kept.
- **CI**: its `tier pr` run 41362692004 is red by name: `fork_lanes::the_drain_thread_outlives_every_fork … FAILED` (14 passed, 1 failed). The report reads `REPORT drain.lu native: drain: round 805: threads 1 of 2 after 8 forks: the drain thread is gone | rc=1` (`logs/ci-41362692004-plant-fork-lanes.log`).
- **The wolf_rt hold row did not run in that run.** `cargo test --workspace` runs `wolf_driver`'s test binaries before `wolf_rt`'s, and the gauntlet's `test` step stopped at the first failing binary (`fork_lanes`, 23:44:01Z), so `s275_a_poll_interrupted_in_a_forks_hold_keeps_the_drain_thread` never ran there. Its red at the plant was measured on macOS: `logs/mac-turn-4-head-cc3984bd-plant-a8ea1410.log`, the same code as `1c7a73ba` (an empty diff under `crates/`). The row failed on its EINVAL pass, "the park leaving errno 22 (state 5, 5 = gone …)", while the other two rows stayed green.
- **hasu**: the witness at the plant is red in 30 of 30 runs (thread gone at round 0 to 41; `logs/hasu-proof-witness.log`). With the recovery kept, no plant run stalled: the next `os_signal_wait` made the thread again.

## The macOS row (R-s275-6, R-s275-7)

The labelled tier full at `abd9482f` (run 41488742290, attempt 1, job 125425931192, `build + test (macos-latest)`) failed one row: `signal::tests::wait_fd_wakes_for_a_signal_while_parked` (s234), "1 of 200 waits took over 50 ms", with 345 passed (`lat/ci-41488742290-abd9482f-macos-attempt1.log`). My change is on the path that wakes a parked wait, so this was measured, not called noise.

**The wake latency on nomad, trunk `bb728d66` against `abd9482f`.**
- Method:
  - Measurement-only commits, never pushed, print each wait's latency: `c4c0e0fb` = trunk + the print, `9f3c5882` = `abd9482f` + the same print (`lat/scripts/lat-patch.py`).
  - Built once under the Mac lock with rustc 1.97.1 and the 8 GiB gate.
  - Ten turns per condition, one lock take per turn and at least 65 s apart; each turn ran trunk once and the head once, the order alternating (`lat/scripts/series.sh`, `series-suite.sh`).

| condition | side | waits | median | p99 | longest | over 50 ms |
|---|---|---|---|---|---|---|
| the row alone (`--exact`, one thread), 10 runs | trunk | 2000 | 2.54 ms | 3.09 ms | 3.13 ms | 0 |
| the row alone (`--exact`, one thread), 10 runs | head | 2000 | 2.54 ms | 3.09 ms | 3.11 ms | 0 |
| the whole wolf_rt lib binary, default threads (CI's shape), 10 runs | trunk | 2000 | 2.56 ms | 3.19 ms | 3.37 ms | 0 |
| the whole wolf_rt lib binary, default threads (CI's shape), 10 runs | head | 1999\* | 2.56 ms | 3.23 ms | 3.43 ms | 0 |

\* One head value is split across a line by another test's interleaved output. All 200 waits ran, and the run passed its own bound. Per-run rows: `lat/isolated/summary-isolated.txt`, `lat/suite/summary-suite.txt`; raw runs beside them.

- **Trunk failed the same row before s275**: push run 39990785067 (2026-10-23, trunk `1f6fb786`, job 120541753840), "1 of 200 waits took over 50 ms", 337 passed (`lat/ci-39990785067-trunk-2026-10-23-macos.log`). Of the last 40 trunk runs' macOS `build + test` jobs, 2 failed, and that one is this row.
- **What nomad cannot rule out**:
  - In the failed job the row took about 3.6 s for its 200 rounds (about 18 ms a round, against 2.5 ms on nomad), so the 3-vCPU runner was loaded.
  - My three `s275_` parent rows took no `serial()` lock, and their re-execs of the test binary ended just as the row began.
  - nomad's whole lib run (2.3 s) is uncontended, so it cannot exclude that contention.

**The rulings (the orchestrator's):**
- **R-s275-7 (yes, `180a11d4`).** The `s275_` parent rows hold `serial()` while their child runs, so a child's start never overlaps another signal row's timing.
- **R-s275-6 (A, `e730a403`).** The row is judged by `wake_verdict`:
  - at most 2 of 200 waits over 50 ms;
  - none over 1 s;
  - the median under 25 ms;
  - each failure named (`the count:`, `none over 1 s:`, `the median:`), with the slow waits' latencies and the whole count in the message;
  - a `REPORT` line written to standard error past libtest's capture whenever a wait is over 50 ms, on success too.

**Seen red before it landed**: four witnesses on nomad, under the Mac lock with its gates. They ran a local measurement-only plant, never pushed (`2004bf6b` on top of `e730a403`, `lat/scripts/witness-plant.py`). Exact outputs are in `lat/witness/`:

| witness | plant | verdict | the line |
|---|---|---|---|
| the real path | none | ok, no REPORT | `witness-w0-real-path.log` |
| (a) a 100 ms fallback on every wait | `WOLF_S275_PLANT_POLL_MS=100` | **red, the count** | "the count: … 200 of 200 waits took over 50 ms [105.0 ms, …], the median 104.54 ms" |
| (b) a reinstated 30 ms poll period | `WOLF_S275_PLANT_POLL_MS=30` | **red, the median** | "the median: … 0 of 200 waits took over 50 ms [] (at most 2), the median 34.17 ms (under 25)" |
| (c) one injected 200 ms stall | `WOLF_S275_PLANT_STALLS=1` | **green, the stall printed** | "REPORT … 1 of 200 waits took over 50 ms [204.4 ms] … the median 2.55 ms" |
| (d) three injected stalls | `WOLF_S275_PLANT_STALLS=3` | **red, the count** | "the count: … 3 of 200 waits took over 50 ms [205.1 ms, 204.3 ms, 205.1 ms]" |

At `e730a403` itself: all 28 `signal::tests` rows are green, the row is green three more times, and clippy is clean (`lat/mac-verdict-build.log`).

**What A gives up, plainly.** A race that delays 1% or fewer of wakes past 50 ms now passes this row. The printed count is where it stays visible: the `REPORT` line in every CI log where a wait went over 50 ms. A lost wake still hangs the row, and a fallback timer or a reinstated tick still fails it, on the count or on the median.

## Rulings recommended (R-s275-N), each implemented as recommended

- **R-s275-1: a drain thread that ends.**
  - Options: (A) recoverable: READER_UP cleared under the hub lock, every parked wait woken, the thread made again by the next listen or wait (a parked one included), once a call, then `io`; (B) it never ends: every poll errno retried (a hub whose pipe is gone would spin a CPU for ever); (C) the process aborts with a line.
  - **A**: B turns an impossible case into a busy loop, and C ends programs for a thread the runtime can make again. A is the contract's "the next try_hub makes one" with the parked waits included; without them, pelt's parked `wait` would still hang.
- **R-s275-2: the deterministic row.**
  - Options: (A) `cfg(test)` hooks standing in for the futex's errno and the poll's, each row in a child of the test binary; (B) a real fork loop in the unit test (a fork storm on the Mac's table, and only probably red); (C) a hidden public hook API for an integration test (the runtime's surface grows for a test).
  - **A**.
- **R-s275-3: a transient poll failure.**
  - Options: (A) retry only the host's named shortage (linux ENOMEM, macOS EAGAIN) after 1 ms; (B) retry EAGAIN and ENOMEM everywhere; (C) no retry, left to R-s275-1's recovery.
  - **A**: B masked the bug itself, since the futex's EAGAIN would be retried (the plant read green, `logs/mac-turn-2-red-head-plant.log`). C spends a thread on a documented shortage.
- **R-s275-4: a wait that finds the thread gone twice in one call.**
  - Options: (A) `io`, the hub-refused answer; (B) remade without bound (a thread that ends at once makes a spawn loop); (C) park (trunk's hang).
  - **A**.
- **R-s275-5: pelt for the F0 proof.**
  - Options: (A) built with the head's `wolf` and runtime together (trunk's compiler is 0.2.36's plus 78 commits, none in the hub); (B) the 0.2.36 compiler with the head's `libwolf_rt.a` swapped in (an archive the compiler was not built against).
  - **A**.
- **R-s275-6: what `wait_fd_wakes_for_a_signal_while_parked` asserts** (ruled by the orchestrator on 11-02, on this lane's evidence).
  - Options: (A) at most 2 of 200 waits over 50 ms, none over 1 s, the median under 25 ms; (B) a rerun of the 200 when one is slow; (C) a control sleep timed in each round; (D) keep 0 of 200 and isolate the row.
  - **A**, shown red before it landed (the four witnesses above).
- **R-s275-7: the `s275_` parent rows hold `serial()`** while their child runs. Ruled yes.

## Evidence index (`notes/s275-evidence/`)

Commits (base `bb728d66`):
- `98641479` the prediction
- `7b41ec88` red rows: the three `s275_` rows and `sys::hook`
- `fafb1a06` the witness row plus `drain.lu`; the red commit
- `f5b6d366` the fix
- `df56bf91` spec/11 `[os.signal.hub]`
- `6067674a` the rig
- `ff7a6265` the host's shortage only
- `d8747e1a` the rows count starts; the hooks at the end of `sys` (clippy `items_after_test_module`)
- `62887ed5` spec wording
- `5f7a57ec`, `04bb9cb5` the Mac turns
- `cc3984bd` the hold row's two clobbers
- `dd289a60` CHANGELOG
- `38e9d5d4` the hasu logs; `abd9482f` this note with the counts (the gauntlet's head)
- `180a11d4` R-s275-7: the `s275_` rows hold `serial()`
- `e730a403` R-s275-6: `wake_verdict`
- the latency evidence (`lat/`), and this note's macOS section

Scripts, as run:
- `scripts/hasu/`:
  - `launch.sh`: the detached waiter, both lock names, one `flock -w 21600` each, the pid, log and exit files;
  - `ctr.sh`: the container, bash as pid 1, `--memory 12g`;
  - `red.sh`, `proof-witness.sh`, `proof-f0.sh`, `build-at.sh`, `witness-runs.sh`;
  - `gauntlet.sh` and `g/`: s273's gauntlet rig, copied; the taskset shim, a foreign pid 1.
- `scripts/mac-run.sh` and its step files: one turn on the Mac lock, the 8 GiB gate, the disk gate (from 11-02), a worktree pinned to a commit.
- `lat/scripts/`: the latency series (`series.sh`, `series-suite.sh`, `summary.py`), the measurement patch (`lat-patch.py`), the witness plant (`witness-plant.py`), and their step files.

Logs (`logs/`):
- `hasu-red.log`, `hasu-proof-witness.log`, `hasu-proof-f0.log`: the three jobs, every `BEGIN`/`END`.
- `hasu-witness-every-run.txt`: every witness run's exit, milliseconds and line.
- `ci-41362666151-head-rows.log` and `ci-41362692004-plant-fork-lanes.log`: excerpts of the two CI runs.
- `mac-turn-1-relative-path.log`: a turn lost to a relative step file.
- `mac-turn-2-red-head-plant.log`: Homebrew's 1.99.1. Rows red at the red commit, green at the fix; the plant masked by the old EAGAIN retry; the pthread_t row wrong.
- `mac-turn-3-head-cf3b11df.log`: 1.97.1. 28 `signal::tests` green, clippy clean.
- `mac-turn-4-head-cc3984bd-plant-a8ea1410.log`: 1.97.1. The head green; the plant red by name in the hold row.
- `lat/`: the macOS row.
  - `isolated/` and `suite/`: the two latency series, summaries and raw runs (`suite/runs.tar.xz`).
  - `witness/`: the four witnesses' exact outputs.
  - `mac-build.log`, `mac-verdict-build.log`: the builds.
  - `ci-39990785067-trunk-2026-10-23-macos.log`, `ci-41488742290-abd9482f-macos-attempt1.log`: the two CI failures.

## Forbidden

- No kernel's, libc's, shell's or other implementation's source was read, and Rust's standard library source was not read either: the futex's EAGAIN is sh34's strace, black box. wolf-lang's own runtime was read and changed; wolf-interp's and wolf_mem's checked machine were read (the maintainer's own).
- No attribution trailer in any commit or PR. No label, merge, tag or release; no issue filed or shut. No CI run waited on, re-run or cancelled: each run id was read once and handed back.
- kasumi untouched. hasu:
  - every job took both of its block's lock names, the block name outside, one `flock -w 21600` each, detached with a pid file, a log and an exit marker, on the emptier block at submission;
  - containers keep bash as pid 1;
  - nothing was killed but a row's own capped child (by its pid) and `timeout`'s child.
- The Mac:
  - one turn per call on the build lock, at least 60 s between this lane's turns, the 8 GiB gate a conditional, builds in a worktree pinned to a commit;
  - the disk gate (15 GB free before a build, 2026-11-01 22:30Z) was missed on 11-01's turns and kept from 11-02 on. The volume never fell under it (62 GB or more), and the build output was pruned after the runs;
  - no fork ran on its table (the fork rows skip there, and `drain.lu` was only compiled).
- Nothing under `/var/folders`, `~/.claude` or the planning repo, and nothing of the maintainer's data. Other lanes' files were reused by copy only: sh34's std tree and `tools/wait-replay`, s274's cargo registry, r41's archives, s273's gauntlet scripts.

## Done-when

- [x] Item 1: red rows before the fix (`7b41ec88`, `fafb1a06`): the witness bad in 60 of 60 trunk runs on four CPUs, the three rows red (`logs/hasu-red.log`); a deterministic row built (R-s275-2)
- [x] Item 2: the errno read first; the end never silent; every errno read in the hub and the fork path audited (the table)
- [x] Item 3: the witness 0 red in 400 at the head beside 200 of 200 bad at trunk, from one rig; pelt's F0 0 stalls in 100 native and 100 static
- [x] Item 4: neither the checked machine nor lupin has the shape; no mirror
- [x] Item 5: the plant red by name (CI 41362692004; the hold row's red on macOS); the note, the CHANGELOG, the PR
- [x] `tier pr` green at `dd289a60` (41362666151)
- [x] the gauntlet at `abd9482f`, hasu block B: `S275_GAUNTLET_GREEN … steps=23`, every `END` 0
- [x] R-s275-6 and R-s275-7, as ruled; the new assertion seen red by name before it landed (`lat/witness/`)
- [ ] the gauntlet again at the final head, which adds `180a11d4` and `e730a403` (test code) after `abd9482f` (in the hand-back)
- [ ] `lane-audit.sh` at the final head, after CI is green (in the hand-back)
- README read, current.
