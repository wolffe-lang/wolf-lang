//! The comptime intrinsics allowlist and the D33 sandbox tables.
//!
//! Everything callable at comptime is *enumerated here*: the
//! reflection intrinsics ([`Intrinsic`]) and user functions with
//! checked bodies. Everything else diagnoses — the host-stub table
//! maps each ambient-IO surface to its [`SandboxCategory`], and each
//! category names *why* it is refused (determinism or confinement),
//! because "not available at compile time" without a reason teaches
//! nothing (VOICE rule 3).
//!
//! Reflection consumes and produces **semantic values only** — types,
//! fields, variants, trait names — never syntax trees (the D29
//! amendment). No API in this module can observe or construct AST.

use wolf_ast::EnumDecl;
use wolf_span::Span;

use crate::sig::ItemSig;
use crate::traits;
use crate::types::{Prim, TyId, TyKind, TypeTable};

use super::eval::{Engine, FaultKind};
use super::value::{CtType, ValId, ValueKind};

/// The sentinel module of engine-built meta values (`TypeInfo`,
/// `Field`): not a user module, never resolvable.
pub const META_MODULE: u32 = u32::MAX;

/// The explicit comptime intrinsics allowlist (D33). Closed set:
/// growing it is a design decision, not a convenience.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intrinsic {
    /// `typeinfo(T)` / `reflect(T)` → a `TypeInfo` value.
    TypeInfo,
    /// `typebuild(desc)` → a synthesized struct type.
    TypeBuild,
    /// `implements(T, Trait)` → bool.
    Implements,
    /// `size_of(T)` → int for primitives; unresolved-until-codegen
    /// (E0708) for everything c05 has not laid out.
    SizeOf,
    /// `assert(cond)` — the one comptime assertion (E0710 on failure).
    Assert,
}

/// Which ambient surface a refused call reaches, and why it is
/// refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SandboxCategory {
    Io,
    Fs,
    Net,
    Env,
    Clock,
    Random,
    Ffi,
}

impl SandboxCategory {
    /// What the call reaches.
    pub fn surface(self) -> &'static str {
        match self {
            SandboxCategory::Io => "ambient output",
            SandboxCategory::Fs => "the filesystem",
            SandboxCategory::Net => "the network",
            SandboxCategory::Env => "environment variables",
            SandboxCategory::Clock => "the clock",
            SandboxCategory::Random => "randomness",
            SandboxCategory::Ffi => "native code",
        }
    }

    /// The named reason (D33): determinism, confinement, or both.
    pub fn reason(self) -> &'static str {
        match self {
            SandboxCategory::Io => {
                "confinement: compiling a package must never act on the machine \
                 that compiles it"
            }
            SandboxCategory::Fs => {
                "confinement: a build must not read the machine it runs on — and \
                 the same source would compile differently on different machines"
            }
            SandboxCategory::Net => {
                "confinement: `wolf add` must never mean arbitrary code talks to \
                 the network with your credentials"
            }
            SandboxCategory::Env => {
                "determinism and confinement: environment contents differ per \
                 machine and may hold secrets"
            }
            SandboxCategory::Clock => {
                "determinism: two identical builds must not observe different times"
            }
            SandboxCategory::Random => {
                "determinism: the same program and target must produce bit-identical \
                 comptime results on every host"
            }
            SandboxCategory::Ffi => {
                "confinement: the sandbox cannot see into native code, so it never \
                 runs any"
            }
        }
    }
}

/// The intrinsic a bare name denotes, if any.
pub fn intrinsic(name: &str) -> Option<Intrinsic> {
    Some(match name {
        "typeinfo" | "reflect" => Intrinsic::TypeInfo,
        "typebuild" => Intrinsic::TypeBuild,
        "implements" => Intrinsic::Implements,
        "size_of" => Intrinsic::SizeOf,
        "assert" => Intrinsic::Assert,
        _ => return None,
    })
}

/// The sandbox category a host-stub name reaches, if it is one. These
/// are the prelude's ambient host surfaces — callable at runtime,
/// *never* at comptime.
pub fn host_stub(name: &str) -> Option<SandboxCategory> {
    Some(match name {
        "print" | "print_raw" => SandboxCategory::Io,
        "read_text" => SandboxCategory::Fs,
        "net_fetch" => SandboxCategory::Net,
        "env_var" => SandboxCategory::Env,
        "clock_ms" => SandboxCategory::Clock,
        "random_seed" => SandboxCategory::Random,
        _ => return None,
    })
}

/// Convert a checker type to a self-contained comptime type value at
/// the engine boundary (never the other way inside results).
pub fn ty_to_ct(table: &TypeTable, ty: TyId) -> CtType {
    match table.kind(ty) {
        TyKind::Unit => CtType::Unit,
        TyKind::Prim(p) => CtType::Prim(*p),
        TyKind::TypeTy => CtType::TypeTy,
        TyKind::Tuple(ts) => CtType::Tuple(ts.iter().map(|&t| ty_to_ct(table, t)).collect()),
        TyKind::Nominal { module, name } => CtType::Nominal {
            module: *module,
            name: name.clone(),
        },
        _ => CtType::Opaque(crate::types::render(table, ty, &|_| Err("_"))),
    }
}

impl<'a> Engine<'a> {
    /// Evaluate an allowlisted intrinsic against argument values.
    pub(super) fn eval_intrinsic(
        &mut self,
        i: Intrinsic,
        args: &[ValId],
        _span: Span,
    ) -> Result<ValId, FaultKind> {
        match i {
            Intrinsic::Assert => {
                let Some(&c) = args.first() else {
                    return Err(FaultKind::EngineGap {
                        construct: "`assert` without an argument",
                    });
                };
                match self.arena.kind(c) {
                    ValueKind::Bool(true) => Ok(self.arena.unit()),
                    ValueKind::Bool(false) => {
                        // The optional second argument rides into the
                        // E0710 rendering when it is a plain string;
                        // anything else is silently dropped (#9).
                        let msg = args.get(1).and_then(|&m| match self.arena.kind(m) {
                            ValueKind::Str(s) => Some(s.clone()),
                            _ => None,
                        });
                        Err(FaultKind::AssertFailed { msg })
                    }
                    _ => Err(FaultKind::EngineGap {
                        construct: "a non-boolean `assert` at comptime",
                    }),
                }
            }
            Intrinsic::TypeInfo => {
                let Some(ct) = self.type_arg(args) else {
                    return Err(FaultKind::NotComptime {
                        what: "`typeinfo` needs a type value".to_string(),
                    });
                };
                Ok(self.build_typeinfo(&ct))
            }
            Intrinsic::TypeBuild => {
                let Some(&d) = args.first() else {
                    return Err(FaultKind::EngineGap {
                        construct: "`typebuild` without a descriptor",
                    });
                };
                self.build_type(d)
            }
            Intrinsic::SizeOf => {
                let Some(ct) = self.type_arg(args) else {
                    return Err(FaultKind::NotComptime {
                        what: "`size_of` needs a type value".to_string(),
                    });
                };
                match ct {
                    CtType::Prim(p) => match prim_size(p) {
                        Some(n) => Ok(self.arena.int(n, Prim::Int)),
                        None => Err(FaultKind::Layout {
                            what: format!("`{}`", p.name()),
                        }),
                    },
                    CtType::Unit => Ok(self.arena.int(0, Prim::Int)),
                    other => Err(FaultKind::Layout {
                        what: format!("`{}`", other.render()),
                    }),
                }
            }
            Intrinsic::Implements => Err(FaultKind::EngineGap {
                construct: "`implements` outside its call form",
            }),
        }
    }

    fn type_arg(&self, args: &[ValId]) -> Option<CtType> {
        match self.arena.kind(*args.first()?) {
            ValueKind::Type(t) => Some(t.clone()),
            _ => None,
        }
    }

    /// `implements(T, Trait)`: answered by the s14 satisfaction engine
    /// over a scratch table — trial unification, snapshot-safe, no
    /// impl selected, nothing recorded.
    pub(super) fn eval_implements(
        &mut self,
        args: &[ValId],
        tr: &traits::TraitRef,
        _span: Span,
    ) -> Result<ValId, FaultKind> {
        let Some(ct) = self.type_arg(args) else {
            return Err(FaultKind::NotComptime {
                what: "`implements` needs a type value".to_string(),
            });
        };
        let mut table = self.sigs.table.clone();
        let ty = match ct {
            CtType::Prim(p) => table.prim(p),
            CtType::Unit => table.unit(),
            CtType::Nominal { module, name } => table.intern(TyKind::Nominal { module, name }),
            // Synthesized/opaque types have empty impl sets today.
            _ => {
                let v = self.arena.bool_(false);
                return Ok(v);
            }
        };
        let mut vars = crate::unify::VarStore::new();
        let bounds = traits::Bounds::new();
        let yes = traits::satisfies(
            self.sigs, &mut table, &mut vars, &bounds, ty, tr.module, &tr.name, 8,
        );
        Ok(self.arena.bool_(yes))
    }

    /// Build the `TypeInfo` semantic value for a type.
    pub(super) fn build_typeinfo(&mut self, ct: &CtType) -> ValId {
        let (kind, name) = match ct {
            CtType::Prim(p) => ("prim", p.name().to_string()),
            CtType::Unit => ("unit", "()".to_string()),
            CtType::Tuple(_) => ("tuple", ct.render()),
            CtType::Nominal { module, name } => {
                let k = match self.sigs.get(*module as usize, name) {
                    Some(ItemSig::Struct(_)) => "struct",
                    Some(ItemSig::Enum { .. }) => "enum",
                    _ => "opaque",
                };
                (k, name.clone())
            }
            CtType::Synth { .. } => ("struct", ct.render()),
            CtType::TypeTy => ("type", "type".to_string()),
            CtType::Opaque(s) => ("opaque", s.clone()),
        };
        let fields = self.field_list(ct);
        let variants = self.variant_list(ct);
        let traits = self.trait_list(ct);
        let kind_v = self.arena.str_(kind);
        let name_v = self.arena.str_(name);
        let fields_v = self.arena.intern(ValueKind::Slice(fields));
        let variants_v = self.arena.intern(ValueKind::Slice(variants));
        let traits_v = self.arena.intern(ValueKind::Slice(traits));
        self.arena.intern(ValueKind::Struct {
            module: META_MODULE,
            name: "TypeInfo".to_string(),
            fields: vec![
                ("kind".to_string(), kind_v),
                ("name".to_string(), name_v),
                ("fields".to_string(), fields_v),
                ("variants".to_string(), variants_v),
                ("traits".to_string(), traits_v),
            ],
        })
    }

    fn field_value(&mut self, name: &str, ct: CtType) -> ValId {
        let n = self.arena.str_(name);
        let t = self.arena.ty(ct);
        self.arena.intern(ValueKind::Struct {
            module: META_MODULE,
            name: "Field".to_string(),
            fields: vec![("name".to_string(), n), ("ty".to_string(), t)],
        })
    }

    fn field_list(&mut self, ct: &CtType) -> Vec<ValId> {
        match ct {
            CtType::Nominal { module, name } => {
                let Some(ItemSig::Struct(s)) = self.sigs.get(*module as usize, name) else {
                    return Vec::new();
                };
                // Declaration order — never hash order (determinism).
                let fields: Vec<(String, CtType)> = s
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), ty_to_ct(&self.sigs.table, f.ty)))
                    .collect();
                fields
                    .into_iter()
                    .map(|(n, t)| self.field_value(&n, t))
                    .collect()
            }
            CtType::Synth { fields } => {
                let fields = fields.clone();
                fields
                    .into_iter()
                    .map(|(n, t)| self.field_value(&n, t))
                    .collect()
            }
            CtType::Tuple(ts) => {
                let ts = ts.clone();
                ts.into_iter()
                    .enumerate()
                    .map(|(i, t)| self.field_value(&i.to_string(), t))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    fn variant_list(&mut self, ct: &CtType) -> Vec<ValId> {
        let CtType::Nominal { module, name } = ct else {
            return Vec::new();
        };
        let m = *module as usize;
        if !matches!(self.sigs.get(m, name), Some(ItemSig::Enum { .. })) {
            return Vec::new();
        }
        let Some(item) = self.pkg.tables[m].get(name) else {
            return Vec::new();
        };
        let (file, decl) = (item.file, item.decl);
        let Some(node) = self.pkg.files[file]
            .parse
            .root
            .nodes()
            .filter(|n| n.kind.is_item())
            .nth(decl)
        else {
            return Vec::new();
        };
        let Some(d) = EnumDecl::cast(node) else {
            return Vec::new();
        };
        let src = &self.pkg.files[file].raw.src;
        let names: Vec<String> = d
            .variants()
            .filter_map(|v| v.name())
            .map(|t| {
                String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize]).into_owned()
            })
            .collect();
        names.into_iter().map(|n| self.arena.str_(n)).collect()
    }

    /// The implemented-trait names of a type, in sorted order (never
    /// impl-declaration order — that would leak file iteration).
    fn trait_list(&mut self, ct: &CtType) -> Vec<ValId> {
        let mut names: Vec<String> = Vec::new();
        for imp in &self.sigs.impls {
            let Some(tr) = &imp.trait_ref else { continue };
            let self_kind = self.sigs.table.kind(imp.self_ty);
            let hit = match (ct, self_kind) {
                (CtType::Prim(p), TyKind::Prim(q)) => p == q,
                (CtType::Nominal { module, name }, TyKind::Nominal { module: m, name: n }) => {
                    *module == *m && name == n
                }
                _ => false,
            };
            if hit && !names.contains(&tr.name) {
                names.push(tr.name.clone());
            }
        }
        names.sort();
        names.into_iter().map(|n| self.arena.str_(n)).collect()
    }

    /// `typebuild(desc)`: a slice of `(name, type)` pairs → a
    /// synthesized struct type value. Semantic in, semantic out.
    fn build_type(&mut self, desc: ValId) -> Result<ValId, FaultKind> {
        let ValueKind::Slice(items) = self.arena.kind(desc).clone() else {
            return Err(FaultKind::NotComptime {
                what: "`typebuild` needs a slice of `(name, type)` pairs".to_string(),
            });
        };
        let mut fields = Vec::with_capacity(items.len());
        for it in items {
            let ValueKind::Tuple(pair) = self.arena.kind(it).clone() else {
                return Err(FaultKind::NotComptime {
                    what: "`typebuild` needs `(name, type)` pairs".to_string(),
                });
            };
            let (Some(&n), Some(&t)) = (pair.first(), pair.get(1)) else {
                return Err(FaultKind::NotComptime {
                    what: "`typebuild` needs two-element pairs".to_string(),
                });
            };
            let (ValueKind::Str(name), ValueKind::Type(ty)) =
                (self.arena.kind(n).clone(), self.arena.kind(t).clone())
            else {
                return Err(FaultKind::NotComptime {
                    what: "`typebuild` pairs are `(str, type)`".to_string(),
                });
            };
            fields.push((name, ty));
        }
        Ok(self.arena.ty(CtType::Synth { fields }))
    }
}

/// The size in bytes of a primitive whose layout is already fixed by
/// spec 02 (str is not — its representation is c05's).
fn prim_size(p: Prim) -> Option<i128> {
    Some(match p {
        Prim::Bool | Prim::Byte | Prim::I8 | Prim::U8 => 1,
        Prim::I16 | Prim::U16 => 2,
        Prim::I32 | Prim::U32 | Prim::F32 => 4,
        Prim::I64 | Prim::U64 | Prim::Int | Prim::Uint | Prim::F64 => 8,
        Prim::Str => return None,
    })
}
