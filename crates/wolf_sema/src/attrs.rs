//! The attribute set is closed (K13, STATUS #31 — wolf-lang#519) and
//! the `extern` ABI string is `"c"` or nothing (K7 — wolf-lang#524).
//!
//! `[gram.item.attr]` says attributes are "closed, structured — not
//! token soup", but until kw01 nothing enforced the closure: an
//! attribute no pass read (`#[frobnicate]`, `#[repr(packed)]`,
//! `#[section(…)]`, `#[noalloc]` over an allocating function) compiled
//! on every machine and meant nothing, and `#[cfg(target = …)]` was
//! ignored, so target-gated code was never gated. Two passes close it:
//!
//! - [`strip_cfg`] runs on each file's tree right after parsing,
//!   before any item is collected: a node whose `#[cfg(target = "S")]`
//!   does not hold for the build's target is dropped, so resolution,
//!   typing, the checked machine and both lowerings never see it
//!   (`[gram.item.attr.cfg]`). A predicate this compiler cannot decide
//!   — an unknown target string, another key, several predicates —
//!   KEEPS its node (one mistake is one diagnostic, never a cascade of
//!   unresolved names at its callers), and [`check`] refuses it.
//! - [`check`] walks every surviving attribute: each must be one this
//!   compiler implements, in a position where it means something.
//!   Everything else is **E0817**, naming the attribute and what exists
//!   instead — the known-but-unimplemented ones by the lane that owns
//!   them, so the refusal says "not yet", not "never". The same walk
//!   refuses every `extern "S"` with `S != "c"` (**E0818**).
//!
//! Inner (`#![…]`) attributes are the origin scan's (E0813), not ours.

use wolf_ast::{Child, FnDecl, GreenNode, SyntaxKind, is_stmt_kind};
use wolf_diag::{Diagnostic, codes};
use wolf_span::Span;

use crate::graph::Package;

/// The target triples a `cfg(target = "…")` predicate may name: the
/// hosted matrix `[abi.c.targets]` names, and the freestanding target
/// K1 rules (`x86_64-unknown-none`, no build selects it until kw04).
pub const KNOWN_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-freebsd",
    "x86_64-unknown-none",
];

/// The architectures — a triple's first component — a predicate may
/// name instead of a whole triple.
pub const KNOWN_ARCHES: &[&str] = &["x86_64", "aarch64"];

/// The attributes this compiler implements, for the E0817 note.
const IMPLEMENTED: &str = "`trusted`, `consttime`, `allow`, `index`, `budget`, `repr(c)` and \
                           `cfg(target = \"…\")`";

/// The build's target triple. Until `--target` exists (K1, kw04) the
/// build's target is the host the compiler runs on — the same answer
/// for the checked machine, the native tier and the release tier.
pub fn host_target() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux" if cfg!(target_env = "musl") => format!("{arch}-unknown-linux-musl"),
        "linux" => format!("{arch}-unknown-linux-gnu"),
        "macos" => format!("{arch}-apple-darwin"),
        "windows" if cfg!(target_env = "gnu") => format!("{arch}-pc-windows-gnu"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        os => format!("{arch}-unknown-{os}"),
    }
}

/// Does `cfg(target = "s")` hold when building for `target`? `s`
/// names the whole triple or its architecture.
pub fn target_matches(s: &str, target: &str) -> bool {
    s == target || target.split('-').next() == Some(s)
}

/// Is `s` a string a `target` predicate may carry at all?
pub fn target_known(s: &str, target: &str) -> bool {
    KNOWN_TARGETS.contains(&s) || KNOWN_ARCHES.contains(&s) || s == target
}

fn text(src: &[u8], span: Span) -> String {
    String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize])
        .trim()
        .to_string()
}

/// The attribute item's name as spelled (`repr`, `pkg.name`).
fn item_name(item: &wolf_ast::AttrItem<'_>, src: &[u8]) -> String {
    item.path()
        .map(|p| text(src, p.syntax().span))
        .unwrap_or_default()
}

/// The nested `attr_arg` items of an item's `(…)` input (empty for a
/// bare item or the `= literal` form).
fn args<'a>(item: &wolf_ast::AttrItem<'a>) -> Vec<wolf_ast::AttrItem<'a>> {
    item.input()
        .filter(|inp| inp.child_token(SyntaxKind::LParen).is_some())
        .map(|inp| inp.nodes().filter_map(wolf_ast::AttrItem::cast).collect())
        .unwrap_or_default()
}

/// A string literal's contents, quotes removed (the attribute and ABI
/// strings this module reads are plain ASCII; an escape or a hole
/// simply fails to name anything).
fn string_value(node: &GreenNode, src: &[u8]) -> String {
    let raw = text(src, node.span);
    raw.strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(&raw)
        .to_string()
}

fn e0817(span: Span, message: String, label: &str, note: String) -> Diagnostic {
    Diagnostic::error(codes::E0817, span, message)
        .with_label(label.to_string())
        .with_note(note)
}

// ------------------------------------------------------------- cfg ----

/// What one `cfg(…)` item says about its node.
enum CfgVerdict {
    Holds,
    Fails,
    /// Refused (E0817 already pushed): the node is kept.
    Refused,
}

fn cfg_item(
    item: &wolf_ast::AttrItem<'_>,
    src: &[u8],
    target: &str,
    diags: &mut Vec<Diagnostic>,
) -> CfgVerdict {
    let span = item.syntax().span;
    let preds = args(item);
    // The notes name no host: a diagnostic's text is one truth on
    // every host (the fixtures are snapshots).
    let shape_note = || {
        "the one predicate is `cfg(target = \"<triple or architecture>\")` \
         ([gram.item.attr.cfg])."
            .to_string()
    };
    if preds.len() != 1 {
        diags.push(e0817(
            span,
            "`cfg` takes exactly one predicate, `target = \"…\"`".to_string(),
            "not a predicate this compiler decides",
            shape_note(),
        ));
        return CfgVerdict::Refused;
    }
    let pred = preds[0];
    let key = item_name(&pred, src);
    let value = pred
        .input()
        .filter(|inp| inp.child_token(SyntaxKind::Eq).is_some())
        .and_then(|inp| inp.nodes().find(|n| n.kind == SyntaxKind::StringLit))
        .map(|n| string_value(n, src));
    let (true, Some(value)) = (key == "target", value) else {
        diags.push(e0817(
            pred.syntax().span,
            format!("`cfg({})` is not a predicate wolf knows", text(src, pred.syntax().span)),
            "not a predicate this compiler decides",
            shape_note(),
        ));
        return CfgVerdict::Refused;
    };
    if !target_known(&value, target) {
        diags.push(e0817(
            pred.syntax().span,
            format!("`{value}` names no target wolf knows"),
            "no such target",
            format!(
                "a `target` predicate names a triple ({}) or an architecture ({}); a name \
                 that matches nothing is refused, never read as \"false\", so a typo cannot \
                 silently drop code.",
                KNOWN_TARGETS
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                KNOWN_ARCHES
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ));
        return CfgVerdict::Refused;
    }
    if target_matches(&value, target) {
        CfgVerdict::Holds
    } else {
        CfgVerdict::Fails
    }
}

/// Keep `node`? Every `cfg` on it must hold (several are a
/// conjunction); a refused predicate keeps the node.
fn cfg_keeps(node: &GreenNode, src: &[u8], target: &str, diags: &mut Vec<Diagnostic>) -> bool {
    let mut keep = true;
    for attr in node.nodes().filter_map(wolf_ast::Attribute::cast) {
        for item in attr.items() {
            if item_name(&item, src) != "cfg" {
                continue;
            }
            match cfg_item(&item, src, target, diags) {
                CfgVerdict::Holds | CfgVerdict::Refused => {}
                CfgVerdict::Fails => keep = false,
            }
        }
    }
    keep
}

/// Drop every node whose `#[cfg(target = …)]` does not hold for
/// `target` (`[gram.item.attr.cfg]`). Runs on a freshly parsed tree,
/// before items are collected; a dropped node's contents are never
/// resolved, typed or lowered. A predicate it cannot decide keeps its
/// node; [`check`] reports it (E0817), at the same phase as every
/// other attribute refusal.
pub fn strip_cfg(root: &mut GreenNode, src: &[u8], target: &str) {
    node_strip(root, src, target);
}

fn node_strip(node: &mut GreenNode, src: &[u8], target: &str) {
    let mut quiet = Vec::new();
    node.children.retain(|c| match c {
        Child::Node(n) => cfg_keeps(n, src, target, &mut quiet),
        Child::Token(_) => true,
    });
    for c in node.children.iter_mut() {
        if let Child::Node(n) = c {
            node_strip(n, src, target);
        }
    }
}

// ----------------------------------------------------------- check ----

/// Where an attribute was written, for the position rules.
fn is_fn(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::FnDecl
}

/// One attribute item on a node of `kind`: implemented and in place,
/// or one E0817.
fn check_item(
    item: &wolf_ast::AttrItem<'_>,
    kind: SyntaxKind,
    src: &[u8],
    target: &str,
    diags: &mut Vec<Diagnostic>,
) {
    let name = item_name(item, src);
    let span = item.syntax().span;
    let misplaced = |diags: &mut Vec<Diagnostic>, wants: &str| {
        diags.push(e0817(
            span,
            format!("`{name}` means nothing here — it applies to {wants}"),
            "nothing reads this attribute in this position",
            format!(
                "an attribute in a place no pass reads would be accepted and ignored, so it \
                 is refused instead ([gram.item.attr]); move it to {wants}, or delete it."
            ),
        ));
    };
    let not_yet = |diags: &mut Vec<Diagnostic>, what: String, owner: &str| {
        diags.push(e0817(
            span,
            format!("`{what}` is not implemented yet"),
            "known, but nothing implements it",
            format!(
                "an attribute nothing implements would compile and mean nothing, so it is \
                 refused by name ([gram.item.attr], K13); {owner}. The attributes this \
                 compiler implements: {IMPLEMENTED}."
            ),
        ));
    };
    match name.as_str() {
        // Read anywhere an attribute may be written.
        "allow" | "index" => {}
        // Every `cfg` still in the tree held or could not be decided
        // (the strip dropped the rest); the undecidable ones are
        // refused here.
        "cfg" => {
            cfg_item(item, src, target, diags);
        }
        "trusted" | "consttime" => {
            if !is_fn(kind) {
                misplaced(diags, "a function");
            }
        }
        "budget" => {
            if !is_stmt_kind(kind) {
                misplaced(diags, "a statement or an item holding a comptime evaluation");
            }
        }
        "repr" => check_repr(item, kind, src, diags),
        "noalloc" | "nopanic" | "inplace" | "bounded_stack" => not_yet(
            diags,
            name.clone(),
            "the performance contracts (I15) have no checker yet (wolf-lang#180), and an \
             unchecked promise is worse than none",
        ),
        "thread_local" => not_yet(
            diags,
            name.clone(),
            "thread-local storage is ruled (STATUS #32 R4) and waits on its own lane",
        ),
        "section" | "link_section" => not_yet(
            diags,
            name.clone(),
            "section placement is `#[section(\".x\")]` under KWC's K6 (kw09)",
        ),
        _ => diags.push(e0817(
            span,
            format!("`{name}` is not an attribute wolf knows"),
            "unknown attribute",
            format!(
                "the attribute set is closed ([gram.item.attr]): {IMPLEMENTED}. A name \
                 outside it is refused, never ignored."
            ),
        )),
    }
}

/// `#[repr(c)]` on a struct is the one representation implemented
/// (`[abi.layout.c]`); `packed`, `align(N)` and `transparent` are
/// KWC F4's later half (kw08).
fn check_repr(
    item: &wolf_ast::AttrItem<'_>,
    kind: SyntaxKind,
    src: &[u8],
    diags: &mut Vec<Diagnostic>,
) {
    let span = item.syntax().span;
    let reprs = args(item);
    if reprs.is_empty() {
        diags.push(e0817(
            span,
            "`repr` needs a representation: `repr(c)`".to_string(),
            "no representation named",
            "the one representation implemented is `#[repr(c)]` on a struct \
             ([abi.layout.c])."
                .to_string(),
        ));
        return;
    }
    if kind != SyntaxKind::StructDecl {
        diags.push(e0817(
            span,
            "`repr` means nothing here — it applies to a struct".to_string(),
            "nothing reads this attribute in this position",
            "`#[repr(c)]` lays a struct out at the C layout ([abi.layout.c]); on anything \
             else it would be accepted and ignored, so it is refused instead."
                .to_string(),
        ));
        return;
    }
    for r in reprs {
        let rname = item_name(&r, src);
        let rspan = r.syntax().span;
        if rname == "c" && r.input().is_none() {
            continue;
        }
        let (message, label) = match rname.as_str() {
            "packed" | "align" | "transparent" => (
                format!("`repr({})` is not implemented yet", text(src, rspan)),
                "known, but nothing implements it",
            ),
            _ => (
                format!("`repr({})` is not a representation wolf knows", text(src, rspan)),
                "unknown representation",
            ),
        };
        diags.push(e0817(
            rspan,
            message,
            label,
            "the one representation implemented is `#[repr(c)]` ([abi.layout.c]); \
             `packed`, `align(N)` and `transparent` are KWC F4's later half (kw08) and are \
             refused by name until then, never ignored."
                .to_string(),
        ));
    }
}

/// E0818: an `extern "S"` whose ABI string is not `"c"`.
fn check_abi(node: &GreenNode, src: &[u8], diags: &mut Vec<Diagnostic>) {
    let Some(d) = FnDecl::cast(node) else { return };
    let Some(abi) = d.extern_abi() else { return };
    let value = string_value(abi.syntax(), src);
    if value == "c" {
        return;
    }
    diags.push(
        Diagnostic::error(
            codes::E0818,
            abi.syntax().span,
            format!("`\"{value}\"` is not an ABI wolf has — the only one is `\"c\"`"),
        )
        .with_label("unknown ABI")
        .with_note(
            "`[abi.c.seams]`: the one ABI string is `\"c\"`. An interrupt or exception \
             handler is entered through an assembly trampoline that saves the registers, \
             calls an `export fn` with the C convention, and returns with `iretq` (KWC F7, \
             K7); a function written with another ABI string would compile as an ordinary \
             function and return with `ret`."
                .to_string(),
        ),
    );
}

fn walk(node: &GreenNode, src: &[u8], target: &str, diags: &mut Vec<Diagnostic>) {
    for attr in node.nodes().filter_map(wolf_ast::Attribute::cast) {
        for item in attr.items() {
            check_item(&item, node.kind, src, target, diags);
        }
    }
    check_abi(node, src, diags);
    for child in node.nodes() {
        walk(child, src, target, diags);
    }
}

/// E0817 and E0818 over every file of `pkg` (after [`strip_cfg`]).
pub fn check(pkg: &Package) -> Vec<Diagnostic> {
    let target = host_target();
    let mut diags = Vec::new();
    for unit in &pkg.files {
        walk(&unit.parse.root, &unit.raw.src, &target, &mut diags);
    }
    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_predicate_names_a_triple_or_its_architecture() {
        let t = "x86_64-unknown-linux-gnu";
        assert!(target_matches("x86_64-unknown-linux-gnu", t));
        assert!(target_matches("x86_64", t));
        assert!(!target_matches("aarch64", t));
        assert!(!target_matches("x86_64-unknown-none", t));
        assert!(!target_matches("x86", t));
        assert!(target_matches("aarch64", "aarch64-apple-darwin"));
    }

    #[test]
    fn an_unknown_string_is_not_a_target() {
        let t = "x86_64-unknown-linux-gnu";
        assert!(!target_known("no-such-target", t));
        assert!(!target_known("x86-64", t));
        assert!(!target_known("linux", t));
        assert!(target_known("x86_64-unknown-none", t));
        assert!(target_known("aarch64", t));
        // The host is always a target, even off the matrix.
        assert!(target_known("riscv64-unknown-linux-gnu", "riscv64-unknown-linux-gnu"));
    }

    #[test]
    fn the_host_is_a_known_target_or_names_itself() {
        let h = host_target();
        assert!(target_known(&h, &h));
        assert!(h.contains('-'), "{h}");
    }

    /// The stripped tree's text, and what the check then refuses.
    fn stripped(src: &str, target: &str) -> (String, Vec<Diagnostic>) {
        let parse = wolf_parse::parse_file(wolf_span::FileId::from_index(0), src.as_bytes());
        let mut root = parse.root;
        strip_cfg(&mut root, src.as_bytes(), target);
        let mut diags = Vec::new();
        walk(&root, src.as_bytes(), target, &mut diags);
        (String::from_utf8(root.text(src.as_bytes())).unwrap(), diags)
    }

    #[test]
    fn a_failing_cfg_drops_its_item_and_a_holding_one_keeps_it() {
        let src = "#[cfg(target = \"x86_64\")]\nfn a() -> int { 1 }\n\
                   #[cfg(target = \"aarch64\")]\nfn a() -> int { 2 }\n";
        let (x, d) = stripped(src, "x86_64-unknown-linux-gnu");
        assert!(d.is_empty());
        assert!(x.contains("{ 1 }") && !x.contains("{ 2 }"), "{x}");
        let (a, d) = stripped(src, "aarch64-apple-darwin");
        assert!(d.is_empty());
        assert!(a.contains("{ 2 }") && !a.contains("{ 1 }"), "{a}");
    }

    #[test]
    fn a_failing_cfg_drops_a_statement() {
        let src = "fn f() -> int {\n    var x = 1\n    #[cfg(target = \"x86_64-unknown-none\")]\n    x = 2\n    x\n}\n";
        let (s, d) = stripped(src, "x86_64-unknown-linux-gnu");
        assert!(d.is_empty());
        assert!(!s.contains("x = 2"), "{s}");
        let (k, _) = stripped(src, "x86_64-unknown-none");
        assert!(k.contains("x = 2"), "{k}");
    }

    #[test]
    fn an_undecidable_predicate_is_refused_and_keeps_its_node() {
        for (src, frag) in [
            ("#[cfg(target = \"no-such-target\")]\nfn a() { }\n", "no-such-target"),
            ("#[cfg(unix)]\nfn a() { }\n", "cfg(unix)"),
            ("#[cfg]\nfn a() { }\n", "exactly one predicate"),
            (
                "#[cfg(target = \"x86_64\", target = \"aarch64\")]\nfn a() { }\n",
                "exactly one predicate",
            ),
        ] {
            let (s, d) = stripped(src, "x86_64-unknown-linux-gnu");
            assert_eq!(d.len(), 1, "{src}");
            assert_eq!(d[0].code, codes::E0817, "{src}");
            assert!(d[0].message.contains(frag), "{src}: {}", d[0].message);
            assert!(s.contains("fn a()"), "the node is kept: {s}");
        }
    }
}
