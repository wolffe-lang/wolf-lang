# s290 prediction: a new session

Committed on branch `s290` before any change to the spec, the runtime, the compiler or a row. Base:
wolf-lang trunk `bca5dbc5` (0.2.38), paired with lupin 0.1.60 (`crates/wolf_driver/PAIRING`).
This file is the first phase of the lane: the shape, measured where a probe was cheap, for a ruling
before any code. The evidence is `notes/s290-evidence/probe/` (three C probes and one Rust probe,
their sources and their outputs, each ending in an `END exit=` line).

## 1. What a program cannot do at the base

No builtin calls `setsid(2)` and none reads a session id. The surface around it is: `os_set_pgid`
moves the caller into a group and is `denied` for a session leader (`[os.term.group]`);
`os_spawn_job`'s `group` keeps a child in this process's group (-1), makes it lead a new group (0)
or joins one (> 0), and its `tty` hands a group a terminal's foreground (`[os.proc.job]`);
`os_fork`'s child keeps its parent's session and controlling terminal "until the child changes
them" (`[os.proc.fork]`), which no call can do. `[os.signal.group.note]` left "a session of its own
(`setsid`, no customer yet)" out on purpose.

The customer is burrow's server, `burrowd`. Each connection is a chain of forks (`process.fork`)
and the user's command is an `os_exec` in the last of them (burrow `src/server/server.lu`,
`src/ssession/ssession.lu`), so every process it makes stays in the session burrowd was started
in. Started at boot on PAX, that is the console's session, with the console as the controlling
terminal: a bare `sudo` over ssh opens `/dev/tty`, which is the console, and asks there for a
password; a read of the terminal from a background group stops that group, and it is burrowd's
own. A command's `kill 0` reaches the same group.

## 2. Measured before any change (black box)

`sess.c` asks `setsid` the questions below as a process on a fresh pseudo-terminal (it leads that
session, as a shell on a terminal does; `runpty.c`) and again with no controlling terminal (where
it first makes itself a group leader); `gsid.c` asks `getsid` about another session's process;
`rsid.rs` asks whether stable Rust's standard library can start a child in a new session.

| question | Linux x86-64 (7.2.4, glibc 2.42) | macOS arm64 (26.4.1) | PAX (read, not run: §3) |
|---|---|---|---|
| `setsid` by a session leader | EPERM | EPERM | EPERM |
| `setsid` by a group leader that leads no session | EPERM | EPERM | EPERM |
| `setsid` by any other process | ok: its session id and group id are its pid; `/dev/tty` ENXIO; `tcgetpgrp(0)` on the old terminal ENOTTY | the same | the same |
| `setsid` called on a second thread | the whole process moves: the first thread reads `getsid(0)` and `getpgrp()` equal to the pid | the same | "a thread's answers are its process's" |
| a session leader with no terminal opens a free pseudo-terminal without `O_NOCTTY` | it takes it as its controlling terminal (`/dev/tty` opens after) | the same | no pseudo-terminals; of the console, px48 P2: "a session leader with none that opens the terminal (no O_NOCTTY), when no session holds it, gets it" |
| `TIOCNOTTY` by a process that leads no session | ok; same session; `/dev/tty` ENXIO after | the same | the same for a non-leader; a leader's foreground group gets SIGHUP and SIGCONT |
| `getsid(pid)` of a process in another session | its session id | its session id | its session id |
| `getsid(pid)` of a reaped process | ESRCH | ESRCH | ESRCH |
| `CommandExt::setsid` on stable Rust | unstable (`process_setsid`, E0658) at 1.97.1, the toolchain wolf-lang and wolf-interp pin, and at 1.99.0 | the same | n/a |

Two of these decide the shape. macOS's `tty(4)` says "the current system does not allocate a
controlling terminal to a process on an open() call", and macOS 26.4.1 does: a session leader that
opens a free terminal takes it there as on Linux. The runtime's opens pass no `O_NOCTTY`, so a
child in a new session that opens a free terminal takes it on every host; the clause says so. And Rust's standard library has a stable `process_group` but no stable
way to start a child in a new session, and no `getsid`.

## 3. PAX's side (read at pax trunk `483bfab`, not run)

PAX's kernel serves `setsid` (112) and `getsid` (124) since px48 (pax `notes/px48-job-control.md`
§A): `setsid` is -EPERM for a process that leads a group (a group whose id is its pid exists: a
session leader, or a group leader); otherwise the caller leads a new session and group, its pid the
id, with no controlling terminal (`/dev/tty` -ENXIO, `tcgetpgrp(0)` -ENOTTY). `getsid` answers any
process, another session's too and a zombie until it is reaped, and -ESRCH for none. Each was held
byte for byte to the same probe on Linux under QEMU (`tests/mpx5-jobs`, the `G:` rows of
`tests/mpx5-jobs.d/linux/probe.tty`). A wolf program reaches both through its C library like any
static Linux binary there. One difference is PAX's own and named there: **a process the kernel
starts (pid 1 and each line of `/etc/pax-run`) leads a session of its own with the console as its
controlling terminal.** A wolf program started directly by `/etc/pax-run` is therefore a session
leader, and its own `setsid` is `denied`. burrowd, as burrow's server notes start it (a background
loop in the boot script that pid 1's shell runs), is in pid 1's process group and leads none, so
its own `setsid` succeeds; that placement follows POSIX's rule for a shell without job control and
is not measured on PAX.

## 4. The shapes (Q1 to Q9, each with its recommendation)

- **Q1, a child started in a new session.** (A) a new value of `os_spawn_job`'s `group`: -2, "the
  child leads a new session" (and so a new group, both ids its pid), done first in the child, before
  the defaults and the map; `tty` must be -1 beside it (`invalid` otherwise), and `invalid` moves to
  -3 and below. No new name and no new table line. But a released answer changes: -2 has been
  `invalid` since s219, and two released witnesses move without an edit of their own,
  `corpus/os/job_rows.lu` here and wolf-std's `tests/process/job_rows.lu` (`process.start_job(c, -2,
  0)` reads `invalid` at its pin and would read `not_found` at a pin carrying A). Until lupin
  mirrors it, lupin answers the well-formed call with a catchable `invalid` row instead of refusing
  it by name. (B) a sibling, **`os_spawn_session(exe: str, args: List[str], map: List[int],
  defaults: int) -> int ! {denied, invalid, io, not_found, unsupported}`**: `os_spawn_job` with the
  child leading a new session in place of the group and terminal options. No `tty` parameter: a
  child in a new session has no controlling terminal, and giving it one (`setsid`, `TIOCSCTTY`, the
  foreground, as a login does) is a later call's, so the exclusion is in the signature rather than a
  row. Released answers stay as they are, as they did when `os_wait_status` kept its answers and
  `os_wait_status_core` carried the new one (`[os.proc.status]`). One more builtin, through the same
  files Q2 and Q3 touch anyway. (C) a new parameter on `os_spawn_job`: a released builtin's arity
  changes and every caller breaks. **Recommend B.** A is the cheaper compiler change and is
  acceptable if a new name is the greater cost; its two witness moves are listed above.
- **Q2, this process into a session of its own.** (A) **`os_setsid() -> () ! {denied, io,
  unsupported}`**: `setsid(2)` on the caller. `denied` is EPERM, a caller that leads a group (a
  session leader included), and nothing has changed; `io` any other host answer (none seen);
  `unsupported` on windows. The host's number goes to `os_error()` (`[os.fs.error]`), as the
  newest calls of the family set it. (B) the same answering the new session id: it is `os_pid()`,
  so the int says nothing. (C) `TIOCNOTTY` on the controlling terminal (`os_term_detach(fd)`): the
  process loses the terminal (measured) but keeps its session and its group, so the console's
  keys still reach the group and a child's `kill 0` still reaches the daemon; it does not meet the
  need. (D) the runtime forks for a caller that leads a group (`daemon(3)`'s order): a fork the
  program did not write, under `os_fork`'s rule. **Recommend A.** The name: `os_setsid` keeps the
  C name, as `os_setpriority`, `os_chroot` and `os_umask` do; `os_set_sid` would follow
  `os_set_pgid`'s split but reads as setting an id it does not take.
- **Q3, the session id.** (A) **`os_sid() -> int ! {unsupported}`**: `getsid(0)`, this process's
  session id, `os_pgid`'s shape (equal to `os_pid()` for a session leader). (B) `getsid(pid)` of
  any process: measured portable on Linux and macOS, so a parent could witness a child that is
  not a wolf program, but no table call takes a host pid for a read today. (C) none: a witness
  would read `/proc/self/stat`, which macOS has not. **Recommend A.**
- **Q4, threads.** `setsid` is one system call on state that belongs to the process (its session,
  its group, its controlling terminal), measured from a second thread on both hosts. Unlike a
  fork, it copies nothing and no lock of the runtime's is held across it, so the runtime needs no
  protocol: `os_setsid` may be called from any thread, `main` or a task, at any time. What it
  needs is the program's: the caller must not lead a group. The sound routes, stated in the
  clause: (1) a process started in its parent's group (a spawn with no group option; a background
  job of a shell without job control: burrowd at boot); (2) `os_fork` from `main` with no task
  live, the child calling it (a fork's child never leads a group: POSIX keeps a pid while a group
  of that id lives), the classic daemon order, on the native tiers; (3) the process starts itself
  again with Q1 and ends, on every machine Q1 serves, from a task too. What moves for the caller:
  the old terminal is no longer its controlling terminal, so `os_term_set_foreground` on it
  answers `io` after; `os_term_restore_at_exit` keeps its duplicate and still restores (predicted;
  a row in phase 2). **Recommend as stated.**
- **Q5, the std surface (wolf-std).** `process.start_session(c: Command, defaults: int) -> Child !
  {unsupported, invalid, not_found, denied, io}` (Q1; `start_job`'s shape without the group),
  `process.lead_session() -> () ! {denied, io, unsupported}` (Q2; `lead_group`'s twin) and
  `process.own_session() -> int ! {unsupported}` (Q3; an int, since no call takes a session).
  wolf-std consumes released binaries only, so it lands at the pin of the release that carries
  Q1 to Q3. **Recommend as listed.**
- **Q6, capabilities.** `os_spawn_session` and `os_setsid` `exec` (process control, with
  `os_spawn_job` and `os_set_pgid`); `os_sid` `env` (a read of this process, with `os_pgid`). All
  three refused at comptime; the freestanding target refuses them by name. **Recommend as listed.**
- **Q7, the checked machine.** It serves `os_spawn_session` through its own spawn path (the same
  call first in the child's hook), and `os_setsid` and `os_sid` act on the `wolf` process it runs as,
  as `os_set_pgid` does. Its `os_fork` is `unsupported`, so Q1 is the checked machine's only way to
  start a child in a new session. **Recommend as listed.**
- **Q8, lupin.** lupin links no C library and admits no `unsafe`, and Rust's standard library at
  1.97.1 has neither a stable new-session spawn nor `getsid` (measured, §2). (A) it answers the
  shape rows and refuses each well-formed call by name, as it refuses `os_set_pgid`; (B) a safe
  system-call crate, which would change what lupin links. **Recommend A**, in a mirror lane of
  wolf-interp's own (small: three names resolved, their rows, three refusals, its rows), paired
  in the release that carries this lane. Until then lupin 0.1.60 does not resolve the three
  names, and the rows pin that by version.
- **Q9, the other hosts.** windows: `unsupported` by name for all three, after the shape (it has no
  sessions of this kind); freebsd: served through its C library, cross-built only (unmeasured);
  PAX: §3. **Recommend as listed.**

## 5. What each change will show (phase 2, after the ruling)

1. **The rows, committed first and red.** `corpus/os/session_rows.lu` (every host: the shape rows,
   and on windows the three `unsupported`), and `crates/wolf_driver/tests/session_lanes.rs` with
   fixtures under `fixtures/session/`, each program run on a pseudo-terminal the test opens (so it
   has a controlling terminal to leave), on the checked machine and as native and release
   binaries, plain and under `taskset` on four CPUs, on Linux in throwaway containers and on CI's
   macOS runner. At the base every new name is E0301 at `wolf build` (exit 2, which the rows read
   as red, never as a skip).
2. **The witnesses.** A child started with `os_spawn_session` reads `os_sid() == os_pid()` and its
   group id equal to both, opening `/dev/tty` answers `io` with `os_error()` 6 (ENXIO), and the
   parent's session and `/dev/tty` are unchanged; a map and the defaults still apply. `os_setsid`:
   by the program on the terminal (a session leader) `denied` with `os_error()` 1, nothing moved;
   by a group leader that leads no session `denied`; by a forked child `ok`, after which
   `os_sid() == os_pid()` and `/dev/tty` is `io`; from inside a task `ok`, read back on `main`'s
   thread; a second call `denied`. After it, `os_term_set_foreground` on the old terminal is `io`
   and a registered restore still puts the mode back. The refusals by name: windows (three),
   the freestanding target (three), lupin 0.1.60 ("does not resolve", pinned by version).
3. **The plant.** One hunk on the final head: the child's hook calls `setpgid(0, 0)` where it should
   call `setsid()`. The child then leads a group but not a session, so its session id is not its
   pid and `/dev/tty` opens: the spawn rows go red by name on every machine, and every `os_setsid`
   row stays green.
4. **PAX.** The same fixture as a static x86-64 binary, booted under QEMU with pax's own image
   tools from a pinned pax commit and no change to pax: as a process the kernel starts, `os_setsid`
   is `denied` and `os_sid() == os_pid()`; a child it starts with `os_spawn_session` reads its own
   session and fails `/dev/tty` with ENXIO; a forked child's `os_setsid` is `ok`. If pax's tools
   cannot carry the fixture without a change to pax, this row is a px lane's.

## 6. What would move

With Q1 = B (with A instead, the numbers in brackets):

- `spec/11-os.md`: three new anchors, `[os.proc.session]` (the spawn, after `[os.proc.job]`),
  `[os.proc.session.note]` (Q1 to Q9 as decided) and `[os.term.session]` (`os_setsid` and `os_sid`,
  after `[os.term.group]`) [A: two, the spawn's text inside `[os.proc.job]`]. Text moves in
  `[os.host.sigs]` (three lines [two] and its count), `[os.proc.job]` (a pointer to the sibling
  [A: the value, its order and its `invalid` move]), `[os.proc.poll]` and `[os.env.unset]` (the
  spawns each lists), `[os.fs.error]` (the os calls that set the number), `[os.proc.fork]` (the
  session unchanged "until the child changes it", now with `os_setsid`), `[os.term.group]` (a
  pointer), `[os.signal.platform]` (windows' three refusals) and a forward pointer in
  `[os.signal.group.note]`.
- `spec/anchors.json`: 733 anchors to 736 [735], every other key unchanged (diffed both ways).
- The host table: 177 lines to 180 [179]; `crates/wolf_sema/tests/host_table_spec.rs` with it.
- `spec/prelude.json`: 229 names to 232 [231], functions 188 to 191 [190]; marks stay 19 (no new
  tag); `prelude_names.rs` and `prelude_list.rs` with it.
- The clif runtime symbol table +3 [+2]; `str_list_fs_native.rs`, `freestanding_target.rs` and a
  new `sandbox_s290.rs` with it; the corpus lexer snapshot for `session_rows.lu` [A: and
  `job_rows.lu`'s].
- `CHANGELOG.md` under Unreleased.

## 7. CI tiers

The change touches `crates/` (the runtime, sema, WIR, clif, the checked machine, the driver),
`spec/` and `corpus/`: cross-cutting. Each push runs `tier pr` (the Linux gauntlet, the fuzz
smoke); the merge needs the `full-matrix` label at the final head and a green `tier full`, whose
`build + test (macos-latest)` runs the macOS terminal rows, the two windows legs the `unsupported`
rows, and `freebsd-check` the cross-build of the two new C library calls. The macOS native
gauntlet (`native-macos`) runs at the next nightly. The red-first rows and the plant each run as a
partial dispatch (`-f jobs=lint-test`, and `-f jobs=build-test` for macOS) on a plant branch.

## 8. Out of this repository

lupin's mirror (Q8) is wolf-interp's lane. wolf-std's surface (Q5) lands at the pin of the
release that carries Q1 to Q3. burrowd's use (its listener calling `process.lead_session()` at
start, and each connection's command calling it in the forked child before `os_exec`, so a
command's `kill 0` reaches only its own session) is burrow's. The release itself is a separate step.
