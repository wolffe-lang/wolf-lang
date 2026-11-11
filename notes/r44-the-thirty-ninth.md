# r44: the thirty-ninth release, wolf 0.2.39 with lupin 0.1.61

wolf 0.2.39 was released from `3fc86f77`, paired with lupin 0.1.61 and the bundled std at wolf-std `2411d868`:
- wolf: wolf-lang#1097, release run 42990889186;
- lupin: from `a10cbe57`, wolf-interp#365, release run 42946919991.

The release note is `docs/releases/0.2.39.md`, and the prediction is `docs/releases/0.2.39-prediction.txt`. The doors move in homebrew-wolf#50.

This note records two things:
- what was measured after the tag, against the published archives;
- how the release gauntlet ran the container rows, which the debug shard skips.

## The published archives, read back

All five wolf assets and all six lupin assets were downloaded fresh on each of two Linux hosts, each into its own new empty directory. Every SHA-256 was read twice on each host, and all four reads of each asset equal the release metadata.

| Asset | SHA-256 |
| --- | --- |
| wolf-0.2.39-aarch64-apple-darwin.tar.gz | 63de947671c240fc8be3e3480bb36321b0a6ac0b5e12e16bdae164d9050cfde0 |
| wolf-0.2.39-aarch64-unknown-linux-gnu.tar.gz | 390b6548d6bd8c7eac3ae0a1d09099437564418440de782caca45954463dd344 |
| wolf-0.2.39-x86_64-pc-windows-msvc.tar.gz | dbb958fe64e348e9b4421553dfa7718a223d36515b09a6d88ca17d0d80adeb9a |
| wolf-0.2.39-x86_64-unknown-linux-gnu.tar.gz | 53f75d2b2dd79c0532b21920bc9f073098154aa0133caa2405209a1ef2af0457 |
| wolf-rt-0.2.39-x86_64-unknown-linux-musl.tar.gz | 6cc429551e34935a1e7b6f3edd77ff034ff5be3fb6b7f399f33fe4132b9f594e |
| lupin-0.1.61-aarch64-apple-darwin.tar.gz | 7236424618f5bbf1e5f7693cd6333df90b715e1401b08b795668cbb1425ed5af |
| lupin-0.1.61-aarch64-unknown-linux-gnu.tar.gz | 3a2b92423e53a6b539e30b8a4f055c09ccac30853f332da3305966550d1ac7d3 |
| lupin-0.1.61-x86_64-pc-windows-msvc.zip | 06c4cc82f1ab7d8f6acdd7d21f43b519a2927c22344947d6c068aec588138436 |
| lupin-0.1.61-x86_64-unknown-linux-gnu.tar.gz | 61ce849eac0b531803e5bdf1fe07d91d3d69ecb57f83c5e7eb7c5d553cbef41b |
| lupin-0.1.61-x86_64-unknown-linux-musl.tar.gz | 7e9f5363b98c60e32700623f154460182fa79dd518e9d770005504f18548b9da |
| lupin.exe | 5635d6e05c19b0d878f3e99b5fbac932229be5a61f86ab48f9b784454288affa |

The archives' own answers:
- Every archive run prints a bare tag version (`wolf 0.2.39 (wolfgang, pin 3fc86f7)`, paired with lupin 0.1.61 at spec pin `5f1f1c94`; `lupin 0.1.61 (wolf-interp, reference interpreter at pin 5f1f1c9)`). That is Linux x86-64 in an Ubuntu 24.04 container, plus hosted macOS arm64, Linux x86-64 and Linux aarch64. Windows was not executed.
- The six GNU executables (`wolf`, `wolf-cimport-worker` and `lupin`, x86-64 and aarch64) need at most GLIBC_2.34.
- The musl runtime's `libwolf_rt.a` is db50108f80eae4423c44e708c739f17f09ceaa3314196b3a31fd563b2b36748d (47,693,716 bytes). That equals the release notes and both release-workflow dry runs (42947097179 at `b12b1e71`, 42988685093 at `3fc86f77`).
- With `WOLF_STD` absent, the bundled std (`std/STD-REV` 2411d868) answers the never-set environment question `false`, checked and native.

## The session and pty rows against the published archives

`corpus/os/session_rows.lu` (s290) and `corpus/os/pty_rows.lu` (s292) are the rows of the new calls that reach no terminal, deterministic on every host. Each was held to its own `check:` line on the three tiers, by the archive for that host:

| Host | checked | native | release |
| --- | --- | --- | --- |
| Linux x86-64, Ubuntu 24.04 container | both rows equal | both equal | both equal |
| Linux x86-64, hosted Ubuntu 22.04 | both equal | both equal | both equal (release tier through clang-15) |
| macOS arm64, hosted | both equal | both equal | both equal |
| Linux aarch64, hosted Ubuntu 22.04 | both equal | refused by name, exit 2 | refused by name, exit 2 |

Linux aarch64 is served by neither compiled tier yet. The archive owes it a refusal by name, which is what the release workflow's own run gate holds it to. The hosted rows ran in run 43009133068 on the branch `r44-published-rows`, which holds only that workflow. Each runner read its own wolf and lupin archive twice against the digests above.

Two earlier runs on that branch were red for reasons in the workflow, not the archives:
- the first gave the Linux runners neither `lld` nor clang;
- the second built the release tier without `WOLF_CLANG=clang-15`, and held Linux aarch64 to rows it does not serve.

lupin 0.1.61 runs the pty rows (exit 0) and refuses the session rows by name, as its mirror says.

The s292 PAX fixture (`notes/s292-evidence/pax/ptyrows.lu`) and its helper (`crates/wolf_driver/tests/fixtures/pty/helper.lu`) were linked static against glibc's `libc.a` by the published compiler, with `cc -static` in front of its link, as s292's rig did. Both are ET_EXEC with no PT_INTERP. Run on Linux, the pair prints all eleven rows the PAX boot leg holds, from `pax: pair handles true, master a terminal true, slave a terminal true` to `pax: gone true`. That includes the spawn in a new session (`pax: spawned: Ctrl-G interrupts true, status 7`). Linux's table lacks opcodes 11, 15 and 17, as the release note says.

## Downstream against the published 0.2.39

These ran without any change to the repositories' sources, pins or expectations:
- pelt `b80d6570`: corpus 1035 passed, 0 failed, 14 named pending; sessions 593 passed, 0 failed; native and release; tree clean after.
- boreutils `ff5396ad`: 107 binaries per tier with `--deny-warnings`; difftest against its pinned GNU coreutils 9.11, `16647 passed, 0 failed, 156 skipped`, dev and release; tree clean after. Its pin is 0.2.36 and can move straight to 0.2.39.
- sniff `6dd382d4`: its Linux DWARF rows in its own pinned oracle image, all eleven steps exit 0 with no SKIP (live transcript `PASS session events=3` under gdb). Its pin is 0.2.38; the re-pin is its own lane.

## The container rows in the release gauntlet

CI's Linux job runs the rows that start a container (fork, group, ids, job-wait, the fs gaps, two machines and the rest) in rootful docker on the runner. Inside the local gauntlet's container there is no engine, so the debug shard prints those witnesses as SKIP: 58 lines, as at 0.2.38.

At 0.2.39 the release gauntlet added a step that runs the eleven test files that start one, with CI's `WOLF_{FORK,GROUP,IDS,JOBWAIT}_REQUIRE_CONTAINER=1`:
- Each container is a sibling on the host's rootless podman, reached through a socket the take starts and stops.
- Each command runs under `taskset` on the take's CPUs.
- A row that cannot start its container is red.

Three differences from CI are named in the record:
- **Core rows.** The core-dump rows stay a loud SKIP: the host pipes cores to a program, which would write the core outside the test's directory.
- **The cpuset row.** `two_machines::stack_overflow_is_named_on_both_tiers_and_threads` asks the engine for a cpuset, and a rootless engine there has no cpuset controller. Its container exited 126 before the row started. The row ran green at `3fc86f77` in the hosted Linux job of run 42956093408, which is its evidence, and later takes leave that one row out in a step that names it.
- **The pipe budget.** Linux gives a new pipe 8192 bytes instead of 65536 once a user's pipe pages pass `fs.pipe-user-pages-soft`. The rows' containers run as the host user, which other jobs share. In one take, two fs_gaps rows read the smaller pipe (`REPORT taken=8192`, `rg1: took 8192 of 1048576`) where they hold Linux's 65536, at a head where the take before and the hosted release tier passed them. The final take reads a fresh pipe's size before the rows, waiting for 65536, and again after them. Both readings were 65536.

The final record at `3fc86f77`: `RECORD-END worst=0 steps=20`, SHA-256 72815220e8b402568a3fa811a031b5a96a37e32b11853703f9bfdc8f06d4dac5. The container rows passed 83 tests in 11 suites, 0 failed, with no SKIP line naming a container. The debug shard passed 4,207 tests in 372 suites, 0 failed.

## Records

| Record | SHA-256 | End |
| --- | --- | --- |
| the published archives' answers, rows, PAX fixture, pelt, boreutils, sniff | dcadca73b709d4e22e11d7e35d292aa5bcb785d382356d235aac45bc70a54b25 | `RECORD-END worst=1 steps=13` (the GLIBC step alone, which missed the aarch64 lupin archive) |
| the GLIBC step, retaken alone | 46a3d65c188c489bc5d15c850a9da87c28b42858cb03ff8f0faf049078b685cc | `RECORD-END worst=0 steps=4`, six executables at GLIBC_2.34 |
| the release gauntlet at `3fc86f77` | 72815220e8b402568a3fa811a031b5a96a37e32b11853703f9bfdc8f06d4dac5 | `RECORD-END worst=0 steps=20` |
