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

## §5 `str` concatenation `[type.str]`

- `[type.str.concat]` **`+` and `+=` on two `str`s are
  interpolation-append** (D62, ruled from live dogfooding — the
  interpreter's behavior is the language): `s + u` is legal exactly
  when BOTH operands are `str`, and means precisely `"{s}{u}"` — a
  new `str` whose bytes are the operands' bytes in order (UTF-8
  concatenation is closed; no boundary can be violated). `+` chains
  left-associatively; `s += u` is `s = s + u`, in every place shape
  an assignment admits. This is a builtin operator on the builtin
  type, like `==` on `str` — NOT a trait bridge (no `Add` trait
  opens; D49's bridge shape is untouched).

- `[type.str.concat.mix]` **Mixed operands stay E0409** — `str + int`,
  `str + char`, `int + str`, and their `+=` forms. The conversion is
  spelled where it always was: inside an interpolation hole
  (`t += "{count}"`). Interpolation remains the general surface and
  is unchanged; `+` is its two-`str` special case, not a replacement.

- `[type.str.concat.cost]` **The cost model is interpolation's**: a
  fresh `str` per application — the compiler lowers `+` onto the same
  strbuf path an interpolated string materializes through, so `+=` in
  a loop is quadratic, never an amortized push. `std.strbuf` is the
  builder. The diagnostics say so beside the refusal note.

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
library over `List[byte]`, and the migration of every byte-producing
surface from `List[int]` — D72, wolf-std sc34).
