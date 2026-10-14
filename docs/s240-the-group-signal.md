# s240 — signalling a group: the scope (wolf-lang#739, #740, #741)

Lane note. **Opus, read and measure only. Wave 53.** Branch `s240` off
wolf-lang trunk `6655e873`. Contract:
`sprints/compiler/s240-the-group-signal-scope.md` (planning `f8dbd3f`).
One deliverable: this plan. No surface changes in this lane: `spec/`,
`crates/`, the prelude and the corpus stay as they are. §1 to §3 are
committed before the first probe runs; §3 is scored in place later
(§3a), never rewritten.

The question: `os_signal_send` reaches one process. `kill 0`, `kill -1`
and `kill -- -PGID` cannot be written, so boreutils' `kill` refuses them
and pelt's job control (`kill %job`, `fg`, `bg`) and boreutils' `timeout`
wait on it. Two neighbours: no meaning for host-only and real-time
signals (#740), and no signal description (#741).

## 1. Forbidden

- **No implementation's source is read**: no libc (glibc, musl, Apple's
  libc), no kernel (Linux, XNU), no shell (dash, bash, zsh, busybox), no
  coreutils `kill` or `timeout`, no other language's runtime (Rust's std
  included). What POSIX, Linux and macOS do comes from POSIX.1-2024, the
  man pages as documentation, and black-box runs of programs this lane
  wrote, plus `strace` of shipped binaries.
- **Signals go only to processes the probe itself started**, inside a
  fresh `--network none` container on kasumi (its own pid namespace), or
  on CI's throwaway macOS runner after the probe has put itself in a
  session of its own. `kill(-1)` is run only inside a container, where
  every process in the namespace is the probe's. Never a signal to a
  host process: kasumi is shared with other lanes.
- No surface change: nothing under `spec/`, `crates/`, `corpus/`,
  `xtask/`, and no prelude entry. The PR holds this note and the probe
  programs (`docs/s240/`).
- No build on nomad-1. kasumi under `~/lanes/s240/` only; its login shell
  is fish (`bash -c`); a detached job by `setsid` with a pid file, killed
  only by that pid, never by name, pattern or group. Containers and
  images this lane makes are removed at the end; no other lane's image is
  changed. Never host ports 8080/8088.
- r35 is cutting wolf 0.2.30: none of its branches is touched. No merge,
  no tag, no issue filed or closed (listed in the report). No other
  lane's CI run is cancelled. Nothing under `~/.claude`. No attribution
  trailers. No `git add -A`; the branch is asserted before every commit.
  Scratch under `~/lanes/s240/`, never `/tmp`.
- Strict evidence (#571): exit 2 from `wolf build` is a compile error;
  SigBlk and SigIgn are read and printed before any signal row is called
  red; process probes also run with four cpus asserted from inside the
  container (`Cpus_allowed_list`; bu23 found `taskset … podman run`
  confines nothing). No claim of "seen" without a log path and digest.

## 2. Inputs, verified (2026-10-14)

| input | contract says | found | drift |
|---|---|---|---|
| planning | `f8dbd3f` | `wolf` trunk head `f8dbd3f` "ruling #78; s238, s239, s240 contracts"; contract read in full | none |
| wolf-lang trunk | — | `6655e873` (tl21's CONTRIBUTING); branch `s240` cut from it | — |
| the released surface | `os_spawn_job`, `os_pgid`, `os_term_foreground`, `os_term_set_foreground` in 0.2.27 | `v0.2.27` (`0ddda6b7`) `spec/11-os.md` names `os_spawn_job` 9 times and `os_pgid()` twice; `v0.2.26` (`89dc1394`) neither; `os_signal_send` and `[os.signal.set.stable]` first at `v0.2.29` (`6ffe7c60`) | none |
| `[os.signal.send]` | pid above 0 | at trunk and at `v0.2.29`: `invalid` for a `pid` not above 0; "a group (`kill`'s 0 and negative ids) is job control's, the next lane's (`[os.term.note]`)". The runtime (`crates/wolf_rt/src/signal.rs`, `send`) refuses `p <= 0` before `kill(2)` | none |
| `[os.signal.set]`, `.stable` | a published bit is permanent | 28 meanings, bits 0..27; the next bit any new meaning may take is 28; ruled 2026-10-12 | none |
| `os_kill(h)` | — | the handle-only send: the runtime calls the child's `kill()` (Rust `Child`), one process, no meaning argument | — |
| `os_wait_status` | — | blocks; a stop is not reported (`[os.proc.status]`); #680 open | — |
| a process into its own group | — | **no call**: `os_spawn_job`'s `group` acts on a child; nothing moves the calling process (`setpgid(0,0)`) or starts a session (`setsid`); `[os.term.note]` R4(C) rejected `os_setpgid(h, g)` as racy for a child | reported (§4) |
| lupin | — | wolf-interp trunk `10de3a6`, `src/eval/builtin.rs`: `os_signal_send` answers its shape rows and is refused by name ("std has no `kill(pid, sig)`"); `os_pgid` and every `os_term_*` refused by name; `os_spawn_job` mirrored for `group` (#57 R8, #70 R9); `unsafe_code = "forbid"` | none |
| boreutils | `notes/bu23-the-kill.md` | trunk `9819e44`; §4 "What `kill` cannot do yet": the group (33 cases state GNU's answer beside ours), 142 linux skips for host-only signals, `-t`'s third column | none |
| pelt | `notes/sh14-the-signals.md`, the census | trunk `00e15ca`; `tests/diff/PENDING.md` rows *jobs*, *job-control*, *interruptible wait*; `docs/CENSUS.md` §2.11: 15 statements, 3 met, 10 pending on job control; 15 `c_jc_*` sessions and 4 `jobsctl/` cases | — |
| ruling #69 R8 | — | `sprints/pelt/index.md`: "R8 a pax lane after sh16" — job control on PAX is a pax lane after pelt's H3e works on Linux; P5's U5 edits `kernel/signal` and is "not beside the job-control lane" (`pax/p5-the-users-campaign.md`) | — |
| issues | #739, #740, #741, #680 | all four OPEN; #739 and #740 carry one pelt comment each | none |
| wolf-std | — | trunk `9e5bd6b`: `std.process.start_job`/`start_foreground` over `os_spawn_job`; `std.term` over `os_pgid`/`os_term_set_foreground`; `std.signal.send` over `os_signal_send` | — |
| kasumi | podman | Linux 7.2.8-1-cachyos, podman 6.1.3 rootless, `/home` 97% (32 GB free) | — |

## 3. Prediction (committed before the first probe runs)

Linux below is kasumi's kernel in a rootless podman container (its own
pid namespace); "root" is the container's root (a user namespace's: px38
found it differs from real root on 99 lines, none of them `kill(2)`'s
rule for processes in the same namespace, which is all a probe here
reaches).

1. **`kill(0, sig)`** reaches every process in the caller's group, the
   caller included, and no process outside it, on linux and macOS, root
   and user alike.
2. **`kill(-G, sig)`** reaches every member of group G and nothing else;
   the caller outside G is not signalled. A G with no process is
   `ESRCH`. A G whose leader has exited while members live is still a
   group (0). A G whose members all are zombies answers 0 (as a zombie
   pid does). Signal 0 to a live group is 0.
3. **Permission over a group**, as a user: a group with one member the
   user may signal and one it may not answers **0** and signals only the
   permitted one; a group with no permitted member is **`EPERM`**.
   `SIGCONT` reaches a member of another uid in the same session (px38's
   finding for a pid, predicted to hold for a group).
4. **`kill(-1, sig)`** on linux reaches every process the caller may
   signal **except the caller and the namespace's pid 1**; as root that
   is every other process in the container; as a user, only that user's
   processes, whatever their session or group — an unrelated process of
   the same uid in another session is signalled. With nothing it may
   signal it answers `ESRCH`. macOS: not measured (a runner process of the
   same uid would be reached; the man page is the source).
5. **`kill(INT_MIN, sig)`** is `ESRCH` on linux (its negation does not
   fit); on macOS unknown.
6. **Sessions and groups:** `setsid` by a group leader is `EPERM`; by a
   non-leader it answers the new sid, equal to its pid and pgid, with no
   controlling terminal. `setpgid` into a group of another session is
   `EPERM`; on a child after it has exec'd, `EACCES`; on a process that is
   not a child, `ESRCH`; by a session leader on itself, `EPERM`.
7. **The terminal:** a session leader opening a pty's slave gets it as
   its controlling terminal on linux without `TIOCSCTTY`, and needs
   `TIOCSCTTY` on macOS. `tcsetpgrp` from a background group with
   `SIGTTOU` at its default stops the caller; with it blocked or ignored
   it succeeds; to a group of another session it is `EPERM`. The
   interrupt character reaches the foreground group only. An orphaned
   group with a stopped member is sent `SIGHUP` then `SIGCONT` when it
   becomes orphaned.
8. **Stops:** `kill(-G, SIGSTOP)` stops every member, and a parent's
   `waitpid(…, WUNTRACED)` reports each stop; `SIGCONT` to the group is
   reported with `WCONTINUED`.
9. **The host's signal table:** glibc on linux has 64 signals, real-time
   34..64 (`SIGRTMIN` 34, 32 and 33 kept by the C library); musl's
   `SIGRTMIN` is **35**; macOS has 31 and no real-time range. So the
   real-time numbers are the C library's, not only the kernel's.
10. **Descriptions (`strsignal`):** glibc, musl and macOS each answer a
    different text for at least one of the 28 meanings (macOS appends
    the number, "Killed: 9"); glibc's is English in a program that never
    calls `setlocale`, whatever `LANG` says, and translated after
    `setlocale(LC_ALL, "")` where the translations are installed.
11. **What shells and `timeout` do, by `strace`:** dash with `set -m`
    starts each job with `setpgid` (in the child and the parent),
    signals a job by `kill(-PGID, …)`, continues one with
    `kill(-PGID, SIGCONT)` and waits with `WUNTRACED` (and
    `WCONTINUED`); GNU `timeout` puts itself in a new group
    (`setpgid(0, 0)`) and sends with `kill(0, sig)` unless
    `--foreground`, so its command's own children die with it.
12. **What wolf 0.2.29 gives (a witness in wolf, native and release):**
    `os_spawn_job` with `group` 0 makes a job a group whose id is
    `os_proc_pid`; `os_signal_send` to that pid ends the leader and
    leaves the job's own children running; `os_signal_send(0, …)`,
    `(-1, …)` and `(-pgid, …)` are `invalid`, sending nothing; lupin
    refuses the send by name.
13. **The plan:** the recommendation is a separate builtin for a group
    and one for "everyone", not a sign on `os_signal_send`'s pid; a
    group type lives in wolf-std, not the builtin tier; host-only
    signals get **no** meaning (an escape by host number instead, with
    no portability promise), because 31 real-time numbers and five
    host-only names do not fit the 35 bits left under the sign;
    `os_signal_text` returns the C-locale text. Five lanes or more
    follow, the first in wolf-lang; at least four rulings are owed.
14. One thing here is wrong in a way this list does not name.

## 3a. The prediction, scored

§3 is as committed in `50cfab64`, pushed before the first probe ran.
"linux" is kasumi's kernel (7.2.8-1-cachyos) in rootless podman, glibc
2.39 (Ubuntu 24.04) and musl (Alpine 3.20), root and uid 1000, four
cpus asserted from inside (`Cpus_allowed_list: 0-3`), SigBlk and SigIgn
0 in every probe; "macOS" is CI's runner (macOS 26.6.2, arm64), as
`runner` and under `sudo`. Logs: `docs/s240/ev/linux/run-linux.log`,
`docs/s240/ev/macos/run-macos.log` (§4).

| # | predicted | measured | verdict |
|---|---|---|---|
| 1 | `kill(0)`: the caller's group, the caller included, nothing else | linux (both C libraries, root and user) and macOS (user and root): `heard=A:,B:,S:` — the two members and the sender; C, in another group, not | right |
| 2 | `kill(-G)`: the members only; no group `ESRCH`; leader gone, members live: 0; only zombies: 0; signal 0: 0 | all of it on linux. **macOS parts on one row:** a group whose members are all zombies is **`EPERM`** there (0 on linux), for signal 0 and a real signal alike | right on linux; wrong on macOS's zombie row |
| 3 | as a user: a mixed group 0 and only the permitted member hears; none permitted `EPERM`; `SIGCONT` reaches another uid in this session | as predicted on linux and macOS; `SIGCONT` to a root member of **another** session is `EPERM`, by pid and by group | right |
| 4 | `kill(-1)` skips the caller and pid 1; root reaches every other process; a user only its own uid, whatever the session; nothing to reach `ESRCH`; macOS not measured | root: `A:,B:,R:,Uuser:` (not the sender, not pid 1); user: `Uuser:` only, a process in another session. **Nothing to reach answers 0, not `ESRCH`**: a census just before the send lists only root's four processes (`1/uid0 2/uid0 36/uid0 37/uid0`), and `kill(-1, sig)` and `kill(-1, 0)` both return 0 with nobody hearing. macOS: not run (it would reach the runner's own processes) | wrong on the empty case |
| 5 | `kill(INT_MIN)` `ESRCH` on linux; macOS unknown | `ESRCH` on both, and `kill(-INT_MAX)` `ESRCH` on both | right (macOS measured) |
| 6 | `setsid` by a leader `EPERM`; by a non-leader the new sid = pid = pgid, no terminal; `setpgid` across sessions `EPERM`, after exec `EACCES`, not a child `ESRCH`, a session leader on itself `EPERM` | all, on linux. **Not predicted:** `setpgid` into a group that does not exist is `EPERM` (both hosts). **macOS parts:** `setpgid(grandchild, 0)` answers **0** there (`ESRCH` on linux) | right on linux; wrong on macOS's grandchild row |
| 7 | the pty slave is the ctty without `TIOCSCTTY` on linux, and needs it on macOS; `tcsetpgrp` from the background with `SIGTTOU` at default stops; blocked or ignored, succeeds; another session `EPERM`; the interrupt reaches the foreground group only; an orphaned group with a stopped member gets `SIGHUP` and `SIGCONT` | **macOS needs no `TIOCSCTTY` either** (wrong). The stop happens **only for a group that is not orphaned**: from the session leader's own group — orphaned, since the leader's parent is in another session — linux answers **`ENOTTY`** and macOS **`EIO`**, and nothing stops. Ignored: 0. Another session: `EPERM` on both. A group that does not exist: `ESRCH` on linux, `EINVAL` on macOS; `tcsetpgrp(0)`: `ESRCH` / `EPERM`. The interrupt reached the two job members, not the leader. The orphaned group's member heard both `HUP` and `CONT` (their order is not measured: both were pending) | wrong on two macOS halves and on the orphaned-group case |
| 8 | `kill(-G, SIGSTOP)`: every member stops and `WUNTRACED` reports each; `SIGCONT` with `WCONTINUED` | 2 of 2 and 2 of 2, linux and macOS | right |
| 9 | glibc: 64 signals, real-time 34..64; musl `SIGRTMIN` 35; macOS 31, none | glibc `NSIG` 65, `SIGRTMIN` 34, `SIGRTMAX` 64; musl 65, **35**, 64; macOS `NSIG` 32, no real-time | right |
| 10 | the three C libraries part on at least one of the 28; macOS appends the number; glibc English without `setlocale`, translated after it | glibc and musl part on **2 of 28** (`FPE`: "Floating point exception" / "Arithmetic exception"; `CHLD`: "Child exited" / "Child process status") and on every real-time and unknown number ("Real-time signal 0" / "RT34"; "Unknown signal 32" / "Unknown signal"); macOS appends ": N" to **28 of 28** and words 7 differently even without it (`TRAP`, `ABRT`, `STOP`, `TSTP`, `XCPU`, `XFSZ`, `WINCH`). glibc under `LANG=de_DE.UTF-8` is byte-identical to C without `setlocale` (`3f9883723ac7…` twice) and German after it ("Getötet"); musl unchanged. macOS translation not measured: the runner's environment pins `en_US.UTF-8` | right |
| 11 | dash `-m`: `setpgid` both sides, `kill(-PGID)`, `SIGCONT` to the group, `WUNTRACED` and `WCONTINUED`; GNU `timeout`: `setpgid(0, 0)` and `kill(0)` unless `--foreground` | dash: `setpgid(0, N)` in the child and `setpgid(N, N)` in the parent, `TIOCSPGRP`, `kill(-N, SIGSTOP / SIGCONT / SIGTERM)`, `wait4(-1, …, WNOHANG\|WSTOPPED)`. **Never `WCONTINUED`** (wrong). **Not predicted:** interactive dash puts *itself* in a group of its own at start (`setpgid(0, own pid)`, then `TIOCSPGRP`) and back into its first group at exit (`setpgid(0, 37)`). `timeout` 9.4: `setpgid(0, 0)`, then `kill(child, TERM)`, `kill(0, TERM)`, `kill(child, CONT)`, `kill(0, CONT)`; nothing outlives it; with `--foreground`, one `kill(child, TERM)` and both of the command's own `sleep`s outlive it | right but for `WCONTINUED`; the shell's own group not predicted |
| 12 | wolf 0.2.29: `os_spawn_job(…, 0, …)` makes a group named by `os_proc_pid`; a send to it ends the leader and leaves the job's child; 0, -1, -pgid `invalid`; lupin refuses by name | native, release and checked alike: `ps` shows `sh` and its `sleep` in the leader's group; `send(0)`, `send(-1)`, `send(-leader)`, `send(-leader, 0)` `invalid`; `send(leader, TERMINATE)` ok, status -15, the `sleep` alive (`S`); `os_kill(h)` status -9, the `sleep` alive (`S`). lupin 0.1.52 refuses `os_signal_send` by name. **Not predicted:** the job lupin started before its refusal is still running after lupin exits (`sh` and `sleep` in group 70, `ps` before and after), and its record says `"phase_reached":"resolve"` for a program that ran far enough to start a job | right, with two lupin findings |
| 13 | separate builtins, a std type, no meanings for host-only signals (36 do not fit 35 bits), C-locale text; five lanes or more; at least four rulings | as §C–§E: two builtins and one self-group call, a std `Group`, the `-N` escape, `os_signal_text`; **ten** lanes (two in wolf-lang plus #680's), **nine** rulings | right in shape; the counts higher |
| 14 | one thing wrong this list does not name | three, all the instrument's and all caught before a number was kept: (a) the runner's `read` of a sender's answer was cut short by the `kill(-1)` it was measuring (`EINTR`), so the first run printed `errno-1`; fixed with `SA_RESTART`. (b) `strace -f` waits for every tracee, so the first "who outlives `timeout`" check could not see a survivor; rerun without `strace`. (c) dash read the cleanup's `kill -7` as **signal 7** with no operand (a usage error): the shell-level twin of the sign problem §C argues from | — |

## A. What the hosts do (measured)

**`kill(2)` beyond one process** (grp, linux and macOS; `kill(-1)` linux
only):

| target | linux | macOS |
|---|---|---|
| `0` | every member of the caller's group, the caller too | same |
| `-G`, G live | every member of G; the caller only if it is one | same |
| `-G`, no such group | `ESRCH` | `ESRCH` |
| `-G`, leader gone, members live | 0; the members hear | same |
| `-G`, members all zombies | **0** | **`EPERM`** |
| `-G`, signal 0 | 0 / `ESRCH` / `EPERM`, as for a real signal | same |
| `-G` as a user, one member permitted, one not | **0**, the permitted one only | same |
| `-G` as a user, none permitted | `EPERM` | `EPERM` |
| `-G`, `SIGCONT`, another uid, same session | 0, heard | same |
| `-G`, `SIGCONT`, another uid, another session | `EPERM` | `EPERM` |
| `-1` as root | every process but the caller and pid 1 | not measured |
| `-1` as a user | that uid's processes, any session or group; **0 even when there are none** | not measured |
| `INT_MIN`, `-INT_MAX` | `ESRCH` | `ESRCH` |

**Sessions, groups and the terminal** (sess, both hosts, root and user
alike): a session leader cannot `setsid` or `setpgid(0, 0)` (`EPERM`);
nor can a group leader `setsid`; a non-leader's `setsid` makes it leader
of a new session and group with no terminal (`/dev/tty` `ENXIO`).
`setpgid` of a child into another session's group or a group that does
not exist is `EPERM`, after the child's `exec` `EACCES`. A pty slave
opened by a session leader becomes its controlling terminal on both
hosts. `tcsetpgrp` by the leader with `SIGTTOU` blocked hands the
terminal over; from a background group that is **not orphaned** it
stops the caller with `SIGTTOU` (both hosts); from an **orphaned** group
it fails without a stop — `ENOTTY` on linux, `EIO` on macOS. A
`kill(-G, SIGSTOP)` stops each member and `waitpid(-G, WUNTRACED)`
reports each once; `SIGCONT` is reported with `WCONTINUED`. The
interrupt character reaches the foreground group and not the session
leader behind it. A group that becomes orphaned with a stopped member is
sent `SIGHUP` and `SIGCONT`.

| row | linux | macOS |
|---|---|---|
| `setpgid` of a grandchild | `ESRCH` | **0** |
| `tcsetpgrp` to a group that does not exist | `ESRCH` | `EINVAL` |
| `tcsetpgrp(fd, 0)` | `ESRCH` | `EPERM` |
| `tcsetpgrp` from an orphaned group, `SIGTTOU` default | `ENOTTY` | `EIO` |

**What a shell and `timeout` ask** (strace of Ubuntu 24.04's dash and
GNU `timeout` 9.4 in the glibc image; dash is `0.5.12-6ubuntu5` by
`dpkg-query` in the same image, not in the log, and pelt's oracle is
0.5.13.4): job control is `setpgid` in child and
parent, `TIOCSPGRP` to hand the terminal over and take it back,
`kill(-PGID, …)` for `kill %n`, `fg` and `bg` (`SIGCONT`), and
`wait4(-1, WNOHANG|WUNTRACED)` — any child, without blocking, stops
reported; never `WCONTINUED`. An interactive dash also moves **itself**:
`setpgid(0, its pid)` at start and `setpgid(0, the old group)` at exit.
`timeout` moves itself into a new group, signals its child, then
`kill(0, …)` (itself included), then `SIGCONT` to both; nothing the
command started outlives it. `--foreground` signals the child alone.

**The host's table** (sigtab): linux has 64 signals; the real-time range
is the **C library's**: 34..64 under glibc, 35..64 under musl, on one
kernel. Host-only names: linux `STKFLT` 16, `IO`/`POLL` 29, `PWR` 30;
macOS (31 signals, no real-time) `EMT` 7, `IO` 23, `INFO` 29.
`strsignal` is the C library's: glibc and musl part on 2 of the 28 and
on every real-time text; macOS adds ": N" to all 28 and words 7
differently; glibc translates only after `setlocale`, which no wolf
program calls.

## B. What wolf has (0.2.29; trunk `6655e873`)

| need (measured above) | wolf today | gap |
|---|---|---|
| start a job in a new group, a pipeline's stages in the first's | `os_spawn_job(…, group, …)`: 0 leads, >0 joins; parent and child both `setpgid` (as dash) | none |
| a job's group id | `os_proc_pid(h)` of a job started with `group` 0 | none |
| this process's group | `os_pgid()` | none (lupin refuses it by name) |
| hand the terminal over, take it back | `os_spawn_job`'s `tty`, `os_term_set_foreground` (blocks `SIGTTOU` itself) | none |
| signal a job (`kill %n`, `fg`, `bg`, `timeout`) | `os_signal_send(pid)` and `os_kill(h)`: one process; the job's own children live on (witness) | **#739** |
| signal everything I may (`kill -1`) | none | **#739** |
| put **this** process in a new group, or back (interactive shell; GNU `timeout`) | none: `os_spawn_job` acts on a child | **new** (not filed) |
| wait for any child, without blocking, with stops | `os_wait_status(h)` blocks, one handle, no stops | **#680** and pelt's W03 |
| a host-only or real-time signal | none: 28 meanings | **#740** |
| a signal's words | none (`os_error_text` is `strerror`) | **#741** |

## C. The options, and the recommendation per question

**Q1. How a program names a group (#739).**

| option | shape | for | against |
|---|---|---|---|
| **A** | two builtins: `os_signal_send_group(pgid: int, meaning: int) -> () ! {denied, invalid, io, not_found, unsupported}` (pgid above 0 and a process id's width, else `invalid`; `kill 0` is `send_group(os_pgid()?)`) and `os_signal_send_all(meaning: int) -> () ! {denied, invalid, io, not_found, unsupported}` (`kill -1`) | each call says what it reaches; `os_signal_send`'s released `invalid` rows for 0 and -1 (`corpus/os/signal_shell_rows.lu`) stay true; the same rows and order as `[os.signal.send]` | two names for one syscall |
| B | a group as its own *type* in the builtin tier | the type says "group" | the builtin tier has no structs (`[os.term.note]` R4(B)); a type belongs to wolf-std, built over A |
| C | `kill(2)`'s encoding: 0, -1 and -G on `os_signal_send`'s pid | one call | a pid computed wrong becomes a broadcast: `to_int() else -1` is the house idiom (this lane's own witness writes it), and `kill(-1)` as a user reached a process in **another session** (§A); dash read `kill -7` as signal 7 (§3a 14); changes the meaning of a released call's rows; #70 R6(B) rejected it |
| D | `os_signal_send_to(kind, id, meaning)` | one name | an int that means a verb (#57 R2(B)'s reason) |

**Recommend A**, with the type in wolf-std: `process.Group` (from a
`Child` started as a job, or `term`'s own group), `signal.send_group(g,
meaning)`, `signal.send_all(meaning)`. Rows as the host answers them
and as `[os.signal.send]` orders them: shape, host, target. Stated in
the clause: a mixed group is ok and reaches the permitted members only;
a linux `send_all` with nothing to reach is ok (measured); macOS answers
`EPERM` (`denied`) for a group of zombies where linux answers 0.

**Q1b. This process into a group of its own** (not asked by #739;
measured: an interactive shell and GNU `timeout` both do it).

| option | shape | for | against |
|---|---|---|---|
| **A** | `os_set_pgid(pgid: int) -> () ! {denied, invalid, io, unsupported}` on the calling process only: 0 leads a new group, >0 joins one in this session | what dash does at start and exit, and what makes `timeout` exact; not the racy child form #57 R4(C) rejected | one more call |
| B | none: pelt relies on being started as a session leader (its `c_jc_shell_is_leader` is met only so); a wolf `timeout` puts the *command* in a new group | no surface | a shell started inside another job never leads; `timeout` differs from GNU in who hears Ctrl-C |
| C | A and `os_setsid()` | daemons, `setsid(1)` | no customer yet |

**Recommend A.**

**Q2. What the existing builtins give.** §B: the group's placement,
id and terminal are done (0.2.27); only reaching it is missing, plus the
calling process's own group and #680's wait. A program can approximate
`kill %n` today by signalling every pid it spawned into the job — the
witness shows why that is not the job: the leader's own children (a
`make`'s compilers, a script's `sleep`) are in the group and not in the
program's list, and they outlive the leader.

**Q3. Windows and lupin.**

| | recommend | why |
|---|---|---|
| windows | **`unsupported`, by name, after the shape** for `send_group`, `send_all`, `set_pgid` (`[os.signal.platform]` gains the line) | no process groups for signals; its console groups (`CREATE_NEW_PROCESS_GROUP`, `GenerateConsoleCtrlEvent` with `CTRL_BREAK_EVENT`) and job objects are another model, reached only through a spawn option `os_spawn_job` refuses there today (from Microsoft's documentation; not measured, no windows probe) |
| lupin | **(A) answer the shape rows and refuse by name**, as #70 R9(A) for `os_signal_send`; (B) a safe syscall crate (rustix) is the maintainer's call, asked a third time | std has no `kill(-pgid)`, no `setpgid` for self; `unsafe_code = "forbid"` |

Two lupin findings from the witness, for wolf-interp (not filed): a job
lupin started is left running when lupin later refuses a call by name,
and that run's record says `"phase_reached":"resolve"`.

**Q4. Host-only and real-time signals (#740).** The arithmetic first:
bits 0..27 are published; a meaning is a bit of a 64-bit int, so 35
positive bits remain (28..62). Linux and macOS name five host-only
signals (`STKFLT`, `IO`, `PWR`, `EMT`, `INFO`) and glibc has 31
real-time numbers: **36 do not fit**, and musl's range starts one
higher, so "a meaning per host signal" cannot be the general answer.

| option | shape | for | against |
|---|---|---|---|
| A | meanings for the five named host-only signals (bits 28..32), `invalid` where the host lacks one | `trap INFO` portable by name | five bits spent for ever (`[os.signal.set.stable]`); real-time still needs B |
| **B** | **an escape by host number with no portability promise**: a sending call (`os_signal_send`, `send_group`, `send_all`) and `os_signal_text` take **-N** for the host's signal N, 1 ≤ N ≤ `os_signal_last()`; every set-taking call (`listen`, `wait`, `poll`, `ignore`, `default`) refuses it by the one shape rule; `os_signal_last() -> int ! {unsupported}` (64 linux, 31 macOS) and `os_signal_realtime() -> (int, int) ! {unsupported}` ((34, 64) glibc, (35, 64) musl, (0, 0) macOS) | no bit spent; composes with Q1 without doubling it; -N is `os_wait_status`'s spelling of "ended by signal N", so a shell re-sends what it saw | a negative meaning is a second shape on the sending calls (argued: a meaning is never computed by arithmetic, and the spec's "never a host number" sentence gains its one, named exception) |
| B′ | B as separate `…_number` twins of each sending call | no overloaded parameter | three more builtins now and one per future sender |
| C | A and B | both | A's door is still one-way |

**Recommend B.** `trap … IO` and `trap INFO` stay out (listening is by
meaning, through the self-pipe); a later ruling may give `INFO` alone a
meaning if pelt's macOS users ask for Ctrl-T. boreutils keeps its names
for host-only signals in a per-target table (as it gates `CLD` today)
and derives `RTMIN+n` from `os_signal_realtime`.

**Q5. A signal's words (#741).**

| option | shape | for | against |
|---|---|---|---|
| **A** | `os_signal_text(meaning: int) -> str ! {invalid, unsupported}`: the host C library's `strsignal` in the C locale; a meaning, or -N under Q4(B) | GNU `kill -t` and a shell's "Killed" match the host's words on every C library (measured: they differ) | the text varies by host and library, by design |
| B | a fixed wolf table of English texts | one answer everywhere | wrong for macOS on 28 of 28 and for musl on 2; a table to keep |
| C | A, translated by the user's locale | GNU's NLS behaviour | wolf has no locale anywhere; glibc translates only after `setlocale`, which wolf never calls (measured byte-identical) |

**Recommend A**: uncharged (`os_error_text`'s and `os_signal_number`'s
shelf), refused at comptime, `unsupported` on windows, refused by name
on lupin (a table there would be one C library's: glibc and musl part).

## D. The lane sequence

| # | lane | repo | size | needs | delivers |
|---|---|---|---|---|---|
| 1 | the group send | wolf-lang | M | rulings R1–R3, R6–R8 | `os_signal_send_group`, `os_signal_send_all`, `os_set_pgid`; clauses beside `[os.signal.send]`; native, release, checked; windows rows; shape rows in `corpus/os/`; live witnesses that send only inside a group the test spawned (never `os_pgid()`'s, which is the test runner's) and `send_all` only in a container on linux CI (macOS: shape rows, named) |
| 2 | host numbers and words | wolf-lang | S | R4, R5; after 1 (same files) | -N on the sending calls, `os_signal_last`, `os_signal_realtime`, `os_signal_text`; may ride in lane 1 as one M+ lane |
| 3 | the job wait (#680, W03) | wolf-lang | M | its own scope or contract | any child, without blocking, stops reported (`wait4(-1, WNOHANG\|WUNTRACED)` is what dash asks; no `WCONTINUED`); a wait a signal can cut short |
| 4 | the mirror | wolf-interp | S | 1–3 | shape rows, refusals by name; the two lupin findings (Q3) |
| 5 | the release | wolf-lang / wolf-interp | S | 1–4 | wolf 0.2.3x, lupin 0.1.5x |
| 6 | std | wolf-std | S | 5 | `process.Group`, `signal.send_group`, `signal.send_all`, `process.lead_group`, `signal.text`, `signal.last`, `signal.realtime` |
| 7 | job control (H3e) | pelt | L (two lanes) | 5, 6 | `set -m`, `jobs`, `fg`, `bg`, Ctrl-Z, notices, `kill 0`, `kill -- -N`, `kill %n` to the group; census: §2.11's 10 pending statements, `jobs/1–3`, `fg/1–2`, `bg/1`, `kill/6`'s group half; the 15 `c_jc_*` sessions and 4 `jobsctl/` cases |
| 8 | `kill`'s groups and tables | boreutils | S | 5 (6 if std) | the 33 group cases differential; `-l`/`-t` full on linux (the 142 skips) through -N and `os_signal_realtime`; `-t`'s third column |
| 9 | `timeout` | boreutils | M | 5 | GNU 9.11's group behaviour (measured here on 9.4: `setpgid(0, 0)`, child then `kill(0)`, then `SIGCONT`), `--foreground`, `-k`, statuses |
| 10 | job control on PAX | pax | L | U5 (`kernel/signal`, not beside it); after pelt's H3e works on linux (#69 R8) | groups and sessions, `kill`'s 0/-1/-G with U5's permission rule, `TIOCSPGRP`/`TIOCGPGRP`/`TIOCSCTTY`, background stops, `WUNTRACED`, orphaned-group `HUP`+`CONT`; this lane's `grp.c` and `sess.c`, built static, are its oracle on both answers |

Critical path for pelt: 1 → 3 → 4 → 5 → 6 → 7 (lane 2 can run beside 3).
boreutils' `kill` needs only 1, 2 and the release.

## E. Rulings owed

- **R1, the group send (#739).** A (two builtins), the type in wolf-std.
- **R2, the calling process's group.** A, `os_set_pgid(pgid)` on this
  process only.
- **R3, the rows.** As the host answers (`send_all` with nothing to
  reach is ok on linux; a zombie group `denied` on macOS), stated in the
  clause rather than evened out.
- **R4, host-only signals (#740).** B: -N on the sending calls and the
  text, `os_signal_last`, `os_signal_realtime`; no new bits; `INFO` left
  to a later ruling.
- **R5, a signal's words (#741).** A, `os_signal_text`, C locale.
- **R6, capabilities (#54's reading).** `send_group`, `send_all` and
  `set_pgid` are `exec` (with `os_signal_send`); `os_signal_last`,
  `os_signal_realtime` and `os_signal_text` uncharged; all refused at
  comptime.
- **R7, windows.** `unsupported` by name, after the shape.
- **R8, lupin.** A (shape rows, refusals by name), or B (rustix) — the
  maintainer's call, asked at #57, #70 and here.
- **R9, the order.** §D: the group send first, the job wait (#680) next,
  then lupin, the release, std and the downstreams.

## F. Measured, and inferred

- **Measured:** every row of §A on linux (glibc and musl, root and user)
  and every row but `kill(-1)` on macOS (user and root); the C-library
  tables on glibc, musl and macOS; dash's and `timeout`'s calls on
  linux; wolf 0.2.29's answers on native, release and checked, and
  lupin 0.1.52's.
- **Not measured:** `kill(-1)` on macOS; macOS `strsignal` under a
  translation (the runner pins `en_US.UTF-8`); dash and `timeout` on
  macOS (no `strace`); real root (a rootless container's root reaches
  only its namespace, which is every process a probe here could reach);
  windows (no probe); linux aarch64; GNU `timeout` 9.11 (the image has
  9.4); the order of `SIGHUP` and `SIGCONT` to an orphaned group.
- **Inferred:** that linux's 0 for an empty `kill(-1)` counts the
  caller, which it does not signal; why macOS answers `EPERM` for a
  group of zombies; that the rows of §A hold for real root as they do in
  a user namespace; the windows column of Q3; every size in §D.

## 4. Evidence index

Commits on `s240` (all pushed):

- `50cfab64` — §1–§3, the prediction, before any probe ran.
- `2a9188dd` `grp.c`, `b56181b4` `sess.c`, `c7839afb` `sigtab.c`,
  `4f392b7e` `witness.lu`, `7c950dd0` `shells.sh`, `c0c05734` the linux
  driver and images, `81d80dd9` the macOS driver, `e2d98de6` the final
  driver (the sources every logged number comes from).
- `08ab7218` the linux evidence, `2bba1fbd` the macOS evidence.

Logs (sha256, first twelve; the full files are in the branch):

- `docs/s240/ev/linux/run-linux.log` `8b7bfef49fd3` — kasumi, 2026-10-14
  22:03:14–22:04:43Z, the probes as at `e2d98de6`: images (`localhost/s240-ubuntu`
  from `ubuntu:24.04` `69f919497cad…`, `localhost/s240-alpine` from
  `alpine:3.20` `bf8527eb54c3…`), the binaries' digests, grp and sess on
  glibc and musl as root and uid 1000, sigtab, the shells, and the
  witness: wolf 0.2.29 (`ae466e85418a…`) and lupin 0.1.52
  (`f5194b74879c…`), each checked twice against the release API's
  digest.
- `docs/s240/ev/linux/sigtab-glibc-C.txt` `3f9883723ac7`, `…-de-nosetlocale.txt`
  `3f9883723ac7` (identical), `…-de-setlocale.txt` `6fb0cfa09142`,
  `sigtab-musl-C.txt` `7310514528ca`, `sigtab-musl-de-setlocale.txt`
  `c296360f7d31`.
- `docs/s240/ev/linux/dash-kill.trace` `9535c68cd6e0`, `dash-m.trace`
  `d624f816cfb5`, `dash-im.trace` `0587f62c3909` (the shell's own
  `setpgid(0, 38)` at line 17 and `setpgid(0, 37)` at line 49),
  `timeout.trace` `c16838d4a57a`, `timeout-fg.trace` `c7550480f7f3`.
- `docs/s240/ev/macos/run-macos.log` `291fae740da6` — CI run 38644896760
  on a throwaway branch (`s240-macos-probe`, the probes at `e2d98de6`
  plus one workflow file; deleted after the run), macOS 26.6.2, Apple
  clang 21; `sigtab-macos-C.txt` `217e600c7db4`,
  `sigtab-macos-de-setlocale.txt` `d74e8ea7c99e`. An earlier run of the
  same probes, 38644564590, answered alike on every row.
- Earlier linux runs (instrument defects, §3a 14) are not evidence and
  are not kept.

## 5. Done-when

- Branch `s240` on origin, PR open and unmerged, the five sections above
  by name, the commit shas as bullets, a test checklist.
- CI per the tiers: the probes live under `docs/s240/`, which
  `ci/tiers.py` does not call prose (`docs/**` is `pr`), so a push runs
  `tier pr`; the `full-matrix` label at the final head runs `tier full`.
- `wolf/tools/lane-audit.sh wolf-lang s240 <PR>` run from the org
  directory, exit 0 read directly.
- Every container and image this lane made on kasumi removed; no
  detached job left.
