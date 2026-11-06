# s284 — the drain row's stall: a fork claims the drain thread at work (wolf-lang#1022)

`fork_lanes::the_drain_thread_outlives_every_fork` ended `rc=124` on CI in 3 of 5 runs of the row
on 2026-11-06. The parent was never stuck in its own wait. A forked child was stuck: it was born
holding the hub's state lock, which the parent's drain thread held at the instant of the fork, and
it waited for ever in `fork_child_pipes`. The parent's reap then waited for a `SIGCHLD` that never
came, until the row's cap.

The drain thread was working during a fork because of one second look. In `back_from_poll`, a
claim (`POLLING -> LOOPING`) that failed because a fork held the thread looked at the state again
and went on unless that look read the hold too. A fork's release that landed between the two
looks read as "no fork here". The thread then went to work with the state still saying "in its
poll", and the next fork, microseconds behind in drain.lu's back-to-back loop, claimed it there.
The reactor thread's `leave_kernel_wait` had the same shape. Now each acts on its claim's own
answer: a claim that finds the hold parks and claims again, and nothing else is looked at.

**What s275 fixed, and what this fixes.**
- **s275 (wolf-lang#1006):** the drain thread read its poll's errno after its park for a fork's
  hold. The park's futex could answer `EAGAIN` first, so an interrupted poll looked fatal and the
  thread ended; every later wait parked for ever. That thread was gone (`Threads:` fell). s275's
  `drain.lu` row counts threads after every round to catch it.
- **s284 (wolf-lang#1022):** the thread lives, but a fork could claim it while it worked, and the
  child was made holding the hub's lock. A stalled run here has `Threads: 2` and a live child
  stuck in `fork_child_pipes`. This stall was never s275's bug come back.

**Why CI saw it and a quiet box did not.**
- The slip happens with either runtime, about a dozen times a run quiet or beside churn.
- It becomes a claim at work, and then a child stuck on the copied lock, only when the drain
  thread's path from the slip back to its poll is slow. That is the debug-profile runtime that
  `cargo test`'s `wolf` links, which CI's row uses.
- Trunk's release-runtime binaries never let a fork claim the thread at work and never stalled
  (0 in 60). The debug-runtime binaries stalled about 1 run in 80 on H1, quiet or beside churning
  neighbours.
- CI's rate is higher (3 of the 20 lanes in its last 5 runs). A run that loads its four CPUs with
  hogs, a quota or a low priority makes it rarer, not commoner: contention mostly keeps the thread
  from coming back during a hold at all.
- **Unexplained:** why CI went from one stall (2026-11-02, #1009's head) to 3 in the row's 5 runs
  from 02:30Z on 2026-11-06. The
  only runtime change since s275 is the hashed Map (`d531f947`), which drain.lu never calls. I did
  not measure its effect on the binary's link layout. The fix closes the race whatever moved the
  rate.

**The proof.**
- **Primary, the event count (R-s284-3).** One detector, built into trunk and into the fix the
  same way: hook points (a)-(e) below, and trunk's second look (f). Interleaved quiet runs of CI's
  binary, 50 runs each. Trunk + detector `0d6fb1ff`: 764 slips, **41 claims at work in 26 runs**,
  and 46 takes of the hub lock during a fork's hold in 25 runs. The fix + detector `3062e28e`:
  **0, 0 and 0**. At trunk's 0.82 claims a run, 0 in 50 runs for an unchanged binary is a chance
  of about e^-41.
- **The deterministic rows.** `signal::tests::s284_…_drain_thread` and its reactor twin are red at
  the red commit by name, and green at the fix with every `signal::tests` and `reactor::tests` row.
- **No regression (proof1, the supplement).** In the row's own shape, the fix ran 0 stalls in 304
  runs, interleaved with trunk's binary, which stalled 1 in 152 (proof1's table below). Medians:
  1.43 s against 1.87 s quiet, and 3.47 s against 2.82 s beside churn, both within the runs'
  spread. At H1's rate, 0 in 304 for an unchanged binary would still be a 7% chance. That is why
  the event count, not this, is the proof.
- **The reproduction row** (R-s284-1, R-s284-4) stays in `fork_lanes`. It runs drain.lu 40 times
  beside four churning neighbour containers, and it misses a reintroduced race about 0.15% of the
  time on CI and about 61% on H1. Local takes set `WOLF_FORK_CHURN_RUNS=400`, which misses about
  2% of the time on H1. That figure comes from the churn condition alone: 1 stall in 100 runs
  beside churning neighbours on H1. An earlier figure of 0.7% came from the pooled rate, 3 stalls
  in 246 quiet and churning runs together (below), which understates the churn row. The plant's
  400-run take missed: 400 of 400 green beside 1,078 neighbour containers, 361 s (#1053's body).
  CI's default of 40 runs is sized from CI's own rate and is unaffected.

## Forbidden

Footprint: cross-cutting

The fix is in `crates/wolf_rt/` (the runtime) and the spec moves (`spec/11-os.md`, two sentences in
`[os.signal.hub]` and `[os.proc.fork]`), so the PR is `cross-cutting`: the full-matrix label and
its run are the orchestrator's. The rest of the diff is test code (two `cfg(test)` hook modules
and their rows in `signal.rs` and `reactor.rs`, one `fork_lanes` row), the CHANGELOG and notes. No
CI workflow, pin, toolchain or shared tool changes. The drain row's assertion, its `timeout 120`
and its 1,000 rounds are untouched, and no retry is wrapped around any row.

- **Clean room.** I used the man pages, the spec, the runtime's own source and black-box
  measurement. The man pages, read on H1: `fork(2)` ("created with a single thread … including the
  states of mutexes"), `futex(2)` (`FUTEX_WAIT_BITSET`, `FUTEX_PRIVATE_FLAG`), and `proc(5)`'s
  `proc_pid_syscall`, `proc_pid_wchan` and `proc_pid_status`. I read no kernel's, libc's or other
  runtime's source, and not Rust's standard library either. The `Hub` layout came from drain.lu's
  own debug-runtime binary's DWARF (`dwarfdump`, on M1), and so did the futex word inside `std`'s
  `Mutex` (`inner` at offset 0). No debugger ran on any process: every look at a stalled run was a
  read of `/proc` from the host.
- **Hosts.**
  - Every heavy job ran on one of H1's two six-CPU blocks behind three lock names: the block's, then
    the block's second name, then its light lock (every take used the block's light CPUs too), each
    a `flock -w 21600`. Each was detached with a pid file, a log and an exit marker, and went to the
    emptier block at submission by the waiter count. Twice the count missed a holder that wasn't a
    `flock` process. Each time I queued the same job on the other, free block too; the first waiter
    then took its block, found the work done or its target present, and ended itself (exit 0 or 2).
    One waiter had a mistyped sha and refused at its worktree step (exit 2).
  - Three man-page reads ran through H1's light-job helper (1 busy CPU, about 3 s each).
  - The contention lived in my own containers and ended with them: busy loops and fork loops
    inside the run's container, and neighbour containers from my own host-side loops, stopped by
    their recorded pids.
  - `podman system service` ran on H1 under my own config for the length of a take only,
    stopped by its recorded pid.
  - No host setting changed. Quotas were podman's `--cpus` and `--cpu-quota`/`--cpu-period`, the
    cpu controller H1 delegates. Priorities were `nice` and `chrt -i` inside the container.
  - No commit was made on H1; branches reached my H1 clone by `git push` over ssh. No other lab
    host was used, and M1 built nothing (it ran `cargo fmt` and `dwarfdump` only).
  - Nothing ran under `/tmp` or `/var/folders`: TMPDIR was `~/lanes/s284/tmp` on both machines.
  - The only processes killed were ones I started, by their recorded pids: the churn loops, the
    API service and a run's own `timeout`.
- **No label, merge, tag, release, issue filed or closed, and no attribution trailer.**
  wolf-lang#1022 closes by hand at the merge. The plant is pushed without a PR. The
  measurement-only builds (`8e8ce00f`, `0d6fb1ff`, `3062e28e`) never left my machines.

## Inputs, verified

- **Hardware.**

  | host | what | used for |
  |---|---|---|
  | H1 | linux 7.2.4 (NixOS), i7-12700KF (12 cores, 20 threads), 46 GB, rootless podman 5.8.9, no cpuset controller delegated (cpu, io, memory, pids are) | every build, run, reproduction, proof and record, inside one six-CPU block (four of its CPUs for the runs) |
  | M1 | macOS arm64 | editing, `git`, `gh`, `cargo fmt`, `dwarfdump` on a copied binary; no build, no fork |
  | CI | GitHub's `ubuntu-latest` runner, `nproc` 4, docker | the three stalls this lane fixes |

- **Base.** wolf-lang trunk `275f9904` at launch (`git rev-parse`, read twice), on branch `s284`
  in a fresh clone. Rebased once onto `ed19904c` (s283's #1052, read twice) for the final records.
  `signal.rs`, `reactor.rs`, `fork.rs`, `fork_lanes.rs` and the fork fixtures are byte-identical
  across that rebase; its only runtime additions are data-only layout constants
  (`debug_layout.rs`).
- **The CI reds.** I read wolf-lang#1022 and #1006 whole, and s275's note. I downloaded the three
  stalled jobs' logs and kept their `fork_lanes` sections in `notes/s284-evidence/ci/`:
  - run 42150404080 attempt 1, job 127672051670 (the release lane);
  - attempt 2, job 127684481374 (the native lane);
  - push run 42164880420 attempt 1, job 127718950517 (the native `taskset -c 0-3` lane).

  In each, the stalled lane started while the other fork rows were still running their
  containers (`loop.lu`, `hub.lu`, `reactor.lu`, the pelt-shape loop).
- **The binary CI runs.** `fork_lanes` builds drain.lu with `env!("CARGO_BIN_EXE_wolf")`, the debug
  `target/debug/wolf`, and the driver links the `libwolf_rt.a` beside it, the debug profile
  (no `[profile.dev]` in `Cargo.toml`). s275's rig built with `--release`. I built both
  profiles at each commit (`rig/build-at.sh`): `dbg` is `cargo build`, `rel` is
  `cargo build --release`. Digests are in `logs/build-*.log`. At trunk:
  - `drain-dbg-native` `e54d11b3…`, `drain-dbg-release` `35c7fa53…`;
  - `drain-rel-native` `b3d53ffb…`, `drain-rel-release` `5cd8ccb1…`;
  - the debug runtime `15816a25…`, the release runtime `6ffaf933…`.
- **Toolchains and images** (H1):
  - rustc 1.97.1 (the pin) in `localhost/r41-toolchain:1` (`77fdad6b841a`), `/opt/s263-rust`;
  - the row's image `docker.io/library/ubuntu:24.04` (`6277ab668b2a`);
  - podman 5.8.9, rootless, the lane's own config and storage;
  - for the gauntlet, lupin 0.1.59's linux archive (sha256 `12121909…`, read twice) and wolf-std
    at the STD-PIN `5b3a5994`, copied by value from s280's rig.
- **The rig** is copied by value from s280's `scripts3` (`GITHUB_ENV`, the pinned toolchains,
  lupin 0.1.59, the fresh-target refusal) and kept in `notes/s284-evidence/rig/`. I added an
  engine for `fork_lanes` (`rig/engine.sh`, `rig/engine-lib.sh`): H1's gauntlet container has no
  nested engine, so the fork witnesses had always skipped there.
  - It is the host's rootless podman, a remote client of `podman system service` on the take's
    own socket, its `/nix/store` binary mounted read-only.
  - Each nested container is pinned to the block with `taskset` in front of its `sh`, which stays
    process 1, and capped with `--memory 4g`.
  - Proved before use (`logs/enginetest.log`): `sh` is pid 1, `nproc` 6, `Cpus_allowed_list` 6-11,
    the bind mount readable, `memory.max` 4 GiB.

## Prediction

Committed before any change, `1b567cda` (`notes/s284-prediction.txt`). How it held:

- **Held: the suspect.** The second look in `back_from_poll` was the cause. A measurement-only
  build (`8e8ce00f`, trunk plus one raw `write(2)` line per event, never pushed) slipped in
  every run, quiet or loaded: the failed claim's second look read the release's `POLLING`.
- **Held: the stuck child and the lock.** Both held stalls read from `/proc` as predicted:
  - a parent with `Threads: 2`, its main thread in a futex wait on the hub's condvar and
    `wolf-signal` alive in `poll`;
  - one or two forked children alive, `Threads: 1`, with `block_signals`' mask (inside `child()`),
    each in a futex wait with value 2 on the hub's state mutex. The binary's DWARF places that
    mutex's futex word at `HUB+0x00` and the condvar at `HUB+0x58`.
  - The prediction said "exactly one" child. One stall had two, from consecutive forks.
- **Held in direction, wrong in mechanism: why CI.** I predicted the debug-profile runtime widens
  the window between the two looks. Both runtimes slip, though (quiet, 30 runs each: 424 slips
  with the debug runtime, 162 with the release runtime). The difference is downstream: a slip
  becomes a fork that claims the thread at work only with the debug runtime (23 claims and 15
  takes of the hub lock during a fork in 30 runs, against 0 and 0). The debug runtime's path from
  the slip back to its poll is slow enough for main's next hold to land in it.
- **Wrong: the reproducing condition.** I predicted it would need contention that stops the drain
  thread at an arbitrary instruction (a quota, hogs, a low priority). The quiet pinned row stalls
  (1 in 40 runs of trunk's debug-native binary), and contention suppresses the race: four hogs
  left 2 slips in 12 runs against 187 quiet, and nice, idle priority and hogs gave 0 claims.
  Neighbour containers starting and stopping, as CI's other fork rows do beside the drain row,
  keep the quiet rate of slips (168 in 12) and stalled 1 run in 12.
- **Held: the debug runtime stalls more.** The release runtime never stalled and never let a fork
  claim the thread at work.
- **Refined: the fix.** I predicted an `Err(POLLING)` arm and a compare at the loop's top. A strong
  compare-exchange never fails while the state reads POLLING, so the fix acts on the compare's
  own answer: `Err(HELD)` parks and claims again, anything else returns as before. The loop's top
  stays a store (R-s284-2): with the second look gone the thread reaches it only as LOOPING,
  which no hold claims. The reactor's `leave_kernel_wait` takes the same fix.
- **Held: the unit rows.** Both are red at the red commit by name, with every other
  `signal::tests` and `reactor::tests` row green there.

## Evidence index

Commits, base `ed19904c`. The measured shas are the same commits before the one rebase (from
`275f9904`): `signal.rs`, `reactor.rs` and `fork_lanes.rs` are byte-identical across it.

| commit | before the rebase | what |
|---|---|---|
| `96c7cefa` | `1b567cda` | the prediction |
| `da12f584` | `d3b35e80` | red row: `signal::tests::s284_a_release_in_the_claims_gap_never_lets_a_fork_claim_a_working_drain_thread` and its `sys::hook` stops |
| `748cf85c` | `6fdd9d7e` | red row: the reactor's twin (`reactor::tests::s284_…_reactor_thread`, `reactor::hook`); the red commit |
| `35ba9374` | `9d8bf2aa` | the hook's unused constant out |
| `4c4ddd98` | `978d9148` | rustfmt |
| `ee8f389c` | `2be96dc2` | the fix: `back_from_poll` acts on its claim's own answer |
| `36ff009e` | `bf843401` | the fix: `leave_kernel_wait`, the same |
| `b8bbeca6` | `2050a3f9` | spec/11 `[os.signal.hub]`, `[os.proc.fork]` |
| `bd1449b3` | `e8eaf3d4` | CHANGELOG |
| `f40f2402` | `e09dd371` | `fork_lanes`: the churn row (R-s284-1, R-s284-4) |
| (this note) | | the note and `notes/s284-evidence/` |

Measurement-only commits, never pushed. Their patches are kept as scripts here:
- `8e8ce00f` on `275f9904`: the first event lines (`rig/` history, `logs/events-8e8ce00f.txt`).
- `0d6fb1ff` on `275f9904` and `3062e28e` on `bf843401`: R-s284-3's one detector
  (`rig/measure2-patch.py`).

**1. The reproduction** (trunk `275f9904`, H1, block B, the row's own shape; `logs/sweep1.log`,
`repro2.log`, `repro3.log`, `proof1.log`, tallied by `rig/tally.py`):

| binary | condition | runs | stalled |
|---|---|---|---|
| debug runtime, native (CI's) | quiet, four CPUs | 78 | 2 |
| debug runtime, release tier (CI's) | quiet, four CPUs | 68 | 0 |
| debug runtime, native | beside four churning neighbour containers | 50 | 1 |
| debug runtime, release tier | beside four churning neighbour containers | 38 | 0 |
| debug runtime, native | beside two churning neighbour containers | 12 | 0 |
| debug runtime, native or release tier | hogs (2 or 4), fork loops, nice 19, SCHED_IDLE, `--cpus` 1 or 2, 1 ms quota periods | 100 | 0 |
| release runtime, native | quiet, hogs, a quota | 60 | 0 |

That is 3 stalls in 246 quiet or churning runs of CI's binaries, and 0 in 160 under hogs, quotas
and lowered priority or with the release runtime.

**2. The diagnosis.** Three stalls were read while they held, from the host through
`/proc/<pid>/{status,task/*/stat,wchan,syscall,maps}` at 40 s and again at 45 s
(`dumps/sweep1-r10-p0-…`, `repro3-r1-p9-…`, `proof1-r18-p0-…`). Each shows the same:
- **The parent:** `Threads: 2`. Main is in `futex_do_wait`, syscall 202 on `HUB+0x58` with op 0x89
  (`FUTEX_WAIT_BITSET` with `FUTEX_PRIVATE_FLAG`), a sequence value. `wolf-signal` is in
  `poll_schedule_timeout`, syscall 7, on two descriptors with no timeout.
- **One or two children:** alive, `Threads: 1`, `SigBlk: fffffffe7ffbfa27` (`block_signals`' set,
  so inside `child()`). Each is in `futex_do_wait` on `HUB+0x00` with value 2.
- **Nothing moves** between the two looks: utime, stime and the children are the same.
- **The address arithmetic** (`sweep1-r10-p0`): the binary's base is `0x5560dd6d7000` (`maps`), `nm`
  puts `HUB` at `0xe84c8`, and the children wait on `0x5560dd7bf4c8`, exactly `HUB`. The binary's
  own DWARF (`dwarfdump --name=Hub --show-children`) gives `OnceLock<Hub>.value` at 0x00,
  `Hub.state` at 0x00, `Hub.cv` at 0x58, and `Mutex.inner` (the futex word) at 0x00.

**3. The chain, measured** (`8e8ce00f`, `logs/events-8e8ce00f.txt`). One line per event:
- a slip: the second look read POLLING;
- a fork's claim while the thread had slipped;
- the loop's top overwriting HELD;
- the hub lock taken while a fork held the thread.

| binary, quiet, 30 runs | slips | claims at work | overwrites | lock in a fork |
|---|---|---|---|---|
| debug runtime | 424 | 23 (17 runs) | 26 | 15 |
| release runtime | 162 | 0 | 0 | 0 |

In 12 runs each:
- under hogs and lowered priority, the slips fell to 2–22 and the claims to 0;
- fork loops left 24 slips and 8 claims;
- beside churn or under a 1 ms quota, the slips stayed at 134–172 and the claims at 6–7.

A measured stall (`dumps/repro3-r7-p3-…`, forks4) ended with: slip, slip, a claim at work, a claim
at work, an overwrite, then two children stuck on the hub mutex.

**4. The event-count proof (R-s284-3)** (`logs/measure2.log`). The detector, applied by
`rig/measure2-patch.py` to `signal.rs`'s `mod sys`, the same way in both builds:
- (a) `reader_loop`, right after `back_from_poll();` returns: `AT_WORK = true`.
- (b) `reader_loop`'s top, right before `READER_STATE.store(READER_POLLING, …)`: `AT_WORK = false`.
- (c) `fork_hold`, on its successful POLLING→HELD compare: `FORKING = true`; with `AT_WORK` set it
  writes `S284 CLAIM_AT_WORK`.
- (d) `fork_release`'s first statement: `FORKING = false`.
- (e) `reader_loop`'s drain branch: `FORKING` read right after the hub lock is taken and again
  after `drain()`, before the guard drops; either one true writes `S284 LOCKED_IN_FORK`.
- (f) trunk only, at `back_from_poll`'s second look returning: `S284 SLIP`.

Each line is one `write(2)` to fd 2: no lock, no allocation.

| build (debug runtime, the row's quiet pinned shape) | runs | slips (f) | claims at work (c) | runs with one | hub lock in a fork (e) | runs with one | stalls |
|---|---|---|---|---|---|---|---|
| trunk + detector `0d6fb1ff` (on `275f9904`) | 50 | 764 | **41** | 26 | **46** | 25 | 0 |
| fix + detector `3062e28e` (on `bf843401`) | 50 | — (no second look) | **0** | 0 | **0** | 0 | 0 |

Interleaved, one run of each a round, 08:05–08:09Z (`logs/events-measure2.txt` has each run's
counts). Green medians were 1.43 s and 1.23 s.

**5. No regression (proof1)** (`logs/proof1.log`): 38 interleaved rounds, quiet and beside four
churning neighbours, CI's two binaries.

| binary | quiet | beside churn |
|---|---|---|
| fix `bf843401`, native | 0/76 (median 1.43 s) | 0/76 (3.47 s) |
| fix `bf843401`, release tier | 0/76 (1.24 s) | 0/76 (3.42 s) |
| trunk `275f9904`, native | 1/38 (1.87 s) | 0/38 (2.82 s) |
| trunk `275f9904`, release tier | 0/38 (1.24 s) | 0/38 (3.12 s) |

**6. The rows.**
- **Red** at `6fdd9d7e` (`logs/rows-red.log`), both by name:
  - hub row: "a fork's hold claimed the drain thread at work, its state reading 3 (2 = at work,
    3 = in its poll) …";
  - reactor row: "… the reactor thread at work, its state reading 3 …";
  - every other row green: `signal::tests` 30 of 31, `reactor::tests` 5 of 6.
- **Green** at `bf843401` (`logs/head-bf843401-B.log`): both pass, `signal::tests` 31 of 31,
  `reactor::tests` 6 of 6.
- **The churn row**, first run at `f40f2402` (the fix, rebased) through the engine
  (`logs/forklanes2-f40f2402.log`). All 16 `fork_lanes` rows passed in 109 s, the drain row's
  four lanes among them. The churn row ran 40 of 40 in 39.9 s beside 41 neighbour containers,
  none refused. An earlier take of the same trial skipped every build, because `libwolf_rt.a` was
  not built, and counted for nothing; the trial now builds it and fails on such a skip.

**7. The CI reds** (`ci/`): the `fork_lanes` section of each stalled job.

**8. The rig** (`rig/`, copied as run, with the host's name written H1): the build, run, repro,
rows and engine scripts and the plans; `rig/scripts3/` is s280's gauntlet rig as adapted.

**Rulings** (each implemented as ruled):
- **R-s284-1, the reproduction row: (A).** A `fork_lanes` row, the native tier, up to 40 runs beside
  four churning neighbour containers, red at the first stall. Options were (B) a gate step and (C)
  no new row.
- **R-s284-2: the loop's top stays a store.** With the second look gone, the thread reaches it only
  as LOOPING, which no hold claims.
- **R-s284-3: the event count is the proof.** One detector in both builds, measurement-only.
- **R-s284-4, the run count: (A).** 40 by default, sized from CI's rate; `WOLF_FORK_CHURN_RUNS`
  overrides it, and the plant take sets 400 and stops at its first stall. Options were (B) size it
  for H1 (about 230 runs, 12 minutes) and (C) keep 40 and name only the unit row in the plant.

## Done-when

- [x] Item 1, the reproduction: trunk's drain.lu stalls on H1 in the row's own shape, at the rates
  in the evidence index (CI's binary, the debug runtime; never the release runtime).
- [x] Item 2, the diagnosis: two stalls read while they held, from `/proc` on the host, the futex
  addresses tied to `HUB+0x00` (the state mutex) and `HUB+0x58` (the condvar) by the binary's DWARF;
  the chain measured by event (slip, claim at work, the hub lock taken during a fork).
- [x] Item 3, the fix, red first: the rows red at `6fdd9d7e`, green at `bf843401`. The
  event count is decisive: trunk 41 claims at work in 26 of 50 runs, the fix 0 in 50. proof1 shows
  no regression: the fix 0 stalls in 304 runs, trunk 1 in 152 beside it.
- [x] Item 4, the rows that stay: the two `s284_` unit rows, and the churn row in `fork_lanes`
  (R-s284-1 = A): 40 runs by default, sized from CI's rate, and `WOLF_FORK_CHURN_RUNS=400` in a
  local take that must see it red (R-s284-4 = A).
- [x] Item 5, the note: what s275 fixed, what this fixes, why CI saw it and a quiet box did not.
- [x] The rebase onto the trunk named for the final records, `ed19904c`.
- [ ] The gauntlet at the final head, `fork_lanes` through the engine, every `END` 0: the
  record and its `END` lines are in the PR.
- [ ] The plant `s284-plant-*`, red by name at the churn row: the record is in the PR.
- [ ] The PR, `Footprint: cross-cutting`, and the audit run once.
- README read, current: its only runtime mention is the 0.2.37 release note's list, which this
  change does not move.
