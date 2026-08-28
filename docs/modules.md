# Modules: how a directory becomes a program

The rules a user actually needs, in one place (D31/D32, ruled for the
single-file path by D59 / `[conf.directive.standalone]`, sprint s124).
Until s124 these lived only in driver source comments; #145 and #149
were both filed by users who could not have known them.

## The one rule

**A directory is a module.** Every plain `.lu` file in a directory
belongs to that directory's module: one shared namespace, no `use`
between sibling files, and a top-level name may be defined only once
across all of them (E0302 if not — file boundaries create no scopes).

This holds for every way a compilation starts:

- `wolf build src/main.lu` inside a package (`wolf.pkg` present): the
  package root is the entry file's directory; `src/` is the root
  module.
- `wolf build file.lu` on a bare file outside any package: exactly the
  same. The file's directory is the package root, and its plain
  siblings are part of the program. There is no separate
  "single-file mode".

## Importing a sibling directory

A sibling directory of the package root is a child module. `use sub`
binds it file-scoped; its `pub` items resolve as `sub.item`. The
directory needs nothing beyond at least one plain `.lu` file — no
marker, no interface file, no manifest:

```
src/main.lu          use sub          fn main() …
src/sub/anything.lu  pub fn seven() -> int { 7 }
```

The file name never matters, only the directory name. Deeper nesting
is `use sub.inner`. Imports must form a DAG (E0303); visibility is
`pub` / `pub(pkg)` / private-by-default (E0304 explains).

## Opting out: standalone entries

A file that is a program of its own — not a part of its directory's
module — says so, and is then excluded from every sibling's build.
Four spellings, all pre-existing conventions that D59 names:

| spelling | who it is for |
|---|---|
| `//! member: false` in the leading `//!` block | the general opt-out: scratch programs, exercises, one-off tools |
| both `//! check:` and `//! phase:` directives | conformance-corpus entry files (`[conf.directive.member]`) |
| a `#!` first line, or `pkg { … }` frontmatter | scripts (s53) — a script is a single-file package |
| a file name ending `_test.lu` | test files (s39) — `wolf test` runs each as its own program |

An explicit `member:` key always wins: `member: true` joins a file to
the module even when its header looks entry-shaped, and is otherwise
unnecessary — membership is the default. (Historically `member: true`
was *required*; existing trees that carry it are simply explicit now.)

Two asymmetries worth knowing, both deliberate:

- **A standalone mark opts the file itself out. It does not shrink
  anyone's module — not even its own build's.** Building a standalone
  entry still takes the directory's plain files as members (they are
  shared helpers for every program in the directory). So a plain file
  with `fn main` beside a standalone program collides (E0302) when the
  standalone one is built: mark *each* program in a shared directory,
  and leave only genuine shared library files plain.
- **The named entry always compiles**, whatever its own markers say.

## Several small programs in one directory

The ordinary want (a scratch directory, the book's exercise
directories) is therefore one line per file:

```
//! member: false

fn main() -> !int {
    …
}
```

Each file builds and runs alone; a plain sibling (no marker, no
`main`) is a helper library shared by all of them.

## What the diagnostics tell you

`E0301` now distinguishes the situations (s124; `wolf --explain E0301`
carries the long form):

- the name exists in a **standalone sibling** → the note names the
  file, the marker it carries, and the fix (remove the marker);
- an imported directory exists but **every file opted out** → the note
  lists the files and says no module forms there;
- the directory exists but holds **no `.lu` files**, or **does not
  exist** → the note teaches what a module is instead of asserting a
  layout rule you may already satisfy.

An unparseable file that belongs to the module **fails the build** —
before D59 a bare-file build silently ignored it and exited 0.

## Interfaces, for completeness

`.wolfi` interface files are compiler-emitted surface descriptions
(D31); they are *outputs* of module resolution, never inputs to it. A
directory does not need one to be importable — that folklore came from
#149's first (corrected) analysis.
