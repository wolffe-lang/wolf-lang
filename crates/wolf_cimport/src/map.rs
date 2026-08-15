//! The C-type → wolf-type mapping. **Compiler-side, on purpose.**
//!
//! The artifact describes C. This module holds wolf's opinions about
//! what C means, and it lives here — not in the worker — so that a
//! better pointer story, a new niche, or a fixed mapping bug costs a
//! recompile and not a re-import of every header on the machine.
//!
//! Two rules govern everything below.
//!
//! **Widths are never widened silently.** C's `int` is 32 bits on every
//! target wolf supports; wolf's `int` is 64. Mapping one to the other
//! would make `c.memset(p, 0x1_0000_0041, n)` compile and truncate
//! somewhere the reader cannot see. C `int` maps to `i32`.
//!
//! **Pointers land on the raw-tier floor.** Every C pointer becomes
//! `*u8` — C's own `void *`/`char *` floor — retyped by `as` at the use
//! site ([mem.unsafe.raw.1]). This is D11's stance, not a shortcut: an
//! importer that inferred `*Foo` from a header comment would be
//! manufacturing a type guarantee out of prose.

use crate::artifact::{Artifact, Decl, DeclKind};
use crate::ctype::{CType, CTypeId, IntSpelling};
use crate::refuse::{Refusal, Status};

/// The wolf types an imported C entity can land on. Deliberately small:
/// this is the raw tier, and its vocabulary is the machine's, not the
/// safe tier's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WolfTy {
    /// `()` — a C `void` return.
    Unit,
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    /// Pointer-sized signed (`int`).
    Int,
    /// Pointer-sized unsigned (`uint`) — where `size_t` lands.
    Uint,
    F32,
    F64,
    /// `*u8`: every C pointer, including function pointers.
    RawPtr,
}

impl WolfTy {
    /// The wolf spelling, for diagnostics and the dump.
    pub fn spelling(self) -> &'static str {
        match self {
            WolfTy::Unit => "()",
            WolfTy::Bool => "bool",
            WolfTy::I8 => "i8",
            WolfTy::I16 => "i16",
            WolfTy::I32 => "i32",
            WolfTy::I64 => "i64",
            WolfTy::U8 => "u8",
            WolfTy::U16 => "u16",
            WolfTy::U32 => "u32",
            WolfTy::U64 => "u64",
            WolfTy::Int => "int",
            WolfTy::Uint => "uint",
            WolfTy::F32 => "f32",
            WolfTy::F64 => "f64",
            WolfTy::RawPtr => "*u8",
        }
    }
}

/// The shape of a call through the `c` namespace: what `c.malloc(…)`
/// typechecks as.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CallShape {
    /// The C name, which is also the link-time symbol.
    pub symbol: String,
    pub params: Vec<WolfTy>,
    pub ret: WolfTy,
    /// A variadic C function (`printf`). Recorded rather than refused
    /// here: whether the *backend* can make the call is a separate
    /// question, and answering it in the mapping would conflate "wolf
    /// cannot say this" with "this target's codegen cannot do it yet".
    pub variadic: bool,
}

/// Map one C type. `Err` is a refusal by name — never a fallback.
pub fn map_type(a: &Artifact, id: CTypeId) -> Result<WolfTy, Refusal> {
    match a.types.get(id) {
        CType::Void => Ok(WolfTy::Unit),
        CType::Bool => Ok(WolfTy::Bool),
        CType::Int {
            bits,
            signed,
            spelling,
        } => map_int(*bits, *signed, *spelling, a),
        CType::Float { bits } => match bits {
            32 => Ok(WolfTy::F32),
            64 => Ok(WolfTy::F64),
            // 80-bit x87 and 128-bit quad have no wolf spelling, and
            // guessing `f64` would silently lose precision at the seam.
            _ => Err(Refusal::LongDouble),
        },
        // Every pointer, including a pointer to a refused type: a
        // pointer to something we do not understand is still a valid
        // address, and refusing it would make opaque types useless.
        CType::Ptr { .. } => Ok(WolfTy::RawPtr),
        // An array in a C declaration position has already decayed;
        // one in a struct field is reached through the record, not
        // here.
        CType::Array { .. } => Ok(WolfTy::RawPtr),
        // A function *type* by value is not a thing; a pointer to one
        // took the Ptr arm above.
        CType::Func { .. } => Ok(WolfTy::RawPtr),
        CType::Record(r) => {
            let rec = a
                .records
                .get(r.0 as usize)
                .ok_or_else(|| Refusal::Unmodelled("a record the artifact does not hold".into()))?;
            // By-value aggregates across the C seam are s29's ABI
            // classification, and passing one wrongly is silent
            // corruption rather than a crash. Until the classifier
            // answers for imported records, they cross by pointer.
            Err(Refusal::DependsOnRefused(format!(
                "{} {}",
                rec.kind.tag(),
                if rec.name.is_empty() {
                    "(anonymous)"
                } else {
                    &rec.name
                }
            )))
        }
        CType::Enum(e) => {
            let def = a
                .enums
                .get(e.0 as usize)
                .ok_or_else(|| Refusal::Unmodelled("an enum the artifact does not hold".into()))?;
            map_type(a, def.underlying)
        }
        CType::Refused(tag) => Err(Refusal::DependsOnRefused(tag.clone())),
    }
}

fn map_int(
    bits: u16,
    signed: bool,
    spelling: IntSpelling,
    _a: &Artifact,
) -> Result<WolfTy, Refusal> {
    // `size_t`/`ptrdiff_t` are the pointer-sized types by definition,
    // and wolf's `uint`/`int` are the pointer-sized types by
    // definition, so these two meet by meaning rather than by width.
    if spelling == IntSpelling::SizeT {
        return Ok(if signed { WolfTy::Int } else { WolfTy::Uint });
    }
    Ok(match (bits, signed) {
        (8, true) => WolfTy::I8,
        (8, false) => WolfTy::U8,
        (16, true) => WolfTy::I16,
        (16, false) => WolfTy::U16,
        (32, true) => WolfTy::I32,
        (32, false) => WolfTy::U32,
        (64, true) => WolfTy::I64,
        (64, false) => WolfTy::U64,
        // `_BitInt(24)`, `__int128`, and friends.
        _ => return Err(Refusal::BitInt),
    })
}

/// The call shape of an imported function, or the refusal that stops
/// it being callable.
///
/// A declaration the *worker* already refused keeps that refusal — the
/// mapping never launders a demotion into something callable.
pub fn call_shape(a: &Artifact, d: &Decl) -> Result<CallShape, Refusal> {
    if let Status::Refused { refusal, .. } = &d.status {
        return Err(refusal.clone());
    }
    let DeclKind::Func { ty, inline_only } = &d.kind else {
        return Err(Refusal::Unmodelled(format!(
            "`{}` is a {}, not a function",
            d.name,
            d.kind.tag()
        )));
    };
    if *inline_only && !a.shims.iter().any(|s| s.function == d.name) {
        return Err(Refusal::InlineWithoutShim);
    }
    let CType::Func {
        ret,
        params,
        variadic,
    } = a.types.get(*ty)
    else {
        return Err(Refusal::UnmappableSignature);
    };
    let mut ps = Vec::with_capacity(params.len());
    for p in params {
        ps.push(map_type(a, *p)?);
    }
    Ok(CallShape {
        symbol: d.name.clone(),
        params: ps,
        ret: map_type(a, *ret)?,
        variadic: *variadic,
    })
}

/// Every callable declaration in the artifact, in canonical order.
/// This is what the `c` namespace resolves against.
pub fn callable(a: &Artifact) -> Vec<(String, CallShape)> {
    let mut out = Vec::new();
    for d in &a.decls {
        if !matches!(d.kind, DeclKind::Func { .. }) {
            continue;
        }
        // Internal linkage means no symbol: importing it as callable
        // would produce a program that compiles and fails to link.
        if d.linkage != crate::artifact::Linkage::External {
            continue;
        }
        if let Ok(shape) = call_shape(a, d) {
            out.push((d.wolf_name.clone(), shape));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctype::TargetInfo;
    use crate::testkit;

    /// The mapping must not widen. Wolf's `int` is 64 bits and C's is
    /// 32; a mapping that put C `int` on wolf `int` would let a value
    /// that cannot fit through the seam typecheck as if it could.
    #[test]
    fn c_int_maps_to_i32_not_to_wolf_int() {
        let a = testkit::modelled_libc(TargetInfo::x86_64_linux());
        let memset = a.decl("memset").expect("present");
        let shape = call_shape(&a, memset).expect("callable");
        assert_eq!(
            shape.params,
            vec![WolfTy::RawPtr, WolfTy::I32, WolfTy::Uint],
            "memset is (void*, int, size_t): the middle argument is 32 bits"
        );
    }

    /// `size_t` and wolf's `uint` are both "the pointer-sized unsigned
    /// one" — they meet by meaning, and stay met on a 32-bit target.
    #[test]
    fn size_t_maps_to_uint_on_every_target() {
        for triple in [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc",
        ] {
            let t = TargetInfo::for_triple(triple).expect("tier-1");
            let a = testkit::modelled_libc(t);
            let malloc = a.decl("malloc").expect("present");
            let shape = call_shape(&a, malloc).expect("callable");
            assert_eq!(shape.params, vec![WolfTy::Uint], "{triple}");
            assert_eq!(shape.ret, WolfTy::RawPtr, "{triple}");
        }
    }

    /// The five intrinsics sema models today, reproduced from an
    /// artifact. This is the seam: when `c_call` consults the importer
    /// instead of a hardcoded match, these are the shapes it gets.
    #[test]
    fn the_modelled_five_come_out_of_an_artifact() {
        let a = testkit::modelled_libc(TargetInfo::x86_64_linux());
        let by_name: std::collections::BTreeMap<_, _> = callable(&a).into_iter().collect();
        assert_eq!(
            by_name.keys().cloned().collect::<Vec<_>>(),
            vec!["calloc", "free", "malloc", "memcpy", "memset"]
        );
        assert_eq!(by_name["free"].ret, WolfTy::Unit);
        assert_eq!(by_name["free"].params, vec![WolfTy::RawPtr]);
        assert_eq!(
            by_name["memcpy"].params,
            vec![WolfTy::RawPtr, WolfTy::RawPtr, WolfTy::Uint]
        );
        assert_eq!(
            by_name["calloc"].params,
            vec![WolfTy::Uint, WolfTy::Uint],
            "calloc is (size_t, size_t)"
        );
        // Every symbol is the C name: the `c.` prefix is wolf's
        // namespace, never part of the linker symbol.
        for (name, shape) in &by_name {
            assert_eq!(&shape.symbol, name);
            assert!(!shape.variadic);
        }
    }

    /// A declaration the worker refused must not become callable
    /// because the mapping happened to understand its types.
    #[test]
    fn the_mapping_never_launders_a_refusal() {
        let a = testkit::sample_artifact();
        let d = a.decl("cursed_union_arg").expect("the sample refuses one");
        let e = call_shape(&a, d).expect_err("must stay refused");
        assert_eq!(e.tag(), "union-active-member");
        assert!(
            !callable(&a).iter().any(|(n, _)| n == "cursed_union_arg"),
            "a refused decl must not appear in the callable set"
        );
    }

    /// Internal linkage means there is no symbol to call.
    #[test]
    fn internal_linkage_is_not_callable() {
        let a = testkit::sample_artifact();
        assert!(
            a.decl("internal_helper").is_some(),
            "the sample carries one"
        );
        assert!(
            !callable(&a).iter().any(|(n, _)| n == "internal_helper"),
            "a static function has no link-time symbol"
        );
    }

    /// A header-only `inline` has no symbol either, unless the worker
    /// built a companion for it.
    #[test]
    fn inline_only_without_a_shim_is_refused() {
        let a = testkit::sample_artifact();
        let d = a.decl("inline_no_shim").expect("the sample carries one");
        assert_eq!(
            call_shape(&a, d).expect_err("no symbol").tag(),
            "inline-without-shim"
        );
        // …and with a shim, it is callable.
        let d = a.decl("inline_with_shim").expect("the sample carries one");
        assert!(call_shape(&a, d).is_ok(), "the shim gives it a body");
    }

    #[test]
    fn long_double_is_refused_not_rounded() {
        let a = testkit::sample_artifact();
        let d = a.decl("takes_long_double").expect("the sample carries one");
        assert_eq!(call_shape(&a, d).expect_err("refused").tag(), "long-double");
    }
}
