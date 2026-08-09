//! Declaration-structure dump: one line per item (kind + name + span,
//! bodies shown as `Block`, initializers by their expression kind),
//! members of `trait`/`impl` indented. The corpus snapshot format
//! (s08, fences opened in s09).

use crate::ast::{
    ConstDecl, EnumDecl, FnDecl, ImplDecl, ImportCDecl, LetDecl, StructDecl, TraitDecl, TypeDecl,
    UseDecl, VarDecl,
};
use crate::green::GreenNode;
use crate::kind::SyntaxKind;
use std::fmt::Write;
use wolf_span::Span;

fn text(src: &[u8], span: Span) -> String {
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
}

fn preview(src: &[u8], span: Span) -> String {
    let t: String = text(src, span).chars().take(40).collect();
    let esc: String = t.escape_debug().collect();
    let cut = span.len() > 40;
    format!("\"{esc}{}\"", if cut { "…" } else { "" })
}

/// Render the declaration structure of a parsed file.
pub fn dump_decls(root: &GreenNode, src: &[u8]) -> String {
    let mut out = String::new();
    for node in root.nodes() {
        dump_item(node, src, 0, &mut out);
    }
    out
}

fn dump_item(node: &GreenNode, src: &[u8], depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let head = format!("{pad}{:?} {}..{}", node.kind, node.span.lo, node.span.hi);
    let name_of = |tok: Option<&crate::green::GreenToken>| -> String {
        tok.map_or("<missing>".to_string(), |t| text(src, t.span))
    };
    let mut flags = String::new();
    if let Some(v) = node.nodes().find_map(crate::ast::Visibility::cast) {
        flags.push_str(if v.is_pkg() { " pub(pkg)" } else { " pub" });
    }
    let attrs = node
        .nodes()
        .filter(|n| n.kind == SyntaxKind::Attribute)
        .count();
    if attrs > 0 {
        let _ = write!(flags, " attrs={attrs}");
    }

    match node.kind {
        SyntaxKind::FnDecl => {
            let f = FnDecl::cast(node).expect("kind checked");
            let mut quals = String::new();
            if f.is_comptime() {
                quals.push_str(" comptime");
            }
            if f.extern_abi().is_some() || node.child_token(SyntaxKind::ExternKw).is_some() {
                quals.push_str(" extern");
            }
            if f.is_export() {
                quals.push_str(" export");
            }
            let body = match f.body() {
                Some(_) => " body=Block",
                None => "",
            };
            let _ = writeln!(out, "{head} name={}{flags}{quals}{body}", name_of(f.name()));
        }
        SyntaxKind::UseDecl => {
            let u = UseDecl::cast(node).expect("kind checked");
            let path = u
                .path()
                .map_or("<missing>".to_string(), |p| text(src, p.syntax().span));
            let mut extra = String::new();
            if let Some(g) = u.group() {
                let names: Vec<String> = g.names().map(|t| text(src, t.span)).collect();
                let _ = write!(extra, " group={{{}}}", names.join(","));
            }
            if let Some(a) = u.alias() {
                let _ = write!(extra, " as={}", text(src, a.span));
            }
            let _ = writeln!(out, "{head} path={path}{flags}{extra}");
        }
        SyntaxKind::ImportCDecl => {
            let i = ImportCDecl::cast(node).expect("kind checked");
            let header = i
                .header()
                .map_or("<missing>".to_string(), |s| text(src, s.syntax().span));
            let _ = writeln!(out, "{head} header={header}{flags}");
        }
        SyntaxKind::TypeDecl => {
            let t = TypeDecl::cast(node).expect("kind checked");
            let def = t
                .def()
                .map_or("<missing>".to_string(), |d| format!("{:?}", d.kind));
            let _ = writeln!(out, "{head} name={}{flags} def={def}", name_of(t.name()));
        }
        SyntaxKind::StructDecl => {
            let s = StructDecl::cast(node).expect("kind checked");
            let _ = writeln!(
                out,
                "{head} name={}{flags} fields={}",
                name_of(s.name()),
                s.fields().count()
            );
        }
        SyntaxKind::EnumDecl => {
            let e = EnumDecl::cast(node).expect("kind checked");
            let _ = writeln!(
                out,
                "{head} name={}{flags} variants={}",
                name_of(e.name()),
                e.variants().count()
            );
        }
        SyntaxKind::TraitDecl => {
            let t = TraitDecl::cast(node).expect("kind checked");
            let _ = writeln!(out, "{head} name={}{flags}", name_of(t.name()));
            for m in t.members() {
                dump_item(m, src, depth + 1, out);
            }
        }
        SyntaxKind::ImplDecl => {
            let i = ImplDecl::cast(node).expect("kind checked");
            let path = i
                .trait_path()
                .map_or("<missing>".to_string(), |p| text(src, p.syntax().span));
            let for_ty = i
                .self_ty()
                .map_or(String::new(), |t| format!(" for={}", text(src, t.span)));
            let _ = writeln!(out, "{head} path={path}{for_ty}{flags}");
            for m in i.members() {
                dump_item(m, src, depth + 1, out);
            }
        }
        SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
            let (pat, init) = if node.kind == SyntaxKind::LetDecl {
                let l = LetDecl::cast(node).expect("kind checked");
                (l.pattern(), l.init())
            } else {
                let v = VarDecl::cast(node).expect("kind checked");
                (v.pattern(), v.init())
            };
            let pat = pat.map_or("<missing>".to_string(), |p| text(src, p.span));
            let init = init.map_or(String::new(), |i| format!(" init={:?}", i.kind));
            let _ = writeln!(out, "{head} pat={pat}{flags}{init}");
        }
        SyntaxKind::ConstDecl => {
            let c = ConstDecl::cast(node).expect("kind checked");
            let init = c
                .init()
                .map_or(String::new(), |i| format!(" init={:?}", i.kind));
            let _ = writeln!(out, "{head} name={}{flags}{init}", name_of(c.name()));
        }
        SyntaxKind::ErrorNode => {
            let _ = writeln!(out, "{head} {}", preview(src, node.span));
        }
        // Non-item nodes (attributes/visibility of a parent, etc.) are
        // rendered by their item; anything else names its kind.
        _ => {
            if node.kind.is_item() || depth == 0 {
                let _ = writeln!(out, "{head}");
            }
        }
    }
}
