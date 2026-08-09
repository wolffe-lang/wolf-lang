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
//! lexer, `E000x` spec/01 §9 reservations + `E02xx` parser, `E1xxx+`
//! sema and beyond (s13+). Warnings are `W####`. Retiring a code
//! retires its number forever.

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
