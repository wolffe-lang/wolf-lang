//! The build's target (kw04; spec/04 `[abi.target]`, STATUS #31 K1,
//! K8, K10).
//!
//! A build has one target. The hosted targets are the host the compiler
//! runs on (the `[abi.c.targets]` matrix); the one freestanding target
//! is `x86_64-unknown-none` (`[abi.target.none]`): objects only, no
//! `main` shim, no runtime library, and imports limited to the hook list
//! (`[abi.target.none.hooks]`) plus the program's own `extern`
//! declarations.
//!
//! This module owns the two facts every backend and the driver share:
//! the target value itself, and [`freestanding_refusal`] — the scan
//! that refuses, BY NAME and before any backend runs, every construct a
//! freestanding object could not carry: one that needs the hosted
//! runtime, one that allocates outside the freestanding runtime
//! (`[abi.target.none.alloc]`), and every float
//! (`[abi.target.none.codegen]`, K10(b) = A). The scan reads the
//! lowered WIR, so it is complete by construction: anything that would
//! import a runtime symbol is a call to one there.
//!
//! kw12 (K8(b) = B): the allocating constructs a kernel needs — `List`,
//! `Map`, string interpolation, a capturing closure and `region` —
//! compile against [`NONE_RT_SYMBOLS`], which the `no_std` runtime
//! archive [`NONE_RT_LIB`] defines over the program's allocator hooks
//! ([`ALLOC_HOOKS`]). Every other runtime symbol is still refused.

use wolf_wir::ir::{Aux, Function, Module, ValueDef};
use wolf_wir::ops::Opcode;
use wolf_wir::types::{TypeData, TypeId};

/// The freestanding target's triple, as the flag and the manifest spell
/// it (K1).
pub const FREESTANDING: &str = "x86_64-unknown-none";

/// The one trap hook (`[abi.target.none.hooks]`, K8(a)): `wolf_trap(kind:
/// i32, file: *u8, file_len: i64, line: i64, col: i64)`, never returning.
/// The program supplies it on the freestanding target; the hosted
/// runtime defines it as the report-and-exit every trap already takes.
pub const TRAP_HOOK: &str = "wolf_trap";

/// The memory functions a freestanding object may import with C's
/// meaning (`[abi.target.none.hooks]` (b)): every freestanding
/// toolchain's floor, and what a backend may emit for an aggregate copy.
pub const MEM_HOOKS: [&str; 4] = ["memcpy", "memmove", "memset", "memcmp"];

/// The allocator hooks (`[abi.target.none.hooks]` (d), K8(b) = B, kw12):
/// `wolf_alloc(size: i64, align: i64) -> *u8` and `wolf_free(p: *u8,
/// size: i64, align: i64)`. The program supplies them; only the
/// freestanding runtime archive ([`NONE_RT_LIB`]) imports them.
pub const ALLOC_HOOKS: [&str; 2] = ["wolf_alloc", "wolf_free"];

/// The freestanding runtime archive (kw12, `[abi.target.none.alloc]`):
/// the `no_std` build of the region runtime (crate `wolf_rt_none`), the
/// same file name on every host. A freestanding build whose object
/// imports any of [`NONE_RT_SYMBOLS`] writes it beside the object.
pub const NONE_RT_LIB: &str = "libwolf_rt_none.a";

/// The runtime symbols [`NONE_RT_LIB`] defines — the hosted runtime's
/// names, signatures and layouts for `List`, `Map`, string interpolation
/// (every hole but `f64`), a capturing closure and `region`. A call to
/// one of these is admitted on the freestanding target; a call to any
/// other runtime symbol is refused by name.
pub const NONE_RT_SYMBOLS: &[&str] = &[
    "__wolf_rt_region_new",
    "__wolf_rt_region_alloc",
    "__wolf_rt_region_free",
    "__wolf_rt_region_set_cap",
    "__wolf_rt_region_bytes",
    "__wolf_rt_live_region_bytes",
    "__wolf_rt_region_ambient_enter",
    "__wolf_rt_region_ambient_leave",
    "__wolf_rt_closure_alloc",
    "__wolf_rt_list_new",
    "__wolf_rt_list_push",
    "__wolf_rt_list_pop",
    "__wolf_rt_list_read",
    "__wolf_rt_list_write",
    "__wolf_rt_list_len",
    "__wolf_rt_list_clear",
    "__wolf_rt_list_copy",
    "__wolf_rt_map_new",
    "__wolf_rt_map_get",
    "__wolf_rt_map_set",
    "__wolf_rt_map_remove",
    "__wolf_rt_map_pairs",
    "__wolf_rt_map_copy",
    "__wolf_rt_map_clear",
    "__wolf_rt_strbuf_new",
    "__wolf_rt_strbuf_str",
    "__wolf_rt_strbuf_i64",
    "__wolf_rt_strbuf_bool",
    "__wolf_rt_strbuf_char",
    "__wolf_rt_strbuf_finish",
];

/// Does the freestanding runtime define `symbol`?
pub fn none_rt_provides(symbol: &str) -> bool {
    NONE_RT_SYMBOLS.contains(&symbol)
}

/// The build's target.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Target {
    /// The host the compiler runs on (every build before kw04).
    #[default]
    Host,
    /// `x86_64-unknown-none` (`[abi.target.none]`).
    X86_64None,
}

impl Target {
    /// Parse a triple the flag or the manifest names. The host's own
    /// triple (`host`) is the hosted build; any other hosted triple is
    /// a cross build no tier performs yet.
    pub fn parse(triple: &str, host: &str) -> Result<Target, String> {
        if triple == FREESTANDING {
            Ok(Target::X86_64None)
        } else if triple == host {
            Ok(Target::Host)
        } else {
            Err(format!(
                "unknown target `{triple}` (this compiler builds `{FREESTANDING}` and its \
                 host, `{host}`)"
            ))
        }
    }

    /// True for the freestanding target.
    pub fn is_freestanding(self) -> bool {
        self == Target::X86_64None
    }
}

/// Why a construct cannot be compiled for the freestanding target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefusalClass {
    /// It calls into the hosted runtime (`[abi.target.none]`).
    HostedRuntime,
    /// It allocates, and the freestanding runtime does not carry it
    /// (`[abi.target.none.alloc]`).
    Allocates,
    /// It computes with a float (`[abi.target.none.codegen]`, K10(b) = A).
    Float,
}

/// One refusal: the construct by name, why, and where.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Refusal {
    /// The construct, spelled as the programmer wrote it (`` `print` ``,
    /// `` `List` ``, `string interpolation`, `` `f64` ``).
    pub construct: String,
    pub class: RefusalClass,
    /// Byte span in the function's source file, when it has one.
    pub span: Option<(u32, u32)>,
}

impl Refusal {
    /// The refusal's sentence, naming the construct and the target.
    pub fn message(&self) -> String {
        match self.class {
            RefusalClass::HostedRuntime => format!(
                "{} needs the hosted runtime (target {FREESTANDING})",
                self.construct
            ),
            RefusalClass::Allocates => format!(
                "{} allocates, and the freestanding runtime ({NONE_RT_LIB}) does not carry \
                 it (target {FREESTANDING})",
                self.construct
            ),
            RefusalClass::Float => format!(
                "a floating-point value ({}) on target {FREESTANDING} (K10: kernel \
                 code keeps no SSE or x87 state)",
                self.construct
            ),
        }
    }
}

/// The source construct a runtime symbol stands for, and whether it
/// allocates. Total: an unknown runtime symbol is still refused, under
/// its own name.
pub fn runtime_construct(symbol: &str) -> (String, RefusalClass) {
    use RefusalClass::{Allocates, HostedRuntime};
    let s = symbol.strip_prefix("__wolf_rt_").unwrap_or(symbol);
    let has = |p: &str| s.starts_with(p);
    let (name, class): (String, RefusalClass) = if has("print_") {
        ("`print`".into(), HostedRuntime)
    } else if has("write_") {
        ("stream output (`write`)".into(), HostedRuntime)
    } else if s == "read_line" {
        ("`read_line`".into(), HostedRuntime)
    } else if s == "strbuf_f64" {
        // Reached only if no float value was refused first (K10(b)).
        ("`f64`".into(), RefusalClass::Float)
    } else if has("strbuf_") {
        ("string interpolation".into(), Allocates)
    } else if s == "str_eq" || s == "str_cmp" {
        ("`str` comparison".into(), HostedRuntime)
    } else if let Some(op) = s.strip_prefix("str_") {
        (format!("`str.{op}`"), HostedRuntime)
    } else if has("list_") {
        ("`List`".into(), Allocates)
    } else if has("map_") {
        ("`Map`".into(), Allocates)
    } else if has("pool_") {
        ("`Pool`".into(), Allocates)
    } else if has("region_") || s == "live_region_bytes" {
        ("`region`".into(), Allocates)
    } else if s == "closure_alloc" {
        ("a capturing closure".into(), Allocates)
    } else if has("box_") {
        ("a boxed channel payload".into(), Allocates)
    } else if has("chan_") || s == "select" {
        ("a channel".into(), HostedRuntime)
    } else if has("scope_") || has("task_") {
        ("`spawn`".into(), HostedRuntime)
    } else if has("par_") {
        ("`par`".into(), HostedRuntime)
    } else if has("proc_") {
        ("`spawn proc`".into(), HostedRuntime)
    } else if has("sync_") || has("when_") {
        ("`sync`/`when`".into(), HostedRuntime)
    } else if s == "main_err" {
        ("an error-union `main`".into(), HostedRuntime)
    } else if ["env_", "os_", "time_", "fs_", "net_", "json_", "bytes_"]
        .iter()
        .any(|p| has(p))
    {
        (format!("`{s}`"), HostedRuntime)
    } else {
        (format!("the runtime operation `{symbol}`"), HostedRuntime)
    };
    (name, class)
}

/// Does `ty` carry a float anywhere (an aggregate's field, an error
/// union's payload)?
fn float_in(m: &Module, ty: TypeId) -> Option<&'static str> {
    match m.types.get(ty) {
        TypeData::F32 => Some("`f32`"),
        TypeData::F64 => Some("`f64`"),
        TypeData::Agg(fields) => fields.iter().find_map(|&t| float_in(m, t)),
        TypeData::Eu { ok, slots } => ok.iter().chain(slots.iter()).find_map(|&t| float_in(m, t)),
        _ => None,
    }
}

/// The first span any instruction of `f` carries (the function's own
/// coordinates, for a refusal with no instruction of its own).
fn first_span(f: &Function) -> Option<(u32, u32)> {
    f.layout
        .iter()
        .flat_map(|&b| f.blocks[b].insts.iter())
        .find_map(|&i| f.srcspan(i))
        .map(|sp| (sp.lo, sp.hi))
}

/// Does the lowered `m` call into the freestanding runtime — a region op,
/// or a call to one of [`NONE_RT_SYMBOLS`]? (kw12: such an object links
/// [`NONE_RT_LIB`]; one that does not imports exactly the hook list.)
pub fn uses_none_rt(m: &Module) -> bool {
    m.funcs.values().any(|f| {
        f.ext_funcs.values().any(|e| none_rt_provides(&e.name))
            || f.layout.iter().any(|&b| {
                f.blocks[b].insts.iter().any(|&i| {
                    matches!(
                        f.insts[i].op,
                        Opcode::RegionNew | Opcode::RegionAlloc | Opcode::RegionFree
                    )
                })
            })
    })
}

/// The first construct in `m` the freestanding target cannot carry, in
/// function order and, within a function, instruction order; `None`
/// when the module is freestanding-clean. Run on the LOWERED module,
/// before the mid-end: a construct the optimizer would later fold away
/// is still the program's, and both tiers must answer alike.
pub fn freestanding_refusal(m: &Module) -> Option<Refusal> {
    for (_, f) in m.funcs.iter() {
        if let Some(r) = function_refusal(m, f) {
            return Some(r);
        }
    }
    None
}

fn function_refusal(m: &Module, f: &Function) -> Option<Refusal> {
    let at = |inst| f.srcspan(inst).map(|sp| (sp.lo, sp.hi)).or(first_span(f));
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            let data = &f.insts[inst];
            match (data.op, data.aux) {
                (Opcode::Call, Aux::Callee(ext)) => {
                    let name = &f.ext_funcs[ext].name;
                    if name.starts_with("__wolf_rt_") && !none_rt_provides(name) {
                        let (construct, class) = runtime_construct(name);
                        return Some(Refusal {
                            construct,
                            class,
                            span: at(inst),
                        });
                    }
                }
                // `region` (kw12): the freestanding runtime carries the
                // region family, so the three region ops lower as hosted.
                (Opcode::SyncFreeze, _) => {
                    return Some(Refusal {
                        construct: "`freeze`".into(),
                        class: RefusalClass::HostedRuntime,
                        span: at(inst),
                    });
                }
                _ => {}
            }
            for v in f.vpool.get(data.results) {
                if let Some(ty) = float_in(m, f.value_ty(v)) {
                    return Some(Refusal {
                        construct: ty.into(),
                        class: RefusalClass::Float,
                        span: at(inst),
                    });
                }
            }
        }
    }
    // Floats no instruction produces: a parameter, a block parameter,
    // the signature's results.
    for (_, vd) in f.values.iter() {
        if let Some(ty) = float_in(m, vd.ty) {
            let span = match vd.def {
                ValueDef::Result(inst, _) => at(inst),
                ValueDef::Param(..) => first_span(f),
            };
            return Some(Refusal {
                construct: ty.into(),
                class: RefusalClass::Float,
                span,
            });
        }
    }
    let sd = &m.sigs[f.sig];
    if let Some(ty) = sd
        .params
        .iter()
        .map(|p| p.ty)
        .chain(sd.results.iter().copied())
        .find_map(|t| float_in(m, t))
    {
        return Some(Refusal {
            construct: ty.into(),
            class: RefusalClass::Float,
            span: first_span(f),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_names_the_freestanding_target_or_the_host() {
        let host = "x86_64-unknown-linux-gnu";
        assert_eq!(Target::parse(FREESTANDING, host), Ok(Target::X86_64None));
        assert_eq!(Target::parse(host, host), Ok(Target::Host));
        let e = Target::parse("aarch64-apple-darwin", host).unwrap_err();
        assert!(e.contains("unknown target `aarch64-apple-darwin`"), "{e}");
        assert!(e.contains(FREESTANDING) && e.contains(host), "{e}");
        assert!(Target::X86_64None.is_freestanding());
        assert!(!Target::Host.is_freestanding());
        assert_eq!(Target::default(), Target::Host);
    }

    #[test]
    fn every_runtime_family_has_a_surface_name() {
        let cases = [
            (
                "__wolf_rt_print_str",
                "`print`",
                RefusalClass::HostedRuntime,
            ),
            ("__wolf_rt_list_push", "`List`", RefusalClass::Allocates),
            ("__wolf_rt_map_new", "`Map`", RefusalClass::Allocates),
            ("__wolf_rt_pool_new", "`Pool`", RefusalClass::Allocates),
            (
                "__wolf_rt_strbuf_new",
                "string interpolation",
                RefusalClass::Allocates,
            ),
            (
                "__wolf_rt_closure_alloc",
                "a capturing closure",
                RefusalClass::Allocates,
            ),
            ("__wolf_rt_region_new", "`region`", RefusalClass::Allocates),
            (
                "__wolf_rt_scope_spawn",
                "`spawn`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_str_eq",
                "`str` comparison",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_str_trim",
                "`str.trim`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_env_args",
                "`env_args`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_fs_open",
                "`fs_open`",
                RefusalClass::HostedRuntime,
            ),
            // s199 (#426): positional I/O is hosted surface by family.
            (
                "__wolf_rt_fs_seek",
                "`fs_seek`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_fs_tell",
                "`fs_tell`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_fs_read_at",
                "`fs_read_at`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_read_line",
                "`read_line`",
                RefusalClass::HostedRuntime,
            ),
            // s200 (#417, #407, #411): the fused copy and the task's
            // host code by the families they joined; the byte scan
            // names itself (`bytes_`), never "the runtime operation".
            (
                "__wolf_rt_fs_copy_chunk",
                "`fs_copy_chunk`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_os_error",
                "`os_error`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_bytes_find",
                "`bytes_find`",
                RefusalClass::HostedRuntime,
            ),
            (
                "__wolf_rt_unheard_of",
                "the runtime operation `__wolf_rt_unheard_of`",
                RefusalClass::HostedRuntime,
            ),
        ];
        for (sym, name, class) in cases {
            assert_eq!(runtime_construct(sym), (name.to_string(), class), "{sym}");
        }
    }

    /// kw12: the freestanding runtime's symbols are the allocating
    /// families' and nothing else — no scheduler, no I/O, no `Pool`, no
    /// float hole.
    #[test]
    fn the_freestanding_runtime_carries_the_allocating_families_only() {
        for s in NONE_RT_SYMBOLS {
            let (name, class) = runtime_construct(s);
            assert_eq!(class, RefusalClass::Allocates, "{s}");
            assert!(
                [
                    "`List`",
                    "`Map`",
                    "string interpolation",
                    "a capturing closure",
                    "`region`"
                ]
                .contains(&name.as_str()),
                "{s}: {name}"
            );
            assert!(none_rt_provides(s));
        }
        for s in [
            "__wolf_rt_pool_new",
            "__wolf_rt_region_freeze",
            "__wolf_rt_strbuf_f64",
            "__wolf_rt_scope_spawn",
            "__wolf_rt_proc_spawn",
            "__wolf_rt_print_str",
            "__wolf_rt_str_eq",
            "__wolf_rt_trap_at",
        ] {
            assert!(!none_rt_provides(s), "{s}");
        }
        assert_eq!(
            runtime_construct("__wolf_rt_strbuf_f64"),
            ("`f64`".to_string(), RefusalClass::Float)
        );
        assert_eq!(ALLOC_HOOKS, ["wolf_alloc", "wolf_free"]);
    }

    #[test]
    fn the_message_names_the_construct_and_the_target() {
        let r = Refusal {
            construct: "`List`".into(),
            class: RefusalClass::Allocates,
            span: None,
        };
        let m = r.message();
        assert!(m.starts_with("`List` allocates"), "{m}");
        assert!(m.contains(NONE_RT_LIB), "{m}");
        assert!(m.contains("target x86_64-unknown-none"), "{m}");
        let f = Refusal {
            construct: "`f64`".into(),
            class: RefusalClass::Float,
            span: None,
        };
        assert!(f.message().contains("floating-point value (`f64`)"));
    }
}
