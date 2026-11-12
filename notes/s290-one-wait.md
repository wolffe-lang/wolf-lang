# s290: one wait over files, sockets and signals

The second lane with the id s290 (the first added sessions, `notes/s290-a-new-session.md`).
Base: trunk `991f56e1`, after wolf 0.2.39, paired with lupin 0.1.61. Prediction:
`notes/s290-one-wait-prediction.md` (`f35e36a0`), committed before any change. The contract
refined here is wolf-std's `docs/sc82-one-wait.md`, section "Missing below std" (wolf-std branch
`sc82`, `ea8e952b`): three calls a terminal multiplexer written in wolf needs.

## Prediction

The shapes, the decisions and the rows were written down first (`f35e36a0`), after a black-box
probe of poll, epoll and kqueue on Linux and macOS (`24d1e30a`,
`notes/s290-one-wait-evidence/probe/`). The signatures are sc82's, unchanged:

```text
os_wait_ready(List[int], int, int) -> (List[int], int) ! {invalid, io, unsupported}
os_ready_open() -> int ! {io, unsupported}
os_ready_set(int, int, int, int) -> () ! {invalid, io, unsupported}
os_ready_wait(int, int, int) -> (List[int], int) ! {invalid, io, unsupported}
os_ready_close(int) -> () ! {io}
net_write_some(int, List[byte]) -> int ! {busy, closed, invalid, io}
net_peer_cred(int) -> (int, int, int) ! {io, unsupported}
```

How the prediction held, item by item (section 5 of the prediction):

1. The corpus rows and `ready_lanes.rs` red at the base: E0301 at `wolf build` and at the
   checked machine's resolve. Right (the red take's record is cited in the PR).
2. The rows green after: right, with three defects of the rows' own, found on macOS and fixed
   before any Linux take (`53e73eb8`): the spawn's map is `[target, source, …]`, not the
   reverse; a forked client must stay connected while its credentials are asked on macOS (see
   below); and the checked machine's blocking socket parked a send on macOS (below).
3. The counts: right (host table 192, prelude 203 functions, W0304 235, runtime symbols 285,
   anchors 748, the lowering ledger +1).
4. The plant: its record is cited in the PR (Seen red, below).
5. Three facts the prediction did not have, now in the spec. On Linux, a pty master that has
   answered `busy` can be ready to write again a moment later (`probe/ptyfull.c`: 11776 bytes,
   `busy`, ready, 3584 more, `busy` and not ready; macOS: 1022, `busy`, not ready): the first
   Linux take's drain row assumed a write wait after `busy` would answer its deadline, and now
   writes and waits until one does (`6a48bb58`). And two measured on macOS 26.4.1:
   **macOS's `send` on a blocking Unix-domain stream parks whatever `MSG_DONTWAIT` says**
   (`probe/dontwait.c`: 8192 bytes, then parked until the alarm; Linux answers EAGAIN), so the
   send is made on a non-blocking descriptor; and **macOS's `LOCAL_PEERPID` answers only while
   the peer is connected** (`probe/peerpid.c` and the `fork` row: a child's pid while it is
   connected, -1 after it closed; `LOCAL_PEERCRED`'s ids and Linux's `SO_PEERCRED` survive the
   close).

## What changed

- `spec/11-os.md`: `[os.ready.wait]`, `[os.ready.set]`, `[os.ready.facts]` (the facts each host
  answers, measured), `[os.ready.note]` (why two forms, what was left out),
  `[os.net.write.some]`, `[os.net.peer.cred]`; seven lines in the host table (185 to 192);
  `[os.signal.hub]` and `[os.proc.fork]` name the new calls. `spec/anchors.json` 742 to 748 (the
  six new anchors; none dropped or changed, diffed both ways), `spec/prelude.json` seven names.
- `wolf_rt`: `ready.rs` (the one-shot over a duplicate of each element and `poll(2)`; the set
  over epoll on Linux and kqueue on macOS, polled entries beside it, the wake pipe registered in
  the kernel set; `forget` from `fs_close` and `net_close`); `signal.rs` `wait_looking`, the
  meaning half around a caller's look (the registration protocol of `wait_fds`, unchanged; set 0
  asks for no hub); `net.rs` `write_some`, `peer_cred`, their shims and the close hook.
- The checked machine (`wolf_mem::ubcheck`): all seven on its own tables; a set is a list it
  polls entry by entry.
- sema (prelude, signatures, sandbox categories), WIR lowering, the clif symbol table and the
  before-`main` hub lists; the capability rows; the freestanding refusals; the snapshots;
  `CHANGELOG.md` under Unreleased.

### Decisions (R-s290-N), each chosen and continued

R-s290-1 to R-s290-18 are the prediction's, implemented as recommended, with these moves:

- **R-s290-10 (windows) and the tier-2 hosts.** Every M1 call answers `unsupported` by name on
  any host but Linux and macOS: windows, and freebsd, whose runtime has no signal hub and no
  task layer (`os_signal_wait_fds` is not served there either). The prediction named kqueue for
  freebsd; the runtime cannot serve the meaning half there, so the spec says so.
- **R-s290-13 (the send).** One `send(2)` with `MSG_DONTWAIT` (and `MSG_NOSIGNAL` on Linux and
  freebsd; std sets `SO_NOSIGPIPE` on macOS) on every unix host, on a descriptor that is
  non-blocking for the send: the runtime's streams are on Linux and macOS; a blocking socket is
  made so for the one send and put back (the checked machine's, and a tier-2 host's). The
  prediction said the reactor hosts would use the stream's own write step; `send` with the flags
  is the same one call and does not depend on how std writes a Unix stream.
- **R-s290-19, macOS's pid after the peer has closed.** (A) -1, the host giving none (the
  clause's own rule); (B) `io`. **Chosen: A**: the ids are still the host's answer, and a server
  that needs the pid reads it at the accept.
- **R-s290-20, the plant.** (A) The shims' answer cut to its first pair, one hunk in
  `ready.rs`: the narrow form's lowest-ready answer. **Chosen: A.**

### What wolf-std (sc82) is told

No signature moved. Two narrowings, each a behaviour sc82's contract left open: windows answers
`unsupported` for `os_wait_ready` whatever the elements (sc82 asked for net handles alone by
`WSAPoll`; std keeps `net.wait` for a sockets-only waiter there), and a non-empty set that is not
a set of meanings is `io` (sc82: std decides `invalid` itself, R-sc82-7). macOS's pid of a closed
peer is -1.

## Seen red

- The rows were committed red before any implementation (`f4cce536`, `f004123c`, `489cc4b2`,
  `7e404485`); the red take at `f71426b4` names each red test (record cited in the PR).
- The first Linux gauntlet, at `d25a5ee8`, was red in three places, each fixed before the take at
  the head: rustfmt (`16131aa0`), `wolf fmt` of the corpus rows (`1c390cba`, snapshots
  `2fe603bd`) and the pty drain row above (`6a48bb58`). Every other step was green there.
- macOS (this lane's runs at `af9fb070` and `53e73eb8`, single builds): `ready_lanes` 10 of 14
  green at the first, the four reds the row defects and the two macOS facts above; 14 of 14 at
  `53e73eb8`; `wolf_rt`'s ready, net and wait unit tests 17 of 17; clippy `-D warnings` on the
  touched crates; `cargo check` of `wolf_rt` for Linux, freebsd and windows.
- The plant `s290-plant-first` (pushed, no PR): the answer cut to its first pair; red by name in
  its record (cited in the PR).

## Not measured

- **PAX**: read from its source (pax trunk `309036e`), not run. A pax lane runs these rows as
  static Linux binaries (below).
- **windows and freebsd**: compiled only (`cargo check`); the refusals are by construction.
- **A client under another uid**: no test host here lets a test change its uid; the rows hold the
  same uid and a forked child's pid.
- **The container rows** of CI's linux job: this rig runs none (no engine); they skip loudly in
  the local gauntlet and run in CI.
- **lupin**: not changed here (below).

## Done when

The gauntlet at the head is green, the plant red by name, the PR's tier full green (cross-cutting:
the spec and the runtime), and a release pairs wolf 0.2.40 with a lupin that resolves the seven
names.

## For a pax lane

Build each fixture of `crates/wolf_driver/tests/fixtures/ready/` (`mixed`, `drain`, `hangup`,
`meaning`, `cred`, `fork`) and `corpus/os/ready_rows.lu` as a static Linux binary with this head's
`wolf build`, copy them to PAX, run each in a scratch directory, and compare stdout with the
lines `ready_lanes.rs` expects on Linux. Predicted from PAX's source: green, with PAX's console a
polled entry (epoll refuses it EPERM), a pipe's write end whose reader left answering 10 (write,
error) as on Linux, and `os_ready_open` answering `io` (ENFILE) past sixteen epoll objects in the
whole system (the reactor holds one per process that parks a net call). `drain` writes to a Unix
stream PAX holds 4096 bytes of and to a pty master holding 20480: the rows print no count.

## For lupin (wolf-interp)

lupin 0.1.61 does not resolve the seven names: a program naming one is refused, "does not
resolve" (`ready_lanes.rs`' `lupin_refuses_the_one_wait_by_name` holds it). The mirror, each name
resolved with the host table's row, answers:

- `os_wait_ready`: the shape first, as the compiler does — a non-zero set that is not a set of
  listenable meanings `io`, a deadline below -1 `invalid`, windows `unsupported`, no element with
  set 0 and -1 `io`, a negative element or one that names no live handle of either table (bit 61
  a net handle, bit 62 write interest) `io` — then refused by name: std has no `poll`.
- `os_ready_open`: windows `unsupported`, else refused by name (no kernel set, no `poll`).
  `os_ready_set`: interest outside 0..3 or a negative key `invalid`, windows `unsupported`, then
  `io` (no set exists on this machine, so every set number is forged). `os_ready_wait`: the set's
  shape `io`, a deadline below -1 `invalid`, windows `unsupported`, then `io`. `os_ready_close`:
  `io`.
- `net_write_some`: served: a listener, a forged or closed handle `io`; a list of the wrong width
  `invalid`; an empty list 0; one write on lupin's non-blocking socket, its count, `busy` for
  would-block, `closed` for a peer gone.
- `net_peer_cred`: windows `unsupported`; a TCP stream, a listener, a forged or closed handle `io`;
  a Unix stream refused by name (stable Rust has no call for it).

With that mirror the corpus rows stay refused by name on lupin (their first wait is
well-formed), which is what the lupin row accepts either way.

## For the release lane

wolf 0.2.40 carries the seven names; the changelog entry is under Unreleased. The pairing needs a
lupin that resolves them (above). wolf-std's sc82 retires its four `unsupported` combinations
without a signature change once its pin carries this release.
