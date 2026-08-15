//! The refusal vocabulary — what the importer will **not** pretend to
//! understand, named.
//!
//! A C header describes things wolf's type system cannot express: a
//! union whose live member is a fact the programmer keeps in their
//! head, a pointer whose lifetime is documented in a comment, an
//! integer whose width is decided by a `#if` on a platform we are not
//! compiling for. The importer's job is to translate what it can and
//! **refuse the rest by name**, in the compiler's own voice. An
//! importer that guesses turns a header's prose into a miscompile in
//! somebody else's program, and the guess is invisible at the site
//! where it does the damage.
//!
//! Every refusal is a closed-set [`Refusal`] with a stable machine tag
//! (the dump and the conformance snapshots key on it, so a tag is an
//! interface — retire one, never repurpose it), a headline in the
//! compiler's voice, and a note that says what to do instead. The
//! demotion *level* ([`Demotion`]) is separate: it says how far the
//! declaration falls, not why.

use std::fmt;

/// How far a declaration falls when the importer cannot fully
/// translate it. Zig's ladder, carried wholesale: one bad declaration
/// never kills the header it lives in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Demotion {
    /// The type imports as an opaque body: it has a name, a size the
    /// header may or may not have told us, and no fields. Pointers to
    /// it are usable; its members are not.
    Opaque,
    /// The entity keeps its symbol but loses its signature: callable
    /// only through a hand-asserted `extern "c"` declaration, which is
    /// itself an unsafe act (RFC 3484's lesson — asserting a signature
    /// is a claim, not an import).
    ExternOnly,
    /// The declaration is recorded and refused at every use site, with
    /// this refusal replayed there. Costs nothing if never used.
    ErrorOnUse,
}

impl Demotion {
    /// The stable machine tag (dump + conformance snapshots).
    pub fn tag(self) -> &'static str {
        match self {
            Demotion::Opaque => "opaque",
            Demotion::ExternOnly => "extern-only",
            Demotion::ErrorOnUse => "error-on-use",
        }
    }
}

impl fmt::Display for Demotion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// Why the importer refused. Closed set: a worker that meets something
/// genuinely new reports [`Refusal::Unmodelled`] with its own words
/// rather than inventing a tag, and the tag is added here deliberately
/// (with a conformance case) when we decide what it means.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Refusal {
    // ------------------------------------------------------- types --
    /// A union. Which member is live is a rule the C programmer knows
    /// and the header does not state.
    UnionActiveMember,
    /// The declaration's type changed shape under the preprocessor —
    /// its width or presence depends on a macro we did not fix.
    PlatformConditionalWidth,
    /// A bitfield whose allocation unit the worker could not place for
    /// this target.
    BitfieldLayout,
    /// A struct or union with no definition anywhere in the
    /// translation unit.
    IncompleteType,
    /// A flexible array member (`T rest[];`) — the length lives in a
    /// sibling field by convention, not in the type.
    FlexibleArrayMember,
    /// `_Complex` / `_Imaginary`.
    ComplexType,
    /// A SIMD vector type (`__attribute__((vector_size))`, `__m128`, …).
    VectorType,
    /// `_Atomic`-qualified: the memory model at the seam is not one we
    /// will guess at.
    AtomicType,
    /// `_BitInt(N)`.
    BitInt,
    /// `long double` — the width and the format are per-target and per
    /// ABI, and nothing in wolf spells the x87 80-bit form.
    LongDouble,
    /// A type built out of a type we already refused.
    DependsOnRefused(String),

    // --------------------------------------------------- functions --
    /// An old-style (K&R) declaration: `int f();` promises nothing
    /// about its parameters.
    UnprototypedFunction,
    /// A function whose parameter or return type we could not map.
    UnmappableSignature,
    /// A `static inline` body the worker could not emit an out-of-line
    /// companion for.
    InlineWithoutShim,

    // ------------------------------------------------------ macros --
    /// The expansion is a statement (or several), not an expression.
    MacroExpandsToStatement,
    /// The macro takes a *type* as an argument (`container_of`-shaped).
    MacroTypeArgument,
    /// The macro pastes (`##`) or stringizes (`#`) to build names.
    MacroTokenPasting,
    /// The expansion dispatches on `_Generic`.
    MacroGenericDispatch,
    /// The expansion names something that is not in the artifact.
    MacroReferencesUnknown(String),
    /// The token sequence does not balance — it is a fragment meant to
    /// be pasted into a larger form.
    MacroUnbalanced,
    /// An object-like macro whose expansion is not a C constant
    /// expression.
    MacroNotConstant,
    /// The macro is defined more than once with different bodies and
    /// the header never says which wins at our use site.
    MacroRedefined,

    // ------------------------------------------------------- other --
    /// The worker met something this vocabulary does not name yet. The
    /// string is the worker's own words; it is replayed verbatim.
    Unmodelled(String),
}

impl Refusal {
    /// The stable machine tag. **Interface**: the dump, the
    /// conformance snapshots and `wolf audit` key on these strings.
    pub fn tag(&self) -> &'static str {
        match self {
            Refusal::UnionActiveMember => "union-active-member",
            Refusal::PlatformConditionalWidth => "platform-conditional-width",
            Refusal::BitfieldLayout => "bitfield-layout",
            Refusal::IncompleteType => "incomplete-type",
            Refusal::FlexibleArrayMember => "flexible-array-member",
            Refusal::ComplexType => "complex-type",
            Refusal::VectorType => "vector-type",
            Refusal::AtomicType => "atomic-type",
            Refusal::BitInt => "bit-int",
            Refusal::LongDouble => "long-double",
            Refusal::DependsOnRefused(_) => "depends-on-refused",
            Refusal::UnprototypedFunction => "unprototyped-function",
            Refusal::UnmappableSignature => "unmappable-signature",
            Refusal::InlineWithoutShim => "inline-without-shim",
            Refusal::MacroExpandsToStatement => "macro-expands-to-statement",
            Refusal::MacroTypeArgument => "macro-type-argument",
            Refusal::MacroTokenPasting => "macro-token-pasting",
            Refusal::MacroGenericDispatch => "macro-generic-dispatch",
            Refusal::MacroReferencesUnknown(_) => "macro-references-unknown",
            Refusal::MacroUnbalanced => "macro-unbalanced",
            Refusal::MacroNotConstant => "macro-not-constant",
            Refusal::MacroRedefined => "macro-redefined",
            Refusal::Unmodelled(_) => "unmodelled",
        }
    }

    /// Parse a tag back (the dump is lossless, so the payload-carrying
    /// tags take their payload alongside).
    pub fn from_tag(tag: &str, payload: &str) -> Option<Refusal> {
        Some(match tag {
            "union-active-member" => Refusal::UnionActiveMember,
            "platform-conditional-width" => Refusal::PlatformConditionalWidth,
            "bitfield-layout" => Refusal::BitfieldLayout,
            "incomplete-type" => Refusal::IncompleteType,
            "flexible-array-member" => Refusal::FlexibleArrayMember,
            "complex-type" => Refusal::ComplexType,
            "vector-type" => Refusal::VectorType,
            "atomic-type" => Refusal::AtomicType,
            "bit-int" => Refusal::BitInt,
            "long-double" => Refusal::LongDouble,
            "depends-on-refused" => Refusal::DependsOnRefused(payload.to_string()),
            "unprototyped-function" => Refusal::UnprototypedFunction,
            "unmappable-signature" => Refusal::UnmappableSignature,
            "inline-without-shim" => Refusal::InlineWithoutShim,
            "macro-expands-to-statement" => Refusal::MacroExpandsToStatement,
            "macro-type-argument" => Refusal::MacroTypeArgument,
            "macro-token-pasting" => Refusal::MacroTokenPasting,
            "macro-generic-dispatch" => Refusal::MacroGenericDispatch,
            "macro-references-unknown" => Refusal::MacroReferencesUnknown(payload.to_string()),
            "macro-unbalanced" => Refusal::MacroUnbalanced,
            "macro-not-constant" => Refusal::MacroNotConstant,
            "macro-redefined" => Refusal::MacroRedefined,
            "unmodelled" => Refusal::Unmodelled(payload.to_string()),
            _ => return None,
        })
    }

    /// The payload a payload-carrying tag round-trips through the dump
    /// (empty for the rest).
    pub fn payload(&self) -> &str {
        match self {
            Refusal::DependsOnRefused(s)
            | Refusal::MacroReferencesUnknown(s)
            | Refusal::Unmodelled(s) => s,
            _ => "",
        }
    }

    /// How far this refusal demotes a declaration by default. A worker
    /// may demote further (never less): an opaque type it also failed
    /// to size becomes error-on-use.
    pub fn demotion(&self) -> Demotion {
        match self {
            // The shape is unusable but the name and the address are
            // fine — this is exactly what opaque is for.
            Refusal::UnionActiveMember
            | Refusal::IncompleteType
            | Refusal::FlexibleArrayMember
            | Refusal::BitfieldLayout => Demotion::Opaque,
            // We know the symbol exists; we do not know its shape.
            Refusal::UnprototypedFunction
            | Refusal::UnmappableSignature
            | Refusal::InlineWithoutShim => Demotion::ExternOnly,
            _ => Demotion::ErrorOnUse,
        }
    }

    /// The headline, in the compiler's voice: what happened, in the
    /// reader's terms, without jargon they did not bring.
    pub fn headline(&self) -> String {
        match self {
            Refusal::UnionActiveMember => {
                "this is a union, and which member is live is a rule the header \
                 does not state"
                    .to_string()
            }
            Refusal::PlatformConditionalWidth => {
                "this declaration's width depends on a macro that is not fixed for \
                 this target"
                    .to_string()
            }
            Refusal::BitfieldLayout => {
                "the importer could not place this type's bitfields for this target".to_string()
            }
            Refusal::IncompleteType => {
                "this type is declared but never defined in the headers imported".to_string()
            }
            Refusal::FlexibleArrayMember => {
                "this struct ends in a flexible array member, so its length lives \
                 outside its type"
                    .to_string()
            }
            Refusal::ComplexType => {
                "this type is `_Complex`, which wolf does not spell".to_string()
            }
            Refusal::VectorType => {
                "this is a SIMD vector type, whose layout is the target's business, \
                 not the header's"
                    .to_string()
            }
            Refusal::AtomicType => {
                "this type is `_Atomic`, and the memory ordering at the seam is not \
                 something the importer will assume"
                    .to_string()
            }
            Refusal::BitInt => {
                "this type is a `_BitInt`, which has no wolf spelling yet".to_string()
            }
            Refusal::LongDouble => {
                "this type is `long double`, whose width and format differ per target \
                 and per ABI"
                    .to_string()
            }
            Refusal::DependsOnRefused(what) => {
                format!("this is built out of `{what}`, which the importer already refused")
            }
            Refusal::UnprototypedFunction => {
                "this function is declared without a prototype, so the header \
                 promises nothing about its parameters"
                    .to_string()
            }
            Refusal::UnmappableSignature => {
                "this function's signature uses a type the importer could not map".to_string()
            }
            Refusal::InlineWithoutShim => {
                "this function is `static inline`, and the importer could not build \
                 an out-of-line companion for it"
                    .to_string()
            }
            Refusal::MacroExpandsToStatement => {
                "this macro expands to a statement, and a wolf call site needs an \
                 expression"
                    .to_string()
            }
            Refusal::MacroTypeArgument => {
                "this macro takes a type as an argument, which a wolf call site \
                 cannot pass"
                    .to_string()
            }
            Refusal::MacroTokenPasting => {
                "this macro builds names with `##` or `#`, so its expansion is not a \
                 value"
                    .to_string()
            }
            Refusal::MacroGenericDispatch => {
                "this macro dispatches on `_Generic`, which the v0 importer does not \
                 re-expand"
                    .to_string()
            }
            Refusal::MacroReferencesUnknown(what) => {
                format!("this macro's expansion names `{what}`, which was not imported")
            }
            Refusal::MacroUnbalanced => {
                "this macro's tokens do not balance, so it is a fragment meant to be \
                 pasted into a larger form"
                    .to_string()
            }
            Refusal::MacroNotConstant => {
                "this macro's expansion is not a C constant expression, so it has no \
                 value to import"
                    .to_string()
            }
            Refusal::MacroRedefined => {
                "this macro is defined more than once with different bodies, and the \
                 headers do not say which one reaches a wolf call site"
                    .to_string()
            }
            Refusal::Unmodelled(what) => {
                format!("the importer does not know how to translate this: {what}")
            }
        }
    }

    /// What to do about it. Always concrete, and never "it works, but".
    pub fn note(&self) -> String {
        match self {
            Refusal::UnionActiveMember => {
                "the union imports as an opaque type: pointers to it work, its \
                 members do not. Read a member through a hand-written `extern \"c\"` \
                 accessor, or an inline C block, where the choice of member is \
                 written down and reviewable."
                    .to_string()
            }
            Refusal::PlatformConditionalWidth => {
                "import for one target at a time — pass `--target` (and the `-D` \
                 defines the platform sets) so the width is a fact rather than a \
                 guess."
                    .to_string()
            }
            Refusal::BitfieldLayout => {
                "bitfield placement is per-target and the importer will not \
                 approximate it. Reach the fields through an inline C block, which \
                 uses the platform compiler's own layout."
                    .to_string()
            }
            Refusal::IncompleteType => {
                "import the header that defines it too, or keep using it through \
                 pointers, which is all C gives you here either."
                    .to_string()
            }
            Refusal::FlexibleArrayMember => {
                "the fixed prefix imports; the tail does not. Compute the tail's \
                 address yourself in an `unsafe` block, the way the C caller does."
                    .to_string()
            }
            Refusal::ComplexType | Refusal::BitInt | Refusal::LongDouble => {
                "wrap the entity in a C function that takes and returns types wolf \
                 does spell, and import that instead."
                    .to_string()
            }
            Refusal::VectorType => {
                "call through a C wrapper taking a pointer to the vector, or write \
                 the kernel in wolf where the vector types are the compiler's."
                    .to_string()
            }
            Refusal::AtomicType => {
                "reach it through the C library's own atomic accessors, imported as \
                 ordinary functions."
                    .to_string()
            }
            Refusal::DependsOnRefused(_) => {
                "the refusal above is the one to fix; this one follows from it.".to_string()
            }
            Refusal::UnprototypedFunction => {
                "the symbol is real and callable, but only through an `extern \"c\"` \
                 declaration you write — and writing one is asserting a signature the \
                 header never gave, which is why it is unsafe."
                    .to_string()
            }
            Refusal::UnmappableSignature => {
                "the type refused above is the one to fix. Until then the symbol is \
                 reachable only through an `extern \"c\"` declaration you write and \
                 vouch for."
                    .to_string()
            }
            Refusal::InlineWithoutShim => {
                "the definition is in the header and nowhere else, so there is no \
                 symbol to link. An inline C block calling it compiles the body in \
                 place."
                    .to_string()
            }
            Refusal::MacroExpandsToStatement
            | Refusal::MacroTypeArgument
            | Refusal::MacroTokenPasting
            | Refusal::MacroGenericDispatch
            | Refusal::MacroUnbalanced => {
                "an inline C block is the escape: paste the macro use into it, where \
                 the C preprocessor is the one reading it."
                    .to_string()
            }
            Refusal::MacroReferencesUnknown(_) => {
                "import the header that declares it, or use the macro from an inline \
                 C block that includes the header itself."
                    .to_string()
            }
            Refusal::MacroNotConstant => {
                "if it is meant as a value, the header's own `enum` or `const` is the \
                 thing to import; if it is meant as code, an inline C block runs it."
                    .to_string()
            }
            Refusal::MacroRedefined => {
                "fix the ambiguity at the import: `-D` the macro to the definition you \
                 mean, so the artifact records one body."
                    .to_string()
            }
            Refusal::Unmodelled(_) => {
                "this is a gap in the importer, not in your header. An inline C block \
                 is the escape today; please report the declaration so the refusal \
                 gets a name."
                    .to_string()
            }
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())?;
        let p = self.payload();
        if !p.is_empty() {
            write!(f, "({p})")?;
        }
        Ok(())
    }
}

/// The per-declaration import status: translated, or refused by name
/// with a demotion level.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Status {
    Ok,
    Refused {
        demotion: Demotion,
        refusal: Refusal,
    },
}

impl Status {
    /// Refuse at the refusal's own default level.
    pub fn refuse(refusal: Refusal) -> Status {
        Status::Refused {
            demotion: refusal.demotion(),
            refusal,
        }
    }

    /// Refuse, demoting at least as far as `at` (never less far than
    /// the refusal itself asks for).
    pub fn refuse_at(refusal: Refusal, at: Demotion) -> Status {
        Status::Refused {
            demotion: at.max(refusal.demotion()),
            refusal,
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Status::Ok)
    }

    /// Usable at all? An error-on-use declaration is not.
    pub fn usable(&self) -> bool {
        !matches!(
            self,
            Status::Refused {
                demotion: Demotion::ErrorOnUse,
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tag round-trips, and no two variants share one — the tags
    /// are an interface, so collisions are a silent corpus corruption.
    #[test]
    fn tags_are_unique_and_round_trip() {
        let all = [
            Refusal::UnionActiveMember,
            Refusal::PlatformConditionalWidth,
            Refusal::BitfieldLayout,
            Refusal::IncompleteType,
            Refusal::FlexibleArrayMember,
            Refusal::ComplexType,
            Refusal::VectorType,
            Refusal::AtomicType,
            Refusal::BitInt,
            Refusal::LongDouble,
            Refusal::DependsOnRefused("union tm".into()),
            Refusal::UnprototypedFunction,
            Refusal::UnmappableSignature,
            Refusal::InlineWithoutShim,
            Refusal::MacroExpandsToStatement,
            Refusal::MacroTypeArgument,
            Refusal::MacroTokenPasting,
            Refusal::MacroGenericDispatch,
            Refusal::MacroReferencesUnknown("__errno".into()),
            Refusal::MacroUnbalanced,
            Refusal::MacroNotConstant,
            Refusal::MacroRedefined,
            Refusal::Unmodelled("a GNU attribute we do not read".into()),
        ];
        let mut seen = std::collections::BTreeSet::new();
        for r in &all {
            assert!(seen.insert(r.tag()), "duplicate refusal tag {}", r.tag());
            assert_eq!(
                Refusal::from_tag(r.tag(), r.payload()).as_ref(),
                Some(r),
                "tag {} does not round-trip",
                r.tag()
            );
        }
    }

    /// Refusals speak prose, not enum names: every headline and note is
    /// a real sentence. A stub here is how an importer starts lying.
    #[test]
    fn every_refusal_says_something_and_offers_a_way_out() {
        let all = [
            Refusal::UnionActiveMember,
            Refusal::PlatformConditionalWidth,
            Refusal::BitfieldLayout,
            Refusal::IncompleteType,
            Refusal::FlexibleArrayMember,
            Refusal::ComplexType,
            Refusal::VectorType,
            Refusal::AtomicType,
            Refusal::BitInt,
            Refusal::LongDouble,
            Refusal::DependsOnRefused("union tm".into()),
            Refusal::UnprototypedFunction,
            Refusal::UnmappableSignature,
            Refusal::InlineWithoutShim,
            Refusal::MacroExpandsToStatement,
            Refusal::MacroTypeArgument,
            Refusal::MacroTokenPasting,
            Refusal::MacroGenericDispatch,
            Refusal::MacroReferencesUnknown("__errno".into()),
            Refusal::MacroUnbalanced,
            Refusal::MacroNotConstant,
            Refusal::MacroRedefined,
            Refusal::Unmodelled("something new".into()),
        ];
        for r in &all {
            let h = r.headline();
            let n = r.note();
            assert!(h.len() > 24, "headline too short for {}: {h}", r.tag());
            assert!(n.len() > 24, "note too short for {}: {n}", r.tag());
            assert!(
                !h.contains("TODO") && !n.contains("TODO"),
                "{} still has a stub",
                r.tag()
            );
            // The voice: lower-case opening, no terminal period on the
            // headline (it is a message line, not a paragraph).
            assert!(
                h.chars().next().is_some_and(|c| !c.is_uppercase()),
                "headline for {} starts upper-case: {h}",
                r.tag()
            );
        }
    }

    #[test]
    fn refuse_at_never_demotes_less_than_the_refusal_asks() {
        // Opaque is the union's default; asking for less is ignored.
        let s = Status::refuse_at(Refusal::UnionActiveMember, Demotion::Opaque);
        assert!(matches!(
            s,
            Status::Refused {
                demotion: Demotion::Opaque,
                ..
            }
        ));
        // Asking for more is honored.
        let s = Status::refuse_at(Refusal::UnionActiveMember, Demotion::ErrorOnUse);
        assert!(matches!(
            s,
            Status::Refused {
                demotion: Demotion::ErrorOnUse,
                ..
            }
        ));
        assert!(!s.usable());
    }
}
