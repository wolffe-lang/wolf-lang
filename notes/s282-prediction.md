# s282 — variant names in binder positions (W0801) and select arms (E0806): prediction, before the first change

Written 2026-11-06 on the Mac (arm64 macOS), before any change to either tree. It covers both
repositories: wolf-lang (the compiler) and wolf-interp (lupin, the reference interpreter),
whose own prediction cites this one.

## Inputs, read
- Branch `s282` cut at wolf-lang trunk `ed19904c`, and at wolf-interp trunk `f8c2ff7`.
- The previous lane's note (`notes/s281-the-nested-variant.md`) and wolf-lang#1043: a bare
  variant nested in a match payload now tests its variant on every machine; binder positions
  were left binding silently, and a `select` arm's bare variant was left for a refusal. Both
  decisions are this lane's input: binders keep binding but warn, and the `select` arm is
  refused by name.
- The spec: `[gram.pat]`, `[gram.pat.nullary]`, `[gram.pat.struct]` ("admitted wherever a
  binder pattern is (`let`/`var`, D63 comma groups, `for` headers)"), `[gram.expr.variant]`,
  `[gram.item.let]`, `[gram.expr.conc]`, `[conc.select]`, §9 (`[diag.sev]`, `[diag.family]`,
  `[diag.level]`).
- The compiler: the resolver (`crates/wolf_sema/src/resolve.rs`) binds every binder through
  `pattern_names` and records the binding table (`Resolution::refs`, every binder and every use
  as a `Local` ref); the checker's `bind_pattern` (`check.rs`) binds every bare name and asks no
  type; its `variant_homes` is the set of enums a file names directly (module enums, then item
  imports) that `[gram.expr.variant]`'s E0301 already uses; W0801 today lives in the checker's
  `match_pattern`, for a capitalized arm name over a scrutinee with no cases (s68);
  `synth_select` binds a bare arm name and declines every other arm shape but `exit(…)`.
- The diagnostics machinery: `Suggestion` with `Applicability`; `wolf fix` applies the FIRST
  machine-applicable suggestion of each diagnostic and lists the rest as "alternative to the
  selected fix"; the LSP offers every machine-applicable suggestion as a quick fix.
- lupin: its warning walk (`src/lint.rs`) runs at the resolve rung and lists W0801 as
  honest-absent (it has no case tables); `Module::variants` (variant → the module's enums that
  declare it) is what its runtime reads to decide that a bare pattern name is a variant test.

## The base, measured
`ed19904c` built on the Mac (`cargo build --locked -p wolf_driver -p wolf_rt`, Rust 1.97.1);
every probe on the checked machine, native and release (`wolf conform-run --json --<tier>`) and
on lupin 0.1.59 (the release archive, digest `9f091af8…f5f5`, read twice and equal to the
release page). Evidence: `notes/s282-evidence/base-ed19904c.log`, probes under
`notes/s282-evidence/probe/`.

| probe | checked, native, release | lupin 0.1.59 |
|---|---|---|
| `let (Drink, n) = (Kind.Food, 3)` | binds: `food 3`, no warning | same |
| `let P { k: Drink, n } = …` | binds, no warning | same |
| `let Drink = Kind.Food` | binds, no warning | same |
| `var (Drink, n) = …` | binds, no warning | same |
| `let a = 1, (Drink, n) = …` (a comma group) | binds, no warning | same |
| `let ((Drink, a), b) = …` | binds, no warning | same |
| `let Drink @ (k, n) = …` | declined ("destructuring bindings (this pattern shape)") | runs |
| `for (Drink, n) in xs`, `for Drink in xs` | binds, no warning | same |
| `pick(0) else \|Drink\| 7` | binds: `7`, no warning | refuses at run: "the `else \|pat\|` pattern did not match" (it reads a test) |
| `fn twice(Drink: int)`, `fn(Drink: int) …` | binds, no warning | same |
| `Drink from ch` over `channel[Kind]` | binds: `food`, no warning | refuses at run: "the committed `select` arm's pattern did not match" |
| `drink from ch` (control) | `food` | same |
| `Kind.Drink from ch` | declined ("this select-arm pattern shape") | declined |
| an item-imported enum (`use kinds.Kind`), `let (Drink, n)` | binds, no warning | binds (b27, b28); b14 also imports a fn by item, which lupin cannot resolve |
| an enum reached only as `kinds.Kind` | declined natively (member access) | declined |
| `let (fast, n) = (Mode.slow, 3)` over `enum Mode { fast, slow }` | binds | cannot construct `Mode.slow` |
| `let (Zed, n) = (5, 3)`, `let (drink, n) = …` (controls) | binds, no warning | same |
| `let (Some, n) = (5, 3)` over `enum Opt { Some(int), None }` | binds, no warning | same |
| two enums declaring `Drink`, `let (Drink, n)` | binds, no warning | same |
| an item-level `let (Drink, N) = …` | E0407 (each binder needs a type); annotated, every binder takes the whole annotation's type (E0401) | runs, wrongly |
| an item-level `let Drink: Kind = …` | declined: module state must be static data (`[mem.static.3]`) | declined |
| `let Q { Drink, n } = …` (a capitalized field, shorthand) | binds | E0301 on the struct literal's field |
| `match k { Drink => … }` (control) | tests | tests |

Every binder position binds a name that spells a variant, silently, on every machine. No
probe warns.

## The rule, predicted

**Positions.** A *binder position* is a pattern that binds unconditionally:
1. a statement `let` or `var` binder, every binder of a comma group (D63);
2. a `for` header;
3. the `else |…|` handler's binding form (the `|Tag(p)|` form is a test and is not a binder).

At any depth through the shapes a binder admits: a tuple element, a struct field (explicit
`k: Drink` and the shorthand `Drink`), and both sides of `@` (`Drink @ (k, n)` and
`p @ (Drink, n)`). Refutable shapes inside a binder (a path, an or-pattern, a literal, a range)
are already E0806 and are not walked.

Not binder positions for this rule, each a decision with its reason:
- **Function and closure parameters.** The grammar takes a name there, never a pattern
  (`param ::= param_mode? IDENT ':' type`, `closure_param ::= param_mode? IDENT (':' type)?`),
  so no test can be meant, and the qualified alternative would not parse. They stay silent.
  The other option was to warn with the lowercase fix alone.
- **Item-level `let` and `var`.** They declare module items, not locals. At this cut module
  state cannot hold an enum value (`[mem.static.3]`), and an item-level destructuring gives
  each binder the whole annotation's type, so the destructuring shape this rule is about cannot
  be written there; renaming an item also crosses files and importers. They stay silent.
- **`match` arms** keep their rule: a bare name is a test when it spells a case of the
  scrutinee's type, and the existing W0801 covers a capitalized arm name over a scrutinee with
  no cases. Unchanged.
- **A `select` arm** is the one position refused instead of warned (E0806, below).

**Which names.** A bare name that begins with an uppercase letter and spells a variant of an
enum *in scope*:
- an enum declared in the binder's own module (any file of the directory, D32);
- an enum this file imports by item (`use m.Kind`, `use m.{Kind}`, `use m.Kind as K`, then
  spelled `K`);
- the prelude contributes nothing: wolf has no builtin enum (the builtin spellings are row
  tags, and row tags are not variants).

That is exactly the set `[gram.expr.variant]`'s E0301 consults. Variants that carry values
count (`Some`). An enum reached only through a module path (`kinds.Kind`) is not in scope for
a bare name, and neither is an error-row tag (`else |gone| …` stays silent).

The binder's type does not enter the rule: `let (Drink, n) = (3, 4)` warns too. The rule is
scope, not type, because it is one both machines can run at the resolve rung (lupin has no
checker), and it is the same set the bare-value refusal uses. The other option was the
checker's type-directed rule, compiler-only, with lupin honest-absent for the binder half.

A lowercase name never warns, even one that spells a lowercase variant (`let (fast, n)` over
`enum Mode { fast, slow }`): a lowercase name is the binding spelling the warning itself
recommends, and W0801's own title is "a capitalized name". The other option was every letter
case, with no lowercase fix to offer.

The std tree is not exempt, and neither is a dependency: a W0801 in std or in a dependency is
reported.

**W0801's text** (the resolver; one diagnostic per binder name):
- headline: "`Drink` binds a new name here — it does not test the variant `Kind.Drink`";
- primary label at the name: "a binding, not a test";
- secondary label at the variant's declaration: "`Drink` is declared here, in `Kind`";
- note: "a `let`, `var`, `for` or `else |…|` pattern binds every value it meets, so a bare name
  there is a new binding even when it spells a variant. To test the value, `match` on it
  (`Kind.Drink => …`); to keep the binding, give it a lowercase name so it reads as one.";
- with a second enum declaring the name: "`Drink` is also a variant of `Menu` here.".

**The two fixes**, in this order:
1. "bind it under a lowercase name: `drink`": the binder and every use of it, from the
   binding table (interpolations and closure captures included; a struct shorthand becomes
   `Drink: drink`). The name is the binder's snake-case spelling; when that spelling is a
   keyword, a prelude or builtin type name, an item or import of this module or file, or is
   already spelled anywhere in the file, `_` is appended until it is none of those, so the
   rename can neither capture nor shadow anything. Machine-applicable: it is the binding the
   program already has, renamed, so it cannot change a correct program's meaning. `wolf fix`
   applies this one.
2. "test the variant instead: `Kind.Drink`" (`Kind.Some(_)` for a variant that carries one
   value): the binder alone. Machine-applicable when one enum declares the name, `maybe` when
   several do (one alternative per enum). In a binder position the result is refutable, so
   E0806 then asks for a `match`. Not offered on the name before `@`, where no pattern can
   stand.

The order is a decision: the lane's brief names the qualified fix first, but `wolf fix --apply`
takes the first machine-applicable suggestion, and the qualified spelling turns a compiling
program into an E0806. The rename comes first, so `wolf fix` never breaks a program; the
editor offers both. The other options were the qualified fix first, or the qualified fix
always `maybe`.

**`--deny-warnings`** turns W0801 into a failure through the existing level machinery, and
`#[allow(w0801)]` suppresses it, as for every warning. Both rungs see it: it is a resolve-rung
warning, so a program that later fails typing still reports it.

**E0806 for a `select` arm** (the resolver): `Drink from ch`, with `Drink` capitalized and
spelling an in-scope variant by the same rule, is refused:
- headline: "`Drink` is the variant `Kind.Drink`, and a `select` arm cannot test it";
- primary label: "a receive cannot fail to match";
- secondary label at the declaration: "`Drink` is declared here, in `Kind`";
- note: "a committed receive takes whatever its channel delivers, so a `select` arm's pattern
  only binds. Receive into a name and test it in the arm's body: `k from ch => match k {
  Kind.Drink => …, _ => … }`.";
- one fix, machine-applicable: "bind it under a lowercase name: `drink`" (the binder and its
  uses), the binding the compiled tiers gave this arm before.

A lowercase arm name and a capitalized name that spells no in-scope variant bind, as before.
This is the one new refusal: a program that compiled (`Drink from ch` bound on the three
compiled tiers) no longer does. lupin refused it at run before; it refuses it statically now.

**The catalog.** W0801 keeps its title; its explanation gains the binder paragraph. E0806's
explanation gains the `select` arm. `wolf --explain W0801` and `wolf --explain E0806` print
them. Each code gets new reviewed fixtures.

**The spec.** A new clause `[gram.pat.binder]` in §5 states the positions, the rule, the
warning, its fixes and the `select` refusal; §9.2's W0801 line and an E0806 line name both.
No existing anchor moves or narrows.

## The rows, predicted

wolf-lang, committed red first at the base's code, then green at the change:
- **corpus** (each `check: run(exit=0, stdout=…)`, `phase: run`, `warns: W0801`):
  `lints/binder_variant_let.lu` (a tuple, a struct field, a plain `let`, a `var`, a comma
  group, a nested tuple, a variant that carries a value), `lints/binder_variant_for.lu` (a
  tuple header and a plain one), `lints/binder_variant_else.lu` (the handler),
  `lints/binder_variant_imported/main.lu` (an item-imported enum). Red at the base by name in
  the corpus step: "warning ledger mismatch — fired [], header declares [W0801]", four rows.
- **corpus controls**, green at the base and after (no `warns:`, so warning-clean):
  `lints/binder_variant_controls.lu` (a lowercase binder, a capitalized non-variant, a
  parameter and a closure parameter named `Drink`).
- **corpus refusal**: `conc/select_arm_bare_variant.lu`, `check: fail(E0806)`. Red at the base
  by name: it runs (`exit(0)`) where the header pins `fail(E0806)`, and its phase header does
  not match. Control `conc/select_arm_lowercase_binds.lu` runs, green before and after.
- **every machine** (`crates/wolf_driver/tests/four_machine_regressions.rs`): each W0801 row's
  `warnings` array, `{code, span}`, exact, on the checked machine, native and release, with its
  stdout; lupin's verdict and stdout, and its W0801 spans whenever its record carries W0801
  (lupin 0.1.59 does not). The handler row runs on the three compiled tiers only: lupin 0.1.59
  reads the handler's name as a test. Red at the base: no W0801 in any array.
- **sema snapshots** (`crates/wolf_sema/tests/`): one rendered snapshot per position, with both
  fixes; the `select` refusal; a structured test of each suggestion's edits and applicability
  (the rename reaches every use, a struct shorthand expands, a taken name gets `_`, two enums
  make the qualified fix `maybe`, `@` offers the rename alone).
- **the driver**: `wolf fix` renames, a second run finds nothing, and the qualified fix is
  listed as the alternative; `--deny-warnings` fails a binder row, naming W0801; `--explain
  W0801` and `--explain E0806` print the new paragraphs.
- **the snapshot sets** the new corpus files join: the lexer's and parser's per-file snapshots,
  the lowering ledger, and the parse-tree snapshots of `corpus/grammar/` (none of these files
  is under `grammar/`, so that set should not move).
- **the corpus, whole**: no existing program warns. The only W0801 in the corpus today is the
  match-arm row `lints/binder_capitalized.lu`; a search of the corpus for a capitalized binder
  that spells a variant finds none.
- **wolf-std and the org's other wolf repositories** at their pins: no new W0801 expected; a
  search of the sources found no local capitalized binder (the capitalized item-level `let`s are
  constants that spell no variant). Every new W0801 is listed, never silenced.

wolf-interp (lupin's mirror), its own rows red first at its trunk:
- W0801 at the same spans as the compiler on every binder fixture, emitted by the warning walk,
  std included; the controls silent;
- the handler's binder binds when it spells an in-scope variant (today it tests and refuses at
  run): a run row;
- E0806 at the `select` arm's name, statically, with the variant named;
- the warning ledger: W0801 is enforced in one direction (lupin never reports a W0801 the
  corpus ledger lacks), because the match-arm half needs case tables lupin does not have; the
  exact spans are the mirror rows' job. The other options were a waiver for the one match-arm
  row, or a static guess at the match-arm half.

**The plants.** wolf-lang: `s282-plant-*`, the `for` header's check dropped, red by name at the
`for` row (the corpus ledger and the four-machine row). wolf-interp: the same break in lupin's
walk, red at its `for` mirror row.

**No meaning changes** beyond the one refusal: every program that compiled still compiles and
prints the same, except a `select` arm that spells an in-scope variant.
