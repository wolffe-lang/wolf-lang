//! The origin marker scan (s126, D61 — `[gram.attr.index]`).
//!
//! `#![index(0|1)]` on the file and `#[index(0|1)]` on a statement or
//! item decide whether subscripts in their lexical scope count from 0
//! or from 1. The scan is syntax-only, exactly like the `#[allow]`
//! scan: markers attach to nodes, a marker's region is the annotated
//! node's span, and the innermost region containing a subscript site
//! answers for it. No marker anywhere means origin 0 — the scan of an
//! unmarked package is a no-op by construction.
//!
//! The scan owns one diagnostic, **E0813**: a marker whose argument is
//! not the literal `0` or `1` (wrong number, missing, several, the
//! `index = …` spelling), a node that chose its origin twice, and a
//! file-wide `#![…]` attribute naming anything but `index` (the inner
//! form is strict from birth — D61). Misplaced `#![…]` position is the
//! parser's E0211, not ours.

use wolf_ast::{GreenNode, SyntaxKind};
use wolf_diag::{Diagnostic, codes};
use wolf_span::Span;

use crate::graph::Package;

/// Where every origin marker in a package reaches: file defaults and
/// lexical regions. Queried by the WIR lowering and the checked
/// executor at each subscript site.
#[derive(Debug, Default, Clone)]
pub struct OriginMap {
    /// `(file span of the whole unit is not needed — the file id on
    /// the marker's span keys the default)`: files whose header says
    /// `#![index(1)]` (or `0`, explicitly).
    file_defaults: Vec<(wolf_span::FileId, u8)>,
    /// `(region, origin)` for every statement/item marker, in scan
    /// order. Regions nest lexically; the innermost wins.
    regions: Vec<(Span, u8)>,
}

impl OriginMap {
    /// The origin governing a subscript spelled at `site`: the
    /// innermost `#[index(…)]` region containing it, else the file's
    /// `#![index(…)]` default, else 0.
    pub fn origin_at(&self, site: Span) -> u8 {
        self.regions
            .iter()
            .filter(|(r, _)| r.file == site.file && r.lo <= site.lo && site.hi <= r.hi)
            .min_by_key(|(r, _)| r.hi - r.lo)
            .map(|&(_, o)| o)
            .or_else(|| {
                self.file_defaults
                    .iter()
                    .find(|(f, _)| *f == site.file)
                    .map(|&(_, o)| o)
            })
            .unwrap_or(0)
    }

    /// True when any marker exists at all — the fast path for the
    /// zero-cost-when-absent promise.
    pub fn is_empty(&self) -> bool {
        self.file_defaults.is_empty() && self.regions.is_empty()
    }
}

/// What the scan found: the map, and its E0813 diagnostics in
/// file/position order.
#[derive(Debug, Default)]
pub struct OriginScan {
    pub map: OriginMap,
    pub diagnostics: Vec<Diagnostic>,
}

/// Walk every file of `pkg` for origin markers.
pub fn scan_origins(pkg: &Package) -> OriginScan {
    let mut scan = OriginScan::default();
    for unit in &pkg.files {
        file_header(&unit.parse.root, &unit.raw.src, &mut scan);
        walk(&unit.parse.root, &unit.raw.src, &mut scan);
    }
    scan
}

/// The `#![…]` header: direct children of the source file only —
/// a misplaced inner attribute (nested under an error node) already
/// carries the parser's E0211 and never takes effect.
fn file_header(root: &GreenNode, src: &[u8], scan: &mut OriginScan) {
    let mut chosen: Option<Span> = None;
    for inner in root
        .nodes()
        .filter(|n| n.kind == SyntaxKind::InnerAttribute)
    {
        for item in inner.nodes().filter_map(wolf_ast::AttrItem::cast) {
            let name = item
                .path()
                .map(|p| text(src, p.syntax().span))
                .unwrap_or_default();
            if name.trim() != "index" {
                scan.diagnostics.push(
                    Diagnostic::error(
                        codes::E0813,
                        item.syntax().span,
                        format!(
                            "`{}` is not a file-wide attribute — the only one is \
                             `index`",
                            name.trim()
                        ),
                    )
                    .with_label("not a file-wide attribute")
                    .with_note(
                        "the file-wide form chooses the subscript origin: \
                         `#![index(0)]` or `#![index(1)]`."
                            .to_string(),
                    ),
                );
                continue;
            }
            let Some(origin) = index_argument(&item, src, scan) else {
                continue;
            };
            if let Some(first) = chosen {
                scan.diagnostics.push(
                    Diagnostic::error(
                        codes::E0813,
                        item.syntax().span,
                        "this file already chose its origin",
                    )
                    .with_label("second `index` marker")
                    .with_secondary(first, "the origin was chosen here")
                    .with_note(
                        "one `#![index(…)]` speaks for the whole file; delete \
                                the extra marker."
                            .to_string(),
                    ),
                );
                continue;
            }
            chosen = Some(item.syntax().span);
            let file = item.syntax().span.file;
            scan.map.file_defaults.push((file, origin));
        }
    }
}

/// The statement/item markers: any node carrying `#[index(…)]` in the
/// ordinary attribute position scopes its whole span.
fn walk(node: &GreenNode, src: &[u8], scan: &mut OriginScan) {
    let mut chosen: Option<Span> = None;
    for attr in node.nodes().filter_map(wolf_ast::Attribute::cast) {
        for item in attr.items() {
            let named = item
                .path()
                .map(|p| text(src, p.syntax().span))
                .is_some_and(|n| n.trim() == "index");
            if !named {
                continue;
            }
            let Some(origin) = index_argument(&item, src, scan) else {
                continue;
            };
            if let Some(first) = chosen {
                scan.diagnostics.push(
                    Diagnostic::error(
                        codes::E0813,
                        item.syntax().span,
                        "this scope already chose its origin",
                    )
                    .with_label("second `index` marker")
                    .with_secondary(first, "the origin was chosen here")
                    .with_note(
                        "one marker per statement; nest a block to switch origins \
                         inside it."
                            .to_string(),
                    ),
                );
                continue;
            }
            chosen = Some(item.syntax().span);
            scan.map.regions.push((node.span, origin));
        }
    }
    for child in node.nodes() {
        walk(child, src, scan);
    }
}

/// The one argument an `index` marker takes: the literal `0` or `1`.
/// Anything else is E0813 and the marker is dropped (no region — the
/// scope keeps its surroundings' origin, so one mistake is one
/// diagnostic, not a cascade of shifted subscripts).
fn index_argument(item: &wolf_ast::AttrItem<'_>, src: &[u8], scan: &mut OriginScan) -> Option<u8> {
    let refuse = |scan: &mut OriginScan, span: Span, label: &str| {
        scan.diagnostics.push(
            Diagnostic::error(codes::E0813, span, "the subscript origin is `0` or `1`")
                .with_label(label.to_string())
                .with_note(
                    "write `index(1)` to count from one, or `index(0)` to restore \
                 the default."
                        .to_string(),
                ),
        );
        None
    };
    let Some(input) = item.input() else {
        return refuse(scan, item.syntax().span, "no origin named");
    };
    // The paren form only: `index = 1` and nested-attribute arguments
    // are not origins.
    let mut ints = input.tokens().filter(|t| t.kind == SyntaxKind::Int);
    let has_eq = input
        .tokens()
        .next()
        .is_some_and(|t| t.kind != SyntaxKind::LParen);
    let nested = input.nodes().next().is_some();
    let (first, second) = (ints.next(), ints.next());
    if has_eq || nested || second.is_some() {
        return refuse(scan, input.span, "not an origin");
    }
    let Some(tok) = first else {
        return refuse(scan, input.span, "no origin named");
    };
    match text(src, tok.span).trim() {
        "0" => Some(0),
        "1" => Some(1),
        _ => refuse(scan, tok.span, "not an origin"),
    }
}

fn text(src: &[u8], sp: Span) -> String {
    String::from_utf8_lossy(&src[sp.lo as usize..sp.hi as usize]).into_owned()
}
