# s292 prediction: pseudo-terminals for wolf programs

Committed on branch `s292` before any change to the spec, the runtime, the compiler or a row, and
before any output of this lane's probe was read. Base: wolf-lang trunk `878490c4`. This work lands
after the new-session change (#1089, branch `s290`) and will be rebased onto it; every count below
is stated on top of s290's (anchors 738, host table 180, prelude 191 functions, 273 runtime
symbols). This file is the first phase: the shape, for a ruling before any code, and the
predictions the probe in `notes/s292-evidence/probe/` is scored against.

## 1. What a program cannot do at the base

Measured by burrow's bw06 (`notes/bw06-the-terminal.md` and `notes/bw06/evidence/` in the public
burrow repository; linux 7.2.4 x86-64 and macOS 26.4.1 arm64, wolf 0.2.37):

- `fs_open_mode("/dev/ptmx", 3)` opens a master on both hosts, and it is a terminal. Nothing in the
  table grants, unlocks or names its slave: linux 7.2.4's `/proc/self/fdinfo` holds no
  `tty-index`, and macOS has no `/proc`. A slave cannot be opened.
- No call sets a window size (`os_term_size` reads), none sets a special character
  (`os_term_chars` reads twelve), and the mode int reaches nine flags of the thirty-odd RFC 4254
  section 8 names.
- On macOS, `os_term_size` and `os_term_mode` on a master answered `io`, before any slave was
  opened, granted or not.
- Through `extern "c"` the C library's `posix_openpt`, `grantpt`, `unlockpt` and `ptsname` work on
  both hosts, but `ioctl` is variadic and wolf refuses a variadic prototype on aarch64-apple-darwin
  (E0826), so no size can be set on macOS that way.

What exists in `[os.term]`: the foreground (`os_term_foreground`, `os_term_set_foreground`), the
group (`os_pgid`, `os_set_pgid`), the mode int (`os_term_mode`, `os_term_set_mode`), the restore
(`os_term_restore_at_exit`), the size read (`os_term_size`), the twelve characters
(`os_term_chars`); s290 adds `os_setsid` and `os_sid`.

## 2. References

POSIX.1-2024: XBD 11 (General Terminal Interface), `posix_openpt`, `grantpt`, `unlockpt`,
`ptsname` and `ptsname_r`, `tcgetattr`, `tcsetattr`, `cfgetispeed` and its siblings; linux's
`ioctl_tty(2)` and `pty(7)`; macOS's `pty(4)`, `tty(4)` and `termios(4)`; RFC 4254 sections 6.2,
6.7 and 8; RFC 8160 (IUTF8, opcode 42); the macOS 26.4 SDK's `sys/termios.h`, `sys/ttycom.h`,
`sys/ttydefaults.h` and `_stdlib.h`; glibc 2.42's `termios.h` and `sys/ioctl.h` (as `gcc -E -dM`
prints them). PAX: px111's note and its recorded probe rows (pax branch `px111`:
`notes/px111-ptys.md`, `tests/mpx5-pty.d/`). No runtime, C library or terminal source of any
other project.

## 3. RFC 4254 section 8, per host

"has" is the host's `termios` (its header names the field); PAX runs the static linux binary, so
its table is linux's by construction, and what PAX's line discipline does with each is px111's.
The last two columns are this lane's predictions for a set on a pseudo-terminal's slave read back
with `tcgetattr`; the probe scores them.

| op | name | linux | macOS | PAX | round trip, linux | round trip, macOS |
|---|---|---|---|---|---|---|
| 1 | VINTR | has | has | has | yes | yes |
| 2 | VQUIT | has | has | has | yes | yes |
| 3 | VERASE | has | has | has | yes | yes |
| 4 | VKILL | has | has | has | yes | yes |
| 5 | VEOF | has | has | has | yes | yes |
| 6 | VEOL | has | has | has | yes | yes |
| 7 | VEOL2 | has | has | has | yes | yes |
| 8 | VSTART | has | has | has | yes | yes |
| 9 | VSTOP | has | has | has | yes | yes |
| 10 | VSUSP | has | has | has | yes | yes |
| 11 | VDSUSP | **lacks** | has | lacks | — | yes |
| 12 | VREPRINT | has | has | has | yes | yes |
| 13 | VWERASE | has | has | has | yes | yes |
| 14 | VLNEXT | has | has | has | yes | yes |
| 15 | VFLUSH | **lacks** | **lacks** | lacks | — | — |
| 16 | VSWTCH | has (`VSWTC`) | **lacks** | has | yes (stored; nothing acts on it) | — |
| 17 | VSTATUS | **lacks** | has | lacks | — | yes |
| 18 | VDISCARD | has | has | has | yes | yes |
| 30 | IGNPAR | has | has | has | yes | yes |
| 31 | PARMRK | has | has | has | yes | yes |
| 32 | INPCK | has | has | has | yes | yes |
| 33 | ISTRIP | has | has | has | yes | yes |
| 34 | INLCR | has | has | has | yes | yes |
| 35 | IGNCR | has | has | has | yes | yes |
| 36 | ICRNL | has | has | has | yes | yes |
| 37 | IUCLC | has | **lacks** | has | yes | — |
| 38 | IXON | has | has | has | yes | yes |
| 39 | IXANY | has | has | has | yes | yes |
| 40 | IXOFF | has | has | has | yes | yes |
| 41 | IMAXBEL | has | has | has | yes | yes |
| 42 | IUTF8 | has | has | has | yes | yes |
| 50 | ISIG | has | has | has | yes | yes |
| 51 | ICANON | has | has | has | yes | yes |
| 52 | XCASE | has | **lacks** | has | yes | — |
| 53 | ECHO | has | has | has | yes | yes |
| 54 | ECHOE | has | has | has | yes | yes |
| 55 | ECHOK | has | has | has | yes | yes |
| 56 | ECHONL | has | has | has | yes | yes |
| 57 | NOFLSH | has | has | has | yes | yes |
| 58 | TOSTOP | has | has | has | yes | yes |
| 59 | IEXTEN | has | has | has | yes | yes |
| 60 | ECHOCTL | has | has | has | yes | yes |
| 61 | ECHOKE | has | has | has | yes | yes |
| 62 | PENDIN | has | has | has | yes (low confidence: a state flag) | yes (low confidence) |
| 70 | OPOST | has | has | has | yes | yes |
| 71 | OLCUC | has | **lacks** | has | yes | — |
| 72 | ONLCR | has | has | has | yes | yes |
| 73 | OCRNL | has | has | has | yes | yes |
| 74 | ONOCR | has | has | has | yes | yes |
| 75 | ONLRET | has | has | has | yes | yes |
| 90 | CS7 | has | has | has | **no**: a pseudo-terminal keeps CS8 | yes |
| 91 | CS8 | has | has | has | yes | yes |
| 92 | PARENB | has | has | has | **no**: a pseudo-terminal keeps parity off | yes |
| 93 | PARODD | has | has | has | yes | yes |
| 128 | TTY_OP_ISPEED | has | has | has | yes (9600 stored; medium confidence) | yes |
| 129 | TTY_OP_OSPEED | has | has | has | yes (medium confidence) | yes |

So linux lacks 11, 15, 17; macOS lacks 15, 16, 37, 52, 71; PAX answers as linux. The disabled
character is 0 on linux (glibc's `_POSIX_VDISABLE`) and 255 on macOS (its header), as
`[os.term.chars]` already measured black-box.

## 4. The shapes (Q1 to Q8), each with a recommendation

- **Q1, opening a pty.**
  (A) **`os_term_open_pty() -> int ! {io, unsupported}`**: a new master, granted and unlocked,
  opened with `O_NOCTTY` (so it never becomes the opener's controlling terminal), close-on-exec,
  as a handle of the fs table (3 and up). Its slave is named by Q2 and opened by the program with
  `fs_open_mode(name, 3)`; the runtime's opens pass no `O_NOCTTY`, so a session leader with no
  controlling terminal that opens the slave takes it (s290's clause), which is exactly the child's
  step (`os_setsid`, then the open). The runtime makes its pty opens one at a time (a lock), since
  POSIX does not make `ptsname` thread-safe and the runtime's own tests saw macOS's
  `posix_openpt` fail when eight threads opened at once.
  (B) **`os_term_open_pty() -> (int, int)`**: the master and the slave (opened with `O_NOCTTY`),
  both handles, `openpty(3)`'s shape and `os_pipe`'s answer. The opener then holds a slave of its
  own, so the master reads no end at the program's end until the opener closes it.
  (C) one call answering `(int, str)`: saves a call, but no builtin answers a pair with a `str`
  today, so the lowering, the checked machine and lupin each grow an answer shape.
  (D) `fs_open_mode("/dev/ptmx", 3)` stays the open and a new call grants, unlocks and names: no
  new open, but the master is opened without `O_NOCTTY` and the path is assumed.
  **Recommend A**, with B if the probe shows macOS's master refusing the size while a slave is
  open (then a holder needs a slave of its own for `window-change`).
  **A host without ptys:** windows answers `unsupported`, by name, for every call of this family;
  the freestanding target refuses the names at compile time; a unix host that cannot make one (no
  `/dev/ptmx`, none free, the descriptor limit) is `io`, with the host's number in `os_error()`
  (`[os.fs.error]`). Rows `{io, unsupported}` (alternative: `denied` and `not_found` split out of
  `io`; no customer distinguishes them).
- **Q2, the slave's name.** **`os_term_pty_name(fd: int) -> str ! {io, unsupported}`**:
  `ptsname_r` on the master (`/dev/pts/N` on linux and PAX, `/dev/ttysNNN` on macOS). A handle
  that is closed, forged or not a master (a file, a slave, a terminal) is `io`. It names any
  master the table holds, so a program also has the name for `SSH_TTY`.
- **Q3, the window's size.** **`os_term_set_size(fd: int, rows: int, columns: int) -> () !
  {invalid, io, unsupported}`**: `TIOCSWINSZ` on the terminal `fd` names, rows and columns as
  given, the two pixel fields as found. `invalid` for rows or columns outside 0..65535, before the
  handle. When the size changes the host sends `SIGWINCH` (the meaning `RESIZE`) to the
  terminal's foreground group; a size it already has sends nothing. **Which end**, predicted:
  linux and PAX take it on either end at any time (px111's W rows hold PAX to linux); macOS
  refuses it on the master until a slave descriptor is open and takes it on the slave always; the
  probe measures both and the rows hold the answer. Options: (A) as stated; (B) four numbers, the
  pixels too (RFC 4254 carries them, `[os.term.size]` reads only rows and columns, and its own
  ruling left the pixels out as two words nobody fills); (C) a size only at the open
  (`openpty(3)`'s `winp`), which leaves `window-change` with nothing. **Recommend A.**
- **Q4, the modes.** The mode int keeps its meaning and gains no bit. Two new calls address the
  terminal's settings one at a time by **RFC 4254 section 8's opcode** (with RFC 8160's 42), the
  one published, portable numbering of exactly this vocabulary:
  **`os_term_attr(fd: int, op: int) -> int ! {invalid, io, unsupported}`** reads one, and
  **`os_term_set_attr(fd: int, op: int, value: int) -> () ! {invalid, io, unsupported}`** sets
  one, every other field as found, `TCSADRAIN` and `SIGTTOU` blocked as `os_term_set_mode` sets.
  Values: a character is its byte (0..255) or -1 for disabled (the family's word; the host's
  disabled value never reaches the program, and a byte equal to it, 0 on linux and PAX and 255
  on macOS, disables the character); a flag is 0 or 1; `CS7` and `CS8` read 1 when the character
  size is that one, a 1 selects it and a 0 leaves the size as found (a client sends both); a speed
  is bits per second. RFC 4254's own 255 for "none" is the decoder's to map to -1 (std gives the
  function). **The order of refusal:** (1) the shape: **`invalid`** for an opcode outside the
  vocabulary (0, 19..29, 43..49, 63..69, 76..89, 94..127, 130 and up, negatives) and a value
  outside its op's range (a character outside -1..255, a flag outside 0..1, a speed outside
  0..4294967295); (2) the host: **`unsupported`**, by name, on windows, for an op the host lacks
  (section 3's table) and for a speed it has no setting for; (3) the handle: **`io`** for one that
  is closed, forged or not a terminal; (4) the host's answer: a refused `tcsetattr` is `io`, and a
  set the host did not keep, read back after it, is **`unsupported`** by name (predicted: linux
  ptys keep CS8 and parity off). So every op either round-trips or is refused by name.
  Options: (A) as stated; (B) one call taking a list of opcode-value pairs, applied in one
  `tcsetattr`, answering the ops it refused: one system call pair per `pty-req`, but a list answer
  and an all-or-nothing rule for a bad pair; (C) grow the mode int to the whole vocabulary and add
  a character setter: every mode built by hand would then force some thirty more flags to their
  usual value, and the usual value differs by host (IUTF8, ECHOCTL, IMAXBEL), the hazard s234 and
  s248 accepted for five flags, multiplied; and a host's lack has no row; (D) a raw `termios` byte
  image, refused before as not portable. **Recommend A.** `os_term_chars` and the mode int read
  the same settings as before; nothing they answer changes.
- **Q5, the checked machine and lupin.** **The checked machine** performs each call on the host
  from the `wolf` process with the runtime's rules: the master in its own table, the name, the
  size and the attributes through `libc`, the op table transcribed (the locked graph keeps
  `wolf_rt` out of the compiler's reach), and a row that runs every op on both machines holds the
  two tables equal. `os_fork` is refused there, so its child rows start a compiled helper with
  `os_spawn_session` that opens the slave itself. **lupin** (wolf-interp's own lane, its own
  change): resolves the five names with these signatures; answers every row decided before the
  terminal exactly as the runtime (`invalid` for the shapes, `unsupported` on windows and for an
  op the target's table lacks, since that table is fixed per target, `io` for a closed or forged
  handle and for one that is not a terminal, which Rust's `IsTerminal` tells); and refuses by name
  every well-formed call that reaches a terminal (the open always; the name, size and attributes
  on a terminal). It links no C library and admits no `unsafe`, and Rust's standard library has
  no `posix_openpt`, `ioctl` or `tcsetattr`. A set the host does not keep is the host's answer,
  so lupin refuses it by name too.
- **Q6, capabilities and the freestanding target.** `os_term_open_pty` is **`exec`**, with
  `os_pipe` and `os_spawn_session` (a pty is made to run a program on). `os_term_pty_name`,
  `os_term_set_size`, `os_term_attr` and `os_term_set_attr` are **`io`**, uncharged, with
  `os_term_mode` and `os_term_set_mode` (a descriptor the program holds, nothing started; the
  `SIGWINCH` is the host's, as a typed Ctrl-C written to a master is). Alternative: the size set
  `exec`. All five are refused at comptime (D33) and refused by name on the freestanding target;
  windows answers `unsupported` after the shape.
- **Q7, the std surface.** wolf-std's own prediction (its `docs/s292-prediction.md`):
  `term.open_pty`, `term.pty_name`, `term.set_size`, `term.attr`, `term.set_attr`, and pure
  helpers for the vocabulary.
- **Q8, the spec and the census.** Four clauses: `[os.term.pty]` (the open and the name),
  `[os.term.size.set]`, `[os.term.attr]` (the vocabulary, section 3's table, the order of
  refusal) and `[os.term.pty.note]` (this design note): anchors 738 to 742. Host table 180 to 185
  lines (`[os.host.sigs]`'s count sentence), prelude 191 to 196 functions and W0304 223 to 228,
  runtime symbols 273 to 278; the error rows' name lists in `spec/prelude.json` (`io` and
  `unsupported` +5, `invalid` +3). Text that changes with them: `[os.term]`'s opening (the calls
  it lists), `[os.term.chars]` ("Nothing sets them" now has a setter), `[os.term.editor.note]`
  ("no setter for the special characters"), `[os.term.size]` (a pointer to the set),
  `[os.fs.error]` (the open sets `os_error()`), the windows posture list, CHANGELOG. Count tests:
  `host_table_spec.rs`, `prelude_names.rs`, `RT_SYMBOLS` and its count test, `pkg_cmd`'s
  capability tests, a `sandbox_s292.rs`, `freestanding_target.rs`'s rows, the lowering ledger and
  the corpus snapshots.

## 5. The probe, predicted

`notes/s292-evidence/probe/ptyshape.c`, one black-box C program, run on linux (7.2.4, glibc 2.42,
on the host, two CPUs) and on macOS 26.4.1 arm64. Each row's prediction:

- **A1** `posix_openpt(O_RDWR|O_NOCTTY|O_CLOEXEC)`: linux a master, close-on-exec; macOS
  **refuses the flag** (`EINVAL`; low confidence).
- **A2** `open("/dev/ptmx", O_RDWR|O_NOCTTY|O_CLOEXEC)`, then `grantpt`, `unlockpt`, `ptsname_r`:
  a master on both, close-on-exec, its slave named (`/dev/pts/N`, `/dev/ttysN`).
- **A3** the slave opened before `unlockpt`: linux `EIO`; macOS refused (errno recorded).
- **A4** `ptsname_r` on a file and on the slave: refused on both (errno recorded).
- **B1** the master, no slave ever opened: `TIOCGWINSZ`, `TIOCSWINSZ`, `tcgetattr`, `tcsetattr`
  succeed on linux; **all four refused on macOS** (errno recorded).
- **B2** a slave open (`O_NOCTTY`): all four succeed on the master on both hosts.
- **B3** a size set on the master reads back on the slave, and one set on the slave on the master:
  both hosts.
- **B4** after the slave's last close: linux's master still takes all four; macOS refuses them
  again (low confidence).
- **B5** a size and an interrupt character set on a slave, the slave closed, opened again:
  linux keeps both; **macOS resets them at the new first open** (low confidence).
- **B6** a setting set on the master before any slave open (linux) is the slave's at its first
  open.
- **C1** a child that leads a session and holds the slave as its controlling terminal hears
  `SIGWINCH` when the size is set on the master; **C2** and when it is set on another descriptor
  of the slave that is not the setter's controlling terminal; **C3** none for the same size again;
  **C4** one for a change of the pixel fields alone. Both hosts.
- **D** every op of section 3 the host has: set on the slave to a value it does not have, read
  back, set back. The table's last two columns.
- **D2** the default settings of a new pty: linux iflag 0x500, oflag 0x5, cflag 0xbf, lflag
  0x8a3b, speed 38400 (px111's recording); macOS its `ttydefaults.h`: iflag BRKINT ICRNL IMAXBEL
  IXON IXANY, oflag OPOST ONLCR, lflag ECHO ICANON ISIG IEXTEN ECHOE ECHOKE ECHOCTL, cflag CREAD
  CS8 HUPCL, speed 9600 (medium confidence for a pty).
- **E1** VINTR set to Ctrl-G on the slave: Ctrl-G written to the master interrupts the child's
  group (its INT trap), and Ctrl-C written after it reaches the child as a byte. Both hosts.
- **E2** a slave a non-leader holds open (`O_NOCTTY`) does not stop a session leader's later open
  by name from taking it as its controlling terminal. Both hosts.
- **J1** a session leader with no controlling terminal that opens `/dev/ptmx` without `O_NOCTTY`:
  the master does not become its controlling terminal (linux; macOS low confidence).

## 6. The rows, red first; the plant; the CI tiers; PAX

- **wolf-lang.** `corpus/os/pty_rows.lu`, every host, rows that reach no terminal: each shape's
  `invalid`, each host's `unsupported` ops, a forged handle's `io`, windows' `unsupported` for all
  five. `crates/wolf_driver/tests/pty_lanes.rs` with fixtures under
  `crates/wolf_driver/tests/fixtures/pty/`, on the checked machine, native and release, on linux
  and on CI's macOS runner, on a pty the test opens through the new call: the open, the name, the
  slave opens and is a terminal; the master's answers before a slave is open (per host, as
  measured); a size set on one end reads back on the other; the child (a session leader on the
  slave) hears `RESIZE`, and nothing for the same size; every op round-trips on the slave or is
  `unsupported` by name; Ctrl-G interrupts after VINTR is set to it; the name is gone after the
  master's close. `wolf_rt`'s unit tests for the table and the shapes. Each row committed before
  the change it covers (E0301 on the compiler lanes until the names exist).
- **The plant.** For the record: the runtime's open skips `unlockpt`, and the slave's open goes
  red by name on native and release, linux and macOS, while the checked machine stays green (its
  own implementation). For development: VINTR and VQUIT swapped in the runtime's op table; every
  round trip stays green and the Ctrl-G row goes red (the child ends by QUIT), which is why the row
  of the character's effect exists beside the round trips.
- **CI.** Cross-cutting (spec, runtime, sema, lowering, the checked machine): a full-matrix hosted
  run at tier full before merge, the local gauntlet beside it.
- **PAX.** The rows' static linux binaries on px111's kernel under QEMU, as s290's PAX leg ran on
  trunk's: predicted to answer as linux in every row (the name `/dev/pts/0`, both ends take the
  size and send `SIGWINCH`, every linux op round-trips; whether PAX keeps CS7 and parity on a pty
  is not in px111's recording and is predicted as linux), `/proc/sys/kernel/pty/nr` back to 0.

## 7. Phase 2, its cost

Spec (four clauses, the table, the touched text); `wolf_rt` (the open, the name, the size, the op
table per host with the read-back); sema, prelude, lowering and the census; the checked machine's
five; the rows above; then wolf-std's half once a release carries the calls. Machine time: one
heavy local gauntlet take on linux, one macOS build hold for `pty_lanes` (two workers), one light
KVM job for the PAX leg, one full-matrix hosted run (about two hours). About two lane sessions.
Nothing lands before the ruling on Q1 to Q8.

## 8. Addendum, after the first probe's output was read

The first run (section 5) showed macOS putting a pty's settings back to its defaults when the
slave is opened while no descriptor of it is open (row B5). That bears on Q1, so a second, smaller
probe, `notes/s292-evidence/probe/ptyhold.c`, asks what a held slave does, predicted here before it
runs:

- **K1** a slave descriptor held open, the size and VINTR set on it, a second descriptor opened by
  name: the second reads both, on both hosts.
- **K2** the pattern Q1 (B) gives a holder: the opener holds a slave with the settings set; a forked
  child that inherited that descriptor leads a session and opens the slave by name (its controlling
  terminal) after the parent closed its own copy: the child reads the settings, on both hosts.
- **K3** the same with the child closing its inherited copy first, so no descriptor is open at its
  open: linux keeps the settings; macOS reads its defaults (interrupt 3, size 0 0).
- **K4** a session leader with no controlling terminal opens the slave with `O_NOCTTY`: `/dev/tty`
  stays `ENXIO`; opened again without it: it takes the slave. Both hosts.
