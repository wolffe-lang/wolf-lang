# s260 — the three wrong answers t06 replayed (wolf-lang#642, #682, #683)

Lane note. **Opus 5.5. Crawl 54.** Branch `s260` off wolf-lang trunk
`06cff722`, rebased onto `e2ed1607` (s259, #893) before the final head —
one conflict, the CHANGELOG's Unreleased section, resolved with s260
above s259; the prediction commit is `9f7e7b74` after the rebase.
Contract: `sprints/compiler/s260-the-three-wrong-answers.md`.
Toolchains: wolf 0.2.34 (`9da7bdcf`, linux x86-64 archive `88a9dd3d…`,
read twice) the control, lupin 0.1.55 (archive `1152abc0…`, read twice)
the reference.

## Prediction (committed before the first change)

Read from the source at `06cff722`, from the three witnesses in
`sprints/triage/t06-evidence/{642,682,683}.txt` and from the issues;
nothing built or run by this lane yet.

1. **Which rung each host takes for the `/dev/full` witness**
   (`fs_copy_chunk(fd, 1, 262144)`, `fd` the 17-byte `in.txt`, stdout
   redirected to `/dev/full`).
   - **linux**: the read/write rung, after three refusals of the
     pairing: `copy_file_range` (`EINVAL`, the sink is not a regular
     file), `sendfile` (`EINVAL`) and `splice` (`EINVAL`, neither side
     a pipe) — `kernel::refusal` takes all three — then `copy_loop`:
     one `read` of 17 bytes and a `write_all` that fails with
     `ENOSPC`. The 17 bytes are gone: the source's offset is 17 and
     nothing puts it back. The rung is remembered for the pair
     (`COPY_RUNG`), so a second call on the same pair goes straight
     to the loop.
   - **macOS and freebsd**: the read/write rung by name, the only one
     they have — but **macOS has no `/dev/full`**, so the witness as
     written cannot run there; a failing sink on macOS (and windows)
     has to be another one: a handle opened for reading only used as
     the destination fails its write on every host (`EBADF` on unix,
     an access refusal on windows) and, on linux, is refused by all
     three kernel rungs (`EBADF` is in the refusal list), so it
     reaches the read/write rung on every host.
   - **windows**: the read/write rung by name.
   - **The kernel rungs lose nothing**: a failed `copy_file_range`,
     `sendfile` or `splice` moved no byte, so the loss is the
     read/write rung's alone, and only after a read that answered
     bytes.
   - **The checked machine** has the same loss in its own buffer
     (`ubcheck.rs` `"fs_copy_chunk"`: a `read`, then `write_all`, and
     `io` with no seek), so it prints t06's line too.
   - **lupin 0.1.55**: not read (clean-room); predicted to lose the
     bytes as well (the clause says nothing about a failed write, and
     the checked machine and native agree), which would make lupin's
     half an issue to file against wolf-interp.

2. **Whether the spawn path's shell fallback is the library's or the
   runtime's: the library's.** `wolf_rt` has no `/bin/sh` anywhere on
   the spawn path. Every spawn with a non-empty descriptor map — and
   `os_spawn_job` always, and `os_spawn_with` with a non-empty set —
   installs a `pre_exec` hook on a `std::process::Command`, which makes
   the standard library fork and then call the C library's `execvp`
   after the hook; `execvp(3)` documents that when `execve` fails with
   `ENOEXEC` it runs the file under `/bin/sh`. The spawns with no hook
   (`os_spawn`, `os_spawn_with` with an empty set, `os_spawn_fds` with
   an empty map) take the standard library's `posix_spawnp` path,
   which on glibc does not fall back, so they answer `io` already
   (t06's "an empty-map first attempt took a different path"). So the
   rows are: hook paths run the file under `sh` (the witness's
   `unexpectedly-ran`, `junk`'s `cannot execute binary file` from
   bash), no-hook paths answer `io`; `os_exec` answers `io` (it calls
   `execve` itself, `exec.rs`). The checked machine's
   `spawn_mapped` is the same hook pattern and runs the file too (not
   measured by t06). lupin serves targets 0..2 with no `unsafe`, so no
   hook, so `io`. **`os_error()` after any of them is not ENOEXEC**:
   no os-family call clears or sets the word except `os_fork`, so the
   witness's `io` arm reads whatever the last fs call left (0 there).
   macOS: the same split is expected (its `execvp` documents the same
   fallback); whether its `posix_spawnp` falls back is not known from
   documentation alone and is measured on CI.

3. **How many parking sites the runtime has, and which the kill wakes
   through a failure arm today.** Eight a compiled program can reach,
   counted by the wait that parks, not by the call:
   1. the reactor's readiness wait (`reactor::submit` through
      `net::wait_raw`): `net_read`, `net_read_bytes`, `net_write`,
      `net_write_bytes`, `net_writev`, `net_writev_head`,
      `net_accept`, `net_wait`;
   2. the channel cell wait (`chan.rs` `Cell::wait`): `ch.send`,
      `ch.recv`, `select`, `for v in ch`;
   3. the `when` acquisition (`when.rs`);
   4. the signal hub's waits: `os_signal_wait`, `os_signal_wait_fd`,
      `os_signal_wait_fds`;
   5. the scope join at a `scope { }` block's exit
      (`__wolf_rt_scope_join_free`);
   6. the proc join and the kill's own wait (`proc.rs` `wait_exited`:
      `w.join()`, `w.kill()`);
   7. `time_sleep_ms` — a host sleep (`host_wait` around
      `std::thread::sleep`), not a runtime park;
   8. `os_wait`, `os_wait_status`, `os_wait_status_core` — the host's
      `waitpid` with the child table held.

   The kill (`proc::kill_mark` → `ScopeInner::kill` → the parked
   registry's `wake_all`) **wakes 1–4**. **Through a failure arm: 1
   and 4** — the reactor completes the claimed wait `Cancelled`,
   `kill_teardown_check` is a no-op on a compiled frame
   (`IN_RUST_TASK` is false under `suppress_kill_unwind`), and
   `wait_raw` answers `io`, so the program's `else` runs (the
   witness); the signal waits answer `io` the same way (their kill
   look is a 100 ms / 5 ms poll inside a killable scope). **2 and 3**
   answer status 2, and the lowered teardown branch
   (`kill_teardown_branch`, `__wolf_rt_task_killed`) returns out of
   the parking frame before its `else`: right when that frame is the
   proc's entry, wrong one frame up — the caller resumes user code (the
   "row-less intermediate frame" gap `conc_abi.rs` records). **5–8 are
   not woken by the kill**: 5 returns when the (killed) children have
   finished, 6 when the other proc exits, 7 after its whole duration
   and 8 when the child ends — and each then returns into user code.
   Beyond the eight, calls that block in the kernel and that no
   runtime wake reaches: `net_connect`/`net_connect_unix` (the dial),
   reads and writes of pipes and terminals through the fs table,
   `read_line`.

   The **mechanism this lane needs** (to be designed, not measured
   yet): compiled frames have no unwind tables
   (`[abi.native.nounwind]`) and the contained-trap path parks the
   thread for good (`park_after_contained_trap`) — one thread per
   kill if reused here, which a server that kills a proc per timed-out
   request (lobo, #678's workaround) cannot afford. The prediction is
   that the leave needs the runtime to discard the task's compiled
   frames and return to the task's entry in the runtime — a saved
   register context at the entry and a restore at the park's shim
   exit — and that the frames are safe to discard there because the
   proc's regions are its ledger's (bulk-freed by the reaper, step 3)
   and every runtime structure the park touched has been put back
   before the shim returns.

4. **The differ's moved rows.** Against 0.2.34, no existing corpus row
   moves (no row today reaches a failed copy write, a `#!`-less spawn
   or a killed park); the rows that move are the ones this lane adds.
   The copy row moves on native, release and checked (0.2.34 loses the
   bytes; the head puts them back) and is predicted to diverge from
   lupin 0.1.55 (lupin lags, item 1). The spawn and kill witnesses live
   in driver lane tests (they spawn children or need a container), not
   in the differ's corpus.

## How the rows were measured

On kasumi (linux x86-64, CachyOS, podman 6.1.3): each program built on
the host with the toolchain under test (`taskset -c 4-7`), every run —
native, release, `wolf conform-run --json --checked`, and lupin 0.1.55's
`conform-run` — inside an `archlinux:base-devel` container on cpus 0–3,
`ulimit -c 0`, `SigBlk`/`SigIgn` printed first (all zero). The control
is the wolf 0.2.34 release archive (`9da7bdc`), the head the lane's own
`cargo xtask dist` at `37c3cc37` — the pre-rebase twin of `f27ebf3b`
(the rebase took only s259's compiler-side commits, none of which touch
the runtime, the checked machine's os and fs tiers or these programs).
The logs and the scripts are under `docs/s260-evidence/` (`v034-*.log`
the control, `head-rows.log` the head, `strace.log` the rungs, `probes/`
the programs that are not fixtures).

## 1. A failed copy write (#642, `[os.fs.copy]`)

**What the clause said.** One transfer of at most `max` bytes, both
offsets advancing, `io` "from either side" — and nothing about the
bytes a read had already taken when the write fails. **What the
runtime did.** linux tries `copy_file_range`, `sendfile` and `splice`,
each refused by the pairing of a regular file and `/dev/full`
(`EINVAL`), then the read/write rung: one `read` of 17 bytes, one
`write` that fails `ENOSPC`, and `io` with the source at 17
(`strace.log`, both builds). The checked machine lost them the same way
in its own buffer; lupin 0.1.55 too.

**The answer (R-s260-1, the contract's recommendation, implemented).**
The read/write rung writes as far as the sink takes the bytes (a counted
`write` loop in place of `write_all`), then puts the source back by the
shortfall — `lseek(src, -(read - written), SEEK_CUR)` — so after any
`io` the source stands exactly past what the sink took; the count taken
goes through the call's out word beside the `io` (a program reads it as
the offset's advance; nothing in the language reads an out word on the
error arm today). The kernel rungs fail before a byte moves, so they
needed nothing. A source that cannot seek (a pipe, a terminal,
descriptor 0 after `read_line` read ahead) keeps its offset and the
shortfall is lost, as it is for a read then a failed write by hand. The
side is named by rule, not by a new tag: the failing call's number is in
`os_error()`, and a caller that must say which side failed retries the
read — after a write's failure the bytes are there again, and the write
after them fails as the copy's did. That is what boreutils' `cat`
already does (`src/cat.lu` at `f138736a`, the fast path `6b5105c7`,
bu19's adoption: on the copy's `io` the rest of the operand goes through
`copy_through`, the read-then-write loop whose own failure names its
side), so its expectations were read and not changed. Until this lane
that fallback met the END of the file — the copy had taken the bytes —
and failed at nothing, and `cat` reached its last line, "a copy that
failed and a fallback that did not is the write's failure", by
inference; now the fallback reads the bytes again and its own write
fails, so `cat: write error: No space left on device` comes from the
side that failed, and the inference is reached only on a source that
cannot seek. The checked machine mirrors it (its handed input buffer
moves its own position back).

| row | 0.2.34 native / release | 0.2.34 checked | head, all three | lupin 0.1.55 |
|---|---|---|---|---|
| t06's witness, fd 1 on `/dev/full` | `offset_after=17 left_to_read=0` | fd 1 is the capture: `copy=17` | native, release: `offset_after=0 left_to_read=17` (checked: the capture) | `copy=17` (its own buffer) |
| `/dev/full` by handle | `offset_after=17` | `offset_after=17` | `offset_after=0 left_to_read=17`, `os_error=28` | `unsupported` by name (`[os.fs.path.domain]`) |
| a sink opened for reading only | `offset_after=17` | `offset_after=17` | `offset_after=0`, the retry moves 17 | `offset_after=17` |
| a pipe with one page free (crate test, linux) | — | — | `io`, `out` = the page taken, offset = the page | — |

The corpus rows: `fs/copy_chunk_failed_write.lu` (the read-only sink,
every host: `EBADF` on unix refuses every kernel rung, an access refusal
on windows) and `fs/copy_chunk_dev_full.lu` (`cat f > /dev/full` with the
offset read back; a host with no `/dev/full`, macOS and windows, takes the
read-only sink in its `else`, the same line). `copy_write_lanes.rs` holds
the checked machine, native and release to both, lupin 0.1.55 pinned by
version and record commit (`e642f26`) to its measured loss, and t06's
own program with descriptor 1 on `/dev/full` on linux. The crate tests
(`wolf_rt::fs`): the read-only sink, `/dev/full` twice (the remembered
rung goes straight to the loop and still keeps the bytes), and a partial
write into a non-blocking pipe with one page free.

## 2. A file without `#!` (#682, `[os.proc.exec]`)

**What the clause said.** "A file without the `#!` line the kernel
needs is `io` (`ENOEXEC`): no machine re-runs it under `/bin/sh`."
**What the runtime did** (`v034-spawn2.log`, `v034-rows.log`):

| call | 0.2.34 native, release, checked | head, all three | lupin 0.1.55 |
|---|---|---|---|
| `os_spawn_fds(.., [1, 1])` | RAN (`junk`: bash's `cannot execute binary file`, 126) | `io`, `os_error=8` | `io`, `os_error=0` |
| `os_spawn_job(.., [1, 1], ..)` | RAN | `io`, 8 | `io`, 0 |
| `os_spawn_fds(.., [])`, `os_spawn`, `os_spawn_with(.., [])` | `io`, `os_error=0` | `io`, 8 | `io`, 0 |
| `os_spawn` once the program ignores `BROKEN_PIPE` | RAN | `io`, 8 | `unsupported` (`os_signal_ignore`, no `unsafe`) |
| `os_exec` | `io`, 0 | `io`, 8 | RAN (lupin replaced itself with `/bin/sh noshebang`; no record) |
| the `#!/bin/sh` control, every call | runs, status 7 | runs, status 7 | runs |

**Whose fallback it was: the C library's.** No `/bin/sh` appears
anywhere in `wolf_rt`'s spawn path. A spawn with a `pre_exec` hook is
forked by the standard library and exec'd with `execvp`, and
`execvp(3)` documents the fallback: when `execve` answers `ENOEXEC` it
runs the file under the shell. A spawn with no hook takes the standard
library's `posix_spawn` path, which on glibc does not fall back — and on
macOS does: the macos-latest leg of full run 39906953002 measured
`./noshebang spawn_fds[]: RAN status 0` and `./noshebang spawn: RAN
status 0` (`./junk`: `RAN status 126`, the shell's refusal) beside `io`
with 8 for both hooked rows. The
hooked spawns are more than the issue's: every non-empty map, every job
spawn, a non-empty inherit set, and — found while building the fix —
ANY spawn of a process that ignores `SIGPIPE` or holds a fault ignored,
because `keep_pipe_ignore` installs a hook to hand the ignore on
(`[os.signal.disp]`), so `os_spawn(["./noshebang"])` ran too.

**The fix.** A hooked spawn's last hook is the exec, made by the runtime
(`os::ExecPlan`, `exec_last`): the candidates (the name as written when
it holds `/`, else each directory of the child's `PATH` in order,
`/usr/bin:/bin` when it has none, as `os_exec` reads it), `argv` and the
child's whole environment are built in the parent; the child walks the
candidates with `execve` alone — `ENOENT`/`ENOTDIR` try the next,
`EACCES` is remembered and the search goes on, anything else (`ENOEXEC`
among them) is the spawn's error, which the standard library carries
back. The documented contract of `pre_exec` ("run just before the exec
function is invoked"; an `Err` is the spawn's error) is what this rests
on. A spawn with no hook keeps `posix_spawn`'s fast path on glibc only,
where it was measured not to fall back (`os::EXEC_ALWAYS_OURS`): off
glibc — macOS measured, freebsd and musl not — every spawn ends with the
plan. The spawn
family and `os_exec` now keep the program step's host number in
`os_error()` — cleared on entry, the refusal's number left behind, 0 for
a refusal decided before the host (the shape, a forged source) — which
is what lets a shell say "not an executable format" without reading the
file's first bytes (R-s260-3 on the spec sentence this contradicts).
Downstream: pelt's `exec` already runs a `#!`-less file itself (XCU
2.9.1.4, `notes/sh05-the-exec.md`), but its plain-command path never saw
one — the spawn ran it under `/bin/sh` — and now meets `io` with
`ENOEXEC` there, the shape its `exec` path already handles. The checked
machine carries the same plan as a hand mirror (wolf_mem and wolf_rt may
not see each other), its overlay applied to the environment.
`spawn_noexec_lanes.rs` (unix) runs the three fixtures on native,
release (and native under `taskset -c 0-3`), the checked machine and
lupin; the crate tests cover every spawn with the marker the file would
write, the shim's `ENOEXEC`, the refusals decided first, and the argv
and environment a hooked exec hands on.

## 3. A killed proc at its park (#683, `[conc.proc.kill]` step 1)

**What the clause said.** The proc's task tree is cancelled "without
running any further user code". **How the parked read was woken.** Not
by the deadline and not by a descriptor's close: by the kill itself.
`kill_mark` sets the scope's `killed` flag and `ScopeInner::cancel`
calls the parked registry's `wake_all`; the reactor's `wait_on` sees the
flag, claims its own wait and completes it `Cancelled`;
`kill_teardown_check` unwinds a Rust task but is a no-op on a compiled
frame (`IN_RUST_TASK` is false under `suppress_kill_unwind`), and
`wait_raw` answered `io` — so the program's `else` ran, then its next
statements, and the proc ended only when it returned. The channel and
`when` waits answer status 2 to the lowered teardown branch, which
returns ONE frame: right in the proc's entry frame, wrong one frame up.

**The leave (R-s260-4).** Compiled frames have no unwind tables
(`[abi.native.nounwind]`), and the contained-trap path parks the thread
for good — one thread per kill, which a server that kills a proc per
timed-out request (lobo's #678 workaround) could not afford. So a
compiled task is now entered through `task::leave::run_compiled`, whose
`enter` (a naked function, a few instructions on x86-64 System V,
windows x64 and aarch64) saves the callee-saved registers, the stack
pointer at the entry and the floating-point control words, then calls
the entry. Every runtime call that parks ends with `leave_if_killed`:
when the task's scope is killed, `leave` restores that state and
returns from `enter` with `CANCEL_TAG`, discarding every frame between —
the task's compiled frames and the shim's own, whose locals were dropped
before the call. The worker thread goes back to the pool; the reaper
sees the tree quiesce, bulk-frees the ledger (`[mem.region.proc]`), and
the join answers `killed`. What the discarded frames held is answered
in `leave.rs`'s doc: a scope the task opened and had not joined is
joined by the runtime before the task is retired (its children are in
the killed tree and leave at their parks), so the reaper still frees
the regions after the whole tree is at rest; a `when` set held stays
held (step 1's stated consequence) and the thread's holder token and
depth are renewed; channel references, files and sockets are not
released (their release is code the program would have run). A CET
shadow stack, where a host enables one, is unwound by the same count
(`rdsspq` reads 0 and nothing happens where none is). `time_sleep_ms`
in a task became a sleep the kill ends (a condvar the parked registry
nudges; a polite cancel still does not end it — a sleep is not a
cancellation point). The lowered poll `__wolf_rt_task_killed` leaves the
same way, so its branch is reached only off a compiled task.

**Every parking site** (`v034-rows*.log`, `head-rows.log`; the fixtures
are `fixtures/kill_park/`, the gate `kill_park_lanes.rs`):

| site | woken by the kill at 0.2.34 | 0.2.34 native / release | head native / release | checked, lupin (both pins) |
|---|---|---|---|---|
| `net_read` (the witness) | yes, `io` | `else` ran, the pipe written | nothing runs, `killed` | nothing runs |
| `net_read` one frame down | yes, `io` | `else` and the caller ran | nothing runs | nothing runs |
| `net_accept` | yes, `io` | `else` and after ran | nothing runs | nothing runs |
| `net_write` (16 MiB, peer not reading) | yes, `io` | `else` and after ran | nothing runs | nothing runs |
| `ch.recv()` in the entry frame | yes, status 2 | right (the teardown branch) | nothing runs | nothing runs |
| `ch.recv()` one frame down | yes, status 2 | the caller ran on | nothing runs | nothing runs |
| `select` one frame down | yes, status 2 | the caller ran on | nothing runs | nothing runs |
| `when` acquisition | yes, status 2 | right (entry frame) | nothing runs | nothing runs |
| `os_signal_wait` | yes (a 100 ms look), `io` | `else` and after ran | nothing runs | nothing runs |
| `os_signal_wait_fds` | yes, `io` | `else` and after ran | nothing runs | nothing runs |
| `o.join()` | yes (its monitor receive), `cancelled` | `else` and after ran | nothing runs | nothing runs |
| `scope { }`'s join | when the children end | the code after the block ran | nothing runs | nothing runs |
| `time_sleep_ms(3000)` | no (a host sleep) | ran after the sleep; kill `slow` | the kill ends it: nothing runs, `prompt` | nothing runs |
| `os_wait` (child sleeps 2 s) | no (the host's `waitpid`) | ran after; kill `slow` | leaves when the child ends: nothing runs, kill `slow` (R-s260-2) | the machine serializes the host wait: the proc ends `normal` before the kill |
| `w.kill()`'s own wait | when the target ends | — | leaves after it | — |
| the dial, pipe and terminal reads, `read_line` | no (the kernel) | — | leave when the host returns, if the call is one of the shims above; fs reads are not (R-s260-2) | — |

A task that is computing when the kill lands runs on until its next
such call — the clause's compute-bound half, the checkpoint story
`conc_abi.rs` records. The `when` row one frame down is not in the
table: a `Mutex[int]` parameter is refused by the checker (E0401, "a
`when` operand must be `Mutex[_]`"), filed as found.

## 4. The gauntlet, the plants, the differ

**`cargo xtask ci`** on kasumi under `taskset -c 4-7` at `7111883b` (the
rebased head with the rows' snapshots; kasumi `~/lanes/s260/ev/gauntlet3.log`,
its step and summary lines `s260-evidence/gauntlet3-summary.log`), the
container rows required (`WOLF_JOBWAIT_REQUIRE_CONTAINER=1` and its
siblings), `LUPIN` set with `WOLF_PAIRING_REQUIRE_SIBLING=1`: exit 0,
04:17:38–04:39:43 UTC, "all steps green" — `fmt`, `fmt-fixpoint`,
`clippy`, `rt-none`, `test` (329 test binaries, 0 failed), `fuzz-check`,
`deps-check`, `corpus`, `abi-check`, `debug-check`, `midend-rate`,
`bench-gates` (the ir-volume table: 694 recorded, 716 measured, 22 new,
0 regressed), `spec-extract`, `conformance` (693 anchors), `print-gate`,
`diag-catalog`, `doc-catalog`, `fmt-lu`, `fmt-fuzz`, `audit-surface`,
`dist-smoke`, `differ-self` (1,219 files, 0 divergences, 21 in the
conservatism ledger, 3 forward pins), `lane-coverage` (floors held,
221/242/242/256/207; 385 static rejections). The SKIP census
(`RUST_TEST_NOCAPTURE=1`): seven lines, all the host's — 4 × `lldb not
installed`, 2 × `core (on): this host's core_pattern pipes cores to a
program`, 1 × `the clang drift witness runs on macOS/aarch64 hosts
only`; none in this lane's tests (lupin was required and ran). The
first run, before the rebase, stopped at `fmt` (the three lane tests),
the second at `test` on the two copy rows' missing token-stream
snapshots; each `.snap.new` was read before it was moved into place
(the lexer and declaration snapshots, and two `lowers` lines in the
lowering ledger). The commit after `7111883b` makes `kill_park_lanes`
unix-only, inert on linux.

**The full run's reds, folded in.** Full run 39906953002 (at `f1b8f769`)
went red on three legs the linux gauntlet cannot see, each fixed by its
own commit:
- `cross-check (freebsd x86-64, tier-2)`: E0433 at every new
  `crate::task::leave_if_killed()` site, because `wolf_rt/src/lib.rs`
  configures `task` only on linux, macOS and windows. The leave and the
  kill-ended sleep now go through crate-level wrappers gated as `task` is
  (a no-op leave and the host sleep where it is configured out),
  `3d0bb03c`.
- `build + test (windows-latest)`: the failed-write crate test expected
  the coarsened `io` from the shim, but the shim answers the host kind and
  windows refuses a write to a read-only handle as an access refusal,
  `denied` (2), which lowering coarsens to `io` for the program (the
  prediction's own "an access refusal on windows"); the test reads the
  raw code per host, `27e4312a`. And `copy_write_lanes`' strict-skip
  helper was dead code off linux, `d0456901`. The win64 `enter`/`leave`
  assembled and passed there (`enter_answers_the_entrys_value`,
  `a_leave_discards_the_frames_below_the_entry`); the leg stops at its
  first red binary, so `wolf_rt`'s integration tests and the crates after
  it did not run.
- `build + test (macos-latest)`: the aarch64 `enter`/`leave` passed all
  fifteen `kill_park_lanes` rows and `copy_write_lanes` passed;
  `spawn_noexec_lanes` failed on the hookless rows, `posix_spawn`'s
  fallback (§2), and stopped the leg there (the driver's later binaries,
  `wolf_mem`, `wolf_rt` and the rest did not run, the two crate tests of
  the leave among them). Every spawn off glibc now ends
  with the runtime's own exec, `a5a05581`, and the checked machine's,
  `2c529ab7`.

Before the push, in a container on kasumi (each target added to the
container's toolchain only): `cargo check --workspace --target
x86_64-unknown-freebsd` exit 0 at `2c529ab7`; `cargo clippy -p wolf_rt -p
wolf_mem --lib --tests -- -D warnings` for `x86_64-unknown-freebsd`,
`aarch64-apple-darwin` and `x86_64-pc-windows-gnu` at the head and at
trunk `e2ed1607`, the (file, message) sets identical on each target —
apple clean on both (blake3's C objects stubbed, since a check links
nothing), the freebsd and windows-gnu lints all trunk's; msvc is CI's alone (no
`ml64.exe` here) (`docs/s260-evidence/cross-clippy.log`).

**The anchors.** `cargo xtask spec-extract` regenerated at the head:
`spec/anchors.json` byte-identical to trunk `e2ed1607`'s, 693 keys on
both sides, nothing in either difference — the `[os.fs.copy]` sentence
added no tag.

**The plants** (`workflow_dispatch -f jobs=lint-test`, `tier partial`;
each branch is `7111883b` plus one commit that reverts the item's
behaviour in the smallest lint-clean edit, every test left in place):

| item | branch, commit | what the plant does | run, job | red on |
|---|---|---|---|---|
| #642 | `s260-plant-642` `57526c32` | the read/write rung never seeks back; the checked machine never puts back | 39899683181, `lint + test + corpus (linux x86-64)` 120238170633 | `copy_write_lanes`: 0 passed, 3 failed — the two rows on every machine and t06's witness |
| #682 | `s260-plant-682` `550d484d` | the exec hook answers `Ok`, so the standard library's `execvp` runs | 39899685124, job 120238180662 | `spawn_noexec_lanes`: 1 passed, 2 failed — the spawn rows and the hooked plain spawn (`os_exec`, which the plant does not touch, stays green) |
| #683 | `s260-plant-683` `699a2925` | `LEAVES = false` on x86-64 System V: the park returns into the program | 39899686795, job 120238185898 | `kill_park_lanes`: 3 passed, 12 failed — exactly the twelve sites wrong at 0.2.34; the entry-frame receive, the `when` and the row count stay green |

Cargo stops at the first red test binary, so each plant was also run on
kasumi at the same head with `--no-fail-fast` (`s260-evidence/plant-local.log`):
#642 reddens its three `wolf_rt::fs` crate tests (the read-only sink,
`/dev/full`, the partial write) beside the lane test's three; #682 its
two `wolf_rt::os` crate tests (every spawn, the shim's `ENOEXEC`) beside
the lane test's two; #683 reddens the lane rows only — the leave's own
crate tests return early when the host does not leave, and nothing in
them parks. At the head every one of these is green.

**4.1 The differ** (`s260-evidence/differ.log`, the moved lines
`differ-moved.log`). Every wolf in one configuration, the corpus's own
(no `std/` beside the binary; 0.2.34's archive files copied without
it): head = `target/release/wolf` of this lane's dist at `7111883b`,
before = the base `e2ed1607`'s own dist, control = 0.2.34. `cargo xtask
differ <wolf> <lupin 0.1.55> --triage --<tier>`, 1,219 files, 1,157
entries:

| A | tier | agreements | completeness | SOUNDNESS | unsupported (forward) | hard | A ran / B ran / both |
|---|---|---|---|---|---|---|---|
| head `7111883b` | checked | 812 | 259 | 9 | 104 (3) | 30 | 738 / 931 / 708 |
| before `e2ed1607` | checked | 813 | 259 | 9 | 104 (3) | 29 | 738 / 931 / 708 |
| 0.2.34 `9da7bdc` | checked | 816 | 265 | 0 | 107 (3) | 21 | 719 / 931 / 694 |
| head `7111883b` | native | 796 | 259 | 23 | 106 (3) | 44 | 742 / 931 / 706 |
| before `e2ed1607` | native | 797 | 259 | 23 | 106 (3) | 43 | 742 / 931 / 706 |
| 0.2.34 `9da7bdc` | native | 794 | 265 | 14 | 116 (3) | 34 | 716 / 931 / 685 |

**Against the base**, the ledgers differ by exactly one line on each
tier: `fs/copy_chunk_failed_write.lu` leaves the agreements (at the base
wolf and lupin both lose the bytes, `exit(1)` with the same stdout) and
enters as a hard `Verdict` divergence (`Exit(0) vs Exit(1)`): lupin 0.1.55
lags the clause — wolf-interp's to mirror, pinned in `copy_write_lanes.rs`.
`fs/copy_chunk_dev_full.lu` does not enter the ledger (lupin declines
`/dev/full` by name, `unsupported` on both sides of the comparison). On
native, `conc/atomic_ub_uaf`, `memory/unsafe_ub_uaf` and
`memory/volatile_ub_uaf` print a different exit status in each run
inside the same SOUNDNESS class: undefined behaviour, not a move. The
spawn and kill witnesses are driver fixtures, not corpus files, so the
differ does not see them. The plain pass head against before reports 0
divergences (the static rungs; no row of this lane's is static).
**Against 0.2.34** the other 24 lines (checked; 26 on native with the UB
rows) are trunk's since the tag — s252's region-scalar rows, s254's
`region_os_cwd_return`, s255's outer-write and `List` rows, s256's
`read_param_indexed_caller`, s253's `core_limit_rows`, s258's
`row_operand_member`/`_method`, s259's `ctor_arg_unresolved` — each in
the same class at the base; none of them is this lane's. The plain pass
head against 0.2.34: 21 static divergences, trunk's.

## 5. Rulings (asked, each with a recommendation; implemented where reversible)

- **R-s260-1 — the failed write on the read/write rung** (#642). Asked:
  what does `fs_copy_chunk` answer when its write fails after its read
  answered bytes? Recommended and implemented (the contract's): write
  what the sink takes, answer `io` with the write's number in
  `os_error()`, put the source back by the shortfall so after any `io` it
  stands exactly past what was written, the count beside the `io` in the
  out word; a source that cannot seek loses the shortfall, stated; the
  side named by rule (retry the read). The clause gained the sentence by
  its own commit. Not chosen: the short-write convention (answer the
  partial count, fail on the next call) — equally lossless, but it splits
  one failure across two calls and changes what a caller that loops to
  `eof` sees on a disk that fills mid-chunk.
- **R-s260-2 — the parks no kill can end without user code** (#683).
  Asked: what of a park the runtime cannot wake? Measured: `os_wait`,
  `os_wait_status` and `os_wait_status_core` hold the child table in the
  host's `waitpid`; the dials (`net_connect`, `net_connect_unix`) block in
  the kernel; reads and writes of pipes and terminals through the fs
  table, and `read_line`, block in the kernel. None of them runs user code
  after the kill at the head where it is a shim this lane wrapped (the
  child waits and the dials leave when the host returns), but the kill
  itself waits that long (`os_wait`: `kill slow`), and the fs calls are
  not wrapped (they are no park of the runtime's, and every fs call would
  pay the look). Recommended, not implemented here (it is a design of
  its own): the child wait through the signal hub's `SIGCHLD` or a pidfd
  in the reactor, so a kill can claim it as it claims a readiness wait;
  the dial non-blocking under the reactor; and the fs table's pipe and
  terminal reads through the readiness set. Issue to file.
- **R-s260-3 — `os_error()` for the os family** (#682). Asked: the
  contract asks for `ENOEXEC` in `os_error()` from both calls, and
  `[os.fs.error]` says "the os family does not [set it] at this pin" —
  a sentence `os_fork` already contradicts (s247). Recommended and
  implemented for the runtime and the checked machine: the spawn family
  and `os_exec` keep the program step's number (cleared on entry, 0 for a
  refusal decided before the host). The spec sentence is not moved here
  (the contract moves `[os.fs.copy]` only); recommended: amend it to
  name the os calls that set the word (`os_fork`, the spawns, `os_exec`).
  Issue to file.
- **R-s260-4 — how a killed compiled task leaves** (#683). Asked: the
  frames cannot be unwound; how does the park leave them? Recommended and
  implemented: discard them by restoring the state saved at the task's
  entry (`leave.rs`), the thread back in the pool, a scope the frames
  left open joined by the runtime, a `when` set they held left held. Not
  chosen: the contained trap's answer (park the thread for good), which
  costs a thread and its stack per kill. Reversible (one module and the
  wrapped shims).

## 6. The prediction, scored

1. **Right.** linux: the three kernel refusals, then the read/write rung
   (`strace.log`; the crate test asserts the remembered rung is the loop);
   macOS has no `/dev/full` and a read-only sink is the failing write every
   host has (linux refuses it on every kernel rung, `EBADF`); the checked
   machine and lupin 0.1.55 lost the bytes too.
2. **Right, and short by one shape.** The fallback was the C library's
   `execvp`, reached only by hooked spawns; the hookless spawns and
   `os_exec` answered `io`; `os_error()` was 0 after every one; the checked
   machine ran the file too; lupin answered `io` for every spawn. Missed:
   `os_spawn` is hooked whenever the process ignores `SIGPIPE` or holds a
   fault ignored (`keep_pipe_ignore`), so the "no hook" list was wrong for
   such a program (found while building the fix, measured RAN at 0.2.34).
   Not predicted: lupin's `os_exec` RUNS the file (it replaces itself
   through the same `execvp`). Left open and measured wrong-way: macOS's
   `posix_spawn` falls back too (full run 39906953002), so on macOS the
   hookless rows RAN at the first fix and the exec is now the runtime's
   on every spawn off glibc.
3. **Right on the count and the mechanism, wrong on one site.** Eight
   parks, the kill's wake through the parked registry, the failure arm at
   the reactor's and the signal hub's waits, the one-frame gap at the
   channel waits, the host sleep and the host `waitpid` not woken, the
   scope join returning into the code after the block — all as measured.
   Wrong: `w.join()` IS woken by the kill — it is a monitor-channel
   receive — and answered `cancelled` to the joiner's `else` (a failure
   arm, so three sites, not two). The mechanism built is the one
   predicted (a context saved at the entry, restored at the shim's exit).
4. **Right, with one thing not said.** No existing corpus row moved; the
   failed-write row moved on every compiled tier and the checked machine
   (0.2.34 loses the bytes, the head puts them back) and diverges from
   lupin 0.1.55, which lags — the one line the ledger gains against the
   base, on each tier. Not said: the `/dev/full` row stays out of the
   ledger, because lupin declines the device by name.

## 7. Issues

**To close** (on the merge): wolf-lang#642, #682, #683.

**To file**, each with its witness in this lane's rows:

- **wolf-interp — `fs_copy_chunk`'s failed write loses the bytes**
  (lupin 0.1.55): the `[os.fs.copy]` sentence to mirror;
  `fs/copy_chunk_failed_write.lu` is the one hard divergence this lane
  adds to the pairing ledger, pinned by version and commit in
  `copy_write_lanes.rs`.
- **wolf-interp — `os_exec` runs a `#!`-less file under `/bin/sh`, and
  `os_error()` stays 0 after a spawn or exec refusal** (lupin 0.1.55,
  `fixtures/spawn_noexec/`): `[os.proc.exec]`'s "no machine re-runs it"
  and the `ENOEXEC` number; pinned in `spawn_noexec_lanes.rs`.
- **wolf-lang — `[os.fs.error]`'s "the os family does not at this pin"
  is stale** (R-s260-3): `os_fork` since s247, the spawn family and
  `os_exec` since s260 keep the word.
- **wolf-lang — the parks no kill can end** (R-s260-2): `os_wait`,
  `os_wait_status`, `os_wait_status_core` (the host's `waitpid` under the
  child table), the dials, the fs table's pipe and terminal reads and
  `read_line`; a proc's kill waits for the host (`fixtures/kill_park/
  os_wait.lu`, `kill slow`).
- **wolf-lang — a `when` over `Mutex[int]` parameters is E0401**:
  `fn inner(m: Mutex[int], n: Mutex[int]) -> int { when (m, n) { m += 1 }
  7 }` answers "this is `Mutex[int]`, but a `when` operand must be
  `Mutex[_]`", and the call `inner(m, n)` on `Mutex(0)`s "`Mutex[{integer}]`
  … needs `Mutex[int]`" (wolf 0.2.34, kasumi; found writing the `when`
  row one frame down, which is why that row is not in the table).
- **pelt — a `#!`-less file on the plain-command path**: `os_spawn_fds`
  answers `io` with `ENOEXEC` now, where it ran the file under `/bin/sh`;
  the plain command should run it as pelt's `exec` already does
  (XCU 2.9.1.4).

**Not filed** (postures, recorded): the checked machine and lupin
serialize a host wait with the program, so a proc parked in `os_wait`
ends `normal` before a kill reaches it; boreutils' `cat` keeps its
"failed copy, clean fallback" inference, now reached only on a source
that cannot seek.

**README** read against the pass: nothing it states moved (the windows
task-layer sentence still holds); current.
