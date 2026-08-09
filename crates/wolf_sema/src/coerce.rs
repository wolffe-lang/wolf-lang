//! The closed coercion set (s17, Target 2). **The list below is the
//! spec**: these are the only places the checker changes a value's
//! type without the programmer spelling an operation, and each one is
//! recorded in the typed HIR ([`crate::check::TypedBody::coercions`])
//! so c04/c05 lower exactly what sema admitted.
//!
//! # The set
//!
//! 1. **Literal typing** — `{integer}`/`{float}` literal variables
//!    solve from context, then default (`i32`/`f64`) *by rule* at body
//!    end (s13). A typing discipline rather than a value conversion,
//!    so it is not recorded as a coercion node.
//! 2. **Ok-injection** ([`Coercion::OkInjection`]) — `T ⇐ !T`: a plain
//!    value flows into a fallible context by tagging it as the ok
//!    half. A representation-level injection, no user code (D30).
//! 3. **Error-row width subtyping** ([`Coercion::RowWiden`]) — at call
//!    and return boundaries only, a value of `T ! {A}` flows where
//!    `T ! {A, B}` is expected; the tag re-numbers by injection
//!    (s15). Never subsumption inside the unifier.
//! 4. **Never-to-any** ([`Coercion::NeverToAny`]) — a diverging
//!    expression (`return`, `break`, `continue`, a `loop` without
//!    `break`) has type `!` and checks at any type. No value exists
//!    at runtime; lowering emits no conversion.
//! 5. **Function item → function value** — a named non-generic `fn`
//!    used as a value has its signature's `fn(…) -> …` type. Item and
//!    value share one representation in the bootstrap (a code
//!    pointer), so nothing is recorded; if c05 splits the
//!    representations, the entry lands here first.
//!
//! # Deliberately NOT coerced
//!
//! - **No implicit numeric widening or narrowing** (X3 posture:
//!   conversions are spelled). `i32 → i64` is written `x as i64`; the
//!   E0401 hint says so. Implicit widening is where C's usual
//!   arithmetic conversions and their bug class begin.
//! - **No autoderef / auto-ref chains.** The receiver of a method
//!   call is a place, and its mode is explicit per X1
//!   (`(mut p).norm()`); no `Deref` ladder exists to walk (report 02:
//!   Swift's exponential search, Rust's method-probe subtleties).
//! - **No user-defined implicit conversions.** Conversion is an
//!   ordinary function or trait method invoked by name — `From`-style
//!   implicit ceremonies are exactly the `?`-inference trap D30
//!   rejects (RFC 1859's lesson).
//! - **No `distinct` transparency.** An adapter type never silently
//!   becomes its base: `as` casts both ways are *explicit* and free
//!   (the layout-identity fact, [`crate::check::CastKind::Adapter`]).
//! - **No bool/int bridging, no truthiness** — comparisons are
//!   spelled (`x != 0`).
//!
//! # Extension contract (c04, s18–s21)
//!
//! Tier-related adjustments (region/`shared`/`handle` interactions,
//! borrow adjustments at call boundaries) are c04's to define: add a
//! [`Coercion`] variant + a doc entry above, record it where the tier
//! checker admits it, and lower it in c05 — the checker's structure
//! does not reopen.

/// One inserted coercion, recorded in visit order in
/// [`crate::check::TypedBody::coercions`]. The variants are the
/// closed set documented at module level — extension is by variant
/// addition (c04), never by a new implicit mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coercion {
    /// `T ⇐ !T`: ok-injection into the error union (D30).
    OkInjection,
    /// `T ! {A} ⇐ T ! {A, B}`: width subtyping at a checking
    /// boundary; tags re-number by injection (s15).
    RowWiden,
    /// `! ⇐ T`: divergence checks at any type; no runtime value.
    NeverToAny,
}
