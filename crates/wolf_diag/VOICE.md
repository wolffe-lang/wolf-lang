# The wolf diagnostic voice

Error messages are the language's UI — most users meet wolf through a
diagnostic long before they meet the reference manual (report 05). This
guide is reviewed like code: a PR that adds or changes a diagnostic
quotes the rule it leaned on, and snapshot review is where the voice is
enforced.

The register is Elm's: a knowledgeable colleague looking over your
shoulder — plain, specific, never scolding, never showing off compiler
internals. The layout is Rust's RFC 1644: the machine shows its work on
your own source. Wolf uses both.

## The rules

1. **Full sentences, addressed to a person.** The message is one
   complete sentence about *the program*, in second person where a
   person acts ("move it up", "pick another name") — never about the
   parser's feelings ("unexpected token") and never in compiler-ese
   ("ILLEGAL_IDENT"). Headline messages start lowercase and take no
   trailing period (the header line completes them); labels are short
   fragments; notes and help are full sentences with periods.

2. **No jargon before its definition.** A term of art may appear only
   if the message defines it in passing: not "margin mismatch" but
   "the whitespace before the closing `\"\"\"` is the margin; this line
   doesn't start with it." If defining the term takes more than a
   clause, the note explains and the message stays plain.

3. **A concrete next step, always.** Every diagnostic offers at least
   one of: a suggestion with a machine-applicable edit, a `help:` with
   the fix spelled out, or a note that says what to do — "delete the
   `;`", "write `}}` for a literal `}`", "indent this line to match".
   Naming the rule broken is not a next step.

4. **"Did you mean `fn`?" beats "expected one of {…}".** When the typo
   machinery finds a near-miss, lead with it and attach the edit.
   Token-set dumps are a parser's stack trace, not a message; if there
   is no good guess, say what the *construct* needed ("a declaration
   starts with its keyword: `fn`, `let`, `struct`, …"), not the raw
   FIRST set.

5. **Primary label = what, secondary label = why.** The primary span
   points at the mistake ("the string opens here and runs to the end of
   the line"); secondary spans point at the reason it *became* a
   mistake ("the margin is set here", "the `(` was opened here"). One
   root cause, one diagnostic — cascade suppression is part of the
   voice.

6. **Code fragments wear backticks.** `fn`, `}}`, `s[^1]` — always.
   Spec references (`[gram.amb.else]`, D25) may appear in notes, never
   in the headline.

## Worked examples (calibrated from report 05 §D22)

**1. The generic expect-miss** — say what the construct needed, and
where.

> Before
> ```
> error: unexpected token `)` (expected one of: IDENT, `mut`, `take`, `self`)
> ```
>
> After
> ```
> error[E0201]: this parameter list is missing a parameter name
>   --> srv.lu:3:14
>    |
>  3 | fn serve(port: , host: str) { }
>    |               ^ a parameter looks like `name: type`
> ```

**2. The typo** — edit distance turns a token dump into a fix.

> Before
> ```
> error: expected declaration, found identifier `fnn`
> ```
>
> After
> ```
> error[E0203]: `fnn` is not how a declaration starts — did you mean `fn`?
>   --> main.lu:1:1
>    |
>  1 | fnn broken() { 1 }
>    | ^^^
> help: replace `fnn` with `fn`
>    |
>  1 | fn broken() { 1 }
>    |
> ```

**3. The action-at-a-distance error** — the secondary span carries the
*why*, so the user stops re-reading the flagged line.

> Before
> ```
> error: expected expression, found keyword `fn` at line 7
> ```
>
> After
> ```
> error[E0202]: this `(` is never closed
>   --> main.lu:5:12
>    |
>  5 |     let x = f(1, 2
>    |              ^ opened here
>  7 | fn next() { }
>    | - the parser expected the closing `)` by here
>    |
>    = note: inside `(…)`, line ends do not terminate statements, so an unclosed
>      `(` swallows the lines after it. Add the `)` and the errors below this
>      one will likely disappear.
> ```

## Litmus

Read the message aloud to someone who has used wolf for a week. If you
had to explain a word in it, rule 2 failed. If they ask "so what do I
do?", rule 3 failed. If the message described the parser instead of
their program, rule 1 failed.
