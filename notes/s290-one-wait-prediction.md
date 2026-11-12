# s290 prediction: one wait over files, sockets and signals

Committed on branch `s290` before any change to the spec, the runtime, the compiler or a row.
Base: wolf-lang trunk `991f56e1` (after 0.2.39), paired with lupin 0.1.61. This is the second
lane to carry the id s290: the first added sessions (`notes/s290-a-new-session.md`, #1089), so
this lane's files are `notes/s290-one-wait*.md` and `notes/s290-one-wait-evidence/`.

The contract this lane refines is wolf-std's `docs/sc82-one-wait.md`, section "Missing below
std" (wolf-std branch `sc82`, `ea8e952b`): three calls a terminal multiplexer written in wolf
needs and the runtime lacks. The measurements behind the shapes below are
`notes/s290-one-wait-evidence/probe/` (`ready.c`, its Linux and macOS outputs), committed before
this file.

## 1. What a program cannot do at the base

- **Wait on both tables.** `os_signal_wait_fds` takes fs handles (pty masters, pipes, a
  terminal) and a signal set; `net_wait` takes net handles (listeners, streams) and no signal.
  No call takes both, so a server with pty masters, a Unix listener and its clients, a timer and
  `CHILD` must time-slice between two waits.
- **Learn every ready entry.** `os_signal_wait_fds` answers the lowest ready index, so a busy
  low entry starves the others unless the caller re-asks past it (wolf-std's sweep: k + 1 waits
  for k ready entries). A meaning hides the ready entries. A hangup reads as "ready", nothing
  more. An empty signal set is `io`.
- **Hold a set across waits.** Every wait sends the whole list again and duplicates every
  descriptor: O(N) per wait for N entries, however few are ready.
- **Write to a socket without parking.** `net_write` and `net_write_bytes` drain the whole
  buffer, parking between chunks; there is no count of what one send took. (A pty master and a
  pipe have it: `fs_set_nonblock` + `fs_write_some`.)
- **Know who connected.** No call reads a Unix-domain peer's credentials.

## 2. Measured before any change (black box, `probe/ready.c`)

| descriptor and state | Linux 7.2.4: poll / epoll | macOS 26.4.1: poll / kqueue | PAX (read, §3) |
|---|---|---|---|
| pty master, no slave ever opened | OUT / OUT | OUT / WRITE | OUT |
| pty master, last slave closed | OUT HUP / OUT HUP; read EIO | IN HUP / READ EOF, WRITE EOF; read 0 | HUP; read EIO |
| pty master, slave wrote then closed | IN OUT HUP; read 2 then EIO | IN HUP; read 0 (the bytes went with the close) | — |
| pipe read end, writer gone | HUP / HUP | IN HUP / READ EOF | HUP |
| pipe write end, reader gone | OUT ERR / OUT ERR | HUP / WRITE EOF | OUT ERR |
| unix stream, peer `SHUT_WR` | IN OUT / IN OUT RDHUP | IN HUP / READ EOF | IN RDHUP |
| unix stream, peer closed | IN OUT HUP / + RDHUP | IN HUP / READ EOF, WRITE EOF | IN RDHUP HUP |
| tcp stream, peer closed | IN OUT / IN OUT RDHUP | IN HUP / READ EOF | — |
| unix listener, one pending | IN / IN | IN / READ data=1 | IN |
| regular file at its end | IN OUT / `epoll_ctl` EPERM | IN OUT / READ not answered | EPERM in epoll |
| `/dev/null` | IN OUT / EPERM | NVAL (select: both) / EINVAL | EPERM in epoll |
| `/dev/tty` (a pty slave) | OUT / OUT | NVAL (select: write) / EINVAL | the console: EPERM in epoll |
| a nonblocking send to a unix stream of 1 MiB | took 219264 | took 8192 | 4096 a direction (its notes) |
| `SO_PEERCRED` / `LOCAL_PEERCRED` | stream: pid, uid, gid; a listener answers too (its own creds), an unconnected tcp socket pid 0 uid -1 | stream: uid, groups[0]; `LOCAL_PEERPID` the pid; a listener ENOTCONN; a tcp socket answers 0 | any socket |
| the set's own descriptor | pollable (IN) | pollable (IN) | pollable (IN) |
| a registered fd closed, a dup alive | epoll still answers it | kqueue dropped it | as Linux |
| the set in a fork's child | usable (shared object) | EBADF (not inherited) | shared |

Three facts decide the shape. (1) Hangup has no single host meaning: Linux reports a socket
peer's close as RDHUP (only if asked) and a pipe reader's loss as ERR; macOS reports both as HUP
or EOF. (2) Each kernel set refuses descriptors the program can hold: epoll every regular file,
`/dev/null` and, on PAX, the console; kqueue a terminal opened by path and `/dev/null`; and
kqueue's read filter on a regular file answers only before the end, where `poll(2)` answers
always. (3) epoll keeps a registration while any descriptor of the open file lives (a child's
copy included), so only the runtime, which owns both tables, can make a close mean "gone".

## 3. PAX's side (read at pax trunk `309036e`, not run)

PAX runs the static Linux binary. Its kernel (`kernel/ready/ready.lu`) serves `poll` (7),
`epoll_create1` (291), `epoll_ctl` (233) and `epoll_wait` (232); `epoll_pwait` (281) is ENOSYS
and the C library falls back to `epoll_wait`, as the runtime's reactor already does there.
Readiness is computed at the wait from `kernel/files`' per-kind rules: a pty master is HUP once
every slave descriptor ever opened is closed (`kernel/pty/pty.lu`'s `slave_gone`), a pipe's read
end HUP when no writer is left, its write end OUT|ERR when no reader is left, a Unix stream
RDHUP when nothing more will come and HUP when neither direction will move. Level-triggered by
default; EPOLLRDHUP is askable. Refused in an epoll object: a regular file, a directory, the
console and `/dev/null` (EPERM). Sixteen epoll objects in the whole system (the runtime's
reactor holds one per process that parks a net call) and 256 registrations each; past them,
ENFILE and ENOSPC. `SO_PEERCRED` (17) answers on every kind of socket (`kernel/socket`,
`kernel/files`' `peercred`): a connected one its peer's pid and effective ids. `sendto` honours
`MSG_DONTWAIT` and `O_NONBLOCK`, and `MSG_NOSIGNAL`.

## 4. The shapes, each decision with its recommendation (chosen and continued)

The signatures are sc82's, unchanged:

```text
os_wait_ready(List[int], int, int) -> (List[int], int) ! {invalid, io, unsupported}
os_ready_open() -> int ! {io, unsupported}
os_ready_set(int, int, int, int) -> () ! {invalid, io, unsupported}
os_ready_wait(int, int, int) -> (List[int], int) ! {invalid, io, unsupported}
os_ready_close(int) -> () ! {io}
net_write_some(int, List[byte]) -> int ! {busy, closed, invalid, io}
net_peer_cred(int) -> (int, int, int) ! {io, unsupported}
```

- **R-s290-1, the lane id.** The orchestrator's id is kept; the files are named apart from the
  first s290's (`s290-one-wait*`). Branch `s290` is free on origin (the first lane's was deleted
  at its merge).
- **R-s290-2, where the clauses live.** (A) `[os.ready.wait]`, `[os.ready.set]`,
  `[os.ready.facts]` and the design note `[os.ready.note]` in §1, after
  `[os.signal.wait.write]`, the wait family they extend; `[os.net.write.some]` and
  `[os.net.peer.cred]` in §3. (B) A new section. **Chosen: A.**
- **R-s290-3, how a meaning reaches a kernel-held set.** (A) The hub's wake pipe registered in
  the set itself (once, at the first wait with a non-empty `set`; removed again by a wait with
  `set` 0, so a meaning another wait has not yet acknowledged never spins it), so a wait with no
  polled entries is one `epoll_wait` or `kevent`. (B) A `poll(2)` over the set's descriptor and
  the pipe every wait, then a harvest: two calls a wake. **Chosen: A.**
- **R-s290-4, a descriptor the kernel set will not hold.** (A) Kept beside the set as a *polled*
  entry: every regular file and directory (decided by `fstat` at the add, so the answer is
  `poll(2)`'s on every host: always ready), and whatever `epoll_ctl` refuses EPERM or kqueue
  EINVAL (`/dev/null`; PAX's console; macOS's `/dev/tty`). A wait with polled entries is one
  `poll(2)` over the set's descriptor and them (`select(2)` where poll answers POLLNVAL, the
  descriptor waits' rule) and one harvest: O(ready + polled). (B) Refused, `io`: a multiplexer
  on PAX or macOS could not wait on its own terminal. (C) A helper thread per refused entry.
  **Chosen: A.**
- **R-s290-5, a handle closed while in a set.** (A) `fs_close` and `net_close` remove it from
  every set of this process before the descriptor closes (sc82's rule); a program that names no
  set pays one atomic load a close. (B) Each set holds a duplicate: the program's close would
  then not close the pipe or the socket. (C) The next wait answers `io`. **Chosen: A.**
- **R-s290-6, the answer's order.** The one-shot answers in list order; the kernel-held set in
  the kernel's order, promised to none (sc82). Each entry at most once a wait.
- **R-s290-7, a meaning with ready entries.** Both in one answer (sc82): a wait that takes a
  meaning looks at the entries once more with no park and answers what is ready. The narrow
  `os_signal_wait_fds` keeps its answers.
- **R-s290-8, hangup and error.** Reported as each host's wait says, and named per host in
  `[os.ready.facts]`: hangup (4) is POLLHUP or POLLRDHUP (the runtime asks RDHUP for every read
  interest on Linux and PAX), kqueue's EV_EOF; error (8) is POLLERR, kqueue's EV_ERROR or an
  EV_EOF carrying a socket error. A hangup of the reading side (RDHUP; EV_EOF on the read filter)
  comes with read (1); a hangup of both sides (POLLHUP) and an error come with every interest the
  entry asked (sc82's rule, so a loop that tests read alone reads and meets the end). Where the
  host's poll refuses a descriptor and `select(2)` answers, only read and write are known.
- **R-s290-9, the spelling of a bad set.** `io`, `os_signal_wait_fds`'s; std decides `invalid`
  itself before the call (R-sc82-7), so nothing reaches a std caller either way.
- **R-s290-10, windows.** Every M1 call answers `unsupported` by name (`os_ready_close` answers
  `io`, every number being forged there). sc82 asked for the one-shot over net handles alone by
  `WSAPoll`; this lane has no Windows machine to measure facts on, and std keeps `net.wait` for a
  sockets-only waiter there. Reported to wolf-std as a narrowing (no signature moves).
  `net_write_some` is served on windows (its sockets live non-blocking); `net_peer_cred` is
  `unsupported`.
- **R-s290-11, the `os_ready_set` details.** Interest 0..3, a key 0 or more, else `invalid`,
  before anything else; then windows `unsupported`; then a forged, closed or inherited-by-fork set,
  a forged or closed handle (an element with bit 62 names none), and the removal of a handle the
  set does not hold, `io`; a host refusal (PAX's 257th registration) `io` with its number in
  `os_error()`. A set made before `os_fork` answers `io` to the child's set and wait; the child's
  close frees its copy.
- **R-s290-12, a wait nothing could end.** `os_wait_ready([], 0, -1)` and `os_ready_wait` on a
  set with no entries, `set` 0 and -1 are `io`, decided at the call (`net_wait`'s and
  `os_signal_wait`'s rule).
- **R-s290-13, `net_write_some`.** One send, never a park: on the reactor hosts (linux, macOS,
  windows) the stream already lives non-blocking and the send is the runtime's own write step;
  elsewhere on unix `send(2)` with `MSG_DONTWAIT`. `busy` keeps the host's EAGAIN in `os_error()`
  (`fs_write_some`'s rule, not the net budget's 0). An empty list answers 0 with the handle
  checked and no send. `net_deadline` does not apply. `closed` for EPIPE and ECONNRESET with no
  SIGPIPE, `invalid` for a list of the wrong width, `io` for a listener, a forged or closed handle
  and the rest.
- **R-s290-14, `net_peer_cred`.** The effective ids the host recorded at the connection, and its
  pid (-1 where the host has none). The kind is the runtime's check, before the host is asked
  (Linux answers a listener and macOS a TCP socket, measured). freebsd: `getpeereid` and pid -1.
- **R-s290-15, capabilities.** `os_wait_ready` and the four `os_ready_*` charge `exec`, with
  `os_signal_wait_fds`; `net_write_some` and `net_peer_cred` charge `net`. All seven are refused
  at comptime and refused by name on the freestanding target.
- **R-s290-16, before `main`.** A program that names `os_wait_ready` or `os_ready_wait` makes the
  signal hub and the shared wake pipe before `main`, as one naming `os_signal_wait_fds` does.
- **R-s290-17, the checked machine.** One-shot and set on its own tables: the set is a list in the
  machine, each wait a `poll(2)` over duplicates of its entries (O(N), stated); `net_write_some`
  one `send` with `MSG_DONTWAIT`; `net_peer_cred` the same options.
- **R-s290-18, lupin.** Refuses M1 and M3 by name (sc82); mirrors M2 if wolf-interp can do it
  in a small change, else refuses it by name too and a lupin lane mirrors it.

## 5. What each row will show (red first)

1. `corpus/os/ready_rows.lu` (every host; windows answers its own side): red at the base by
   E0301 on every machine; green after. Rows: the one-shot over a pipe with a byte, both ends
   (`[0, 1, 1, 2]`); `set` 0 with deadline 0 (`([], 0)`); `([], 0, -1)` `io`; a deadline of -2
   `invalid`; a forged fs handle, a forged net handle, a negative element `io`; a set made, a
   pipe added under key 7, the wait `[7, 1]`, removed, the wait empty; interest 4 and key -1
   `invalid`; a removal of a handle never added `io`; a closed set `io`; `net_write_some` and
   `net_peer_cred` of a forged handle `io`.
2. `crates/wolf_driver/tests/ready_lanes.rs` with `fixtures/ready/` (unix; checked machine,
   native, release, linux also under `taskset` on four CPUs): red at the base (E0301, a build
   failure, never a skip), green after:
   - `mixed.lu`: two pty masters with bytes, a pipe, a Unix listener with a pending connection
     and two connected clients with bytes, all ready at once, every one reported, by both forms;
     a flooding pipe beside them never hides the others (every wait reports every ready entry);
     a set surviving add, modify and remove across waits; a handle closed while in the set
     leaves it.
   - `drain.lu`: a Unix stream filled with `net_write_some` until `busy`, a write wait that
     answers its deadline, the peer reads, the write wait answers write, the drain completes; a
     pty master the same with `fs_write_some`.
   - `hangup.lu`: a pty master whose child exits (the slave's last descriptor closed) answers read
     and hangup; a pipe whose writer is gone; a socket whose peer closed.
   - `meaning.lu`: a meaning raised before the wait with a pipe ready: one answer carries both;
     a sibling task raising during a park ends it; `set` 0 never takes a meaning.
   - `cred.lu`: a client dialed by this process answers this pid and its effective ids; native
     and release also a client in a forked child, its pid. Another uid: not measured (no host
     here lets a test change uid).
3. `wolf_rt` unit tests in the new module (both forms, the close rule, facts).
4. Counts: `[os.host.sigs]` 185 → 192, `prelude_names` functions and W0304 +7, `RT_SYMBOLS` +7,
   anchors +6, the lowering ledger +1 (`ready_rows` lowers).
5. **The plant** (`s290-plant-first`, pushed, no PR): both waits answer only the first ready
   entry, as the narrow form does. Red by name: the `mixed` rows on every machine that runs the
   runtime (native, release); the checked machine stays green (its own code). Predicted.
6. PAX: not run here. A PAX lane runs the `fixtures/ready/` binaries built as static Linux
   binaries; predicted green, with two differences stated in `[os.ready.facts]`: a pipe write end
   whose reader left answers 10 (write, error) as on Linux, and PAX's console is a polled entry.
