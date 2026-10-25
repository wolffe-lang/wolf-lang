# s267 — the static entry carries argv (wolf-lang#962)

Lane note. **Opus 5.5. Crawl 54.** Branch `s267-the-static-entry` off
wolf-lang trunk `253d80ab`. Contract:
`sprints/compiler/s267-the-static-entry.md` (planning repo). Linux work on
kasumi CPUs 12-15 under `flock ~/lanes/kasumi-cores-12-15.lock` (shared
with ld02a), in the lane's own rootless containers; editing on the Mac.

## Prediction (written before the first change, 2026-10-25 06:56 EDT)

Read before predicting: wolf-lang#962 whole; ld02a's probe on kasumi
(`~/lanes/ld02a/argv-probe/`, read only: `results.json`, `wolf/main.lu`,
`c/main.c`) and its copy on ligature's `ld02a` branch
(`notes/ld02a/argv-issue.json`, `bundle.json`, the two control files);
`crates/wolf_codegen_clif/src/lib.rs` `add_entry_shim` (995-1180),
`crates/wolf_driver/src/main.rs` 1488-1494 (the one call site, ahead of
both backends), `crates/wolf_rt/src/os.rs` 52-160, `native.rs`'s
`__wolf_rt_stack_start`, `translate.rs`'s `RT_SYMBOLS`, the LLVM
emitter's entry parameters (`emit.rs` 2013-2055) and its runtime-call
declarations (3425-3466), `xtask/src/t1.rs`'s `count_ir_instructions`,
`spec/04-abi.md` §5 and `spec/11-os.md`'s `[os.env.argv]`. No libc,
kernel, linker or Rust standard library source.

**The cells.** Four readers (`env_args()`, `env_arg0()`,
`env_args_bytes()` with a non-UTF-8 argument, `env_vars()` under a
controlled environment), each one program, compiled on two tiers (native,
release) to one object, and that object linked three ways: `cc` against
glibc and the host `libwolf_rt.a` (what `wolf build` does), and the musl
bundle's sysroot (`link-bundle-2026-10-25`, fetched by digest) with a musl
`libwolf_rt.a` built from the same checkout, by `ld.lld` 18 and by GNU
`ld`. 2 x 3 x 4 = 24 cells.

- **Wrong today: 12 of 24, every musl cell of the three argv readers on
  both tiers.** Of the six `env_args()` cells, the four musl ones answer
  `argc=0` (two linkers x two tiers; the probe measured the native two),
  and likewise `env_arg0()` answers empty text and `env_args_bytes()` an
  empty list in their four musl cells. The 8 glibc cells are right.
- **`env_vars()` is not dark under musl: the 4 musl `env_vars` cells are
  right today.** The reader does not depend on a capture made by early-init
  code: it reads the live environment, which POSIX has the start-up
  initialize for every program (`environ`), and which `env_set` and
  `env_unset` write. So it stays on the live environment after the fix
  (answering from the entry's `envp` would lose every `env_set`), and the
  table, not this paragraph, is the evidence either way.
- **The release tier shares the defect.** `add_entry_shim` builds one WIR
  shim at `main.rs:1493`, before the driver chooses a backend, and both
  tiers link the same runtime readers; so the release musl cells read as
  the native ones do.
- **The shim's new lines.** Its signature becomes
  `(i32, i64, i64) -> i32` (`int main(int argc, char **argv, char
  **envp)`; the two `make_sig(vec![], vec![types::I32])` sites become one
  helper), its entry block takes the three values as block parameters, and
  its first call, `__wolf_rt_stack_start()`, becomes
  `__wolf_rt_main_start(argc, argv, envp)`, which records the three and
  then does what `stack_start` did. About ten changed lines in
  `add_entry_shim`, **zero instructions added**: one call replaces one
  call and entry parameters are not instructions, so bench-gates'
  ir-volume record moves on no path (s256 moved 36 by adding one call).
  `RT_SYMBOLS` goes 254 -> 255 (`stack_start` stays exported for objects
  an older compiler made).
- **The runtime.** A store in `wolf_rt::os` filled once by
  `__wolf_rt_main_start`; on unix `env_arg0`, `env_args` and
  `env_args_bytes` answer from it, and fall back to the standard library's
  capture only when no wolf shim ran (a wolf object under a foreign `main`,
  the runtime's own Rust tests), which is exactly today's behaviour there.
  Windows keeps the host's WTF-8 path and records nothing. The freestanding
  target has no shim and `libwolf_rt_none.a` defines none of these readers:
  untouched.
- **After: 24 of 24 right; the 8 glibc cells byte-identical** to the
  before table.
- **The plant** (the niladic shim restored on a `s267-plant-*` branch)
  turns exactly the 12 musl argv cells red and leaves the other 12 green.
- **The bundle candidate.** `cargo xtask dist-rt` at the head gives a
  `libwolf_rt.a` that is not `9d859b9e...` (0.2.35's) and about 46.6 MB
  (s266 measured 46,641,340 bytes at its head); built on kasumi in an
  Ubuntu 24.04 image with rust 1.97.1, it should equal a GitHub-runner
  build of the same head byte for byte, as s266's probe equalled the PAX
  image. The other seven bundle members are byte-identical to the
  published bundle's.

## Inputs verified

- wolf-lang trunk `253d80ab` (`git fetch`; the branch's base).
- `link-bundle-2026-10-25` (ligature#13): `ld02-musl-link-bundle.tar.gz`,
  36,594,483 bytes, sha256 `6f99a2794da54b2dd85f06713d736b27862a7cd0da9e28bd940d975a4603a1d1`,
  read twice by `ci/fetch-entry-bundle`, which also holds its member set
  to exactly eight regular files and each member to its size and digest in
  ligature `tests/pax-link.sums` (twice). Its `libwolf_rt.a`
  `9d859b9e6b5319f5a6f609e73ba0d71981d2dfa489882648ac9f454cd9db739c`
  (45,363,160 bytes, 0.2.35's) is verified and never linked by the rows.
- ld02a's probe, read only: kasumi `~/lanes/ld02a/argv-probe/`
  (`results.json`, `wolf/main.lu`, `c/main.c`) and ligature's `ld02a`
  branch (`notes/ld02a/argv-issue.json`, `bundle.json`, the two control
  files). Nothing else under `~/lanes/ld02a/` was opened.
- Containers on kasumi, CPUs 12-15 under `flock
  ~/lanes/kasumi-cores-12-15.lock` (each leg took it for itself; ld02a held
  it between legs), `--pids-limit 16384 --ulimit nproc=32768:32768
  --ulimit core=0 --memory 12g`, no network but for the one `cargo fetch`,
  pid 1 podman's init as the userns root and the command dropped to uid
  1000 by `setpriv` (`NoNewPrivs: 1`), never `--pid=host`:
  - **ctr-u**, the rows and the bundle: `localhost/px57-rust` by id
    `e93022a6c0d0…` (Ubuntu 24.04.5; rust 1.97.1 at `/opt/rust` with the
    `x86_64-unknown-linux-musl` target already in the image, so nothing was
    installed; Ubuntu LLD 18.1.3, GNU ld 2.42, clang 18.1.3, gcc 13.3: the
    linkers of CI's `ubuntu-latest` `lld` and `binutils`).
  - **ctr-g**, lint, the gauntlet and the differ: `localhost/s267-gauntlet`
    (`96098506abd0…`), s266's image by id `5c461b7d4fd8…` with the taskset
    shim moved to CPUs 12-15 (`s267-evidence/image/`); the host's
    toolchain through read-only `/usr/lib`, rust 1.97.1 from the host's
    rustup read only.
- lupin 0.1.56 (`PAIRING`'s) `ad703550ac4787e8e4bb26989d591a731410274d7332f2b4e8b9a64c878822cc`,
  read twice; wolf-std `2386522e` (STD-PIN) as `WOLF_DIST_STD_SRC` and the
  differ's `WOLF_STD`.

## 1. The table: twenty-four cells, before and after

One program per reader (`crates/wolf_driver/tests/static_entry.rs`), built
with `wolf build --emit=obj` on each tier to one object, and that object
linked three ways: `cc -o B OBJ libwolf_rt.a -lpthread -ldl -lm
-Wl,--gc-sections` against glibc and the host runtime (the driver's own
link, less the `-fuse-ld=lld` it adds where lld is found); and `LD -static
--gc-sections -o B crt1.o crti.o --start-group OBJ libwolf_rt.a(musl)
libc.a libunwind.a --end-group crtn.o` with the bundle's sysroot and a musl
runtime built from the same checkout, for `LD` = `ld.lld` (18.1.3) and
GNU `ld.bfd` (2.42) — ligature's own oracle link line. Every binary runs as
argv[0] `s267 name` (not its path), the arguments `--version`, `space
word` and an empty one (and, for `env_args_bytes`, a fourth, the bytes
`ff fe`), under an environment of exactly `S267_A=1`, `S267_B=space word`,
`S267_C=` and the non-UTF-8 `S267_D=\xff`. Right answers:

- `env_args`: `argc=3`, `[--version]`, `[space word]`, `[]`;
- `env_arg0`: `[s267 name]`;
- `env_args_bytes`: `argc=5`, then each argument's bytes in decimal and
  `end` (`s267 name`, `--version`, `space word`, the empty one, `255 254`);
- `env_vars`: `count=3`, `[S267_A=1]`, `[S267_B=space word]`, `[S267_C=]`
  (the non-UTF-8 entry left out, as `[os.env.argv]` says).

**Before** (trunk's code plus the rows, `f347c8fd`/`1ad8fef5`;
`s267-evidence/before/`):

| tier | link | `env_args` | `env_arg0` | `env_args_bytes` | `env_vars` |
|---|---|---|---|---|---|
| native | cc + glibc | right | right | right | right |
| native | ld.lld 18 + musl | **`argc=0`** | **`[]`** | **`argc=0`** | right |
| native | GNU ld + musl | **`argc=0`** | **`[]`** | **`argc=0`** | right |
| release | cc + glibc | right | right | right | right |
| release | ld.lld 18 + musl | **`argc=0`** | **`[]`** | **`argc=0`** | right |
| release | GNU ld + musl | **`argc=0`** | **`[]`** | **`argc=0`** | right |

Every binary exited 0. **After** (`54124626`, `s267-evidence/after/`): all
24 right. The glibc cells are byte-identical: `cc-glibc.json` is
`19869bb4147dd7479573365790b7e6e59b7340482fb781c0949533466c36da72` before
and after; the 12 musl argv cells are the only ones that moved.

## 2. The fix: the shim's new shape (R-s267-1 = A, implemented)

- **The shim** (`add_entry_shim`, `crates/wolf_codegen_clif/src/lib.rs`)
  is now `int main(int argc, char **argv, char **envp)`: its WIR signature
  is `C_MAIN_PARAMS` = `(i32, i64, i64) -> i32` (one helper, `c_main_sig`,
  where two `make_sig(vec![], vec![types::I32])` were), its entry block
  takes the three as block parameters, and its first call is
  `__wolf_rt_main_start(argc, argv, envp)` (`MAIN_START`) in place of
  `__wolf_rt_stack_start()`. Both tiers lower the same WIR shim (the
  driver builds it once, before choosing a backend): the native tier's
  `RT_SYMBOLS` gains the symbol with `argc` typed i32 (254 -> 255), and the
  release tier declares it from the WIR signature, `declare void
  @"__wolf_rt_main_start"(i32, i64, i64)`. One call replaces one and entry
  parameters are not instructions, so the shim keeps its instruction count
  (`entry_shim.rs` pins 4 for a unit and an `int` main).
- **The runtime** (`crates/wolf_rt/src/os.rs`, `native.rs`):
  `__wolf_rt_main_start` records `(argc, argv, envp)` in a store of four
  atomics (`os::entry`) and then does what `__wolf_rt_stack_start` did
  (which stays exported, for objects an older compiler made). On unix
  `env_arg0`, `env_args` and `env_args_bytes` answer from the record: argc
  entries of argv, verbatim bytes, stopping at a null entry, empty for a
  null argv or a negative argc. They fall back to the standard library's
  capture only when no wolf entry ran (a wolf object under a foreign
  `main`; the runtime's own Rust tests), which is today's behaviour there.
  Windows records nothing and keeps the host's WTF-8 arguments. `envp` is
  recorded and read by nothing at this cut: `env_vars` reads the live
  environment, which `env_set` and `env_unset` write, and was right under
  musl before the change.
- **R-s267-1 (where argv lives): A**, the runtime store the shim fills at
  entry, every reader answering from it on every unix C library. B (keep
  the standard library's capture and add an init hook for the libc that
  lacks it) was rejected in advance: it depends on start-up behaviour the
  clean room cannot read. The fix depends on nothing either libc does
  before `main` beyond calling it with its arguments (ISO C 5.1.2.2.1),
  which the C control in ld02a's probe shows musl does.
- **The freestanding target is untouched**: it has no entry shim (the
  driver skips `add_entry_shim` there, `[abi.target.entry]`), and
  `libwolf_rt_none.a` defines none of these readers (`env_args` is refused
  by name on that target).
- **The spec**: `spec/04-abi.md` gains `[abi.target.main]` (the shim's
  signature is ABI, its first call, the order of the start-ups after it,
  older objects, a foreign `main`, Windows), `spec/11-os.md`'s
  `[os.env.argv]` cites it, `spec/anchors.json` gains its key.

## 3. Gates

- **The rows** (`crates/wolf_driver/tests/static_entry.rs`): three tests,
  one per link, each printing its eight cells (`cell <tier> <link>
  <reader>: right|WRONG exit=… stdout="…"`) before it judges; a missing
  input skips its cells loudly by name and counts them, and is a failure
  under `WOLF_STATIC_ENTRY_REQUIRE=1`; `WOLF_STATIC_ENTRY_TABLE` writes the
  JSON tables this note quotes. Inputs: `ci/fetch-entry-bundle` (the
  bundle by digest, same shape as ld02b's `ci/fetch-test-link-bundle` on
  #961, the same cache directory and cache key) and a musl `libwolf_rt.a`
  from the checkout.
- **In CI**: lint-test (the PR tier's gauntlet) fetches the bundle and
  builds a debug musl runtime before `cargo xtask ci`, with
  `WOLF_STATIC_ENTRY_REQUIRE=1`; the new `static-entry` job (full and
  release tiers, and a partial dispatch for plants, as `variadic-abi` is;
  `ci/tiers-test.py` holds both) runs the rows alone against a runtime
  built by `cargo xtask dist-rt`, prints its digest twice and keeps the
  archive as an artifact.
- **The shape** (`crates/wolf_codegen_clif/tests/entry_shim.rs`, every
  host): the shim's parameters are `C_MAIN_PARAMS`, its result i32, its
  first call `__wolf_rt_main_start` with the shim's own three parameters
  as arguments, four instructions for a unit and an `int` main, no
  `__wolf_rt_stack_start`. `exec_smoke.rs`'s C stub now checks the
  arguments arrive (`argc >= 1`, `argv[argc] == NULL`, `envp` non-null)
  and keeps a `__wolf_rt_stack_start` for an older shim.
- **Seen red before green**: the rows at trunk's code (`1ad8fef5`) red on
  exactly the 12 predicted cells (`s267-evidence/logs/before-rows.log`);
  **the plant** (`s267-plant-niladic-entry`, draft #964, `b0f19c52`: the
  niladic shim restored in `lib.rs` alone, `__wolf_rt_stack_start()`
  first, no parameters) red by name in the partial dispatch run
  **40188739927** (`-f jobs=static-entry`), job `static-entry`
  121155442191: `musl_cells_linked_by_lld_answer_the_invocation` and
  `musl_cells_linked_by_gnu_ld_answer_the_invocation` FAILED on the same
  12 cells (`argc=0`, `[]`, `argc=0`), `glibc_cells_answer_the_invocation`
  ok (`s267-evidence/ci/plant-static-entry-40188739927.log`). The plant
  PR's own run (40188731085) went red earlier, in lint-test's clef step
  (below), before any row ran.
- **Green at the head**: the partial dispatch 40188741575 at `54124626`
  (`static-entry`: 24 of 24 on GitHub's Ubuntu 24.04 runner with LLD
  18.1.3 and GNU ld 2.42; `s267-evidence/ci/head-static-entry-40188741575.log`).
- **A trunk CI defect, fixed here because it blocked the PR tier**
  (`c097970a`): since trunk's run of 2026-10-25 10:02Z saved the
  `lint-test` cache `v0-rust-lint-test-Linux-x64-986f06eb-15beb2f5`, every
  run restoring it went red in `ci/build-test-clef` with no output (this
  PR's 40188669958 and ld02b's 40183568329 alike; the next trunk push
  would have too). The restored `target/test-clef/.git` is a directory
  that no longer holds a repository, so `git -C target/test-clef rev-parse
  HEAD` silently answers the enclosing wolf-lang checkout's HEAD and the
  pin test fails (reproduced with git alone: a nested empty `.git`
  directory answers the outer HEAD). The builder now requires the tree to
  be its own checkout (`rev-parse --show-toplevel` equal to the tree),
  clones the default tree again when it is not, refuses a caller-named
  tree by name, and names the pin mismatch.

## 4. The bundle candidate (staged, never published)

Built exactly as `link-bundle-2026-10-25` was (`s267-evidence/scripts/bundle.sh`,
ctr-u, at `54124626`, a clean tree): `libwolf_rt.a` by `cargo xtask dist-rt
--target x86_64-unknown-linux-musl` (the recipe s266's probe reproduced the
published runtime with, byte for byte), and the archive by ld02a's recipe
(ligature `notes/ld02a/bundle.json`: GNU tar `--sort=name --mtime=@0
--owner=0 --group=0 --numeric-owner`, the explicit ordered list of eight
files, `gzip -n`; tar 1.35, gzip 1.12). **The recipe was checked first**:
the published bundle's own eight members, packed by it, give
`6f99a2794da54b2dd85f06713d736b27862a7cd0da9e28bd940d975a4603a1d1`, the
published digest, byte for byte. The candidate is those seven members
unchanged and the head's runtime:

- **Staged path:** kasumi `~/lanes/s267/bundle/ld02-musl-link-bundle.tar.gz`
  (the published asset's name), with `~/lanes/s267/bundle/link.sums` (the
  shape of ligature's `tests/pax-link.sums`; a second read
  `link.sums.read2`, equal) and the staged tree `~/lanes/s267/bundle/stage/`.
- **Archive:** sha256 `dd09895a13d3ea4056db73d3e974f025a732391c38044094ea6fc8ed69fb54cc`,
  36,813,201 bytes (read twice).
- **`libwolf_rt.a`:** sha256 `3d6f821ae70ebb40d106f31c6a95ee46c0c3626d9bf230f43c1f0b8c7c26eb24`,
  46,699,408 bytes (read twice on kasumi), 2,282 undefined symbols, none of
  glibc's large-file names. **GitHub's runner built the same bytes** from
  the same head: `3d6f821a…53d5`, 46,699,408 bytes, in run 40188741575 (and
  again in the plant's 40188739927, whose change is codegen only).
- **The other seven members**, unchanged (`link.sums`): `cc` `3161aea2…5b11`
  499,760; `ld.lld` `1217ca75…757b` 66,247,008; `sysroot/crt1.o`
  `51dba242…90f6` 5,560; `sysroot/crti.o` `5cbc84c7…7b49` 2,816;
  `sysroot/crtn.o` `d884ee5f…a1e7` 2,760; `sysroot/libc.a` `8e69a73c…ddd4`
  10,961,970; `sysroot/libunwind.a` `264296d6…94bc` 201,794.
- **The rows against it:** 24 of 24 right with the candidate's
  `libwolf_rt.a` and sysroot (`s267-evidence/candidate/`).
- **What it fixes, and what it does not:** a program compiled by a wolf
  with this entry and linked against this runtime reads its arguments
  under musl. An object compiled by **0.2.35** (whose shim calls
  `__wolf_rt_stack_start()` and hands nothing) links against it and still
  reads `argc=0` under musl — exactly the plant's red cells. So ligature's
  self-link row (ld02a, which builds ligature with the pinned wolf 0.2.35)
  flips only when ligature pins a wolf release carrying this entry
  **and** links the matching runtime; publishing this candidate alone does
  not flip it.
- **R-s267-2, the bundle's name: recommend B**, a version tied to the wolf
  release whose entry it matches (`link-bundle-wolf-0.2.36`, say). The
  runtime and the compiler's entry now have to come from one wolf (a new
  object against the old runtime fails to link on `__wolf_rt_main_start`;
  an old object against the new runtime links and reads nothing under
  musl), and from 0.2.36 the release itself publishes
  `wolf-rt-<version>-x86_64-unknown-linux-musl.tar.gz` by the same
  `dist-rt` recipe (#935), so a bundle named for the release can hold its
  `libwolf_rt.a` to the digest in that release's notes. The candidate
  staged here is the head's; it equals 0.2.36's runtime only if nothing
  else lands in `wolf_rt` before the tag, so under B it is the rehearsal
  (the 24 rows and the recipe proven), and the published bundle is rebuilt
  from the tag. Under A (`link-bundle-<date>`) this candidate could be
  published as it is, for links made with a toolchain built from this
  branch. The maintainer decides.

## 5. Rulings (recommended; never decided here)

- **R-s267-1, where argv lives: A, implemented** (§2): the runtime store
  the shim fills at entry; every reader answers from it on every unix C
  library. B was rejected in advance by the contract and is not built.
- **R-s267-2, the bundle's name: B recommended** (§4), a version tied to
  the wolf release whose entry it matches; A would publish the staged
  candidate as `link-bundle-<date>`. For the maintainer, through the
  orchestrator.

## Hits and misses

- **12 of 24 wrong today, the musl cells of the three argv readers: hit**,
  cell for cell, on both tiers and both linkers (`argc=0`, `[]`,
  `argc=0`); every binary exited 0, as the probe's did.
- **`env_vars()` not dark under musl: hit.** The four musl `env_vars`
  cells were right before the change and are byte for byte the same after.
- **The release tier shares the defect: hit** (its four musl argv cells
  were the native tier's).
- **The shim's lines: hit.** About ten changed lines in `add_entry_shim`
  (the helper, the three entry parameters, the one call), zero
  instructions added (`entry_shim.rs` counts four, as before).
  `RT_SYMBOLS` 254 -> 255: hit.
- **The glibc cells byte-identical: hit** (`cc-glibc.json` the same 1,761
  bytes, `19869bb4…`, before and after).
- **The plant reds exactly the 12 musl argv cells: hit** (40188739927).
- **The bundle candidate's runtime about 46.6 MB and a GitHub-runner build
  equal to kasumi's: hit** (46,699,408 bytes; `3d6f821a…` on both).
- **Not predicted:** that the published bundle's runtime pairs only with
  objects from a wolf with the new entry (an object 0.2.35 compiled stays
  dark against it), so ligature's blocked row needs a wolf release as well
  as a bundle (§4); and the trunk CI defect in the clef builder (§3).

## 6. The gauntlet and the differ

- **The gauntlet** (`cargo xtask ci`, ctr-g on kasumi CPUs 12-15 under the
  lock: the 12-15 shim, `--pids-limit 16384 --ulimit nproc=32768:32768`,
  pid 1 foreign (uid 0 in the namespace against the command's 1000),
  `SigBlk`/`SigIgn` 0, never `--pid=host`; the verdict is the command's own
  exit status in `gauntlet.exit`):
  - Take 1 at `54124626`: exit 1 in `fmt-fixpoint`, the lane's set-up:
    `CARGO_TARGET_DIR` pointed outside the checkout and xtask's steps run
    `target/debug/wolf` (`logs/gauntlet1-target-dir-fault.log`).
  - Take 2 at `54124626` (11:45-12:01Z, the checkout's own `target/`):
    `fmt`, `fmt-fixpoint`, `clippy`, `rt-none` and **`test` green**, the
    24 static-entry cells among them (`static_entry` 8/8 on each link,
    with the host's lld and GNU ld this time); 61 SKIP lines, every one a
    rootless container's (no nested engine: the ids, group, job-wait, fork,
    core and stack-overflow witnesses, which CI's lint-test runs with
    docker and REQUIRE), lldb and the clang-drift witness (macOS only), the
    real-clef rows (no `WOLF_TEST_CLEF` here; CI builds clef) and one
    release-profile row; 180 taskset requests, none outside the block. Then
    exit 1 in `fuzz-check`, the lane's set-up again: the offline cargo home
    held the workspace's crates and not the fuzz workspace's
    (`libfuzzer-sys`), as s265's take 2 found
    (`logs/gauntlet2-steps-skips.log`, `logs/gauntlet2-chain.log`).
  - Take 3, the whole `cargo xtask ci` at this note's head with the fuzz
    workspace's crates fetched: its exit and log are in the PR body and the
    hand-back (a note commit after it would restart the PR tier).
  - The PR tier's own gauntlet (lint-test, GitHub's ubuntu-latest, docker
    and REQUIRE for every container witness, clef built) is green at
    `c097970a`: run **40189764489**.
- **The differ against trunk** (`xtask differ <head's wolf> <trunk's
  wolf>`, three tiers, `WOLF_STD` the std pin; trunk's `wolf` is the
  binary built at `1ad8fef5`, trunk's code and the rows only,
  `fda2e1b9…51b8`; the head's `75475c56…f27b`; `logs/differ-*.log`):
  checked **0** divergences of 1,228 files; native **3**, release **1**,
  every one a use-after-free witness whose exit code is a garbage read and
  differs from run to run (`corpus/conc/atomic_ub_uaf.lu`,
  `corpus/memory/unsafe_ub_uaf.lu`, `corpus/memory/volatile_ub_uaf.lu`
  natively, the last alone on the release tier; s257 and s265 saw the same
  three). **No row moved because of this change**; the corpus runs with
  no arguments and a conform-run name, which both entries hand over alike.
  A and B executed 752 (checked) and 757 (native, release) of 1,166 run
  entries each.

## 7. For the orchestrator

- **Issues to close at the merge (by hand):** wolf-lang#962.
- **Issues to touch:** wolf-lang#887 (the musl `wolf` archive) gains a
  working target: a static musl wolf binary now reads its arguments
  (`dist-rt` already builds the musl `wolf`; the 24 rows are the witness
  for any program it builds). ligature#13 (the bundle): the candidate in
  §4 is staged for the maintainer.
- **Issues to file:**
  - wolf-lang (or ld02b on #961): `ci/build-test-ligature` has the shape
    the clef builder had (`[ ! -e "$tree" ]`, then a pin test), so a
    rust-cache restore of `target/test-ligature` can fail it the same
    silent way; the guard in `c097970a` is the fix's shape.
  - wolf-lang: `cargo xtask dist-rt`'s `.tar.gz` is not reproducible across
    hosts (kasumi `a6d0bfb0…53d5`, the runner `139daa5b…3f39`, from the
    same head, around one byte-identical `libwolf_rt.a`): its `tar` takes the
    files' mtimes and owners and `gzip` its name and time. The release's
    `readback` holds the digests of one build, so no release is wrong;
    a rebuild cannot reproduce the asset, only its runtime.
- **Follow-ons:**
  - Whichever of #961 and this PR lands second folds the two fetchers of
    the same bundle (`ci/fetch-test-link-bundle`, `ci/fetch-entry-bundle`)
    and lint-test's two cache steps with one key into one (they share the
    cache directory and key already, so neither breaks the other).
  - ligature's self-link row (ld02a) flips when ligature pins a wolf
    release with this entry and links the matching musl runtime (§4), not
    when a bundle is published alone.
  - `envp` is recorded and unused: a reader of the initial environment
    (or PAX's own start-up) can take it without another ABI change.
- **Corrections for the planning repo and memory (reported, not
  written):**
  - s267's contract and ld02a's: "ld02a's blocked row cites #962 and flips
    when the new bundle is published" — it also needs a wolf toolchain
    whose entry hands argv (0.2.36 at the earliest); an object 0.2.35
    compiles stays `argc=0` under musl against the new runtime.
  - The container recipe (RULES §4, hasu's `~/lanes/hasu-env/README.md`):
    `cargo xtask ci` hardcodes `target/debug/wolf` (fmt-fixpoint, the
    corpus steps), so a lane container must not point `CARGO_TARGET_DIR`
    outside the checkout; this lane's first gauntlet take failed at
    `fmt-fixpoint` for exactly that (`s267-evidence/logs/`).
  - kasumi's `localhost/px57-rust` (Ubuntu 24.04, rust 1.97.1 with the
    `x86_64-unknown-linux-musl` target, LLD 18.1.3, GNU ld 2.42) is a
    ready image for any lane that needs the musl rust target or CI's
    ubuntu linkers: nothing to install.
- **Prune list:** branch `s267-plant-niladic-entry` (draft #964, to close);
  the Mac worktrees `~/lanes/s267/{wolf-lang,plant}`; on kasumi
  `~/lanes/s267` (`wolf-lang/target`, `target-u`, `cargo-home`,
  `trunk-wolf`, `toolchain`, `wolf-std`, `scratch`) once the bundle is
  published or declined (keep `~/lanes/s267/bundle/` until then), and the
  image `localhost/s267-gauntlet`.

## Evidence index (`notes/s267-evidence/`)

- `before/`, `after/`, `candidate/`: each link's eight cells as JSON
  (`WOLF_STATIC_ENTRY_TABLE`), at `1ad8fef5` (trunk's code), `54124626`,
  and `54124626` against the candidate's runtime.
- `logs/before-rows.log` (the red rows), `logs/chain1-lint-rows-bundle-candidate.log`
  (fmt, clippy, the shim tests, the rows, the bundle build with the recipe
  check, the candidate's rows), `logs/clippy.log`, and the gauntlet and
  differ logs (§6).
- `ci/head-static-entry-40188741575.log`, `ci/plant-static-entry-40188739927.log`:
  the lines of the two partial runs that carry their verdicts.
- `bundle/link.sums` (the candidate's members), `bundle/HEAD`,
  `bundle/dist-rt.notes.md`.
- `image/` (the gauntlet image's `Containerfile` and the 12-15 taskset
  shim) and `scripts/` (the two launchers, the job wrapper, every leg).

## Forbidden (what this lane did not do)

RULES §1-§10 held: no libc, kernel, linker, shell or Rust standard
library source read (the probe table and black-box runs are the
evidence); no ligature or pax change; no merge, tag, release, label or
public step; the bundle candidate staged and named, not published; no
issue filed, closed or commented on (listed in §7); no `trap`, `kill` or
signal script, nothing on a host's process table (the process list was
read once to see who held the CPU lock), no kill by process group; no
build on nomad (rustfmt run on changed files only); no `git config` on the
Mac (`commit.gpgsign false` in the kasumi clone only); nothing under
`~/lanes/ld02a/` but the probe, read; nothing installed (the musl target
came with an existing image); every kasumi leg in the lane's rootless
container on CPUs 12-15 under the shared lock, taken per leg. No
attribution trailers.
