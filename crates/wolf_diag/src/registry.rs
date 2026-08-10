//! The diagnostic-code registry — the single place a code can be born.
//!
//! Every code the compiler can emit is declared here, one rigid
//! `code!` entry per code:
//!
//! ```text
//! code!(E0101, "one-line summary", r#"extended explanation"#);
//! ```
//!
//! **This format is a hard interface.** `cargo xtask diag-catalog`
//! parses this file *textually* (one entry per code, no computed
//! values, grep-friendly): do not wrap entries in helper macros, build
//! summaries from `concat!`, or declare two codes in one entry. The
//! explanation is mandatory and must be real prose (a test below
//! rejects stubs); it becomes `wolf --explain E####` and the s65 docs
//! pages.
//!
//! [`Code`] has a private field, so the *only* way to obtain one is
//! through the consts this registry emits — a diagnostic cannot be
//! constructed with an unregistered code, by type. Families: `E01xx`
//! lexer, `E000x` spec/01 §9 reservations + `E02xx` parser, `E03xx`
//! name resolution (s12), `E1xxx+` sema and beyond (s13+). Warnings
//! are `W####`. Retiring a code retires its number forever.

/// A registered diagnostic code (e.g. `E0101`). Constructible only by
/// this registry; compare with other codes or with `&str`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Code(&'static str);

impl Code {
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::fmt::Debug for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl PartialEq<&str> for Code {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Code> for &str {
    fn eq(&self, other: &Code) -> bool {
        *self == other.0
    }
}

/// One registry row: the code, its one-line summary (headline of
/// `--explain` and the catalog), and the extended explanation.
#[derive(Clone, Copy, Debug)]
pub struct CodeInfo {
    pub code: Code,
    pub summary: &'static str,
    pub explanation: &'static str,
}

/// Look up a code by its text (`wolf --explain E0101`).
pub fn explain(code: &str) -> Option<&'static CodeInfo> {
    REGISTRY.iter().find(|info| info.code.0 == code)
}

macro_rules! registry {
    ( $( code!($name:ident, $summary:literal, $explanation:literal); )+ ) => {
        /// The registered codes, importable as `wolf_diag::codes::E0101`.
        pub mod codes {
            $( pub const $name: super::Code = super::Code(stringify!($name)); )+
        }

        /// Every registered code with its prose, in declaration order.
        pub static REGISTRY: &[CodeInfo] = &[
            $( CodeInfo {
                code: codes::$name,
                summary: $summary,
                explanation: $explanation,
            }, )+
        ];
    };
}

registry! {

// ------------------------------------------------------------------------
// E000x — codes reserved by spec/01 §9 (grammar counter-examples).
// ------------------------------------------------------------------------

code!(E0001, "a statement begins with an operator", r#"
Wolf ends a statement at the end of the line, so a line that begins with
an operator such as `+` or `.` has nothing to attach to: the previous
statement already ended at the newline. Continuations in wolf are
*trailing* — to spread an expression over several lines, end the line
with the operator (or an open `(`/`[`, inside which newlines never
terminate), rather than starting the next line with it. Move the
operator to the end of the previous line and the two lines become one
statement again.
"#);

code!(E0002, "an empty statement (`;` that terminates nothing)", r#"
In wolf, `;` and the end of a line are the same statement terminator,
and a terminator must terminate something. A `;` directly after `{`, or
directly after another terminator, ends an *empty* statement, which the
grammar rejects rather than silently ignoring — a stray `;` is usually
a typo or a leftover from another language. Delete the `;`; use it only
to separate statements written on a single line.
"#);

code!(E0003, "comparison operators do not chain", r#"
`a < b < c` does not mean "b is between a and c" in wolf: comparison
operators are non-associative, so the grammar rejects a comparison whose
operand is itself a comparison instead of silently evaluating
`(a < b) < c` on a boolean, which is never what anyone means. Write the
two comparisons out and join them: `a < b && b < c`.
"#);

code!(E0004, "a float exponent written as member access", r#"
`1.e5` is not a float in wolf: a float literal needs digits on both
sides of the dot, so `1.e5` parses as the member `e5` accessed on the
integer `1` — and integers have no such member. The exponent form you
meant spells the fraction out: `1.0e5`. Write digits after the dot
(`1.0e5`, `2.5e-3`) and the literal lexes as one float token; the
suggested edit does exactly that ([gram.amb.intdot]).
"#);

code!(E0005, "`else` may not start a new line", r#"
The newline after the `}` of the then-block ends the `if` statement, so
an `else` on the next line belongs to nothing — wolf will not guess
whether it was meant for the `if` above it. Put the `else` on the same
line as the closing brace of the block before it: `} else {`. This is
the one place wolf's newline-termination rule constrains layout
([gram.amb.else]).
"#);

code!(E0006, "a struct literal cannot sit bare in condition position", r#"
In a condition or scrutinee — after `if`, `while`, `match`, or `for
… in` — a `{` must open the construct's block, so a bare struct literal
like `if x == Point { x: 1 } …` would be ambiguous: is `{ x: 1 }` the
literal or the then-block? Wolf resolves the ambiguity by fiat: the `{`
opens the block, always ([gram.amb.structlit]). To use a struct literal
there, wrap it in parentheses — `if x == (Point { x: 1 }) { … }` — and
the ambiguity disappears.
"#);

code!(E0007, "string interpolations nest deeper than 8 levels", r#"
A string inside an interpolation inside a string can nest, but past 8
levels wolf stops you: nobody can read a 9-deep interpolation, and the
limit exists to keep pathological one-liners out of the language
([gram.lex.str]). Hoist the innermost string expression into a `let`
binding and interpolate the binding instead — each hoist removes a
level. (The lexer has a separate hard safety rail at 32 levels, E0108.)
"#);

code!(E0008, "a reserved keyword used as a name", r#"
All 50 of wolf's keywords are reserved everywhere — a keyword can never
name a function, parameter, field, or binding, and wolf deliberately has
no escape hatch like Rust's `r#` raw identifiers ([gram.inv.kw],
spec/01 §9). Pick a different name; the conventional dodges are a
trailing underscore (`type_`) or a more specific word (`kind`,
`variant`). Member access is the one keyword-transparent position:
`x.take(n)` is fine because `.take` can only be a member name.
"#);

// ------------------------------------------------------------------------
// E01xx — the lexer's family.
// ------------------------------------------------------------------------

code!(E0101, "invalid escape sequence in a string literal", r#"
Inside a plain or multiline string, `\` starts an escape, and wolf
recognizes exactly these: `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, `\xNN`
(two hex digits), and `\u{…}` (one to six hex digits)
([gram.lex.str.escape]). Anything else after a `\` is an error rather
than passing through silently — a typo like `\d` almost always means a
regex or a Windows path ended up in the wrong kind of string. For a
literal backslash write `\\`; for text that should not be escaped at
all, use a raw string `r"…"`, which has no escapes.
"#);

code!(E0102, "unterminated string literal or interpolation", r#"
A plain `"…"` string must close before the end of its line — a newline
inside one is almost always a missing closing quote, so wolf ends the
string there, reports it once, and carries on lexing the next line
cleanly. If you meant the text to span lines, use a multiline `"""`
string, which closes at the next `"""`. The same recovery applies to a
format spec (`{value:…}`) left open at the end of a line, and to a
string still open when the file ends.
"#);

code!(E0103, "text after the opening `\"\"\"` on the same line", r#"
A multiline string's content starts on the line *after* the opening
`"""` — the opener must be the last thing on its line (SE-0168 lineage:
the layout is part of the literal). Text on the opening line has no
column to measure the margin against, so wolf rejects it. Move the text
down to the next line; the whitespace before the closing `"""` then
defines the margin stripped from every content line.
"#);

code!(E0104, "a multiline string line sits left of the margin", r#"
The whitespace before the closing `"""` is the *margin*: every content
line of the multiline string must start with exactly that whitespace,
which wolf strips when it builds the value ([gram.lex.str]). A line
indented less than the margin has bytes the margin would eat, so wolf
asks you to choose: indent the line to at least the margin, or move the
closing `"""` left to the shallowest content line. Blank lines are
exempt. This code also fires when the closing `"""` is not alone on its
line — its column *is* the margin, so it must stand alone.
"#);

code!(E0105, "margin tabs and spaces do not match the closing `\"\"\"`", r#"
Wolf compares a multiline string's margin byte-for-byte, never by
visual width: a tab in the margin of a content line matches only a tab
in the whitespace before the closing `"""`, and a space only a space.
Mixing them would make the stripped value depend on the reader's tab
width, so it is an error instead. Re-indent the flagged line with the
same tab/space mix as the closing delimiter's line — most editors fix
this with one select-and-reindent.
"#);

code!(E0106, "source bytes are not valid UTF-8", r#"
Wolf source files are UTF-8, nothing else ([gram.lex.source]) — no
latin-1, no UTF-16, no "mostly ASCII with a few stray bytes". The lexer
reports one error per run of invalid bytes and skips them, so the rest
of the file still lexes and later errors stay meaningful. Re-save the
file as UTF-8; if the bytes are intentional binary data, they belong in
a separate file loaded at runtime or a `\x…` escape, not raw in the
source.
"#);

code!(E0107, "a stray character that fits no token", r#"
This character cannot start any wolf token — commonly a `$` or `` ` ``
from another language's syntax, an invisible Unicode character pasted
from a web page, or a byte-order mark (wolf sources are BOM-less
UTF-8). Delete the character. A related case is a lone `}` inside a
string: `}` closes an interpolation there, so a literal closing brace
must be written `}}` (just as `{{` is a literal `{`).
"#);

code!(E0108, "string/interpolation nesting exceeds the lexer's 32-level rail", r#"
Strings and interpolations nest through one mode stack, and past 32
levels the lexer refuses to push further — this is a hard safety rail
against pathological or generated input, not a style limit. The
language's *intended* ceiling is 8 levels, enforced with E0007 and its
better advice; input deep enough to reach 32 is almost certainly
machine-generated or malicious. Hoist inner strings into bindings, or
generate simpler code.
"#);

code!(E0109, "unterminated raw or generalized string literal", r##"
A raw string `r"…"` (or a fenced one, `r#"…"#`) closes only at a quote
followed by the same number of `#` as it opened with; if the file ends
first, the fence never matched — check that the closing fence has
exactly as many `#` as the opener. A generalized literal like `re"…"`
must close with `"` before the end of its line, like a plain string;
its body is raw (no escapes), so a `\"` does not extend it — split long
bodies or use a fenced raw string.
"##);

// ------------------------------------------------------------------------
// E02xx — the parser's family.
// ------------------------------------------------------------------------

code!(E0201, "the parser expected a different token or construct here", r#"
The workhorse parse error: at this position the grammar required
something — a name, a `(`, an expression, a line end — and found
something else. The message names exactly what was expected and the
label points at where it should have been; the parser then inserts a
zero-width placeholder and continues, so one miss does not cascade into
a screenful. Fix the flagged spot first: later errors in the same
region may be echoes of this one.
"#);

code!(E0202, "an opening delimiter is never closed", r#"
A `(`, `[`, `{`, or `#[` was opened and its closing partner never
arrived — the error points at the *opener*, because that is where the
fix goes, with a note at the place the parser gave up looking. Inside
`(` and `[`, newlines do not terminate statements, so one lost `)` can
otherwise swallow every following line; the parser instead stops at the
next declaration keyword and reports the wreck once. If the code below
this error looks fine, trust the opener: count delimiters on the
flagged line.
"#);

code!(E0203, "expected a declaration at the top level", r#"
The top level of a wolf file (and the body of a `trait` or `impl`) is
declarations only: `fn`, `let`, `var`, `const`, `type`, `struct`,
`enum`, `trait`, `impl`, `use`, `import` — every one led by its keyword
([gram.item]). A bare expression or statement up here usually means a
function body's `{` went missing above, or a keyword was mistyped
(`fnn` for `fn` — the message suggests the fix when the typo is close).
A run of stray lines is reported once: fix the first flagged line and
the rest usually follow.
"#);

code!(E0204, "malformed attribute", r#"
An attribute is `#[name]`, optionally dotted (`#[pkg.name]`), with
arguments that are literals or nested attributes: `#[inline]`,
`#[repr(c)]`, `#[deprecated = "use other"]` ([gram.item.attr]).
Anything else inside the `#[…]` — stray operators, unbalanced parens, a
missing name — is malformed. Attributes attach to the declaration that
follows them, so a broken attribute is contained and the declaration
itself still parses.
"#);

code!(E0205, "malformed generic parameter list", r#"
Generic parameters are square-bracketed names with optional bounds:
`fn f[T](…)`, `struct Map[K: Hash + Eq, V] { … }`, and comptime value
parameters as `[N: type]` ([gram.item.fn]). Each parameter starts with
a name — nested brackets, literals, or operators cannot appear in
parameter position. Note that wolf uses `[]` for generics everywhere;
if you wrote `<T>`, the angle brackets parse as comparisons, and this
error (or E0201) is how that surfaces.
"#);

code!(E0206, "expected a type", r#"
Type position — after `:` in a binding or parameter, after `->`, inside
a type argument list — needs a type: a path like `int` or `pkg.Type`,
possibly applied (`List[int]`), or one of the prefixed forms `*T`,
`!T`, `shared T`, `handle T`, `weak T`, `distinct T`, `dyn Trait`,
`fn(…) -> T` ([gram.type]). The token found here cannot begin any of
those. If you deleted a type mid-edit, the `:` or `->` in front of it
is now dangling — remove it or complete the type.
"#);

code!(E0207, "expected a pattern", r#"
Pattern position — after `let`/`var`, after `for`, at the head of a
match or select arm — needs a pattern: a binding name, `_`, a literal,
a tuple `(a, b)`, a payload form `Tag(x)`, an `@` binding, or an
or-pattern joined with `|` ([gram.pat]). The token found here cannot
begin one. In `let` position this often means the binding name was
deleted or a keyword sits where the name should be (that case is E0008
with its own advice).
"#);

code!(E0208, "assignment used as an expression", r#"
Assignment is a *statement* in wolf: `x = y` stores into `x` and
produces no value, so it cannot sit inside a larger expression
([gram.expr.assign]) — `if (x = 5) …` is the classic bug this rule
exists to keep impossible. If you meant to compare, write `==`. If you
really meant to assign first, make it its own statement on the line
above, then use `x`. Chains like `a = b = c` are rejected as one
mistake, reported once.
"#);

code!(E0209, "negative integer literal used as an index", r#"
Wolf indexes count from the front only; there is no Python-style
negative indexing, because a silent `-1` wrapping to "last element" is
a classic off-by-one factory (D25). End-relative positions have their
own operator: `^n` counts from the end, so `s[^1]` is the last element
and `s[1..^1]` trims one from each side. Replace the `-` with `^` —
the suggested edit does exactly that. (If you meant to index with a
negative *computed* value, that is a bounds error at runtime; compute
the offset explicitly instead.)
"#);

code!(E0210, "a moded receiver outside receiver position", r#"
`(mut x)` and `(take x)` are receiver spellings, not expressions: they
exist so a call to a `mut self` or `take self` method names its
exclusive or consuming access at the call site — `(mut p).norm()` —
mirroring the argument modes of `f(mut x)` (X1). Detached from a
method call the mode marks nothing, so the grammar rejects it
anywhere a `.` does not follow the closing parenthesis. Delete the
mode to get a plain parenthesized expression, or complete the method
call the receiver was written for ([gram.expr.primary]).
"#);

// ------------------------------------------------------------------------
// E03xx — name resolution's family (s12).
// ------------------------------------------------------------------------

code!(E0301, "nothing with this name is in scope", r#"
Wolf could not find anything with this name: it is not a local binding,
a parameter, an item defined in this module (remember: every `.lu` file
in a directory is the *same* module), one of this file's imports, or a
prelude name. Most of the time this is a typo — when a near-miss exists
the message suggests it, and applying the suggested edit fixes the
program. If the name lives in another module, add the import: `use
that_module` at the top of the file, then reach it as
`that_module.name`. Names never resolve through types here — a
capitalized name used as an error-row tag (D30) is deferred to the type
checker rather than reported by this pass.
"#);

code!(E0302, "the same name is defined twice in one module", r#"
A directory is one module in wolf: every `.lu` file in it contributes
to a single shared namespace, and each top-level name may be defined
only once across all of them (D32). The second definition is the
flagged one; the other definition's location is shown alongside so you
can pick which to keep. Rename one of them, or delete one — file
boundaries do not create scopes, so moving a definition to a sibling
file changes nothing. If both definitions really are different things,
one of them probably belongs in its own subdirectory, which *is* a new
module.
"#);

code!(E0303, "modules import each other in a cycle", r#"
Imports between wolf modules must form a DAG — a module cannot depend
on itself through any chain of `use` declarations (D32). The message
draws the whole cycle (`a → b → a`) and points at every `use` that
participates, because the fix is rarely at the flagged line alone.
The standard cure is to extract the pieces both sides need into a
third module that each of them imports; the cycle disappears and the
shared surface gets a name. Acyclic imports are what make wolf's
module-parallel compilation and interface hashing possible, so there
is no escape hatch.
"#);

code!(E0304, "the item exists but is not visible from here", r#"
The name resolves — the module you imported really does define it —
but the item is private, and private is the default in wolf:
visibility is granted with a keyword, never guessed from a naming
convention. To use the item from another module, its definition needs
`pub` (exported to everyone) or `pub(pkg)` (visible only within this
package); the message names the visibility the access would need.
If the item is deliberately private, the module means to hide it —
look for the `pub` function it exposes instead.
"#);

code!(E0305, "this import is never used", r#"
Nothing in this file mentions the imported name, and an unused import
is a hard error in wolf, not a lint: imports are the bounded, honest
statement of what a file depends on, and dead ones rot fast (D32).
Delete the line — the suggested edit does exactly that, and `wolf fix`
can apply it unattended. Imports are file-scoped, so a name used only
in a *sibling* file must be imported there, not here. There is no
`import _` escape at v1; if you need an import purely for its side
effects, comptime registration (D29) is the sanctioned pattern.
"#);

code!(E0306, "an import that collides with another binding", r#"
Each imported name must be new in its file: importing the same name
twice (even from two different paths), importing something that a
module-level definition already claims, or importing the module you
are standing in all report this error. The colliding binding's
location is shown alongside. Drop the redundant import, or give one
side its own name with `use long.path as other_name`. `import c`
headers are exempt from the twice rule — every `import c` feeds the
same `c` namespace, so repeating it with a new header is the normal
form.
"#);

// ------------------------------------------------------------------------
// E04xx — the type checker's family (s13).
// ------------------------------------------------------------------------

code!(E0401, "the types do not match", r#"
The workhorse type error: an expression has one type, and the place it
sits requires another. Wolf tracks *why* the requirement exists — a
return type declared on the function, a parameter of the called
function, a `let` annotation, the other operand of an operator — and
the message points at that origin as well as the mismatch, so you can
decide which side is wrong. When an `if` or `match` is used as a value
and its branches disagree, neither branch is reported as "expected":
both are shown, because the fix may belong to either. For large types
the message names only the differing parts (a structural diff) instead
of making you eyeball two long renderings. Note that wolf never
converts numbers implicitly — `int` and `i64` are simply different
types, and the fix is an explicit `as` conversion.
"#);

code!(E0402, "wrong number of arguments in a call", r#"
The function exists and the call is well-formed, but the argument count
does not match the function's parameter list — every wolf parameter is
required (there are no optional or variadic parameters at v1), so a
call must pass exactly as many arguments as the signature declares.
The message shows where the function is defined so you can compare the
lists side by side. Passing too many arguments often means an argument
was meant for a different call; passing too few often means a value
was dropped while refactoring. Check the order too: a swapped argument
pair usually surfaces as a type mismatch on the *next* argument.
"#);

code!(E0403, "no such field", r#"
Member access resolved the value's type, but that type has no field
with this name. When a near-miss exists ("did you mean `radius`?") the
message suggests it — most unknown fields are typos. Otherwise the
struct's actual fields are listed, and the struct's definition site is
shown so you can check which version of the type you are holding.
Methods are looked up separately from fields (s17 method resolution):
if you meant to *call* something, the parentheses matter — `p.len` is
a field access, `p.len()` is a method call.
"#);

code!(E0404, "this would be an infinite type", r#"
Unification found a type that would have to contain itself — the
classic case is applying a function to itself (`f(f)`), which needs a
type `t = fn(t) -> …` that expands forever. No finite type satisfies
the constraint, so wolf stops and shows the cycle rather than looping.
This almost always signals a confusion one level up: a closure passed
where its *result* was meant, or a recursion through values where a
named recursive *type* (a struct or enum, which may mention itself by
name) was intended. Introduce a named type or an explicit annotation
at the point where the cycle closes.
"#);

code!(E0405, "the type here cannot be inferred", r#"
Nothing in the body pins this type down. Wolf infers freely *inside*
function bodies, but the information must come from somewhere: literal
types default by rule (`i32` for integers, `f64` for floats), closure
parameters take their types from the context the closure is checked
against, and everything else must flow from a use. A closure bound to
a plain `let` and never given context is the common case — annotate
its parameters (`fn(x: int) …`) or pass it directly to the call that
gives it a type. This is deliberately an error, not a guess: an
arbitrary default here would change meaning silently (D27).
"#);

code!(E0406, "this is not a function, so it cannot be called", r#"
Call syntax `value(args)` applies only to functions and closures, and
the thing being called here is neither — commonly a struct name (wolf
constructs structs with braces: `Point { x: 1, y: 2 }`, not
`Point(1, 2)`), a value that lost its function type to a shadowing
binding, or a field that holds data rather than a closure. The message
names the type the callee actually has. If you expected a method, note
that s13 checks free functions only; method calls resolve in a later
phase, and a field holding a closure *is* callable as `x.field(…)`.
"#);

code!(E0407, "an item is missing its type annotation", r#"
Items — functions, `const`s, module-level `let`/`var` bindings —
declare their full signatures in wolf; inference happens only *inside*
function bodies (D27). This is the separate-compilation firewall: a
module's interface must be readable from its signatures alone, so an
item's type may never depend on running inference over its body
(SE-0244 calls the alternative "a mistake"). The compiler may well
know the type — when the initializer makes it obvious, the suggested
fix states it — but you still write it down: the annotation is what
your callers, incremental rebuilds, and future readers depend on.
"#);

code!(E0408, "struct literal fields do not match the struct", r#"
A struct literal must initialize every field of the struct exactly
once — wolf has no field defaults and no partial construction, so a
missing field is an error here at the literal, and a repeated field is
an error on the second write. The message lists what is missing (or
flags the duplicate) and points at the struct's definition. If many
call sites want a "default" shape, the wolf pattern is an ordinary
function that builds one (`fn default_config() -> Config { … }`) —
explicit, checkable, and versioned with the type.
"#);

code!(E0409, "the operator does not work on this type", r#"
Each operator family in wolf works on a fixed family of types:
arithmetic (`+ - * / %`) and ordering (`< <= > >= <=>`) on numbers,
logic (`&& || !`) on `bool` exactly, bitwise and shifts on integer
types. This operand is outside the operator's family. Two classics:
wolf has no truthiness, so `if x` on a number must be written as a
comparison (`x != 0`); and `+` does not join strings — interpolation
does (`"{first}{second}"`), which formats any primitive and never
surprises you with a numeric `+` overload. Trait-based operators for
user types arrive with the trait engine (s14).
"#);

code!(E0410, "a `let` binding cannot be assigned again", r#"
`let` names a value once: the binding is immutable for its whole scope
(spec/01 `[gram.item.let]` — "`let` immutable, `var` mutable"), and
that covers plain assignment and every compound form (`=`, `+=`, `-=`,
…). A binding you intend to update is declared with `var` instead —
the fix-it offers exactly that edit. When the second value is really a
*new* thing rather than an update to the old one, the wolf idiom is
shadowing: a second `let x = …` introduces a fresh binding under the
same name without mutating the first. Function parameters and `match`
bindings are not `let` bindings; their mutability is governed by modes
(`mut`, `take`), not by this rule.
"#);

// ------------------------------------------------------------------------
// E05xx — traits, checked generics, and coherence (s14).
// ------------------------------------------------------------------------

code!(E0501, "the generic body uses something its bounds do not provide", r#"
The golden rule of wolf generics: a generic body is checked once,
against its declared bounds, and everything the body does with a type
parameter must be provable from those bounds alone — a call site can
then never fail inside the callee (D28). This body uses a capability —
a trait method, an associated type or constant, an operator, `==`, a
call — that no bound on the parameter provides. When a specific trait
would supply it, the message says which bound to add and where; apply
that edit and the body is provable again. Capabilities with no trait
behind them yet (arithmetic and comparison operators arrive with the
operator traits) cannot be granted by any bound today — for those,
take a concrete type instead of a generic parameter. The error always
lands here, at the definition, never as a backtrace out of some
instantiation.
"#);

code!(E0502, "a type argument does not satisfy the generic's bound", r#"
The generic function is fine — its body was proven against its bounds
at its definition — but this call instantiates it with a type that
does not satisfy one of those bounds: no impl of the named trait
exists for the argument type. The fix is at the call, never inside the
callee: pass a type that implements the trait, or write the missing
`impl Trait for Type` in the trait's module or the type's module
(coherence allows exactly those two homes). If the type is foreign and
the trait is foreign, the sanctioned escape is an adapter: declare
`type Local = distinct Foreign` and implement the trait for the
adapter — same layout, free casts, its own impl set.
"#);

code!(E0503, "this bound is not a trait", r#"
Only traits can appear as bounds (`T: Show`) — the name written here
resolves to something else: a struct, an enum, a type alias, a
function, or a module. A bound is a promise about capabilities, and
only traits define capability sets, so wolf rejects the bound rather
than guessing what constraint you meant. Check the spelling first (a
struct and the trait it implements often share a stem). This error
also fires when a bound or `dyn` names a trait that declares its own
input parameters: applying trait arguments inside a bound has no
surface syntax yet, so such traits cannot be used as bounds today —
use a trait without input parameters, or dispatch through qualified
calls instead.
"#);

code!(E0504, "an impl must live with its trait or with its type", r#"
Wolf's coherence rule keeps every `impl Trait for Type` findable and
unique: the impl must be written in the module that defines the trait
or in the module that defines the self type — nowhere else (the
"simple orphan rule", D28). An impl in a third module could collide
invisibly with someone else's, and which one wins would depend on who
happens to be compiled together; wolf refuses instead. Move the impl
into the trait's module or the type's module. When both are foreign —
you own neither the trait nor the type — declare an adapter in your
own module: `type Mine = distinct Theirs` has the same layout, casts
freely to and from its base, starts with an empty impl set, and you
may implement anything for it.
"#);

code!(E0505, "an impl header parameter is not covered by the impl", r#"
Every generic parameter of an impl must appear in the impl's subject —
inside the self type or the trait's arguments. A parameter that
appears nowhere (`impl[T] Show for Point`) is *uncovered*: no use of
the impl could ever determine what `T` is, so the impl could apply
infinitely many ways or none. Rust threads this needle with a covering
rule; wolf v1 simply disallows uncovered parameters outright — simpler
and more honest (D28). Delete the unused parameter, or make the impl
subject actually mention it.
"#);

code!(E0506, "two impls of the same trait overlap", r#"
Global coherence means one trait has at most one impl for any given
type — the program's behavior can never depend on which impl a
particular call happened to see. These two impl headers can describe
the same type (wolf checks by trial unification, so a blanket
`impl[T] Show for T` overlaps a specific `impl Show for Point` exactly
like two duplicates would), and wolf has no specialization: there is
no rule that could pick a winner, deliberately (D28 — locked). Delete
one impl, or narrow the blanket so the two sets of types are disjoint.
Overlap is judged on headers alone; bounds on the impls do not
disambiguate them.
"#);

code!(E0507, "the impl does not match the trait it implements", r#"
An `impl Trait for Type` must supply exactly what the trait declares:
every required method with the same signature (after substituting the
implementing type for `Self`), every associated type bound to a
concrete type, and every associated constant at its declared type.
This impl is missing a member, binds one at the wrong signature, or
defines a member the trait never declared — extra members do not
become part of the trait, because callers dispatch through the trait's
declaration, not through any particular impl. The message names the
member and shows the trait's declaration; make the impl agree with it.
"#);

code!(E0508, "the trait cannot be a `dyn` object: a generic method", r#"
A `dyn Trait` value carries a witness table — one function pointer per
method, fixed when the table is built. A generic method would need one
entry per instantiation, a set that is not known until every caller is
seen, so no finite table can represent it (the RFC 0255 model). The
message names the offending method. Either drop the method's generic
parameters (take a concrete type, or `dyn` of another trait, as the
parameter), split the trait so the dynamic part is generic-free, or
keep this trait static-only — generics over `T: Trait` have no such
restriction and are wolf's default dispatch.
"#);

code!(E0509, "the trait cannot be a `dyn` object: an unconstrained associated type or input escapes", r#"
This trait's methods mention an associated type (or the trait declares
input parameters), and a `dyn Trait` object erases exactly the
information that would pin those down: two objects behind the same
`dyn Trait` may answer `Self.Item` with two different types, so a
method signature that exposes it has no single ABI to dispatch
through. Wolf has no surface syntax yet for constraining an associated
type at a `dyn` spelling, so any escape makes the trait dyn-unsafe.
Keep the associated type out of the dynamic methods' signatures, split
the trait, or use static generics (`T: Trait`), where associated types
work fully.
"#);

code!(E0510, "the trait cannot be a `dyn` object: `Self` outside receiver position", r#"
Behind `dyn Trait`, the concrete type is erased — only the object
itself knows what it is. A method that takes another `Self` as an
ordinary parameter or returns `Self` by value would require the caller
to name the erased type, which is exactly what `dyn` gave up (the RFC
0255 self-position rule; the receiver itself is fine, because the
object supplies it). The message names the method. Replace the loose
`Self` with a concrete type or another `dyn Trait`, or keep this trait
to static generics, where `Self` is a known rigid type and all of this
checks.
"#);

code!(E0511, "a generic parameter cannot take type arguments", r#"
Wolf generics are rank-1 over *types*, not over type constructors:
a parameter `T` stands for one complete type, so applying it —
`T[int]` — asks for higher-kinded polymorphism, which wolf does not
have and v1 deliberately excludes (D28: the ceilings are spec'd, not
discovered). The checking cost of higher kinds is a proof search wolf
refuses to run; "the executed steps are in the source." Take the
applied type as its own parameter instead: where you wanted
`T[int]`, accept `U` and let the caller pass `List[int]` whole.
"#);

code!(E0512, "associated types cannot have their own generic parameters", r#"
An associated type inside a trait is an *output*: each impl binds it
to one concrete type. Giving it generic parameters of its own (`type
Item[X]`) would make it a generic associated type — a family of
outputs indexed by types — which wolf v1 deliberately does not have
(D28: no GATs; the ceilings are stated up front, Roc-style, rather
than discovered at the bottom of an error). Restate the trait so the
parameter lives on the trait itself or on the method that needs it;
both of those are plain rank-1 generics and check today.
"#);

code!(E0513, "associated-type bindings form a cycle", r#"
The associated types of this impl are defined in terms of each other —
following the bindings (`type A = Self.B`, `type B = Self.A`) never
reaches a concrete type. Wolf normalizes associated types by textual
rewriting to a fixed point, which is deterministic and always
terminates precisely because cyclic rule sets are rejected here
instead of being chased forever (Carbon's rewrite-constraint model,
D28). Bind at least one of the associated types in the cycle to a
concrete type and let the others build on it.
"#);

// ------------------------------------------------------------------------
// E06xx — error rows, `?`, `else`, `errdefer` (s15, D30).
// ------------------------------------------------------------------------

code!(E0601, "the error row is not well-formed", r#"
An error row is a *set* of payload-carrying tags, so each tag may appear
exactly once — `{Io(str), Io}` is rejected on the second `Io`, not
silently merged, because two entries for one tag would disagree about
its payload. The same rule keeps a row to at most one row variable (the
entry naming a generic parameter, the row's polymorphic tail): a row
extends exactly one tail. Delete the duplicate entry, or if the two
entries really are different failures, give them different tag names —
tags are structural, so any name you have not used yet is free.
"#);

code!(E0602, "the error row does not include this tag", r#"
An error can only flow where the receiving row expects it. This
failure carries a tag — raised directly, or propagated by `?` from a
callee's row — that the target row does not include. Rows compose by
union and widen automatically toward *larger* rows, so the fix is
almost always to extend the narrower row: the suggested edit adds the
missing tags to the signature. There is no `From`-style conversion in
`?` (deliberately — conversion in the operator makes inference
unsolvable); if you meant to *collapse* several failure kinds into one,
do it explicitly: handle the error (`else |err| …`) and raise your own
tag. Functions with an inferred row (`-> !T`, private only) never hit
this error — their rows grow to fit their bodies.
"#);

code!(E0603, "`?` needs a fallible operand", r#"
The `?` operator propagates the error of a `!T` value and unwraps its
ok half — but the expression it is applied to here cannot fail: its
type has no error row. A `?` on an infallible value would do nothing,
and wolf rejects dead operators rather than ignoring them, since a
stray `?` usually means the call you expected to be fallible is not
(check the callee's signature), or the value was already unwrapped by
an earlier `?` or `else`. Delete the `?`, or apply it to the fallible
call itself.
"#);

code!(E0604, "an error cannot leave a function with no error row", r#"
This function's signature has no error row — it promises to always
return normally — but the body raises or propagates an error (`?`, or
an error-tag return). Errors are values that travel in the declared
row, never an invisible side channel, so the signature must admit the
failure. Make the function fallible: write `-> !T` (private functions
infer their row from the body) or state the row explicitly with
`-> T ! {Tag, …}`; or handle the error here instead — `else` with a
default, or `else |err| …` — so nothing needs to escape.
"#);

code!(E0605, "an exported function must state its error row", r#"
Inferred rows (`-> !T`) are legal for module-private functions only:
the compiler seals the row from the body, and no one outside the
module ever depends on it. An exported (`pub`/`pub(pkg)`) signature is
a contract other modules rebuild against, so its failure set must be
stated in the interface, not derived from a body that can drift —
Zig's inferred error sets show how an inferred public surface breaks
recursion, function pointers, and target independence. The message
names the sealed row the body implies; the suggested edit writes
exactly that row into the signature (or drops the `!` when the body
cannot fail at all).
"#);

code!(E0606, "the payloads of a shared error tag do not match", r#"
Two rows share a tag name, but disagree about what the tag carries —
`NotFound(Path)` cannot propagate into a row expecting `NotFound(str)`,
and a raise of `Bad(int, int)` does not fit a row declaring `Bad(int)`.
Tag names are structural, so the same name in two signatures is *the
same tag*, and its payload types must agree everywhere it appears
(propagation re-tags by injection — a bit-level move, never a
conversion). Align the payload types across the signatures, or give
the two failures different tag names if they are genuinely different
shapes.
"#);

code!(E0607, "`errdefer` only runs in a function that can fail", r#"
`errdefer` schedules cleanup for the *error path*: it runs only when
the function exits by returning an error, interleaved with `defer` in
reverse declaration order. In a function with no error row there is no
error path, so this `errdefer` could never run — wolf rejects dead
cleanup rather than silently keeping it. Use plain `defer` if the
cleanup should run on every exit; keep `errdefer` and make the
function fallible (`-> !T` or an explicit row) if this function really
can fail.
"#);

code!(E0608, "`else` defaulting needs a fallible operand", r#"
Postfix `else` is the defaulting operator: it takes a `!T` value and
either substitutes the fallback or hands the error to a `|err|`
handler. The expression to its left cannot fail, so there is nothing
to default — the `else` would never fire. This usually means the
fallible call was already unwrapped (an earlier `?` or `else`), or the
callee is not actually fallible. Delete the `else`, or attach it to
the fallible expression itself. (An `else` completing an `if` is a
different construct — this message is about `expr else fallback`.)
"#);

// ------------------------------------------------------------------------
// E07xx — comptime / CTFE (s16, D29 + D33). The sandbox family: every
// refusal names its reason, every budget is finite, and the witness
// line for const-generic equality is documented in the diagnostic.
// ------------------------------------------------------------------------

code!(E0701, "comptime code reached for ambient IO", r#"
Comptime evaluation is hermetically sandboxed (D33): no filesystem, no
network, no environment variables, no clock, no randomness, no FFI —
the intrinsics available at compile time are an explicit allowlist,
and nothing ambient is on it. Each refusal names its category and its
reason: confinement (compiling a package must never act on or read
the machine that compiles it — `wolf add` must never mean arbitrary
code runs with your credentials) or determinism (the same program and
target must produce bit-identical comptime results on every host).
Compute the value at runtime instead; file contents will later arrive
as *declared build inputs* through the build system (s51), never as
an evaluator capability.
"#);

code!(E0702, "comptime evaluation ran out of fuel", r#"
Every comptime evaluation runs under an instruction budget, so a
runaway computation ends in this report instead of a hung build — the
budget also bounds comptime as an attack surface (D33). The
diagnostic carries the comptime call backtrace, so the loop or
recursion that burned the fuel is visible. If the computation is
genuinely that large, raise the budget at the call site with
`#[budget(fuel = N)]` — budgets have defaults, per-site overrides,
and a hard ceiling; no spelling disables one.
"#);

code!(E0703, "comptime evaluation exceeded its heap budget", r#"
Comptime code allocates values in a compiler-owned arena with a hard
cap, so evaluation can never exhaust the machine compiling the
program (D33). Most overruns are unbounded value growth inside a
loop — each iteration building a strictly larger value. The
diagnostic points at the allocation that crossed the cap with the
comptime backtrace attached. If the computation legitimately needs
more, raise the cap at the call site with `#[budget(heap = N)]`;
like all comptime budgets it has a hard ceiling and cannot be turned
off.
"#);

code!(E0704, "comptime evaluation recursed too deeply", r#"
The comptime evaluator keeps its own explicit call stack, so deep
recursion is a *resource limit* with a report, never a compiler crash
(D33). The default depth accommodates ordinary recursive folds; an
overflow usually means the recursion is missing its base case — the
backtrace shows the repeating frame. If the depth is intentional,
raise it at the call site with `#[budget(depth = N)]`, up to the
hard ceiling; consider an iterative shape instead, which spends fuel
rather than frames.
"#);

code!(E0705, "this value is not comptime-known", r#"
A `comptime fn` runs during compilation, so every argument must be
known at compile time: a literal, a `const`, a type, or the result of
another comptime call. A runtime `let`/`var` local, a runtime global,
or a self-referential `const` cannot cross into comptime position —
the evaluator will not guess at a value the program has not produced
yet. Bind the value with `const`, pass a literal, or move the
computation to runtime if the input genuinely arrives at runtime.
"#);

code!(E0706, "comptime arithmetic faulted", r#"
Checked arithmetic has exactly one semantics everywhere (X3): an
operation that would trap at runtime — overflow past the declared
width, division or remainder by zero, an out-of-range shift — is a
compile error when it happens at comptime, at the declared widths of
the declared target, never the host's. Intended wraparound is spelled
in the type system as `wrapping[T]`, and wraps identically at
comptime; there is no flag, profile, or mode that changes any of
this. Fix the computation, widen the type, or spell the wraparound.
"#);

code!(E0707, "const-generic equality needs a witness", r#"
Const-expression equality in generic position is decided in three
steps, and the line between them is fixed: (1) closed expressions
fully evaluate and compare by value; (2) linear `+`/`-` arithmetic
over generic parameters compares by ring normalization — `N + 1`
equals `1 + N`, killing the Rust RFC-2000 identity-only wart at a
defined line; (3) anything beyond linear — `*`, `/`, `%`, shifts, bit
operators — is compared only by identical spelling, and differing
spellings require an explicit witness. This error is step 3 firing:
the two forms may well be equal, but the compiler will not run a
decision procedure it cannot bound. Rewrite both sides into the same
`+`/`-` form, or assert the equality where the reader can see it.
"#);

code!(E0708, "layout is unresolved until codegen", r#"
Sizes and offsets are decided when codegen lays types out (c05), not
by the type checker — so `size_of` at comptime answers only for
fixed-width primitives today, and `typeinfo` describes fields without
offsets. This is a staging rule, not a permanent refusal: when layout
lands, the same intrinsics answer for aggregates, and code written
against them starts compiling without change. Until then, compute
from the primitive widths, or defer the computation to a later phase
that has layout in hand.
"#);

code!(E0709, "invalid comptime budget attribute", r#"
`#[budget(fuel = N, heap = N, depth = N)]` raises the evaluation
budgets for one call site. Every budget has a default and a hard
ceiling, and none can be disabled — a zero value, a value beyond the
ceiling, or a key that is not a budget is rejected here (the bounded
evaluation guarantee is part of the D33 sandbox, so there is
deliberately no spelling that removes a limit). Use one of `fuel`,
`heap`, or `depth` with a positive integer at or below the ceiling.
"#);

code!(E0710, "a comptime assertion failed", r#"
`assert` inside comptime evaluation checks a fact during compilation
and stops the build when the fact does not hold — it is the witness
mechanism for properties the checker cannot see on its own, such as
const-generic equalities beyond the linear line (E0707) or invariants
of reflected type shapes. The diagnostic points at the failing
assertion with the comptime call backtrace attached. Make the
asserted condition true, or delete the assertion if the invariant was
wrong.
"#);

// ------------------------------------------------------------------------
// E08xx — sema completion's family (s17): patterns & exhaustiveness,
// method resolution & receivers, the closed cast set.
// ------------------------------------------------------------------------

code!(E0801, "this `match` does not cover every case", r#"
A `match` used in wolf must handle every value its scrutinee can be —
there is no implicit fall-through and no runtime "no arm matched"
error, so the checker proves coverage up front and names concrete
values that slip past every arm ("`Timeout` not covered", "not
covered: `2`"). Arms with `if` guards do not count toward coverage:
a guard can be false, so only unguarded arms prove anything. Add arms
for the listed witnesses, or end the `match` with a `_` arm (or a
binding) to catch the rest deliberately.
"#);

code!(E0802, "this `match` arm can never match", r#"
The arms before this one already cover every value its pattern
accepts, so the arm is dead: its body will never run, which usually
means an arm is out of order, a pattern is broader than intended, or
a case was written twice. The diagnostic points at the earlier arm
that swallows this one. Delete the unreachable arm, or reorder the
arms so the more specific pattern comes first. (This is a warning:
the program still compiles and its meaning is unchanged.)
"#);

code!(E0803, "more than one trait in scope provides this method", r#"
`recv.method(…)` resolves through the traits in scope, and two or
more of them declare a method with this name that the receiver's type
implements — wolf will not pick one by precedence, because trait
namespaces are isolated by design (D28) and a silent winner would
change meaning when imports change. Say which trait you mean with the
qualified form the suggestion offers: `Trait.method(recv, …)`. The
qualified call is always available and never ambiguous.
"#);

code!(E0804, "the receiver's mode disagrees with the method's declaration", r#"
A method declares how it takes `self` — `mut self` needs exclusive
access, `take self` consumes the value — and the call site must say
so where the reader can see it, exactly like argument modes (X1):
`(mut p).norm()`, `(take conn).close()`. A bare receiver calls only
`read self` methods; conversely, a `read self` method takes no mode.
Wrap the receiver in the declared mode — the suggested edit inserts
`(mut …)`/`(take …)` for you — or drop the mode the method does not
ask for. Whether the access is actually exclusive is checked by the
memory tiers (c04); this rule is the syntax law only.
"#);

code!(E0805, "this `as` cast is outside the cast set", r#"
`as` converts within a closed set: between numeric types (integers,
floats, `wrapping[T]` — explicitly, since wolf never converts numbers
implicitly), and between an adapter type (`type X = distinct B`) and
its base, which share a layout so the cast is free both ways. Nothing
else casts: `as` is not a parser of strings, not a truthiness bridge
from `bool`, and not a reinterpretation of unrelated types. Build the
value you need with the operation that names it — interpolation for
strings ("{x}"), a comparison for `bool` (`x != 0`), a constructor or
conversion function for everything else.
"#);

code!(E0806, "a refutable pattern where matching cannot fail", r#"
`let`, `var`, `for`, and parameters bind unconditionally — there is
no "else" branch there, so their pattern must accept every value of
the initializer's type. A pattern that can *fail* to match (a
literal, an enum variant, an error-row tag) needs somewhere for the
other values to go: that place is `match`. Move the test into a
`match` (or an `if` on the value), keeping only irrefutable patterns
— names, `_`, and tuples of those — in binding position.
"#);

code!(E0807, "the method exists, but its trait is not in scope", r#"
Method calls resolve through the traits *in scope* — defined in this
module or brought in with `use` — so an implemented method still does
not resolve when its trait was never imported: visible resolution is
what keeps a new dependency from silently changing what `.method()`
means (D28). The suggestion adds the `use` for the one trait that
declares this method; after that the call resolves normally. The
qualified form `Trait.method(recv, …)` works too, and needs the same
import.
"#);

code!(E0808, "the pattern does not fit the shape of the value", r#"
A pattern mirrors the value it deconstructs, piece for piece: an enum
variant or error tag with a payload is matched as `Name(pat, …)` with
exactly as many sub-patterns as the payload has parts, a payload-less
one as bare `Name`, and a tuple pattern needs the scrutinee to be a
tuple of that width. This pattern binds a different number of pieces
than the value carries, so it can never be checked against it. Match
the declared shape — the diagnostic names it — adding `_` for pieces
you do not need.
"#);

// ------------------------------------------------------------------------
// E1xxx — the memory tier (c04, spec/02). s18 registers the Tier-0
// value/exclusivity codes; s19 the region-inference codes (E1004
// conflicting placement, E1010 escape of region-local data); s20 the
// region-checker codes (E1005 transfer/freeze of an open region,
// E1011 multiopen antichain violation, E1012 write through frozen
// data); s21 the shared tier's E1006 (strong `shared` cycle at the
// type level — acyclicity is what lets RC drops skip cycle detection
// forever, [mem.ub.defined]); s22 the unsafe tier's E13xx sub-family
// (below, after E1012). The
// dynamic counterparts are normative: E1001 ⇄ trap(use-after-move),
// E1002 ⇄ trap(exclusivity), E1004/E1005/E1010 ⇄ region-fault per
// [conf.trap.map] — the interpreter checks at runtime what these
// codes prove statically. E1011 and E1012 pair with region-fault
// rules the dynamic machine already enforces ([mem.region.multiopen],
// [mem.region.freeze.1]); their [conf.trap.map] rows ride the s23
// conformance sprint.
// ------------------------------------------------------------------------

code!(E1001, "this value was moved away (or never given one) before this use", r#"
In wolf, assignment and argument passing *move* a value: after
`let b = a` or `f(take a)`, the name `a` no longer holds anything —
its value went to the new place, whole. Reading a moved-from (or
never-initialized) name would read nothing, so the checker stops it
here and points at the move it happened in. Moves are field-granular:
moving `s.a` away leaves `s.b` usable, and only the moved path is
off-limits. To keep using the original, make the duplication explicit
where the move happens — `copy a` produces an independent value of
any type — or give the name a new value first: assigning to a
moved-from place makes it live again.
"#);

code!(E1002, "this needs exclusive access, but the value is in use here", r#"
While a value is passed `mut`, that call is the only way to touch it:
`mut` means "mine alone for the whole call", so no other argument of
the same call may read or write the same place, or any path that
contains it ([mem.tier0.excl]). Distinct fields are distinct places —
`f(mut p.x, mut p.y)` is fine — but `f(mut p, p.x)` is not, because
`p.x` lives inside `p`. Split the call so the uses happen one after
the other, pass disjoint fields instead of the whole value, or let
the callee say what it really touches with a view set
(`mut self.{x, y}`), which frees the caller to use the rest.
"#);

code!(E1004, "this value is placed in one region, but needed in another", r#"
Every allocation lands in exactly one region — the innermost
enclosing `region`/`in` block, else the caller's region — and it
stays there for its whole life: moving a value never relocates its
storage. Embedding a value into an aggregate that lives somewhere
else would therefore create a reference between two regions, which
safe wolf does not allow (one region could be freed while the other
still points in). The diagnostic marks where each side was allocated;
make the two placements one: build the value inside the same
`region`/`in` block as its container, or `copy` it — a copy is a
fresh allocation in the ambient region, so it lands where the
container lives. When the two sides are different *parameters*, their
regions are independent by default (that independence is what lets
callers pass arguments from anywhere without annotations), and the
same two fixes apply.
"#);

code!(E1005, "the region is open here, so its handle cannot move or freeze", r#"
A region transfers as a closed subtree only: while a `region` block
or `in` window is open, the region's affine value — its handle — is
pinned in place, because the open window *is* a live borrow of that
handle. Moving it, freezing it, sending it, or lending it `mut`
while inside would leave the window standing on a region that
belongs to someone else (or to nobody). The same rule covers a
region whose *child* region is still open: the forest moves as
closed subtrees, never around an open window. End the `region`/`in`
block first and transfer after, or transfer first and open on the
receiving side.
"#);

code!(E1006, "this type's `shared` references form a strong cycle", r#"
`shared T` is reference-counted: the value is freed the moment its
last strong reference drops. A cycle of strong references keeps
itself alive forever — every cell waits on the others — and wolf has
no cycle collector, because a leak is not an answer either. So strong
`shared` edges must form a DAG, checked right here at the type
definition ([mem.shared.rc.2]). Break the cycle at its back-edge:
make that field `weak T` (upgrade to reach the value, it does not
keep it alive) or `handle T` (a generational index that faults if the
target is gone). If the structure is genuinely cyclic — a graph, a
doubly-linked list — keep the whole structure inside one region
instead: intra-region cycles are safe and free
([mem.region.intra.1]), and the region frees them wholesale.
"#);

code!(E1007, "the argument's mode does not match the parameter's", r#"
A parameter's mode is part of the deal between caller and callee, and
wolf makes the caller spell it at the call site (X1): a `mut`
parameter is written `f(mut x)` — the reader sees the mutation — and
a `take` parameter is written `f(take x)` — the reader sees the value
leave. This argument's spelling disagrees with the declaration: a
mode is missing, or written where the parameter does not ask for one.
The suggested edit inserts or removes the mode word at the argument;
the parameter's declaration is marked so you can decide which side is
wrong.
"#);

code!(E1008, "the method touches a field outside its declared view", r#"
`fn norm(mut self.{x, y})` is a promise: of all of `self`, this
method touches only `self.x` and `self.y`. Callers lean on that
promise — it is what lets them keep using `self.z` while the call
runs — so a use of a field outside the view set (or of `self` whole)
would quietly break every call site. Add the field to the view set if
the method genuinely needs it, or drop to plain `mut self` to claim
the full value — both change the signature, which is exactly where
that decision belongs.
"#);

code!(E1009, "a `mut` argument needs a place, not a temporary", r#"
`mut` lends a location out to be written, so the argument must *name
a location* the caller can see again afterwards: a variable, a field
path like `p.x`. A temporary — `f(mut 1 + 2)`, `f(mut g())` — has no
such location: the callee's writes would vanish with it, which is
never what the call meant. Bind the value first (`var t = …`, then
`f(mut t)`), or pass the expression plainly if the callee only needs
its value. (`take` of a temporary is fine — consuming a value nobody
else owns needs no location.)
"#);

code!(E1010, "the value's region is freed while the value is still needed", r#"
A region dies as a unit: when a `region` block ends (or a region
value's scope does), every allocation in it is freed wholesale — that
is the whole deal, one free instead of thousands. This value is
allocated in such a region, but something that lives longer still
holds it: an outer binding, the function's result, or module state.
After the free, that holder would point at nothing. Keep value and
region together: build the value outside the region block so it lands
in the caller's region, aim the allocation at a longer-lived region
explicitly (`let r = region()` … `in r { … }`), or widen the region
block so it covers every use. Note that `copy` inside the block does
not help — a copy is a fresh allocation in the *current* ambient
region, which is still the dying one. `freeze` (making the whole
region immortal and immutable) and `shared` (counted escape) are
coming in later tiers for the cases that genuinely need to outlive
the region.
"#);

code!(E1011, "this would open a region while a region that contains it is open", r#"
Any number of regions may be open at once, provided none of them
contains another: the open set must be an antichain in the region
forest ([mem.region.multiopen]). Sibling regions have disjoint data,
so mutating through both windows at once is safe — but an owner's
window already reaches everything its child region holds, so opening
the child (or the owner) while the other is open would put one
location behind two live mutable windows. The diagnostic marks both
open sites. Close the first block before opening the second, or
restructure so the two regions are siblings — neither stored inside
the other — and open them together freely.
"#);

code!(E1012, "frozen data cannot be written", r#"
`freeze` consumes a region and promotes everything in it to `imm`:
deeply immutable, shareable from anywhere — across threads, without
synchronization — and readable forever. That deal is permanent, and
it is why frozen data needs no locks and no lifetimes; a single write
anywhere would break every reader everywhere. This write reaches data
that a `freeze` already promoted (the freeze site is marked). Do the
mutation before freezing — build the value completely, freeze last —
or keep a mutable `copy` alongside the frozen original for the part
that must keep changing.
"#);

// ------------------------------------------------------------------------
// E13xx — the unsafe tier (s22, spec/02 §5–§7, D11). The raw tier's
// rules are deliberately *simpler* than the safe tier's (the
// anti-Stacked-Borrows posture): raw pointers are inert data anywhere,
// and only the tier's *operations* — deref, write, provenance ops,
// pointer casts, `assume`, the re-entry doors, C calls — demand the
// `unsafe` ring. These codes gate the surface statically; the UB the
// operations can cause is dynamic by design ([mem.ub] rows P1–P6/L/T/C,
// checked by s23's miri-lite and the is04 oracle), so a program these
// codes accept may still be `ub(mem.ub)` at run time — that split is
// the tier's contract, not a gap.
// ------------------------------------------------------------------------

code!(E1301, "this raw-tier operation needs an `unsafe` block", r#"
Raw pointers themselves are inert values: creating, copying, storing,
and passing them is free in safe code (creation is not a use). What
the safe tier cannot contain are the raw tier's *operations* — reading
or writing through a pointer, pointer casts, provenance operations
(`addr`, `with_addr`, `expose`, `with_exposed`), `assume noalias`,
`borrow … from …`, and calls into imported C. Each of those can reach
behavior the safe tier's guarantees do not cover, so each one lives
inside the `unsafe { }` ring, where the enclosing module carries the
proof obligation. Wrap the operation in an `unsafe` block — the rules
inside are *simpler* than the safe tier's, not stricter — and state
the invariant the block maintains in a `# Safety:` comment.
"#);

code!(E1302, "a raw pointer type cannot cross this boundary", r#"
Unsafety never appears in types crossing function boundaries: every
function signature is fully safe, and there are no `unsafe fn`s — the
proof obligation is discharged at the `unsafe` block, and the module
is the audit granule. A `*T` in a parameter or return type, or in an
exported type's fields, would silently spread the raw tier through
every caller's audit surface. Keep the pointer inside: pass a `handle`
(revalidated at every access) or a region value instead, or hold the
`*T` in a module-private field where the module's own invariants —
and its `unsafe` blocks — can vouch for it.
"#);

code!(E1303, "this module holds `#[trusted]` code the manifest does not declare", r#"
`#[trusted]` marks code whose unsafe blocks assert invariants the
checker cannot see — allocator internals, pinned FFI regions. The deal
that keeps that auditable is declaration: every module containing
`#[trusted]` functions must be listed in the package manifest's
`trusted` entry, so a dependency growing new trusted code is a visible
diff, not a silent one (`wolf audit` reads exactly this roster). Add
the module to the `trusted` list in `wolf.pkg`, or remove the
`#[trusted]` attribute if the code no longer asserts unseen
invariants.
"#);

code!(E1304, "`assume noalias` needs raw pointers to assume about", r#"
`assume noalias p, q` asserts that the ranges reachable through two
*raw pointers* do not overlap, for the assertion's scope — it is the
one way to hand the optimizer an aliasing fact the raw tier otherwise
refuses to guess, and a false assertion is UB (checked dynamically).
An operand that is not a raw pointer has nothing to assert: safe
values already carry stronger, checked aliasing facts (`mut` is
exclusive, `read` is frozen). Pass the `*T` values themselves, or
drop the `assume` — safe code never needs it.
"#);

code!(E1305, "this door needs a region and a raw pointer, in that order", r#"
`borrow r from p` is one of exactly two doors from the raw tier back
into the safe world: it asserts that `p` points into region `r`'s
live allocation and yields a safe value governed by `r`'s rules from
then on. The claim only makes sense with a `region` value on the left
and a raw pointer (`*T`) on the right — anything else has no
allocation to check the claim against. Pass the region the pointer
really points into, or use the other door: launder the raw index
through a checked `handle`, which re-validates its generation at
every access.
"#);

// ------------------------------------------------------------------------
// E14xx — the checked-execution family (s23): verdicts of the miri-lite
// UB machine, reported when `--checked` runs a program dynamically.
// ------------------------------------------------------------------------

code!(E1401, "undefined behavior detected by the checked-build UB machine", r#"
The `--checked` execution machine (the miri-lite UB checker) ran this
program against the operational memory model and reached a state the
spec's closed UB enumeration names: every finding cites its `[mem.ub]`
row (P1-P6, L1, L2, T1), the raw-tier operation responsible, and the
licensed optimization the D2 pairing attaches to that row — the
transformation compiled code is entitled to make, which is exactly why
the unchecked behavior is undefined rather than merely wrong. The
static tier accepts this program by design: raw pointers carry no
statically-checkable aliasing claims, so the unsafe tier's obligations
are discharged dynamically, here or by the independent is04 oracle.
Fix the operation the finding points at (the second span shows the
provenance it violates); the near-miss corpus files show the closest
defined shape for each row.
"#);

// ------------------------------------------------------------------------
// W03xx — the formatter's family (s11).
// ------------------------------------------------------------------------

code!(W0301, "file only partially formatted: syntax errors present", r#"
`wolf fmt` formats through the resilient parse tree, so a file with
syntax errors still mostly formats: every well-formed declaration and
statement is laid out canonically, while the regions the parser could
not understand — plus one statement of margin on each side — pass
through byte-for-byte untouched, so half-typed code is never mangled.
This warning marks that partial result, and `wolf fmt` exits nonzero
so scripts and editors know the file is not fully canonical yet. Fix
the syntax errors it reports alongside this warning and run `wolf
fmt` again; with a clean parse the whole file formats and the warning
disappears.
"#);

// ------------------------------------------------------------------------
// W13xx — unsafe-tier style lints (s22). Advisory, never load-bearing.
// ------------------------------------------------------------------------

code!(W1301, "this `unsafe` block does not state its invariant", r#"
Every `unsafe` block discharges a proof obligation the checker cannot:
some invariant, maintained by this module, makes the raw-tier
operations inside defined. The reader auditing the module needs that
invariant written down, next to the block that relies on it — the
convention is a `# Safety:` comment immediately above the block (or on
its first line) stating what must hold and why it does. This is a
style lint, not a gate: the block still checks and compiles. Add the
comment; future auditors — including `wolf audit` — read the rings by
exactly these markers.
"#);

}

#[cfg(test)]
mod tests {
    use super::*;

    /// Count the sentences in a prose block (rough: `.`-terminated).
    fn sentence_count(text: &str) -> usize {
        text.split('.')
            .filter(|s| s.split_whitespace().count() >= 3)
            .count()
    }

    #[test]
    fn codes_are_unique_and_well_formed() {
        let mut seen = std::collections::BTreeSet::new();
        for info in REGISTRY {
            let c = info.code.as_str();
            assert!(seen.insert(c), "duplicate code {c}");
            let (fam, num) = c.split_at(1);
            assert!(fam == "E" || fam == "W", "bad family for {c}");
            assert!(
                num.len() == 4 && num.bytes().all(|b| b.is_ascii_digit()),
                "code {c} is not X####"
            );
        }
    }

    /// The placeholder detector: an explanation is real prose — at
    /// least two sentences and a real summary — or registration fails.
    /// Stub explanations are a CI failure by design (s10 acceptance).
    #[test]
    fn explanations_are_not_stubs() {
        for info in REGISTRY {
            let c = info.code.as_str();
            assert!(
                !info.summary.trim().is_empty() && info.summary.len() >= 10,
                "{c}: summary is a stub"
            );
            assert!(
                info.summary.trim() == info.summary,
                "{c}: summary has stray whitespace"
            );
            let words = info.explanation.split_whitespace().count();
            assert!(
                words >= 30,
                "{c}: explanation too short to be real prose ({words} words)"
            );
            assert!(
                sentence_count(info.explanation) >= 2,
                "{c}: explanation must be at least two sentences"
            );
            let lower = info.explanation.to_lowercase();
            for stub in ["todo", "tbd", "fixme", "write me", "explanation goes here"] {
                assert!(!lower.contains(stub), "{c}: explanation contains `{stub}`");
            }
        }
    }

    #[test]
    fn explain_looks_up_by_text() {
        assert_eq!(explain("E0101").expect("registered").code, codes::E0101);
        assert!(explain("E9999").is_none());
        assert_eq!(codes::E0101.to_string(), "E0101");
        assert_eq!(format!("{:?}", codes::E0101), "E0101");
    }

    /// Every code the lexer and parser document is registered — the
    /// migration left nobody behind.
    #[test]
    fn frontend_codes_all_registered() {
        for c in [
            "E0001", "E0002", "E0003", "E0005", "E0006", "E0007", "E0008", "E0101", "E0102",
            "E0103", "E0104", "E0105", "E0106", "E0107", "E0108", "E0109", "E0201", "E0202",
            "E0203", "E0204", "E0205", "E0206", "E0207", "E0208", "E0209",
        ] {
            assert!(explain(c).is_some(), "{c} missing from the registry");
        }
    }
}
