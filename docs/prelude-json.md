# `wolf prelude --json`: the wolf-prelude/0 list

Every name a wolf program can use without an import, as data (s212;
ruling #39 = B, wolf-lang#586, wolf-lsp#32). The rows are the checker's
own tables (`wolf_sema::prelude::PRELUDE` and `BUILTIN_TYPES`, which
resolution and the W0304 lint read), so the list cannot name something
the compiler does not resolve or miss something it does.

Three copies of the same bytes:

- `wolf prelude --json` prints it from the binary you have.
- `spec/prelude.json` is that output, committed beside `anchors.json`, so a
  consumer that vendors `spec/` at a pin (wolf-lsp) has the list for that
  pin. `crates/wolf_driver/tests/prelude_list.rs` reds when the two part.
- `cargo xtask prelude-diff <tagA> <tagB> [--markdown]` reads it at two
  tags and lists what was added and removed (for a tag older than this
  file, it reads the tables from that tag's `prelude.rs`).

`wolf prelude` without `--json` prints the same rows as a table.

## Versioning

`wolf-prelude/0`. A removed field, or a field whose meaning changes,
bumps the number. A new field does not: consumers ignore keys they do not
know. Entries are one per line, in table order, so a diff of two pins is
a diff of lines.

## Shape

```json
{
  "schema": "wolf-prelude/0",
  "names": [
    {"name": "print", "kind": "function", "anchor": null, "w0304": true},
    {"name": "Scope", "kind": "type", "anchor": "conc.proc.handle", "w0304": true},
    {"name": "range", "kind": "type", "anchor": "type.range.name", "w0304": false},
    {"name": "offset_of", "kind": "intrinsic", "anchor": "abi.layout.query", "w0304": true}
  ],
  "marks": [
    {"name": "cross_device", "anchor": "os.host.sigs", "by": ["fs_rename"]}
  ]
}
```

### `names`: one entry per ambient name

- `name` (string): the spelling.
- `kind` (string), one of:
  - `builtin_type`: a built-in scalar (`int`, `str`, `u8`, `wrapping`, …).
  - `type`: a prelude type name written in type position (`List`, `Map`,
    `Pool`, `Mutex`, `channel`, `Scope`, `Proc`, `range`).
  - `function`: a callable. The host builtins, whose signature and row are
    `[os.host.sigs]`'s table, and the output helpers.
  - `intrinsic`: a comptime intrinsic or primitive (`assert` and the D33
    allowlist: `reflect`, `typeinfo`, `typebuild`, `implements`, `size_of`,
    `align_of`, `offset_of`).
  - `provisional`: an s02 corpus stand-in, not language. It retires with
    the real std surface.
- `anchor` (string or null): the spec clause that defines the name, as it
  appears in `spec/anchors.json` (no brackets). `null` means no clause
  defines it yet. That is a spec debt, stated rather than filled with a
  near miss.
- `w0304` (bool): a module that declares this name draws W0304 (shadowing
  an ambient name), which `--deny-warnings` makes fatal. `false` for the
  provisional stand-ins and for `range` (type position only).

### `marks`: one entry per builtin mark, sorted by name

Wolf has no builtin enum. The builtin marks are the payload-free row tags
the host builtins declare. They are written bare in a handler or a
`match` over a builtin's caught value.

- `name` (string): the tag.
- `anchor` (string): `os.host.sigs`, the table that declares every builtin
  row.
- `by` (array of strings): the builtins whose rows carry it, in table
  order.

## Consumers

- **The release CHANGELOG.** "Read this before you bump the pin" lists
  the names a release adds with `w0304: true`, because a downstream
  function of the same name now draws W0304
  (`cargo xtask prelude-diff <previous tag> <this tag> --markdown`).
- **The editor (wolf-lsp's type-name gate).** The candidate set for type
  position is the entries whose `kind` is `builtin_type` or `type`. This
  replaces the second segment of the spec's `type.*` anchors, which missed
  `Scope` and `Proc` at 0.2.16 (wolf-lsp#32).
