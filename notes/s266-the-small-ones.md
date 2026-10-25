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

## Rulings (recommended, implemented; never decided here)

- **R-s266-1** (#932): (A) a note on the existing refusal. Recommended A.
- **R-s266-2** (#923): (A) trap by index, as `[os.env.argv]` already
  says. Recommended A; trunk does it (s261).
