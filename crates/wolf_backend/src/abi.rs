//! ABI lowering v0 (s29) — the backend-INDEPENDENT call-boundary
//! contract, done once (QBE's lesson, report 04: ABI lowering belongs
//! in the backend layer). `wolf_codegen_clif` executes these plans
//! mechanically today; the owned Tier-F backend (c12) consumes the
//! same plans verbatim. No ABI knowledge may live inside a backend
//! crate beyond plan execution.
//!
//! # The two conventions
//!
//! **`wolf-abi-0`** ([`Conv::Wolf`], spec/04 `[abi.native]`) — the
//! internal convention, versioned and unstable
//! ([`CONVENTION_VERSION`]). v0 is deliberately conservative: SysV-
//! shaped register usage (so Cranelift can express it) plus the
//! wolf-specific rules that must be stable now:
//!
//! - **Internal symbols are mangled** ([`crate::mangle`], which folds
//!   [`CONVENTION_VERSION`] into its hash): nothing links across the C
//!   membrane by accident — the membrane is nominal, not conventional.
//!   Changing the convention version changes every symbol, so stale
//!   objects fail to link rather than silently miscompiling (D7).
//! - **By-value aggregates ≤ 2 eightbytes scalarize into registers**
//!   regardless of C classification corner cases — internal layout
//!   owes C nothing (D19, `[abi.native.layout]`). Larger aggregates
//!   pass by reference (params) or through a caller-allocated out-slot
//!   (results). The out-slot pointer is a PLAIN trailing parameter —
//!   not C's `sret` (no pointer-returned-in-`%rax` obligation).
//! - **Error unions** (`[abi.err.repr]`, D30): the discriminant rides
//!   the FIRST INTEGER return register whenever the union returns —
//!   in registers when the whole union fits two eightbytes
//!   ([`RetPass::Split`], tag first: the layout puts it at offset 0),
//!   else out-slot with the discriminant alone in a register
//!   ([`RetPass::Sret`] `disc_in_reg: true`). Either way `?` is a
//!   compare-and-branch on a register value — never a memory reload,
//!   never a landing pad.
//! - **Mode-carrying parameters** at the boundary (the s26 table,
//!   realized in WIR sigs before this module ever runs): `mut` = one
//!   pointer + one region token (the token erases here —
//!   [`ParamPass::Token`]); `take` and read/`val` pass by value under
//!   the rules above.
//! - **No shadow space, no unwind tables, ever** (`[abi.native.
//!   nounwind]`): every control transfer is a call, return, branch, or
//!   trap.
//!
//! **C SysV x86-64** ([`Conv::C`], spec/04 `[abi.c]`, psABI +
//! qbe/doc/abi.txt) — the membrane convention for `extern "c"` imports
//! and `export`ed wolf functions. Classification is INTEGER/SSE/MEMORY
//! over eightbytes with the recursive two-eightbyte merge; > 2
//! eightbytes ⇒ MEMORY ([`ParamPass::Memory`]: byval stack argument;
//! [`RetPass::Sret`]: true C sret — pointer in `%rdi`, returned in
//! `%rax`). WIR aggregate layout is natural/C-like (`super::layout`),
//! so no unaligned-field ⇒ MEMORY demotions arise in v0's type
//! universe; `repr(c)`-specific layouts (packed, bitfields) are the
//! importer campaign's (c10), and bitfields are a compile error until
//! then (report 06 §8).
//!
//! # Per-target seam (c13)
//!
//! Only `x86_64-linux-sysv` is implemented. The module boundary IS the
//! c13 seam; the deltas, per report 06's ABI table, are documented as
//! stubs:
//! - **aapcs64** (linux aarch64): 8 GP + 8 FP argument registers,
//!   composites ≤ 16 bytes in registers, HFA/HVA classes, indirect
//!   `x8` result pointer for large returns.
//! - **apple-arm64**: aapcs64 minus stack-slot padding rules
//!   (arguments pack), `char` signedness and varargs deltas.
//! - **win64**: 4 register slots shared across GP/XMM, shadow space
//!   for C calls (never for wolf-native — `[abi.native.call]`),
//!   aggregates > 8 bytes by reference.
//!
//! # Reserved divergence room (spec/04 `[abi.native.unstable]`)
//!
//! Documented so Tier-F/Tier-R can cash these in without a design
//! round: multi-value returns beyond two registers, callee-save
//! retuning per target contract file (`[abi.native.call]`), niche-
//! packed passing (`[abi.native.niche]`), and variadic *calls* through
//! the membrane (`%al` = SSE-count; inexpressible in Cranelift, so the
//! debug tier refuses them honestly — c10 owns the surface anyway).
//!
//! # What cannot cross the membrane
//!
//! Error unions are not `repr(c)`-expressible: an `export`ed `!T`
//! function is a front-end error (E1201 with a flatten fix-it,
//! `[abi.err.row]`) — [`plan_sig`] with [`Conv::C`] over an eu
//! signature is therefore a caller bug and panics in debug builds.

use wolf_wir::ir::SigData;
use wolf_wir::types::{TypeData, TypeId, TypeInterner};

use crate::layout;

/// The wolf-native convention version. Part of every mangled symbol's
/// hash ([`crate::mangle`]): bumping it invalidates every previously
/// compiled object at link grain — caches rebuild, nothing silently
/// miscompiles (D7). The s31 driver additionally folds this string
/// into its rebuild keys; until then the wolfi interface hash pins the
/// toolchain version, which can only change together with this one.
pub const CONVENTION_VERSION: &str = "wolf-abi-0";

/// Which convention a signature crosses under.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Conv {
    /// `wolf-abi-0`: internal calls (mangled symbols).
    Wolf,
    /// SysV x86-64 C: the explicit membrane (`extern "c"` / `export`).
    C,
}

/// Register class of one eightbyte (SysV vocabulary; wolf-native v0
/// reuses it deliberately — `[abi.native.call]`'s divergence room is
/// reserved, not spent).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegClass {
    Int,
    Sse,
}

/// One eightbyte of a scalarized aggregate: where its bytes live in
/// the value's layout, which register class carries it, and how many
/// of its bytes are meaningful (trailing eightbytes of odd-sized
/// aggregates carry fewer than 8; bytes beyond `bytes` are undefined
/// in the register, exactly like C).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Unit {
    pub offset: u32,
    pub class: RegClass,
    pub bytes: u8,
}

/// How one parameter position crosses the boundary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParamPass {
    /// Effect token: erased, crosses as nothing.
    Token,
    /// One scalar, directly in its class register (or the stack once
    /// registers exhaust — the register/stack split is the executing
    /// backend's, per target).
    Direct(TypeId),
    /// Aggregate scalarized into ≤ 2 eightbyte units.
    Split(Vec<Unit>),
    /// Aggregate in memory: `Conv::Wolf` passes a pointer to it;
    /// `Conv::C` passes it byval on the stack (psABI MEMORY class).
    Memory,
}

/// How one result position crosses the boundary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RetPass {
    /// Effect token: erased.
    Token,
    /// One scalar in its class return register.
    Direct(TypeId),
    /// Aggregate in ≤ 2 return registers (eu: tag is unit 0 — the
    /// discriminant IS in the first INTEGER return register).
    Split(Vec<Unit>),
    /// Caller-allocated out-slot. `Conv::C`: true sret (pointer in
    /// `%rdi`, returned in `%rax`). `Conv::Wolf`: a plain trailing
    /// pointer parameter; `disc_in_reg` additionally returns the
    /// error-union discriminant as an `i64` in the first INTEGER
    /// return register (`[abi.err.repr]`'s big-payload case).
    Sret { disc_in_reg: bool },
}

/// The complete plan for one signature under one convention.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SigPlan {
    pub conv: Conv,
    pub params: Vec<ParamPass>,
    pub rets: Vec<RetPass>,
}

impl SigPlan {
    /// Does any result use an out-slot?
    pub fn has_sret(&self) -> bool {
        self.rets.iter().any(|r| matches!(r, RetPass::Sret { .. }))
    }
}

/// Scalar register class of a non-aggregate, non-token type.
fn scalar_class(types: &TypeInterner, ty: TypeId) -> Option<RegClass> {
    Some(match types.get(ty) {
        TypeData::I8
        | TypeData::I16
        | TypeData::I32
        | TypeData::I64
        | TypeData::Bool
        | TypeData::Ptr => RegClass::Int,
        TypeData::F32 | TypeData::F64 => RegClass::Sse,
        TypeData::Mem(_) | TypeData::Io | TypeData::Agg(_) | TypeData::Eu { .. } => return None,
    })
}

fn is_agg(types: &TypeInterner, ty: TypeId) -> bool {
    matches!(types.get(ty), TypeData::Agg(_) | TypeData::Eu { .. })
}

/// Recursively record every scalar leaf's `(offset, class, size)`.
fn scalar_leaves(types: &TypeInterner, ty: TypeId, base: u32, out: &mut Vec<(u32, RegClass, u32)>) {
    let fields: Vec<TypeId> = match types.get(ty) {
        TypeData::Agg(fs) => fs.clone(),
        TypeData::Eu { .. } => layout::eu_fields(types, ty),
        _ => {
            if let Some(class) = scalar_class(types, ty) {
                let size = layout::layout_of(types, ty).expect("scalar layout").size;
                out.push((base, class, size));
            }
            return;
        }
    };
    let offs = layout::field_offsets(types, &fields).expect("aggregate layout");
    for (f, off) in fields.iter().zip(offs) {
        scalar_leaves(types, *f, base + off, out);
    }
}

/// SysV-style eightbyte classification of an aggregate/eu type:
/// `Some(units)` when it fits two eightbytes (fields merged per
/// eightbyte — any INTEGER makes the eightbyte INTEGER, all-SSE stays
/// SSE), `None` for MEMORY (> 2 eightbytes). Also the wolf-native
/// scalarization shape — v0 reuses the classification wholesale
/// (divergence room reserved, not spent).
pub fn classify_units(types: &TypeInterner, ty: TypeId) -> Option<Vec<Unit>> {
    let size = layout::layout_of(types, ty)?.size;
    if size > 16 {
        return None;
    }
    let mut leaves = Vec::new();
    scalar_leaves(types, ty, 0, &mut leaves);
    let n_units = size.div_ceil(8).max(1) as usize;
    let mut units = Vec::with_capacity(n_units);
    for i in 0..n_units as u32 {
        let lo = i * 8;
        let hi = lo + 8;
        let mut class = None;
        let mut covered = 0u32;
        for &(off, c, sz) in &leaves {
            if off >= hi || off + sz <= lo {
                continue;
            }
            class = Some(match (class, c) {
                (None, c) | (Some(RegClass::Sse), c) => c,
                (Some(RegClass::Int), _) => RegClass::Int,
            });
            covered = covered.max((off + sz).min(hi) - lo);
        }
        // An eightbyte no field covers (padding only) is INTEGER; it
        // can only arise interior to a two-eightbyte aggregate.
        units.push(Unit {
            offset: lo,
            class: class.unwrap_or(RegClass::Int),
            bytes: covered.max(1) as u8,
        });
    }
    Some(units)
}

/// Plan one signature under one convention. Tokens erase; scalars pass
/// direct; aggregates split or go to memory per the module contract.
///
/// Panics (debug builds) when an error union meets [`Conv::C`]: rows
/// never cross the membrane (`[abi.err.row]` — the front end rejects
/// with E1201 before any backend runs).
pub fn plan_sig(types: &TypeInterner, sig: &SigData, conv: Conv) -> SigPlan {
    let plan_agg = |ty: TypeId| -> Option<Vec<Unit>> {
        if conv == Conv::C {
            debug_assert!(
                !matches!(types.get(ty), TypeData::Eu { .. }),
                "error unions never cross the C membrane (E1201)"
            );
        }
        classify_units(types, ty)
    };
    let rets: Vec<RetPass> = sig
        .results
        .iter()
        .map(|&r| {
            if types.is_token(r) {
                RetPass::Token
            } else if is_agg(types, r) {
                // Split returns always fit: ≤ 2 units per class never
                // exceed the two INTEGER + two SSE return registers.
                match plan_agg(r) {
                    Some(units) => RetPass::Split(units),
                    None => RetPass::Sret {
                        disc_in_reg: matches!(types.get(r), TypeData::Eu { .. }),
                    },
                }
            } else {
                RetPass::Direct(r)
            }
        })
        .collect();
    // psABI argument-register accounting (`Conv::C` only): when an
    // aggregate's units no longer ALL fit in registers, the aggregate
    // reverts WHOLESALE to memory — never one eightbyte in a register
    // and one on the stack (QBE's "don't think in pushes" table).
    // Scalars need no accounting: an exhausted scalar spills to the
    // stack identically either way. wolf-abi-0 deliberately skips the
    // demotion (internal layout owes C nothing — the executing
    // backend's splitting IS the convention there).
    let mut int_left: i32 = if conv == Conv::C {
        // A C sret pointer consumes `%rdi` before any argument.
        6 - rets
            .iter()
            .filter(|r| matches!(r, RetPass::Sret { .. }))
            .count() as i32
    } else {
        i32::MAX
    };
    let mut sse_left: i32 = if conv == Conv::C { 8 } else { i32::MAX };
    let params = sig
        .params
        .iter()
        .map(|p| {
            if types.is_token(p.ty) {
                ParamPass::Token
            } else if is_agg(types, p.ty) {
                match plan_agg(p.ty) {
                    Some(units) => {
                        let ints = units.iter().filter(|u| u.class == RegClass::Int).count() as i32;
                        let sses = units.iter().filter(|u| u.class == RegClass::Sse).count() as i32;
                        if ints <= int_left && sses <= sse_left {
                            int_left -= ints;
                            sse_left -= sses;
                            ParamPass::Split(units)
                        } else {
                            ParamPass::Memory
                        }
                    }
                    None => ParamPass::Memory,
                }
            } else {
                match scalar_class(types, p.ty) {
                    Some(RegClass::Int) => int_left -= 1,
                    Some(RegClass::Sse) => sse_left -= 1,
                    None => {}
                }
                ParamPass::Direct(p.ty)
            }
        })
        .collect();
    SigPlan { conv, params, rets }
}

/// The libc import subset the WIR lowerer emits today (the is04
/// modelled set, s22 — native truth lands at s29): WIR callee name →
/// unmangled C symbol. Hand-declared `extern "c"` beyond this set is
/// c10's header-importer territory.
pub fn c_import_symbol(callee: &str) -> Option<&str> {
    let name = callee.strip_prefix("c.")?;
    matches!(name, "malloc" | "calloc" | "free" | "memset" | "memcpy").then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wolf_wir::ir::{Module, Param};
    use wolf_wir::types;

    #[test]
    fn scalars_pass_direct_and_tokens_erase() {
        let mut m = Module::new();
        let io = types::IO;
        let sig = m.make_sig(
            vec![
                Param::val(types::I32),
                Param::val(io),
                Param::val(types::F64),
            ],
            vec![types::I64],
        );
        let plan = plan_sig(&m.types, &m.sigs[sig], Conv::Wolf);
        assert_eq!(
            plan.params,
            vec![
                ParamPass::Direct(types::I32),
                ParamPass::Token,
                ParamPass::Direct(types::F64),
            ]
        );
        assert_eq!(plan.rets, vec![RetPass::Direct(types::I64)]);
        assert!(!plan.has_sret());
    }

    #[test]
    fn small_aggregates_scalarize_with_sysv_classes() {
        let mut m = Module::new();
        // {i32, i32}: one INTEGER eightbyte.
        let ii = m.types.intern(TypeData::Agg(vec![types::I32, types::I32]));
        assert_eq!(
            classify_units(&m.types, ii),
            Some(vec![Unit {
                offset: 0,
                class: RegClass::Int,
                bytes: 8
            }])
        );
        // {f32, f32}: one SSE eightbyte.
        let ff = m.types.intern(TypeData::Agg(vec![types::F32, types::F32]));
        assert_eq!(
            classify_units(&m.types, ff),
            Some(vec![Unit {
                offset: 0,
                class: RegClass::Sse,
                bytes: 8
            }])
        );
        // {f64, i64}: SSE then INTEGER.
        let fi = m.types.intern(TypeData::Agg(vec![types::F64, types::I64]));
        assert_eq!(
            classify_units(&m.types, fi),
            Some(vec![
                Unit {
                    offset: 0,
                    class: RegClass::Sse,
                    bytes: 8
                },
                Unit {
                    offset: 8,
                    class: RegClass::Int,
                    bytes: 8
                },
            ])
        );
        // {i32, f32}: INTEGER wins the merged eightbyte.
        let mixed = m.types.intern(TypeData::Agg(vec![types::I32, types::F32]));
        assert_eq!(
            classify_units(&m.types, mixed),
            Some(vec![Unit {
                offset: 0,
                class: RegClass::Int,
                bytes: 8
            }])
        );
        // {i8, i8, i8}: 3 meaningful bytes in one INTEGER eightbyte.
        let bbb = m
            .types
            .intern(TypeData::Agg(vec![types::I8, types::I8, types::I8]));
        assert_eq!(
            classify_units(&m.types, bbb),
            Some(vec![Unit {
                offset: 0,
                class: RegClass::Int,
                bytes: 3
            }])
        );
        // Nested {{i32, i32}, f64}: two eightbytes, INT then SSE.
        let nested = m.types.intern(TypeData::Agg(vec![ii, types::F64]));
        assert_eq!(
            classify_units(&m.types, nested),
            Some(vec![
                Unit {
                    offset: 0,
                    class: RegClass::Int,
                    bytes: 8
                },
                Unit {
                    offset: 8,
                    class: RegClass::Sse,
                    bytes: 8
                },
            ])
        );
    }

    #[test]
    fn three_eightbytes_are_memory_class() {
        let mut m = Module::new();
        let big = m
            .types
            .intern(TypeData::Agg(vec![types::I64, types::I64, types::I64]));
        assert_eq!(classify_units(&m.types, big), None);
        let sig = m.make_sig(vec![Param::val(big)], vec![big]);
        for conv in [Conv::Wolf, Conv::C] {
            let plan = plan_sig(&m.types, &m.sigs[sig], conv);
            assert_eq!(plan.params, vec![ParamPass::Memory]);
            assert_eq!(plan.rets, vec![RetPass::Sret { disc_in_reg: false }]);
        }
    }

    #[test]
    fn eu_returns_put_the_discriminant_in_the_first_int_register() {
        let mut m = Module::new();
        // eu{i64}: {tag i64, ok i64} = 16 bytes — both in registers,
        // tag first (`?` branches on %rax).
        let small = m.types.eu(Some(types::I64), vec![]);
        let sig = m.make_sig(vec![], vec![small]);
        let plan = plan_sig(&m.types, &m.sigs[sig], Conv::Wolf);
        let RetPass::Split(units) = &plan.rets[0] else {
            panic!("small eu returns in registers, got {:?}", plan.rets[0]);
        };
        assert_eq!(units[0].offset, 0);
        assert_eq!(units[0].class, RegClass::Int);
        // eu{i64, i64}: 24 bytes — out-slot, discriminant STILL in a
        // register.
        let big = m.types.eu(Some(types::I64), vec![types::I64]);
        let sig = m.make_sig(vec![], vec![big]);
        let plan = plan_sig(&m.types, &m.sigs[sig], Conv::Wolf);
        assert_eq!(plan.rets, vec![RetPass::Sret { disc_in_reg: true }]);
        assert!(plan.has_sret());
    }

    #[test]
    fn c_aggregates_revert_wholesale_when_registers_exhaust() {
        let mut m = Module::new();
        let ll = m.types.intern(TypeData::Agg(vec![types::I64, types::I64]));
        // Five scalars leave one INTEGER register: the two-eightbyte
        // aggregate no longer fits and reverts WHOLESALE under C —
        // while wolf-abi-0 keeps splitting (layout freedom, D19).
        let mut params = vec![Param::val(types::I64); 5];
        params.push(Param::val(ll));
        let sig = m.make_sig(params, vec![types::I64]);
        let c = plan_sig(&m.types, &m.sigs[sig], Conv::C);
        assert_eq!(c.params[5], ParamPass::Memory);
        let w = plan_sig(&m.types, &m.sigs[sig], Conv::Wolf);
        assert!(matches!(w.params[5], ParamPass::Split(_)));
        // A C sret consumes `%rdi`: four scalars + sret leave one reg.
        let big = m
            .types
            .intern(TypeData::Agg(vec![types::I64, types::I64, types::I64]));
        let mut params = vec![Param::val(types::I64); 4];
        params.push(Param::val(ll));
        let sig = m.make_sig(params, vec![big]);
        let c = plan_sig(&m.types, &m.sigs[sig], Conv::C);
        assert_eq!(c.rets[0], RetPass::Sret { disc_in_reg: false });
        assert_eq!(c.params[4], ParamPass::Memory);
    }

    #[test]
    fn convention_version_participates_in_mangling() {
        let mut m = Module::new();
        let sig = m.make_sig(vec![Param::val(types::I64)], vec![types::I64]);
        let now = crate::mangle_versioned(&m, "f", sig, CONVENTION_VERSION);
        assert_eq!(now, crate::mangle(&m, "f", sig));
        let bumped = crate::mangle_versioned(&m, "f", sig, "wolf-abi-1");
        assert_ne!(
            now, bumped,
            "a convention bump must invalidate every symbol (D7)"
        );
    }

    #[test]
    fn c_import_set_is_exactly_the_modelled_five() {
        for (callee, sym) in [
            ("c.malloc", "malloc"),
            ("c.calloc", "calloc"),
            ("c.free", "free"),
            ("c.memset", "memset"),
            ("c.memcpy", "memcpy"),
        ] {
            assert_eq!(c_import_symbol(callee), Some(sym));
        }
        assert_eq!(c_import_symbol("c.printf"), None, "varargs are c10's");
        assert_eq!(
            c_import_symbol("malloc"),
            None,
            "the namespace is the membrane"
        );
    }
}
