# s278 — s272's rows on macOS: `os_signal_action_one` for KILL and STOP, `os_chroot` on a missing path

The nightly's macOS native gauntlet was red at the corpus step by two of s272's rows, whose
headers were linux's answers. The spec already promised each host's own answer, so the rows now
hold each host to its side by `os_name()`, as s277's rows do. No behaviour, no spec clause and no
anchor moves.

## Forbidden

Footprint: cross-cutting

The diff is the two corpus rows, their lexer and parser snapshots, the two
`os_surface_lanes` rows that follow them, and notes. It touches no runtime, codegen, loader,
spec, CI or pin. It is declared `cross-cutting` because the macOS native gauntlet, the job that
was red, runs only in `tier release` (the `release-matrix` label), and the contract names that
tier for the merge. No other row's expectation changes, and no other builtin's behaviour or name.
Clean room: the Darwin man pages on nomad, the Linux man-pages project's sigaction(2) and
chroot(2) (6.19), POSIX's `sigaction()`, and black-box runs; no libc or kernel source. No label,
merge, tag, release, issue filed or closed, and no attribution trailer. The plant is pushed
without a PR. Nothing ran under `/tmp` or `/var/folders`: TMPDIR is the lane's own.

## Inputs, verified

- CI's nightly run 41611100640 at `0d6b3a64`, job `native gauntlet (macOS aarch64, gates)`
  (macos-26-arm64, macOS 26.6.2 25G83), step `corpus`: `1247 file(s), 2 bad`.
  - `corpus/os/signal_action_rows.lu` expected `kill=0`, `kill_number=0`, `stop=0`, `stop_number=0`
    and got `kill=io`, `kill_number=22`, `stop=io`, `stop_number=22`.
  - `corpus/os/chroot_rows.lu` expected `missing=not_found`, `missing_number=2` and got
    `missing=denied`, `missing_number=1`.
  - The same run's linux job is green. s272's PR ran `full-matrix`, where the macOS native
    gauntlet is skipped, so no PR run had run these rows through the corpus step on macOS.
- Spec 11 `[os.signal.action.one]`: "Darwin refuses the GET on `KILL` and `STOP` (`EINVAL`,
  22 …, where linux answers the default for both)"; its witness, the corpus row, holds "KILL and
  STOP as the host answers" on every host. `[os.proc.chroot]`: "linux looks the path up before it
  asks for the privilege, so a missing path is `not_found` to anyone; macOS asks first, so for a
  caller that is not the super-user every path is `denied` (`EPERM`)"; its witness is the corpus
  row, "every host, as a user". So the spec says what each host answers, and the rows' headers
  said only linux's.
- `notes/s272-prediction.md` (R-s272-5, R-s272-8, R-s272-9) and `notes/s272-evidence/`
  (m0-darwin, m1-hasu): the same split, measured by s272.
- `crates/wolf_driver/tests/os_surface_lanes.rs`: `the_root_rows_on_every_host` and
  `the_signal_action_rows_on_every_host` wrote macOS's and windows' lines in the test and read
  linux's from the header. That is why they were green on macOS while the corpus step, which reads
  only the header, was red.
- s277's six rows (`22948195`): the house shape for a row that answers by host. The host is named
  by `os_name()`, each line is a truth that holds on each side, and `os.uname` is in `conforms:`.
- The man pages. Darwin's sigaction(2) lists `EINVAL` for "an attempt is made to ignore or supply
  a handler for SIGKILL or SIGSTOP" and for setting `SIG_DFL` on one; it says nothing of a query.
  Darwin's chroot(2) lists no `EPERM` at all ("This call is restricted to the super-user").
  Linux's sigaction(2) says `SIGKILL` and `SIGSTOP` cannot have their disposition changed and that
  a null new action inspects the current one; its chroot(2) lists `EPERM` (no `CAP_SYS_CHROOT` in
  the caller's user namespace) and `ENOENT`. POSIX says a null `act` leaves the handling unchanged
  and "enquires" about it, and gives `EINVAL` for catching or ignoring a signal that cannot be.
  Neither OS's page gives the order of `EPERM` and `ENOENT`; the measurement does.
- README read, current: "the two tiers agree on every corpus program that runs, on linux x86-64
  and macOS aarch64" was true of the tiers throughout (they agreed with each other on macOS; the
  header did not), and nothing else it states moves.

## Prediction

`notes/s278-prediction.md` (`77cfcb1a`, before the first change) held, with one count wrong:
- at `63c1278d` the corpus has 1253 files, not 1247 (s277 added six rows after CI's
  `0d6b3a64`). The corpus step on nomad read `1253 file(s), 2 bad`, with the two lines exactly
  as CI's;
- each row on every machine at the base, as predicted. The corpus step's native leg, the checked
  machine, and the native and release binaries all answered `kill=io kill_number=22 stop=io
  stop_number=22` and `missing=denied missing_number=1` beside `dot=denied dot_number=1`.
  lupin 0.1.58 answered `does not resolve` for `os_signal_action_one` and `os_chroot`;
- the black box, as predicted on both hosts (`notes/s278-evidence/probe/`). Darwin 26.4.1: the
  queries of `SIGKILL` and `SIGSTOP` are -1 `EINVAL` (22), and so is every set; `SIGTERM`'s query
  is 0 (`SIG_DFL`); every unprivileged `chroot` is -1 `EPERM` (1), the two missing paths and a
  regular file included. linux 7.2.4 (uid 1000, CapEff 0): the queries are 0 (`SIG_DFL`) and
  every set -1 `EINVAL`; `chroot` on a missing path is -1 `ENOENT` (2), `.` -1 `EPERM`, and a
  file -1 `ENOTDIR` (20);
- the formatter re-lays the chroot row's three-way host line (`d2f4672f`); the action row was
  already in its form;
- the lexer's and parser's snapshots of the two rows move (byte spans); the lowering ledger does
  not.

**R-s278-1, what the two calls answer on macOS.** (A) As the host does. Darwin's `EINVAL` on the
GET of `KILL` and `STOP` is `io` with 22, and Darwin's unprivileged `chroot` is `EPERM`, which is
`denied` with 1, whatever the path. The spec says so per host (it already did), and the rows hold
each host to its side by `os_name()`. (B) The runtime makes macOS match linux: it answers 0 for
`KILL` and `STOP` without asking, and looks a chroot path up before it asks. That claims a
disposition the host refused to report. It adds a second call, with a race between the two, and
gives an answer the host never gave. It changes the spec, the checked machine (which asks the
host for the `wolf` process) and the lupin mirror (wolf-interp#315), and it contradicts what GNU
`env` on Darwin reports (`failed to get signal action for signal 9`), the reason the call exists
(#975). **Recommended and implemented: A.** The spec promised A and not B, so no behaviour, no
clause, no anchor and no CHANGELOG line moves. The checked machine answers the host's, as natively.
lupin 0.1.58 still stops at the row's first call (`os_signal_action_one` or `os_chroot`), before
`os_name`, so its pins are unchanged.

## Evidence index

`notes/s278-evidence/index.txt` lists every file:
- `ci/`: the nightly's runner and the corpus step's end;
- `probe/`: `probe.c` and its runs on Darwin 26.4.1 arm64 and linux 7.2.4 x86-64;
- `nomad/mac-job1-63c1278d.log`: the base on nomad. It holds the probe, each row on every
  machine, and the corpus step red by the two rows;
- `hasu/`: the linux pre-check at the fixed rows (each row on every machine, the two
  `os_surface_lanes` rows 2/2), the formatter's check, and the snapshots;
- `rig/`: the scripts as run.

The rows, at the head:
- `signal_action_rows.lu` prints `kill: true`, `kill_number: true`, `stop: true` and
  `stop_number: true` where the answer is the host's: `io` and 22 on macOS (by `os_name`), 0 and
  0 elsewhere. windows stops at `term`, as before.
- `chroot_rows.lu` prints `missing: true` and `missing_number: true` where the answer is the
  host's: `denied` and 1 on macOS, `unsupported` and 0 on windows, `not_found` and 2 elsewhere.
  `dot=denied dot_number=1` is unchanged.
- Every other line of both headers is unchanged, and `os.uname` joins both `conforms:`.
- `os_surface_lanes.rs` reads both headers on macOS and linux. windows' chroot line is the row's
  new form (`missing: true\nmissing_number: true\ndot=unsupported\ndot_number=0`), and its action
  line is unchanged.

The records at the final head live outside the tree, under the lane's directory, because a
record cannot predate the head it names. The PR body quotes every `END` line. There are three:
- the macOS record (nomad, behind the Mac lock, two takes). Its `rows` chunk is `fmt`,
  `fmt-fixpoint`, the three crates' corpus snapshot tests, the two `os_surface_lanes` rows, and
  clippy on that test. Its `gates` chunk is the gates shard's fifteen steps in `cargo xtask ci`'s
  order, corpus first;
- the linux record (hasu block B, `localhost/r41-toolchain:1`, a foreign pid 1, the taskset
  shim). It runs the build, `cargo xtask ci`'s corpus step and the two `os_surface_lanes` rows;
- the plant record (nomad). `s278-plant-darwin-kill` is one commit, one file and one hunk on the
  final head: the macOS branch of the action row expects linux's row (`"0"` for `io`). It must be
  red by name as `FAIL corpus/os/signal_action_rows.lu`.

## Done-when

R-list, recommendations implemented:
- **R-s278-1:** (A) each host's own answer, the spec per host and the rows branching on
  `os_name()`; (B) the runtime makes macOS match linux. **Recommend A**, implemented. The spec
  already promises A; nothing on the checked machine or in lupin's pins moves.

Test targets:
- [x] the prediction committed before the first change (`77cfcb1a`)
- [x] both rows red at `63c1278d` on macOS with CI's exact lines (`nomad/mac-job1-63c1278d.log`)
- [x] the C black box on Darwin and linux, from the man pages and the measurement
- [x] each row on every machine, linux, at the fixed rows (`hasu/precheck1-433ec385.log`)
- [x] `wolf fmt --check` of both rows; the snapshots regenerated at the formatter's form
- [ ] the macOS record at the final head, every `END … exit=0` (in the PR body)
- [ ] the linux record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name on macOS (in the PR body)
- [ ] the audit, once
- [x] README read, current

No issue to file or close. wolf-interp#315 (the lupin mirror) is unchanged.
