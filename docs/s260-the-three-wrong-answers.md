# s260 — the three wrong answers t06 replayed (wolf-lang#642, #682, #683)

Lane note. **Opus 5.5. Crawl 54.** Branch `s260` off wolf-lang trunk
`06cff722`. Contract: `sprints/compiler/s260-the-three-wrong-answers.md`.
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
