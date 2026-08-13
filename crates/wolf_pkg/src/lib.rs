//! wolf_pkg — the package manager (s51, c11-tooling).
//!
//! The D33 covenant as running code: a **declarative** manifest
//! (`wolf.pkg`, wolf-literal data — reading the dependency graph never
//! executes anything, and a manifest that asks for build-time
//! execution is refused with E1503), **MVS** resolution (deps state
//! minimums, builds use max-of-minimums; no ranges, no solver, unique
//! solution), an **integrity ledger** (`wolf.sum`, blake3 content
//! addresses over source-filtered trees), a **content-addressed
//! store** for fetched packages, **capability manifests** with the
//! `wolf audit` tree (I13), and the **transparency-log record format**
//! pinned registry-shaped from day one (X7 — v1 transport is
//! path/git + dumb files; c15 changes only the fetch URL scheme).
//!
//! Layered bottom-up: [`version`] (points, not ranges) → [`manifest`]
//! (parse + schema + the D33 gate) → [`mvs`] (the algorithm, pure) →
//! [`lock`]/[`source`]/[`log`] (integrity, store, provenance) →
//! [`project`] (disk resolution) → [`audit`] (I13 trees and the E1504
//! import-graph check). The driver owns the verbs; this crate owns the
//! formats and never touches sema.
//!
//! s53 adds [`script`]: the same manifest schema and the same parser
//! reading a manifest that rides in a script's `//!` block, plus the
//! cache layout ([`source::cache_root`]) every artifact lives under.

pub mod audit;
pub mod lock;
pub mod log;
pub mod manifest;
pub mod mvs;
pub mod project;
pub mod script;
pub mod source;
pub mod version;

pub use lock::{Lock, LockEntry, hash_tree};
pub use manifest::{Cap, Dep, DepSource, Manifest, is_manifest};
pub use project::{Project, ResolveOpts, ResolvedPkg, resolve_manifest, resolve_project};
pub use script::{Frontmatter, Script, script_id};
pub use version::Version;
