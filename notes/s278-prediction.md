# s278 — the macOS rows of s272: prediction, before the first change

Contract: the s278 lane contract and the lane rules it cites. Written 2026-11-03 on nomad,
before any change to the tree.

## Inputs, read
- Branch `s278` cut at origin/trunk `63c1278d` (s277). Both rows are unchanged since s272.
- CI's nightly run 41611100640 at `0d6b3a64`, job `native gauntlet (macOS aarch64, gates)`
  (runner image macos-26-arm64, macOS 26.6.2 25G83), step `corpus`:
  `corpus: 1247 file(s), 2 bad`, the two lines
  - `corpus/os/chroot_rows.lu: expected stdout "missing=not_found\nmissing_number=2\n…" but got
    "missing=denied\nmissing_number=1\n…"`;
  - `corpus/os/signal_action_rows.lu: expected stdout "…kill=0\nkill_number=0\nstop=0\n
    stop_number=0…" but got "…kill=io\nkill_number=22\nstop=io\nstop_number=22…"`.
  The linux job (`lint + test + corpus (linux x86-64)`) of the same run is green. s272's own
  PR ran `full-matrix`, where the macOS native gauntlet is skipped, so no PR run ran the corpus
  step on macOS.
- Spec 11 `[os.signal.action.one]` and `[os.proc.chroot]`, their host sentences:
  - the action query: "Darwin refuses the GET on `KILL` and `STOP` (`EINVAL`, 22, measured on
    nomad, 26.4.1, where linux answers the default for both)"; its witness sentence says the
    corpus row holds "KILL and STOP as the host answers" on every host;
  - the root: "linux looks the path up before it asks for the privilege, so a missing path is
    `not_found` to anyone; macOS asks first, so for a caller that is not the super-user every
    path is `denied` (`EPERM`)"; the corpus row is the witness "every host, as a user".
  So the spec already promises each host's own answer; only the rows' headers are linux's.
- s272's notes (`notes/s272-prediction.md` R-s272-5, R-s272-8, R-s272-9; `notes/s272-evidence/`
  m0-darwin and m1-hasu): the same split, measured.
- `crates/wolf_driver/tests/os_surface_lanes.rs`: `the_root_rows_on_every_host` and
  `the_signal_action_rows_on_every_host` hold each machine to its host's lines (macOS and
  windows written in the test, linux from the row's header). That is why they were green on
  macOS while the corpus step, which reads only the header, is red there.
- s277's six rows (`corpus/fs/*_at.lu` and others, `22948195`): the house shape for a row that
  answers differently by host — the host named by `os_name()`, the line a truth that holds on
  each side, `os.uname` in `conforms:`.
- Clean room: the Darwin man pages on nomad (sigaction(2), chroot(2)), the Linux man-pages
  project's sigaction(2) and chroot(2) (6.19), POSIX's sigaction(); black-box runs. No libc or
  kernel source.

## The ruling (R-s278-1, recommended and implemented)
- **R-s278-1, what the two calls answer on macOS.**
  - (A) As the host does: Darwin's `EINVAL` on the GET of `KILL` and `STOP` is `io` with 22;
    Darwin's unprivileged `chroot` is `EPERM`, `denied` with 1, whatever the path. The spec
    says so per host (it already does), and the rows hold each host to its side by `os_name()`.
  - (B) The runtime makes macOS match linux: answer 0 for `KILL` and `STOP` without asking
    (the runtime would claim a disposition the host refused to report), and look a chroot path
    up (`stat`) before asking, to make a missing path `not_found` (a second call, a race
    between the two, and an answer the host never gave). It changes behaviour on one host, the
    spec clauses, the checked machine (which asks the host for the `wolf` process) and the
    mirror lupin owes (wolf-interp#315), and it goes against what GNU `env` on Darwin reports
    (`failed to get signal action for signal 9`), the reason the call exists (#975).
  - **Recommend A.** The spec promises A and not B: no behaviour, no clause, and nothing on the
    checked machine or lupin's pins moves. lupin 0.1.58 does not resolve either call (pinned by
    version in `os_surface_lanes.rs`) and stops at the row's first call, before `os_name`.

## What each change will show
1. **The base, measured on nomad (Darwin 26.4.1 arm64), wolf built at `63c1278d`:**
   - the corpus step `1247 file(s), 2 bad`, the two lines exactly as CI's (above);
   - each row on every machine: the corpus step's native leg, the checked machine, the native
     and the release binaries all answer `kill=io kill_number=22 stop=io stop_number=22`, and
     `missing=denied missing_number=1` with `dot=denied dot_number=1`; lupin 0.1.58
     `unsupported`, "does not resolve" `os_signal_action_one` and `os_chroot`;
   - the C black box (`probe.c`), as the lane's user: `sigaction(SIGKILL, NULL, &old)` and
     `sigaction(SIGSTOP, NULL, &old)` -1 `EINVAL` (22); every set (`SIG_IGN`, `SIG_DFL`, a
     handler) on either -1 `EINVAL`; `SIGTERM`'s query 0, `SIG_DFL`; `chroot` on the row's
     missing path, an absolute missing path, `.` and a regular file: -1 `EPERM` (1) for each.
2. **linux, the same probe** in a throwaway r41-toolchain container on hasu block B as uid 1000
   (no `CAP_SYS_CHROOT`): the queries 0 and `SIG_DFL`; every set -1 `EINVAL`; `chroot` on a
   missing path -1 `ENOENT` (2), `.` -1 `EPERM` (1), a file -1 `ENOTDIR` (20).
3. **The rows, fixed** (one commit each): `corpus/os/signal_action_rows.lu` prints
   `kill: true`, `kill_number: true`, `stop: true`, `stop_number: true` where the answer is the
   host's (`io` and 22 on macOS, by `os_name`; 0 and 0 elsewhere); `corpus/os/chroot_rows.lu`
   prints `missing: true`, `missing_number: true` (`denied` and 1 on macOS, `unsupported` and 0
   on windows, `not_found` and 2 elsewhere). Every other line of both headers is unchanged;
   `os.uname` joins both `conforms:`. On nomad the corpus step reads `1247 file(s), 0 bad`; on
   hasu (linux) `0 bad` as well.
4. **`os_surface_lanes.rs`** follows the rows: macOS reads the header (the row now holds the
   host's side itself), windows' chroot line is `missing: true\nmissing_number: true\n
   dot=unsupported\ndot_number=0`, windows' action line is unchanged (that row stops before
   `KILL` there). Both tests green on nomad (checked, native, release; lupin pinned) and on
   linux.
5. **The snapshots** of the two rows move (wolf_lex `corpus_snapshots`, wolf_parse
   `corpus_decls`: the byte spans); the lowering ledger does not (both still `lowers`). No other
   row's snapshot or expectation moves.
6. **The spec and the CHANGELOG** do not move under A: no behaviour changes and no clause
   changes, so no anchor moves either (`spec-extract --check` green, the anchors untouched).
7. **The plant** (`s278-plant-darwin-kill`, one commit, one file, one hunk on the final head):
   the macOS branch of the action row expects linux's row (`"0"` for `io`). On nomad the corpus
   step reads `1 bad`, `kill: false` and `stop: false` in the got line, red by name as
   `FAIL corpus/os/signal_action_rows.lu`.
8. **The macOS record** at the final head (the org's gauntlet recorder, behind the Mac lock):
   the gates shard's fifteen steps in `cargo xtask ci`'s order (corpus first), plus the steps
   of the build shard that read the changed files (`fmt`, `fmt-fixpoint`, the two crates'
   corpus snapshot tests, the lowering ledger, the two `os_surface_lanes` rows, clippy on that
   test): every `END … exit=0`. bench-gates keeps its named macOS skip.
