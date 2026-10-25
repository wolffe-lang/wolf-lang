# s266 — the small ones before 0.2.36 (#932, #936, #923, #935, #925)

Lane note. **Opus 5.5. Crawl 54.** Branch `s266` off wolf-lang trunk
`a5378e3f` (s263's variadic clause, s264 and s265 before it). Contract:
`sprints/compiler/s266-the-small-ones.md` (planning repo). Linux work and
the gauntlet on kasumi CPUs 4-7 under the lane lock, in the lane's own
rootless container; macOS witnesses on nomad under RULES §4; editing on
the Mac.

## Prediction (written before the first change, 2026-10-25 01:24 EDT)

Read before predicting: the five issues whole, `spec/11-os.md`'s
`[os.env.argv]`, `[os.fs.error]`, `[os.proc.exec]`; `wolf_rt/src/os.rs`;
`wolf_sema/src/check.rs`'s `synth_range`; `.github/workflows/release.yml`;
`xtask dist`; pax `tools/mkwolf`, `user/wolf.pin`, `user/wolf.sums`,
`user/wolf-link.sums` (pax `09471da4`, read only); pelt's
`notes/sh26-evidence/probe/wolf-spawn-errno/` at `f3b2160` (read through
the API, the checkout untouched); s263's `taskset-issue.txt`.

- **Which item needs a clause beyond its one line: none, and #936 the
  only one that needs words at all.** Two of the five are already fixed
  on trunk and in no release, so their issues describe 0.2.35:
  - #923: s261's `b5526a0e` (2026-10-23) reads argv with `args_os` and
    traps `argument-utf8` by index with the message `[os.env.argv]`
    already specifies; `env_argv_lanes.rs` witnesses it on unix. The
    spec says what to do (trap by index), so **R-s266-2 is answered by
    the clause: (A)**; no clause change. The lane's part is the macOS
    witness through a pty and the Linux one in the container.
  - #936: s260 (`4505f6bb`, `ec1e92ce`) made `[os.fs.error]` name the
    spawn family and `os_exec`, and `spawn_refusal` / the checked
    machine's twin record the host's number; pelt's `./busy: spawn io
    os_error 0` is 0.2.35's answer. I predict the trunk build answers
    pelt's probe with 26/8/13 on linux and that E2BIG (7) also arrives,
    so the clause gains one sentence in `[os.proc.spawn]` naming the
    four witnessed numbers per host and their rows (`io`, `io`, `io`,
    `denied`), and no runtime change — unless a spawn path (the
    `os_spawn` Command path or `os_spawn_with`) drops the number, which
    would be the one code change.
  - #932 is diagnostic text (no clause; `[expr.else]`'s binding stays),
    #935 is the release workflow, #925 is test plumbing.
- **macOS and `ETXTBSY`: not reported.** APFS lets a file held open for
  writing be executed, so the `busy` witness spawns (or answers some
  other row) on macOS where linux (kasumi, kernel 7.2.8) says `ETXTBSY`
  26. ENOEXEC 8, E2BIG 7 and EACCES 13 are the same numbers on both
  hosts.
- **The gauntlet's wall time on 4-7:** about 2 h 30 min for `cargo
  xtask ci` from a cold `target/` (the contract says 2-3 h; s263's runs
  on hasu's block took a similar time).
- **The musl archive's bytes against the ISO's:** `pax/user/wolf-link.sums`
  has no runtime line: it holds the link toolchain (`ld.lld`, `cc`,
  `sysroot/crt1.o`, `crti.o`, `crtn.o`, `libc.a`, `libunwind.a`). The
  ISO's runtime digest is in `user/wolf.sums`: `libwolf_rt.a`
  `9d859b9e…739c`, 45,363,160 bytes, built by `tools/mkwolf` from
  v0.2.35 (`cf0c13d3`). The dry run builds this branch, a different
  source, so its bytes differ; I predict mkwolf's recipe (the three
  packages in one cargo invocation, `+crt-static`,
  `relocation-model=static`, the three path remaps, rust 1.97.1) run on
  a GitHub runner at v0.2.35 reproduces `9d859b9e…` byte for byte, and
  that `libwolf_rt_none.a` built on the same runner equals the GNU
  archive's, so no second rt-none ships.
- **#925's size:** s263's list names 13 files; the tree has 23
  `crates/wolf_driver/tests/*.rs` files spelling `taskset`, some of them
  inside an inner container (`group_lanes`, `ids_lanes`, `job_wait_lanes`).
  I predict every host-side site moves to the helper and the in-container
  sites take their list from the helper too (the container's own
  allocation), with no test losing a row.
- **#932:** R-s266-1 (A), the note on the existing refusal, needs the
  refusal to carry an optional note to the driver; I predict a
  syntax-side classification (the refused range IS the fallback operand
  of an `else`) rather than threading a field through the ~120 `NotYet`
  sites.

## Inputs verified

- wolf-lang trunk `a5378e3f` (`git fetch`; the branch's base, unchanged at
  the hand-back).
- Controls, each hashed twice against r40's read-back: wolf 0.2.35 linux
  x86-64 `4cc4045b…dad1a` and macOS arm64 `240b84d0…f042a`; lupin 0.1.56
  linux x86-64 `ad703550…822cc` (`PAIRING`'s).
- wolf-std `2386522e` (STD-PIN) as `WOLF_DIST_STD_SRC`; pax `09471da4` and
  pelt `f3b2160` read only.
- The container (`container/`): archlinux:base-devel with kasumi's
  `/usr/bin` snapshot over it, the host's `/usr/lib`, `/usr/share`,
  `/usr/include` read-only, rust 1.97.1 from the host's rustup read-only;
  `--cpuset-cpus 4-7 --pids-limit 16384 --ulimit nproc=32768:32768
  --ulimit core=0 --memory 12g --network none`; pid 1 `podman-init` as the
  userns root, the command dropped to uid 1000 by `setpriv` (no
  capabilities, `NoNewPrivs`), then `s266-exec-clean` (every signal to its
  default, an empty mask). `container/selftest.log`: `kill -0 1` refused,
  `pids.max=16384`, nproc 32768, `Cpus_allowed_list: 4-7`, the shim's
  mapping and an ignored `SIGPIPE` through it.

## Rulings (recommended, implemented; never decided here)

- **R-s266-1** (#932): (A) a note on the existing refusal. Recommended A.
- **R-s266-2** (#923): (A) trap by index, as `[os.env.argv]` already
  says. Recommended A; trunk does it (s261).
- **R-s266-3** (#936, new): `os_exec`'s own `PATH` search decided
  `not_found` and `denied` without the host's number, where the spawns'
  `execvp` answers 2 and 13 for the same name. (A) the search answers the
  number `execvp(3)` reports (`ENOENT`, or `EACCES` when a candidate lacked
  the execute bit), on every machine; (B) leave 0 as "decided before the
  host". Recommended A; implemented (`97931f91`, `96b8ae6c`).
- **R-s266-4** (#925, new): a pin request that cannot fit. (A) it fails by
  name when `WOLF_TEST_CPUS` named the allocation, and skips loudly (as
  today's four-CPU checks do) when the host itself is smaller; (B) fail in
  both cases (a three-CPU macOS runner never asks: the pins are linux's).
  Recommended A; implemented in `test_cpus` (the gates that already
  refused to skip on linux, `atomic_witness`, `parked_tasks`,
  `reactor_lanes`, `stdfd_lanes`, use `pin_required`, which fails either way).

## Hits and misses

- **Spec clauses: hit, with one miss of degree.** #923 needed none (the
  clause already said A); #932, #935, #925 none. #936 needed more than one
  sentence: the probe found two machine-made refusals with 0 (below), so
  `[os.fs.error]` gained a paragraph and `[os.proc.exec]`'s program step a
  clause (`1f83e279`). Anchors regenerated: 705 keys at trunk and at the
  head, none only on either side (`kasumi/anchors-xtask-argv.log`).
- **#936 "no runtime change": miss.** Native and the spawn family on the
  checked machine carried every number already (s260). The checked
  machine's `os_exec` on a name holding `/` answered 0 for a missing file,
  a directory and a file without an execute bit (native: 2, 13, 13), and
  `os_exec`'s own bare-name search answered 0 on every machine (the spawns:
  2, 13). Two code changes, each after its row went red.
- **macOS and `ETXTBSY`: hit.** nomad (macOS 26.4, APFS) runs a script
  another descriptor holds open for writing: the four spawns exit 0 and
  `os_exec` replaces the program. kasumi (linux 7.2.8): `io`, 26, on all
  five calls and three machines.
- **The numbers:** `ENOENT` 2, `E2BIG` 7, `ENOEXEC` 8, `EACCES` 13 on both
  hosts, all five calls, native, release and checked; `ETXTBSY` 26 linux
  only. The published 0.2.35 answers 0 for every one, on both hosts, and
  on macOS still runs a `#!`-less file under `/bin/sh` (status 126)
  (`spawn-errno/macos.log`, `kasumi/spawn-errno-linux-first-probe.log`).
- **The musl bytes: hit, and stronger than predicted.** The dry run's
  `libwolf_rt.a` (branch head) is `b6eb9949…e72b`, 46,641,340 bytes. The
  probe branch `s266-probe-musl-0235` (v0.2.35 plus `dist-rt` alone, run
  40168451539 on ubuntu-22.04) built `9d859b9e…739c`, 45,363,160 bytes:
  **byte for byte the PAX image's runtime** (`pax/user/wolf.sums`). The
  freestanding `libwolf_rt_none.a` the musl job builds equals the GNU
  archive's (`6b9c4852…61b6`), so it ships once.
- **#925's size: hit.** 23 test files, every host-side and in-container
  site through the helper; no row lost (157 passed before the shim's
  descriptor fix and after it).
- **#932's shape: miss of mechanism, hit of intent.** No syntax walk and no
  field on `NotYet`: the checker records each `else` fallback's span, the
  refused range that IS one gets its own construct name, and
  `wolf_sema::ledger_note` maps that name to the note the driver prints.
- **Gauntlet wall time: a miss.** Take 2 ran 17 minutes to its
  `bench-gates` red (06:46-07:03Z) from a fresh `target/`; the steps
  after it took 9 minutes alone. The prediction was about 2 h 30 min.

## 1. #932 — the `else` fallback and the index

- `12250eec` the three corpus rows (`corpus/rows/else_fallback_range_refused.lu`
  `check: pass`/`phase: resolve`; `else_fallback_paren_slice.lu` and
  `else_fallback_bound_slice.lu` `run(exit=0, stdout="[ value]")`) and the
  driver row (`refusal_site.rs`): red at this commit, the build's first line
  still "open-ended or end-relative ranges (slicing)"
  (`kasumi/item1-witness-red-excerpt.txt`).
- `df29dd01` the fix: `else_fallbacks` in the checker, `ELSE_FALLBACK_RANGE`,
  `ledger_note`; the build prints `= note: the `else` fallback extends to
  the end of the expression; parenthesise `(… else 0)`` under the source
  line, `conform-run` records `x-unsupported-note`. Green on kasumi and
  nomad (`kasumi/item1-row-red-then-green.log`, `nomad/item1-refusal-site.log`).
- `6b7d04f1`, `232c83b4`, `354221d9` test fixes (the span found by the
  code's spelling; the plain `(2..)` and the parenthesised fallback `else
  (2..)` keep the old name and no note, each in its own module).
- `1549db55` the rows' lex and decl snapshots.
- `docs/diagnostics.md` regenerated: unchanged (a refusal is not a code);
  `diag-catalog --check` exit 0; `xtask corpus` 1,228 files, 0 bad.
- Not done here, for the guide's owner: `[gram.amb.else]`'s binding stays,
  so the guide's "bind the fallback first, then slice" line (the issue's
  second ask) belongs in the planning repo's guide, which this lane may not
  write.

## 2. #936 — the spawn family and `os_exec` carry errno

- `1f83e279` the clause (`[os.fs.error]`, `[os.proc.exec]`).
- `0599df8a` `spawn_errno_lanes.rs` (+ `fixtures/spawn_errno/main.lu`):
  every reason on all five calls and three machines, `busy` held open by
  the test, 8 MiB of arguments (past linux's 6 MiB ceiling at any stack
  limit and macOS's `ARG_MAX`), bare names through `PATH=./p:/usr/bin:/bin`.
  Red at this commit on kasumi and nomad: `os_exec bare-missing: not_found
  os_error 0` (`kasumi/item2-witness-red-excerpt.txt`,
  `nomad/item2-spawn-errno-red.log`).
- `97931f91` native: `exec_now` sets `ENOENT`/`EACCES` on the search's own
  refusals. `96b8ae6c` checked: `exec_resolve` keeps its look's number, or
  `EACCES` for a directory or a file without an execute bit, and the
  search's. `14b7286a` a clippy fix in the test.
- Green at the fixes on kasumi (native, release, checked; with
  `spawn_noexec_lanes`, `exec_lanes`, `process_calls_lanes`,
  `byte_surface_lanes`, `wolf_rt` 343 and `wolf_mem` 32 unit tests) and on
  nomad (`kasumi/item2-row-red-then-green.log`, `nomad/item2-spawn-errno-green.log`).
- lupin is not run by the new test: 0.1.56 leaves `os_error()` 0 on every
  spawn (`spawn_noexec_lanes.rs` pins it); the mirror is wolf-interp's (§7).

## 3. #923 — non-UTF-8 argv on macOS

- Already fixed on trunk by s261 (`b5526a0e`, 2026-10-23, in no release):
  `args_os`, the trap by index, the message `[os.env.argv]` names.
- `2b678e6b` a pty witness in `env_argv_lanes.rs` (`unix` module): the
  program on a fresh pseudo-terminal as its controlling terminal, `-F` and
  byte `ff`, both native tiers: exit 134, `wolf-trap: argument-utf8`,
  `argument 2 is not UTF-8; read it with env_args_bytes()`, no `panicked`,
  no `fatal runtime error`; the byte reader reads `3 1 255`.
- macOS (nomad), through `argv/pty-run.py`: **0.2.35 aborts exactly as the
  issue says** (`env.rs:876:51`, `called Result::unwrap() on an Err value:
  "\xFF"`, `failed to initiate panic, error 5`, signal 6) on native and
  release; the branch traps by index, exit 134 (`argv/macos-pty.log`). The
  whole `env_argv_lanes` (9) green on nomad (`nomad/item3-env-argv-lanes.log`)
  and in the kasumi container (`kasumi/anchors-xtask-argv.log`).

## 4. #935 — the musl runtime in the release

- `a01663d1` `xtask::release`: mkwolf's flags, the notes section, the
  digest-line parser and the read-back's rules (6 unit tests).
- `e6946551` `cargo xtask dist-rt` (the build, the `nm -u` check that no
  glibc large-file name is asked for, the stage, `SHA256SUMS`, the tarball,
  the notes section) and `cargo xtask release-readback`.
- `8dcd25e0` `release.yml`: `dist-musl` (uploads by release id, appends
  the marked section to the body, replacing a re-run's), `readback`
  (every draft asset by id, hashed twice; the musl archive unpacked;
  `publish` needs it), `publish` wants four host archives and one musl
  runtime, `workflow_dispatch` with `dry` (true only) runs every build and
  the read-back with nothing uploaded; `grammar-mirror` and `publish` only
  on a tag push.
- **Dry run 40167549421** (s266 at `8dcd25e0`, no tag): every job green in
  10.5 min; the read-back table: the four host archives
  (`553c58b2…` darwin, `8a6e640a…` linux arm64, `504df9a2…` windows,
  `df7ccf5d…` linux x86-64), `wolf-rt-0.2.35-x86_64-unknown-linux-musl.tar.gz`
  `05288b7b…57bb`, its `libwolf_rt.a` `b6eb9949…e72b`; 1,652 undefined
  symbols, none of `open64`, `fstat64`, `stat64`, `lseek64`.
- **Probe 40168451539** (`s266-probe-musl-0235`, never merged): v0.2.35's
  runtime reproduced, `9d859b9e…739c`, 45,363,160 bytes = pax `user/wolf.sums`.

## 5. #925 — the affinity helper

- `c58a00cb` `tests/test_cpus/mod.rs` (`WOLF_TEST_CPUS` or
  `sched_getaffinity`; `subset`, `pin`, `pin_required`, `Pin::label`;
  `taskset` from `PATH`; a named allocation outside the process's affinity
  fails by name) and `test_cpus_helper.rs` (4 tests). Then one commit per
  file, `d1459f60`…`dca7496c`: 23 files, every `taskset -c 0-3`, `-c 0`,
  `/usr/bin/taskset` and two_machines' `--cpuset-cpus 0-3`.
- The row (kasumi CPUs 4-7, the lane's container, all 23 pinning gates,
  `--no-fail-fast`): before, at `14b7286a`, **every request the gates made
  was outside the block — 180 of 180 translated by the shim** (167 `0-3`,
  13 `0`), 153 passed (`kasumi/item5-before*`); after, at `dca7496c`: 180
  requests, **0 translated** (167 `4-7`, 13 `4`), 157 passed (the four new
  helper tests with them), 0 failed; natively (the host's real `taskset`
  mounted over the shim, `WOLF_TEST_CPUS=4-7`): 157 passed, 0 failed,
  every label `taskset -c 4-7` or `taskset -c 4`
  (`kasumi/item5-after-and-native.log`). A first before-take (149 of 149
  translated, `kasumi/item5-before-take1*`) stopped at three
  `stdfd_lanes` reds that were the shim's own (below).
- **A shim defect, found and fixed in the lane's container only:** a POSIX
  sh shim reads its own script from the lowest free descriptor, so a
  caller that closed 2 (`stdfd_lanes` closes 0, 1 and 2 in turn) found the
  script open there in the program. The shim now closes exactly that
  descriptor before it execs (`container/taskset-shim.sh`); with it the
  shim and the real `taskset` leave the same descriptors. RULES §4's shim
  note on hasu should carry this (§7).

## 6. Gauntlet, plants, PR

- **The gauntlet** (`cargo xtask ci`, kasumi CPUs 4-7 under the lane lock,
  the lane's container: the shim, `--pids-limit 16384 --ulimit
  nproc=32768:32768`, a foreign pid 1, never `--pid=host`; the verdict is
  the command's own exit status). Known flake wolf-lang#631
  (`region_copyout_lanes::a_loop_keeping_a_scalar_is_flat`) is named
  here, never waived; it did not fire. Logs: `kasumi/gauntlet/`.
  - Take 1 at `52824068` (06:30-06:44Z): exit 1 in `test`, `wolf_wir
    --test lower_corpus` alone: the corpus lowering ledger lacked the
    three new #932 rows. `e65b89e5` adds exactly those three lines.
  - Take 2 at `e65b89e5` (06:46-07:03Z, a fresh `target/`): every step
    through `midend-rate` green (fmt, fmt-fixpoint, clippy, rt-none, test,
    fuzz-check, deps-check, corpus, abi-check, debug-check, midend-rate);
    180 pinned requests, none outside 4-7. Then exit 1 in `bench-gates`,
    the start-up row alone: a release-tier hello world makes **23 more
    system calls than its C twin, recorded 12** (wolf 58, C 35). **The
    environment's, not this pass's:** the bench step alone at trunk
    `a5378e3f` in the same container on the same target reads the same
    58 / 35 / 23, and the two wolf traces differ only in `getrandom`'s
    bytes (`bench-startup.log`, `wolf-trace-trunk-vs-head.diff`). The
    hello's `NEEDED` list is `libm.so.6`, `libgcc_s.so.1`, `libc.so.6`:
    the driver links `-lm` unconditionally (wolf-lang#951), and this
    container runs kasumi's own Arch userland, whose linker keeps an unused
    library (clang wraps only `-lgcc_s` in `--as-needed`), where the
    ubuntu rig the 12 was recorded on drops it. The loader mapping
    `libm.so.6` is 10 of the 11 extra calls (the `ld.so.cache` stat,
    `openat`, `read`, `fstat`, four `mmap`, `close`, and its RELRO
    `mprotect`); the eleventh is unattributed. The row counts the loader's
    work, so it depends on the host's linker defaults (wolf-lang#952).
    Neither is this pass's; no waiver: CI's ubuntu lint-test is the row's
    instrument and was green at `e65b89e5` (tier pr 40170282076, tier full
    40170422660).
  - The steps `xtask ci` did not reach after that red, each alone at
    `e65b89e5` in the same container: spec-extract, conformance,
    print-gate, diag-catalog, doc-catalog, fmt-lu, fmt-fuzz, audit-surface,
    dist-smoke, differ-self, lane-coverage — every one exit 0
    (`after-bench.log`).
  - Seen in the same run, present at trunk (the same 34 lines at
    `a5378e3f`): bench-gates' IR-volume sweep builds eight `phase: run`
    corpus files that need their sibling `std/` fixture without
    `--std-root`, so they fail E0301 and are never measured —
    `corpus/methods/{comb_map_filter_fold,comb_sorted_ord,comb_sort_enumerate_zip,home_range_str_map,method_is_free_call,wordcount_par,wordcount_serial}.lu`
    and `corpus/rows/ctor_arg_qualified/main.lu` (wolf-lang#953). The
    sweep measured 720 files at trunk and 722 at `e65b89e5` (the two new
    compiling #932 rows), 0 regressed.
- **Plants** (each from `dca7496c`, a draft PR, never merged, no label):
  - `s266-plant-else-note` (#944, `9a8dc604`): `ledger_note` returns
    `None`; red by name in `refusal_site::an_else_fallback_range_names_the_fallback_and_says_to_parenthesise`,
    the PR's own `tier pr` run 40168807568 (lint-test's `test` step).
  - `s266-plant-enoexec` (#945, `337281dd`): `ENOEXEC` leaves `os_error()`
    0 natively; red by name in `spawn_errno_lanes` and
    `spawn_noexec_lanes`, run 40168808623.
  - `s266-plant-musl-digest` (#946, `b7b79813`): the notes lose the musl
    archive's digest line; red by name in the release dry run's
    `readback` job, run 40168787285 (and xtask's
    `release::tests::the_notes_section_parses_back_to_its_two_digests` in
    the PR's run 40168811446).
- **README** read against the pass: the archive paragraph now names the
  musl runtime (`489005ac`, a README-only commit); the rest current.

## 7. For the orchestrator

- **Issues to close at the merge (by hand):** wolf-lang#932, #936, #923,
  #935, #925.
- **Issues to file:**
  - wolf-interp: lupin's spawns and `os_exec` leave `os_error()` 0 (0.1.56;
    `spawn_noexec_lanes.rs` pins it); the mirror of `[os.fs.error]`'s
    program-step paragraph (#936).
  - planning repo (the guide): #932's second ask, "bind the fallback first,
    then slice", beside the other `else` notes.
  - hasu `~/lanes/hasu-env/README.md` and RULES §4's shim note: a POSIX sh
    `taskset` shim must close its own script's descriptor before it execs,
    or a gate that closes 0, 1 or 2 (`stdfd_lanes`) reads it open
    (`container/taskset-shim.sh` here does).
  - wolf-lang (not this pass's): `cargo clippy -p wolf_mem` on macOS
    (1.97.1) fails on two lints at `ubcheck.rs` lines 14209
    (`chunks_exact` → `as_chunks`) and 15600 (`isolate_lowest_one`) that
    linux's clippy does not raise; and the `fetch_update` deprecation
    warnings in `wolf_rt/src/task/{pool,stack}.rs` on macOS.
  - #887 stays: the musl `wolf` itself (this pass publishes the runtime
    only; `dist-rt` already builds the musl `wolf` and the worker, unshipped).
- **r41 inputs** (0.2.36 / lupin 0.1.57): wolf-lang trunk `a5378e3f` (this
  branch's base; nothing landed since); STD-PIN `2386522e` unchanged
  (wolf-std trunk `78b9d3cb`); PAIRING lupin 0.1.56 / `294d626` (wolf-interp
  trunk `a1a394d9`, latest tag v0.1.56); the release now has five archives
  and a `readback` job that `publish` needs, and a dry run to rehearse it
  (`gh workflow run release.yml --ref <branch> -f dry=true`, no tag);
  the downstream list: muzzle lc02 (a released archive carrying s263's
  variadic clause), mk (`MAKE` from `env_arg0`, s261 on trunk), pelt R-F-1
  (friendly mode from `env_arg0`; `os_error()` on spawns lets pelt say
  `Text file busy` and `Exec format error` from the call, #936), yew
  (the start-up fix s265 and the argv byte reader), ligature's and clef's
  CI (the musl runtime archive by digest, #935).
- **Prune list:** branches `s266-plant-else-note`, `s266-plant-enoexec`,
  `s266-plant-musl-digest` (PRs #944, #945, #946, to close) and
  `s266-probe-musl-0235` (no PR); the Mac worktrees `~/lanes/s266/{wolf-lang,
  plant-*,probe-0235}` and `~/lanes/s266/{downloads,scratch,evidence-mac}`;
  on kasumi `~/lanes/s266` (the clone's `target/`, `cargo-home`,
  `downloads`, `scratch`) and the image `localhost/s266-gauntlet` (with its
  untagged parent layers).

## Forbidden (what this lane did not do)

RULES §1-§10 held: no merge, tag, release or public step; no issue filed
or closed (listed in §7); no label on any PR; no `git config` on the Mac;
on nomad only builds and tests behind the 8 GB `vm_stat` gate, `-j4`,
`CARGO_INCREMENTAL=0`, every process ended, nothing outside
`~/lanes/s266/`; on kasumi every build and test in the lane's own rootless
container on CPUs 4-7 under the lane lock, nothing outside `~/lanes/s266`
written (s264's image recipe and shim were read as the reference); no
package installed (the musl target only on GitHub's runners); pax and
pelt read only (pelt's probe through the API at `f3b2160`); no kernel,
libc or shell source read. No attribution trailers.
