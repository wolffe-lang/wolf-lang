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
//! # Implemented today (s15)
//!
//! - **Error rows** ([`types`]/[`unify`]): `!T` carries a structural,
//!   payload-carrying tag set (D30 — Koka's `exn` row made concrete in
//!   Zig's syntax, with the payloads Zig lacks). Rows are *sets*
//!   (duplicate labels E0601), unify by tag name with pointwise
//!   payloads, and admit one polymorphic tail — a row entry naming a
//!   generic parameter (`fn(T) -> U ! {E}`, the HOF `rethrows`
//!   answer). **Width subtyping is checking-direction only**, at call
//!   and return boundaries — never subsumption inside the unifier.
//! - **`?` / `else`** ([`check`]): `expr?` is a `R_callee ⊆ R_caller`
//!   width check plus re-tagging by injection — no `From`, ever
//!   (coarsening is an explicit trait impl invoked by name). Failures
//!   name exactly the missing tags (E0602, signature-extending
//!   fix-it; the structural diff rides the JSON schema as
//!   `row_diff`). `else` defaults, `else |err| …` binds the row.
//! - **Sealing** ([`rows`]): private `-> !T` rows resolve to concrete
//!   tag sets at signature elaboration (cycle-aware fixpoint over the
//!   module call graph), so inferred-row functions recurse and have
//!   their addresses taken; `wolf interface` shows the sealed rows
//!   (never hashed — private items are not interface surface).
//!   Exported inferred rows are E0605 with a state-the-row fix-it.
//! - **`errdefer`** ([`check`]): fallible-function-only (E0607),
//!   recorded with `defer` in declaration order in the typed HIR for
//!   s27's strict-LIFO lowering; every `?`/`else`/raise site is an
//!   error-trace hook point for s32 (debug-only, [abi.err.trace]).
//!
//! # Implemented today (s17 — sema completion, the c03 closer)
//!
//! - **Patterns & exhaustiveness** ([`check`] + the crate-private
//!   `exhaust` engine): the full
//!   [gram.pat] grammar types against the scrutinee (variants, row
//!   tags with payloads, tuples, or-patterns, `@`-bindings), and every
//!   `match` runs through Maranget-style usefulness at body end —
//!   non-exhaustive is E0801 with ≤3 *concrete witnesses*, a dead arm
//!   is E0802 (warning) citing its subsumer; guards never count toward
//!   coverage. Sealed rows are closed universes; open rows
//!   (`! {A, ..}`, [`types::TyKind::OpenTail`]) elaborate for real,
//!   admit new raises, and force rest arms — the growing-tag-union
//!   bar. Refutable patterns in binding position are E0806.
//! - **Method resolution** ([`check`]): `recv.method(args)` resolves
//!   in a fixed order — inherent impls first, then trait methods from
//!   traits *in scope* (coherence makes the impl unique per trait);
//!   cross-trait ambiguity is E0803 (qualify), an out-of-scope trait's
//!   method is E0807 (import fix-it). No auto-ref/deref, no fallback
//!   chains (D27). The X1 receiver-mode law binds: `mut self` /
//!   `take self` methods are called `(mut p).norm()` /
//!   `(take p).consume()` (E0804, machine-applicable edits);
//!   `read self` stays bare. `dyn` receivers dispatch through the s14
//!   dyn-safe set, resolution identical by name. Enum variants
//!   construct (`Color.Rgb(1, 2, 3)`), inherent associated functions
//!   call on the type, and trait *default* bodies check once against
//!   the trait's own archetype `Self`.
//! - **The closed coercion set** ([`coerce`] — the list is the spec)
//!   and the completed `as` cast checking: numeric casts,
//!   adapter↔base free casts (layout identity), identity; everything
//!   else is E0805. No implicit widening, no autoderef, no
//!   user-defined conversions — deliberately.
//!
//! # The typed-HIR contract (what c04/s18 consumes)
//!
//! [`typecheck_package`] returns [`Typecheck`]; for every body that
//! reached [`BodyResult::Checked`], the [`TypedBody`] is the c04
//! input language — s18 starts from this list, not from spelunking:
//!
//! - **Names**: resolved by s12 ([`Resolution`]); no name in a
//!   checked body is unresolved.
//! - **Types**: `TypedBody::exprs` / `::locals` carry every recorded
//!   expression and binding with its *final, defaulted* type in the
//!   body's own [`TypeTable`] (zonked — no live inference vars).
//! - **Dispatch**: `TypedBody::dispatch` records every method call's
//!   resolution ([`check::Dispatch`]) — inherent vs trait, static vs
//!   `dyn`. Receiver modes live in the signature tables
//!   ([`sig::ParamSig::mode`]): c04 reads the declared mode of every
//!   parameter and receiver from there; the call-site spelling was
//!   enforced here (E0804), the exclusivity/move *semantics* are
//!   s18–s21's to check.
//! - **Matches**: `TypedBody::matches` annotates each `match` span
//!   with its exhaustiveness fact; s27 lowers decision trees from it.
//! - **Calls** (s18 addendum): `TypedBody::calls` records every
//!   resolved call site's declared mode surface ([`check::CallSig`]) —
//!   parameter modes, declaration spans, receiver view sets
//!   ([`sig::ParamSig::view`]), and whether the call is enum-variant
//!   construction (`ctor`: payloads move in). `wolf_mem`'s call-site
//!   mode agreement (E1007) and exclusivity checks read from here and
//!   never re-resolve.
//! - **Coercions**: `TypedBody::coercions` lists every inserted
//!   coercion from the closed set ([`coerce::Coercion`]);
//!   `TypedBody::casts` lists every `as` with its [`check::CastKind`]
//!   (only `Numeric` lowers to a conversion). Tier adjustments are
//!   c04's to add — new variant, new record, checker unopened.
//! - **Control facts**: `TypedBody::cleanups` (defer/errdefer in
//!   declaration order, strict-LIFO for s27), `::trace_points` (every
//!   `?`/`else`/raise site, the s32 error-trace hooks),
//!   `::comptime_calls` (the s16 ctfe roots), `::warnings` (E0802 et
//!   al. from otherwise-clean bodies).
//! - **Suppression**: parser error regions
//!   (`Parse::error_regions`) were already applied — diagnostics in
//!   `Typecheck::diagnostics` are cascade-suppressed and sorted; c04
//!   feeds its own diagnostics through the same
//!   `wolf_diag::Diagnostics` sink contract.
//! - **Refusals**: anything sema could not type is in
//!   `Typecheck::not_yet` — a body is either fully typed or honestly
//!   refused, never half-guessed.
//!
//! # The s22 addendum to the typed-HIR contract (the unsafe tier)
//!
//! - **Raw casts**: [`check::CastKind::Raw`] marks every pointer
//!   bridge (`*T ↔ *U`, int↔ptr with expose semantics, region→ptr);
//!   `wolf_mem` gates them to `unsafe` blocks (E1301) and records the
//!   expose facts — typing is permissive by design (the raw tier's
//!   rules are *simpler* than the safe tier's, D11).
//! - **C calls**: [`CallSig::c_call`] marks calls through the
//!   `import c` namespace (the modelled intrinsic set: `malloc`,
//!   `calloc`, `free`, `memset`, `memcpy` — mirroring the is04
//!   oracle); other imported names refuse until c10.
//! - **Trust**: [`sig::FnSig::trusted`] carries the `#[trusted]`
//!   obligation; the per-module roster rides the `wolfi` interface
//!   (both hash partitions) and [`audit`] renders the D11 ring
//!   inventory + enforces the manifest rule (E1303).
//!
//! The std/prelude stub tables ([`prelude`]) carry *names only* until
//! the real standard library lands (s05/s51).

pub mod audit;
pub mod check;
pub mod coerce;
pub mod ctfe;
pub(crate) mod exhaust;
pub mod graph;
pub mod interface;
pub mod prelude;
pub mod resolve;
pub mod rewrite;
pub mod rows;
pub mod sig;
pub mod traits;
pub mod typecheck;
pub mod types;
pub mod unify;

pub use check::{BodyRef, BodyResult, CallSig, CastKind, Dispatch, NotYet, TypedBody, check_body};
pub use coerce::Coercion;
pub use ctfe::{Budget, CtfeStats, Engine, ValueArena, ValueKind};
pub use graph::{
    AliasTable, BindTarget, Binding, DiskLoader, ItemKind, ItemTable, MemoryLoader, ModuleData,
    ModuleLoader, Package, RawFile, SourceUnit, Vis, load_package,
};
pub use interface::{Interface, build_interfaces, decode, encode, pretty};
pub use resolve::{Resolution, SINGLE_THREAD_ENV, resolve_package, resolve_package_with};
pub use sig::{BoundRef, GenericSig, ItemSig, SigTables, build_sigs};
pub use traits::{DynReport, ImplDef, TraitDef, TraitRef};
pub use typecheck::{BodyOutcome, Typecheck, typecheck_package, typecheck_package_with};
pub use types::{MetaTy, Prim, TyId, TyKind, TypeTable};
