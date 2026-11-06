# s282 — variant names in binder positions warn (W0801); a select arm's is refused (E0806)

`let (Drink, n) = pair` binds `Drink` to whatever the first element holds, on every machine,
with `Kind.Drink` in scope and nothing said. It still binds: a binder pattern binds every
value it meets. It now warns W0801 at the name, naming the variant and its declaration, with
two fixes: another name with every use renamed (the one `wolf fix` applies: `drink`, or
`fast_` for a variant spelled `fast`), and the qualified `Kind.Drink`. The letter case does not
enter, as it does not where a `match` payload reads a bare name as a variant. The same holds in a `var` binder, a comma group, a `for` header and an
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

Committed before the first change (`292e9b3a`, `notes/s282-prediction.md`). Scored:

- **The base: hit.** Every binder position bound a variant-spelled name silently on all four
  machines; the handler and the `select` arm refused at run on lupin, which read them as tests
  (`notes/s282-evidence/base-ed19904c.log`).
- **Positions, names, scope: hit, as decided.** A statement `let`/`var` binder (every binder of
  a comma group), a `for` header and the `else |…|` handler's binding form, through tuples,
  struct fields (explicit and shorthand) and both sides of `@`. A name that spells a variant of
  an enum the module declares or the file imports by item; the prelude has no enum. Scope
  decides, not type. Parameters, closure parameters, item-level bindings, a module-path enum
  (`kinds.Kind`) and row tags stay silent. The decisions, each with the alternative not taken:
  - **two narrowings of the lane's list.** The lane was asked to cover function and closure
    parameters, and every binder position that can hold a bare variant name. Two places that
    bind a name are left out, by decision, for one reason: W0801 applies where a pattern can
    stand, so where `Kind.Drink` could have been meant as a test. A parameter takes a plain
    name, never a pattern (`[gram.item.fn]`, `[gram.expr.closure]`), so no test can be meant
    there. An item-level `let` or `var` cannot hold an enum (`[mem.static.3]`), so no variant
    test can be meant there either. The spec clause says both in these words. (Not taken: warn
    at parameters with the rename alone; warn at item level with a module-wide rename. An
    item-level destructuring also gives each binder the whole annotation's type today, probe
    `b29`, E0401, filed apart.)
  - **every letter case: a miss, decided against the prediction.** The prediction had only
    capitalized names warn. The rule is instead the one a `match` payload has used since s281
    (wolf-lang#1043) to decide that a bare name tests a variant: `names_sema_case`
    (`crates/wolf_wir/src/lower.rs:24304`) compares the name with the variant names as written,
    with no letter-case test (`variants.iter().any(|v| v.name == name)`, line 24317). So a name
    that would test in a match payload warns in a binder, and one that would bind there stays
    silent: `let (fast, n)` over `enum Mode { fast, slow }` warns, and `for slow in …` does.
    The rename cannot then be "the lowercase spelling": it is the snake-case spelling, with a
    trailing `_` while that is a keyword, a name the file or its scope holds, or a variant in
    scope, so `fast` becomes `fast_`; it stays machine-applicable and first. The set the name
    is compared with is the scope set below, not a scrutinee's cases: a binder has no
    scrutinee type to read.
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
- **E0806: hit.** At the arm's name, at the resolve rung, in either letter case, with the
  rename as its one machine-applicable fix; the note spells the test in the arm's body.
- **The rows: hit.** At the rows' commit (`74e53ef4`, the base's code) the corpus step is
  `1270 file(s), 5 bad`: the four W0801 rows by name ("warning ledger mismatch — fired [],
  header declares [W0801]") and the `select` row ("header claims phase `parse` but deepest
  passing phase is `wir`"); `four_machine_regressions` fails the five new rows by name and passes
  the two controls (`notes/s282-evidence/mac-red-1c0a75ba.log`). At the fix every row is green,
  the spans exact on all three compiled tiers, and the corpus step is `1270 file(s), 0 bad`
  (`mac-fix-5441d267.log`). The two lowercase-variant rows (`lints/binder_variant_lowercase.lu`,
  `conc/select_arm_lowercase_variant.lu`) were added red first once the letter case was
  decided: at their commit (`ad993530`, the capitalized-only code) the corpus step is
  `1272 file(s), 2 bad`, those two rows by name, and `four_machine_regressions` fails exactly
  `binders_that_spell_a_lowercase_variant_warn` (W0801 spans `[]` against
  `[(566, 570), (630, 634)]`) and `a_select_arm_that_spells_a_lowercase_variant_is_refused`
  (`exit(0)` against `fail(E0806)`), 20 passed (`mac-red-4261280f.log`). At the case-blind code
  (`9f93d3a3`) all 22 pass and the corpus step is `1272 file(s), 0 bad`
  (`mac-fix-83a64499.log`).
- **The lowering ledger: as predicted, one move.** `conc/select_arm_bare_variant.lu` goes from
  `lowers` at the base's code to `not mem-clean (pre-wir rungs own it)`: the resolver refuses it
  now. The two later rows enter the ledger as `lowers` (the binder row) and
  `not mem-clean (pre-wir rungs own it)` (the `select` row).
- **The corpus, wolf-std and the org's repositories: hit, none warns.** No file that was already
  in the corpus warns (the corpus step is warning-ledger exact). Every `.lu` file of 29 public
  checkouts (the org's wolf repositories at their trunks, wolf-std at the `STD-PIN` and at its
  trunk; 3,581 files) and of one private downstream consumer, at the resolve rung on the base
  and on the head: 0 W0801 and 0 E0806 on both (`mac-measure-447ba6d8.log`). A syntactic census
  of the same trees finds no capitalized name in any binder position at all
  (`census-org.txt`); the only capitalized `let`s are item-level constants, which the rule does
  not cover. With the rule case-blind the head was measured again (`9f93d3a3`,
  `mac-remeasure-83a64499.log`): 0 W0801 and 0 E0806 in every tree, and every tree's verdicts
  equal to the base's, file count for file count (3,615 files in 30 trees). The census in
  either letter case (`census-variant.txt`) finds no binder name in any of those trees that
  equals a variant name the tree declares; in the corpus it finds exactly this lane's rows.
- **lupin: hit.** lupin built from the wolf-interp branch answers the compiler's binder rows
  with the same verdicts, stdout and W0801 spans, runs the handler row, and refuses the `select`
  row with E0806 at the same span (`mac-measure-447ba6d8.log`, the last step; at the case-blind
  heads, `b0b521c` and `9f93d3a3`, all nine rows, `mac-remeasure-83a64499.log`).
- **The direction of the one-way ledger: lupin's W0801 is a subset of the compiler's.** The
  compiler may report W0801 where lupin does not; lupin never reports one the compiler does
  not. Both machines were run over every corpus file, lupin's mirror fixtures and every probe,
  each stopping at a static rung, and their W0801 spans compared file by file
  (`tools/w0801-diff.py`, in `mac-remeasure-83a64499.log`): 0 spans lupin-only anywhere. The
  compiler-only spans are two, and only two:
  - `lints/binder_capitalized.lu` (449..452), the checker's match-arm half (a capitalized arm
    name over a scrutinee with no cases), which needs case tables lupin does not have; it is
    the only corpus file where the two differ, and lupin keeps W0801 one way for it;
  - probe `b34` (63..68), a binder over an enum declared as `type Kind = enum { Drink, Food }`.
    lupin does not register that enum's variants at all (a `match` arm `Drink =>` over it does
    not apply either, `unsupported`), so its binder half cannot see them. That is a real
    difference in the binder half, outside the corpus, filed as an issue on wolf-interp.
  Every other file with a W0801 (the five binder rows, the five mirror fixtures, twenty-one probes)
  has the same span set on both. The comparison also lists one E0806 mismatch,
  `typecheck/refutable_let.lu`: the checker's refutable-`let` E0806, which lupin answers
  `unsupported` (honest) at its full run; the `select` E0806 agrees everywhere.
- **A miss, of formatting:** the shorter rename message let one assertion fit a line, and the
  green take's `cargo fmt --check` failed on it (`mac-fix-83a64499.log`); `5bfaa1da` formats it.
- **A miss, of the rig and of the code:** the first build of the fix failed (`Span` has no
  order, so a `BTreeSet<Span>` does not compile; `mac-fix-eb97a800.log`), and that job's later
  steps ran on the previous build's binary. `95984cc8` holds the spans in a hash set, and the
  rig now stops at a failed build. The row headers were rewritten once to cite the new clause
  (`f8d03f9e`), which moved every span by the header's growth; the rows and their lexer and
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
- `mac-red-4261280f.log`: the lowercase-variant rows at the capitalized-only code, two red by
  name;
- `mac-fix-83a64499.log`: the case-blind code: the rows, the corpus, every probe and its
  rendering, fmt (red, one line), clippy, the `wolf_sema` suite, the snapshot sets;
- `mac-measure-447ba6d8.log`, `census-org.txt`: the corpus-wide and org-wide measurements, and
  the rows with lupin from the wolf-interp branch;
- `mac-remeasure-83a64499.log`, `census-variant.txt`: the same at the case-blind head, the
  census in either letter case, and the compiler-lupin W0801 comparison;
- `rig/`: the scripts as run.

The branch was rebased once, onto trunk `03c0f04e`, which changed the runtime's signal and
reactor threads, their row, `spec/11-os.md` and notes, and none of this lane's files but the
changelog. Every log here ran before that rebase and is named by the commit it ran at;
`rebase-map.txt` gives each one's rebased commit, which has the same subject and the same change.
The note names rebased commits throughout. The records at the final head live outside the tree,
because a record cannot predate the head it names. The PR body quotes every `END` line of the Linux gauntlet record and of the plant record.

## Done-when

Commits:
- `72e9a067`: the base measured; `292e9b3a`: the prediction, before the first change.
- `4ed2186a`, `6de8af3e`, `ba81d130`: the W0801 rows, their silent controls, and the `select`
  refusal row with its control, red first.
- `74e53ef4`: the rows on every machine (`four_machine_regressions`).
- `347f805b`, `e29deee0`: the rows' lexer and parser snapshots.
- `2f979924`: the binder analysis (`binder.rs`): homes, names, the fresh lowercase name, the
  reports.
- `8ec9d663`: the resolver reports W0801 at binders and E0806 at `select` arms.
- `e824f8be`, `95984cc8`: formatting, and the hash set.
- `a3d83762`, `ad695a04`: the rendered snapshots and the structured fix tests.
- `389c5dec`: the lowering ledger.
- `9ba8d964`: W0801's and E0806's explanations.
- `4ed90e8d`, `c4201600`: `[gram.pat.binder]` and §9.2's lines; the anchor indexed.
- `f8d03f9e`, `495324eb`, `dd4a0211`: the rows cite the clause; their spans and snapshots move.
- `e4fefadc`: `wolf fix`, `--deny-warnings`, `#[allow(w0801)]`, the refusal and `--explain`
  through the binary.
- `b60d35dc`: the catalogs regenerated.
- `e02189ef`: the changelog.
- `48d24972`, `ad993530`: the lowercase-variant binder and `select` rows, and their
  four-machine tests, red first.
- `296c96e4`: binder names read in either letter case; the rename steps past a variant in
  scope. `9f93d3a3`: its tests (the rename to `fast_`, the refusal).
- `928819b5`, `bfddd361`, `86525c2e`, `03c56bb0`: the explanations, the spec clause (the letter
  case and the two narrowings in plain words), the changelog, a test comment.
- `5bfaa1da`: formatting. `2f381d7b`, `6f0af4ef`, `e71a0f4c`, `015c9357`: the new rows'
  snapshots, the lowering ledger, the rendered fixes.
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`292e9b3a`)
- [x] the rows red at the base's code, by name, in the corpus step and on every machine
      (`mac-red-1c0a75ba.log`); the lowercase-variant rows red at the capitalized-only code
      (`mac-red-4261280f.log`)
- [x] the rows green at the fix, spans exact on the checked machine, native and release; the
      corpus `0 bad` (`mac-fix-5441d267.log`, and case-blind `mac-fix-83a64499.log`)
- [x] fmt, clippy (`wolf_sema`, `wolf_driver`, all targets, `-D warnings`), the `wolf_sema`
      suite with the new snapshots
- [x] `spec-extract`: no anchor removed, one added; the catalogs regenerated
- [x] the corpus, wolf-std and the org's wolf repositories: no new W0801, no E0806, capitalized
      and case-blind (`mac-measure-447ba6d8.log`, `mac-remeasure-83a64499.log`)
- [x] lupin from the wolf-interp branch agrees on every row; lupin's W0801 is a subset of the
      compiler's over the corpus, the mirror fixtures and the probes (`mac-remeasure-83a64499.log`)
- [ ] the macOS `cargo xtask ci` at the head (a shakedown, not a record)
- [ ] the Linux gauntlet record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once

Issues, filed once both PRs are open, each with its probe and output (linked from the PR, no
closing keyword):
- wolf-lang: an item-level destructuring under an annotation gives each binder the whole
  annotation's type (`let (a, b): (int, int) = (1, 2)`, then `a + b`: E0409, "this is
  `(int, int)`"; probe `b36`; with an enum element, `b29`, E0401).
- wolf-interp: lupin cannot import a function by item (`use kinds.name`: "`name` does not
  resolve"; probe `b14`).
- wolf-interp: lupin refuses (E0301) the use of a name a struct pattern's shorthand bound when
  an enum in scope declares a variant of that name (`let Q { Drink, n } = …`, then
  `Drink + n`; probe `b35`); the compiler binds it and prints `8`.
- wolf-interp: lupin reads an `else |Boom|` handler name that spells a tag of the operand's row
  as a test and refuses statically (E0809); the compiler binds a bare handler name and runs it
  (probe `b31`, prints `7`). This lane changed only the variant case.
- wolf-interp: at the next pin advance, the vendored corpus gains these rows and the exact-code
  lists in `tests/conformance.rs` should gain E0806 for the `select` rows.
- wolf-interp: lupin does not register the variants of an enum declared as
  `type Kind = enum { … }` (probe `b34`: no W0801 where the compiler has one; a `match` arm
  `Drink =>` over it does not apply).
- wolf-interp: lupin does not resolve a qualified variant whose name is lowercase
  (`Mode.slow` of `enum Mode { fast, slow }`: "does not resolve"; probes `b16`, `b32`).
- wolf-interp: lupin runs an item-level destructuring with capitalized binders and prints the
  names, not the values (`let (Drink, N): (Kind, int) = (Kind.Food, 3)`: `drink N`; probe `b29`).
