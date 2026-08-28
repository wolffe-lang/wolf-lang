# wolf corpus

Programs the compiler must grow into; each file's `//!` header directives
(`check:`, `phase:`, `conforms:`, `warns:`, `forward:`, `member:`) drive
`cargo xtask corpus`. Seeded at sprint s02. Canonical phase list: none,
lex, parse, resolve, typecheck, mem, wir, run.

## Membership (D59, `[conf.directive.standalone]`)

A file with no directives at all is a **member** of its directory's
module — compiled through the directory's entry files, never
conform-run on its own (that is the language rule: directory = module,
and plain files join by default). A file is an *entry* when it carries
both `check:` and `phase:`; `member: true` marks membership explicitly
(now redundant for plain files), and `member: false` marks a
standalone entry. `corpus/resolve/bare_sibling/` is the witness for
the bare form. See `docs/modules.md` for the user-facing rules.

## Rules and forward pins

A `check: fail(CODE)` header is a claim that the compiler **enforces**
that rejection today, and every count taken over the corpus reads it
that way. Some files instead pin behaviour we intend but have not built:
the construct does not compile at all, so the compiler declines the file
rather than rejecting it, and the pinned code may be one nothing emits.
Those files say so with

```text
//! forward: borrow expressions
```

naming the construct that is missing. The marker is checked in both
directions — a `fail(CODE)` file the compiler declines must carry it,
and a file carrying it that the compiler now rejects properly is stale
and must drop it in the same commit. `cargo xtask diag-catalog --check`
is the other half: a pinned code must be one the compiler can emit, or a
declared forward pin, and forward-pinned codes are published under their
own heading in `docs/diagnostics.md`.

Counts always report both quantities — so many rules, so many forward
pins — because one corrected number cannot be told apart from a corpus
that simply grew.
