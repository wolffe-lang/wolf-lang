# Wolf Language Specification — 10: Types

Status: normative, v1 (campaign c27, sprint s113). Anchors `[type.*]`.
The numeric-literal typing rules this chapter writes down were, before
D54, scattered across s13 (the unification-with-levels engine), D49 (the
operator bridge), and s17 (the closed coercion set); D54 unified them
and this chapter is their single home. Implemented by `wolf_sema`
(literal kinds are variable constraints in `unify`, the cast lowers in
`wolf_wir`). Evidence: `.docs/planning/02-decisions.md` D54, D49; the
`corpus/typecheck/numlit_*` and `corpus/faults/cast_*` litmus files are
the checkable consequences.

The claim this chapter buys: an integer literal reads as the number it
denotes in the float context it is written into — `0` is `0.0` when
`0.0` is expected — while a concrete *value* never silently changes its
type. The `.0`-on-everything tax and the `let a: int = 3` workaround
both retire; the C usual-arithmetic-conversions footgun does not follow.

---

## §1 Numeric literals `[type.numlit]`

A numeric literal is untyped at birth: it carries a **kind**, not a
type, and acquires its type from the context it is written into or, if
no context decides, from a defaulting rule. The four rules below are one
system.

- `[type.numlit.kind]` Every numeric literal has one of two kinds:
  `{integer}` (a digit sequence, `0x…` hex, `_` separators — no dot, no
  exponent) or `{float}` (a literal with a fraction dot or an exponent).
  The kinds are **distinct** and neither is a type: `{integer}` and
  `{float}` name constraints a real type must satisfy. A literal's kind
  is fixed by its spelling and never changes; what a literal *adopts* is
  a type (§`[type.numlit.adopt]`), never a kind.

- `[type.numlit.adopt]` **An integer literal satisfies a float
  expectation.** In a position whose expected type is a float
  (`f32`/`f64`), an `{integer}`-kind literal resolves as that float:
  `let x: f64 = 0`, `c <= 200` with `c: f64`, and a bare `0` argument to
  an `f64` parameter are all legal, and mean `0.0`, `200.0`, `0.0`. This
  is **sound and lossless** because a literal carries no computation
  history — `0` denotes the exact value `0`, representable in the float
  with no rounding — and it is **one-directional**: a `{float}`-kind
  literal never satisfies an integer expectation (`let n: int = 0.0` is
  refused, E0401), because `0.0` is not an integer denotation. Adoption
  is a property of **literals only**; a concrete value never adopts
  (§`[type.numlit.value]`).

- `[type.numlit.propagate]` **Adoption propagates through an arithmetic
  or comparison term.** The operators `+ - * / %` and `< <= > >= == !=`
  bridge to their arithmetic/ordering meaning over the numeric
  primitives (the D49 lineage extended from comparison to arithmetic by
  D54; same static-dispatch-over-visible-primitive-impls temperament, no
  erasure, no implicit value coercion). Because the operands of a term
  share one type through the checker's unification, a float expectation
  reaching **any** point of the term pins the **whole** term: in
  `c * 1.8 + 32` with `c: f64`, the `32` (and, absent an `f64` `c`, the
  `1.8` and every other literal in the term) resolves as `f64`, and the
  arithmetic is float arithmetic. The **reach** is exactly the term
  connected by these operators and the expectation flowing into its
  root: adoption does not cross a binding (`let n = 0; …` fixes `n`'s
  type at the binding, §`[type.numlit.value]`), and into a call it
  reaches only the declared parameter type. The soundness consequence,
  pinned: `let x: f64 = 1 / 2` is `0.5` — both literals adopt `f64`
  *before* `/` runs, so it is float division — whereas `let n: int =
  1 / 2` is `0`, integer division, the two literals having adopted
  `int`. The operator chooses int-vs-float by its operands' resolved
  type, never by the literals' spelling.

- `[type.numlit.default]` **A literal with no reachable numeric context
  defaults by kind.** When no expectation decides a literal's type, an
  `{integer}` literal resolves as `i32` and a `{float}` literal as
  `f64` (the s13 rule), applied once at the end of the enclosing body,
  not as a solver step. Defaulting is a *rule*, never a guess: a literal
  whose kind itself cannot be determined is an inference error (E0405),
  and a literal reached by two **incompatible** concrete expectations
  (an `f64` from one side and an `i32` from another) is the ordinary
  type mismatch (E0401, `[type.numlit.ambig]`) — the checker names the
  conflict and the spelled `as` fix, it never picks one side.

- `[type.numlit.ambig]` Ambiguity is a **named error, never a guess.**
  If a single literal is required to be two incompatible concrete types
  at once, the checker reports E0401 at the literal, naming both
  requirements and their origins, with the `as`-conversion fix — the
  same diagnostic that reports any type mismatch. No literal is ever
  resolved by choosing arbitrarily between live expectations.

## §2 Values are spelled `[type.numlit.value]`

A concrete numeric **value** never implicitly changes its type. `let n =
0` fixes `n` as `int` (by §`[type.numlit.default]`); `let x: f64 = n` is
**refused** (E0401) — the `.0` of adoption is a literal's privilege, not
a value's. This is the X3 safety posture and the closed-coercion-set
discipline of the memory model (`[mem.dyn.unsize]`'s "the coercion table
grows by addition, never by a new implicit mechanism"): C's
usual-arithmetic-conversions and Swift's exponential-search overloading
are both declined. A value's conversion is the spelled `as` cast.

## §3 The numeric cast `[type.numlit.cast]`

`as` between numeric types is the **only** spelling of a value
conversion, and its numeric arms are closed and total:

- `[type.numlit.cast.widen]` **`int as float` is free.** An integer
  value widens to a float (`n as f64`, `n as f32`) with the platform
  round-to-nearest for the rare wide-integer case; there is no semantic
  question and no trap. `int as int` widening is the sign/zero-extend of
  the memory model (source signedness decides), unchanged.

- `[type.numlit.cast.trunc]` **`float as int` truncates toward zero and
  traps if the result does not fit.** `3.7 as int` is `3`, `-3.7 as int`
  is `-3` (the `fptosi`/`fptoui` default, C-familiar truncation). If the
  truncated value does not fit the target integer — overflow, or a NaN
  input, which fits no integer — the cast **traps** (`trap(overflow)`,
  `[conf.trap.set]`), joining the checked-arithmetic trap family: it is
  the `255 + 1`-on-a-`u8` event, not C's undefined behavior and not a
  silent saturating clamp. Rounding other than truncation is a spelled
  `std.math` call (`x.round() as int`); saturation, if ever wanted, is a
  named `std` function — the cast itself never guesses. Target
  signedness (`fptosi` vs `fptoui`) follows the target type.

- `[type.numlit.cast.wrap]` **`wrapping[T] as int` is value-preserving
  and traps out of range** (D56). Leaving the wrapping domain is a
  conversion of the held VALUE, not a reinterpretation of its bits: a
  `wrapping[u64]` holding `2^63` or more does not fit `i64`, so the cast
  **traps** (`trap(overflow)`, `[conf.trap.set]`), joining the same
  checked-arithmetic trap family as the float row — never the silent
  bit-cast to a negative number that the earlier lowering emitted (the
  "cast that lies" D54 forbids). An in-range value converts unchanged;
  the widening unsigned direction (`wrapping[u32] as int`) zero-extends
  and never traps. A bit-reinterpretation, if ever wanted, is a distinct
  explicit unsafe-tier operation, never this `as` cast's silent default —
  the same posture as saturation on the float row.

## §4 The `char` type `[type.char]`

- `[type.char]` **`char` is a Unicode scalar value** (D58, s121): its
  domain is `0..=0x10FFFF` **excluding** the surrogate gap
  `0xD800..=0xDFFF` — exactly the set of values UTF-8 can encode. Not a
  byte (`'é'` is one `char`, two bytes — the byte tier stays
  `bytes()`), not a grapheme (an accented letter typed as base +
  combining accent is two `char`s even when it renders as one glyph),
  not a UTF-16 code unit (no wolf value holds a surrogate, ever — the
  same D24 invariant that makes every `str` valid UTF-8). **Layout: 4
  bytes, alignment 4** — an `i32`-shaped scalar at every tier
  (`List[char]` strides by 4; the value crosses the runtime's C seam
  widened to `i64` like every sub-word scalar). The representation
  invariant — every `char` value IS a scalar — means the i32's sign
  bit is never set, so signed machine compares are scalar-value order.
  `char` is **not** an integer type: no arithmetic, no numeric-literal
  adoption (`let c: char = 65` is a type error; write `65 as char`),
  no indexing with one. The only numeric bridges are the two casts of
  `[type.char.cast]`.

- `[type.char.lit]` A `char` literal is one scalar between single
  quotes — `'a'`, `'é'`, `'🐺'` — with the string escape set plus
  `\'`: `\n \t \r \\ \' \" \0 \xNN \u{1–6 hex}` (grammar:
  `[gram.lex.char]`). A `\x`/`\u` escape naming a non-scalar (the
  surrogate gap, or above `0x10FFFF`) is refused at the literal
  (E0110): the value a `char` cannot hold is the value its literal
  cannot spell — the same domain the trapping cast enforces at run
  time. Distinct spellings of one scalar are one value: `'\n'` equals
  `'\u{A}'`.

- `[type.char.order]` **`char` orders by scalar value.** Equality and
  the comparisons (`== != < <= > >=`) are total and locale-free —
  scalar order, not collation and not glyph order (`'z' < 'é'` because
  `0x7A < 0xE9`). The same temperament as `[mem.str.order]`'s byte
  order: cheap, deterministic, honest about not being a collator.

- `[type.char.cast]` **`char as int` is total; `int as char` traps on
  a non-scalar.** Every scalar fits an `int`, so the outbound cast has
  no failure case (`'a' as int` is `97`). The inbound cast is D56's
  trapping family (`trap(overflow)`, `[conf.trap.set]`): a value that
  is **negative**, **above `0x10FFFF`**, or **inside the surrogate gap
  `0xD800..=0xDFFF`** names no character, and admitting it would mint
  a `char` that cannot be UTF-8-encoded — un-writable into any `str`
  without breaking D24. The gap edges are legal: `0xD7FF as char` and
  `0xE000 as char` both convert. Other widths cast through `int`; a
  checkable conversion (a `from_int -> char ! {domain}` shape) is
  std's to name over this primitive, never a second cast semantics.

- `[type.char.interp]` **`{c}` prints the character**, never the
  code-point number: the hole renders as the scalar's UTF-8 encoding,
  and a format spec on a char hole takes the `str` spec surface
  (fill/align/width — width in bytes, D25); numeric specs (`{c:x}`)
  are the E0413 mismatch they look like. The number is spelled, not
  ambient: `{c as int}`.

## §4b The `byte` type `[type.byte]`

- `[type.byte]` **`byte` is an 8-bit unsigned scalar** (D72, s135): its
  domain is `0..=255`, exactly the values one octet holds — what comes
  off a socket, a file, or a `str`'s UTF-8 encoding, one value per
  byte. **Layout: 1 byte, alignment 1** — an `i8`-shaped storage cell
  at every tier (`List[byte]` **strides by 1**, so a byte buffer
  charges one ledger byte per payload byte under
  `[mem.region.account.1]` — the property wolf-lang#203 measured the
  absence of; a `byte` struct field is one byte at its layout offset;
  the value crosses the runtime's C seam zero-extended to `i64` like
  every sub-word scalar). Every `byte` value IS an octet, so loads
  zero-extend and machine compares are unsigned compares. `byte` is
  **not an integer type** — the posture is `char`'s (`[type.char]`),
  so there is one rule for width-bearing scalars rather than two: no
  numeric-literal adoption (`let b: byte = 65` is the type mismatch
  E0401, whose note names the fix; write `65 as byte` — in every
  position, a `match` arm included: a `byte` scrutinee binds or
  wildcards, and a literal arm is spelled over `b as int`), no closed
  arithmetic (a `byte` never holds the result of `+`,
  `[type.byte.op]`), no indexing with one, and no literal suffix
  (there is no suffix inventory; `65 as byte` is the spelling, and
  the name is `byte`, not `u8` — an alias arrives only if an inventory
  ever does). `byte` is a builtin type NAME resolved in type position
  like `int` and `char`, not a keyword: `[gram.inv.kw]`'s closed set
  stays at 50.

- `[type.byte.cast]` **`byte as int` widens; `int as byte` truncates.**
  The outbound cast is total — every octet fits an `int`
  (`200 as byte as int` is `200`) — and is a zero-extension, never a
  sign-extension, because the domain has no negatives. The inbound
  cast is the **only** narrowing `as` in the language that never
  traps: it keeps the value's low eight bits and discards the rest, so
  `255 as byte` is `255`, `256 as byte` is `0`, `300 as byte` is `44`,
  and `-1 as byte` is `255` — the low-bits meaning `wrapping[u8]`'s
  narrowing already has, ruled for `byte` because a byte type exists
  to hold the low octet of whatever arithmetic produced it (a mask, a
  shift, a checksum step), and a trap there would make every such site
  an `as wrapping[u8] as …` dance. The boundary values `0`, `255`,
  `256`, `-1` are the witness. This is a ruled answer for ONE target,
  not the general narrowing cast's range-check question (s27's, still
  open for `i32`/`u16`/…). Other widths cast through `int`
  (`x as int as byte`); `byte` and `char` bridge through `int` too
  (`b as int as char`); there is no `byte as f64`. W0401 (a literal
  outside the target's range) does not fire on `as byte`: truncation
  is the clause, not an accident.

- `[type.byte.op]` **Every arithmetic and bitwise operator widens a
  `byte` operand to `int` first, and the result is `int`.** `b + 1`,
  `b * 2`, `b & 0x0F`, `b >> 4`, `b1 + b2` and `-b` are `int`-typed:
  the operands are read through `[type.byte.cast]`'s widening and the
  operation is `int`'s — X3 checked arithmetic, so `b1 + b2` cannot
  overflow and `b - 1` on a zero byte is `-1`, an `int`, neither a
  trap nor a wrap. Narrowing the result back is spelled:
  `(b + 1) as byte`. A `{integer}` literal beside a byte operand
  adopts `int`; a mixed term `b + n` with `n: int` is legal and `int`.
  The consequence is that `byte` has no compound assignment — `b += 1`
  is the E0401 an `int` assigned to a `byte` is, because `b + 1` is an
  `int`. **Comparisons are total and closed**: `== != < <= > >=`
  between two `byte`s compare octet values (unsigned order — `200 as
  byte > 100 as byte`); `<=>` yields `int`; `byte` against `int` is
  the ordinary type mismatch — widen the byte.

- `[type.byte.interp]` **`{b}` prints the number** — the decimal octet
  value, `0` through `255`, never a character: a byte is a quantity,
  and the character it might encode is `str`'s business
  (`[mem.str.chars]`). A format spec on a byte hole takes the integer
  spec surface (`{b:x}` is `ff` at most), the same surface `int` has,
  because the hole widens the byte to `int` before formatting.

## §4c Interpolation of every value `[type.interp]`

(Appended 2026-09-09, s143 — wolf-lang#268. `{x}` was ruled per
primitive — `[type.char.interp]`, `[type.byte.interp]`, the s38 float
rendering — and for nothing else: the reference interpreter rendered
every value it holds, the compiler refused every hole that was not a
primitive, and the book's largest one-machine family (10 samples at
wolf 0.2.8) was exactly that gap. These clauses adopt the
interpreter's rendering, byte for byte, as the language's — it was
the only rendering anyone had written down, in code — and name the
values that have none.)

- `[type.interp.value]` A hole renders its value by the value's type,
  recursively, and **the same bytes whether the hole is printed or
  built into a `str`** (`"{x}"` in value position materializes exactly
  what `print("{x}")` writes). A `str` inside a composite renders as
  its bytes, unquoted — `Doc { title: regions, words: 900 }`, not
  `"regions"`; interpolation is for reading, not for round-tripping.
  A format spec (`{x:>8}`) is defined on the primitive holes
  (`[type.char.interp]`'s `str` surface, the integer surface, the
  float surface) and on nothing composite: a spec on a composite is a
  refusal, never silently ignored (wolf-lang#10's rule).
- `[type.interp.agg]` **`()` renders `()`**; a tuple `(a, b)` — the
  elements in order, `, `-separated, in parentheses; a struct
  `Name { f: v, g: w }` — the type's name, a space, `{`, then each
  field as ` f: v` with `,` between, then ` }` (a zero-field struct is
  `Name { }`); a `List` `[a, b]` — the elements in order,
  `, `-separated, `[]` when empty. An enum value renders its variant
  **qualified**, `Enum.Variant`, with the payload as `(p1, p2)` when
  the variant carries one — the spelling the program constructs it
  by. Weighed and rejected: the bare variant name — `Rgb(1, 2, 3)`
  reads as a row tag, and rows and variants are different things
  (D30 rows are structural; a variant belongs to its enum).
- `[type.interp.row]` A caught row value (`else |err|`, a `match`
  binding) renders as **the tag's name**, then `(p1, p2)` when the
  tag carries a payload, each payload rendered by its own rule:
  `too_short`, `BadDigit(q, 1)`, `closed`. The name is the tag as the
  program spells it, module-qualified by nothing — tags are
  structural (D30), so there is nothing to qualify by.
- `[type.interp.union]` A `!T` value renders as **its ok payload when
  it holds one, and as its row (`[type.interp.row]`) when it does
  not** — the value's two halves are two values, and the hole prints
  whichever is there. `{popped}` after `let popped = xs.pop()` reads
  `3` or `none`. This is a reading rule, not a handling rule: `?` and
  `else` still decide what the program does with the row.
- `[type.interp.reason]` An exit reason (`[conc.proc.exit]`) renders
  as its class name with its payload in parentheses: **`normal(v)`**
  with the proc's `int` result (the value the body returned — a body
  whose result is not an `int` reports `normal(0)`), **`error(Tag)`**
  with the row that crossed the boundary rendered per
  `[type.interp.row]`, **`killed`**, **`cancelled`**, and
  **`fault(kind)`** with the trap kind from `[conf.trap.set]`'s
  vocabulary (`fault(bounds)`).
- `[type.interp.none]` Some values have **no rendering the language
  promises**: a channel, a proc, a task scope, a region value, a raw
  pointer, a fn value, a `dyn` object, a shared-tier handle. An
  implementation may print its own bookkeeping for them (the
  interpreter prints `channel#3`, `region#1@2`) or refuse the hole by
  name (the compiler does); a program must not depend on either, and
  a conformance witness may not interpolate one. Weighed and
  rejected: ruling the interpreter's bookkeeping as the rendering —
  it names ids the compiled program does not have.

## §5 `str` concatenation `[type.str]`

- `[type.str.concat]` **`+` and `+=` on two `str`s are
  interpolation-append** (D62, ruled from live dogfooding — the
  interpreter's behavior is the language): `s + u` is legal exactly
  when BOTH operands are `str`, and means precisely `"{s}{u}"` — a
  new `str` whose bytes are the operands' bytes in order (UTF-8
  concatenation is closed; no boundary can be violated). `+` on a
  `str` and a `char`, either order, is the same operation: `s + c`
  is `"{s}{c}"` and `c + s` is `"{c}{s}"` — the `char` contributes
  its scalar's UTF-8 bytes (a `char` is text, D58, with exactly one
  rendering; wolf-lang#278). `+` chains left-associatively; `s += u`
  and `s += c` are `s = s + u` and `s = s + c`, in every place shape
  an assignment admits. This is a builtin operator on the builtin
  type, like `==` on `str` — NOT a trait bridge (no `Add` trait
  opens; D49's bridge shape is untouched).

- `[type.str.concat.mix]` **Mixed operands stay E0409** — `str + int`,
  `int + str`, and their `+=` forms. The `int` rows stay because `+`
  is not a formatter: an integer has more than one rendering (sign,
  radix, width — `{n}`, `{n:x}`, `{n:>4}`) and a bare `+` would pick
  one silently. A `char` is not a mix (`[type.str.concat]`): it is
  text with one rendering. The `int` conversion is spelled where it
  always was: inside an interpolation hole (`t += "{count}"`).
  Interpolation remains the general surface and is unchanged; `+` is
  its `str`-and-text special case, not a replacement.

- `[type.str.concat.cost]` **The cost model is interpolation's**: a
  fresh `str` per application — the compiler lowers `+` onto the same
  strbuf path an interpolated string materializes through, so `+=` in
  a loop is quadratic, never an amortized push. `std.strbuf` is the
  builder. The diagnostics say so beside the refusal note.

## §6 Closures `[type.closure]`

(Appended 2026-09-09, s145 — wolf-lang#268's `return` inside a closure
family: seven of the book's samples, chapter 16's receivers among
them, spell `let r = ch.recv() else |_| { return }` inside a spawned
closure. Every machine that ran them agreed on what the `return`
means; only the compiler's typing withheld it.)

- `[type.closure.return]` **`return` inside a closure returns from the
  closure.** Its operand types against the closure's own result — the
  result the context fixes for a closure checked against a fn type,
  the one inferred from the body's tail otherwise (a `return` and the
  tail meet at one type), the declared return type on a nested `fn` —
  and a bare `return` is the unit result. The enclosing function is
  out of reach: no `return` in a closure body leaves the function the
  closure was written in, on any tier, and the closure's own `defer`s
  run on the way out exactly as a function's do. `?` follows the same
  frame (its row is the closure's, s73). This is the meaning every
  reader assumed and every machine already ran; it is written down so
  the typing can admit it.

## §7 The unit context `[type.unit]`

(Appended 2026-09-10, s146 — wolf-lang#275: `chan.send` was typed `()`
on the compiler while `[conc.chan.close]` says a send after close
returns an error value. Typing it `() ! {closed, cancelled}` turned
every `for i in … { ch.send(i) }` and every else-less
`if ready { ch.send(1) }` into a type mismatch — s144 counted 13
corpus witnesses, 26 sites, and chapter 12 on every page — because no
clause said what a fallible unit-typed value *means* where `()` is
expected. This section is that clause; `send`'s row follows it in
`[conc.chan.close]`.)

- `[type.unit.context]` **A unit context is a position whose expected
  type is `()`.** The closed list: the body of a `for`, `while`, or
  `loop`; the then-block of an `if` with no `else`, and every block of
  an `if … else if …` chain that ends without one; the body of a
  function or method whose result is `()`, declared or omitted, and
  the operand of a `return` in one; the body of a closure checked
  against a `fn(…) -> ()` type; and any block, `match` arm, or `else`
  branch checked against one of those. The tail of a block in a unit
  context is consumed by no one: the block's value is `()` whatever the
  tail's type.

- `[type.unit.discard]` **A `!()` tail in a unit context is a discard,
  warned (W0601), never a mismatch.** The value's row is lost, and the
  compiler says so at the tail with the diagnostic a non-trailing `!T`
  statement has drawn since s67. Only `!()` qualifies: a `!int` tail
  where `()` is expected is a mismatch on the `int`, exactly as a plain
  `int` tail is. The two readings, costed before choosing:

  | | (i) handled — a `!()` tail is a mismatch | (ii) discarded — W0601 |
  |---|---|---|
  | what the reader learns | a `!()` statement is warned and the same statement one line lower, at the block's tail, is refused: *position* decides | one rule — a `!T` nobody consumes is a warned discard, wherever it sits |
  | the shortest fire-and-forget send | `ch.send(v) else {}` — the same silent discard, spelled longer and warned nowhere | `ch.send(v)`, with the warning naming what it drops |
  | a `closed` send the program never looks at | refused at a tail; compiles, warned, one line earlier | compiles, warned, at every site; refused under `--deny-warnings` |
  | programs written before the clause | every statement-tail send is an error: the 13 witnesses, chapter 12 | none changes meaning (`[diag.sev.warn]`); a warning-clean corpus or book spells the handling or declares the warning |
  | `send`'s type fix (#275) | breaking | non-breaking |

  (ii) is the rule. Its cost is real — the row *can* be lost — and it
  is the cost W0601 already carries for every other fallible call in
  the language; (i)'s price is a rule about position, which the
  language has no other instance of and just retired one of (#276).
  The warning cites its hazard by name (`[diag.sev.warn]`): the send
  after close that `[conc.chan.close]` was written so a sender could
  see, unseen. The three spellings that see it are the ones the
  warning already names — propagate with `?`, handle with `else`, or
  bind it away — and the corpus witnesses spell the first.

- `[type.unit.consume]` **A closure body with no fixed result is not a
  unit context.** `s.spawn(fn() { ch.send(v) })` infers `fn() -> !()`
  from its tail as `[type.closure.return]` says, and the scope consumes
  the value: a task completing with an error value re-raises at the
  scope exit (`[conc.task.fail]`). That is the row reaching the one
  reader who can act on it, not a discard, and it draws no warning.
  Inside such a closure a loop body is a unit context as everywhere
  else: `s.spawn(fn() { for i in 1..=n { ch.send(i) } })` warns at the
  send, and `ch.send(i)?` is the spelling that hands the loop's
  failure to the scope.

## §8 Functions `[type.fn]`

(Appended 2026-09-10, s148 — wolf-lang#284. Both machines check a
body's tail against the declared return type and refuse a `()` tail
under `-> str` or `-> !int`, and no clause said so directly: the rule
was read out of `[gram.expr.block]` — a block's value is its trailing
expression — together with `[gram.expr.tagident]`, a clause about tag
resolution that names "a fallible function's tail" only inside its
list of checked positions. wolf-interp#73 found the interpreter
reading neither: it ran the body and exited with a status the program
never wrote, and wollf wl10 measured the gap at 56 of 2,153 generated
programs. The rule is written here so a witness can cite a clause that
is about it.)

- `[type.fn.ret]` **A body is checked against its declared result.**
  The body block's value (`[gram.expr.block]`: its trailing expression,
  `()` when the block ends in a statement) is checked against the
  function's declared return type, and so is the operand of every
  `return`. For a fallible function — `-> T ! row` or `-> !T` — the
  tail is checked against the ok half `T`: a bare tag resolves to the
  declared row first (`[gram.expr.tagident]`), a nested row flattens
  into it (`[gram.type.row.flatten]`), and a plain `T` coerces to the
  union. `()` is not `T`: a body under `-> str` or `-> !int` that ends
  in a `while`, a `print`, or any other statement is **E0401 at the
  tail**, naming the declaration as the origin — never a `()` printed
  and never an exit status a runtime invents (wolf-interp#69's shape
  one level up). The omitted return type is `()`, and the one place a
  fallible tail is not a mismatch is the unit context: a `!()` tail
  where `()` is expected is a warned discard, `[type.unit.discard]`.
  Witnesses: `typecheck/tail_declared_str.lu`,
  `typecheck/tail_declared_union.lu`.

## §9 The error row as a value `[type.row]`

(Appended 2026-09-10, s148 — wolf-lang#284, wolf-interp#81. A bare
`!T` in operator position was refused by both machines, and the rule
was stated nowhere: it was inferred by inverting `[type.interp.union]`'s
carve-out — an interpolation hole may *read* a `!T` without handling
it, so a hole is the one place that happens, so an operator has no
reading at all. That works, and it is a clause about string holes
carrying a rule about operators. The rule is written here, beside
`[type.str.concat.mix]`, which fixes its code.)

- `[type.row.operand]` **A `!T` is two values, never one, and no
  operator reads it.** `?`, `else` and a `match` are the whole of how
  a row is handled; `[type.interp.union]`'s rendering is a reading,
  not a handling; no cast and no coercion narrows a `!T` to its `T`.
  An operator — arithmetic, comparison, logic, bitwise, shift — applied
  to a `!T` operand is **E0409**, the code `[type.str.concat.mix]`
  spends on "this operator is not defined on these operands", and it
  is E0409 **on either side of the operator**: `n + 1` and `1 + n`,
  `n <= 5` and `5 <= n` are one refusal, because the fact refused is
  the same one — no operator has a `!T` in its family. The fix is to
  handle the row first (`n? + 1`, `(n else 0) + 1`). Weighed and
  rejected: reporting the row as a mismatch (E0401) against the
  operand that fixed the operator's type. That sentence says the row
  *could* have been in the family had it been the other type, which it
  could not; and a code that depends on which side the row sits on is
  a rule about position, of which the language has no other instance
  (`[type.unit.discard]` retired one at #276). Status at s148: the
  compiler answers E0409 with the row on the left and E0401 with it on
  the right; lupin 0.1.31 answers E0409 for arithmetic and E0401 for a
  comparison, on either side — the two follow-ups are filed against
  this clause. Witnesses: `rows/negative/row_operand_add.lu`,
  `rows/negative/row_operand_compare.lu`.

This chapter deliberately does **not** write the full numeric tower
(mixed integer-width arithmetic, a complete `Add`/`Mul` trait hierarchy
beyond what literal adoption needs) nor the general narrowing integer
cast's range-check question (s27's, still open — the wrapping row above
is the value-preservation answer only for leaving the wrapping domain,
and `[type.byte.cast]`'s truncation is the answer for ONE target, the
octet) —
D54 is the literal story, D56 the wrapping escape, and the cast's numeric
directions, no more. `char`'s method surface (classification,
case-mapping, the checkable conversion) is std's tier over the
primitive (D58), not this chapter's; so is `byte`'s (the `bytes`
library over `List[byte]` — D72, wolf-std sc35; the language's own
byte producers and consumers — `str.bytes()`, `str_from_utf8`, the
`fs_*`/`net_*` byte calls — speak `List[byte]` since s136, #231).
