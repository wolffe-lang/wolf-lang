//! Semantic analysis (s12–s17).
//!
//! Contract: name resolution and module interfaces (`wolfi` files, the D7
//! incremental spine), the bidirectional type checker (D27: signatures on
//! items, inference inside bodies), nominal traits with checked generics
//! (D28), `!T` error rows (D30), and the sandboxed CTFE engine (D29 — no
//! ambient IO; the D33 supply-chain guarantee starts here). Produces the
//! typed HIR that both `wolf_mem` and `wolf_wir` consume.
//!
//! # Implemented today (s12)
//!
//! - **Module graph** ([`graph`]): directory = module (D32), lazy
//!   import-driven loading, per-module item tables (E0302 duplicates),
//!   file-scoped import bindings, acyclic-import enforcement with
//!   full-cycle diagnostics (E0303), and the package-alias input slot
//!   for s51.
//! - **Name resolution** ([`resolve`]): two-pass, never type-dependent
//!   (D27). Pass B resolves signatures and bodies per-module in
//!   parallel (rayon over the topological order;
//!   `WOLF_SEMA_SINGLE_THREAD=1` for the sequential fallback), with
//!   `pub`/`pub(pkg)` visibility (E0304), unused-import hard errors
//!   with machine-applicable fix-its (E0305), import collisions
//!   (E0306), and typo suggestions on unresolved names (E0301).
//!   Capitalized misses in expression position defer as candidate
//!   error-row tags (D30) — the type checker owns them.
//! - **Interfaces** ([`interface`]): the `wolfi` v0 binary format with
//!   the two-hash design — `export_hash` over the `pub` partition,
//!   `pkg_hash` including `pub(pkg)` — byte-deterministic by
//!   construction (canonical item order, no absolute paths, no
//!   timestamps, no hash-map iteration), plus the loader API and the
//!   `wolf interface` pretty renderer.
//!
//! # Implemented today (s13)
//!
//! - **Signature elaboration** ([`sig`]): every item's signature
//!   elaborates against resolved names into the interned type table
//!   ([`types`], hash-consed flat arena, index-equality fast path) —
//!   the s12 interface content as types, the typing firewall (D27).
//!   Missing item annotations are E0407 with insertion fix-its.
//! - **Bidirectional checker** ([`check`]): check/synthesize on the
//!   Pfenning recipe, DK-style algorithmic checking on a union-find of
//!   leveled existentials ([`unify`]) with snapshot/rollback trial
//!   unification; monomorphic locals; literals polymorphic over
//!   `{integer}`/`{float}` with the `i32`/`f64` defaulting *rule*;
//!   constraint provenance and structural type diffs (D22, E04xx);
//!   constructs beyond s13's set return `NotYetCheckable`, never a
//!   guess.
//! - **Package driver** ([`typecheck`]): per-body independent checking
//!   (rayon-parallel by default), the conform-run `typecheck` rung
//!   contract, and the bodies/sec bench surface (D5).
//!
//! # Implemented today (s14)
//!
//! - **Traits & impls** ([`traits`]): nominal traits with isolated
//!   member namespaces (qualified calls `Trait.method(args)`;
//!   receiver syntax is s17), associated types/consts with the
//!   input/output distinction (RFC 0195), impl conformance (E0507),
//!   and global coherence — the simple orphan rule (E0504),
//!   uncovered-parameter rejection (E0505), and overlap as a hard
//!   error by trial unification (E0506; blanket impls uniform, **no
//!   specialization — locked**). Adapter types
//!   (`type X = distinct B`) are the sanctioned orphan escape: own
//!   nominal identity, empty impl set, free bidirectional casts, the
//!   layout-identity fact recorded for c05.
//! - **The golden rule** ([`check`]): generic bodies check once
//!   against archetypes built from their bounds — unprovable
//!   capability uses are definition-site E0501 with add-this-bound
//!   edits; instantiation checks arguments against bounds only
//!   (call-site E0502 naming the unmet bound), satisfaction cached
//!   per (type, trait). Ceilings enforced: rank-1, no HKP (E0511),
//!   no GATs (E0512).
//! - **Rewrite constraints** ([`rewrite`]): associated-type equality
//!   by textual canonicalization to a fixed point with up-front
//!   cycle rejection (E0513) — deterministic, terminating, confluent
//!   under rule reordering (property-tested). No surface syntax yet
//!   (spec-amendment candidate for s15/s17).
//! - **`dyn Trait`** ([`traits`]): RFC-0255 dyn-safety at the
//!   object-creation site (E0508/E0509/E0510), with the dyn-safe
//!   method set recorded in canonical order in the interface.
//!
//! The std/prelude stub tables ([`prelude`]) carry *names only* until
//! the real standard library lands (s05/s51).

pub mod check;
pub mod graph;
pub mod interface;
pub mod prelude;
pub mod resolve;
pub mod rewrite;
pub mod sig;
pub mod traits;
pub mod typecheck;
pub mod types;
pub mod unify;

pub use check::{BodyRef, BodyResult, NotYet, TypedBody, check_body};
pub use graph::{
    AliasTable, BindTarget, Binding, DiskLoader, ItemKind, ItemTable, MemoryLoader, ModuleData,
    ModuleLoader, Package, RawFile, SourceUnit, Vis, load_package,
};
pub use interface::{Interface, build_interfaces, decode, encode, pretty};
pub use resolve::{Resolution, SINGLE_THREAD_ENV, resolve_package, resolve_package_with};
pub use sig::{BoundRef, GenericSig, ItemSig, SigTables, build_sigs};
pub use traits::{DynReport, ImplDef, TraitDef, TraitRef};
pub use typecheck::{BodyOutcome, Typecheck, typecheck_package, typecheck_package_with};
pub use types::{Prim, TyId, TyKind, TypeTable};
