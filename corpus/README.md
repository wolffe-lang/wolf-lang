# wolf corpus

Programs the compiler must grow into; each file's `//!` header directives
(`check:`, `phase:`, `conforms:`) drive `cargo xtask corpus`. Seeded at
sprint s02. Canonical phase list: none, lex, parse, resolve, typecheck,
mem, wir, run.
