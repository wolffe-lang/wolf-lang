# s290: a new session

A program could start a child in a new process group (`os_spawn_job`) and move itself into one
(`os_set_pgid`), but nothing in wolf could start a child in a new SESSION or move a process into
one. A server that forks a chain of processes per connection and runs the user's command in the
last left every process it made in the session it was started in: on PAX the console's, where a
command's `/dev/tty` was the console and its `kill 0` reached the server. This change adds three
builtins, `[os.proc.session]` and `[os.term.session]`:

- **`os_spawn_session(exe, args, map, defaults) -> int ! {denied, invalid, io, not_found,
  unsupported}`**: `os_spawn_job` with one step done first in the child, `setsid`, in place of the
  group and the terminal. The child's session id, group id and process id are one number and it
  has no controlling terminal. There is no `tty` parameter: a new session is handed no terminal.
- **`os_setsid() -> () ! {denied, io, unsupported}`**: this process into a session of its own;
  `denied` (with `os_error()` 1) for a process that leads a group, a session leader included.
- **`os_sid() -> int ! {unsupported}`**: `getsid(0)`, `os_pgid`'s shape.

The spawn and the move charge `exec`, the read `env`; all three are refused at comptime and by
name on the freestanding target, and windows answers `unsupported` after the shape. The checked
machine serves all three (`setsid` and `getsid` on the `wolf` process; the spawn through its own
spawn path). The design note `[os.proc.session.note]` records what was measured before the code
(`notes/s290-evidence/probe/`) and the decisions. The plan is `notes/s290-prediction.md`.

wolf-std's `process.start_session`, `lead_session` and `own_session` land at the pin of the
release that carries the builtins; until then they are on wolf-std's branch `s290`, recorded
against this head's wolf (below). lupin's mirror is wolf-interp's change ("For the lupin mirror").

## Forbidden

Footprint: cross-cutting

The diff touches the spec (`spec/11-os.md`, `spec/anchors.json`, `spec/prelude.json`), the
runtime (`wolf_rt` `os.rs`, `term.rs`), the checked machine (`wolf_mem` `ubcheck.rs`), sema, WIR,
the clif backend's symbol table, the driver's capability table, the corpus and tests, the
changelog and notes. There is no CI, pin or toolchain change, and no released answer moves:
`os_spawn_job`'s group -2 stays `invalid`.

Clean room: the spec, the compiler's own source, POSIX and the host manuals, black-box probes of
Linux and macOS (`notes/s290-evidence/probe/`), and black-box runs of the published lupin 0.1.60;
never another compiler's or interpreter's source, and no change to pax. No label, merge, tag,
release, publish or issue closed, and no attribution trailer. The plants are pushed without a PR.
No commit was made on a Linux host; the Linux rig's container runs an init as its pid 1 and calls
no `setsid` of its own; nothing ran under `/tmp`.

## Inputs, verified

- Base: trunk `bca5dbc5` (0.2.38), `crates/wolf_driver/STD-PIN` wolf-std `d4a4d4ca`,
  `crates/wolf_driver/PAIRING` lupin 0.1.60, read twice by the Linux rig's prep take against the
  staged archives' digests (`rig-linux/gauntlet-prep.sh`, `rig-linux/stage-inputs.sh`).
- The anchors: 733 to 736 (`os.proc.session`, `os.proc.session.note`, `os.term.session`); the
  regenerated file diffed against the base's both ways: none dropped, none changed.
- The host table 177 to 180 lines, `spec/prelude.json` 229 to 232 names (191 functions), the clif
  runtime symbols 270 to 273.

## Prediction

`notes/s290-prediction.md` section 5, item by item:
1. **The rows, committed first and red: right, with one defect of the rows' own.** The rows
   commit (`3111ce50`) carried a clippy error (`#[allow(dead_code)]` on `mod test_cpus` beside the
   module's own `#![allow]`), so its first dispatch stopped at `clippy` before any test; `f5007e3d`
   removes it. Rebuilt as `s290-red2` (the rows, that fix, and text only), every CI leg stopped
   red by name at the first failing test binary, `freestanding_target`'s refusals, E0301 for each
   of the three names on native and release (linux, macOS, windows: `ci-red.txt`). `cargo test`
   stops at the first failing binary, so the other rows red at base were not reached there. The
   rows also missed one count: `str_list_fs_native` named the three symbols but kept
   `RT_SYMBOLS.len()` at 270; the first Linux check met it (`linux-cheap-68f33aba.log`) and
   `15f808ae` moves it to 273.
2. **The witnesses: right.** `session_lanes` on the checked machine, native and release, on a
   pseudo-terminal the test opens (the Mac and Linux; the pinned rows on four CPUs in the Linux
   gauntlet's block); `corpus/os/session_rows.lu` on every machine; the freestanding target
   refuses each name; lupin 0.1.60 "does not resolve". windows' `unsupported` rows run at the
   PR's `tier full`.
3. **The plant: right.** With `setpgid(0, 0)` where the child's hook calls `setsid()`,
   `a_spawned_child_leads_a_new_session` goes red by name ("sid=pid false ... tty opened") and
   the three other tests stay green: on CI with both hooks planted (`ci-plant.txt`), and in the
   Linux record with the runtime's hook alone, one hunk on the final head (cited in the PR).
4. **PAX: right, with no change to pax.** A fresh clone of pax's trunk (`483bfabf`) and its own
   tools carried a static fixture built with this head's wolf (`pax/sessrows.lu`) into an
   initramfs, built `kernel/kmain_session.lu` with pax's pinned wolf 0.2.35, and booted it under
   KVM to `pax_halt`. As the program the kernel starts (a session's leader with the console):
   it leads its session, `os_setsid` is `denied` with 1; the child it starts with
   `os_spawn_session` reads its session, group and process ids one number and `/dev/tty`
   failing with 6; a forked child's `os_setsid` is `ok` and it reads its session id its pid
   after; its own session is unchanged (`pax-boot-68f33aba.log`, every row by line).

## Decisions

Decided after the prediction, before the code, each as the prediction recommended:
- The spawn is a sibling, `os_spawn_session`, with no `tty`; not a new `group` value on
  `os_spawn_job` (two released rows would change answer) and not a new parameter on it.
- `os_setsid` answers the unit; the new id is `os_pid()`. Not `TIOCNOTTY`, after which the process
  keeps its session and group; not a fork by the runtime for a group leader.
- `os_sid` reads this process's session only (`getsid(0)`); no call takes a host pid for a read.
- Threads: one system call on the process's own state, callable from any task; the restore at
  exit still works after it, and `os_term_set_foreground` on the old terminal answers `io`.
- wolf-std's three functions land at a release pin; the rows are recorded against the branch
  build meanwhile.
- `exec` for the spawn and the move, `env` for the read; refused at comptime; the freestanding
  target refuses each by name.
- The checked machine serves all three.
- lupin's mirror is wolf-interp's change, opened when this PR is up; this PR says exactly what it
  must answer.
- windows `unsupported` after the shape; the freebsd cross-build covers the two new C calls; the
  PAX row from a fresh pax clone with no change to pax.
- The spec says plainly that the runtime's opens pass no `O_NOCTTY`, so a session leader that
  opens a free terminal takes it (macOS included, measured).

## For the lupin mirror

lupin 0.1.60 does not resolve the three names (`session_lanes`'
`lupin_does_not_resolve_the_session_calls_before_its_mirror` pins that for 0.1.60). Its mirror
answers:
- **`os_spawn_session(exe, args, map, defaults)`**: `invalid` for every map `os_spawn_fds` calls
  invalid (an odd length, a target outside 0..255, a target twice, a source below -1, more than
  64 pairs) and for a `defaults` below 0 or with a bit outside the twenty listenable meanings;
  on windows `unsupported` after that shape; every other call refused BY NAME (the verdict
  `unsupported`, the reason naming `os_spawn_session`): Rust's standard library at 1.97.1 has no
  stable way to start a child in a new session (`process_setsid` is unstable), and lupin links no
  C library and admits no `unsafe`.
- **`os_setsid()`**: refused by name (the standard library has no `setsid` for the calling
  process).
- **`os_sid()`**: refused by name (the standard library has no `getsid`).
- On `corpus/os/session_rows.lu`: the four `invalid` lines, then the refusal by name at the
  forged-source spawn. `session_lanes`' lupin test expects a reason containing
  `os_spawn_session` from any lupin release other than 0.1.60.

## Evidence index

`notes/s290-evidence/`:
- `probe/`: `setsid`, `getsid` and a terminal's acquisition measured on Linux and macOS before
  the shape (the C probes, a pseudo-terminal runner, Rust's `process_setsid` status).
- `macos-session-lanes-651f0648.log`, `macos-sema-tests-651f0648.log`: the Mac's checks at the
  change's tree (`session_lanes` 4 passed; the counts).
- `ci-red.txt`: the rows red on CI, run ids, jobs and failing tests by name.
- `ci-plant.txt`: the two-hook plant red on CI by name, the other tests green.
- `linux-cheap-68f33aba.log`: the first Linux check in a two-CPU slot (fmt, clippy, the build,
  the freestanding archive, `session_rows` on three machines, the changed test files): red on
  `RT_SYMBOLS` 270 (fixed in `15f808ae`); `session_lanes`' pinned rows skip loudly there (four
  CPUs asked of two).
- `linux-cheap-15f808ae.log`: the same check after the fix: every step green.
- `std-rows-and-pax-fixture-68f33aba.log`: wolf-std's four session rows (its branch `s290`) on
  this head's wolf, checked and native, and on lupin 0.1.60; and the PAX fixture built static.
- `pax-boot-68f33aba.log`: the PAX leg (fresh pax clone, unchanged; KVM), the serial console
  and the seven rows. Lines in which the kernel prints control-register values are dropped from
  this copy.
- `pax/sessrows.lu`: the PAX fixture.
- `rig-linux/`: the scripts that ran the Linux checks, the PAX leg and the gauntlet; the host's
  own names (its podman, its locks, its CPUs, its images) come from a host file that is not
  part of them.

`68f33aba` and the final head differ in tests and notes only: the compiler and the runtime that
built the fixture and ran wolf-std's rows are the final head's. The Linux gauntlet's record at
the final head and the one-hunk plant's record are cited in the PR.

## Done-when

Commits, in order:
- `2a9a572f` the probes; `da388be5` the prediction.
- The spec: `923d747b` (`[os.proc.session]`, `[os.term.session]`, the design note, the host
  table 180), `f7199241` (the anchors, 736), `4f371d93`, `ec7ed9c1` (the decisions as stated).
- The rows, red before the change: `05d7ffb7` (`session_rows`), `0f453ee1` (`session_lanes` and
  its fixtures), `57c48127` (the counts), `0e3f002e`, `d1cad3a8`, `3111ce50` (the snapshots).
- The change: `515b08db` (wolf_rt), `d2bbaee7` (clif), `26d409eb` (sema), `fc7676cf` (WIR),
  `0c3574f8` (the checked machine), `700bb629` (the charges), `f5007e3d` (the rows' clippy fix),
  `6837e670` (`spec/prelude.json`), `888655ab` (the lowering ledger), `651f0648` (the
  changelog).
- `e72596a3`, `8f5475b8`, `68f33aba` (text), `07c229e6` (the CI evidence), `15f808ae`
  (`RT_SYMBOLS` 273), then this note and the Linux evidence.

Done when: the PR's `tier full` is green at this head with the `full-matrix` label, the Linux
gauntlet's record at this head is green and the one-hunk plant's record is red by name, and the
audit is clean.
