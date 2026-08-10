//! WIR — the mid-level SSA IR that is the product (s24–s27).
//!
//! s24 ships the data model, the textual format, and the verifier:
//!
//! - **Flat arenas** ([`entity`]): u32 newtype indices, all per-entity
//!   data in side tables, packed operand lists, LIFO delete
//!   (truncate-to-mark) for s25's speculative construction. No pointer
//!   graphs, no `Rc`, no interior mutability.
//! - **Block parameters, not φs** ([`ir`]): blocks declare typed
//!   parameters, branches pass argument lists, function params are the
//!   entry block's params. No φ instruction exists (D1).
//! - **A closed op set** ([`ops`]): checked arithmetic traps in both
//!   tiers (X3); loads/stores/calls thread `mem`/`io` effect tokens as
//!   OPERANDS; there is no unwind-shaped terminator (D30) — `region.*`,
//!   `rc.*`, `sync.*`, `eu.*` mnemonics are reserved for s26/s27.
//! - **Facts as first-class semantics** ([`facts`]): noalias edges,
//!   dereferenceability, value ranges, region identity, and frozen
//!   immutability are function-level ENTITIES with justification tags
//!   and analysis spans. There is no metadata channel: nothing a pass
//!   may silently drop, nothing a backend may ignore-and-miscompile —
//!   the structural answer to the Rust/LLVM noalias fiasco (D2).
//! - **Textual round-trip** ([`print`], [`parse`]): canonical printing
//!   (RPO blocks, defs in layout order, sorted facts) reaches a
//!   byte-identical `print → parse → print` fixpoint; dumps diff
//!   cleanly and D8 content-hashing has a stable input. The grammar is
//!   documented in `wolf_wir/text.md`.
//! - **The verifier** ([`verify`]): SSA/block-param well-formedness,
//!   type checking, linear effect tokens, and FACT CONSISTENCY — a
//!   module whose facts don't hold is rejected with a deterministic
//!   diagnostic naming the fact and its justification, offending
//!   function dumped inline. [`verify::run_pass`] is the pass-manager
//!   contract stub: a pass that loses a fact on a live value without a
//!   justified invalidation fails verify-after-pass.
//!
//! s25 adds **construction**:
//!
//! - **The builder** ([`build`]): Braun on-the-fly SSA — `def_var`/
//!   `use_var`, incomplete-block sealing with on-demand block
//!   parameters (no dominance-frontier pass), trivial-param removal
//!   with cascades — plus the Click §5 peephole gauntlet at every
//!   `ins()`: constant fold (checked ops fold to `trap` on provable
//!   overflow, X3 exactly), the data-driven identity table
//!   ([`peephole_rules`]), scope-stacked GVN hash-consing, LIFO
//!   arena rollback on every hit. Effect tokens are ordinary Braun
//!   variables (one `mem` chain per region + one `io`), giving
//!   store→load forwarding and redundant-load GVN through use-def
//!   alone.
//! - **Typed-HIR lowering** ([`lower`]): scalar expression bodies,
//!   `let`/`var`/assignment, calls, `if`/`while`/`loop` with
//!   `break`/`continue`, `assert`, casts. Anything beyond the current
//!   surface refuses with an honest `NotYet` naming its owner (error
//!   unions/`match`/`for`/`defer`/receivers s27, closures/concurrency
//!   c05).
//!
//! s26 lowers the MEMORY STORY:
//!
//! - **The memory family goes live**: `region.new`/`region.alloc`/
//!   `region.free` (wholesale free CONSUMES the token — no live token,
//!   no loads: use-after-free is a verifier structural error),
//!   `sync.freeze` (the frozen token is never invalidated and never
//!   consumable), token-threaded `rc.dup`/`rc.drop`, and `stack.alloc`
//!   (one-slot regions: stack provenance, the s19 promotion landing
//!   pad). Only `sync.transfer` and `eu.*` stay reserved.
//! - **The unsigned decision** (recorded in [`ops`]): one integer type
//!   per width; the checked family grows `uadd.chk`/`usub.chk`/
//!   `umul.chk`/`udiv.chk`/`urem.chk` — signedness is an op property.
//! - **Pointer-shaped `mut`**: a `mut` param lowers to (ptr, `mem.rK`
//!   token) with FORMAL regions bound per call site (verified
//!   substitution); `mut` args spill to `stack.alloc` slots and
//!   reload. By-value aggregates (`agg.make`/`agg.get`) carry
//!   structs/tuples; `region`/`in`/`freeze` sugar lowers to the ops.
//! - **Facts translation** (the c04 handoff): `excl.mut` entry facts
//!   (pairwise noalias + deref), `excl.field` noalias between spilled
//!   call-site slots, op-derived `region`/`deref` facts on
//!   allocations, `frozen` citing its `sync.freeze` point, and `range`
//!   postconditions on checked remainders (X3's claw-back) — every
//!   one held to its justification by the verifier (D2): region-fact
//!   forgery, deref overclaims, stale-token reads (token order), dual
//!   token roots, and frozen-token consumption are rejection classes.
//!
//! Error-union and control lowering is s27; unsafe-tier WIR ops
//! (`ptr.assume_prov`/`assume.noalias`/`prov.retag`) were deferred out
//! of s26 — recorded in the campaign closeout.

pub mod build;
pub mod entity;
pub mod facts;
pub mod ir;
pub mod lower;
pub mod ops;
pub mod parse;
pub mod peephole_rules;
pub mod print;
pub mod types;
pub mod verify;

pub use build::{Const, FuncBuilder, InsOut, Stats, Var};
pub use facts::{DerefSize, FactData, FactId, FactKind, Just, Theorem};
pub use ir::{
    Aux, Block, BlockCall, ExtFunc, ExtFuncData, FuncId, Function, Inst, InstData, Mode, Module,
    Param, SigData, SigId, Value, ValueData, ValueDef,
};
pub use lower::{Build, lower_package};
pub use ops::{FloatCc, IntCc, Opcode};
pub use parse::{ParseError, parse_module};
pub use print::print_module;
pub use types::{RegionId, TypeData, TypeId, TypeInterner};
pub use verify::{
    ErrClass, Invalidation, PassCtx, VerifyError, run_pass, verify_function, verify_module,
};
