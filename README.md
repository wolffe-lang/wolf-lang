# wolf

<img src="https://raw.githubusercontent.com/wolffe-lang/wolf/trunk/assets/wolf-logo.svg" alt="the wolf mark" width="120" align="right"/>

Wolf is a new compiled systems language built on one idea carried end to end:
memory lives in inferred **regions** — value semantics on the surface (no
lifetime annotations, ever), arenas by default, data-race freedom by region
transfer, and aliasing guarantees the optimizer can exploit beyond what C or
Rust can express — with Rust-grade safety, a millisecond-rebuild toolchain,
first-class C interop, and deterministic, replayable concurrency testing.

Status: pre-alpha; the compiler is being built. The language specification
lives in [`spec/`](spec/), and the example corpus (programs the compiler must
grow into) in [`corpus/`](corpus/).

## License

Licensed under [MIT](LICENSE).
