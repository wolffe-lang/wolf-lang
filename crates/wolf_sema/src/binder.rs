//! Variant names in binder positions (`[gram.pat.binder]`): W0801's binder
//! half and the `select` arm's E0806.
//!
//! A binder pattern — a `let` or `var` binder, a `for` header, the `else
//! |…|` handler's binding form — binds every value it meets, so every bare
//! name in it is a new binding, at any depth. A bare name that spells a
//! variant of an enum in scope, in either letter case, still binds, but it
//! reads as the test it is not: W0801, naming the variant, with two fixes (a
//! name that spells no variant, the same binding; the qualified variant, a
//! test a binder then refuses). A `select` arm's receive takes whatever its
//! channel delivers, so the same name there is refused by name (E0806).
//!
//! The letter case does not enter, because it does not where a `match`
//! decides the same question: the nested-payload rule (`names_sema_case` in
//! `wolf_wir`'s lowering, wolf-lang#1043) and the checker's #4 rule read a
//! bare name as a test whenever it spells a case, `fast` of `enum Mode { fast,
//! slow }` exactly as `Drink`. A name that would test there warns here.
//!
//! "In scope" is the set `[gram.expr.variant]` already consults: the enums
//! of the binder's module, then the enums its file imports by item. Wolf has
//! no builtin enum, so the prelude adds none. Scope decides, not type: the
//! rule needs resolution alone, which is what lets the reference interpreter
//! run the same analysis at the same rung. The resolver calls in here; the
//! rename fix reads its binding table, so the reports are made when a
//! file's walk is complete.

use std::collections::BTreeSet;

use wolf_ast::{EnumDecl, EnumVariant, GreenNode, SyntaxKind, TypeDecl};
use wolf_diag::{Applicability, Diagnostic, Suggestion, codes};
use wolf_span::Span;

use crate::graph::{BindTarget, Binding, Item, Package};
use crate::prelude;

/// One enum that declares the variant a binder name spells.
#[derive(Clone, Debug)]
pub(crate) struct VariantHome {
    /// The enum's spelling in this file (an aliased import's alias).
    pub enum_name: String,
    /// The variant's declaration name token.
    pub decl: Span,
    /// How many values the variant carries.
    pub arity: usize,
}

/// Where in a binder pattern a name stands — it decides the edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Site {
    /// A plain sub-pattern: `Drink`, `(Drink, n)`, `P { k: Drink }`.
    Plain,
    /// A struct pattern's shorthand `P { Drink }`, which binds the field's
    /// own name: an edit there spells the field out (`Drink: drink`).
    Shorthand,
    /// The name before `@`, where no pattern can stand: the rename alone.
    AtName,
    /// A `select` arm's receive pattern (E0806, not W0801).
    SelectArm,
}

/// A binder name that spells an in-scope variant, held until the file's
/// binding table is complete.
#[derive(Clone, Debug)]
pub(crate) struct BinderVariant {
    pub name: String,
    pub span: Span,
    pub site: Site,
    pub homes: Vec<VariantHome>,
    /// The `select` arm's channel expression, as written (its note's
    /// worked example).
    pub channel: Option<String>,
}

/// Every name a binder pattern binds, with its site, through the shapes a
/// binder admits: tuples, struct fields, both sides of `@`. A refutable
/// shape (a path, an or-pattern, a literal, a range) is E0806's already and
/// is not walked.
pub(crate) fn binder_names(pat: &GreenNode, out: &mut Vec<(Span, Site)>) {
    match pat.kind {
        SyntaxKind::IdentPat => {
            if let Some(t) = pat.child_token(SyntaxKind::Ident) {
                out.push((t.span, Site::Plain));
            }
        }
        SyntaxKind::BindingPat => {
            if let Some(t) = pat.child_token(SyntaxKind::Ident) {
                out.push((t.span, Site::AtName));
            }
            for sub in pat.nodes().filter(|n| wolf_ast::is_pattern_kind(n.kind)) {
                binder_names(sub, out);
            }
        }
        SyntaxKind::TuplePat => {
            for sub in pat.nodes().filter(|n| wolf_ast::is_pattern_kind(n.kind)) {
                binder_names(sub, out);
            }
        }
        SyntaxKind::StructPat => {
            for field in pat.nodes().filter_map(wolf_ast::FieldPat::cast) {
                let Some(sub) = field.pattern() else { continue };
                if field.name().is_none() && sub.kind == SyntaxKind::IdentPat {
                    if let Some(t) = sub.child_token(SyntaxKind::Ident) {
                        out.push((t.span, Site::Shorthand));
                    }
                } else {
                    binder_names(sub, out);
                }
            }
        }
        _ => {}
    }
}

/// The variants an enum item declares, whichever form declared it
/// (`enum K { … }` or `type K = enum { … }`); `None` for any other item.
fn enum_variants<'p>(pkg: &'p Package, item: &Item) -> Option<Vec<EnumVariant<'p>>> {
    let node = crate::sig::item_node(pkg, item);
    match node.kind {
        SyntaxKind::EnumDecl => EnumDecl::cast(node).map(|d| d.variants().collect()),
        SyntaxKind::TypeDecl => TypeDecl::cast(node)
            .and_then(|d| d.def())
            .filter(|def| def.kind == SyntaxKind::EnumDef)
            .map(|def| def.nodes().filter_map(EnumVariant::cast).collect()),
        _ => None,
    }
}

fn home_in(pkg: &Package, item: &Item, enum_name: &str, name: &str) -> Option<VariantHome> {
    let src = &pkg.files[item.file].raw.src;
    enum_variants(pkg, item)?.into_iter().find_map(|v| {
        let t = v.name()?;
        (&src[t.span.lo as usize..t.span.hi as usize] == name.as_bytes()).then(|| VariantHome {
            enum_name: enum_name.to_string(),
            decl: t.span,
            arity: v.payload().count(),
        })
    })
}

/// The enums this file names directly that declare a variant `name`: the
/// module's own enums in name order, then the file's item imports in
/// import order — the checker's `variant_homes` order, so both say the same
/// enum first.
pub(crate) fn variant_homes(
    pkg: &Package,
    module: usize,
    bindings: &[Binding],
    name: &str,
) -> Vec<VariantHome> {
    let mut local: Vec<&Item> = pkg.tables[module].items.iter().collect();
    local.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out: Vec<VariantHome> = local
        .into_iter()
        .filter_map(|item| home_in(pkg, item, &item.name, name))
        .collect();
    for b in bindings {
        let BindTarget::Item {
            module: home,
            name: item_name,
        } = &b.target
        else {
            continue;
        };
        if out.iter().any(|h| h.enum_name == b.name) {
            continue;
        }
        if let Some(item) = pkg.tables[*home].get(item_name)
            && let Some(h) = home_in(pkg, item, &b.name, name)
        {
            out.push(h);
        }
    }
    out
}

/// The snake-case spelling of a name: `Drink` → `drink`, `fast` → `fast`,
/// `HotDrink` → `hot_drink`, `HTTPCode` → `http_code`.
pub(crate) fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1).copied();
            let boundary = prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit())
                || (prev.is_some_and(char::is_uppercase) && next.is_some_and(char::is_lowercase));
            if boundary && !out.ends_with('_') {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Can `name` stand as a binder at all? The parser answers: a keyword,
/// reserved or contextual, cannot. The probe is parsed and thrown away.
fn parses_as_binder(file: wolf_span::FileId, name: &str) -> bool {
    let src = format!("let {name} = 0\n");
    let parse = wolf_parse::parse_file(file, src.as_bytes());
    if !parse.diagnostics.is_empty() {
        return false;
    }
    let Some(decl) = parse.root.nodes().find(|n| n.kind == SyntaxKind::LetDecl) else {
        return false;
    };
    let Some(pat) = wolf_ast::binding_binders(decl)
        .into_iter()
        .next()
        .and_then(|b| b.pattern)
    else {
        return false;
    };
    pat.kind == SyntaxKind::IdentPat
        && pat.child_token(SyntaxKind::Ident).is_some_and(|t| {
            &src.as_bytes()[t.span.lo as usize..t.span.hi as usize] == name.as_bytes()
        })
}

/// Every identifier the file spells, anywhere.
pub(crate) fn spelled_names(root: &GreenNode, src: &[u8]) -> BTreeSet<String> {
    fn walk(n: &GreenNode, src: &[u8], out: &mut BTreeSet<String>) {
        for t in n.tokens() {
            if t.kind == SyntaxKind::Ident {
                out.insert(
                    String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize])
                        .into_owned(),
                );
            }
        }
        for c in n.nodes() {
            walk(c, src, out);
        }
    }
    let mut out = BTreeSet::new();
    walk(root, src, &mut out);
    out
}

/// The binding name the rename fix writes: the snake-case spelling, with
/// `_` appended while it would collide — a keyword, a prelude or builtin
/// type name, a module item, an import of this file, a variant in scope
/// (the new name must not warn again), or any identifier the file already
/// spells. A lowercase variant's own name is spelled in the file, so it
/// always takes the `_`: `fast` → `fast_`. A name nothing in the file spells
/// can neither capture a use nor shadow a binding, so the rename keeps every
/// use resolving to the binding it resolved to before.
pub(crate) fn fresh_name(
    pkg: &Package,
    module: usize,
    bindings: &[Binding],
    spelled: &BTreeSet<String>,
    file: wolf_span::FileId,
    name: &str,
) -> String {
    let mut cand = snake_case(name);
    loop {
        let clash = spelled.contains(&cand)
            || pkg.tables[module].get(&cand).is_some()
            || bindings.iter().any(|b| b.name == cand)
            || prelude::in_prelude(&cand)
            || prelude::is_builtin_type(&cand)
            || cand == "self"
            || !variant_homes(pkg, module, bindings, &cand).is_empty()
            || !parses_as_binder(file, &cand);
        if !clash {
            return cand;
        }
        cand.push('_');
    }
}

/// `Kind.Drink`, or `Kind.Some(_)` for a variant that carries values.
fn qualified(home: &VariantHome, name: &str) -> String {
    if home.arity == 0 {
        format!("{}.{name}", home.enum_name)
    } else {
        format!(
            "{}.{name}({})",
            home.enum_name,
            vec!["_"; home.arity].join(", ")
        )
    }
}

/// The report for one binder name: W0801 at a binder, E0806 at a `select`
/// arm. `rename` is the rename fix's edits (the binder and every use).
pub(crate) fn report(b: &BinderVariant, new: &str, rename: Vec<(Span, String)>) -> Diagnostic {
    let home = &b.homes[0];
    let name = &b.name;
    let en = &home.enum_name;
    let mut d = if b.site == Site::SelectArm {
        let ch = b.channel.as_deref().unwrap_or("ch");
        Diagnostic::error(
            codes::E0806,
            b.span,
            format!("`{name}` is the variant `{en}.{name}`, and a `select` arm cannot test it"),
        )
        .with_label("a receive cannot fail to match")
        .with_secondary(home.decl, format!("`{name}` is declared here, in `{en}`"))
        .with_note(format!(
            "a committed receive takes whatever its channel delivers, so a `select` arm's \
             pattern only binds. Receive into a name and test it in the arm's body: \
             `{new} from {ch} => match {new} {{ {en}.{name} => …, _ => … }}`."
        ))
    } else {
        Diagnostic::warning(
            codes::W0801,
            b.span,
            format!("`{name}` binds a new name here — it does not test the variant `{en}.{name}`"),
        )
        .with_label("a binding, not a test")
        .with_secondary(home.decl, format!("`{name}` is declared here, in `{en}`"))
        .with_note(format!(
            "a `let`, `var`, `for` or `else |…|` pattern binds every value it meets, so a \
             bare name there is a new binding even when it spells a variant. To test the \
             value, `match` on it (`{en}.{name} => …`); to keep the binding, give it a \
             name that spells no variant."
        ))
    };
    if b.homes.len() > 1 {
        let others: Vec<String> = b.homes[1..]
            .iter()
            .map(|h| format!("`{}`", h.enum_name))
            .collect();
        d = d.with_note(format!(
            "`{name}` is also a variant of {} here.",
            others.join(", ")
        ));
    }
    d = d.with_suggestion(Suggestion::new(
        format!("bind it under another name: `{new}`"),
        rename,
        Applicability::MachineApplicable,
    ));
    if matches!(b.site, Site::Plain | Site::Shorthand) {
        let applicability = if b.homes.len() == 1 {
            Applicability::MachineApplicable
        } else {
            Applicability::Maybe
        };
        for h in &b.homes {
            let q = qualified(h, name);
            let edit = if b.site == Site::Shorthand {
                format!("{name}: {q}")
            } else {
                q.clone()
            };
            d = d.with_suggestion(Suggestion::new(
                format!("test the variant instead: `{q}`"),
                vec![(b.span, edit)],
                applicability,
            ));
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::snake_case;

    #[test]
    fn snake_case_splits_words_and_acronyms() {
        assert_eq!(snake_case("Drink"), "drink");
        assert_eq!(snake_case("HotDrink"), "hot_drink");
        assert_eq!(snake_case("HTTPCode"), "http_code");
        assert_eq!(snake_case("A"), "a");
        assert_eq!(snake_case("Stage2Done"), "stage2_done");
    }
}
