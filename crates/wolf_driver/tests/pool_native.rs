//! s173 — the pool's runtime shape, the handle, and the unsafe tier.
//!
//! # The prediction, written before the first change
//!
//! This header is committed BEFORE any source edit, so the table can
//! be read against what was measured rather than rewritten after it.
//! Nothing in this file measures anything yet; the gates land in the
//! commits that follow.
//!
//! ## The baseline, re-derived rather than copied
//!
//! The contract's inputs line gives s168's `lane-coverage` "after"
//! column as `634/364/405/404/419/350`. Measured here at trunk
//! `2f8deb7f` on kasumi (linux x86-64), `cargo xtask lane-coverage`:
//!
//! ```text
//! 637 non-member corpus entries
//!   checked  executes 364 at run
//!   native   executes 408 at run
//!   release  executes 407 at run
//!   UNION 422 of 637 — all three 350
//! ```
//!
//! So `637/364/408/407/422/350`. Three of the six numbers moved after
//! s168's report: s170 landed the conc surface between them. The
//! drift is the orchestrator's, not a defect, and it is recorded
//! rather than absorbed.
//!
//! ## What is predicted
//!
//! | # | claim | falsified by |
//! |---|---|---|
//! | P1 | `Pool[T]` is one runtime header pointer and `handle T` one packed `i64` (index in the low half, generation in the high half), so both lower with no new backend shape | a backend that needs a third WIR type for either |
//! | P2 | the three `phase: mem` Pool witnesses — `corpus/memory/handle_stale.lu`, `corpus/memory/region_multiopen_swap.lu`, `corpus/regions.lu` — all advance to `phase: run` and are executed by native and release | any one of the three still refused by the native lane at the end |
//! | P3 | four of the five unsafe-tier witnesses advance `mem → run`: `corpus/memory/unsafe_noalias.lu`, `corpus/memory/unsafe_creation_not_use.lu`, `corpus/lints/assume_reassigned.lu`, `corpus/lints/safety_comment_missing.lu`. `corpus/memory/unsafe_ub_uaf.lu` does NOT — its header asks for `trap(ub)`, which is the CHECKED build's answer to a use-after-free, and a native build's answer to the same program is undefined by `[mem.unsafe.raw.1]` | any of the four still refused; or `unsafe_ub_uaf.lu` advancing |
//! | P4 | two new three-lane corpus entries land: the `pool[h].next = k` place write (wolf-lang#31's LRU centrepiece) and the Pool accessor set | fewer or more than two |
//! | P5 | `lane-coverage` after: **639 / 366 / 417 / 416 / 424 / 359** — entries +2, checked +2, native +9 (3 Pool movers, 4 unsafe movers, 2 new), release +9, union +2 (the seven movers are already in the union through checked), all-three +9 | any cell off by more than 0 |
//! | P6 | `wolf_rt`'s quarantine allocator stops being `unimplemented!`, and the test that proves it is seen RED at trunk before the fix — `alloc` returns an address with a nonzero tag, `check` accepts it, `free` then makes `check` answer `UseAfterFree` | the test passing at trunk |
//! | P7 | the `Shared[T]` / `Weak[T]` half of `TyKind`'s shared tier stays refused by name. It is the rc row on wolf-lang#268 and it is NOT this lane's: a pool slot's liveness is a generation compare, a cell's is a refcount, and the second needs a drop protocol the native pipe does not have | a shared-cell constructor lowering here |
//! | P8 | no file under `spec/` is touched. Every clause the work wants is written in the report as a proposal with its witness | one byte of `spec/` in this branch's diff |
//!
//! P5 is the falsifiable one; P2, P3 and P6 say which artifacts carry
//! it. P7 is a prediction about scope, and saying it here is what
//! makes "not done" a result rather than an omission.
//!
//! ## Which wolf-lang#268 rows this expects to retire
//!
//! | rows | the compiler's own words, as #268 quotes them |
//! |---|---|
//! | ×6 | `Pool/shared constructor lowering (runtime shapes, c06)` — the Pool half only |
//! | ×3 | `indexing outside str/List (Pool/Map runtime shapes, c06/std)` — the Pool and raw-pointer halves |
//! | ×3 | `index writes outside List (raw-pointer writes c10; Pool/Map c06/std)` |
//! | ×3 | `raw-pointer casts (unsafe-tier WIR ops, deferred from s26 — see closeout)` |
//! | ×2 | `assume noalias (unsafe-tier WIR ops, deferred from s26 — see closeout)` |
//!
//! #268's table is quoted at wolf 0.2.7. s157 has since struck the
//! sprint ids out of every one of those strings, which is why a grep
//! of `wolf_wir/src` for `c06|s26|s54` finds none of them today. The
//! strings survive; the ids do not, and `cargo xtask print-gate`
//! keeps it that way.
