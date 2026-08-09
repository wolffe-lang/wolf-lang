//! Traits, impls, and coherence (s14, D28).
//!
//! Nominal traits with **isolated member namespaces** (Carbon-style:
//! two traits may collide on method names; calls are qualified,
//! `Trait.method(args)` — no global method soup, receiver syntax is
//! s17). Trait generic parameters are *inputs* (they select the
//! impl); associated types and consts are *outputs* (determined by
//! the impl, never driving input inference — RFC 0195).
//!
//! Coherence is global with the **simple orphan rule**: an impl lives
//! in the trait's module or the self type's module, nowhere else
//! (E0504); uncovered impl-header parameters are rejected outright
//! (E0505); overlap between impls is a hard error found by trial
//! unification on the s13 snapshot/rollback machinery (E0506), with
//! blanket impls checked uniformly — and **no specialization of any
//! kind** (D28, locked): there is no overlap-ordering machinery here
//! and none may be added.
//!
//! The sanctioned orphan escape is the **adapter type**:
//! `type Cover = distinct Song` is a nominal type of its own — same
//! layout as its base (the layout-identity fact recorded in
//! [`crate::sig::ItemSig::Distinct`] for c05; `as` casts both ways
//! are free), an empty impl set (nothing inherited), full impl
//! freedom in its defining module. No `adapt` keyword exists.
//!
//! `dyn Trait` safety follows the RFC 0255 model — no generic
//! methods (E0508), no unconstrained associated-type/input escapes
//! (E0509), `Self` only in receiver position (E0510) — checked where
//! the `dyn` type is written; sema records the dyn-safe method set in
//! canonical order for the interface (witness layout is c05's).

use std::collections::BTreeMap;

use wolf_ast::{ConstDecl, FnDecl, GreenNode, ImplDecl, SyntaxKind, TraitDecl, TypeDecl};
use wolf_diag::{Diagnostic, codes};
use wolf_span::Span;

use crate::graph::ItemKind;
use crate::rewrite;
use crate::sig::{BoundRef, FnSig, GenericSig, Lower, SigTables, TypeHead};
use crate::types::{Prim, TyId, TyKind, TypeTable, render, subst};
use crate::unify::{NumKind, VarStore, unify};

/// A trait's global identity: defining module + name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraitRef {
    pub module: usize,
    pub name: String,
}

/// One trait method declaration. `Self` appears as `Rigid("Self")`
/// in the signature; a receiver elaborates as a leading `Self` param.
#[derive(Debug, Clone)]
pub struct TraitMethod {
    pub name: String,
    pub sig: FnSig,
    pub has_self: bool,
    /// The trait provides a default body (impls may omit the method;
    /// default bodies themselves are s17's checking work).
    pub has_default: bool,
    pub name_span: Span,
}

/// An associated type declared by a trait (`type Item = type` — an
/// *output* slot each impl must bind).
#[derive(Debug, Clone)]
pub struct AssocTypeDecl {
    pub name: String,
    pub name_span: Span,
}

/// An associated const declared by a trait (`const N: int = …` — the
/// initializer is its default; impls may rebind at the same type).
#[derive(Debug, Clone)]
pub struct AssocConstDecl {
    pub name: String,
    pub ty: TyId,
    pub name_span: Span,
}

/// Which dyn-safety rule a trait member breaks (RFC 0255 classes,
/// each with its own catalog code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynClass {
    /// E0508 — a method with its own generic parameters.
    GenericMethod,
    /// E0509 — an associated type escapes into a method signature.
    AssocEscape,
    /// E0510 — `Self` outside receiver position.
    SelfPosition,
    /// E0509 — the trait declares input parameters (unconstrainable
    /// at a `dyn` spelling; shares the escape code).
    InputParams,
}

/// One dyn-safety violation, pointing at the offending member.
#[derive(Debug, Clone)]
pub struct DynViolation {
    pub class: DynClass,
    /// Offending method name (empty for trait-level violations).
    pub method: String,
    pub span: Span,
}

/// A trait's dyn-safety verdict + the dyn-safe method set in
/// canonical (sorted) order — the interface records this (c05 builds
/// witness tables from it).
#[derive(Debug, Clone, Default)]
pub struct DynReport {
    pub violations: Vec<DynViolation>,
    /// Method names in canonical order; meaningful when `safe()`.
    pub methods: Vec<String>,
}

impl DynReport {
    pub fn safe(&self) -> bool {
        self.violations.is_empty()
    }
}

/// One trait's elaborated declaration.
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub module: usize,
    pub name: String,
    pub name_span: Span,
    /// Input parameters (select the impl; RFC 0195).
    pub params: Vec<String>,
    pub methods: Vec<TraitMethod>,
    pub assoc_types: Vec<AssocTypeDecl>,
    pub assoc_consts: Vec<AssocConstDecl>,
    pub dyn_report: DynReport,
}

impl TraitDef {
    pub fn method(&self, name: &str) -> Option<&TraitMethod> {
        self.methods.iter().find(|m| m.name == name)
    }
}

/// One impl member function, fully elaborated (`Self` already
/// substituted by the impl's self type, projections normalized under
/// the impl's rewrite rules).
#[derive(Debug, Clone)]
pub struct ImplMethod {
    pub name: String,
    pub sig: FnSig,
    pub has_self: bool,
    /// Ordinal among the impl's member item nodes (body lookup).
    pub member: usize,
    pub name_span: Span,
    pub has_body: bool,
}

/// One impl member const (an associated-const rebinding).
#[derive(Debug, Clone)]
pub struct ImplConst {
    pub name: String,
    pub ty: TyId,
    pub member: usize,
    pub name_span: Span,
}

/// One `impl` block, elaborated.
#[derive(Debug, Clone)]
pub struct ImplDef {
    pub module: usize,
    pub file: usize,
    /// Ordinal among the file root's item nodes (body lookup).
    pub decl: usize,
    /// The impl header's span (diagnostics).
    pub span: Span,
    pub generics: Vec<GenericSig>,
    /// `None` for inherent impls (`impl Type { … }`).
    pub trait_ref: Option<TraitRef>,
    pub trait_args: Vec<TyId>,
    pub self_ty: TyId,
    pub methods: Vec<ImplMethod>,
    /// Associated-type bindings — the rewrite rules of this impl
    /// (`Self.Name` normalizes through these; [`crate::rewrite`]).
    pub rewrites: rewrite::Rules,
    pub consts: Vec<ImplConst>,
}

/// Rigid-name → bounds environment of the body being checked (the
/// archetype facts: everything a generic body may use, D28).
pub type Bounds = BTreeMap<String, Vec<BoundRef>>;

// ------------------------------------------------------------- build ----

/// Elaborate every trait and impl of the package into `sigs`, then
/// run the coherence, conformance, ceiling, and dyn-safety checks.
/// Diagnostics land in `lower.diags` (merged by `build_sigs`).
pub(crate) fn build(lower: &mut Lower<'_>) -> (BTreeMap<TraitRef, TraitDef>, Vec<ImplDef>) {
    let pkg = lower.pkg;
    let mut traits: BTreeMap<TraitRef, TraitDef> = BTreeMap::new();
    for &m in &pkg.topo {
        for item in &pkg.tables[m].items {
            if item.kind != ItemKind::Trait {
                continue;
            }
            let node = crate::sig::item_node(pkg, item);
            if node.kind != SyntaxKind::TraitDecl {
                continue;
            }
            let def = build_trait(lower, m, item.file, node, &item.name, item.name_span);
            traits.insert(
                TraitRef {
                    module: m,
                    name: item.name.clone(),
                },
                def,
            );
        }
    }
    let mut impls: Vec<ImplDef> = Vec::new();
    for &m in &pkg.topo {
        for &fi in &pkg.modules[m].files {
            let root = &pkg.files[fi].parse.root;
            for (decl, node) in root.nodes().filter(|n| n.kind.is_item()).enumerate() {
                if node.kind == SyntaxKind::ImplDecl
                    && let Some(imp) = build_impl(lower, &traits, m, fi, decl, node)
                {
                    impls.push(imp);
                }
            }
        }
    }
    check_orphans(lower, &traits, &impls);
    check_uncovered(lower, &impls);
    check_overlap(lower, &impls);
    check_conformance(lower, &traits, &impls);
    check_dyn_uses(lower, &traits);
    (traits, impls)
}

fn build_trait(
    lower: &mut Lower<'_>,
    module: usize,
    file: usize,
    node: &GreenNode,
    name: &str,
    name_span: Span,
) -> TraitDef {
    let d = TraitDecl::cast(node).expect("kind");
    let params: Vec<String> = d
        .generics()
        .into_iter()
        .flat_map(|g| g.params())
        .filter_map(|p| p.name().map(|t| text(lower, file, t.span)))
        .collect();
    let mut scope: Vec<String> = params.clone();
    scope.push("Self".to_string());
    let mut methods = Vec::new();
    let mut assoc_types = Vec::new();
    let mut assoc_consts = Vec::new();
    for member in d.members() {
        match member.kind {
            SyntaxKind::FnDecl => {
                let f = FnDecl::cast(member).expect("kind");
                let Some(nt) = f.name() else { continue };
                let mname = text(lower, file, nt.span);
                let sig = lower.fn_sig_in(module, file, member, nt.span, &scope);
                let has_self = f
                    .params()
                    .into_iter()
                    .flat_map(|p| p.params())
                    .any(|p| p.is_self());
                methods.push(TraitMethod {
                    name: mname,
                    sig,
                    has_self,
                    has_default: f.body().is_some(),
                    name_span: nt.span,
                });
            }
            SyntaxKind::TypeDecl => {
                let t = TypeDecl::cast(member).expect("kind");
                let Some(nt) = t.name() else { continue };
                reject_gat(lower, file, member);
                assoc_types.push(AssocTypeDecl {
                    name: text(lower, file, nt.span),
                    name_span: nt.span,
                });
            }
            SyntaxKind::ConstDecl => {
                let c = ConstDecl::cast(member).expect("kind");
                let Some(nt) = c.name() else { continue };
                let ty = c
                    .ty()
                    .map(|t| lower.lower_type(module, file, &scope, t))
                    .unwrap_or_else(|| lower.table.error());
                assoc_consts.push(AssocConstDecl {
                    name: text(lower, file, nt.span),
                    ty,
                    name_span: nt.span,
                });
            }
            _ => {}
        }
    }
    let dyn_report = dyn_report(node, src(lower, file));
    TraitDef {
        module,
        name: name.to_string(),
        name_span,
        params,
        methods,
        assoc_types,
        assoc_consts,
        dyn_report,
    }
}

/// E0512 — the no-GATs ceiling: associated types take no parameters.
fn reject_gat(lower: &mut Lower<'_>, file: usize, member: &GreenNode) {
    let t = TypeDecl::cast(member).expect("kind");
    if t.generics().is_some_and(|g| g.params().next().is_some()) {
        let span = t.name().map(|n| n.span).unwrap_or(member.span);
        let _ = file;
        lower.diags.push(
            Diagnostic::error(
                codes::E0512,
                span,
                "an associated type cannot have generic parameters of its own",
            )
            .with_label("generic associated type here")
            .with_note(
                "wolf v1 has no GATs (D28 — the ceilings are stated, not discovered); \
                 move the parameter onto the trait or the method that needs it.",
            ),
        );
    }
}

fn build_impl(
    lower: &mut Lower<'_>,
    traits: &BTreeMap<TraitRef, TraitDef>,
    module: usize,
    file: usize,
    decl: usize,
    node: &GreenNode,
) -> Option<ImplDef> {
    let d = ImplDecl::cast(node)?;
    let generics = lower.generic_defs(module, file, node);
    let gnames: Vec<String> = generics.iter().map(|g| g.name.clone()).collect();
    let mut scope = gnames.clone();
    scope.push("Self".to_string());

    let path = d.trait_path()?;
    let segs: Vec<(String, Span)> = path
        .segments()
        .map(|t| (text(lower, file, t.span), t.span))
        .collect();
    if segs.is_empty() {
        return None;
    }
    let path_span = segs.iter().fold(segs[0].1, |a, (_, s)| a.join(*s));
    // The impl's own trait-argument list is a *direct* child of the
    // ImplDecl node (`impl Convert[int] for Meters`).
    let arg_list = node.nodes().find_map(wolf_ast::TypeArgList::cast);
    let trait_args: Vec<TyId> = arg_list
        .into_iter()
        .flat_map(|a| a.args())
        .filter(|a| wolf_ast::is_type_kind(a.kind))
        .map(|a| lower.lower_type(module, file, &gnames, a))
        .collect();

    let (trait_ref, self_ty) = match d.self_ty() {
        Some(self_node) => {
            // `impl Trait[…] for Type` — the first path names a trait.
            let tr = match lower.resolve_type_head(module, file, &segs) {
                TypeHead::Item { module: tm, name } => {
                    let is_trait = traits.contains_key(&TraitRef {
                        module: tm,
                        name: name.clone(),
                    });
                    if is_trait {
                        Some(TraitRef { module: tm, name })
                    } else {
                        let kind = lower.pkg.tables[tm]
                            .get(&name)
                            .map(|i| i.kind.keyword())
                            .unwrap_or("name");
                        lower.diags.push(
                            Diagnostic::error(
                                codes::E0503,
                                path_span,
                                format!("`{name}` is a {kind}, and only a trait can be implemented `for` a type"),
                            )
                            .with_label("not a trait")
                            .with_note(
                                "in `impl X for T`, `X` must name a trait; an inherent \
                                 impl of `T` itself is written `impl T { … }`.",
                            ),
                        );
                        None
                    }
                }
                // Unresolved: resolution already reported E0301.
                TypeHead::Unknown | TypeHead::Poisoned => None,
            };
            let st = lower.lower_type(module, file, &gnames, self_node);
            (tr, st)
        }
        None => {
            // Inherent impl: the path is the subject type.
            let st = subject_type(lower, module, file, &gnames, &segs, path.syntax());
            (None, st)
        }
    };

    // Pass 1 — associated bindings and consts (the rewrite rules).
    let mut rewrites = rewrite::Rules::new();
    let mut consts = Vec::new();
    let self_map: BTreeMap<String, TyId> = [("Self".to_string(), self_ty)].into();
    for (member, mnode) in node.nodes().filter(|n| n.kind.is_item()).enumerate() {
        match mnode.kind {
            SyntaxKind::TypeDecl => {
                let t = TypeDecl::cast(mnode).expect("kind");
                let Some(nt) = t.name() else { continue };
                reject_gat(lower, file, mnode);
                let bound = t
                    .def()
                    .map(|def| lower.lower_type(module, file, &scope, def))
                    .unwrap_or_else(|| lower.table.error());
                let bound = subst(&mut lower.table, bound, &self_map);
                rewrites.insert(text(lower, file, nt.span), bound);
            }
            SyntaxKind::ConstDecl => {
                let c = ConstDecl::cast(mnode).expect("kind");
                let Some(nt) = c.name() else { continue };
                let ty = c
                    .ty()
                    .map(|t| lower.lower_type(module, file, &scope, t))
                    .unwrap_or_else(|| lower.table.error());
                let ty = subst(&mut lower.table, ty, &self_map);
                consts.push(ImplConst {
                    name: text(lower, file, nt.span),
                    ty,
                    member,
                    name_span: nt.span,
                });
            }
            _ => {}
        }
    }
    // Cycle rejection up front (E0513) — canonicalization then always
    // reaches its fixed point (the rewrite-engine contract).
    if let Err(rewrite::Cycle(names)) = rewrite::check_cycles(&lower.table, self_ty, &rewrites) {
        lower.diags.push(
            Diagnostic::error(
                codes::E0513,
                node.span,
                format!(
                    "the associated types {} are bound in terms of each other",
                    names
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
            )
            .with_label("this impl's bindings form a cycle")
            .with_note(
                "associated types normalize by rewriting to a fixed point; a cycle \
                 never reaches one — bind at least one of them to a concrete type.",
            ),
        );
        rewrites.clear();
    }
    // Consts normalize under the rules too.
    for c in &mut consts {
        c.ty = rewrite::canonicalize(&mut lower.table, self_ty, &rewrites, c.ty);
    }

    // Pass 2 — methods, `Self` substituted and projections normalized.
    let mut methods = Vec::new();
    for (member, mnode) in node.nodes().filter(|n| n.kind.is_item()).enumerate() {
        if mnode.kind != SyntaxKind::FnDecl {
            continue;
        }
        let f = FnDecl::cast(mnode).expect("kind");
        let Some(nt) = f.name() else { continue };
        let mut sig = lower.fn_sig_in(module, file, mnode, nt.span, &scope);
        for p in &mut sig.params {
            let s = subst(&mut lower.table, p.ty, &self_map);
            p.ty = rewrite::canonicalize(&mut lower.table, self_ty, &rewrites, s);
        }
        let r = subst(&mut lower.table, sig.ret, &self_map);
        sig.ret = rewrite::canonicalize(&mut lower.table, self_ty, &rewrites, r);
        let has_self = f
            .params()
            .into_iter()
            .flat_map(|p| p.params())
            .any(|p| p.is_self());
        methods.push(ImplMethod {
            name: text(lower, file, nt.span),
            sig,
            has_self,
            member,
            name_span: nt.span,
            has_body: f.body().is_some(),
        });
    }

    Some(ImplDef {
        module,
        file,
        decl,
        span: path_span,
        generics,
        trait_ref,
        trait_args,
        self_ty,
        methods,
        rewrites,
        consts,
    })
}

/// The subject type of an inherent impl (`impl Point { … }`).
fn subject_type(
    lower: &mut Lower<'_>,
    module: usize,
    file: usize,
    gnames: &[String],
    segs: &[(String, Span)],
    path_node: &GreenNode,
) -> TyId {
    if segs.len() == 1 {
        let n = &segs[0].0;
        if gnames.iter().any(|g| g == n) {
            return lower.table.intern(TyKind::Rigid(n.clone()));
        }
        if let Some(p) = Prim::from_name(n) {
            return lower.table.prim(p);
        }
    }
    match lower.resolve_type_head(module, file, segs) {
        TypeHead::Item { module: tm, name } => lower.named_item_type(tm, &name, file, path_node),
        TypeHead::Poisoned => lower.table.error(),
        TypeHead::Unknown => lower.opaque(file, path_node),
    }
}

// --------------------------------------------------------- coherence ----

/// E0504 — the simple orphan rule: an impl lives with its trait or
/// with its self type.
fn check_orphans(lower: &mut Lower<'_>, traits: &BTreeMap<TraitRef, TraitDef>, impls: &[ImplDef]) {
    for imp in impls {
        let Some(tr) = &imp.trait_ref else { continue };
        if tr.module == imp.module {
            continue;
        }
        let self_home = match lower.table.kind(imp.self_ty) {
            TyKind::Nominal { module, .. } => Some(*module as usize),
            _ => None,
        };
        if self_home == Some(imp.module) {
            continue;
        }
        let mut d = Diagnostic::error(
            codes::E0504,
            imp.span,
            format!(
                "this impl of `{}` is an orphan: neither the trait nor `{}` lives in this module",
                tr.name,
                render(&lower.table, imp.self_ty, &|_| Err("_")),
            ),
        )
        .with_label("impl written here")
        .with_note(
            "an impl must live in the trait's module or the self type's module \
             (global coherence, D28). When both are foreign, declare an adapter — \
             `type Local = distinct Foreign` — and implement the trait for it: \
             same layout, free casts, its own impl set.",
        );
        if let Some(td) = traits.get(tr) {
            d = d.with_secondary(td.name_span, "the trait is declared here");
        }
        lower.diags.push(d);
    }
}

/// E0505 — every impl generic parameter must appear in the impl's
/// subject (self type or trait arguments).
fn check_uncovered(lower: &mut Lower<'_>, impls: &[ImplDef]) {
    for imp in impls {
        if imp.trait_ref.is_none() {
            continue;
        }
        for g in &imp.generics {
            let covered = mentions_rigid(&lower.table, imp.self_ty, &g.name)
                || imp
                    .trait_args
                    .iter()
                    .any(|&a| mentions_rigid(&lower.table, a, &g.name));
            if !covered {
                lower.diags.push(
                    Diagnostic::error(
                        codes::E0505,
                        g.span,
                        format!("the impl parameter `{}` is not covered by the impl", g.name),
                    )
                    .with_label("appears in neither the self type nor the trait arguments")
                    .with_note(
                        "nothing could ever determine what this parameter is; wolf v1 \
                         disallows uncovered impl parameters outright (D28) — delete \
                         it, or make the impl subject mention it.",
                    ),
                );
            }
        }
    }
}

/// Does `ty` mention the rigid `name` anywhere (textually inside
/// opaque tokens — `List[T]` counts)?
pub(crate) fn mentions_rigid(table: &TypeTable, ty: TyId, name: &str) -> bool {
    match table.kind(ty) {
        TyKind::Rigid(n) => n == name,
        TyKind::Unsupported(s) => s
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| w == name),
        TyKind::Wrapping(t)
        | TyKind::ErrUnion(t)
        | TyKind::Range(t)
        | TyKind::Ptr(t)
        | TyKind::Shared(t)
        | TyKind::Handle(t)
        | TyKind::Weak(t)
        | TyKind::Distinct(t)
        | TyKind::Proj(t, _) => mentions_rigid(table, *t, name),
        TyKind::Tuple(ts) => ts.iter().any(|&t| mentions_rigid(table, t, name)),
        TyKind::Fn(ps, r) => {
            ps.iter().any(|&t| mentions_rigid(table, t, name)) || mentions_rigid(table, *r, name)
        }
        _ => false,
    }
}

/// E0506 — overlap is a hard error, found by trial unification of
/// impl headers (s13 snapshot/rollback). Blanket impls participate
/// uniformly; bounds never disambiguate (no specialization, locked).
fn check_overlap(lower: &mut Lower<'_>, impls: &[ImplDef]) {
    let mut vars = VarStore::new();
    for (i, a) in impls.iter().enumerate() {
        let Some(tra) = &a.trait_ref else { continue };
        for b in impls.iter().skip(i + 1) {
            let Some(trb) = &b.trait_ref else { continue };
            if tra != trb || a.trait_args.len() != b.trait_args.len() {
                continue;
            }
            let snap = vars.snapshot();
            let ok = headers_unify(&mut lower.table, &mut vars, a, b);
            vars.rollback(snap);
            if ok {
                lower.diags.push(
                    Diagnostic::error(
                        codes::E0506,
                        b.span,
                        format!(
                            "this impl of `{}` overlaps another impl of the same trait",
                            trb.name
                        ),
                    )
                    .with_label("second impl here")
                    .with_secondary(a.span, "it can apply to the same types as this one")
                    .with_note(
                        "one trait has at most one impl per type (global coherence), and \
                         wolf has no specialization to pick a winner (D28 — locked). \
                         Delete one impl, or narrow the blanket one; bounds do not \
                         disambiguate overlap.",
                    ),
                );
            }
        }
    }
}

fn headers_unify(table: &mut TypeTable, vars: &mut VarStore, a: &ImplDef, b: &ImplDef) -> bool {
    let fresh_map = |imp: &ImplDef, table: &mut TypeTable, vars: &mut VarStore| {
        let mut m = BTreeMap::new();
        for g in &imp.generics {
            let v = vars.fresh(table, 0, NumKind::Any, g.span);
            m.insert(g.name.clone(), v);
        }
        m
    };
    let ma = fresh_map(a, table, vars);
    let mb = fresh_map(b, table, vars);
    let sa = subst(table, a.self_ty, &ma);
    let sb = subst(table, b.self_ty, &mb);
    if unify(table, vars, sa, sb).is_err() {
        return false;
    }
    for (&x, &y) in a.trait_args.iter().zip(b.trait_args.iter()) {
        let xi = subst(table, x, &ma);
        let yi = subst(table, y, &mb);
        if unify(table, vars, xi, yi).is_err() {
            return false;
        }
    }
    true
}

// ------------------------------------------------------- conformance ----

/// E0507 — an impl supplies exactly what its trait declares.
fn check_conformance(
    lower: &mut Lower<'_>,
    traits: &BTreeMap<TraitRef, TraitDef>,
    impls: &[ImplDef],
) {
    for imp in impls {
        let Some(tr) = &imp.trait_ref else { continue };
        let Some(td) = traits.get(tr) else { continue };
        if imp.trait_args.len() != td.params.len() {
            lower.diags.push(
                Diagnostic::error(
                    codes::E0507,
                    imp.span,
                    format!(
                        "`{}` takes {} input parameter{}, but this impl applies {}",
                        tr.name,
                        td.params.len(),
                        if td.params.len() == 1 { "" } else { "s" },
                        imp.trait_args.len()
                    ),
                )
                .with_label("wrong number of trait arguments")
                .with_secondary(td.name_span, "the trait is declared here"),
            );
            continue;
        }
        let mut map: BTreeMap<String, TyId> = BTreeMap::new();
        map.insert("Self".to_string(), imp.self_ty);
        for (p, &a) in td.params.iter().zip(imp.trait_args.iter()) {
            map.insert(p.clone(), a);
        }
        // Associated types: every output slot bound exactly once.
        for at in &td.assoc_types {
            if !imp.rewrites.contains_key(&at.name) {
                lower.diags.push(
                    Diagnostic::error(
                        codes::E0507,
                        imp.span,
                        format!(
                            "this impl of `{}` does not bind the associated type `{}`",
                            tr.name, at.name
                        ),
                    )
                    .with_label(format!("missing `type {} = …`", at.name))
                    .with_secondary(at.name_span, "declared by the trait here"),
                );
            }
        }
        // Associated consts: rebindings must keep the declared type.
        for ic in &imp.consts {
            match td.assoc_consts.iter().find(|c| c.name == ic.name) {
                None => {
                    lower.diags.push(
                        Diagnostic::error(
                            codes::E0507,
                            ic.name_span,
                            format!("`{}` is not a member of `{}`", ic.name, tr.name),
                        )
                        .with_label("not declared by the trait")
                        .with_secondary(td.name_span, "the trait is declared here"),
                    );
                }
                Some(c) => {
                    let want = subst(&mut lower.table, c.ty, &map);
                    let want =
                        rewrite::canonicalize(&mut lower.table, imp.self_ty, &imp.rewrites, want);
                    if want != ic.ty {
                        let w = render(&lower.table, want, &|_| Err("_"));
                        let g = render(&lower.table, ic.ty, &|_| Err("_"));
                        lower.diags.push(
                            Diagnostic::error(
                                codes::E0507,
                                ic.name_span,
                                format!(
                                    "`{}` is declared as `{w}` by the trait, but this impl binds it at `{g}`",
                                    ic.name
                                ),
                            )
                            .with_label(format!("found `{g}`"))
                            .with_secondary(c.name_span, "declared here"),
                        );
                    }
                }
            }
        }
        // Methods: required ones present, all at the trait's signature.
        for tm in &td.methods {
            let found = imp.methods.iter().find(|m| m.name == tm.name);
            match found {
                None if tm.has_default => {}
                None => {
                    lower.diags.push(
                        Diagnostic::error(
                            codes::E0507,
                            imp.span,
                            format!(
                                "this impl of `{}` is missing the method `{}`",
                                tr.name, tm.name
                            ),
                        )
                        .with_label("no such member in the impl")
                        .with_secondary(tm.name_span, "required by the trait here"),
                    );
                }
                Some(im) => {
                    self::check_method_match(lower, imp, tr, tm, im, &map);
                }
            }
        }
        for im in &imp.methods {
            if td.method(&im.name).is_none() {
                lower.diags.push(
                    Diagnostic::error(
                        codes::E0507,
                        im.name_span,
                        format!("`{}` is not a member of `{}`", im.name, tr.name),
                    )
                    .with_label("not declared by the trait")
                    .with_secondary(td.name_span, "the trait is declared here")
                    .with_note(
                        "callers dispatch through the trait's declaration, so an impl \
                         cannot add members to it; move extra functions to an inherent \
                         `impl Type { … }` block.",
                    ),
                );
            }
        }
    }
}

fn check_method_match(
    lower: &mut Lower<'_>,
    imp: &ImplDef,
    tr: &TraitRef,
    tm: &TraitMethod,
    im: &ImplMethod,
    base_map: &BTreeMap<String, TyId>,
) {
    let mismatch = |lower: &mut Lower<'_>, why: String| {
        lower.diags.push(
            Diagnostic::error(
                codes::E0507,
                im.name_span,
                format!(
                    "the impl's `{}` does not match `{}`'s declaration: {why}",
                    im.name, tr.name
                ),
            )
            .with_label("does not match the trait")
            .with_secondary(tm.name_span, "declared here"),
        );
    };
    if tm.has_self != im.has_self {
        let why = if tm.has_self {
            "the trait method takes `self`, this one does not".to_string()
        } else {
            "this method takes `self`, the trait's does not".to_string()
        };
        mismatch(lower, why);
        return;
    }
    if tm.sig.generics.len() != im.sig.generics.len() {
        mismatch(lower, "different generic parameter counts".to_string());
        return;
    }
    // Canonicalize both signatures: trait side gets Self/inputs
    // substituted, both sides get method generics renamed positionally.
    let mut tmap = base_map.clone();
    let mut imap: BTreeMap<String, TyId> = BTreeMap::new();
    for (i, (tg, ig)) in tm
        .sig
        .generics
        .iter()
        .zip(im.sig.generics.iter())
        .enumerate()
    {
        let canon = lower.table.intern(TyKind::Rigid(format!("%{i}")));
        tmap.insert(tg.name.clone(), canon);
        imap.insert(ig.name.clone(), canon);
    }
    let norm = |lower: &mut Lower<'_>, ty: TyId, map: &BTreeMap<String, TyId>| {
        let s = subst(&mut lower.table, ty, map);
        rewrite::canonicalize(&mut lower.table, imp.self_ty, &imp.rewrites, s)
    };
    if tm.sig.params.len() != im.sig.params.len() {
        mismatch(lower, "different parameter counts".to_string());
        return;
    }
    for (i, (tp, ip)) in tm.sig.params.iter().zip(im.sig.params.iter()).enumerate() {
        let want = norm(lower, tp.ty, &tmap);
        let got = norm(lower, ip.ty, &imap);
        if want != got {
            let w = render(&lower.table, want, &|_| Err("_"));
            let g = render(&lower.table, got, &|_| Err("_"));
            mismatch(
                lower,
                format!("parameter {} is `{g}`, but the trait says `{w}`", i + 1),
            );
            return;
        }
    }
    let want = norm(lower, tm.sig.ret, &tmap);
    let got = norm(lower, im.sig.ret, &imap);
    if want != got {
        let w = render(&lower.table, want, &|_| Err("_"));
        let g = render(&lower.table, got, &|_| Err("_"));
        mismatch(lower, format!("it returns `{g}`, but the trait says `{w}`"));
    }
}

// ------------------------------------------------------- dyn safety ----

/// AST-level dyn-safety analysis of one trait declaration (RFC 0255
/// model). Pure over the tree so the interface builder shares it.
pub fn dyn_report(node: &GreenNode, src: &[u8]) -> DynReport {
    let text = |span: Span| -> String {
        String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
    };
    let Some(d) = TraitDecl::cast(node) else {
        return DynReport::default();
    };
    let mut violations = Vec::new();
    if let Some(g) = d.generics()
        && g.params().next().is_some()
    {
        violations.push(DynViolation {
            class: DynClass::InputParams,
            method: String::new(),
            span: d.name().map(|t| t.span).unwrap_or(node.span),
        });
    }
    let mut methods = Vec::new();
    for member in d.members() {
        if member.kind != SyntaxKind::FnDecl {
            continue;
        }
        let Some(f) = FnDecl::cast(member) else {
            continue;
        };
        let Some(nt) = f.name() else { continue };
        let mname = text(nt.span);
        methods.push(mname.clone());
        if f.generics().is_some_and(|g| g.params().next().is_some()) {
            violations.push(DynViolation {
                class: DynClass::GenericMethod,
                method: mname.clone(),
                span: nt.span,
            });
        }
        // Walk every type node in the signature (parameter types and
        // return type) for `Self` uses.
        let mut bare_self = false;
        let mut assoc_escape = false;
        let mut visit_types = |root: &GreenNode| {
            walk(root, &mut |n| {
                // Ident tokens live under the `Path` node nested in
                // each `PathType`.
                if n.kind != SyntaxKind::Path {
                    return;
                }
                let segs: Vec<String> = n
                    .tokens()
                    .filter(|t| t.kind == SyntaxKind::Ident)
                    .map(|t| text(t.span))
                    .collect();
                if segs.first().map(String::as_str) == Some("Self") {
                    if segs.len() == 1 {
                        bare_self = true;
                    } else {
                        assoc_escape = true;
                    }
                }
            });
        };
        if let Some(params) = f.params() {
            for p in params.params() {
                if let Some(t) = p.ty() {
                    visit_types(t);
                }
            }
        }
        if let Some(r) = f.ret_ty() {
            visit_types(r.syntax());
        }
        if assoc_escape {
            violations.push(DynViolation {
                class: DynClass::AssocEscape,
                method: mname.clone(),
                span: nt.span,
            });
        }
        if bare_self {
            violations.push(DynViolation {
                class: DynClass::SelfPosition,
                method: mname.clone(),
                span: nt.span,
            });
        }
    }
    methods.sort();
    methods.dedup();
    if !violations.is_empty() {
        methods.clear();
    }
    DynReport {
        violations,
        methods,
    }
}

fn walk<'a>(node: &'a GreenNode, f: &mut impl FnMut(&'a GreenNode)) {
    f(node);
    for c in node.nodes() {
        walk(c, f);
    }
}

/// Check every `dyn Trait` spelling in the package against the
/// trait's dyn report — the object-creation-site rule (RFC 0255):
/// diagnostics land where the `dyn` type is written.
fn check_dyn_uses(lower: &mut Lower<'_>, traits: &BTreeMap<TraitRef, TraitDef>) {
    let pkg = lower.pkg;
    type DynUse = (usize, usize, Span, Vec<(String, Span)>);
    let mut uses: Vec<DynUse> = Vec::new();
    for (m, md) in pkg.modules.iter().enumerate() {
        for &fi in &md.files {
            let src = &pkg.files[fi].raw.src;
            walk(&pkg.files[fi].parse.root, &mut |n| {
                if n.kind != SyntaxKind::DynType {
                    return;
                }
                let Some(p) = n.child_node(SyntaxKind::Path) else {
                    return;
                };
                let segs: Vec<(String, Span)> = p
                    .tokens()
                    .filter(|t| t.kind == SyntaxKind::Ident)
                    .map(|t| {
                        (
                            String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize])
                                .into_owned(),
                            t.span,
                        )
                    })
                    .collect();
                if !segs.is_empty() {
                    uses.push((m, fi, n.span, segs));
                }
            });
        }
    }
    for (m, fi, span, segs) in uses {
        let TypeHead::Item { module: tm, name } = lower.resolve_type_head(m, fi, &segs) else {
            continue;
        };
        let tref = TraitRef {
            module: tm,
            name: name.clone(),
        };
        let Some(td) = traits.get(&tref) else {
            let kind = lower.pkg.tables[tm]
                .get(&name)
                .map(|i| i.kind.keyword())
                .unwrap_or("name");
            lower.diags.push(
                Diagnostic::error(
                    codes::E0503,
                    span,
                    format!("`dyn` needs a trait, but `{name}` is a {kind}"),
                )
                .with_label("not a trait")
                .with_note(
                    "`dyn Trait` is wolf's only dynamic dispatch, and it dispatches \
                     through a trait's method set — a concrete type is just itself.",
                ),
            );
            continue;
        };
        for v in &td.dyn_report.violations {
            let d = match v.class {
                DynClass::GenericMethod => Diagnostic::error(
                    codes::E0508,
                    span,
                    format!(
                        "`{name}` cannot be a `dyn` object: its method `{}` has generic parameters",
                        v.method
                    ),
                )
                .with_label("`dyn` object created here")
                .with_secondary(v.span, "the generic method is declared here")
                .with_note(
                    "a witness table holds one entry per method, but a generic method \
                     would need one per instantiation (RFC 0255 model); drop the \
                     method's generics, split the trait, or use `T: Trait` generics.",
                ),
                DynClass::AssocEscape => Diagnostic::error(
                    codes::E0509,
                    span,
                    format!(
                        "`{name}` cannot be a `dyn` object: `{}`'s signature exposes an associated type",
                        v.method
                    ),
                )
                .with_label("`dyn` object created here")
                .with_secondary(v.span, "the associated type escapes here")
                .with_note(
                    "behind `dyn`, two objects may bind the associated type \
                     differently, so a signature exposing it has no single ABI; \
                     keep it out of the dynamic methods, or use `T: Trait` generics.",
                ),
                DynClass::InputParams => Diagnostic::error(
                    codes::E0509,
                    span,
                    format!(
                        "`{name}` cannot be a `dyn` object: the trait declares input parameters"
                    ),
                )
                .with_label("`dyn` object created here")
                .with_secondary(v.span, "the parameters are declared here")
                .with_note(
                    "a `dyn` spelling cannot apply trait inputs, so they would be \
                     unconstrained behind the object; use a trait without input \
                     parameters for dynamic dispatch.",
                ),
                DynClass::SelfPosition => Diagnostic::error(
                    codes::E0510,
                    span,
                    format!(
                        "`{name}` cannot be a `dyn` object: `{}` uses `Self` outside receiver position",
                        v.method
                    ),
                )
                .with_label("`dyn` object created here")
                .with_secondary(v.span, "`Self` escapes in this method's signature")
                .with_note(
                    "behind `dyn` the concrete type is erased, so only the receiver \
                     itself may be `Self` (RFC 0255); take another `dyn` object or a \
                     concrete type instead.",
                ),
            };
            lower.diags.push(d);
        }
    }
}

// ------------------------------------------------------ satisfaction ----

/// Does `ty` satisfy the trait `(tr_module, tr_name)`? Archetypes
/// (rigids) answer from `bounds` alone (the golden rule — no impl
/// search for generic bodies); concrete types answer by impl
/// matching with trial unification. Pure: every trial rolls back.
/// The recursion depth bounds blanket-impl chains.
#[allow(clippy::too_many_arguments)]
pub fn satisfies(
    sigs: &SigTables,
    table: &mut TypeTable,
    vars: &mut VarStore,
    bounds: &Bounds,
    ty: TyId,
    tr_module: usize,
    tr_name: &str,
    depth: u32,
) -> bool {
    let t = vars.shallow(table, ty);
    match table.kind(t).clone() {
        TyKind::Error | TyKind::Never => true,
        TyKind::Rigid(name) => bounds.get(&name).is_some_and(|bs| {
            bs.iter()
                .any(|b| b.module == tr_module && b.name == tr_name)
        }),
        // A `dyn Trait` object satisfies exactly its own trait
        // (witness semantics; supertraits do not exist in v1).
        TyKind::Dyn { module, name } => module as usize == tr_module && name == tr_name,
        TyKind::Var(_) => false,
        _ => {
            if depth == 0 {
                return false;
            }
            for (i, imp) in sigs.impls.iter().enumerate() {
                let matches = imp
                    .trait_ref
                    .as_ref()
                    .is_some_and(|r| r.module == tr_module && r.name == tr_name);
                if !matches {
                    continue;
                }
                let snap = vars.snapshot();
                let ok = impl_matches(sigs, table, vars, imp, t, depth);
                vars.rollback(snap);
                if ok {
                    let _ = i;
                    return true;
                }
            }
            false
        }
    }
}

/// One impl trial: instantiate the impl's generics as fresh
/// existentials, unify the self type, then require the impl's own
/// bounds recursively. Caller owns snapshot/rollback.
fn impl_matches(
    sigs: &SigTables,
    table: &mut TypeTable,
    vars: &mut VarStore,
    imp: &ImplDef,
    ty: TyId,
    depth: u32,
) -> bool {
    let mut map = BTreeMap::new();
    for g in &imp.generics {
        let v = vars.fresh(table, 0, NumKind::Any, g.span);
        map.insert(g.name.clone(), v);
    }
    let self_inst = subst(table, imp.self_ty, &map);
    if unify(table, vars, self_inst, ty).is_err() {
        return false;
    }
    for g in &imp.generics {
        let gv = map[&g.name];
        for b in &g.bounds {
            if !satisfies(
                sigs,
                table,
                vars,
                &Bounds::new(),
                gv,
                b.module,
                &b.name,
                depth - 1,
            ) {
                return false;
            }
        }
    }
    true
}

/// Resolve the projection `base.name` through the impl that covers
/// `base`, committing the (coherence-unique) match. `None` when no
/// impl binds it — the projection stays symbolic.
pub fn resolve_proj(
    sigs: &SigTables,
    table: &mut TypeTable,
    vars: &mut VarStore,
    base: TyId,
    name: &str,
) -> Option<TyId> {
    let b = vars.shallow(table, base);
    if matches!(
        table.kind(b),
        TyKind::Rigid(_) | TyKind::Var(_) | TyKind::Error
    ) {
        return None;
    }
    for imp in &sigs.impls {
        let Some(&bound) = imp.rewrites.get(name) else {
            continue;
        };
        let snap = vars.snapshot();
        let mut map = BTreeMap::new();
        for g in &imp.generics {
            let v = vars.fresh(table, 0, NumKind::Any, g.span);
            map.insert(g.name.clone(), v);
        }
        let self_inst = subst(table, imp.self_ty, &map);
        if unify(table, vars, self_inst, b).is_ok() {
            // Coherence makes the match unique — commit it.
            return Some(subst(table, bound, &map));
        }
        vars.rollback(snap);
    }
    None
}

/// Normalize every concrete-based projection inside `ty` through the
/// package's impls (the checker's post-instantiation step: inputs
/// are solved first, outputs then normalize — never the other way).
pub fn normalize_projections(
    sigs: &SigTables,
    table: &mut TypeTable,
    vars: &mut VarStore,
    ty: TyId,
) -> TyId {
    let t = vars.shallow(table, ty);
    match table.kind(t).clone() {
        TyKind::Proj(base, name) => {
            let nb = normalize_projections(sigs, table, vars, base);
            match resolve_proj(sigs, table, vars, nb, &name) {
                Some(res) => normalize_projections(sigs, table, vars, res),
                None => table.intern(TyKind::Proj(nb, name)),
            }
        }
        TyKind::Wrapping(x) => {
            let s = normalize_projections(sigs, table, vars, x);
            table.intern(TyKind::Wrapping(s))
        }
        TyKind::ErrUnion(x) => {
            let s = normalize_projections(sigs, table, vars, x);
            table.intern(TyKind::ErrUnion(s))
        }
        TyKind::Range(x) => {
            let s = normalize_projections(sigs, table, vars, x);
            table.intern(TyKind::Range(s))
        }
        TyKind::Tuple(ts) => {
            let s: Vec<TyId> = ts
                .into_iter()
                .map(|x| normalize_projections(sigs, table, vars, x))
                .collect();
            table.intern(TyKind::Tuple(s))
        }
        TyKind::Fn(ps, r) => {
            let sp: Vec<TyId> = ps
                .into_iter()
                .map(|x| normalize_projections(sigs, table, vars, x))
                .collect();
            let sr = normalize_projections(sigs, table, vars, r);
            table.intern(TyKind::Fn(sp, sr))
        }
        _ => t,
    }
}

fn src<'a>(lower: &Lower<'a>, file: usize) -> &'a [u8] {
    &lower.pkg.files[file].raw.src
}

fn text(lower: &Lower<'_>, file: usize, span: Span) -> String {
    String::from_utf8_lossy(&src(lower, file)[span.lo as usize..span.hi as usize]).into_owned()
}
