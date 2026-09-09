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
page. [`docs/platforms.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/platforms.md)
says which compiler tiers run on which host.

```sh
printf 'fn main() {\n  print("hello, wolf")\n}\n' > hello.lu
wolf run hello.lu
```

The interpreter, [lupin](https://github.com/wolffe-lang/wolf-interp),
installs the same two ways (`lupin`, `lupin-bin`).

## What runs today

v0.2.8 is tagged, under the codename wolfgang.

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
tells the history of v0.2.8 by campaign, and
[`docs/release/NOTES-v0.1.0.md`](https://github.com/wolffe-lang/wolf-lang/blob/trunk/docs/release/NOTES-v0.1.0.md)
describes the first release feature by feature.

## The standard library

The standard library is a separate repository,
[wolf-std](https://github.com/wolffe-lang/wolf-std), on its own release
cadence. No package of the compiler includes it. Until one does,
`use std.…` resolves against a small built-in stub, which is why
`use std.list` reports that module `std` has no item named `list`.
Point the compiler at a wolf-std checkout and the real tree answers:

```sh
git clone https://github.com/wolffe-lang/wolf-std
export WOLF_STD="$PWD/wolf-std/std"
wolf run main.lu
```

`--std-root <dir>` does the same for one invocation and overrides
`WOLF_STD`. The directory tree is the namespace (D32): `use std.list`
names `<root>/list/`.

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
choose.
