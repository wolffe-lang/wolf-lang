//! Artifacts the tests and the conformance runner share.
//!
//! Not `#[cfg(test)]`: the conformance suite and the reference worker
//! both build artifacts from here, and an integration test in
//! `tests/` cannot see a unit-test module.

use crate::artifact::{
    Artifact, ConstValue, Decl, DeclKind, Linkage, MacroDef, MacroKind, ShimRequest, SourceLoc,
};
use crate::cache::ImportRequest;
use crate::ctype::{
    CType, CTypeId, EnumDef, Field, IntSpelling, Qual, Record, RecordId, RecordKind, TargetInfo,
};
use crate::refuse::{Refusal, Status};

/// The importer identity the built-in artifacts claim.
pub const BUILTIN_IMPORTER: &str = "wolf-cimport-builtin 1";

fn func(a: &mut Artifact, ret: CTypeId, params: Vec<CTypeId>, variadic: bool) -> CTypeId {
    a.types.intern(CType::Func {
        ret,
        params,
        variadic,
    })
}

fn add_fn(a: &mut Artifact, name: &str, ty: CTypeId, line: u32) {
    a.decls.push(Decl {
        name: name.to_string(),
        wolf_name: name.to_string(),
        kind: DeclKind::Func {
            ty,
            inline_only: false,
        },
        linkage: Linkage::External,
        loc: SourceLoc {
            file: 0,
            line,
            col: 1,
        },
        status: Status::Ok,
    });
}

/// The five allocator/memory intrinsics the compiler models today
/// (`malloc`, `calloc`, `free`, `memset`, `memcpy`), expressed as a
/// real importer artifact for `target`.
///
/// This is the bridge between the hardcoded table in `wolf_sema`'s
/// `c_call` and the importer: the same five functions, but sourced from
/// an artifact and typed from C's actual signatures, so the seam can be
/// switched over without inventing the shapes twice.
pub fn modelled_libc(target: TargetInfo) -> Artifact {
    let mut a = Artifact::new(BUILTIN_IMPORTER, target.clone());
    a.headers = vec!["stdlib.h".into(), "string.h".into()];
    a.files = vec!["<builtin>/stdlib.h".into(), "<builtin>/string.h".into()];

    let void = a.types.void();
    let vp = a.types.void_ptr();
    let cvp = a.types.ptr_to(
        void,
        Qual {
            is_const: true,
            ..Qual::default()
        },
    );
    let size_t = a.types.size_t(&target);
    let int_ = a.types.int_(&target);

    let t = func(&mut a, vp, vec![size_t], false);
    add_fn(&mut a, "malloc", t, 540);
    let t = func(&mut a, vp, vec![size_t, size_t], false);
    add_fn(&mut a, "calloc", t, 544);
    let t = func(&mut a, void, vec![vp], false);
    add_fn(&mut a, "free", t, 565);
    let t = func(&mut a, vp, vec![vp, int_, size_t], false);
    add_fn(&mut a, "memset", t, 61);
    let t = func(&mut a, vp, vec![vp, cvp, size_t], false);
    add_fn(&mut a, "memcpy", t, 43);
    // `memcpy`'s source is `const void *` in C; the mapping puts both
    // on the raw floor, and the qualifier is kept in the artifact
    // because the day wolf has a `const` story it will want it.

    a.canonicalize();
    a
}

/// The request `modelled_libc` answers — used by the cache tests.
pub fn sample_request() -> ImportRequest {
    ImportRequest {
        headers: vec!["stdlib.h".into(), "string.h".into()],
        defines: vec![("_GNU_SOURCE".into(), "1".into())],
        cflags: Vec::new(),
        include_paths: vec!["/usr/include".into()],
        target: "x86_64-unknown-linux-gnu".into(),
        sysroot: None,
    }
}

/// An artifact that exercises **every shape the interface has to
/// carry**: a bitfield struct, a union (which always demotes), an
/// enum, a tag/ordinary name collision, internal linkage, an inline
/// function with and without a companion shim, a refused parameter
/// type, both macro kinds, and a refused macro of each class.
///
/// The conformance suite grows from this: every demotion or mapping
/// bug that gets fixed adds a case here.
pub fn sample_artifact() -> Artifact {
    let target = TargetInfo::x86_64_linux();
    let mut a = Artifact::new("test-worker 1", target.clone());
    a.headers = vec!["sample.h".into()];
    a.files = vec!["/fixtures/sample.h".into()];

    let void = a.types.void();
    let vp = a.types.void_ptr();
    let int_ = a.types.int_(&target);
    let size_t = a.types.size_t(&target);
    let uint = a.types.intern(CType::Int {
        bits: 32,
        signed: false,
        spelling: IntSpelling::Int,
    });
    let long_double = a.types.intern(CType::Float { bits: 80 });

    // ---- a struct with bitfields, laid out for the target ----------
    let flags = RecordId(0);
    let flags_ty = a.types.intern(CType::Record(flags));
    a.records.push(Record {
        name: "flags".into(),
        kind: RecordKind::Struct,
        size_bytes: Some(4),
        align_bytes: Some(4),
        opaque: false,
        fields: vec![
            Field {
                name: "a".into(),
                ty: uint,
                offset_bits: 0,
                bit_width: Some(1),
            },
            Field {
                name: "b".into(),
                ty: uint,
                offset_bits: 1,
                bit_width: Some(3),
            },
            Field {
                name: "rest".into(),
                ty: uint,
                offset_bits: 4,
                bit_width: Some(28),
            },
        ],
    });

    // ---- a union: always opaque, by name ---------------------------
    let onion = RecordId(1);
    let onion_ty = a.types.intern(CType::Record(onion));
    a.records.push(Record {
        name: "value".into(),
        kind: RecordKind::Union,
        size_bytes: Some(8),
        align_bytes: Some(8),
        opaque: true,
        fields: vec![
            Field {
                name: "i".into(),
                ty: int_,
                offset_bits: 0,
                bit_width: None,
            },
            Field {
                name: "p".into(),
                ty: vp,
                offset_bits: 0,
                bit_width: None,
            },
        ],
    });

    // ---- an enum ---------------------------------------------------
    let color = crate::ctype::EnumId(0);
    let color_ty = a.types.intern(CType::Enum(color));
    a.enums.push(EnumDef {
        name: "color".into(),
        underlying: uint,
        constants: vec![("RED".into(), 0), ("GREEN".into(), 1), ("BLUE".into(), 2)],
    });

    // ---- ordinary functions ----------------------------------------
    let t = func(&mut a, vp, vec![size_t], false);
    add_fn(&mut a, "sample_alloc", t, 12);

    let t = func(&mut a, int_, vec![color_ty], false);
    add_fn(&mut a, "sample_color", t, 18);

    // Bitfield structs cross by pointer, which is fine and callable.
    let fp = a.types.ptr_to(flags_ty, Qual::default());
    let t = func(&mut a, void, vec![fp], false);
    add_fn(&mut a, "sample_flags", t, 24);

    // ---- the cursed one: a union argument, refused by name ---------
    let t = func(&mut a, void, vec![onion_ty], false);
    a.decls.push(Decl {
        name: "cursed_union_arg".into(),
        wolf_name: "cursed_union_arg".into(),
        kind: DeclKind::Func {
            ty: t,
            inline_only: false,
        },
        linkage: Linkage::External,
        loc: SourceLoc {
            file: 0,
            line: 31,
            col: 6,
        },
        status: Status::refuse(Refusal::UnionActiveMember),
    });

    // ---- long double: refused rather than rounded ------------------
    let t = func(&mut a, long_double, vec![long_double], false);
    a.decls.push(Decl {
        name: "takes_long_double".into(),
        wolf_name: "takes_long_double".into(),
        kind: DeclKind::Func {
            ty: t,
            inline_only: false,
        },
        linkage: Linkage::External,
        loc: SourceLoc {
            file: 0,
            line: 37,
            col: 13,
        },
        status: Status::Ok,
    });

    // ---- internal linkage: a value, never a symbol -----------------
    let t = func(&mut a, int_, vec![], false);
    a.decls.push(Decl {
        name: "internal_helper".into(),
        wolf_name: "internal_helper".into(),
        kind: DeclKind::Func {
            ty: t,
            inline_only: false,
        },
        linkage: Linkage::Internal,
        loc: SourceLoc {
            file: 0,
            line: 41,
            col: 12,
        },
        status: Status::Ok,
    });

    // ---- static inline, with and without a companion shim ----------
    let t = func(&mut a, int_, vec![int_], false);
    for (name, line) in [("inline_no_shim", 45), ("inline_with_shim", 49)] {
        a.decls.push(Decl {
            name: name.into(),
            wolf_name: name.into(),
            kind: DeclKind::Func {
                ty: t,
                inline_only: true,
            },
            linkage: Linkage::External,
            loc: SourceLoc {
                file: 0,
                line,
                col: 19,
            },
            status: Status::Ok,
        });
    }
    a.shims.push(ShimRequest {
        function: "inline_with_shim".into(),
        source: "#include \"sample.h\"\nint inline_with_shim(int x) { return \
                 inline_with_shim(x); }\n"
            .into(),
    });

    // ---- a tag/ordinary name collision (c23-n3220 §6.2.3) ----------
    // The header has both `struct stat` and `int stat(...)`. C keeps
    // them in separate name spaces; wolf has one `c` namespace, so the
    // tag is renamed — visibly.
    let stat_rec = RecordId(2);
    let stat_ty = a.types.intern(CType::Record(stat_rec));
    a.records.push(Record {
        name: "stat".into(),
        kind: RecordKind::Struct,
        size_bytes: Some(144),
        align_bytes: Some(8),
        opaque: false,
        fields: vec![Field {
            name: "st_size".into(),
            ty: size_t,
            offset_bits: 384,
            bit_width: None,
        }],
    });
    a.decls.push(Decl {
        name: "stat".into(),
        wolf_name: "struct_stat".into(),
        kind: DeclKind::Tag { ty: stat_ty },
        linkage: Linkage::None,
        loc: SourceLoc {
            file: 0,
            line: 55,
            col: 8,
        },
        status: Status::Ok,
    });
    let sp = a.types.ptr_to(stat_ty, Qual::default());
    let ch = a.types.char_(&target);
    let cp = a.types.ptr_to(
        ch,
        Qual {
            is_const: true,
            ..Qual::default()
        },
    );
    let t = func(&mut a, int_, vec![cp, sp], false);
    add_fn(&mut a, "stat", t, 60);

    // ---- an incomplete type ----------------------------------------
    let opaque_rec = RecordId(3);
    let opaque_ty = a.types.intern(CType::Record(opaque_rec));
    a.records.push(Record {
        name: "FILE".into(),
        kind: RecordKind::Struct,
        size_bytes: None,
        align_bytes: None,
        opaque: true,
        fields: Vec::new(),
    });
    a.decls.push(Decl {
        name: "FILE".into(),
        wolf_name: "struct_FILE".into(),
        kind: DeclKind::Tag { ty: opaque_ty },
        linkage: Linkage::None,
        loc: SourceLoc {
            file: 0,
            line: 66,
            col: 8,
        },
        status: Status::refuse(Refusal::IncompleteType),
    });

    // ---- enum constants as declarations ----------------------------
    for (n, v) in [("RED", 0i128), ("GREEN", 1), ("BLUE", 2)] {
        a.decls.push(Decl {
            name: n.into(),
            wolf_name: n.into(),
            kind: DeclKind::EnumConst {
                ty: color_ty,
                value: v,
            },
            linkage: Linkage::None,
            loc: SourceLoc {
                file: 0,
                line: 8,
                col: 5,
            },
            status: Status::Ok,
        });
    }

    // ---- macros ----------------------------------------------------
    a.macros.push(MacroDef {
        name: "EOF".into(),
        kind: MacroKind::Object {
            value: Some(ConstValue::Int {
                value: -1,
                ty: int_,
            }),
            tokens: vec!["(".into(), "-".into(), "1".into(), ")".into()],
        },
        loc: SourceLoc {
            file: 0,
            line: 3,
            col: 9,
        },
        status: Status::Ok,
    });
    a.macros.push(MacroDef {
        name: "SEEK_SET".into(),
        kind: MacroKind::Object {
            value: Some(ConstValue::Int { value: 0, ty: int_ }),
            tokens: vec!["0".into()],
        },
        loc: SourceLoc {
            file: 0,
            line: 4,
            col: 9,
        },
        status: Status::Ok,
    });
    a.macros.push(MacroDef {
        name: "GREETING".into(),
        kind: MacroKind::Object {
            value: Some(ConstValue::Str(b"hello".to_vec())),
            tokens: vec!["\"hello\"".into()],
        },
        loc: SourceLoc {
            file: 0,
            line: 5,
            col: 9,
        },
        status: Status::Ok,
    });
    // The FD_SET shape: a function-like macro that expands to an
    // expression over its arguments. Alive, not translated.
    a.macros.push(MacroDef {
        name: "SAMPLE_SET".into(),
        kind: MacroKind::Function {
            params: vec!["d".into(), "s".into()],
            variadic: false,
            tokens: vec![
                "(".into(),
                "(".into(),
                "s".into(),
                ")".into(),
                "->".into(),
                "bits".into(),
                "|=".into(),
                "(".into(),
                "1".into(),
                "<<".into(),
                "(".into(),
                "d".into(),
                ")".into(),
                ")".into(),
                ")".into(),
            ],
        },
        loc: SourceLoc {
            file: 0,
            line: 70,
            col: 9,
        },
        status: Status::Ok,
    });
    // One of each deferred class, refused by name.
    for (name, refusal, line) in [
        ("SAMPLE_CONTAINER_OF", Refusal::MacroTypeArgument, 74u32),
        ("SAMPLE_STMT", Refusal::MacroExpandsToStatement, 78),
        ("SAMPLE_PASTE", Refusal::MacroTokenPasting, 82),
        ("SAMPLE_NOT_CONST", Refusal::MacroNotConstant, 86),
    ] {
        a.macros.push(MacroDef {
            name: name.into(),
            kind: MacroKind::Function {
                params: vec!["x".into()],
                variadic: false,
                tokens: vec!["/*".into(), "see".into(), "header".into(), "*/".into()],
            },
            loc: SourceLoc {
                file: 0,
                line,
                col: 9,
            },
            status: Status::refuse(refusal),
        });
    }

    a.canonicalize();
    a
}
