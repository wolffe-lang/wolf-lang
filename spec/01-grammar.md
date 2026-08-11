# Wolf Language Specification — 01: Surface Grammar

Status: normative, v0 (sprint s03). Clause anchors (`[gram.*]`) are stable:
they are cited by later spec documents, conformance tags (`conforms:`), and
diagnostics. EBNF blocks tagged ` ```ebnf ` are extracted to
`spec/grammar.ebnf` by `cargo xtask spec-extract` (CI-enforced sync).

Notation: W3C-style EBNF. `::=` defines; `|` alternates; `?` optional; `*`
zero-or-more; `+` one-or-more; parentheses group; terminals in `'quotes'`;
UPPER names are lexer tokens, lower names are syntactic productions.

This document says what *parses*. Meaning is informal gloss only; semantics
live in 02-memory-model, 03-concurrency, and later sema documents.

---

## 1. Lexical structure `[gram.lex]`

### 1.1 Source `[gram.lex.source]`

Source files are UTF-8, extension `.lu`. Byte order marks are rejected.
Tokens are defined over Unicode scalar values; all offsets in diagnostics
and slicing are byte offsets (D25).

### 1.2 Comments `[gram.lex.comment]`

`// …` line comment. `//! …` inner doc comment (file/module header; corpus
directives live here). `/// …` outer doc comment (documents the next item).
No block comments (nesting arguments lose to simplicity + lexer speed).

### 1.3 Identifiers `[gram.lex.ident]`

```ebnf
IDENT ::= ('_' XID_Continue+) | (XID_Start XID_Continue*)
```

A bare `_` is the wildcard identifier (never a binding you can read);
`_foo`-style identifiers are ordinary names, conventionally unused.
Identifiers that collide with reserved keywords do not parse (`[gram.inv]`).

### 1.4 Integer and float literals `[gram.lex.number]`

```ebnf
INT   ::= DEC_LIT | '0x' HEX_DIGIT ('_' | HEX_DIGIT)* | '0o' OCT_DIGIT ('_' | OCT_DIGIT)* | '0b' BIN_DIGIT ('_' | BIN_DIGIT)*
DEC_LIT ::= DIGIT ('_' | DIGIT)*
FLOAT ::= DEC_LIT '.' DEC_LIT EXPONENT? | DEC_LIT EXPONENT
EXPONENT ::= ('e' | 'E') ('+' | '-')? DEC_LIT
```

A float requires digits on **both** sides of the dot: `1.0` is a float;
`1.` is an integer followed by `.` (member access — `1.s`, `4096.kb` are
member/method access on the literal `[gram.amb.intdot]`); `1..10` is a
range. Underscore separators: `2_147_483_647`.

Counter-example: `1.e5` does not parse as a float (it is member access on
`1` with member `e5`); write `1.0e5`. Diagnostic should suggest the fix.

### 1.5 String literals — the mode stack `[gram.lex.str]`

Every plain string literal is an f-string (X9/D26). The lexer runs a mode
stack; inside interpolation braces it re-enters normal token mode. Nesting
strings inside interpolations is legal to depth 8; deeper is an error with
a "you do not want this" diagnostic (E0007, enforced at the parse tier).
The lexer's hard safety rail is depth 32 (E0108) — between 8 and 32 the
input still tokenizes so the parser can produce the friendly error.

```ebnf
STRING     ::= '"' STR_PART* '"'
STR_PART   ::= STR_TEXT | '{{' | '}}' | INTERP
INTERP     ::= '{' expr FORMAT_SPEC? '}'
FORMAT_SPEC ::= ':' /* fill/align/sign/width/precision/type, spec §7.4 */
```

- `{{` and `}}` are literal braces `[gram.lex.str.escape]`.
- The `:` beginning a format spec is the first top-level `:` inside the
  interpolation (top-level = not inside nested `(` `[` `{` or a nested
  string) `[gram.amb.fmtcolon]`.
- Escapes: `\n \t \r \\ \" \0 \x7f \u{1F43A}`.

**Multiline** `[gram.lex.str.multi]`: `"""` opens; the literal ends at the
next `"""`; the closing delimiter's column sets the dedent — every content
line must start with at least that much whitespace, which is stripped
(SE-0168 lineage). First newline after the opening `"""` is dropped.
Interpolation works inside.

**Raw** `[gram.lex.str.raw]`: `r"…"`, `r#"…"#`, `r##"…"##` — no escapes,
no interpolation, `#`-fences balance.

**Generalized literals** `[gram.lex.str.gen]`: `IDENT '"' … '"'` with no
whitespace between — `re"[a-z]+"`, `path"/etc/hosts"`. Desugars to a
comptime call `IDENT.from_literal("…")`; the prefix is any identifier that
is not a reserved keyword. Raw-mode body (no escapes/interpolation).

### 1.6 Newline termination `[gram.lex.newline]`

Go-adapted, last-token-only, **normative and byte-exact** (Track 2 must
match):

A statement terminator is inserted at a newline iff the last token on the
line is one of:

- an identifier or `_`
- any literal (INT, FLOAT, any string-mode end, `true`, `false`)
- one of the keywords: `return`, `break`, `continue`
- a closing delimiter: `)`, `]`, `}`
- postfix `?`

No terminator is inserted otherwise — in particular after binary operators,
`.`, `,`, `=`, open delimiters, and keywords other than the three above.
Multiline expression style is therefore **trailing** operator/dot; the
formatter enforces it (`[gram.fmt.continuation]`).

`;` is an explicit terminator token, interchangeable with the inserted one
— it exists for single-line blocks (`{ print(USAGE); return 2 }`, the
guard-clause idiom). An empty statement (a `;` with no statement before it
on the same line) is an error (E0002). The formatter strips every `;` that
is not separating statements within a single-line block
(`[gram.fmt.inline]`).

Exceptions (grammar-level):
- `else` must appear on the same line as the preceding `}` — the inserted
  terminator after `}` would otherwise orphan it. `[gram.amb.else]`
- The *innermost* enclosing delimiter decides: when it is `(`, `[`, or an
  interpolation, newlines never terminate. A `{…}` block re-enables
  insertion inside itself whatever it is nested in — statements inside a
  closure body passed as a call argument still terminate. `{…}` never
  suppresses.
- No terminator is inserted after the `]` that closes an attribute
  (`#[…]`) — the attribute prefixes the construct on the next line.
- A terminator may be omitted before a closing `}` (Go's rule 2): the
  final statement of a single-line block needs no `;`.

Counter-example (does not parse; diagnostic points at the break):

```text
let x = a
      + b        // ERROR: `+ b` is a new statement; write `a +` on line 1
```

### 1.7 Nesting rails `[gram.lex.rails]`

Hostile nesting degrades to diagnostics, never to crashes. Normative
rails: string/interpolation mode nesting per `[gram.lex.str]` (8
friendly / 32 lexer); **expression/statement recursion depth 256** —
deeper input is rejected with a diagnostic at the point the rail is
hit. Both implementations enforce identical rail values
(differential-tested).

---

## 2. Compilation unit & items `[gram.item]`

### 2.1 Unit `[gram.item.unit]`

```ebnf
unit  ::= inner_doc* item*
item  ::= attribute* visibility? bare_item
bare_item ::= fn_item | let_item | var_item | type_item | trait_item
            | impl_item | use_item | import_c_item | const_item
visibility ::= 'pub' ('(' 'pkg' ')')?
```

Directory = module (D32); there is no `mod` keyword. `[gram.item.module]`

### 2.2 Imports `[gram.item.use]`

```ebnf
use_item ::= 'use' path ( '.' '{' IDENT (',' IDENT)* ','? '}' )? ('as' IDENT)? TERM
import_c_item ::= 'import' 'c' STRING TERM
path ::= IDENT ('.' IDENT)*
```

`use std.fs` · `use std.{fs, net}` · `use verylongname as vln`. `import c
"stdlib.h"` binds the contextual namespace `c` (`[gram.inv.ctx]`); the
string is a header name, not wolf syntax. A prelude (spec 02-…, D31) makes
`print`, `Map`, `List`, `channel`, `Mutex`, region/pool APIs ambient.

### 2.3 Functions `[gram.item.fn]`

```ebnf
fn_item   ::= fn_qual* 'fn' IDENT generics? '(' params? ')' fn_ret? (block | TERM)
fn_qual   ::= 'comptime' | 'extern' STRING | 'export'
generics  ::= '[' generic_param (',' generic_param)* ','? ']'
generic_param ::= IDENT (':' bound)? | IDENT ':' 'type'
bound     ::= path ('+' path)*
params    ::= param (',' param)* ','?
param     ::= param_mode? IDENT ':' type | param_mode? 'self' view_set?
param_mode ::= 'mut' | 'take'
view_set  ::= '.' '{' IDENT (',' IDENT)* '}'
fn_ret    ::= '->' ret_type
ret_type  ::= type ('!' error_row)?   /* `-> !T` parses via type's '!' type */
```

- Modes (D10 Tier 0): default is `read` (no keyword — the absence *is* the
  syntax); `mut` = exclusive inout; `take` = consume.
- View sets: `fn norm(mut self.{x, y})` — field-granular exclusivity.
- Returns: `-> T` plain; `-> !T` error union with inferred private row;
  `-> T ! {Tag(Payload), io.Error}` explicit row (`[gram.type.row]`).
- Generic params use `[]` — there is no `<>` anywhere in the language.
- Bodyless form (TERM) under `extern`, or as a trait member
  (a required method the impl must provide).

Examples: see `corpus/wordcount.lu` (`fn top[T](m: Map[T, int], n: int)`).

Counter-example: `fn f(x: mut int)` does not parse — modes precede the
*name*, not the type: `fn f(mut x: int)`.

### 2.4 Bindings & constants `[gram.item.let]`

```ebnf
let_item ::= 'let' pattern (':' type)? '=' expr TERM
var_item ::= 'var' pattern (':' type)? '=' expr TERM
const_item ::= 'const' IDENT (':' type)? '=' expr TERM
```

`let` immutable, `var` mutable, `const` comptime-evaluated. Item-level and
statement-level share the grammar. `let (a, b) = pair` destructures.

### 2.5 Types `[gram.item.type]`

```ebnf
type_item   ::= 'type' IDENT generics? '=' type_def TERM?
type_def    ::= struct_def | enum_def | type   /* `distinct T` via prefix_type_kw */
struct_def  ::= 'struct' '{' field* '}'
field       ::= attribute* visibility? IDENT ':' type ','?
enum_def    ::= 'enum' '{' variant (',' variant)* ','? '}'
variant     ::= IDENT ('(' type (',' type)* ')')?
```

Sugar: `struct Name { … }` and `enum Name { … }` at item level are
accepted and canonicalized by the formatter to themselves (they are the
idiomatic spelling; `type Name = struct { … }` is the general form that
also serves comptime type construction).

```ebnf
struct_item ::= 'struct' IDENT generics? '{' field* '}'
enum_item   ::= 'enum' IDENT generics? '{' variant (',' variant)* ','? '}'
```

(These two are included in `bare_item`.)

### 2.6 Traits & impls `[gram.item.trait]`

```ebnf
trait_item ::= 'trait' IDENT generics? '{' trait_member* '}'
trait_member ::= fn_item | type_item | const_item
impl_item  ::= 'impl' generics? type ('for' type)? '{' impl_member* '}'
impl_member ::= fn_item | type_item | const_item
```

Nominal traits, checked generics (D28). Adapter types are `distinct`
types (`type Cover = distinct Song`): same layout as the base, free
bidirectional `as` casts, an *own* empty impl set — the sanctioned
orphan-rule escape, with no dedicated keyword. The impl subject is a *type*
(so `impl[T] List[T] { … }` works); when `for` is present the first
type is the trait path applied to its arguments.

Punctuation asymmetry, intentional: struct **fields** are
newline-separated declarations (per-field `','?`); enum **variants**
and error-**row** entries are comma-punctuated lists. Fields read like
items; variants and rows read like alternatives.

### 2.7 Attributes `[gram.item.attr]`

```ebnf
attribute ::= '#[' attr (',' attr)* ']'
attr      ::= path attr_input?
attr_input ::= '(' attr_arg (',' attr_arg)* ')' | '=' literal
attr_arg  ::= attr | literal   /* `key = "v"` is attr with '=' input */
```

Closed, structured — attributes are not token soup (no macros at v1).
Known at v1: `#[trusted]`, `#[noalloc]`, `#[inplace]`, `#[nopanic]`,
`#[bounded_stack]`, `#[repr(c)]`, `#[cfg(target = "…")]`,
`#[allow(w1301)]` (item-granular warning suppression, §9.3 — the
arguments are diagnostic codes through the ordinary `attr_arg`
production; no new grammar).

---

## 3. Statements & expressions `[gram.expr]`

Wolf is expression-oriented: blocks evaluate to their final expression.
Assignment is a **statement**, not an expression (`[gram.expr.assign]`).

### 3.1 Blocks & statements `[gram.expr.block]`

```ebnf
block ::= '{' stmt* expr? '}'
stmt  ::= attribute* stmt_base
stmt_base ::= let_item | var_item | const_item | assign_stmt | defer_stmt
        | expr_stmt | item
assign_stmt ::= place assign_op expr TERM
assign_op   ::= '=' | '+=' | '-=' | '*=' | '/=' | '%=' | '&=' | '|=' | '^=' | '<<=' | '>>='
defer_stmt  ::= ('defer' | 'errdefer') expr TERM
expr_stmt   ::= expr TERM
place ::= expr  /* must be a place-expression; checked in sema, not grammar */
```

### 3.2 Precedence & operators `[gram.expr.prec]`

One authoritative climb, tightest first. Comparison operators do **not**
chain (`a < b < c` is a parse error with a "did you mean `&&`" diagnostic).

| # | Operators | Assoc |
|---|-----------|-------|
| 1 | paths, literals, `(e)`, block-exprs, closures | — |
| 2 | postfix: call `f(…)`, index/generic-apply `e[…]`, member `.`, postfix `?` | left |
| 3 | prefix: `!` `-` `&` `&mut` `*` `move` `copy` `shared` | — |
| 4 | `as` type cast | left |
| 5 | `*` `/` `%` | left |
| 6 | `+` `-` | left |
| 7 | `<<` `>>` | left |
| 8 | `&` | left |
| 9 | `^` | left |
| 10 | `\|` | left |
| 11 | `==` `!=` `<` `>` `<=` `>=` `<=>` | none |
| 12 | `&&` | left |
| 13 | `\|\|` | left |
| 14 | `..` `..=` ranges (endpoints may be `^n` from-end forms) | none |
| 15 | `else` defaulting (`expr else expr`, `expr else \|p\| expr-or-block`) | right |

Prefix `&`/`&mut` create Tier-0 **local** borrows (second-class at
function boundaries; 02-memory-model). Prefix `*` is raw-pointer deref
(unsafe tier). `move` transfers a region/value; `copy` forces a copy of a
non-`Copy` value; `shared` creates a Tier-2 RC cell from a value
(`let a = shared (Cfg { limit: 7 })`).

**Operator↔trait bridge** (posture recorded 2026-08-10; wolf-std F-0004
/ issue #5, contract F3). For user types the comparison operators
desugar to in-scope trait impls by name — `==`/`!=` to `Eq.eq` (negated
for `!=`), the `< <= > >= <=>` family to `Ord.cmp` — with `Ord requires
Eq` as a supertrait clause once the trait engine grows supertraits;
`<=>` yields `std.cmp.Ordering` when std lands (`int` is the v0 stopgap
read). Enum structural `==` is language-side. The bare-literal `i32`
defaulting vs `impl … for int` mismatch (F-0004 gap 3) is acknowledged
and must be resolved by the bridge's clause set when the typing document
lands; this paragraph records the decision, not the mechanism.

### 3.3 Primary expressions `[gram.expr.primary]`

```ebnf
expr ::= else_expr | jump_expr
else_expr ::= range_expr ('else' (block | '|' closed_pattern '|' (expr | block) | expr))?
range_expr ::= r_end (('..' | '..=') r_end?)? | ('..' | '..=') r_end
r_end ::= or_expr | '^' or_expr
/* `^n` marks a from-end endpoint (D25): s[^1], s[^13..], s[..^1].       */
/* tiers 3–13 (or_expr ↓ prefix_operand) are rendered into the extracted
   grammar from the §3.2 climb table by `cargo xtask spec-extract`.      */
postfix_expr ::= receiver (call_args | index_args | '.' member | '?')*
receiver   ::= primary | '(' param_mode expr ')'
/* the moded form is receiver position only: '.' member must follow.  */
call_args  ::= '(' (call_arg (',' call_arg)* ','?)? ')'
call_arg   ::= ('mut' | 'take')? expr
index_args ::= '[' (index_arg (',' index_arg)* ','?)? ']'
index_arg  ::= call_arg | prefix_type_kw type | 'region'
member     ::= IDENT | INT | reserved_kw
/* tuple access: pair.0, pair.1. Member position is keyword-transparent:
   nothing competes with a member name after `.`, so `.take(n)` and
   `s.spawn(…)` parse — member names live in their own namespace. */
primary ::= literal | path | struct_lit | '(' expr (',' expr)* ','? ')' | block
          | if_expr | match_expr | loop_expr | closure | region_expr
          | scope_expr | select_expr | when_expr | unsafe_expr | spawn_expr
          | asm_expr | borrow_expr
struct_lit ::= path '{' (field_init (',' field_init)* ','?)? '}'
field_init ::= IDENT ':' expr | IDENT
literal ::= INT | FLOAT | STRING | MULTILINE_STRING | RAW_STRING
          | GENERALIZED_STRING | 'true' | 'false'
```

Struct literals: `ParseError { at: i, found: c }`; `Point { x }` shorthand
binds the field from the identifier. Illegal in condition/scrutinee
position without parens (`[gram.amb.structlit]`).

- **Call-site modes (X1)**: `f(mut x)`, `pool[mut prev]` — `mut`/`take`
  are argument prefixes in both call and index argument lists.
- **Receiver modes (X1)**: a method declared `mut self` / `take self` is
  called through a parenthesized moded receiver — `(mut p).norm()`,
  `(take conn).close()` — mirroring call-argument modes exactly. The
  moded form is admitted only where a `.` member immediately follows the
  closing `)`; anywhere else it is a parse error (E0210; primary span = the
  entire parenthesized moded receiver). `read self`
  receivers stay bare: `p.dist()`.
- `index_arg` also admits the type forms no expression can spell
  (`List[handle Node]()`, `channel[region]`) — `e[…]` stays one postfix
  shape and sema (D29) sees a single argument list (`[gram.amb.brackets]`).
- `(e)` grouping; `(a, b)` tuple; `(a,)` one-tuple.
- Parenthesized-vs-tuple: comma decides, standard.

### 3.4 Control flow as expressions `[gram.expr.flow]`

```ebnf
if_expr    ::= 'if' expr block ('else' (if_expr | block))?
match_expr ::= 'match' expr '{' arm* '}'
match_arm  ::= pattern ('if' expr)? '=>' (expr | block)
arm        ::= match_arm arm_sep?
arm_sep    ::= ',' | TERM
loop_expr  ::= 'for' pattern 'in' expr block
             | 'while' expr block
             | 'loop' block
jump_expr  ::= 'return' expr? | 'break' expr? | 'continue'
```

Loop labels do not exist at v1: `break`/`continue` target the innermost
loop. Every label surface either collides with expression grammar
(`break name`) or imports sigils; nested-loop escapes use `return` from
a helper. Revisit post-v1 with evidence.

Arm separators (also for `select`): the comma is **required** after an
expression-bodied arm that is followed by another arm, **optional** after a
block-bodied arm (a newline suffices there — the terminator inserted after
the arm's `}` per `[gram.lex.newline]` *is* the separator, hence TERM in
`arm_sep`). The formatter emits trailing commas in multiline arm lists
either way (`[gram.fmt.commas]`).

The condition of `if`/`while` and the scrutinee of `match`/`for` use
no-struct-literal expression mode (a `{` there begins the block —
`[gram.amb.structlit]`).

### 3.5 Closures `[gram.expr.closure]`

```ebnf
closure ::= 'fn' '(' params_untyped? ')' (block | expr)
params_untyped ::= closure_param (',' closure_param)* ','?
closure_param  ::= param_mode? IDENT (':' type)?
```

Expression-bodied closure extent rule (`[gram.amb.closure]`): the body is
one `else_expr` — it extends maximally rightward and is terminated only by
a token the expression grammar cannot consume (`,` `)` `]` `}` TERM).
`sorted_by(fn(a, b) b.1 <=> a.1)` parses as expected.

### 3.6 Regions `[gram.expr.region]`

Both locked forms (X4):

```ebnf
region_expr ::= 'region' IDENT? (':' region_strategy)? block  /* sugar   */
              | 'region' '(' region_strategy? ')'             /* value   */
              | 'in' expr block                               /* into r  */
              | 'freeze' prefix_operand
region_strategy ::= 'rc' | 'pool' '(' type ')'
```

- Sugar: `region tmp { … }` — create, scope, free at `}`; the block's
  value escapes by move. The name is optional: `freeze region { … }`
  builds anonymously and promotes (the build-then-share idiom).
- Value: `let r = region()`, `let r = region(rc)`, `region r: pool(Node)
  { … }` names the sugar's region for `in r { … }` use within.
- `in r { … }` evaluates its block with allocations landing in `r`.
- `freeze e` promotes to deep-immutable; its operand is a *prefix-tier*
  operand (tier 3, like `move`/`copy`/`shared`): `freeze r == x` means
  `(freeze r) == x`. The `in`-block header expression parses in
  no-struct-literal mode. `freeze region { ... } ` composes.
- `move` (prefix operator, tier 3) transfers: `ch.send(move r)`.

`rc` and `pool` are contextual keywords (`[gram.inv.ctx]`).

### 3.7 Concurrency surface `[gram.expr.conc]`

```ebnf
scope_expr  ::= 'scope' IDENT? block
spawn_expr  ::= 'spawn' 'proc' path call_args
select_expr ::= 'select' '{' select_arm (arm_sep select_arm)* arm_sep? '}'
select_arm  ::= pattern 'from' expr '=>' (expr | block)
              | 'timeout' '(' expr ')' '=>' (expr | block)
when_expr   ::= 'when' '(' expr (',' expr)+ ','? ')' block
```

- Task spawning is a *method* on the scope handle (`s.spawn(fn() { … })`)
  — no task-spawn keyword exists; the binder `scope s { … }` makes the
  handle a value (D16: scope handles are values).
- `spawn proc worker(args)` starts a proc under the current supervisor;
  proc linking/monitoring are methods (`w.monitor()`, `w.link()`), not
  keywords.
- `from` and `timeout` are contextual (only meaningful in select arms).
- `when` requires ≥2 operands (one operand is just a method on the sync
  type).

### 3.8 Unsafe tier `[gram.expr.unsafe]`

```ebnf
unsafe_expr ::= 'unsafe' block
              | 'unsafe' 'c' capture_list? block          /* inline C   */
asm_expr    ::= 'asm' '{' STRING (',' asm_operand)* ','? '}'
asm_operand ::= IDENT '=' asm_dir '(' asm_constraint ')' expr
              | asm_dir '(' asm_constraint ')' expr
asm_dir     ::= 'in' | 'out' | 'inout' | 'lateout'
asm_constraint ::= IDENT
capture_list ::= '[' IDENT (',' IDENT)* ','? ']'
assume_stmt ::= 'assume' 'noalias' expr (',' expr)+ TERM
borrow_expr ::= 'borrow' expr 'from' expr
```

`asm`, `assume`, `borrow` are only legal inside `unsafe` (enforced in
sema; the grammar accepts them anywhere for recovery quality, D22).
Inline-C block bodies are opaque token text to wolf's lexer (brace-balanced
scan; c10 owns their meaning). `asm_expr` is a `primary`; `assume_stmt`
is a `stmt`; `borrow_expr` is a `primary`.

---

## 4. Types `[gram.type]`

```ebnf
type ::= path type_args?
       | '!' type
       | type '!' error_row          /* postfix row: T ! {row}, any type position */
       | prefix_type_kw type
       | '*' type                    /* raw pointer, unsafe tier */
       | 'dyn' path
       | '(' type (',' type)* ','? ')'
       | 'fn' '(' (type (',' type)*)? ')' ('->' ret_type)?
       | 'type'                      /* the type of types, comptime */
       | 'region'                    /* the type of first-class regions (X4) */
prefix_type_kw ::= 'shared' | 'handle' | 'weak' | 'distinct'
type_args ::= '[' type_arg (',' type_arg)* ','? ']'
type_arg  ::= type | expr            /* const generics; disambiguated in sema */
error_row ::= '{' row_entry (',' row_entry)* (',' '..')? ','? '}'
row_entry ::= path ('(' type (',' type)* ')')?
```

- `!T` and `T ! {row}` per D30. In *expression* position `!` is unary not;
  in *type* position it is the error-union constructor. The positions are
  syntactically disjoint (`[gram.amb.bang]`).
- Postfix rows are first-class in **every** type position — parameter,
  `let`/`var` annotation, field, variant payload — not just `ret_type`,
  which stays as spelled in `[gram.item.fn]`. (Adopted 2026-08-10 from
  wolf-std F-0002 / issue #3: `std.option`'s six helpers were unwritable
  with rows confined to return position.)
- `handle Node`, `shared Config`, `weak Parent`: prefix type keywords.
- `Map[str, int]`, `List[(T, int)]`: `[]` type application.
- `&T`-style reference *types* do not exist in the surface (borrows are
  second-class at boundaries; D10) — parameter modes carry what references
  would.

---

## 5. Patterns `[gram.pat]`

```ebnf
pattern ::= closed_pattern ('|' closed_pattern)*
closed_pattern ::= '_' | literal | IDENT
          | path '(' pattern (',' pattern)* ','? ')'
          | '(' pattern (',' pattern)* ','? ')'
          | IDENT '@' closed_pattern
```

Payload binding: `BadDigit(e) => …`; or-patterns `A | B`; guards are arm
syntax (`[gram.expr.flow]`), not pattern syntax. `closed_pattern` is a
pattern without a top-level `|`: positions where a `|` delimiter follows
the pattern — the `else` handler `else |pat| …` (`[gram.expr.primary]`)
— take `closed_pattern`, keeping the or-bar and the closing delimiter
unambiguous (parenthesize inside the payload to combine: `E((A | B))`).

---

## 6. Keyword & operator inventory `[gram.inv]`

### 6.1 Reserved keywords `[gram.inv.kw]` (closed set — 50)

```ebnf
reserved_kw ::= 'as' | 'asm' | 'assume' | 'borrow' | 'break' | 'comptime'
  | 'const' | 'continue' | 'copy' | 'defer' | 'distinct' | 'dyn' | 'else'
  | 'enum' | 'errdefer' | 'export' | 'extern' | 'false' | 'fn' | 'for'
  | 'freeze' | 'handle' | 'if' | 'impl' | 'import' | 'in' | 'let' | 'loop'
  | 'match' | 'move' | 'mut' | 'proc' | 'pub' | 'region' | 'return'
  | 'scope' | 'select' | 'shared' | 'spawn' | 'struct' | 'take' | 'trait'
  | 'true' | 'type' | 'unsafe' | 'use' | 'var' | 'weak' | 'when' | 'while'
```

(The list is normative, the count is a checksum: 50.) Additions/removals
are spec commits with corpus updates.

### 6.2 Contextual keywords `[gram.inv.ctx]`

Identifier everywhere except the noted position: `c` (`import c`,
`unsafe c`), `rc` / `pool` (region strategies), `from` / `timeout`
(select arms), `noalias` (after `assume`), `pkg` (in `pub(pkg)`), the v1 asm register class `reg` (target-specific classes arrive with c10),
`self` (receiver), `in`/`out`/`inout`/`lateout`/register classes (asm
operands). Rationale: each appears only after a reserved keyword or inside
a closed construct, so reserving them would steal good identifiers
(`from`, `timeout`, `c`) for no parsing benefit.

### 6.3 Deliberately absent `[gram.inv.absent]`

No turbofish, no `<>` generics, no `do` single-statement form, no
`++`/`--`, no statement macros, no ternary `?:` (use `if` expressions),
no `goto`, no *required* semicolons (terminators are inserted;
`[gram.lex.newline]` defines the explicit `;` and its single legal use).

---

## 7. Formatter-canonical style `[gram.fmt]` (appendix, normative for `wolf fmt`)

- `[gram.fmt.indent]` 4 spaces; no tabs. Line width 100 (soft — the
  formatter breaks at operators/args, never mid-token/mid-string).
- `[gram.fmt.brace]` Opening brace on the construct's line; `} else` on
  one line; `}` of an item followed by one blank line.
- `[gram.fmt.continuation]` Multiline expressions break **after** binary
  operators and after `.` (trailing style — required by
  `[gram.lex.newline]`); continuations indent one level.
- `[gram.fmt.commas]` Trailing comma in every multiline list; none inline.
- `[gram.fmt.inline]` A block stays on one line (with `;` separators) only
  when it is a guard-clause-shaped body (≤2 statements, fits the width);
  otherwise the formatter breaks it multiline and strips the semicolons.
- `[gram.fmt.region]` X4 canonicalization: a region created, used
  lexically, and freed in one scope is written in sugar form; a region
  that escapes (moved, stored, returned) is written in value form. The
  formatter rewrites sugar↔value only when provable from syntax alone.
- `[gram.fmt.imports]` `use` items first, sorted (std first, then
  packages, then relative), one blank line after; `import c` after `use`.
- `[gram.fmt.strings]` Prefer `"""` when a literal contains a newline;
  prefer raw when it contains ≥2 backslash escapes.
- `[gram.fmt.canon]` `corpus/wordcount.lu` is the canonical formatted
  artifact; `wolf fmt` must fix-point every corpus file byte-identically.

---

## 8. Ambiguity annex `[gram.amb]` (living)

Each entry: the rule, and its paired files in `corpus/grammar/`.

- `[gram.amb.brackets]` **`e[…]` is one postfix form.** Whether it is
  indexing or generic application is resolved in sema (types-as-values,
  D29) — the grammar has a single `index_args` production, so there is
  nothing to disambiguate at parse time. `f[int](x)` and `m[k]` are the
  same shape. Files: `brackets_index.lu`, `brackets_generic_call.lu`.
- `[gram.amb.intdot]` `1.s` = member on int; `1.0` float; `1..2` range;
  `1.` = int then member-dot (awaiting member). Files: `intdot_member.lu`,
  `intdot_range.lu`.
- `[gram.amb.fmtcolon]` First top-level `:` in an interpolation starts the
  format spec: `"{m[k]:>8}"` formats `m[k]`; `"{ {a: 1}.a }"` — the `:`
  inside `{…}` nesting is not top-level. Files: `interp_fmtcolon.lu`,
  `interp_nested.lu`.
- `[gram.amb.else]` `else` binds to the nearest viable construct on the
  same logical statement: after an `if`'s `}` it continues the if; after a
  complete expression it is the defaulting operator. `} else` same-line
  rule makes the two cases lexically disjoint. Files: `else_default.lu`,
  `else_chain.lu`.
- `[gram.amb.bang]` `!` prefix in expression position = not; `!` in type
  position (after `->`, after `:`, inside `[…]` type args) = error union.
  Disjoint by position. Files: `bang_not.lu`, `bang_errunion.lu`.
- `[gram.amb.closure]` Expression-bodied closures extend maximally; a
  closure passed as a non-final argument must be block-bodied if its body
  would swallow the comma — it cannot, because `,` terminates. File:
  `closure_extent.lu`.
- `[gram.amb.structlit]` No struct-literal expressions in condition/`in`-header/
  scrutinee position; `if x == (Point { x: 0 }) { … }` requires parens.
  Files: `structlit_cond.lu` (counter), `structlit_paren.lu`.
- `[gram.amb.when]` `when` is reserved, so `when(a, b)` is never a call.
  File: `when_reserved.lu` (counter-example, expects `fail`).
- `[gram.amb.newline]` Trailing-operator continuation accepted; leading-
  operator rejected. Files: `newline_trailing.lu`, `newline_leading.lu`
  (counter).

---

## 9. Diagnostics `[diag]` (seeded s10; warnings formalized s67)

Every counter-example above names an expected diagnostic. Codes reserved:
E0001 (leading-operator continuation), E0002 (empty statement),
E0003 (comparison chaining), E0004 (float `1.e5`), E0005 (`else` on new
line), E0006 (struct literal in condition; primary span = the opening `{`), E0007 (interp nesting depth),
E0008 (keyword as identifier — names the keyword and suggests `r#`-free
rename; wolf has no raw identifiers, pick another name).

### 9.1 The severity contract `[diag.sev]`

- `[diag.sev.error]` An **error** rejects meaning: the program is
  outside the language, and no configuration reinstates it. Errors are
  `E####` and cannot be leveled, allowed, or suppressed.
- `[diag.sev.warn]` A **warning** flags a legal-but-inadvisable
  program. Every warning cites a *concrete hazard* or an idiom rule
  with a D-number — never speculation ("might be slow someday" is not
  a warning). A warning changes no program's meaning; a build with
  warnings is a correct build.
- `[diag.sev.silence]` **Silence** is for what the formatter owns:
  layout, spacing, ordering — `wolf fmt` (§7) is the arbiter of style,
  and no diagnostic duplicates it.
- `[diag.sev.teach]` Warnings obey the same voice discipline as errors
  (D22): a warning teaches or it does not ship, and every code carries
  an extended explanation (`wolf --explain W1301`) and at least one
  reviewed fixture (the s10 catalog law, extended to warnings by s67).

### 9.2 Code families `[diag.family]`

`E####`/`W####` share one numbering plane, family = the first two
digits, owned by the analysis that computes the code. Established
E-families: E000x grammar reservations (above), E01xx lexer, E02xx
parser, E03xx resolution, E04xx typing, E05xx traits, E06xx error
rows, E07xx comptime, E08xx sema completion, E1xxx memory tiers
(E13xx unsafe, E14xx checked execution). W-families mirror the plane:

- **W03xx** — frontend/resolution-adjacent warnings. W0301 (partial
  format, s11 — grandfathered), W0302 (`#[allow]` of an unregistered
  code), W0303 (`#[allow]` of nothing).
- **W1[0-3]xx** — memory/concurrency/abi-adjacent warnings. W1301
  (`unsafe` block without a `# Safety:` comment, s22 —
  grandfathered).

Numbers retire with their codes and are never reused. A
warning-severity diagnostic under an E-number (E0802, unreachable
match arm) is a grandfathered exception: severity, not the code
letter, is normative for leveling.

### 9.3 Levels `[diag.level]`

Every warning has a level: **allow** (dropped), **warn** (reported —
the default), **deny** (promoted to an error that fails the build; the
code keeps its `W####` spelling). Three declarative sources, most
local authority first:

- `[diag.level.attr]` `#[allow(w1301)]` — item-granular source
  suppression through the ordinary attribute grammar (§2.7): `allow`
  with code arguments, lowercase canonical, family form `w13xx`
  accepted. The region is the attributed declaration, body included.
  An argument naming no registered code is W0302; an empty `#[allow]`
  is W0303.
- `[diag.level.cli]` `wolf build --allow|--warn|--deny <sel>` with
  `<sel>` a code (`W1301`), a family (`W13xx`), or `warnings`;
  `--deny-warnings` ≡ `--deny warnings` (the CI posture — wolf's own
  corpus holds itself to it).
- `[diag.level.pkg]` The manifest's `lints.allow|warn|deny = sel, …`
  entries in `wolf.pkg` (declarative only, D33). CLI rules layer over
  manifest rules.

Precedence is specificity: per-code beats per-family beats
all-warnings; among equals the later rule wins (CLI after manifest).
There are no plugin lints, ever — the compiler is the arbiter of
idiomatic wolf (D33's spirit; c16).

`wolf fix` promotes machine-applicable suggestions (s10) to applied
edits — dry-run by default, `--apply` writes, idempotent because a
fix removes the diagnostic that carried it. Only `MachineApplicable`
suggestions are ever applied unattended.
