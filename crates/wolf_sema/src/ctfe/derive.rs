//! The derive-class exemplar (s16, Target 3): a comptime generator
//! producing an equality impl from reflection, whose output then goes
//! through **ordinary s14 checking** — comptime produces concrete
//! code, checked like any other code (the Zig-vs-wolf line: no
//! `anytype`, no post-instantiation surprises).
//!
//! **Staged diagnostics** (the I3 surface): when checking rejects
//! generated code, the error points at the generated output with the
//! *generator's provenance chain* attached — who generated it, invoked
//! from where — so the reader debugs the generator, not a ghost.
//!
//! Discipline (the D29 amendment): the generator consumes semantic
//! values (a type's fields via reflection) and produces semantic
//! values (an [`ImplDef`]) — at no point does any API observe or
//! construct syntax trees.

use wolf_diag::Diagnostic;
use wolf_span::Span;

use crate::graph::Package;
use crate::sig::{FnSig, ItemSig, ParamSig, SigTables};
use crate::traits::{ImplDef, ImplMethod, TraitRef, check_generated_impl};
use crate::types::{Prim, TyKind, TypeTable};

/// Who generated a piece of code, and from where — the provenance
/// chain staged diagnostics render.
#[derive(Clone, Debug)]
pub struct Provenance {
    pub generator: String,
    pub call_site: Span,
}

/// A synthesized impl plus the table its `TyId`s live in.
#[derive(Debug)]
pub struct Generated {
    pub impl_def: ImplDef,
    pub table: TypeTable,
}

/// Generate an equality impl for a struct via reflection: every field
/// must be a primitive (the equatable set today); the generated
/// method is `eq(a: Self, b: Self) -> bool`. Returns the impl as
/// semantic data — no syntax anywhere.
pub fn derive_eq(
    sigs: &SigTables,
    module: usize,
    type_name: &str,
    eq_trait: &TraitRef,
    prov: &Provenance,
) -> Result<Generated, Vec<Diagnostic>> {
    let mut table = sigs.table.clone();
    // Reflection step: the target's fields, as semantic data.
    let Some(ItemSig::Struct(s)) = sigs.get(module, type_name) else {
        return Err(vec![provenanced(
            Diagnostic::error(
                wolf_diag::codes::E0705,
                prov.call_site,
                format!("`{type_name}` is not a struct, so `derive_eq` cannot reflect its fields"),
            )
            .with_label("the generator refused before generating"),
            prov,
        )]);
    };
    for f in &s.fields {
        if !matches!(table.kind(f.ty), TyKind::Prim(_)) {
            return Err(vec![provenanced(
                Diagnostic::error(
                    wolf_diag::codes::E0705,
                    prov.call_site,
                    format!(
                        "the field `{}` of `{type_name}` is not primitive, and `==` \
                         beyond primitives waits for the operator traits (s17)",
                        f.name
                    ),
                )
                .with_label("the generator refused before generating"),
                prov,
            )]);
        }
    }
    let self_ty = table.intern(TyKind::Nominal {
        module: module as u32,
        name: type_name.to_string(),
    });
    let bool_ = table.prim(Prim::Bool);
    let sig = FnSig {
        params: vec![
            ParamSig {
                name: "a".to_string(),
                ty: self_ty,
                span: prov.call_site,
                mode: None,
                view: None,
            },
            ParamSig {
                name: "b".to_string(),
                ty: self_ty,
                span: prov.call_site,
                mode: None,
                view: None,
            },
        ],
        ret: bool_,
        name_span: prov.call_site,
        ret_span: None,
        row_span: None,
        generics: Vec::new(),
        comptime: false,
    };
    let impl_def = ImplDef {
        module,
        // Synthesized: ordinals past any real file's item list, so no
        // body-lookup collision with source impls is possible.
        file: usize::MAX,
        decl: usize::MAX,
        span: prov.call_site,
        generics: Vec::new(),
        trait_ref: Some(eq_trait.clone()),
        trait_args: Vec::new(),
        self_ty,
        methods: vec![ImplMethod {
            name: "eq".to_string(),
            sig,
            has_self: false,
            member: 0,
            name_span: prov.call_site,
            has_body: true,
        }],
        rewrites: Default::default(),
        consts: Vec::new(),
    };
    Ok(Generated { impl_def, table })
}

/// Run the generated impl through ordinary s14 conformance checking.
/// Every resulting diagnostic carries the generator's provenance
/// chain — this is the staged-diagnostics contract.
pub fn check_generated(
    pkg: &Package,
    sigs: &SigTables,
    generated: Generated,
    prov: &Provenance,
) -> Vec<Diagnostic> {
    check_generated_impl(pkg, sigs, generated.table, &generated.impl_def)
        .into_iter()
        .map(|d| provenanced(d, prov))
        .collect()
}

/// Attach the provenance chain to one diagnostic.
fn provenanced(d: Diagnostic, prov: &Provenance) -> Diagnostic {
    d.with_secondary(
        prov.call_site,
        format!("in code generated by `{}` — invoked here", prov.generator),
    )
    .with_note(format!(
        "the error is inside generated code: `{}` produced this impl at compile \
         time, and the generated output is checked like handwritten code (D29). \
         Fix the generator or its input — there is no source line to edit.",
        prov.generator
    ))
}
