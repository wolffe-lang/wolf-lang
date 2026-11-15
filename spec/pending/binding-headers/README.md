# Pending binding-header conformance

These ten programs specify `[gram.expr.if.head]`; they are **expected to
fail until implementation**, not part of the gated `corpus/` suite and
not counted as implemented conformance coverage. The pending marker is
`//! Pending (sp01):` prose. Their standard `check:`, `phase:` and
`conforms:` headers describe the intended answers **after implementation**,
not the deepest phase today's compiler reaches. The suite's `forward:`
marker is not used: it represents an `unsupported` response, not a blanket
expected-failure exemption for syntax rejections.

The implementation change must move these programs to `corpus/grammar/`,
remove the pending prose, verify each phase and warning ledger, and run the
six accepted programs and four refusal checks across native, release,
checked and interpreter tiers. A syntax rejection of an accepted program
is not success; rejection of a malformed header at the initial `let` is
not evidence that its intended diagnostic site is enforced either.

| Program | Intended answer | Property |
| --- | --- | --- |
| `if_let_chain.lu` | `exit(0)`, stdout `3\n10\n22\n` | All chain conditions and arms see the binder; expression value is preserved. |
| `if_var.lu` | `exit(0)`, stdout `8\n3\n` | A typed mutable binding is visible and mutable in either arm. |
| `if_shadow_destructure.lu` | `exit(0)`, stdout `10:7\n7\n` | One destructuring binder, outer lookup during initialization, shadowing, restored outer scope, ambient region. |
| `if_errors.lu` | `exit(0)`, stdout `6\n90\n6\n70\n` | Initializer `?` propagates and `else return` returns from the enclosing function. |
| `while_rebind.lu` | `exit(0)`, stdout `4:4\n3:3\n` | `let` and `var` re-evaluate before every test, including after `continue` and for the final false test. |
| `while_header_jumps.lu` | `exit(0)`, stdout `3:2\n77\n` | Existing initializer-handler `continue`, `break` and `return` follow the loop desugar. |
| `refuse_two_binders.lu` | `fail(E0201)`, deepest successful phase `lex` | Refuse a declaration comma group at the second binder. |
| `refuse_missing_semicolon.lu` | `fail(E0201)`, deepest successful phase `lex` | A newline is not the required literal `;`. |
| `refuse_bare_then.lu` | `fail(E0201)`, deepest successful phase `lex` | Header-bearing bare `then` branches are not allowed. |
| `refuse_after_chain.lu` | `fail(E0301)`, deepest successful phase `parse` | A chain binder is not declared after the chain. |

No compiler, corpus runner or `forward:` semantics change accompanies
these staged programs. Ordinary `else` handlers in an initializer use the
existing expression grammar; there is no new pattern-matching or
fallible-binding protocol.
