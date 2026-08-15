//! `wolf_cimport` — the C header importer **interface** (s46, c10).
//!
//! `import c "stdlib.h"` has to work without wolf becoming a language
//! whose dependencies run configure scripts on your machine. This crate
//! is the seam that makes both true at once.
//!
//! # What this crate is, and is not
//!
//! It **is** the permanent importer interface: the [`artifact`] a C
//! import produces, its versioned [`encode`]ing, the lossless [`dump`]
//! the conformance suite snapshots, the stdio [`protocol`] a worker
//! speaks, the [`cache`] keying that makes rebuilds free, the
//! [`refuse`] vocabulary, and the compiler-side C→wolf [`map`]ping.
//!
//! It is **not** a C frontend, and it does not link one. D17's endgame
//! is wolf's own embedded C frontend (campaign c15); the v0 bootstrap
//! is a libclang-backed worker. Both sit behind [`protocol`], and the
//! conformance suite runs against the interface rather than the
//! implementation — so the scaffold can be burned without a rewrite.
//! Report 06's cautionary tale is Zig's libclang coupling, and the
//! mitigation it names is exactly this: test the interface from day
//! one.
//!
//! # The D33 question, answered
//!
//! D17 names libclang. D33 says **no build scripts, ever**. Taking
//! libclang the ordinary Rust way — `clang-sys`, which `bindgen` sits
//! on — means taking a build script whose entire job is to rummage
//! around the host looking for a shared library. Putting that inside
//! the compiler binary would gut the covenant the project sells, on the
//! very feature whose selling point is that C interop is safe to adopt.
//!
//! The reconciliation is the process boundary the contract already
//! draws, and it is not a compromise: **the worker is a separate
//! executable that the compiler locates at run time and never links.**
//! Whatever a worker needs in order to exist — libclang, a build
//! script, a different language entirely — is on the far side of a
//! pipe. This crate's dependencies are `wolf_span`, `wolf_diag` and
//! `blake3`, and that is the whole list.
//!
//! The precedent is already in the tree: `wolf_codegen_llvm` refuses
//! `llvm-sys`/`inkwell` for the same reason and drives the system LLVM
//! as a *named toolchain requirement, never a build script*. A C
//! importer worker is the same shape of thing, and a missing one is an
//! honest refusal ([`worker::ImportError::NoWorker`]) that names every
//! place it looked.
//!
//! # Honesty about what is imported
//!
//! A header describes things wolf's type system cannot express: a union
//! whose live member is a rule the programmer knows, a pointer whose
//! lifetime is documented in prose, an integer whose width depends on a
//! `#if` for a platform we are not building for. Every one of those is
//! **refused by name** ([`refuse::Refusal`]) with a demotion level
//! ([`refuse::Demotion`]) and a note that says what to do instead, and
//! a refused declaration never costs its siblings anything. An importer
//! that guesses is worse than one that imports half as much, because
//! the guess surfaces as a miscompile in a program nobody is looking
//! at.
//!
//! # Capability posture (I13)
//!
//! Imported C can do anything, which makes it the obvious hole in
//! capability manifests. The posture this crate implements is the
//! conservative half: `import c` is FFI, so a package that imports a
//! header declares the `ffi` capability, and every refusal is counted
//! into the audit surface ([`artifact::Artifact::refusals`]) so a
//! dependency that starts refusing more is a visible diff.
//!
//! What it deliberately does **not** do is claim that an imported
//! declaration carries `exec`, `net` or `fs`. The s46 contract does not
//! say what a declaration carries, and inferring it from a C name
//! (`system`, `fork`, `socket`) would be a heuristic dressed as a
//! guarantee — the same mistake as auto-safe-wrapping, in the security
//! layer where it matters most. That gap is recorded as a finding for
//! the campaign closeout, not decided here.

pub mod artifact;
pub mod cache;
pub mod ctype;
pub mod dump;
pub mod encode;
pub mod map;
pub mod protocol;
pub mod recipe;
pub mod refuse;
pub mod refworker;
pub mod testkit;
pub mod worker;

pub use artifact::{Artifact, Decl, DeclKind, FORMAT_VERSION};
pub use cache::{Cache, ImportRequest};
pub use ctype::{CType, CTypeId, TargetInfo};
pub use dump::dump;
pub use encode::{DecodeError, decode, encode};
pub use map::{CallShape, WolfTy};
pub use protocol::{PROTOCOL_VERSION, Request, Response};
pub use recipe::Recipe;
pub use refuse::{Demotion, Refusal, Status};
pub use refworker::{DiskHeaders, HeaderSource, REFERENCE_WORKER_ID};
pub use worker::{ImportError, Imported, Worker};
