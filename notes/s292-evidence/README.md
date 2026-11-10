# s292 evidence, phase 1

Black-box C probes of each host's pseudo-terminals, written after the prediction
(`notes/s292-prediction.md`, committed at `ec5b4992`; its section 8 at `87dbefea`, before the second
probe) and committed before each first run. Each output ends in an `END exit=` line, which is the
record's truth.

| file | what |
|---|---|
| `probe/ptyshape.c` | rows A (the open, the name), B (which end takes the size and the settings, and when), C (SIGWINCH), D (every RFC 4254 section 8 opcode set on the slave and read back), E (the interrupt character's effect), J (a master opened without `O_NOCTTY`). Committed at `ac80527b`. |
| `probe/ptyhold.c` | rows K1 to K4: a slave descriptor held open across another open, and an `O_NOCTTY` open by a session leader. Committed at `db9d8b9f`. |
| `probe/linux.txt`, `probe/linux-hold.txt` | linux 7.2.4 x86-64, glibc 2.42, built with the host's gcc, two CPUs. |
| `probe/macos.txt`, `probe/macos-hold.txt` | macOS 26.4.1 arm64, built with the system's cc. |

`probe/linux-image.txt` is the toolchain image's run (Ubuntu 24.04, glibc 2.39, linux 7.2.4), queued behind
another job in phase 1 and taken at the start of phase 2: speeds are codes there (`B115200` 4098, a
rate with no code `EINVAL`), and `tcsetattr` answers `EINVAL` for CS7 and PARENB on a pty where glibc
2.42 answered 0; both keep neither.
A pty's number is written N. The sha256 of each file is in `SHA256SUMS`.

## Phase 2

| file | what |
|---|---|
| `plant-dev-macos-abce5fed.log` | the development plant, macOS 26.4.1 arm64: `abce5fed` (branch `s292-plant-dev`) is the head `4ce8706f` with VINTR and VQUIT swapped in the runtime's op table. Each `pty_lanes` row in its own run (the rows' names before they were split by side): every op still round-trips (the swap reads back what it set), the Ctrl-G rows go red natively (the child dies of QUIT, status -3, where INTERRUPT ends it with 7) while the checked machine's spawn row stays green on its own table, and the table row names the swap (ops 1 and 2). |
| `rig-linux/` | the scripts that run the Linux checks, the PAX leg and the gauntlet (adapted by copy from s290's); the host's own names (its podman, its locks, its CPUs, its images) come from a host file that is not part of them. Every pin goes through the toolchain image's own `taskset`, a translator that maps a pin into CPUs 6-11, so that image's block takes run on block B. |
| `pax/ptyrows.lu` | the PAX fixture: built static with the head's own wolf and run by pax's px111 kernel (`kmain_console`, `/etc/pax-run` `/bin/ptyrows /bin/ptyhelper`), with `fixtures/pty/helper.lu` as its child. |
| `linux-cheap-9cc32317.log` | the cheap checks at `9cc32317` in the toolchain image (Ubuntu 24.04, glibc 2.39), a two-CPU slot: fmt, clippy as CI runs it, the build, the freestanding archive, `corpus/os/pty_rows.lu` on the checked machine, natively and on lupin 0.1.60 (which does not resolve the names), the runtime's term tests, `pty_lanes` (seven rows; its pinned lanes skip loudly on two CPUs), the session, freestanding, symbol and prelude tests, the capability tests, the sema tests, the snapshots and `spec-extract --check`. Every step `exit=0`. |
| `pax-boot-9cc32317.log` | the PAX fixture and its helper built static with that head's wolf against the image's `libc.a` (ET_EXEC, no interpreter), then booted on pax's `px111` at `9536d82f` (a fresh clone, unchanged, its own pinned wolf and Limine; `kmain_console`) under KVM: the eleven rows, every op (11, 15 and 17 lacking; every other kept, 90 and 92 included), the child's RESIZE and Ctrl-G. The first take at `5654c46b` failed to link the fixture: the `libc` crate's versioned speed calls, which `9cc32317` replaced. |
