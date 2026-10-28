# wolf

<img src="https://raw.githubusercontent.com/wolffe-lang/wolf-lang/trunk/assets/wolf-logo.svg" alt="the wolf mark" width="120" align="right"/>

Wolf is a compiled systems language. Memory lives in regions that the
compiler infers, so programs carry no lifetime annotations. Allocation
goes through arenas by default. A region moves between tasks instead of
being shared, which is what makes concurrent access safe, and the
aliasing information that produces is available to the optimizer.
Arithmetic is checked in every profile, including release.

Wolf is pre-alpha. The surface still moves.

## Install

```sh
brew tap wolffe-lang/wolf && brew install wolf     # builds from source
yay -S wolf-lang-bin                              # Arch, prebuilt (wolf-lang builds)
```

Archives for linux x86-64, linux aarch64, macOS aarch64 and windows
x86-64 are on the [releases](https://github.com/wolffe-lang/wolf-lang/releases)
page. The linux archives need **glibc 2.35 or later** (Ubuntu 22.04,
Debian 12, Fedora 36 and later); each is built on Ubuntu 22.04 and run,
one program per tier, on a 22.04 host before the release is published
(#447). [`docs/platforms.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/platforms.md)
says which compiler tiers run on which host. From 0.2.36 the page also carries `wolf-rt-<version>-x86_64-unknown-linux-musl.tar.gz`:
the runtime, `libwolf_rt.a`, built for musl (the recipe PAX's image uses),
for linking wolf objects statically against musl; its digests are in the
release notes (#935). Take the objects and the runtime from the same
release: the compiler's entry hands the program's arguments to the
runtime (#962), and an object an older compiler made links but reads no
arguments under musl.

```sh
printf 'fn main() {\n  print("hello, wolf")\n}\n' > hello.lu
wolf run hello.lu
```

The interpreter, [lupin](https://github.com/wolffe-lang/wolf-interp),
installs the same two ways (`lupin`, `lupin-bin`).

## What runs today

The current release is v0.2.36, under the codename wolfgang.

There are two compilation tiers. `wolf build` and `wolf run` compile
`.lu` source to native machine code through the compiler's own backend,
without LLVM. `wolf build --release` goes through LLVM. The two tiers
agree on every corpus program that runs, on linux x86-64 and macOS
aarch64, and the release tier's M2 gate (a thirteen-kernel suite against
`clang -O3`) holds. On windows x86-64 the native tier runs with the task
layer: `hello.exe` builds and runs, traps report and exit 134, and
`spawn`, scopes, `proc`, channels, `select`, `sync`/`when` and
`os.signal` all work. The release tier and external `reload`/`upgrade`
signal delivery do not run on windows yet. [`docs/platforms.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/platforms.md)
is the per-host ledger.

From 0.2.36 the compiler supports hand-declared C variadic calls and
exported variadic functions on Linux x86-64 and `x86_64-unknown-none`.
`VaList` and `va_start`, `va_arg`, `va_copy`, `va_end` provide list access;
C's `va_list` parameter is spelled `*VaList`. Other targets report E0826,
and the checked machine refuses these calls by name.

The compiler also accepts `--assembler PATH` (or `WOLF_AS`)
for assembly files listed in the root `wolf.pkg`. The flag wins over the
environment, and failures are not retried with another tool. The default
remains the target's C driver. Selecting an assembler does not replace the
linker or the release compiler's LLVM path. See `wolf build --help` and
[the assembly contract](https://github.com/wolffe-lang/wolf-lang/blob/trunk/spec/04-abi.md#6-assembly-abiasm).

On native Linux, `--linker PATH` (or `WOLF_LD`) selects a cc-shaped
linker such as ligature. CLI selection wins over the environment; a
failed tool or invalid ELF output is not retried through cc. Without
selection the platform's existing link path remains unchanged. Ligature
needs a matching musl runtime (`WOLF_RT_LIB`) and sysroot
(`LIGATURE_SYSROOT`); the flag does not replace either. This does not
add hosted Windows, Mach-O or foreign-target linking. See
[linker control](https://github.com/wolffe-lang/wolf-lang/blob/trunk/spec/04-abi.md#7-linker-control-abilink).

On trunk, long compilations show the current unit and phase on a terminal.
Non-interactive builds add no output until 30 seconds without output, then
heartbeat every 60 seconds. `--quiet` hides this reporter, not diagnostics;
`--timings` and `--timings=json:<file>` record where compilation time went.
These apply to build/run/test compilation, not program execution. See the
[CLI timing schema](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/cli.md) for nested and parallel elapsed scopes.

### Building from source

```sh
cargo build --release -p wolf_driver
./target/release/wolf run corpus/hello.lu
```

```console
hello, wolf
```

`wolf --version` describes the build it came from (D57). A build made at
a release tag prints the bare version. Any other build, including the
one above, prints `version+dev.<commit>`, so an off-tag build cannot be
mistaken for the release. `cargo xtask dist` stamps the commit; a plain
`cargo build` cannot verify one and prints `+dev.unknown`.

[`CHANGELOG.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/CHANGELOG.md)
tells the history of v0.2.36 by campaign, and
[`docs/release/NOTES-v0.1.0.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/release/NOTES-v0.1.0.md)
describes the first release feature by feature.

## The standard library

The standard library is its own repository,
[wolf-std](https://github.com/wolffe-lang/wolf-std). The release archive
carries it, pinned to the commit `crates/wolf_driver/STD-PIN` names, as
`std/` beside the `wolf` binary (`std/STD-REV` records the commit), and
that copy is the default: `use std.env` works from an unpacked archive
with nothing set. A `wolf` built from source has no `std/` beside it, so
there `use std.…` resolves against a small built-in stub, which is why
`use std.list` reports that module `std` has no item named `list`.
Point the compiler at a wolf-std checkout and the real tree answers:

```sh
git clone https://github.com/wolffe-lang/wolf-std
export WOLF_STD="$PWD/wolf-std/std"
wolf run main.lu
```

`--std-root <dir>` does the same for one invocation and overrides
`WOLF_STD`; both override a `std` path dependency in `wolf.pkg`, and all
three override the `std/` beside the binary, which a freestanding
build (`--target x86_64-unknown-none`) never reads. The directory tree is the
namespace (D32): `use std.list` names `<root>/list/`.

## Where things are

| path | what |
|---|---|
| [`spec/`](https://github.com/wolffe-lang/wolf-lang/tree/trunk/spec) | the normative language specification: grammar, memory model, concurrency, ABI, conformance |
| [`corpus/`](https://github.com/wolffe-lang/wolf-lang/tree/trunk/corpus) | the conformance corpus. Every program states its expected outcome in a `//!` header, and CI checks it |
| `crates/` | the compiler, the runtime (`wolf_rt`), and the driver |
| [`docs/`](https://github.com/wolffe-lang/wolf-lang/tree/trunk/docs) | the diagnostic catalog, the module rules, the lint triage ledger, the release notes |

The reference interpreter is a separate implementation in
[wolf-interp](https://github.com/wolffe-lang/wolf-interp). The two
share the spec and the corpus and no code, and each is tested against
the other.

[CONTRIBUTING.md](https://github.com/wolffe-lang/wolf-lang/blob/trunk/CONTRIBUTING.md)
has the gates and the commit conventions.

## License

[GPL-3.0-or-later](https://github.com/wolffe-lang/wolf-lang/blob/trunk/LICENSE).
The runtime library (`wolf_rt`) carries the
[wolf Runtime Library Exception](https://github.com/wolffe-lang/wolf-lang/blob/trunk/crates/wolf_rt/LICENSE-EXCEPTION),
so programs you compile with wolf are yours, under any license you
choose. The
[wolf Training Data Permission](https://github.com/wolffe-lang/wolf-lang/blob/trunk/LICENSE-TRAINING-DATA)
lets you train models on this repository's text and ship excerpts of it
in datasets under CC BY 4.0.
