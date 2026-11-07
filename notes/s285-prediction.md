# s285 prediction: a literal pattern outside its scrutinee's type

Committed before the first change. Base: wolf-lang trunk `a4d2a9c9`. The base was measured first
(`notes/s285-evidence/base-a4d2a9c9.log`, the probes under `notes/s285-evidence/probe/`), on
the checked machine, native and release (`wolf conform-run --json`) and lupin 0.1.59.

## What the base does

- wolf-lang#1058 reproduces: `match v { 300 => 0, _ => 2 }` over a `u8` holding 44 exits 0 on
  native and release (the arm constant is wrapped into the scrutinee's width, 300 mod 256 = 44)
  and 2 on the checked machine and lupin. No tier says anything.
- The same for every narrow primitive at its max+1 called with the value it wraps to (`i8`,
  `i16`, `i32`, `u8`, `u16`, `u32`), for `wrapping[u8]` and `wrapping[i8]`, in every position a
  literal pattern can stand (a tuple element, an enum payload, a struct field, an or-pattern's
  alternative, after `@`, a statement `match`), and in hex and `_` spellings.
- The 64-bit primitives at max+1 (`i64`, `int`, `u64`, `uint`) are refused on native and release
  as "unsupported — this literal pattern shape"; the checked machine and lupin never match.
- Range endpoints diverge the same way: `250..=300` over a `u8` holding 255 matches on the checked
  machine and lupin and not on native (whose high end became 44); `100..=200` over an `i8` the
  same. `0..256` over a `u8` agrees on all four (the exclusive end is normalized before it wraps).
- The expression twins are refused already: `let v: u8 = 300` and `v == 300` are E0415 on all
  three compiled tiers.
- A literal scrutinee: `match 5 { 3000000000 => … }` exits 2 on all four, with no diagnostic.
- **A second wrong answer, in range:** a `u64` or `uint` pattern above `i64::MAX`
  (`18446744073709551615`, `0xffffffffffffffff`, `9223372036854775808`) never matches on the
  checked machine (exit 2), which reads a pattern's text as an `i64` only. Native refuses the
  decimal spellings ("unsupported") and matches the hex one; lupin matches all of them.
- No literal pattern carries a sign in the grammar (`literal ::= INT | …`, and `INT` has no sign).
  `-1 =>` is E0207 "expected a pattern to start the match arm" on the compiled tiers. lupin
  accepts it and matches it (`match v { -1 => 0 }` over an `i8` holding -1 exits 0).

## The corpus measurement, before any code

`notes/s285-evidence/census-litpat.txt`: every integer literal written in an arm's pattern whose
value is at least 128, across the corpus and 31 public checkouts (the org's wolf repositories at
their trunks, wolf-std at the `STD-PIN` and at its trunk, wolf-interp and yew), 7,899 `.lu` files;
one private downstream consumer was measured too (34 files, no integer literal pattern at all). A
pattern outside its type must be at least 128: the grammar has no signed literal, and 128 is the
smallest max+1 (`i8`). The census finds 470 candidates; none is above 65,535, so each could be out
of range only against an `i8`, a `u8` or an `i16`. Each was read with its `match`
(`census-scrutinee.txt`): every scrutinee is an `int`, a `u64`, a cast to `int`, a `for` counter
over `0..154`, or a `u8` holding 255 (in range); the wolf-book's `Line` hits are guards
(`c > 200`), not patterns. **No program in any tree writes a literal pattern outside its
scrutinee's type, on purpose or not**, and none writes a signed literal pattern or an exclusive
range end at a type's max+1.

## The rulings, recommended

- **R-s285-1, the rule: (A), E0415 on every tier, at the literal.** A literal pattern takes its
  scrutinee's type, and one whose value that type cannot hold is a compile error. E0415 is the
  code, not a new one: `[type.numlit]` says a literal acquires its type from the context it is
  written into, an arm's context is the scrutinee (the checker already unifies the two,
  `Reason::Pattern`), and E0415's clause already reports "a literal whose VALUE cannot be
  represented in the type that was decided". The checker's walk simply never visited a pattern.
  So (C), the rule the other clauses imply, is (A). Not taken: (B), never matches with a warning,
  which would keep a program E0415 says "can never mean what it says", and give `match v { 300 }`
  a meaning `v == 300` does not have; a new code, which would give one rule two codes.
- **R-s285-2, range endpoints: each end is a literal pattern of the scrutinee's type.** An end its
  type cannot hold is E0415 at that end, the exclusive high end included: `0..256` over a `u8` is
  E0415 at `256`, with a note naming `0..=255`. Not taken: admitting max+1 as an exclusive end, a
  second allowance beside E0415's direct `-<literal>`; nothing in the census writes it.
- **R-s285-3, the 64-bit unsigned edge: fixed here.** Once E0415 holds, a pattern above
  `i64::MAX` can only stand against a 64-bit unsigned scrutinee (`u64`, `uint`, `wrapping[u64]`).
  The checked machine and native then read such a pattern as its u64 bit pattern, which is how
  both already hold the value. So the edge row (every primitive's max) agrees on all four
  machines. Ranges whose ends lie above `i64::MAX` stay honest refusals on both compiled machines
  ("unsupported"), unchanged; an issue to file. Not taken: filing the equality case apart, since
  this lane's own edge rows would carry a known wrong answer.
- **R-s285-4, a signed literal pattern: no grammar change; its E0207 says why.** The parser's
  pattern entry gives `-1` its own E0207, "a literal pattern has no sign", at the `-` and the
  literal, in every pattern position, with a note naming the guard spelling (`n if n == -1`).
  lupin's mirror refuses it the same way. Not taken: a signed literal pattern in the grammar,
  which is a language change and needs its own ruling.
- **R-s285-5, a literal scrutinee: the shared literal type defaults to `i32`, and the pattern must
  fit it.** `match 5 { 3000000000 => … }` and `let v = 5` then `match v { 3000000000 => … }` are
  E0415 "does not fit `i32`", with the default's note. Not taken: letting the pattern widen the
  scrutinee (adoption never runs from a pattern to a value).
- **R-s285-6, lupin's rows in this repository:** the pinned lupin 0.1.59 has no E0415 for a
  pattern and accepts a signed one. The rows here hold lupin to the same spans whenever its record
  carries E0415 (or E0207), and otherwise to the never-matching answer each row is written for,
  never to the wrapped one. The mirror is a wolf-interp PR after this one, red first there.
- **R-s285-7, a lowering backstop:** the native lowering wraps every arm constant into the
  scrutinee's width, deliberately (s74). A constant that does not fit the scrutinee's type now
  reaches it only past a broken checker; it is refused there ("unsupported"), as an expression
  literal beyond its width already is (wolf-lang#151's backstop), never wrapped. Not taken: no
  backstop, which would let a future gap in the checker's walk be a silent wrong answer again.

## The changes and their rows, predicted

1. The spec: `[gram.pat.lit]` in `spec/01-grammar.md` §5, and `[gram.pat.range]`'s endpoint
   sentence pointing at it. Red first in the anchors gate: with the clause committed and
   `anchors.json` not regenerated, `cargo xtask spec-extract --check` fails "anchors.json OUT OF
   SYNC"; regenerated, it passes, with one anchor added and none removed.
2. The rows, red first, each a corpus file and a test in `crates/wolf_driver/tests/` run on the
   checked machine, native, release and lupin:
   - `typecheck/numlit_pattern_wraps.lu`, wolf-lang#1058's program, the first red. Base: the
     corpus step fails it by name (no E0415); the test fails native's `exit(0)`. Head: E0415 at
     `300` on all three compiled tiers; lupin 0.1.59 `exit(2)`.
   - `typecheck/numlit_pattern_edges.lu`: every integer primitive's max+1, and `wrapping[u8]`,
     `wrapping[i8]`; twelve E0415s, each at its literal.
   - `typecheck/numlit_pattern_positions.lu`: a tuple element, an enum payload, a struct field, an
     or-pattern's alternative, after `@`, a statement `match`, hex and `_` spellings.
   - `typecheck/numlit_pattern_range.lu`: a high end out (inclusive), the exclusive `0..256`, both
     ends out, an `i8` range; every out-of-type end reported.
   - `typecheck/numlit_pattern_default.lu`: the literal scrutinee and the unannotated binding.
   - `grammar/match_pattern_signed_literal.lu`: `-1` against a `u8`, `-129` against an `i8`, a
     signed literal in a tuple and as a range end. Base: E0207 with the generic message (the
     corpus step passes it, code and phase being unchanged); the test fails on the message and the
     spans. Head: the new message at each `-`.
   - `rows/match_literal_pattern_edges.lu`: every primitive's max and 0 as patterns, `u64` and
     `uint` max in decimal and hex, `2^63`, `wrapping[u64]`, nested; `run(exit=0)` on all four.
     Base: red on the checked machine (the u64 arms miss) and on native (unsupported). Head:
     green on all four (lupin 0.1.59 is already right here).
3. The checker: literal patterns and range ends recorded as they are checked, and ranged in
   `check_literal_fit` once types are final. The parser: the signed-literal E0207. The checked
   machine and native: a pattern above `i64::MAX` read as its u64 bits. Native: the backstop.
4. What must not move: no existing corpus row's verdict or phase; no snapshot but the new rows'
   lexer and parser snapshots; the lowering ledger gains the new rows only (the E0415 rows as
   `not mem-clean (pre-wir rungs own it)`, the edge row as `lowers`); the `wolf_sema` suite
   unchanged; the org's trees and wolf-std give no new E0415 (measured again at the head, file by
   file, on both binaries).
5. E0415's explanation gains the pattern sentence; `docs/diagnostics.md` is regenerated.
6. One plant: the checker's pattern half removed, one hunk in `crates/wolf_sema/src/check.rs`, red
   by name at the #1058 row's test.
