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
//! The std/prelude stub tables ([`prelude`]) carry *names only* until
//! the real standard library lands (s05/s51).

pub mod graph;
pub mod interface;
pub mod prelude;
pub mod resolve;

pub use graph::{
    AliasTable, BindTarget, Binding, DiskLoader, ItemKind, ItemTable, MemoryLoader, ModuleData,
    ModuleLoader, Package, RawFile, SourceUnit, Vis, load_package,
};
pub use interface::{Interface, build_interfaces, decode, encode, pretty};
pub use resolve::{Resolution, SINGLE_THREAD_ENV, resolve_package, resolve_package_with};
