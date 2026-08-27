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

This chapter deliberately does **not** write the full numeric tower
(mixed integer-width arithmetic, a complete `Add`/`Mul` trait hierarchy
beyond what literal adoption needs) nor the general narrowing integer
cast's range-check question (s27's, still open — the wrapping row above
is the value-preservation answer only for leaving the wrapping domain) —
D54 is the literal story, D56 the wrapping escape, and the cast's numeric
directions, no more.
