# wolf

<img src="https://raw.githubusercontent.com/wolffe-lang/wolf/trunk/assets/wolf-logo.svg" alt="the wolf mark" width="120" align="right"/>

Wolf is a compiled systems language. Memory lives in regions the compiler
infers, so a program carries no lifetime annotations. Allocation goes through
arenas by default. A region moves between tasks instead of being shared, which
is what keeps concurrent access safe, and the aliasing that falls out of that
is information the optimizer gets to use. Arithmetic is checked in every
profile, including release.

Wolf is pre-alpha. The surface still moves.

## Install

```sh
brew tap wolffe-lang/wolf && brew install wolf     # builds from source
yay -S wolf-lang-bin                              # Arch, prebuilt (wolf-lang builds)
```

Or take an archive from [releases](https://github.com/wolffe-lang/wolf-lang/releases)
— linux x86-64, linux aarch64, macOS aarch64, windows x86-64. Which hosts run
which tier is in [`docs/platforms.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/platforms.md).

```sh
printf 'fn main() {\n  print("hello, wolf")\n}\n' > hello.lu
wolf run hello.lu
```

The interpreter, [lupin](https://github.com/wolffe-lang/wolf-interp), installs
the same two ways (`lupin`, `lupin-bin`).

## What runs today

v0.2.6 is tagged, under the codename wolfgang. The codenames go on like that.

It ships both tiers: `wolf build` and `wolf run` compile `.lu` source to
native machine code through the compiler's own backend, with no LLVM in the
loop, and `wolf build --release` goes through LLVM instead. The tiers agree
on every corpus program that runs, on linux x86-64 and macOS aarch64 alike,
and the release tier's M2 gate — the thirteen-kernel suite against naive
`clang -O3` — is declared held. Windows x86-64 runs the native tier
with the task layer on it: `hello.exe` builds and runs,
traps report and exit 134, `spawn` and scopes, `proc`, channels and
`select`, `sync`/`when` and `os.signal` all serve, and what refuses by
name there is the release tier and EXTERNAL `reload`/`upgrade` signal
delivery — [`docs/platforms.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/platforms.md) is the per-host
ledger and the road.

### Building from source

```sh
cargo build --release -p wolf_driver
./target/release/wolf run corpus/hello.lu
```

```console
hello, wolf
```

`wolf --version` tells the truth about the build it names (D57): a build made
exactly at its release tag prints the bare version, and every other build —
the one above included — answers `version+dev.<commit>`, so an off-tag build
never claims to be the release. `cargo xtask dist` stamps the commit; a plain
`cargo build` cannot verify one and says `+dev.unknown`.

[`CHANGELOG.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/CHANGELOG.md) tells v0.2.6 by campaign;
[`docs/release/NOTES-v0.1.0.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/release/NOTES-v0.1.0.md) says what the
first release was, feature by feature.

## The standard library

The standard library is a separate repository,
[wolf-std](https://github.com/wolffe-lang/wolf-std), on its own release
cadence: no package of the compiler ships it, and until one does `use std.…`
answers from a small built-in stub — which is why `use std.list` reports that
module `std` has no item named `list`. Point the compiler at a wolf-std
checkout and the real tree answers instead:

```sh
git clone https://github.com/wolffe-lang/wolf-std
export WOLF_STD="$PWD/wolf-std/std"
wolf run main.lu
```

`--std-root <dir>` does the same for one invocation and beats `WOLF_STD`.
The tree is the namespace (D32): `use std.list` names `<root>/list/`.

## Where things are

| path | what |
|---|---|
| [`spec/`](https://github.com/wolffe-lang/wolf-lang/tree/trunk/spec) | the normative language specification: grammar, memory model, concurrency, ABI, conformance |
| [`corpus/`](https://github.com/wolffe-lang/wolf-lang/tree/trunk/corpus) | the conformance corpus. Every program states its own expected outcome in a `//!` header, and CI checks the claim |
| `crates/` | the compiler, the runtime (`wolf_rt`), and the driver |
| [`docs/`](https://github.com/wolffe-lang/wolf-lang/tree/trunk/docs) | the diagnostic catalog, the module rules, the lint triage ledger, the release notes |

The reference interpreter is a separate implementation in
[wolf-interp](https://github.com/wolffe-lang/wolf-interp). The two share no
code, only the spec and the corpus, and each is tested against the other.

[CONTRIBUTING.md](https://github.com/wolffe-lang/wolf-lang/blob/trunk/CONTRIBUTING.md) has the gates and the commit conventions.

## License

Licensed under [GPL-3.0-or-later](https://github.com/wolffe-lang/wolf-lang/blob/trunk/LICENSE). The runtime library
(`wolf_rt`) carries the [wolf Runtime Library Exception](https://github.com/wolffe-lang/wolf-lang/blob/trunk/crates/wolf_rt/LICENSE-EXCEPTION):
programs you compile with wolf are yours, under any license you choose.
