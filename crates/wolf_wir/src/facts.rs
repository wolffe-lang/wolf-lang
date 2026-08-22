//! Facts — first-class verified semantics, the D2 posture mechanized.
//!
//! There is NO metadata channel in WIR: a fact is a function-level
//! ENTITY, printed in every dump, hashed into D8 content hashes, and
//! held by the verifier as an obligation. Passes may rely on facts and
//! may not drop them (see `verify::run_pass`). This is the structural
//! answer to the Rust/LLVM `noalias`-metadata fiasco: nothing here can
//! be silently discarded and nothing unverified can be claimed.
//!
//! The fact vocabulary is re-declared at IR level — sema/mem types do
//! not leak in. Mapping from c04's theorem classes:
//!
//! | c04 (wolf_mem)                       | WIR justification tag |
//! |--------------------------------------|-----------------------|
//! | exclusivity theorem for `mut` params | `excl.mut`            |
//! | field-granular disjointness (s18)    | `excl.field`          |
//! | Frozen immutability for `read` params| `frozen.read`         |
//! | region attribution of allocations    | `region.alloc`        |
//! | branch-refined index range (bounds)  | `bounds.br`           |
//! | value derived from a specific op     | `op` / `op %v`        |
//!
//! s26 additionally admits OP-derived forms the verifier re-derives
//! locally: `region`/`deref` citing a `region.alloc`/`stack.alloc`
//! def, `frozen` citing a `sync.freeze` def, and `range` citing the
//! subject's own defining op (checked-arith postconditions, X3's
//! claw-back).
//!
//! Every fact carries a justification tag so checking is LOCAL, not
//! whole-program re-proof, and an optional source span pointing back
//! at the program point its proving analysis blessed (spans are
//! compiler-internal: the textual format does not carry them).

use crate::entity::entity_id;
use crate::ir::Value;
use crate::types::RegionId;
use wolf_span::Span;

entity_id!(
    /// A fact entity within one function.
    pub struct FactId, "fact"
);

/// The size claimed by a `deref` fact.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DerefSize {
    /// A constant byte count: `fact deref %p 64 : …`.
    Const(u64),
    /// `elem` bytes times a runtime count: `fact deref %p 8x%n : …`.
    Scaled { elem: u64, count: Value },
}

/// What a fact claims. Operand values are function-scoped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FactKind {
    /// `noalias %a %b` — the two pointers address disjoint memory for
    /// the lifetime of the function.
    Noalias(Value, Value),
    /// `deref %p SIZE` — p is dereferenceable for SIZE bytes.
    Deref(Value, DerefSize),
    /// `range %v LO..=HI` — v's runtime value lies in [LO, HI].
    Range(Value, i128, i128),
    /// `region %p rN` — p points into region rN (provenance identity).
    Region(Value, RegionId),
    /// `frozen %p` — memory reachable through p is deeply immutable.
    Frozen(Value),
}

impl FactKind {
    /// The keyword the textual format uses.
    pub fn keyword(&self) -> &'static str {
        match self {
            FactKind::Noalias(..) => "noalias",
            FactKind::Deref(..) => "deref",
            FactKind::Range(..) => "range",
            FactKind::Region(..) => "region",
            FactKind::Frozen(..) => "frozen",
        }
    }

    /// The value the fact is primarily about.
    pub fn subject(&self) -> Value {
        match *self {
            FactKind::Noalias(a, _) => a,
            FactKind::Deref(v, _) => v,
            FactKind::Range(v, ..) => v,
            FactKind::Region(v, _) => v,
            FactKind::Frozen(v) => v,
        }
    }
}

/// A c04 checker-theorem class — the external proof a fact may cite.
/// The verifier checks the citation is WELL-FORMED for the fact's
/// shape; re-proving the theorem itself is c04's job, not WIR's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theorem {
    /// Exclusivity of a `mut` parameter (⇒ noalias + deref).
    ExclMut,
    /// Field-granular disjointness of two derived pointers (s18's
    /// place-disjointness theorem; ⇒ noalias).
    ExclField,
    /// Frozen immutability of a `read` parameter (⇒ frozen, noalias, deref).
    FrozenRead,
    /// Region attribution of an allocation (⇒ region, deref).
    RegionAlloc,
    /// A dominating bounds-check branch refines an index range (⇒
    /// range; the pass-side-of-the-branch theorem, s18/s26).
    BoundsBr,
}

impl Theorem {
    pub const ALL: [Theorem; 5] = [
        Theorem::ExclMut,
        Theorem::ExclField,
        Theorem::FrozenRead,
        Theorem::RegionAlloc,
        Theorem::BoundsBr,
    ];

    pub fn tag(self) -> &'static str {
        match self {
            Theorem::ExclMut => "excl.mut",
            Theorem::ExclField => "excl.field",
            Theorem::FrozenRead => "frozen.read",
            Theorem::RegionAlloc => "region.alloc",
            Theorem::BoundsBr => "bounds.br",
        }
    }
}

/// Why a fact holds. Checking is local: either an external theorem
/// class is cited (well-formedness checked here, proof owned by c04)
/// or a deriving op is cited (re-derived by the verifier).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Just {
    /// `: excl.mut` etc. — proved by a c04 checker theorem.
    Theorem(Theorem),
    /// `: op` — implied by the subject value's own defining op.
    DefOp,
    /// `: op %v` — implied by the op that defines %v (e.g. a deref
    /// citing its allocation).
    Op(Value),
    /// `: summary.<16 hex>` — proved by the s99 whole-program range
    /// analysis, against the summary index whose digest starts with
    /// these 8 bytes. Per-function verification is SHAPE-ONLY for this
    /// tag (one function cannot see the program); the semantic check
    /// is [`crate::midend::interproc::reverify`], which re-derives
    /// every summary-justified fact from the current whole program.
    /// The digest names the exact index the fixpoint read, so a fact
    /// is auditable back to its proof even in a bare dump.
    Summary(u64),
    /// `: guard %v` — proved by a check the PROGRAM executes: %v is
    /// the boolean result of a runtime buffer-overlap test, and the
    /// fact's subjects are defined only on the taken edge (s104). The
    /// verifier audits the SHAPE — the guard's operand identity with
    /// the subjects' own base chains, and that the taken edge
    /// dominates both subjects — per the custody model: the versioner
    /// that minted the fact owns the extent arithmetic, exactly as
    /// c04 owns a theorem tag's proof. The guard IS the proof, which
    /// is what makes this D44-addendum-safe: no range of inputs can
    /// make the claim false on the path where it exists.
    Guard(Value),
}

/// A fact: claim + justification + optional span back to the program
/// point the proving analysis blessed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FactData {
    pub kind: FactKind,
    pub just: Just,
    /// Compiler-internal provenance; not printed (the textual format
    /// is position-independent), so round-trip does not preserve it.
    pub span: Option<Span>,
}

impl FactData {
    pub fn new(kind: FactKind, just: Just) -> FactData {
        FactData {
            kind,
            just,
            span: None,
        }
    }
}
