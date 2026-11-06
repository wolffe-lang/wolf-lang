# s282 — variant names in binder positions warn (W0801); a select arm's is refused (E0806)

`let (Drink, n) = pair` binds `Drink` to whatever the first element holds, on every machine,
with `Kind.Drink` in scope and nothing said. It still binds: a binder pattern binds every
value it meets. It now warns W0801 at the name, naming the variant and its declaration, with
two fixes: a lowercase name with every use renamed (the one `wolf fix` applies), and the
qualified `Kind.Drink`. The same holds in a `var` binder, a comma group, a `for` header and an
`else |…|` handler, at every depth a binder admits. A `select` arm's `Drink from ch` bound
every received value on the compiled tiers while lupin read a test; a receive cannot test, so
it is now refused by name (E0806). The rule lives in the resolver (`crates/wolf_sema/src/binder.rs`,
called from `resolve.rs`), so the checked machine, native and release report it alike; lupin's
mirror is the wolf-interp PR of the same name. The spec gains `[gram.pat.binder]`.

## Forbidden

Footprint: cross-cutting

The diff touches `spec/` (`01-grammar.md` and `anchors.json`), which the lane-audit map names
cross-cutting; the rest is the resolver, its tests, the registry's prose, corpus rows and their
snapshots, the generated catalogs, the changelog and notes. No checker, lowering, codegen,
runtime, CI or pin change. Clean room: the spec, the compiler's own source, and black-box runs
of the published lupin 0.1.59 and of lupin built from this lane's wolf-interp branch; no other
compiler's source. No label, merge, tag, release, issue filed or closed, and no attribution
trailer. The plant is pushed without a PR. Nothing ran under `/tmp` or `/var/folders`: TMPDIR
is the lane's own. No commit was made on the Linux host.

No change to what an existing program means, beyond the one refusal decided for this lane: a
`select` arm whose receive spells an in-scope variant. No program in the corpus, in wolf-std or
in the org's other wolf repositories has one (below).

## Inputs, verified

- Base: wolf-lang trunk `ed19904c`. wolf built there on the Mac (arm64 macOS,
  `cargo build --locked -p wolf_driver -p wolf_rt`, Rust 1.97.1).
- lupin 0.1.59, the release archives: `aarch64-apple-darwin` (sha256 `9f091af8…f5f5`) and
  `x86_64-unknown-linux-gnu` (`12121909…3a18`), each read twice and equal to the release page;
  `lupin --version` names pin `b16353e`, the `PAIRING` stamp.
- wolf-std at the `STD-PIN`, `5b3a5994`, and at its trunk, `d4a4d4ca`.
- The previous lane's note (`notes/s281-the-nested-variant.md`) and wolf-lang#1043: a bare variant
  nested in a match payload tests it on every machine since then; binders and `select` arms
  were left binding.
- The spec: `[gram.pat]`, `[gram.pat.nullary]`, `[gram.pat.struct]`, `[gram.expr.variant]`,
  `[gram.item.let]`, `[gram.expr.conc]`, `[conc.select]`, §9 (`[diag.sev]`, `[diag.family]`,
  `[diag.level]`, `wolf fix`).
- The compiler: the resolver's binding table (`Resolution::refs`) records every binder and
  every use as a `Local` ref, and the LSP's rename reads it; the checker's `variant_homes` is the
  set `[gram.expr.variant]`'s E0301 consults; W0801 lived only in the checker's `match_pattern`
  (a capitalized arm name over a scrutinee with no cases); `wolf fix` applies the first
  machine-applicable suggestion of each diagnostic and lists the others as alternatives; the
  LSP offers every machine-applicable one as a quick fix.
- README read, current: it states nothing about warnings, the catalog or these positions.

## Prediction

Committed before the first change (`dd529314`, `notes/s282-prediction.md`). Scored:

- **The base: hit.** Every binder position bound a variant-spelled name silently on all four
  machines; the handler and the `select` arm refused at run on lupin, which read them as tests
  (`notes/s282-evidence/base-ed19904c.log`).
- **Positions, names, scope: hit, as decided.** A statement `let`/`var` binder (every binder of
  a comma group), a `for` header and the `else |…|` handler's binding form, through tuples,
  struct fields (explicit and shorthand) and both sides of `@`. A name that begins with an
  uppercase letter and spells a variant of an enum the module declares or the file imports by
  item; the prelude has no enum. Scope decides, not type. Parameters, closure parameters,
  item-level bindings, lowercase names, a module-path enum (`kinds.Kind`) and row tags stay
  silent. The decisions, each with the alternative not taken:
  - parameters and closure parameters are names, never patterns, so no test can be meant
    there, and the qualified fix would not parse (not taken: warn with the rename alone);
  - item-level bindings declare module items; module state cannot hold an enum value at this
    cut (`[mem.static.3]`), and an item-level destructuring gives each binder the whole
    annotation's type today (probe `b29`, E0401), so the shape cannot be written there (not
    taken: warn there too, with a module-wide rename);
  - only capitalized names warn: a lowercase name is the binding spelling the warning
    recommends, and W0801's title is "a capitalized name" (not taken: every letter case);
  - scope, not type: the rule needs resolution alone, so lupin, which has no checker, answers it
    at the same rung, and it is the set E0301 already uses (not taken: the checker's
    type-directed rule, compiler-only);
  - the std tree and dependencies are not exempt: a W0801 there is reported.
- **The text and the fixes: hit.** The rendered snapshots
  (`crates/wolf_sema/tests/snapshots/binder_diagnostics__*.snap`) carry the predicted headline,
  labels, note and both fixes. The order is a decision: the rename first, because `wolf fix`
  applies the first machine-applicable fix and the qualified spelling turns a compiling program
  into an E0806 (not taken: the qualified fix first, or always `maybe`). The rename reaches
  closure captures and interpolations, spells struct shorthands out, and appends `_` while the
  name would collide; with two enums declaring the name, each qualified alternative is `maybe`;
  the name before `@` offers the rename alone.
- **E0806: hit.** At the arm's name, at the resolve rung, with the lowercase receive as its one
  machine-applicable fix; the note spells the test in the arm's body.
- **The rows: hit.** At the rows' commit (`1c0a75ba`, the base's code) the corpus step is
  `1270 file(s), 5 bad`: the four W0801 rows by name ("warning ledger mismatch — fired [],
  header declares [W0801]") and the `select` row ("header claims phase `parse` but deepest
  passing phase is `wir`"); `four_machine_regressions` fails the five new rows by name and passes
  the two controls (`notes/s282-evidence/mac-red-1c0a75ba.log`). At the fix every row is green,
  the spans exact on all three compiled tiers, and the corpus step is `1270 file(s), 0 bad`
  (`mac-fix-5441d267.log`).
- **The lowering ledger: as predicted, one move.** `conc/select_arm_bare_variant.lu` goes from
  `lowers` at the base's code to `not mem-clean (pre-wir rungs own it)`: the resolver refuses it
  now.
- **The corpus, wolf-std and the org's repositories: hit, none warns.** No file that was already
  in the corpus warns (the corpus step is warning-ledger exact). Every `.lu` file of 29 public
  checkouts (the org's wolf repositories at their trunks, wolf-std at the `STD-PIN` and at its
  trunk; 3,581 files) and of one private downstream consumer, at the resolve rung on the base
  and on the head: 0 W0801 and 0 E0806 on both (`mac-measure-447ba6d8.log`). A syntactic census
  of the same trees finds no capitalized name in any binder position at all
  (`census-org.txt`); the only capitalized `let`s are item-level constants, which the rule does
  not cover.
- **lupin: hit.** lupin built from the wolf-interp branch answers the compiler's binder rows
  with the same verdicts, stdout and W0801 spans, runs the handler row, and refuses the `select`
  row with E0806 at the same span (`mac-measure-447ba6d8.log`, the last step).
- **A miss, of the rig and of the code:** the first build of the fix failed (`Span` has no
  order, so a `BTreeSet<Span>` does not compile; `mac-fix-eb97a800.log`), and that job's later
  steps ran on the previous build's binary. `4a9c9da9` holds the spans in a hash set, and the
  rig now stops at a failed build. The row headers were rewritten once to cite the new clause
  (`de3f7ade`), which moved every span by the header's growth; the rows and their lexer and
  parser snapshots moved with them.

## Evidence index

`notes/s282-evidence/index.txt` lists every file:
- `base-ed19904c.log`, `probe/`: the base, every probe on four machines, with `wolf build` and
  `wolf fix` renderings;
- `mac-red-1c0a75ba.log`: the rows red at the base's code, and the snapshot sets generated;
- `mac-fix-eb97a800.log`: the first fix build, failed (trimmed at the failure);
- `mac-fix-5441d267.log`: the fix: rows green on four machines with lupin 0.1.59, the corpus,
  the probes, fmt, clippy, the `wolf_sema` suite, the snapshot sets;
- `mac-regen-1183e89e.log`: `spec-extract` (anchors: none removed, `gram.pat.binder` added),
  `diag-catalog`, and the snapshot sets under the rows' new headers;
- `mac-measure-447ba6d8.log`, `census-org.txt`: the corpus-wide and org-wide measurements, and
  the rows with lupin from the wolf-interp branch;
- `rig/`: the scripts as run.

The records at the final head live outside the tree, because a record cannot predate the head it
names. The PR body quotes every `END` line of the Linux gauntlet record and of the plant record.

## Done-when

Commits:
- `084ea023`: the base measured; `dd529314`: the prediction, before the first change.
- `1b778931`, `b4b17012`, `c0ec0709`: the W0801 rows, their silent controls, and the `select`
  refusal row with its control, red first.
- `1c0a75ba`: the rows on every machine (`four_machine_regressions`).
- `d48a821b`, `17c72757`: the rows' lexer and parser snapshots.
- `3e0b7a89`: the binder analysis (`binder.rs`): homes, names, the fresh lowercase name, the
  reports.
- `eb97a800`: the resolver reports W0801 at binders and E0806 at `select` arms.
- `b8784e25`, `4a9c9da9`: formatting, and the hash set.
- `5441d267`, `3a1b98ab`: the rendered snapshots and the structured fix tests.
- `024502a5`: the lowering ledger.
- `a0ce8f4f`: W0801's and E0806's explanations.
- `f5860197`, `0f8f8e3b`: `[gram.pat.binder]` and §9.2's lines; the anchor indexed.
- `de3f7ade`, `10732182`, `cc8770a9`: the rows cite the clause; their spans and snapshots move.
- `1183e89e`: `wolf fix`, `--deny-warnings`, `#[allow(w0801)]`, the refusal and `--explain`
  through the binary.
- `142a6efc`: the catalogs regenerated.
- `447ba6d8`: the changelog.
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`dd529314`)
- [x] the rows red at the base's code, by name, in the corpus step and on every machine
      (`mac-red-1c0a75ba.log`)
- [x] the rows green at the fix, spans exact on the checked machine, native and release; the
      corpus `0 bad` (`mac-fix-5441d267.log`)
- [x] fmt, clippy (`wolf_sema`, `wolf_driver`, all targets, `-D warnings`), the `wolf_sema`
      suite with the new snapshots
- [x] `spec-extract`: no anchor removed, one added; the catalogs regenerated
- [x] the corpus, wolf-std and the org's wolf repositories: no new W0801, no E0806
- [x] lupin from the wolf-interp branch agrees on every row
- [ ] the macOS `cargo xtask ci` at the head (a shakedown, not a record)
- [ ] the Linux gauntlet record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once

Issues, for the orchestrator to file:
- An item-level destructuring with an annotation gives each binder the whole annotation's type:
  `let (Drink, N): (Kind, int) = (Kind.Food, 3)` makes `Drink` a `(Kind, int)` (probe `b29`,
  E0401 at its first use), and lupin runs the same program and prints `drink N`.
- lupin cannot import a function by item (`use kinds.name`: "`name` does not resolve"; probe
  `b14`), and reports E0301 on a struct literal whose field is capitalized (`Q { Drink: 5 }`,
  probe `b26`).
- lupin reads an `else |Boom|` handler name that spells a capitalized tag of the operand's row
  as a test, and refuses the program statically (E0809, "leaves `Bust` unhandled"); the compiler
  binds every handler name and runs it (probe `b31`, `exit(0)`, prints `7`). This lane changed
  only the variant case.
