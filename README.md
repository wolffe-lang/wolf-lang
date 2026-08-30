# wolf

<img src="https://raw.githubusercontent.com/wolffe-lang/wolf/trunk/assets/wolf-logo.svg" alt="the wolf mark" width="120" align="right"/>

Wolf is a compiled systems language. Memory lives in regions the compiler
infers, so a program carries no lifetime annotations. Allocation goes through
arenas by default. A region moves between tasks instead of being shared, which
is what keeps concurrent access safe, and the aliasing that falls out of that
is information the optimizer gets to use. Arithmetic is checked in every
profile, including release.

Wolf is pre-alpha. The surface still moves.

## What runs today

v0.2.0 is tagged, under the codename wolfgang. The codenames go on like that.

It ships both tiers: `wolf build` and `wolf run` compile `.lu` source to
native machine code through the compiler's own backend, with no LLVM in the
loop, and `wolf build --release` goes through LLVM instead. The tiers agree
on every corpus program that runs, on linux x86-64 and macOS aarch64 alike,
and the release tier's M2 gate — the thirteen-kernel suite against naive
`clang -O3` — is declared held.

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

[`CHANGELOG.md`](CHANGELOG.md) tells v0.2.0 by campaign;
[`docs/release/NOTES-v0.1.0.md`](docs/release/NOTES-v0.1.0.md) says what the
first release was, feature by feature.

## Where things are

| path | what |
|---|---|
| [`spec/`](spec/) | the normative language specification: grammar, memory model, concurrency, ABI, conformance |
| [`corpus/`](corpus/) | the conformance corpus. Every program states its own expected outcome in a `//!` header, and CI checks the claim |
| `crates/` | the compiler, the runtime (`wolf_rt`), and the driver |
| [`docs/`](docs/) | the diagnostic catalog, the module rules, the lint triage ledger, the release notes |

The reference interpreter is a separate implementation in
[wolf-interp](https://github.com/wolffe-lang/wolf-interp). The two share no
code, only the spec and the corpus, and each is tested against the other.

[CONTRIBUTING.md](CONTRIBUTING.md) has the gates and the commit conventions.

## License

Licensed under [GPL-3.0-or-later](LICENSE). The runtime library
(`wolf_rt`) carries the [wolf Runtime Library Exception](crates/wolf_rt/LICENSE-EXCEPTION):
programs you compile with wolf are yours, under any license you choose.
