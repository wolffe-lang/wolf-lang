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

(Filled in after the probes run.)

## 4. Evidence index

(Filled in as the probes run.)

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
