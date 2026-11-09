# s289 prediction: the diagnostics sweep

Committed before the first change. Base: wolf-lang trunk `a57276f2` (0.2.38). The base was measured on
macOS arm64 with a debug build of the base (`wolf` sha256 `dedaa6d8…5971e`, its runtime
`libwolf_rt.a` beside it) and lupin 0.1.60 (the `PAIRING` stamp; the aarch64-apple-darwin archive,
sha256 `f3cae16f…6bb8`, read twice and equal to the release page's digest). lupin's source was read at
wolf-interp trunk `725d86da`; nothing between 0.1.60 and that commit touches the paths below (the
interval is the literal-pattern and front-end-rulings mirrors).

Evidence: `notes/s289-evidence/base-a57276f2.log` (every probe on the checked machine, native,
release and lupin), the probes under `notes/s289-evidence/probe/` (one directory each, so no two
share a module), `census-base-a57276f2.tsv` (every ambient name in three uses) and
`corpus-scan-base-a57276f2.tsv` (every corpus entry's checked record). The scripts that made them
are beside them.

Five items. The first was unfiled until this lane: it is wolf-lang#1085, filed before this note.

## 1. What the base does

### wolf-lang#1085: an undeclared capitalized name refused as "unsupported"

A capitalized name that resolves nowhere is deferred by the resolver as a possible error tag
(D30). The checker accepts it only where an error row takes it, and everywhere else declines it with
`NotYet { "an error-row tag outside `!T` context" }`, from two sites in `check.rs` (the bare name in
`synth_path`, and the call form `Bad(3)` in `synth_call`).

| probe | source (the line that matters) | checked, native, release | lupin 0.1.60 |
|---|---|---|---|
| `e1_tail` | `fn answer() -> int { OPEN }` | unsupported, "an error-row tag outside `!T` context" at `OPEN`; native and release exit 4 | runs, exit 1, prints `error: OPEN` |
| `e2_assign` | `d.kind = INPUT` in `main` | the same, at `INPUT` | exit 1, `error: INPUT` |
| `e3_return` | `return CANCEL` in `-> int` | the same, at `CANCEL` | exit 1 |
| `e4_let` | `let x = OPEN` in `-> int` | the same | exit 1, `error: OPEN` |
| `e5_payload` | `Bad(3)` as the tail of `-> int` | the same, at `Bad` | exit 1, `error: Bad` |
| `e7_fallible_let` | `let x = Bad` inside `-> !int` | the same | exit 0 |
| `e8_rowtag_nonfallible` | `bad` as the tail of `-> int`, the function spelling `! {bad}` in a `let` annotation | the same | unsupported, "`bad` does not resolve" |
| `e12_wolf_test` | a module that forgot a constant (`dialog.lu` without `OPEN`), and `wolf test dialog_test.lu` | `test dialog_test.lu ... unsupported (an error-row tag outside `!T` context)`, `1 unsupported`, exit 1 | (no test runner) |

Controls, which this lane leaves as they are:
- `e6_try`: `g()?` in `-> int` is E0604 on all three tiers, with the `! {Bad}` fix.
- `e9_unit_discard`: `if c { bad }` as the tail of `-> () ! {bad}`. The tag is the function's own,
  in a unit position, and `[type.unit.discard]` records this decline in those words. lupin
  discards it (exit 0).
- `e10_raise_ok`: `return Cancel` in `-> !int` raises; exit 7 on native, release and lupin.
- `e11_variant`: a bare variant value is E0301 with `Answer.Open` (`[gram.expr.variant]`).

The spec already answers the cases above that are refused: `[gram.expr.tagident]` says "a bare
identifier no expected row declares resolves as an ordinary name and misses with E0301", and
`[gram.expr.variant]` says the same of a bare capitalized name. The refusal is the compiler's gap,
not the spec's.

### wolf-lang#1083 and #708: the prelude stubs

Every name in the ambient table resolves. When the checker reaches one that it has no typing for, it
declines with `NotYet { "a std/prelude stub without a signature" }` (`check.rs`, `synth_path`).

| probe | source | checked, native, release | lupin 0.1.60 |
|---|---|---|---|
| `p1_worker_unimported` | `worker/worker.lu` beside `main.lu`, which calls `worker.main(args)` with no `use worker` (#1083's witness) | unsupported, the stub message at `worker`; exit 4 | unsupported, "`worker.main` does not resolve" |
| `p2_helper_unimported` | the same with the module named `helper` | E0301 "nothing named `helper` is in scope" at resolve, no note, no `use` | unsupported, "`helper.main` does not resolve" |
| `p3_worker_imported` | `p1` with `use worker` | runs, exit 0 (W0313, W0314) | exit 0 |
| `p4_worker_nomodule` | `worker.main(args)`, no module at all | the stub message | unsupported, "does not resolve" |
| `p5_worker_call` | `worker()` | the stub message | unsupported, "no pinned semantics" |
| `p10_acquire` | `acquire()` | the stub message | unsupported, "no pinned semantics" |
| `p6_min` | `print("{min(3, 4)}")` (#708) | the stub message at `min`; exit 4 | runs, prints `3` |
| `p7_min_shadow` | the same with its own `fn min` | W0304, runs; under `--deny-warnings` an error (#708) | W0304, prints `3` |
| `p8_zip` | `zip(xs, ys)` | the stub message | unsupported, "`zip` is in the ambient std stub but has no pinned semantics…" |
| `p9_host_value` | `let f = env_args` | the stub message | runs, exit 0 |

**The census** (`census-base-a57276f2.tsv`): 229 ambient names, each used as a value (`let f =
NAME`), called (`NAME()`) and as a member's receiver (`NAME.member()`), 687 uses. **431 answer
"a std/prelude stub without a signature"**:
- as a value, 208 names: every function, intrinsic and prelude type except `print`, `print_raw` and
  `VaList`, and every stand-in;
- as a receiver, the same 208;
- called, 15: the prelude types `List`, `Map`, `Pool`, `channel`, `Scope`, `Proc` and `range` (called
  without brackets), `min`, `zip`, and the six stand-ins.

The builtin types (`int`, `str`, …) are the contrast: a value use goes on to "module items in checked
execution", a call is E0406, a receiver "methods on builtin types".

**The corpus** (`corpus-scan-base-a57276f2.tsv`, 1,264 entries on the checked machine): exactly one
entry hits either refusal, `corpus/errors.lu` (the stub message, at `acquire()`). No entry hits the
error-row one. `wordcount.lu` and `grammar/closure_extent.lu` call `min` and `zip` but stop at an
earlier refusal.

### wolf-lang#1082: lld from PATH, unannounced

`lld_fuse_flag` (`wolf_driver/src/main.rs`) probes `ld.lld --version` once and adds `-fuse-ld=lld`
to the cc link when it answers. macOS never routes lld (Apple ld signs). `build()` prints a note only
when lld is **absent** ("ld.lld not found on PATH — linking with the system linker"); when it is
present the build says nothing, and neither `wolf build --help` nor `wolf.1` mentions it. The issue's
own witness (two Ubuntu images, one with LLD 18.1.3) shows the digests differ by that alone, and
`WOLF_LD=/usr/bin/cc` restores the system link. The Linux rows run on the Linux gauntlet host.

### wolf-lang#175: `wolf add` writes what its reader refuses

`render_dep` and `minimal_manifest` (`wolf_pkg/src/manifest.rs`) splice the value between quotes
as it is. The manifest is wolf syntax, read by wolf's lexer, so a backslash, a `"`, a `{` or a `}` in
a path or URL breaks it. The reader's own `unescape` knows `\n \t \r \0 \\ \"` and two escapes the
lexer refuses (`\{`, `\}`), and cooks neither `\xNN`, `\u{…}` nor `{{`/`}}`; it also unescapes a raw
string. Today `wolf add` re-reads the manifest it wrote, so it fails and rolls back:

| probe (the dependency's directory) | `wolf add util --path '../DIR'` |
|---|---|
| `dep\q` | E0101 "there is no `\q` escape in wolf", pointing into `wolf.pkg`; rolled back, exit 1 |
| `dep{1}` | E1501 "manifest strings never interpolate"; rolled back |
| `dep"q` | E0109 "this generalized literal never closes"; rolled back |
| `dep\x` | E0101 "a `\x` escape needs exactly two hex digits"; rolled back |

(The issue's August witness, a Windows `file://` URL, saw the write succeed and the next command
fail; at this base the failure moved into `wolf add` itself, and the manifest it cannot write is
the same.)

## 2. The rulings asked, with the lane's recommendations

- **R-s289-1: an undeclared capitalized name where no row takes it is E0301**, as both clauses
  already say. At the name, "nothing named `OPEN` is in scope", with notes that say:
  - a bare capitalized name that names nothing is read as an error tag, and where a tag is a value
    (a `return` or the tail of a function that can fail, a call argument, an annotated `let`);
  - whether the enclosing function can fail, with the declaration as a secondary span when it
    cannot;
  - both fixes: declare or import the constant, or, at a `return` or tail of a function that
    cannot fail, make it fallible (`-> !int`); inside a fallible function, `return Bad` raises it.
  - When a module the file imports defines a `pub` item of that name, the note offers
    `dialog.OPEN` as the edit (machine-applicable when exactly one does).
  No spec text changes; the registry's E0301 prose gains the paragraph. Not taken: E0604 for the
  raise positions. That code says an error leaves, and an undeclared name may be a constant.
- **R-s289-2: a tag the enclosing function spells in some error row, used in a function that
  cannot fail, is E0604** ("an error cannot leave a function with no error row"; its prose already
  names "an error-tag return"), with the same `! {bad}` fix `?` gets there.
- **R-s289-3: `[type.unit.discard]`'s decline keeps its words.** A tag the enclosing *fallible*
  function's row declares, in a unit position (`if c { bad }`), stays "unsupported" with the old
  construct: the spec records that decline in those words, and lupin discards it. It is the only
  use of the words left.
- **R-s289-4: a provisional stand-in with nothing behind it is a user's error, not a gap.**
  `acquire`, `release`, `build_config`, `build_batch`, `worker` and `sleeper` resolve so that the
  oldest corpus programs can define them.
  - (a) When the package has a module of that name (a root directory of `.lu` files, or a
    dependency alias) and the file does not import it, the resolver reports E0301 exactly as for
    `helper`, with the import note and `use worker`.
  - (b) Otherwise, at a use with no definition, the checker reports E0301 "nothing defines
    `worker`", with a note that names the stand-ins and both fixes (define it, or import a module).
  The ambient table, `wolf prelude` and `spec/prelude.json` do not change. `corpus/errors.lu` moves
  from unsupported to fail(E0301) at typecheck; its header (`phase: resolve`) still holds, so the
  file is not edited. lupin answers "unsupported" for these, which is no divergence.
- **R-s289-5: every E0301 for a name that is a root module or a dependency alias of the package
  carries the import note** ("`helper` is a module of this package, and this file does not import
  it") and a `use helper` suggestion, inserted after the file's last `use` (or at its top).
- **R-s289-6: the census answers by class**, so the stub construct is deleted:
  - the prelude types (`List`, `Map`, `Pool`, `Mutex`, `channel`, `Scope`, `Proc`, `range`) in value
    position are `type`-kinded values, as the builtin types already are (D29). A value use goes as
    `let f = int` goes, a call is E0406 as `int()` is (for the containers with a note naming
    `List[int]()`), and a receiver as `int.member()`.
  - the builtin functions (the host table, `eprint`, `eprint_raw`, `fence`, `va_*`) and the
    intrinsics used as values are declined by name: "`env_args`, a builtin function, used as a
    value (call it by name)". lupin runs these, so this is the compiler's limit, not the user's
    error, and "cannot compile this yet" is the honest frame.
  - `min` and `zip` in R-s289-7; the stand-ins in R-s289-4.
  A census row walks every row of the ambient table in the three uses and holds each class to its
  answer, so a name added later cannot fall back to a stub.
- **R-s289-7 (#708): `min` compiles on every tier, and `zip` is refused by name.**
  - `min(a: T, b: T) -> T` for one integer type `T`, in a new clause. It lowers to a compare and a
    select in WIR and on the checked machine.
  - `zip` is refused with lupin's reason: no clause gives the free function semantics, and std's
    list method `xs.zip(ys)` is the spelling. It is exempt from W0304, as lupin already exempts it,
    so a program may write its own.
  - Both touch `spec/` and the WIR lowering, so they are cross-cutting.
  - Interim, in the module PR: both are declined by name ("`min` has no signature yet: no clause
    types it").
  - Not taken: retiring both names. It would move `wordcount.lu`, the canonical program, off its
    phase.
- **R-s289-8 (#1082): when lld is picked from PATH, `wolf build` says so once, as a note,** beside the
  existing not-found note: "linking through ld.lld found on PATH (`/usr/bin/ld.lld`, `<its version
  line>`); `WOLF_LD=cc` links with the system linker instead". It prints only for a binary
  (`--emit` other than `bin` links nothing), and never on macOS or Windows. `wolf build --help` and
  `wolf.1` document the selection and the override. The choice itself is unchanged; the issue has no
  discussion that rules otherwise.
- **R-s289-9 (#175): one quoting function writes every manifest string, and the reader cooks what
  the lexer accepts.** The manifest is wolf syntax, not TOML, so "every escape TOML defines" is read
  as every character a TOML basic string must escape, plus wolf's two braces:
  - the C0 controls (`\b`, `\t`, `\n`, `\f`, `\r` among them), DEL, `"` and `\`, each written in
    wolf's spelling (`\n \t \r \0 \\ \"`, `\u{…}` for the rest);
  - `{` and `}` written `{{` and `}}`.
  The reader adds `\xNN`, `\u{…}` and the brace pairs, and leaves a raw string raw. The rows are
  the named edge values and a seeded fuzz of 2,000 values per field (path, git URL, tag, package
  name).
- **R-s289-10: the split.** Three wolf-lang PRs, each from the one before's head:
  1. **The two messages a reader of a tutorial hit first** (`module`): #1085 and #1083, which is
     R-s289-1 to R-s289-5.
  2. **The census, #1082 and #175** (`module`): R-s289-6, R-s289-7's interim, R-s289-8 and
     R-s289-9.
  3. **#708's `min`** (`cross-cutting`): R-s289-7 proper.
  lupin's mirror is a wolf-interp PR after the first: today lupin *runs* every #1085 probe, so the
  compiler's E0301 there is a divergence until lupin reports it too. After the third, lupin's
  variadic `min` narrows to two operands.

## 3. The rows, predicted

Each row is red at the base and green at its fix. The corpus rows go through `cargo xtask corpus`
(at the base, a `check: fail(…)` row the compiler declines is red by name: "pins a rejection the
compiler cannot make"). A new driver test, `crates/wolf_driver/tests/diagnostics_sweep_lanes.rs`,
runs each on the checked machine, native and release, and on the pinned lupin. It holds the
compiled tiers to the code, the span and the rendered notes, and lupin to its 0.1.60 answer, named,
until the mirror. Each new corpus file gets its parser snapshot.

### First PR

| row | content | base | head |
|---|---|---|---|
| `corpus/typecheck/tag_undeclared_tail.lu` | `e1` | unsupported (red) | E0301 at `OPEN`, the `-> !int` note, the declaration as secondary |
| `corpus/typecheck/tag_undeclared_assign.lu` | `e2` | unsupported | E0301 at `INPUT`; no `!T` note (not a raise position) |
| `corpus/typecheck/tag_undeclared_return.lu` | `e3` | unsupported | E0301 at `CANCEL`, the `-> !int` note |
| `corpus/typecheck/tag_undeclared_let.lu` | `e4` | unsupported | E0301 at `OPEN` |
| `corpus/typecheck/tag_undeclared_payload.lu` | `e5` | unsupported | E0301 at `Bad` |
| `corpus/typecheck/tag_undeclared_fallible_let.lu` | `e7` | unsupported | E0301 at `Bad`, the `return Bad` note |
| `corpus/typecheck/tag_undeclared_module_item/` | `use dialog`, then a bare `OPEN` that `dialog` defines | unsupported | E0301, `dialog.OPEN` machine-applicable |
| `corpus/typecheck/tag_known_cannot_fail.lu` | `e8` | unsupported | E0604 at `bad`, the `! {bad}` fix |
| `corpus/resolve/stand_in_unimported/` | `p1`, #1083's witness | unsupported | E0301 at `worker` at resolve, the import note, `use worker` |
| `corpus/resolve/module_unimported/` | `p2` | E0301, no note (record green; the test's note row red) | E0301 with the same note, `use helper` |
| `corpus/typecheck/stand_in_undefined_call.lu` | `p5` | unsupported | E0301 "nothing defines `worker`" |
| `corpus/typecheck/stand_in_undefined_receiver.lu` | `p4` | unsupported | the same, at the receiver |
| the test's `wolf test` row | `e12` | `1 unsupported` | `1 rejected`, E0301 rendered |

Controls in the test, green at both: `e6` (E0604 on `?`), `e9` (the recorded decline, unchanged
words), `e10` (exit 7), `e11` (the variant's E0301).

### Second PR

- **The census row**, `crates/wolf_driver/tests/prelude_census.rs`: every ambient name in three uses
  on the checked machine, from `wolf prelude --json`. At the base, 431 uses answer the stub (red by
  name, listed); at the head none does, and each class answers as R-s289-6 says.
- **#1082:**
  - a unit row for the note's text;
  - on Linux, a build with an `ld.lld` on PATH must print the note once, and one with
    `WOLF_LD=cc` must not. Red at the base: no note. macOS and Windows skip that row by name;
  - a help row: `wolf build --help` names lld and `WOLF_LD`.
- **#175:**
  - `wolf_pkg` unit rows: the named edge values (each TOML escape character, both braces, `"`,
    `\`, a Windows `file://` URL, non-ASCII), then the seeded fuzz, through `insert_dep` and
    `minimal_manifest` and back through `parse`. Red at the base;
  - the reader's rows (`{{`, `\u{…}`, `\x41`, a raw string);
  - a driver row: `wolf add` with a dependency directory named `dep\q`, `dep{1}` and `dep"q`, made
    at run time (a Windows checkout cannot hold those names), then `wolf build` and the program's
    output. Unix only, the skip named on Windows.

### Third PR (if ruled)

- `min` rows on every tier: `int`, `u8`, a negative pair, one argument and three (E0402), mixed
  types (E0401), a `str` pair (refused by the clause's type rule).
- The `zip` refusal by name. The W0304 exemption (`spec/prelude.json` moves one flag).
- The census row moves `min` to "typed".

## 4. What could go wrong

- A Linux test that asserts an empty stderr from `wolf build` with lld installed would see the new
  note. None reads the absent-lld note today. The gauntlet host says, and any such row filters the
  note by its prefix.
- The tag's raise position comes from the expectation's reason (`ReturnOfFn`) at the exact
  expression. A parenthesized `(OPEN)` gets the generic note, not the `-> !int` one.
- A closure body has its own return. Inside a closure the note names no function.
