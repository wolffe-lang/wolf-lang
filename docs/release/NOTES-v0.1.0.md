# wolf 0.1.0 (wolfgang) — release notes

An initial release is an identity claim, not a completeness claim.
This is wolf's: a compiled systems language with tiered region memory,
structured concurrency, checked arithmetic in every profile, and two
independent implementations that test each other. Everything below is
written in the present tense because it exists; what is not built is
named as not built.

## The tier this release ships

**The debug tier is real.** `wolf build` and `wolf run` compile `.lu`
programs through the owned debug backend to native machine code — no
LLVM in the loop — and the same driver serves a checked execution lane
(`wolf run` on the WIR-checked machine, `conform-run` for the
differential protocol). The release tier — the LLVM backend, the
optimizing half of the two-tier design — is campaign c09's work and is
not part of v0.1.0.

What a wolf program does today, evidenced by the 254-file conformance
corpus (89 files execute natively; the corpus ledger is enforced in CI
by `cargo xtask corpus` — a directive that lies fails the gate):

- **Native execution**: `str` (all 21 methods, f-string interpolation
  with format specs in every literal), `List` (including `for` over
  lists and checked element arithmetic), the 9 `fs` builtins, `os`
  (argv, env, cwd, spawn, exit codes), monotonic `time`, `print`/
  `eprint` — compiled, linked, and run as ordinary binaries.
- **Checked lane**: stdin (`read_line`), sockets (the net builtins
  over the io reactor), and everything the native lane runs.
- **Memory**: tiered regions with the region checker enforcing the
  memory model statically (the E10xx family) — no lifetime
  annotations anywhere in the language.
- **Arithmetic**: checked in all profiles (X3). Overflow, div-zero,
  and bounds trap deterministically with a named trap kind; the trap
  vocabulary is a closed spec set (`[conf.trap.set]`).
- **Concurrency**: structured tasks, channels, supervised procs, and
  the deterministic scheduler live in `wolf_rt`; the compiler enforces
  the concurrency rules statically (the E11xx family) and the
  reference interpreter executes the concurrency litmuses. Compiled
  concurrent execution is later campaigns' work.
- **Diagnostics**: 136 error codes and 30 warnings, every one with a
  reviewed snapshot and a `wolf --explain` entry; `wolf fix` applies
  machine-applicable suggestions.
- **Tooling in the one binary** (D34): `build`, `run`, `test`, `fmt`,
  `fix`, `lsp`, `interface`, `audit-surface`, `conform-run`,
  `--explain`.

## Two implementations, one spec

wolfgang (this repo) and **lupin** (wolf-interp) are independent
implementations — no shared code — differentially tested through the
spec/06 observation protocol over the shared corpus. Divergences are
first-class artifacts: each one is filed, triaged with the spec as the
first defendant, and closed by a spec clause plus a regression file.
This release pairs wolf 0.1.0 with lupin 0.1.7 as the reference
interpreter at a pinned commit; `wolf --version` and `lupin --version`
each name the pairing.

## The grammar contract

The surface grammar is **grammar/1** (spec/01 §10, `[gram.version]`):
additive-only until v0.2. The `[gram.*]` anchors are the contract —
stable once published — and `cargo xtask spec-extract` keeps
`spec/grammar.ebnf` and `spec/anchors.json` in CI-enforced sync with
the spec text, so a grammar change is always a reviewable diff.

## Platforms

CI builds and tests linux x86-64/aarch64, macOS aarch64, and windows
x86-64 (tier 1). No build scripts anywhere in the wolf ecosystem
(D33); `wolf_rt` stays dependency-thin (D15).

## What v0.1.0 is not

There are no macros (metaprogramming is the CTFE + reflection tier),
no LLVM release tier (c09), no package registry, and the open issue
tracker is public and honest — the known divergence families and
implementation gaps are filed, not hidden. The corpus ledger says
exactly which phase every corpus program reaches today; that ledger,
not this document, is the authority on completeness.
