# s292 — pseudo-terminals for wolf programs

Phase 1: the shape, measured, for a ruling before any code. The prediction is
`notes/s292-prediction.md` (committed at `ec5b4992` before any probe ran; its section 8 at
`87dbefea`, before the second probe). The evidence is `notes/s292-evidence/` (two C probes and their
outputs, each ending in `END exit=0`). No product code, no row and no spec text changes in this
phase.

## 1. Why

burrow's ssh server cannot grant a terminal request: a wolf program cannot open a usable pty on any
host. burrow's bw06 measured it (public, `notes/bw06-the-terminal.md` in the burrow repository): the
master opens through `fs_open_mode`, but nothing grants, unlocks or names the slave; no call sets a
window size or a special character, and the mode int reaches nine of RFC 4254 section 8's flags;
through `extern "c"` the C library works on linux, but wolf refuses the variadic `ioctl` on
aarch64-apple-darwin (E0826).

## 2. Evidence

- linux 7.2.4 x86-64, glibc 2.42, the host's gcc, two CPUs: `ptyshape` and `ptyhold`.
- the same kernel inside the toolchain image (Ubuntu 24.04, glibc 2.39): `ptyshape` was queued and
  had not run when this phase closed (section 4, Speeds).
- macOS 26.4.1 arm64, the system's cc: `ptyshape` and `ptyhold`.

## 3. The prediction, scored

Rows A to J (section 5 of the prediction) and K (its section 8):

- **Hits, both hosts:** A2 (a master from `open("/dev/ptmx", O_RDWR|O_NOCTTY|O_CLOEXEC)`,
  close-on-exec, granted, unlocked and named: `/dev/pts/N`, `/dev/ttysN`); A3 (the locked slave's
  open is refused: linux `EIO`, macOS `EAGAIN`); A4 (`ptsname_r` on a file or a slave: `ENOTTY`);
  B2 and B3 (with a slave open, both ends take the size and the settings, and each reads what the
  other set); C1 to C4 (the child that leads a session on the slave hears `SIGWINCH` for a size set
  on the master and for one set on a slave descriptor that is not the setter's terminal; none for
  the same size again; one for a change of the pixel fields alone); D3 (the disabled value: 0 on
  linux, 255 on macOS); E1 (VINTR set to Ctrl-G: a 0x07 written to the master interrupts the child's
  group, a 0x03 after it is a byte, and Ctrl-\ still quits); E2 (a slave held open, `O_NOCTTY`, by a
  process that leads no session does not stop the child's later open from taking it); J1 (a session
  leader with no terminal that opens `/dev/ptmx` without `O_NOCTTY` does not take the master).
- **Hits, linux:** A1; B1 (the master takes the size and the settings before any slave open); B4,
  B5 and B6 (the settings, set on either end at any time, last as long as the pty, across every
  close and open of the slave); D (51 of 56 opcodes round-trip; VDSUSP, VFLUSH and VSTATUS lacking;
  CS7 and PARENB not kept: `tcsetattr` answers 0 and the pty keeps CS8 and parity off, while PARODD
  is kept, D5); D2 (iflag 0x500, oflag 0x5, lflag 0x8a3b, speed 38400).
- **Hits, macOS:** B1 (the master, no slave ever opened, refuses all four: `ENOTTY`); B5, at low
  confidence (the settings and the size go back to the host's defaults when the slave is opened
  while no descriptor of it is open; K3 again); D (VFLUSH, VSWTCH, IUCLC, XCASE and OLCUC lacking;
  every other opcode round-trips but one, below); D2 (`ttydefaults.h`'s flags, speed 9600).
- **Misses:**
  - A1, macOS: `posix_openpt` takes `O_CLOEXEC` (predicted `EINVAL`).
  - B4, macOS: after the slave's last close the master still takes the size and the settings
    (predicted refused again); they are lost at the slave's next open all the same (B5).
  - D 62 PENDIN, macOS: not kept. After canonical input was turned back on it read 1, and a set of
    0 read 1 again: the kernel owns the flag (predicted kept, at low confidence).
  - D2, linux: cflag read 0xf00bf, not px111's 0xbf: glibc 2.42's `tcgetattr` reports the input
    speed in `CIBAUD` too (section 4, Speeds).
- **Not predicted** (rows the probe carried without a prediction): D4, both hosts keep every speed
  asked of a pty (50, 115200, 230400; and 12345, which has no `B` constant, on glibc 2.42 and on
  macOS); D5, linux drops CS7 and PARENB from one set that also asks PARODD, and keeps PARODD.
- **K1 to K4, all hits:** a held slave keeps the settings across another open on both hosts (K1,
  K2); with no descriptor open at the child's open, linux keeps them and macOS reads its defaults
  (K3); a session leader's `O_NOCTTY` open of the slave leaves `/dev/tty` `ENXIO`, and its open
  without it takes the slave (K4).

## 4. What the probes change

**Q1 moves from A to B.** The prediction recommended a master alone (A) unless macOS's master
refused the size while a slave was open. It does not refuse then (B2); but two things it did not
foresee decide it:

1. macOS puts a pty's settings and size back to its defaults whenever the slave is opened while no
   descriptor of it is open (B5, K3), and its master takes neither before the slave's first open
   (B1). A slave held open keeps them across another open (K1), and across a child's open after
   the parent closed its own copy, when the child kept the descriptor it inherited (K2).
2. So a holder that sets the size and modes before the program runs must hold a slave until the
   program's own open, on macOS. Through the fs family it can only open one without `O_NOCTTY`
   (the runtime's opens pass none, s290), and a holder that leads a session with no terminal would
   then take the slave itself. The runtime can open it with `O_NOCTTY`, which K4 shows keeps a
   session leader from taking it.

So **`os_term_open_pty() -> (int, int) ! {io, unsupported}`** answers the master and a slave, both
close-on-exec handles of the fs table, the slave opened with `O_NOCTTY` (`os_pipe`'s answer shape;
`openpty(3)`'s). From the open on, either end takes the size and the settings on every host, and
they last while any descriptor of the slave is open. The order a program follows, on every host:
open the pair; set the size and the attributes on either end; fork (or spawn with the slave in the
map); the child keeps its inherited slave until its own open by name, after `os_setsid`, makes the
slave its controlling terminal; the parent closes its slave after the fork. The master reads the end
(linux `EIO`, macOS end of file, bw06) only once every slave descriptor is closed, the opener's
included.

**Q3 (the size) moves from A to B**, with the pixels:
**`os_term_set_size(fd: int, rows: int, columns: int, xpixels: int, ypixels: int) -> () ! {invalid,
io, unsupported}`**, each 0..65535 (`invalid` otherwise, before the handle). Measured: either end
takes it on both hosts once a slave has been opened, which Q1's open does; the host signals the
foreground group when any of the four changes, a change of the pixel fields alone included (C4), and
not for the same four again (C3). The prediction kept the pixels as found because `[os.term.size]`'s
read left them out; but burrow's requirements (section 8) ask for RFC 4254's four as `sshd` sets
them, and a client's pixel-only `window-change` then signals as through `sshd`. No row of burrow's
observes the pixels; the cost is two arguments. The read stays `os_term_size`'s rows and columns
(nothing asks for a pixel read).

**Q4 (the attributes) stands**, with section 5's measured table. The read-back after each set is
what turns the measured surprises into rows: linux's pty keeps CS8 and parity off, macOS keeps
PENDIN to itself; each answers `unsupported` by name. The usual settings differ by host (linux's pty
has ECHOK and not IMAXBEL or IXANY; macOS's the reverse; speed 38400 against 9600), which is the
measured case against growing the mode int (option C).

**Speeds.** POSIX leaves `speed_t`'s values to the host. glibc 2.42's header makes each `B` constant
the rate itself (`B38400` is 38400) and its `cfsetospeed` took 12345, which no constant names; macOS's
`speed_t` is the rate too. A C library that encodes a rate as a code would answer otherwise, so the
runtime never compares a speed with constants of a C library it was not built against: it maps a
rate in bits per second to the host's setting through a table of the rates every host names (0, 50,
75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600, 115200,
230400), a rate outside it `unsupported`, and phase 2's rows run the speeds under the toolchain
image's C library (Ubuntu 24.04), a host build and macOS. The image's run of the probe
was queued behind another job and is not in this phase's record; it is phase 2's first measurement.

## 5. RFC 4254 section 8, measured

On a pty's slave, set to a value it did not have and read back (`probe/linux.txt`,
`probe/macos.txt`, rows D). "kept" round-trips; "lacks" is no field in the host's `termios`; "not
kept" is a set the host answered 0 for and did not keep, which the runtime's read-back answers
`unsupported`. PAX is linux's table by construction (the static linux binary) and its line
discipline's answers are unmeasured here: phase 2's PAX leg.

| op | name | linux 7.2.4 | macOS 26.4.1 | PAX (px111) |
|---|---|---|---|---|
| 1..10 | VINTR VQUIT VERASE VKILL VEOF VEOL VEOL2 VSTART VSTOP VSUSP | kept | kept | linux's table |
| 11 | VDSUSP | lacks | kept | lacks |
| 12..14 | VREPRINT VWERASE VLNEXT | kept | kept | linux's table |
| 15 | VFLUSH | lacks | lacks | lacks |
| 16 | VSWTCH | kept (`VSWTC`; nothing acts on it) | lacks | linux's table |
| 17 | VSTATUS | lacks | kept | lacks |
| 18 | VDISCARD | kept | kept | linux's table |
| 30..36 | IGNPAR PARMRK INPCK ISTRIP INLCR IGNCR ICRNL | kept | kept | linux's table |
| 37 | IUCLC | kept | lacks | linux's table |
| 38..42 | IXON IXANY IXOFF IMAXBEL IUTF8 | kept | kept | linux's table |
| 50, 51 | ISIG ICANON | kept | kept | linux's table |
| 52 | XCASE | kept | lacks | linux's table |
| 53..61 | ECHO ECHOE ECHOK ECHONL NOFLSH TOSTOP IEXTEN ECHOCTL ECHOKE | kept | kept | linux's table |
| 62 | PENDIN | kept | **not kept** (the kernel's state flag) | linux's table |
| 70 | OPOST | kept | kept | linux's table |
| 71 | OLCUC | kept | lacks | linux's table |
| 72..75 | ONLCR OCRNL ONOCR ONLRET | kept | kept | linux's table |
| 90 | CS7 | **not kept** (a pty stays CS8) | kept | unmeasured |
| 91 | CS8 | kept | kept | linux's table |
| 92 | PARENB | **not kept** (a pty keeps parity off) | kept | unmeasured |
| 93 | PARODD | kept | kept | linux's table |
| 128, 129 | TTY_OP_ISPEED, TTY_OP_OSPEED | kept (9600; any rate on glibc 2.42) | kept (any rate) | linux's table |

What each answers through `os_term_attr` and `os_term_set_attr`: an op the host lacks is
`unsupported` before the handle is read; a set the host does not keep is `unsupported` after its
read-back, with every other field as it was; every other op round-trips.

## 6. The shape, as recommended now

| builtin | rows | capability | linux, PAX | macOS | windows |
|---|---|---|---|---|---|
| `os_term_open_pty() -> (int, int)` | `{io, unsupported}` | `exec` | the pair; `os_error()` on `io` | the pair | `unsupported` |
| `os_term_pty_name(fd: int) -> str` | `{io, unsupported}` | `io`, uncharged | `/dev/pts/N` | `/dev/ttysN` | `unsupported` |
| `os_term_set_size(fd: int, rows: int, columns: int, xpixels: int, ypixels: int) -> ()` | `{invalid, io, unsupported}` | `io`, uncharged | either end | either end | `unsupported` after the shape |
| `os_term_attr(fd: int, op: int) -> int` | `{invalid, io, unsupported}` | `io`, uncharged | section 5 | section 5 | `unsupported` after the shape |
| `os_term_set_attr(fd: int, op: int, value: int) -> ()` | `{invalid, io, unsupported}` | `io`, uncharged | section 5, read back | section 5, read back | `unsupported` after the shape |

Everything else in the prediction's section 4 (Q2, Q5 to Q8), section 6 (the rows, the plant, the
CI tiers, PAX) and section 7 (phase 2's cost) stands as written there; the census does not move with
Q1's and Q3's changes (`os_pipe`'s pair is an answer shape every machine already has, and a
five-argument call returning nothing is `fs_set_times`'s kind).

## 7. Asked for a ruling

Q1 (B, as revised here), Q2, Q3 (B, as revised here), Q4 (A), Q5, Q6, Q7 (wolf-std's prediction and
its P5) and Q8, each as recommended; and the speed table above.

## 8. burrow's requirements, and what burrow changes

burrow's `notes/bw06-wolf-needs.md` (branch `bw06`) states what its server will call, written as the
consumer's requirements rather than a design. Held against the shape above:

- **The open** (its section 1): a master, close-on-exec, taking no controlling terminal, and the
  slave's name, granted and unlocked; "the slave itself need not be opened by the call; if it is,
  with `O_NOCTTY`", burrow still opens it by name in the child. Met by Q1 (B) and Q2, with the rows
  it names (`io` with `os_error()`, windows `unsupported`). **burrow changes:** its `Pty` carries the
  held slave beside the name (`Pty { master, slave, name }` in wolf-std's P5); the holder closes its
  `slave` right after the fork, or the master never reads the end; the child needs no step of its
  own, since the slave it inherits is close-on-exec and its exec closes it after the open by name.
  The runtime's opens keep passing no `O_NOCTTY` (s290's clause), which the child's open relies on.
- **The size** (its section 2): rows, columns and both pixel sizes, 0..65535, SIGWINCH to the
  foreground group; set once before the program and on each `window-change`; and which end, per
  host. Met by Q3 (B). Measured: either end on both hosts once a slave has been opened, so after Q1's
  open the holder sets the master at any time; macOS's master refuses it only before the slave's
  first open (B1), which Q1 (B) removes. burrow's fallback (a slave of the holder's own) is what
  Q1 (B) gives every caller.
- **The modes** (its section 3): each of section 8's rows set or cleared on its own, and a call that
  says which it could not apply. Met by Q4 (A): one op per call, `unsupported` by name for an op the
  host lacks or does not keep; wolf-std adds `term.set_attrs(tty, pairs) -> List[int]` over it,
  answering the ops it could not apply, which is burrow's `set_modes`. burrow's values (255 for no
  character, any non-zero for a set flag) are the wire's; `term.attr_from_wire` maps them to the
  builtin's (-1, and 1), so burrow decodes as it does now and calls one function. VMIN and VTIME stay
  in the mode int (`term.set_mode`). **Measured beyond its table:** linux's pty does not keep CS7 or
  PARENB and macOS does not keep PENDIN, so a client's `cs7` or `parenb` on linux and `-pendin` on
  macOS come back as not applied, where `sshd`'s `stty -a` would show the same refusal (the kernel's,
  not the server's).
- **Speeds** (not needed by burrow): kept in the vocabulary. A new pty reads 38400 on linux but 9600
  on macOS (D2), so a server on macOS that skips a client's 38400 shows another `stty -a` speed line
  than one that applies it; burrow may apply them.
- **The session** (its section 4) is s290's, unchanged; **the release** (its section 5) is the
  contract's: these calls and s290's in one release, with the PAX leg on px111's kernel.

## 9. Phase 2: what the implementation found

The decisions were taken as recommended in sections 6 and 8. Four things the implementation
measured that the probes had not, each now in the code and, where it changes an answer, the spec:

1. **The read-back follows every set.** In the toolchain image (glibc 2.39) `tcsetattr` answers
   `EINVAL` for CS7 and PARENB on a pty, where glibc 2.42 answered 0; neither keeps the set. So
   the setting is read back whatever `tcsetattr` answered, and a set not kept is `unsupported` with
   what was found put back; another refusal is `io` (`[os.term.attr]`).
2. **No versioned speed calls on linux.** The `libc` crate binds `cfgetispeed` and its three
   siblings to glibc's versions before 2.42 (`cfgetispeed@GLIBC_2.2.5` on x86-64), which keeps its
   `B` constants right on every glibc; but a static link against `libc.a`, the way PAX's programs
   are built, resolves no versioned name, and the PAX fixture failed to link. The runtime and the
   checked machine now read and write a linux speed as the kernel's code in `c_cflag` (`CBAUD` with
   `CBAUDEX`), one speed for input and output, as musl and glibc before 2.42 keep it; an input
   speed of 0 sets nothing and reads back as the output's, so it is `unsupported`. macOS keeps its
   unversioned calls. A held speed the table does not name reads `unsupported` (spec line added).
3. **macOS's PENDIN.** Its kernel ORs its own flag into every set, so a set of 1 is kept and a clear
   while the kernel holds the flag (as after canonical input comes back on) is not: the rows that
   toggle every op see 62 not kept, a set of 1 alone sees it kept. Either is a round trip or a
   refusal by name.
4. **Two fixture lessons.** `os_signal_wait_fds` refuses an empty set (`io`), so a wait that
   listens for nothing passes a set no one raises; and on macOS an open of a slave whose master is
   closed waits for a carrier that never comes, so a child keeps its inherited ends until its own
   open by name, and a parent that failed early cannot leave it parked.

The checked machine has no fork, so its child rows start a compiled helper with `os_spawn_session`
(`fixtures/pty/spawned.lu`, `helper.lu`); every `pty_lanes` row runs on one side, the checked
machine or native and release, so a break on one side is red by name on that side alone (the
development plant: `notes/s292-evidence/plant-dev-macos-abce5fed.log`).
