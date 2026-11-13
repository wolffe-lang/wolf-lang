# r45: the fortieth release, wolf 0.2.40 with lupin 0.1.62

wolf 0.2.40 was released from `b333b9da`, paired with lupin 0.1.62 and the bundled std at wolf-std `be1d10fa`:
- wolf: wolf-lang#1106, release run 43362415637;
- lupin: from `72eff3c9`, wolf-interp#368, release run 43333412815.

The release note is `docs/releases/0.2.40.md`, and the prediction is `docs/releases/0.2.40-prediction.txt`. The doors move in homebrew-wolf#52.

This note records four things:
- what was measured after the tag, against the published archives;
- why aspen does not build with the bundled std since 0.2.37, and the change it needs;
- the plants that held the release's two version-keyed rows;
- how the host's pipe budget and disk floor met the takes.

## The published archives, read back

All five wolf assets and all six lupin assets were downloaded fresh on each of two Linux hosts, each into its own new empty directory. Every SHA-256 was read twice on each host, and all four reads of each asset equal the release metadata. The lupin assets were read before the wolf takes at the final head; the Linux x86-64 lupin archive those takes used is the one in this table.

| Asset | SHA-256 |
| --- | --- |
| wolf-0.2.40-aarch64-apple-darwin.tar.gz | 3f18a56562fb089da05a50a612e031cd297f20a617b9e6e7ada768fbeb69e72b |
| wolf-0.2.40-aarch64-unknown-linux-gnu.tar.gz | cea26eea215e7cdba60db2870c0eb4d2c5738065046ec5fe6e0eec7da55e48cd |
| wolf-0.2.40-x86_64-pc-windows-msvc.tar.gz | df7ef8037df30394de4909217d42861732a6ed60c0991eb00990b4bc5441cbd8 |
| wolf-0.2.40-x86_64-unknown-linux-gnu.tar.gz | bc2020e0b7400c18162a6072c299c65c0582ea74a0eba22aaa57ca04ae13e7e1 |
| wolf-rt-0.2.40-x86_64-unknown-linux-musl.tar.gz | 6d30737a02f02f9b19f6ef5b8b381802f0d0e2a179c37a4c1f638bca5e05c6da |
| lupin-0.1.62-aarch64-apple-darwin.tar.gz | 228790ef3aef7605a2f9524abdcb6de5c8a5fa179437d82f3d3af262495a4e7f |
| lupin-0.1.62-aarch64-unknown-linux-gnu.tar.gz | f5391308ad9cd8ae1e31a42e7a17c1453679e2a0ac03f83783c4ac52e650ef2e |
| lupin-0.1.62-x86_64-pc-windows-msvc.zip | 423ba4073179b72d925e9e835a3b035ca0fdfcb69042dd0eab0732e23141e537 |
| lupin-0.1.62-x86_64-unknown-linux-gnu.tar.gz | 3b3bf9c23a9e545a4a99e65a5183d0c5e71e91d6bfb8a189147e2701c03e177f |
| lupin-0.1.62-x86_64-unknown-linux-musl.tar.gz | 1cfbe548262c3bd0b75be731adbf88caf8239d8f4a5b75cb6727052eb0b33057 |
| lupin.exe | 035da8a67e1d658e7de86b901c5671be0722386565956b304b21ae934662e687 |

The archives' own answers:
- Every archive run prints a bare tag version (`wolf 0.2.40 (wolfgang, pin b333b9d)`, paired with lupin 0.1.62 at spec pin `5f1f1c94`; `lupin 0.1.62 (wolf-interp, reference interpreter at pin 5f1f1c9)`). That is Linux x86-64 in an Ubuntu 24.04 container, plus hosted macOS arm64, Linux x86-64 and Linux aarch64. Windows was not executed.
- The six GNU executables (`wolf`, `wolf-cimport-worker` and `lupin`, x86-64 and aarch64) need at most GLIBC_2.34.
- The musl runtime's `libwolf_rt.a` is bc11aeb97ba08c0081a67197f8dae4152ef61dbec82e98a70168dc5213c7a7bf (49,736,138 bytes). That equals the release notes and the release-workflow dry run at `b333b9da` (43335865681).
- With `WOLF_STD` absent, the bundled std (`std/STD-REV` be1d10fa) answers the never-set environment question `false`, checked and native.

## The one-wait rows against the published archives

`corpus/os/ready_rows.lu` (s290) is the one-wait row that reaches no terminal, deterministic on every host. It was held to its own `check:` line on the three tiers, by the archive for that host:

| Host | checked | native | release |
| --- | --- | --- | --- |
| Linux x86-64, Ubuntu 24.04 container | equal | equal | equal |
| Linux x86-64, hosted Ubuntu 22.04 | equal | equal | equal (release tier through clang-15) |
| macOS arm64, hosted | equal | equal | equal |
| Linux aarch64, hosted Ubuntu 22.04 | equal | refused by name, exit 2 | refused by name, exit 2 |

Linux aarch64 is served by neither compiled tier yet. The archive owes it a refusal by name, which is what the release workflow's own run gate holds it to. The hosted rows ran in run 43374583681 on the branch `r45-published-rows`, which holds only that workflow. Each runner read its own wolf and lupin archive twice against the digests above.

lupin 0.1.62 refuses the row by name at its first wait (`os_wait_ready`, at run), in the container and on all three runners, as its mirror says.

The same run read a host fact beside the release, for wolf-std issue 147: a child spawned in a new session with a pty slave on its standard descriptors, through fork, setsid, dup2 and exec, as the runtime's spawn does. `sleep` and dash leave the terminal alone and have no controlling terminal; bash takes the terminal as its controlling terminal when it starts. On the hosted Linux runners `/bin/sh` is dash and answers `controlling=no`; on hosted macOS `/bin/sh` answers `controlling=yes`, as `/bin/bash` does, and `/bin/dash` answers `controlling=no`.

## A one-wait program linked static

Two one-wait programs were linked static against glibc's `libc.a` by the published compiler, with `cc -static` in front of its link: the readiness-set fixture `crates/wolf_driver/tests/fixtures/ready/mixed.lu` and `corpus/os/ready_rows.lu`. Both are ET_EXEC with no PT_INTERP. Run on Linux, the fixture prints all eight of its lines, from `one_shot: [0, 1, 1, 1, 2, 1, 3, 1, 4, 1, 5, 1, 6, 1, 7, 1] meaning 0` through `fair: 50 of 50` to `closed_handle_add: io`, and the row prints its check line. The link warns that the runtime's `getaddrinfo` and `getpwuid_r` need the host's shared libraries at run time if they are called; both programs ran as above.

## Downstream against the published 0.2.40

These ran without any change to the repositories' sources, pins or expectations:
- pelt `45c4de11`: corpus 1036, 0 failed, 15 named pending; sessions 593 passed, 0 failed; native and release; tree clean after.
- boreutils `ff5396ad`: 107 binaries per tier with `--deny-warnings`; difftest against its pinned GNU coreutils 9.11, `16647 passed, 0 failed, 156 skipped`, dev and release; tree clean after.
- sniff `6dd382d4`: its Linux DWARF rows in its own pinned oracle image, all eleven steps exit 0 with no SKIP (live transcript `PASS session events=3` under gdb, `PASS true-value-join`). The rows script's own clean-tree check could not run inside sniff's image, which has no git; the tree was read from the host after the take: no tracked change.
- The tools whose pins are oldest, each program built by the published compiler with its bundled std and `--deny-warnings`, on the native and release tiers: scent `eb888dfc`, lope `d65bb03c`, lobo `39788b86`, mk `4716bec7` and cub `c72b94ae` (on its engine lair at `df11d213`) build on both tiers, and each tree is clean after. aspen `096245c8` does not; see below.

Which can move straight to 0.2.40, as measured here:

| Tool | Pin today | At 0.2.40 |
| --- | --- | --- |
| boreutils | 0.2.36 | builds on both tiers; difftest green on both |
| pelt | 0.2.36 | builds on both tiers; corpus and sessions green on both |
| scent | 0.2.36 | builds on both tiers (its own suites not run here) |
| lope | 0.2.36 | builds on both tiers (its own suites not run here) |
| lobo | 0.2.36 | builds on both tiers (its own suites not run here) |
| mk | 0.2.36 | builds on both tiers (its own suites not run here) |
| cub | 0.2.36, through its engine | builds on both tiers (its own suites not run here) |
| aspen | 0.2.36 | not without a two-line change, below |
| sniff | 0.2.38 | its DWARF rows green |

## aspen and the bundled std's `Local`

aspen `096245c8` is refused by the published 0.2.40 on both tiers, with or without `--deny-warnings`, exit 2:

```text
error[E0408]: this `Local` literal is missing the field `leap_correction_s`
   --> src/tfmt/tfmt.lu:69:5
error[E0408]: this `Local` literal is missing the field `leap_correction_s`
   --> src/zone/zone.lu:41:13
  ::: std://time/local.lu:715:12  pub struct Local {
  = note: every field is written explicitly — wolf has no defaults yet.
```

wolf-std `b6193f3` (2026-10-25, leap-aware local time) added a public field, `leap_correction_s: int`, to `std.time.Local`. wolf has no field defaults, so every literal of the struct must now name it. The std bundled since 0.2.37 carries that commit. The same aspen head, built by each published compiler (each archive read twice against its release digest; 0.2.36's equals aspen's own pin):

| Compiler | Bundled std | aspen `096245c8` | aspen with the two-line change |
| --- | --- | --- | --- |
| 0.2.36 (`5adf3d9`) | `78b9d3cb`, aspen's own std pin | builds, both tiers | refused, E0403: `Local` has no field named `leap_correction_s` |
| 0.2.37 (`e2757a7`) | `5b3a5994` | refused, E0408 | |
| 0.2.38 (`95ae082`) | `d4a4d4ca` | refused, E0408 | |
| 0.2.39 (`3fc86f7`) | `2411d868` | refused, E0408 | |
| 0.2.40 (`b333b9d`) | `be1d10fa` | refused, E0408 | builds, both tiers |
| 0.2.40 with `--std-root` at `78b9d3cb` | (aspen's pin) | builds, both tiers | |

The compiler is not at fault: 0.2.40 builds aspen against the std aspen pins. The refusal is the bundled std's, from 0.2.37 on. The change aspen needs is one line in each of its two `Local` literals, `leap_correction_s: 0,` (UTC with no leap correction), and it moves with aspen's pin, never before it.

Known limit, not fixed in 0.2.40: since wolf 0.2.37 (bundled wolf-std `5b3a599`), `std.time.Local` has a field `leap_correction_s`, and a program that writes a `Local` literal must name it (0 for UTC with no leap correction). Neither the 0.2.37 release note nor wolf-std's changelog names the field.

## The plants

The two rows of this release that a version keys were each planted on the final head, one file and one hunk, and each plant was red by name with the published lupin 0.1.62:
- `r45-plant-pairing` puts `PAIRING` back to 0.1.61. The metadata check fails `test_published_pairing` (`PAIRING names 0.1.61 where the release pairs 0.1.62`), and the sibling row fails `the_pairing_matches_the_sibling_lupin_when_present` (`the published 0.1.62 sibling refuses a 0.1.61 PAIRING`).
- `r45-plant-ready` lists 0.1.62 among the lupins that predate the one-wait mirror. `lupin_refuses_the_one_wait_by_name` fails (`the published 0.1.62 is held to a pre-mirror answer it never gives`).

Both plant records verify against the green gauntlet record at `b333b9da`. Both branches were deleted after the merge.

## The pipe budget and the disk floor

The release gauntlet runs the container rows on the host's rootless engine, as at 0.2.39. The pipe budget met one take again: the candidate posture take at `3d6526e6` read an 8192-byte pipe in fs_gaps' `the_pipe_a_clipboard_writer_feeds_on_every_machine` (`REPORT taken=8192`), because the host user's pipe pages had passed `fs.pipe-user-pages-soft` under other jobs. The final take read a fresh pipe's size before and after the container rows, and both readings were 65536; all six nonblock reports in it read `taken=65536`.

Every take's preflight holds a 35 GB disk floor on the host. The pairing plant's first take was refused there (30.4 GB free) and ran no step; it ran once a stale build tree was pruned, with its preflight run alone first.

## Records

| Record | SHA-256 | End |
| --- | --- | --- |
| the published archives' answers, the one-wait rows, two static links, pelt, boreutils, sniff and the oldest-pin tools | 3c2ed2f1881f69a193238092f522a7e1a289f6e5e07aa19033aa465605c0e931 | `RECORD-END worst=1 steps=14` (the tools step alone: aspen, above) |
| aspen against 0.2.36 to 0.2.40, its std pin and the two-line change (a probe log) | dd42226098f8280e2ffa3e3b8f7508c83fcee0dad4a70f6af1aff6b4b163112c | every row as in the table above |
| the release gauntlet at `b333b9da` | 7e12a1671522eb7995b3c93e1043db39bdac7129e4bd249e8667e20bf4799186 | `RECORD-END worst=0 steps=20` |
| the release shard at `b333b9da` | 3471e8bfc6ee6d434f150a781777b74d20988c6792b84af6c2acaf388717de36 | `RECORD-END worst=0 steps=9` |
| the nightly's property job at `b333b9da` (a measurement) | 621f38ff2194612e7eb511ac93006c34b771e11a939696e2e71c502f3a52c9b2 | `RECORD-END worst=101 steps=5`, `blast_radius` alone (issue 1057) |
| the one-wait plant | a4a5325ed859e58ddefc19c269828ca6426a216ffb6d22a1a4654efb1e7abc86 | `RECORD-END worst=101 steps=5`, red by name |
| the pairing plant | d1b570c0f0d34a24c9370ae1d22d6f7b5c85ee751b6f6b075b32984c74e181ae | `RECORD-END worst=101 steps=6`, red by name |
