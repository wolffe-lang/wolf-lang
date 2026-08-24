//! Item signature elaboration (s13, Target 1).
//!
//! Every item declares its full signature (D27 — no inference across
//! item boundaries, ever; SE-0244's "a mistake"). This pass elaborates
//! each module item's signature against resolved names into the
//! interned [`TypeTable`], producing the [`SigTables`] that body
//! checking consumes: the elaborated signature set is exactly the s12
//! interface content, so the module interface stays the typing
//! firewall — checking module M's bodies needs only M's bodies plus
//! dependency *signatures*, never dependency bodies.
//!
//! Missing annotations on items are parse-adjacent diagnostics (E0407)
//! with insertion fix-its: the compiler may know the type, but makes
//! you write it.
//!
//! Type forms beyond s13's checkable universe (generic instantiations,
//! trait names in type position) elaborate as *opaque* tokens
//! ([`TyKind::Unsupported`]): values of such types pass around by
//! equality, and every operation on them is NotYetCheckable — the
//! checker never guesses.

use std::collections::BTreeMap;

use wolf_ast::{
    ConstDecl, EnumDecl, FnDecl, GreenNode, PathType, StructDecl, SyntaxKind, TypeDecl,
    is_type_kind,
};
use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use crate::graph::{BindTarget, Binding, ItemKind, Package};
use crate::types::{Prim, TyId, TyKind, TypeTable};

/// One function parameter's elaborated signature.
#[derive(Debug, Clone)]
pub struct ParamSig {
    pub name: String,
    pub ty: TyId,
    /// The parameter's span (the "declared here" secondary for E0401
    /// argument mismatches and E0402).
    pub span: Span,
    /// The declared passing mode (X1): `None` is the default `read`.
    /// On a `self` receiver this is the mode the call site must spell
    /// — `(mut p).norm()` / `(take p).consume()` (E0804, s17); the
    /// exclusivity/move *checking* behind the modes is c04's
    /// (`wolf_mem`, s18).
    pub mode: Option<wolf_ast::ParamMode>,
    /// The `self.{a, b}` view set on a `mut self` receiver (s18,
    /// [mem.tier0.excl.3]): the exact field set the method may touch,
    /// in declaration order. `None` on ordinary parameters and on
    /// unannotated receivers (the full view). Part of the signature
    /// surface — `wolf interface` renders it and the interface hash
    /// covers it (via the rendered param list).
    pub view: Option<Vec<String>>,
}

/// A resolved trait bound on a generic parameter (`T: Show` — module
/// + name identify the trait globally).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundRef {
    pub module: usize,
    pub name: String,
    /// The bound path's span (provenance for E0501/E0502).
    pub span: Span,
}

/// One generic parameter with its resolved bounds (s14: the archetype
/// a body checks against — the only known facts about the parameter).
#[derive(Debug, Clone)]
pub struct GenericSig {
    pub name: String,
    /// The parameter's declaration span (where an add-this-bound
    /// suggestion inserts).
    pub span: Span,
    pub bounds: Vec<BoundRef>,
}

/// One function item's elaborated signature.
#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<ParamSig>,
    /// The declared return type; `()` when no `->` is written. `!T`
    /// elaborates as an opaque-row error union (D30; rows are s15).
    pub ret: TyId,
    pub name_span: Span,
    /// The `-> …` node's span (the "because" locus of return-type
    /// provenance), if written.
    pub ret_span: Option<Span>,
    /// The explicit `! {row}` node's span, if written (E0602 fix-its
    /// insert new tags just before its closing `}`).
    pub row_span: Option<Span>,
    /// Generic parameters, in order, with resolved bounds. Non-empty
    /// ⇒ calls instantiate (s14: fresh existentials per parameter,
    /// argument-driven, bounds checked at the call site only).
    pub generics: Vec<GenericSig>,
    /// `comptime fn` (D29, s16): calls with comptime-known arguments
    /// evaluate during checking; the body itself checks like any
    /// other body.
    pub comptime: bool,
    /// s22 — `#[trusted]` (D11 ring 2): this function's unsafe blocks
    /// assert invariants the checker cannot see. `Some(obligation)`
    /// carries the declared contract text (`#[trusted("…")]`; empty
    /// for the bare attribute). Trusted functions roster per-module in
    /// the `wolfi` interface (both hash partitions) and must be
    /// declared in the package manifest (E1303, `wolf audit`'s
    /// supply-chain surface).
    pub trusted: Option<String>,
}

impl FnSig {
    /// The generic parameter names (rigid names in scope for
    /// lowering).
    pub fn generic_names(&self) -> Vec<String> {
        self.generics.iter().map(|g| g.name.clone()).collect()
    }
}

/// One struct field's elaborated signature.
#[derive(Debug, Clone)]
pub struct FieldSig {
    pub name: String,
    pub ty: TyId,
    pub span: Span,
}

/// A struct item's elaborated signature.
#[derive(Debug, Clone)]
pub struct StructSig {
    pub generic: bool,
    /// Generic parameter names, in declaration order (s94). The
    /// rigid names the field types were elaborated under; an applied
    /// `Nominal { args }` substitutes positionally against these.
    /// (Bounds on data-type parameters are not elaborated anywhere
    /// yet; when they are, this becomes `Vec<GenericSig>`.)
    pub generics: Vec<String>,
    pub fields: Vec<FieldSig>,
    pub name_span: Span,
}

/// A `const`/module-level `let`/`var` item's elaborated signature.
#[derive(Debug, Clone)]
pub struct GlobalSig {
    /// `None` when the annotation is missing (reported as E0407; uses
    /// of the item are then NotYetCheckable, never guessed).
    pub ty: Option<TyId>,
    pub name_span: Span,
    /// For an item-level `let`: the `let` keyword's span — assignment
    /// to the global is E0410, and the fix-it rewrites this token to
    /// `var`. `None` for `var` and `const` globals.
    pub let_kw: Option<Span>,
}

/// One enum variant's elaborated signature (s17: variant construction
/// `Color.Red` / `Color.Rgb(…)` and the exhaustiveness ctor universe).
#[derive(Debug, Clone)]
pub struct VariantSig {
    pub name: String,
    /// Payload types, in order (empty for payload-less variants).
    pub payload: Vec<TyId>,
    pub span: Span,
}

/// One module item's elaborated signature.
#[derive(Debug, Clone)]
pub enum ItemSig {
    Fn(FnSig),
    Struct(StructSig),
    /// An enum with its variant table (s17): construction and match
    /// exhaustiveness both read `variants`.
    Enum {
        generic: bool,
        /// As [`StructSig::generics`] (s94).
        generics: Vec<String>,
        name_span: Span,
        variants: Vec<VariantSig>,
    },
    /// A type alias; non-generic aliases expand during lowering.
    Alias {
        generic: bool,
    },
    /// `type X = distinct T` — an **adapter type** (D28's sanctioned
    /// orphan escape): a nominal type of its own, with its OWN empty
    /// impl set (nothing inherited), whose recorded `base` is the
    /// layout-identity fact for c05 — `as` casts between the adapter
    /// and its base are free and bidirectional (no-ops at lowering).
    Distinct {
        base: TyId,
        name_span: Span,
    },
    Trait,
    Global(GlobalSig),
}

/// The elaborated signature set of a whole package: the typing
/// firewall. Body checking borrows this immutably (Target 5).
#[derive(Debug)]
pub struct SigTables {
    pub table: TypeTable,
    /// Per module (parallel to `Package::modules`): name → signature.
    pub modules: Vec<BTreeMap<String, ItemSig>>,
    /// Dotted module names for rendering.
    pub module_names: Vec<String>,
    /// Every trait declaration, by global identity (s14).
    pub traits: BTreeMap<crate::traits::TraitRef, crate::traits::TraitDef>,
    /// Every impl block, coherence-checked (s14).
    pub impls: Vec<crate::traits::ImplDef>,
    /// Signature-elaboration + trait/coherence + row-sealing
    /// diagnostics (E0407, E05xx, E06xx), deterministic order.
    pub diagnostics: Vec<Diagnostic>,
    /// Sealed inferred rows (s15): `(module, fn name, rendered sealed
    /// signature)` — module-private facts, shown by `wolf interface`
    /// but never serialized or hashed (private items are not
    /// interface surface).
    pub sealed: Vec<(usize, String, String)>,
}

impl SigTables {
    pub fn get(&self, module: usize, name: &str) -> Option<&ItemSig> {
        self.modules.get(module)?.get(name)
    }
}

/// Elaborate every module item's signature (dependency-first order so
/// the process is deterministic; lowering itself is order-independent
/// because aliases expand on demand).
pub fn build_sigs(pkg: &Package) -> SigTables {
    let mut lower = Lower {
        pkg,
        table: TypeTable::new(),
        alias_stack: Vec::new(),
        diags: Vec::new(),
    };
    let mut modules: Vec<BTreeMap<String, ItemSig>> = vec![BTreeMap::new(); pkg.modules.len()];
    for &m in &pkg.topo {
        let mut sigs = BTreeMap::new();
        for item in &pkg.tables[m].items {
            let sig = lower.item_sig(m, item);
            sigs.insert(item.name.clone(), sig);
        }
        modules[m] = sigs;
    }
    // The s14 layer: trait declarations, impls, coherence, dyn use
    // checks — all elaborating into the same base table.
    let (traits, impls) = crate::traits::build(&mut lower);
    let diagnostics = lower.diags;
    let mut sigs = SigTables {
        table: lower.table,
        modules,
        module_names: pkg.modules.iter().map(|md| md.dotted()).collect(),
        traits,
        impls,
        diagnostics,
        sealed: Vec::new(),
    };
    // The s15 layer: seal every inferred row to its concrete tag set
    // (cycle-aware fixpoint over each module's call graph) and reject
    // inferred rows on exported items (E0605).
    crate::rows::seal(pkg, &mut sigs);
    // The entry-signature rule (#106): a root-module `main` is the
    // program's entry, and its shape is a property of the declaration
    // — ruled here, not at some backend's shim.
    check_entry_sig(&mut sigs);
    wolf_diag::sort_diagnostics(&mut sigs.diagnostics);
    sigs
}

/// E0414 (#106) — what `main` may look like. The root module's `main`
/// is the entry the process calls, so its signature is a whole-program
/// contract: no parameters (the process has none to hand it), no
/// generics (nothing instantiates the entry), and a return the process
/// can be handed an exit status from — `()`, `int`, or an error union
/// over either (`!int`, `!()`, `int ! {…}`: the ok value exits, an
/// error value is D30's `error: <tag>` + exit 1). Everything else used
/// to run on the checked rung with the value silently dropped and
/// refuse at the native rung's C-entry shim — a lane divergence AND a
/// codegen-phase answer to a declaration-shape question. Exit-code
/// numerics stay #32's open spec question; this rules only the shapes.
/// Runs after row sealing so `-> !T` inferred rows are concrete.
fn check_entry_sig(sigs: &mut SigTables) {
    let Some(ItemSig::Fn(f)) = sigs.get(0, "main") else {
        return;
    };
    let ret_ok = {
        let ok_ty = match *sigs.table.kind(f.ret) {
            TyKind::ErrUnion(ok, _) => ok,
            _ => f.ret,
        };
        // `Error` unifies with everything (D22): the wreck already
        // has its own diagnostic, never a cascade off it.
        matches!(
            sigs.table.kind(ok_ty),
            TyKind::Unit | TyKind::Prim(Prim::Int) | TyKind::Error
        )
    };
    let (span, what, label): (Span, String, &str) = if !f.params.is_empty() {
        let n = f.params.len();
        (
            f.name_span,
            format!(
                "`main` is the entry the process calls, and the process hands it no \
                 arguments — this one wants {n}"
            ),
            "declared with parameters here",
        )
    } else if !f.generics.is_empty() {
        (
            f.name_span,
            "`main` is the entry the process calls, so nothing exists to choose its \
             type arguments — it cannot be generic"
                .to_string(),
            "declared generic here",
        )
    } else if !ret_ok {
        let no_vars = |_: u32| -> Result<TyId, &'static str> { Err("_") };
        let found = crate::types::render(&sigs.table, f.ret, &no_vars);
        (
            f.ret_span.unwrap_or(f.name_span),
            format!(
                "`main` returns `{found}`, which is not a type the process can be \
                 handed"
            ),
            "the process can take an exit status from `()`, `int`, or an error \
             union over them",
        )
    } else {
        return;
    };
    sigs.diagnostics.push(
        Diagnostic::error(codes::E0414, span, what)
            .with_label(label)
            .with_note(
                "the entry's legal shapes: `fn main()`, `fn main() -> int`, \
                 `fn main() -> !int` (and `!()` / an explicit error row over \
                 `int` or `()`). An ok value becomes the exit status; an error \
                 value prints `error: <tag>` and exits 1 (D30).",
            ),
    );
}

/// The module-namespace bindings of `file` within `module`, as
/// (bound name, package-module index) — the subset a consumer outside
/// sema (the lowering's qualified fn-value read, #116) can act on.
/// Item/std/c bindings are deliberately absent: their uses type
/// through sema's own paths and never reach the namespace fallback.
pub fn module_bindings(pkg: &Package, module: usize, file: usize) -> Vec<(String, usize)> {
    bindings_for(pkg, module, file)
        .iter()
        .filter_map(|b| match b.target {
            crate::graph::BindTarget::PkgModule(m) => Some((b.name.clone(), m)),
            _ => None,
        })
        .collect()
}

/// The import bindings of `file` within `module`.
pub(crate) fn bindings_for(pkg: &Package, module: usize, file: usize) -> &[Binding] {
    let md = &pkg.modules[module];
    let slot = md
        .files
        .iter()
        .position(|&f| f == file)
        .expect("file belongs to module");
    &md.bindings[slot]
}

/// The AST node of a module item (by its recorded decl ordinal).
pub(crate) fn item_node<'a>(pkg: &'a Package, item: &crate::graph::Item) -> &'a GreenNode {
    pkg.files[item.file]
        .parse
        .root
        .nodes()
        .filter(|n| n.kind.is_item())
        .nth(item.decl)
        .expect("item decl index valid")
}

/// Signature-and-type lowering context, shared by [`build_sigs`] and
/// the body checker (`let` annotations, casts, closure params).
pub(crate) struct Lower<'a> {
    pub pkg: &'a Package,
    pub table: TypeTable,
    /// Alias-expansion cycle guard: (module, item name).
    alias_stack: Vec<(usize, String)>,
    pub(crate) diags: Vec<Diagnostic>,
}

impl<'a> Lower<'a> {
    pub fn new(pkg: &'a Package, table: TypeTable) -> Lower<'a> {
        Lower {
            pkg,
            table,
            alias_stack: Vec::new(),
            diags: Vec::new(),
        }
    }

    fn text(&self, file: usize, span: Span) -> String {
        let src = &self.pkg.files[file].raw.src;
        String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn item_sig(&mut self, module: usize, item: &crate::graph::Item) -> ItemSig {
        let node = item_node(self.pkg, item);
        let file = item.file;
        match node.kind {
            SyntaxKind::FnDecl => ItemSig::Fn(self.fn_sig_core(
                module,
                file,
                node,
                item.name_span,
                &[],
                Some(&item.name),
            )),
            SyntaxKind::StructDecl => {
                let d = StructDecl::cast(node).expect("kind");
                let generics = generic_names(self, file, node);
                let fields = d
                    .fields()
                    .map(|f| FieldSig {
                        name: f
                            .name()
                            .map(|t| self.text(file, t.span))
                            .unwrap_or_default(),
                        ty: f
                            .ty()
                            .map(|t| self.lower_type(module, file, &generics, t))
                            .unwrap_or_else(|| self.table.error()),
                        span: f.name().map(|t| t.span).unwrap_or(node.span),
                    })
                    .collect();
                ItemSig::Struct(StructSig {
                    generic: !generics.is_empty(),
                    generics,
                    fields,
                    name_span: item.name_span,
                })
            }
            SyntaxKind::EnumDecl => {
                let generics = generic_names(self, file, node);
                let variants = EnumDecl::cast(node)
                    .map(|d| self.variant_sigs(module, file, &generics, d.variants()))
                    .unwrap_or_default();
                ItemSig::Enum {
                    generic: !generics.is_empty(),
                    generics,
                    name_span: item.name_span,
                    variants,
                }
            }
            SyntaxKind::TypeDecl => {
                let d = TypeDecl::cast(node).expect("kind");
                let generic = d.generics().is_some_and(|g| g.params().next().is_some());
                // `type X = distinct T` — the adapter form (D28): a
                // nominal type of its own with a recorded base (the
                // layout-identity fact for c05).
                if !generic
                    && let Some(def) = d.def()
                    && is_distinct_def(def)
                {
                    let base = self.lower_inner(module, file, &[], def);
                    return ItemSig::Distinct {
                        base,
                        name_span: item.name_span,
                    };
                }
                match d.def().map(|def| def.kind) {
                    Some(SyntaxKind::StructDef) => {
                        let generics = generic_names(self, file, node);
                        let fields = d
                            .def()
                            .into_iter()
                            .flat_map(|def| def.nodes().filter_map(wolf_ast::StructField::cast))
                            .map(|f| FieldSig {
                                name: f
                                    .name()
                                    .map(|t| self.text(file, t.span))
                                    .unwrap_or_default(),
                                ty: f
                                    .ty()
                                    .map(|t| self.lower_type(module, file, &generics, t))
                                    .unwrap_or_else(|| self.table.error()),
                                span: f.name().map(|t| t.span).unwrap_or(node.span),
                            })
                            .collect();
                        ItemSig::Struct(StructSig {
                            generic,
                            generics,
                            fields,
                            name_span: item.name_span,
                        })
                    }
                    Some(SyntaxKind::EnumDef) => {
                        let generics = generic_names(self, file, node);
                        let variants = d
                            .def()
                            .map(|def| {
                                self.variant_sigs(
                                    module,
                                    file,
                                    &generics,
                                    def.nodes().filter_map(wolf_ast::EnumVariant::cast),
                                )
                            })
                            .unwrap_or_default();
                        ItemSig::Enum {
                            generic,
                            generics,
                            name_span: item.name_span,
                            variants,
                        }
                    }
                    _ => ItemSig::Alias { generic },
                }
            }
            SyntaxKind::TraitDecl => ItemSig::Trait,
            SyntaxKind::ConstDecl => {
                let d = ConstDecl::cast(node).expect("kind");
                let ty = d.ty().map(|t| self.lower_type(module, file, &[], t));
                if ty.is_none() {
                    self.missing_annotation(item, node, ItemKind::Const);
                }
                ItemSig::Global(GlobalSig {
                    ty,
                    name_span: item.name_span,
                    let_kw: None,
                })
            }
            SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
                let ty = node
                    .nodes()
                    .find(|n| is_type_kind(n.kind))
                    .map(|t| self.lower_type(module, file, &[], t));
                if ty.is_none() {
                    self.missing_annotation(item, node, item.kind);
                }
                let let_kw = (node.kind == SyntaxKind::LetDecl)
                    .then(|| node.tokens().find(|t| t.kind == SyntaxKind::LetKw))
                    .flatten()
                    .map(|t| t.span);
                ItemSig::Global(GlobalSig {
                    ty,
                    name_span: item.name_span,
                    let_kw,
                })
            }
            _ => ItemSig::Trait,
        }
    }

    /// E0407: an item without its full signature (D27 — "the compiler
    /// knows but makes you write it").
    fn missing_annotation(&mut self, item: &crate::graph::Item, node: &GreenNode, kind: ItemKind) {
        // Fix-it: insert `: <type>` right after the declared name.
        let insert_at = Span::new(item.name_span.file, item.name_span.hi, item.name_span.hi);
        let hint = literal_type_hint(node);
        let mut d = Diagnostic::error(
            codes::E0407,
            item.name_span,
            format!(
                "the {} `{}` is an item, so it must declare its type",
                kind.keyword(),
                item.name
            ),
        )
        .with_label("no `: type` annotation")
        .with_note(
            "items are the typing firewall (D27): types are inferred inside \
             function bodies, never across item boundaries.",
        );
        match hint {
            Some(t) => {
                d = d.with_suggestion(Suggestion::new(
                    format!("the initializer makes it `{t}` — write that down"),
                    vec![(insert_at, format!(": {t}"))],
                    Applicability::Maybe,
                ));
            }
            None => {
                d = d.with_suggestion(Suggestion::new(
                    "annotate the declared type",
                    vec![(insert_at, ": <type>".to_string())],
                    Applicability::HasPlaceholders,
                ));
            }
        }
        self.diags.push(d);
    }

    /// Elaborate a fn signature with `outer` rigid names already in scope
    /// (an impl's generics + `Self` when elaborating members). Member
    /// signatures never carry inferred-row markers: a trait/impl
    /// member's bare `-> !T` is the empty row (conformance compares
    /// these types structurally).
    pub(crate) fn fn_sig_in(
        &mut self,
        module: usize,
        file: usize,
        node: &GreenNode,
        name_span: Span,
        outer: &[String],
    ) -> FnSig {
        self.fn_sig_core(module, file, node, name_span, outer, None)
    }

    /// The shared elaboration. `owner` is `Some(item name)` for
    /// module-level fn items — the only position where a bare `-> !T`
    /// is an *inferred* row (marked here, sealed by [`crate::rows`]).
    pub(crate) fn fn_sig_core(
        &mut self,
        module: usize,
        file: usize,
        node: &GreenNode,
        name_span: Span,
        outer: &[String],
        owner: Option<&str>,
    ) -> FnSig {
        let d = FnDecl::cast(node).expect("kind");
        let own = self.generic_defs(module, file, node);
        let mut generics: Vec<String> = outer.to_vec();
        generics.extend(own.iter().map(|g| g.name.clone()));
        let mut params = Vec::new();
        if let Some(list) = d.params() {
            for p in list.params() {
                if p.is_self() {
                    // Inside a trait/impl (where `Self` is in scope),
                    // a receiver elaborates as a leading `Self`-typed
                    // parameter — qualified calls pass it explicitly
                    // (receiver *syntax* is s17). Free items never
                    // take `self`.
                    if generics.iter().any(|g| g == "Self") {
                        let ty = self.table.intern(TyKind::Rigid("Self".to_string()));
                        let view = p.view_set().map(|vs| {
                            vs.fields()
                                .map(|t| self.text(file, t.span))
                                .collect::<Vec<_>>()
                        });
                        params.push(ParamSig {
                            name: "self".to_string(),
                            ty,
                            span: p.syntax().span,
                            mode: p.mode(),
                            view,
                        });
                    }
                    continue;
                }
                let pname = p
                    .name()
                    .map(|t| self.text(file, t.span))
                    .unwrap_or_default();
                let span = p.name().map(|t| t.span).unwrap_or(p.syntax().span);
                let ty = match p.ty() {
                    Some(t) => self.lower_type(module, file, &generics, t),
                    None => {
                        self.param_missing_annotation(&pname, span);
                        self.table.error()
                    }
                };
                params.push(ParamSig {
                    name: pname,
                    ty,
                    span,
                    mode: p.mode(),
                    view: None,
                });
            }
        }
        let (ret, ret_span, row_span) = match d.ret_ty() {
            Some(r) => {
                let base = r
                    .ty()
                    .map(|t| self.lower_type(module, file, &generics, t))
                    .unwrap_or_else(|| self.table.error());
                match r.error_row() {
                    // Explicit `T ! {row}` — the stated row (D30/s15).
                    Some(row) => {
                        let row_ty = self.lower_row(module, file, &generics, row);
                        let ty = self.table.err_union(base, row_ty);
                        (ty, Some(r.syntax().span), Some(row.syntax().span))
                    }
                    None => {
                        // Bare `-> !T` on a module fn item is an
                        // *inferred* row: mark it for sealing (s15,
                        // the Zig-trap fix). Everywhere else the bare
                        // form is the empty row.
                        let ty = match (owner, self.table.kind(base).clone()) {
                            (Some(name), TyKind::ErrUnion(inner, _)) => {
                                let marker = self.table.intern(TyKind::InferredRow {
                                    module: module as u32,
                                    name: name.to_string(),
                                });
                                self.table.err_union(inner, marker)
                            }
                            _ => base,
                        };
                        (ty, Some(r.syntax().span), None)
                    }
                }
            }
            None => (self.table.unit(), None, None),
        };
        FnSig {
            params,
            ret,
            name_span,
            ret_span,
            row_span,
            generics: own,
            comptime: d.is_comptime(),
            trusted: trusted_attr(node, |sp| self.text(file, sp)),
        }
    }

    /// Elaborate an enum's variant table (s17): names + payload types.
    fn variant_sigs<'n>(
        &mut self,
        module: usize,
        file: usize,
        generics: &[String],
        variants: impl Iterator<Item = wolf_ast::EnumVariant<'n>>,
    ) -> Vec<VariantSig> {
        variants
            .map(|v| {
                let name = v
                    .name()
                    .map(|t| self.text(file, t.span))
                    .unwrap_or_default();
                let span = v.name().map(|t| t.span).unwrap_or(v.syntax().span);
                let payload: Vec<TyId> = v
                    .payload()
                    .map(|t| self.lower_type(module, file, generics, t))
                    .collect();
                VariantSig {
                    name,
                    payload,
                    span,
                }
            })
            .collect()
    }

    /// Elaborate an explicit `! {…}` row (D30/s15). Tags are a *set*:
    /// duplicate labels are E0601. A payload-less entry naming a
    /// generic parameter in scope is the row's polymorphic tail (the
    /// HOF `rethrows` answer); a second tail is E0601. A row ending in
    /// `..` is *open* (s17): it declares at least the listed tags,
    /// admits raises of new ones, and forces rest-arm matching
    /// downstream ([`TyKind::OpenTail`]).
    pub(crate) fn lower_row(
        &mut self,
        module: usize,
        file: usize,
        generics: &[String],
        row: wolf_ast::ErrorRow<'_>,
    ) -> TyId {
        let mut tags: Vec<(String, Vec<TyId>)> = Vec::new();
        let mut tail: Option<TyId> = if row.is_open() {
            Some(self.table.intern(TyKind::OpenTail))
        } else {
            None
        };
        for entry in row.entries() {
            let Some(path) = entry.path() else { continue };
            let segs: Vec<String> = path.segments().map(|t| self.text(file, t.span)).collect();
            let name = segs.join(".");
            let span = path.syntax().span;
            let payload: Vec<TyId> = entry
                .payload()
                .map(|t| self.lower_type(module, file, generics, t))
                .collect();
            // A payload-less single segment naming a generic in scope
            // is a row variable, not a tag.
            if payload.is_empty() && segs.len() == 1 && generics.contains(&name) {
                if tail.is_some() {
                    self.diags.push(
                        Diagnostic::error(
                            codes::E0601,
                            span,
                            format!("this row already has a row variable, so `{name}` cannot be a second one"),
                        )
                        .with_label("second row variable")
                        .with_note(
                            "a row extends exactly one tail; merge the two type \
                             parameters, or spell one side's tags out.",
                        ),
                    );
                    continue;
                }
                tail = Some(self.table.intern(TyKind::Rigid(name)));
                continue;
            }
            if tags.iter().any(|(n, _)| *n == name) {
                self.diags.push(
                    Diagnostic::error(
                        codes::E0601,
                        span,
                        format!("the tag `{name}` appears twice in this error row"),
                    )
                    .with_label("second appearance")
                    .with_note(
                        "an error row is a set of tags, not a list — each tag \
                         appears once (its payload is part of that one entry); \
                         delete the duplicate.",
                    ),
                );
                continue;
            }
            tags.push((name, payload));
        }
        self.table.row(tags, tail)
    }

    /// Elaborate an item's generic parameter list into [`GenericSig`]s
    /// with resolved trait bounds. A bound naming anything but a
    /// trait — or a trait with its own input parameters, which the
    /// bound surface cannot apply yet — is E0503.
    pub(crate) fn generic_defs(
        &mut self,
        module: usize,
        file: usize,
        node: &GreenNode,
    ) -> Vec<GenericSig> {
        let Some(list) = node.nodes().find_map(wolf_ast::GenericParamList::cast) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for p in list.params() {
            let Some(name_tok) = p.name() else { continue };
            let name = self.text(file, name_tok.span);
            let mut bounds = Vec::new();
            if let Some(bound) = p.bound() {
                for path in bound.paths() {
                    let segs: Vec<(String, Span)> = path
                        .segments()
                        .map(|t| (self.text(file, t.span), t.span))
                        .collect();
                    let span = segs
                        .first()
                        .map(|(_, s)| segs.iter().fold(*s, |a, (_, b)| a.join(*b)))
                        .unwrap_or(name_tok.span);
                    match self.resolve_type_head(module, file, &segs) {
                        TypeHead::Item {
                            module: tm,
                            name: tn,
                        } => match self.trait_bound_check(tm, &tn) {
                            BoundCheck::Ok => bounds.push(BoundRef {
                                module: tm,
                                name: tn,
                                span,
                            }),
                            BoundCheck::NotATrait(kind) => {
                                self.diags.push(
                                    Diagnostic::error(
                                        codes::E0503,
                                        span,
                                        format!(
                                            "`{tn}` is a {kind}, and only traits can be bounds"
                                        ),
                                    )
                                    .with_label("not a trait")
                                    .with_note(
                                        "a bound promises capabilities, and only a `trait` \
                                             declares a capability set — check the name, or \
                                             write the trait you meant.",
                                    ),
                                );
                            }
                            BoundCheck::Parameterized => {
                                self.diags.push(
                                    Diagnostic::error(
                                        codes::E0503,
                                        span,
                                        format!(
                                            "the trait `{tn}` takes input parameters, which \
                                                 a bound cannot apply"
                                        ),
                                    )
                                    .with_label("this trait is parameterized")
                                    .with_note(
                                        "bounds are written `T: Trait` with no arguments; \
                                             applying trait inputs inside a bound has no surface \
                                             syntax yet — use a trait without input parameters.",
                                    ),
                                );
                            }
                        },
                        // Unresolved bound paths were already reported
                        // by name resolution (E0301); poisoned imports
                        // stay silent.
                        TypeHead::Unknown | TypeHead::Poisoned => {}
                    }
                }
            }
            out.push(GenericSig {
                name,
                span: name_tok.span,
                bounds,
            });
        }
        out
    }

    /// Is the item at (module, name) a bindable trait bound?
    fn trait_bound_check(&self, module: usize, name: &str) -> BoundCheck {
        let Some(item) = self.pkg.tables[module].get(name) else {
            return BoundCheck::NotATrait("missing item");
        };
        let node = item_node(self.pkg, item);
        if node.kind != SyntaxKind::TraitDecl {
            return BoundCheck::NotATrait(item.kind.keyword());
        }
        let parameterized = wolf_ast::TraitDecl::cast(node)
            .and_then(|d| d.generics())
            .is_some_and(|g| g.params().next().is_some());
        if parameterized {
            BoundCheck::Parameterized
        } else {
            BoundCheck::Ok
        }
    }

    fn param_missing_annotation(&mut self, name: &str, span: Span) {
        let insert_at = Span::new(span.file, span.hi, span.hi);
        self.diags.push(
            Diagnostic::error(
                codes::E0407,
                span,
                format!("the parameter `{name}` needs a type annotation"),
            )
            .with_label("no `: type` here")
            .with_note(
                "function signatures are the typing firewall (D27): parameters \
                 always state their types; only closure parameters may take \
                 theirs from context.",
            )
            .with_suggestion(Suggestion::new(
                "annotate the parameter",
                vec![(insert_at, ": <type>".to_string())],
                Applicability::HasPlaceholders,
            )),
        );
    }

    /// Lower an AST type node to an interned type. `generics` are the
    /// rigid names in scope.
    pub(crate) fn lower_type(
        &mut self,
        module: usize,
        file: usize,
        generics: &[String],
        node: &GreenNode,
    ) -> TyId {
        match node.kind {
            SyntaxKind::PathType => self.lower_path_type(module, file, generics, node),
            SyntaxKind::ErrorUnionType => {
                let inner = node
                    .nodes()
                    .find(|n| is_type_kind(n.kind))
                    .map(|t| self.lower_type(module, file, generics, t))
                    .unwrap_or_else(|| self.table.error());
                // Postfix `T ! {row}` (#3) spells its row right here.
                // Bare prefix `!T` outside a fn item's return position
                // has no body to infer a row from: it is the empty
                // row. Item returns replace this with the inferred-row
                // marker ([`Lower::fn_sig_core`]).
                let row = match node.nodes().find_map(wolf_ast::ErrorRow::cast) {
                    Some(r) => self.lower_row(module, file, generics, r),
                    None => self.table.empty_row(),
                };
                self.table.err_union(inner, row)
            }
            SyntaxKind::TupleType => {
                let elems: Vec<TyId> = node
                    .nodes()
                    .filter(|n| is_type_kind(n.kind))
                    .map(|t| self.lower_type(module, file, generics, t))
                    .collect();
                match elems.len() {
                    0 => self.table.unit(),
                    1 => elems[0], // `(T)` is grouping
                    _ => self.table.intern(TyKind::Tuple(elems)),
                }
            }
            SyntaxKind::FnType => {
                let params: Vec<TyId> = node
                    .nodes()
                    .filter(|n| is_type_kind(n.kind))
                    .map(|t| self.lower_type(module, file, generics, t))
                    .collect();
                let ret = match node.nodes().find_map(wolf_ast::RetType::cast) {
                    Some(r) => {
                        let base = r
                            .ty()
                            .map(|t| self.lower_type(module, file, generics, t))
                            .unwrap_or_else(|| self.table.error());
                        match r.error_row() {
                            Some(row) => {
                                let row_ty = self.lower_row(module, file, generics, row);
                                self.table.err_union(base, row_ty)
                            }
                            None => base,
                        }
                    }
                    None => self.table.unit(),
                };
                self.table.intern(TyKind::Fn(params, ret))
            }
            SyntaxKind::PtrType => {
                let inner = self.lower_inner(module, file, generics, node);
                self.table.intern(TyKind::Ptr(inner))
            }
            SyntaxKind::PrefixType => {
                let inner = self.lower_inner(module, file, generics, node);
                let kw = node.tokens().next().map(|t| t.kind);
                let kind = match kw {
                    Some(SyntaxKind::SharedKw) => TyKind::Shared(inner),
                    Some(SyntaxKind::HandleKw) => TyKind::Handle(inner),
                    Some(SyntaxKind::WeakKw) => TyKind::Weak(inner),
                    Some(SyntaxKind::DistinctKw) => TyKind::Distinct(inner),
                    _ => TyKind::Error,
                };
                self.table.intern(kind)
            }
            SyntaxKind::DynType => {
                let Some(pnode) = node.child_node(SyntaxKind::Path) else {
                    return self.table.error();
                };
                let segs: Vec<(String, Span)> = pnode
                    .tokens()
                    .filter(|t| t.kind == SyntaxKind::Ident)
                    .map(|t| (self.text(file, t.span), t.span))
                    .collect();
                if segs.is_empty() {
                    return self.table.error();
                }
                match self.resolve_type_head(module, file, &segs) {
                    TypeHead::Item { module: tm, name }
                        if self.pkg.tables[tm].get(&name).is_some_and(|i| {
                            item_node(self.pkg, i).kind == SyntaxKind::TraitDecl
                        }) =>
                    {
                        self.table.intern(TyKind::Dyn {
                            module: tm as u32,
                            name,
                        })
                    }
                    // Non-traits and unresolved paths: the dyn-safety
                    // pass / resolution already own the report.
                    _ => self.opaque(file, node),
                }
            }
            SyntaxKind::RegionType => self.table.intern(TyKind::RegionTy),
            SyntaxKind::TypeType => self.table.intern(TyKind::TypeTy),
            _ => self.table.error(),
        }
    }

    pub(crate) fn lower_inner(
        &mut self,
        module: usize,
        file: usize,
        generics: &[String],
        node: &GreenNode,
    ) -> TyId {
        node.nodes()
            .find(|n| is_type_kind(n.kind))
            .map(|t| self.lower_type(module, file, generics, t))
            .unwrap_or_else(|| self.table.error())
    }

    fn lower_path_type(
        &mut self,
        module: usize,
        file: usize,
        generics: &[String],
        node: &GreenNode,
    ) -> TyId {
        let d = PathType::cast(node).expect("kind");
        let segs: Vec<(String, Span)> = d
            .path()
            .map(|p| {
                p.segments()
                    .map(|t| (self.text(file, t.span), t.span))
                    .collect()
            })
            .unwrap_or_default();
        let Some((first, _)) = segs.first().cloned() else {
            return self.table.error();
        };
        let has_args = d.args().is_some_and(|a| a.args().next().is_some());

        // `wrapping[uN]` — the one builtin type constructor (X3).
        if first == "wrapping" && segs.len() == 1 {
            if let Some(args) = d.args() {
                let arg_tys: Vec<TyId> = args
                    .args()
                    .filter(|a| is_type_kind(a.kind))
                    .map(|a| self.lower_type(module, file, generics, a))
                    .collect();
                if arg_tys.len() == 1
                    && matches!(self.table.kind(arg_tys[0]), TyKind::Prim(p) if p.is_integer())
                {
                    return self.table.intern(TyKind::Wrapping(arg_tys[0]));
                }
            }
            return self.opaque(file, node);
        }

        if generics.contains(&first) {
            if has_args {
                // The no-HKP ceiling (D28): a parameter names one
                // complete type, never a constructor.
                self.diags.push(
                    Diagnostic::error(
                        codes::E0511,
                        node.span,
                        format!(
                            "`{first}` is a generic parameter, so it cannot take type arguments"
                        ),
                    )
                    .with_label("applied here")
                    .with_note(
                        "wolf generics are rank-1 over complete types (no higher-kinded \
                         parameters, D28); accept the applied type as its own parameter \
                         and let the caller pass it whole.",
                    ),
                );
                return self.table.error();
            }
            if segs.len() == 1 {
                return self.table.intern(TyKind::Rigid(first));
            }
            if segs.len() == 2 {
                // `T.Item` — an associated-type projection through the
                // parameter's bounds (provability checked by s14's
                // archetype validation).
                let base = self.table.intern(TyKind::Rigid(first));
                return self.table.intern(TyKind::Proj(base, segs[1].0.clone()));
            }
            return self.opaque(file, node);
        }
        if segs.len() == 1
            && !has_args
            && let Some(p) = Prim::from_name(&first)
        {
            return self.table.prim(p);
        }

        // Resolve the head: imports first (file-scoped), then module
        // items — mirroring the resolver's order.
        let target = self.resolve_type_head(module, file, &segs);
        match target {
            TypeHead::Item { module: tm, name } => {
                if has_args {
                    // s94: an APPLIED user nominal (`Map[str, int]`)
                    // elaborates to `Nominal { args }` when every
                    // argument is a type and the arity matches the
                    // declaration. Anything else — const-generic
                    // value arguments (`Buf[2 + 2]`), arity errors,
                    // generic aliases — stays opaque, exactly the
                    // pre-s94 rendering, so nothing downstream
                    // changes for the shapes s94 does not claim.
                    if let Some(t) = self.applied_item_type(tm, &name, module, file, generics, &d) {
                        return t;
                    }
                    return self.opaque(file, node);
                }
                self.named_item_type(tm, &name, file, node)
            }
            TypeHead::Unknown => {
                if generics.contains(&first) || Prim::from_name(&first).is_some() {
                    // qualified/applied builtin form — not a s13 type
                    self.opaque(file, node)
                } else if segs.len() == 1 && matches!(first.as_str(), "List" | "Pool") {
                    // The two prelude containers the Tier-2 corpus
                    // rests on (s21): typed as builtins so `handle`
                    // pools and the region litmuses check. Every other
                    // prelude generic stays opaque until s16/s37.
                    let arg_tys: Vec<TyId> = d
                        .args()
                        .into_iter()
                        .flat_map(|a| a.args())
                        .filter(|a| is_type_kind(a.kind))
                        .map(|a| self.lower_type(module, file, generics, a))
                        .collect();
                    match (first.as_str(), arg_tys.as_slice()) {
                        ("List", &[elem]) => self.table.intern(TyKind::List(elem)),
                        ("Pool", &[elem]) => self.table.intern(TyKind::Pool(elem)),
                        _ => self.opaque(file, node),
                    }
                } else {
                    // Resolution already reported unresolved names; a
                    // prelude collection name lands here too.
                    self.opaque(file, node)
                }
            }
            TypeHead::Poisoned => self.table.error(),
        }
    }

    pub(crate) fn resolve_type_head(
        &self,
        module: usize,
        file: usize,
        segs: &[(String, Span)],
    ) -> TypeHead {
        let first = &segs[0].0;
        for b in bindings_for(self.pkg, module, file) {
            if b.name == *first {
                return match &b.target {
                    BindTarget::PkgModule(m) => match segs.get(1) {
                        Some((member, _)) if segs.len() == 2 => TypeHead::Item {
                            module: *m,
                            name: member.clone(),
                        },
                        _ => TypeHead::Unknown,
                    },
                    BindTarget::Item { module, name } if segs.len() == 1 => TypeHead::Item {
                        module: *module,
                        name: name.clone(),
                    },
                    BindTarget::Poisoned => TypeHead::Poisoned,
                    _ => TypeHead::Unknown,
                };
            }
        }
        if segs.len() == 1 && self.pkg.tables[module].get(first).is_some() {
            return TypeHead::Item {
                module,
                name: first.clone(),
            };
        }
        TypeHead::Unknown
    }

    /// A path that resolved to a module item used as a type.
    pub(crate) fn named_item_type(
        &mut self,
        module: usize,
        name: &str,
        file: usize,
        node: &GreenNode,
    ) -> TyId {
        let Some(item) = self.pkg.tables[module].get(name) else {
            return self.table.error();
        };
        let inode = item_node(self.pkg, item);
        match inode.kind {
            SyntaxKind::StructDecl | SyntaxKind::EnumDecl => self.table.intern(TyKind::Nominal {
                module: module as u32,
                name: name.to_string(),
                args: Vec::new(),
            }),
            SyntaxKind::TypeDecl => {
                let d = TypeDecl::cast(inode).expect("kind");
                let generic = d.generics().is_some_and(|g| g.params().next().is_some());
                match d.def() {
                    Some(def)
                        if matches!(def.kind, SyntaxKind::StructDef | SyntaxKind::EnumDef) =>
                    {
                        self.table.intern(TyKind::Nominal {
                            module: module as u32,
                            name: name.to_string(),
                            args: Vec::new(),
                        })
                    }
                    // An adapter (`type X = distinct T`) is a nominal
                    // type of its own — never alias-expanded (D28).
                    Some(def) if !generic && is_distinct_def(def) => {
                        self.table.intern(TyKind::Nominal {
                            module: module as u32,
                            name: name.to_string(),
                            args: Vec::new(),
                        })
                    }
                    Some(def) if !generic => {
                        // Alias expansion, in the alias's own context,
                        // with a cycle guard.
                        let key = (module, name.to_string());
                        if self.alias_stack.contains(&key) {
                            return self.table.error();
                        }
                        self.alias_stack.push(key);
                        let t = self.lower_type(module, item.file, &[], def);
                        self.alias_stack.pop();
                        t
                    }
                    _ => self.opaque(file, node),
                }
            }
            _ => self.opaque(file, node),
        }
    }

    /// s94: elaborate `Name[args…]` against a generic struct/enum
    /// declaration. `None` means "not a shape s94 elaborates" — the
    /// caller falls back to the opaque rendering.
    #[allow(clippy::too_many_arguments)]
    fn applied_item_type(
        &mut self,
        tm: usize,
        name: &str,
        module: usize,
        file: usize,
        generics: &[String],
        d: &PathType<'_>,
    ) -> Option<TyId> {
        let item = self.pkg.tables[tm].get(name)?;
        let inode = item_node(self.pkg, item);
        // Struct/enum declarations and `type X[…] = struct/enum` defs;
        // generic aliases and everything else keep the opaque path.
        let is_adt = match inode.kind {
            SyntaxKind::StructDecl | SyntaxKind::EnumDecl => true,
            SyntaxKind::TypeDecl => TypeDecl::cast(inode)
                .and_then(|td| td.def())
                .is_some_and(|def| matches!(def.kind, SyntaxKind::StructDef | SyntaxKind::EnumDef)),
            _ => false,
        };
        if !is_adt {
            return None;
        }
        let params = generic_names(self, item.file, inode);
        if params.is_empty() {
            return None;
        }
        let arg_nodes: Vec<&GreenNode> = d.args().into_iter().flat_map(|a| a.args()).collect();
        // Every argument must BE a type: a value argument (`Buf[2 + 2]`)
        // is const-generic surface, which has no home in the table yet.
        if arg_nodes.len() != params.len() || !arg_nodes.iter().all(|a| is_type_kind(a.kind)) {
            return None;
        }
        let args: Vec<TyId> = arg_nodes
            .iter()
            .map(|a| self.lower_type(module, file, generics, a))
            .collect();
        Some(self.table.intern(TyKind::Nominal {
            module: tm as u32,
            name: name.to_string(),
            args,
        }))
    }

    /// The opaque fallback: keep the source rendering (whitespace
    /// normalized) so identical spellings stay equal and every
    /// operation on the value is NotYetCheckable.
    pub(crate) fn opaque(&mut self, file: usize, node: &GreenNode) -> TyId {
        let text = self.text(file, node.span);
        let norm = text.split_whitespace().collect::<Vec<_>>().join(" ");
        self.table.intern(TyKind::Unsupported(norm))
    }
}

pub(crate) enum TypeHead {
    Item { module: usize, name: String },
    Unknown,
    Poisoned,
}

enum BoundCheck {
    Ok,
    NotATrait(&'static str),
    Parameterized,
}

/// Is this TypeDecl right-hand side the `distinct T` prefix form?
pub(crate) fn is_distinct_def(def: &GreenNode) -> bool {
    def.kind == SyntaxKind::PrefixType
        && def
            .tokens()
            .next()
            .is_some_and(|t| t.kind == SyntaxKind::DistinctKw)
}

/// A literal-initializer type for the E0407 hint ("the compiler knows").
fn literal_type_hint(node: &GreenNode) -> Option<&'static str> {
    let init = node.nodes().find(|n| wolf_ast::is_expr_kind(n.kind))?;
    match init.kind {
        SyntaxKind::StringExpr => Some("str"),
        SyntaxKind::LiteralExpr => {
            let t = init.tokens().next()?;
            match t.kind {
                SyntaxKind::Int => Some("int"),
                SyntaxKind::Float => Some("f64"),
                SyntaxKind::TrueKw | SyntaxKind::FalseKw => Some("bool"),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Generic parameter names of an item node (empty if none).
fn generic_names(lower: &Lower<'_>, file: usize, node: &GreenNode) -> Vec<String> {
    node.nodes()
        .find_map(wolf_ast::GenericParamList::cast)
        .map(|g| {
            g.params()
                .filter_map(|p| p.name().map(|t| lower.text(file, t.span)))
                .collect()
        })
        .unwrap_or_default()
}

/// The `#[trusted]` / `#[trusted("obligation")]` attribute on an item
/// node (s22, D11 ring 2). Returns the declared obligation text —
/// empty for the bare form — or `None` when the item is not trusted.
/// Shared by signature elaboration, the `wolfi` roster, and the audit
/// surface, so the three never disagree.
pub(crate) fn trusted_attr(node: &GreenNode, text: impl Fn(Span) -> String) -> Option<String> {
    for attr in node.nodes().filter_map(wolf_ast::Attribute::cast) {
        for item in attr.items() {
            let named = item
                .path()
                .map(|p| text(p.syntax().span))
                .is_some_and(|n| n.trim() == "trusted");
            if !named {
                continue;
            }
            let obligation = item
                .input()
                .map(|input| {
                    let raw = text(input.span);
                    let inner = raw
                        .trim()
                        .trim_start_matches('(')
                        .trim_end_matches(')')
                        .trim();
                    inner.trim_matches('"').to_string()
                })
                .unwrap_or_default();
            return Some(obligation);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{AliasTable, MemoryLoader, load_package};
    use crate::types::render;

    fn pkg(files: &[(&[&str], &str, &str)]) -> Package {
        let mut ml = MemoryLoader::new("t");
        for (m, n, s) in files {
            ml.add_file(m, n, s);
        }
        load_package(&mut ml, &AliasTable::default()).expect("loads")
    }

    fn no_vars(_: u32) -> Result<TyId, &'static str> {
        Err("_")
    }

    #[test]
    fn fn_signatures_elaborate_to_interned_types() {
        let p = pkg(&[(
            &[],
            "main.lu",
            "fn area(w: int, h: int) -> int { w * h }\nfn main() -> !int { area(2, 3) }\n",
        )]);
        let sigs = build_sigs(&p);
        let ItemSig::Fn(f) = sigs.get(0, "area").expect("area") else {
            panic!("fn sig")
        };
        assert_eq!(f.params.len(), 2);
        assert_eq!(render(&sigs.table, f.ret, &no_vars), "int");
        let ItemSig::Fn(m) = sigs.get(0, "main").expect("main") else {
            panic!("fn sig")
        };
        assert_eq!(render(&sigs.table, m.ret, &no_vars), "!int");
        assert!(sigs.diagnostics.is_empty(), "{:?}", sigs.diagnostics);
    }

    #[test]
    fn cross_module_types_are_nominal_and_shared() {
        let p = pkg(&[
            (
                &[],
                "main.lu",
                "use geometry\nfn go(c: geometry.Circle) -> f64 { 0.0 }\nfn main() -> !int { 0 }\n",
            ),
            (
                &["geometry"],
                "shapes.lu",
                "pub struct Circle { pub r: f64 }\n",
            ),
        ]);
        let sigs = build_sigs(&p);
        let ItemSig::Fn(f) = sigs.get(0, "go").expect("go") else {
            panic!("fn sig")
        };
        assert!(matches!(
            sigs.table.kind(f.params[0].ty),
            TyKind::Nominal { .. }
        ));
    }

    #[test]
    fn generic_instantiations_elaborate() {
        // s94 (the reversal of the s21-era pin): an applied DECLARED
        // generic nominal elaborates to `Nominal { args }` — two args,
        // both typed. An UNDECLARED head still goes opaque (resolution
        // owns that report), and a const-generic application (`Buf[2 +
        // 2]`) stays opaque until value arguments exist in the table.
        let p = pkg(&[(
            &[],
            "main.lu",
            "struct Map[K, V] { key: K, val: V }\n\
             struct Big { data: Map[str, int] }\n\
             struct Buf[N: type] { len: int }\n\
             struct Holds { b: Buf[2 + 2] }\n\
             struct Bare { m: Mip[str, int] }\n\
             fn main() -> !int { 0 }\n",
        )]);
        let sigs = build_sigs(&p);
        let ItemSig::Struct(s) = sigs.get(0, "Big").expect("Big") else {
            panic!("struct sig")
        };
        let TyKind::Nominal { name, args, .. } = sigs.table.kind(s.fields[0].ty) else {
            panic!(
                "Map[str, int] should elaborate, got {:?}",
                sigs.table.kind(s.fields[0].ty)
            )
        };
        assert_eq!(name, "Map");
        assert_eq!(args.len(), 2);
        assert!(matches!(sigs.table.kind(args[0]), TyKind::Prim(Prim::Str)));
        assert!(matches!(sigs.table.kind(args[1]), TyKind::Prim(p) if p.is_integer()));
        // The Map declaration itself carries its parameter names.
        let ItemSig::Struct(m) = sigs.get(0, "Map").expect("Map") else {
            panic!("Map sig")
        };
        assert_eq!(m.generics, vec!["K".to_string(), "V".to_string()]);
        assert!(matches!(
            sigs.table.kind(m.fields[0].ty),
            TyKind::Rigid(r) if r == "K"
        ));
        // Const-generic application: opaque, exactly as before s94.
        let ItemSig::Struct(h) = sigs.get(0, "Holds").expect("Holds") else {
            panic!("Holds sig")
        };
        assert!(matches!(
            sigs.table.kind(h.fields[0].ty),
            TyKind::Unsupported(t) if t == "Buf[2 + 2]"
        ));
        // Undeclared head: opaque.
        let ItemSig::Struct(b) = sigs.get(0, "Bare").expect("Bare") else {
            panic!("Bare sig")
        };
        assert!(matches!(
            sigs.table.kind(b.fields[0].ty),
            TyKind::Unsupported(t) if t == "Mip[str, int]"
        ));
    }

    #[test]
    fn prelude_containers_lower_as_builtins() {
        // s21: the two Tier-2 prelude containers are structural types
        // now — `List[int]` and `Pool[Node]` fields elaborate, and the
        // element type is reachable (the E1006 walk needs the edges).
        let p = pkg(&[(
            &[],
            "main.lu",
            "struct Node { value: int }\n\
             struct Big { xs: List[int], pool: Pool[Node], hs: List[handle Node] }\n\
             fn main() -> !int { 0 }\n",
        )]);
        let sigs = build_sigs(&p);
        let ItemSig::Struct(s) = sigs.get(0, "Big").expect("Big") else {
            panic!("struct sig")
        };
        assert!(matches!(sigs.table.kind(s.fields[0].ty), TyKind::List(_)));
        assert!(matches!(sigs.table.kind(s.fields[1].ty), TyKind::Pool(_)));
        let TyKind::List(inner) = sigs.table.kind(s.fields[2].ty) else {
            panic!("List[handle Node]")
        };
        assert!(matches!(sigs.table.kind(*inner), TyKind::Handle(_)));
    }

    #[test]
    fn missing_global_annotation_is_e0407_with_fixit() {
        let p = pkg(&[(
            &[],
            "main.lu",
            "let banner = \"hi\"\nfn main() -> !int { 0 }\n",
        )]);
        let sigs = build_sigs(&p);
        assert_eq!(sigs.diagnostics.len(), 1);
        let d = &sigs.diagnostics[0];
        assert_eq!(d.code, codes::E0407);
        assert!(d.suggestions[0].message.contains("str"), "{d:?}");
    }

    #[test]
    fn wrapping_is_a_builtin_integer_constructor() {
        let p = pkg(&[(
            &[],
            "main.lu",
            "fn spin(h: wrapping[u32]) -> wrapping[u32] { h }\nfn main() -> !int { 0 }\n",
        )]);
        let sigs = build_sigs(&p);
        let ItemSig::Fn(f) = sigs.get(0, "spin").expect("spin") else {
            panic!("fn sig")
        };
        assert!(matches!(
            sigs.table.kind(f.params[0].ty),
            TyKind::Wrapping(_)
        ));
        assert!(sigs.diagnostics.is_empty());
    }

    #[test]
    fn alias_expansion_and_cycle_guard() {
        let p = pkg(&[(
            &[],
            "main.lu",
            "type Meters = f64\ntype A = B\ntype B = A\nfn len(m: Meters) -> f64 { m }\nfn spin(a: A) -> int { 0 }\nfn main() -> !int { 0 }\n",
        )]);
        let sigs = build_sigs(&p);
        let ItemSig::Fn(f) = sigs.get(0, "len").expect("len") else {
            panic!("fn sig")
        };
        assert_eq!(render(&sigs.table, f.params[0].ty, &no_vars), "f64");
        let ItemSig::Fn(s) = sigs.get(0, "spin").expect("spin") else {
            panic!("fn sig")
        };
        assert!(matches!(sigs.table.kind(s.params[0].ty), TyKind::Error));
    }
}
