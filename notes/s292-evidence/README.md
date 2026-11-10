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
