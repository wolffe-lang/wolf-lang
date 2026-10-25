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
