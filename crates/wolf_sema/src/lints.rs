//! The `#[allow(…)]` scan (s67 — the warning system's source surface).
//!
//! `#[allow(w1301)]` is item-granular suppression: a warning whose
//! primary span falls inside the attributed declaration — body
//! included — is dropped before any configured level applies. The
//! attribute rides the ordinary s08 attribute grammar
//! (`[gram.item.attr]`: `allow` with identifier arguments), so no new
//! syntax exists; this module walks the parse trees of a resolved
//! package, collects every allow into a [`AllowRegion`] set, and
//! reports the two lints the scan itself owns:
//!
//! - **W0302** — an argument that names no registered code, and
//! - **W0303** — an `#[allow]` with no arguments at all.
//!
//! The scan is syntax-only (attributes attach to declarations, and the
//! region is the declaration node's span), so it runs on any package
//! that parses — warnings from every later phase are suppressible.

use wolf_ast::GreenNode;
use wolf_diag::lint::{AllowRegion, Selector};
use wolf_diag::{Diagnostic, codes};

use crate::graph::Package;

/// What the scan found: the allow regions, and the scan's own
/// diagnostics (W0302/W0303) in file/position order.
#[derive(Debug, Default)]
pub struct AllowScan {
    pub allows: Vec<AllowRegion>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Walk every file of `pkg` for `#[allow(…)]` attributes.
pub fn scan_allows(pkg: &Package) -> AllowScan {
    let mut scan = AllowScan::default();
    for unit in &pkg.files {
        walk(&unit.parse.root, &unit.raw.src, &mut scan);
    }
    scan
}

fn walk(node: &GreenNode, src: &[u8], scan: &mut AllowScan) {
    for attr in node.nodes().filter_map(wolf_ast::Attribute::cast) {
        for item in attr.items() {
            let named = item
                .path()
                .map(|p| text(src, p.syntax()))
                .is_some_and(|n| n.trim() == "allow");
            if !named {
                continue;
            }
            // The suppressed region is the attributed declaration —
            // the node carrying the attribute, body included.
            let region = node.span;
            let args: Vec<wolf_ast::AttrItem> = item
                .input()
                .map(|input| input.nodes().filter_map(wolf_ast::AttrItem::cast).collect())
                .unwrap_or_default();
            if args.is_empty() {
                scan.diagnostics.push(
                    Diagnostic::warning(
                        codes::W0303,
                        item.syntax().span,
                        "this `#[allow]` names no warning codes, so it allows nothing",
                    )
                    .with_label("no codes to allow")
                    .with_note(
                        "name the codes to suppress (`#[allow(w1301)]`) or delete \
                         the attribute."
                            .to_string(),
                    ),
                );
                continue;
            }
            for arg in args {
                let arg_node = arg.syntax();
                let spelled = text(src, arg_node).trim().to_string();
                let selector = Selector::parse(&spelled).ok().filter(|sel| match sel {
                    Selector::Code(c) => wolf_diag::explain(c).is_some(),
                    _ => true,
                });
                match selector {
                    Some(selector) => scan.allows.push(AllowRegion {
                        selector,
                        span: region,
                    }),
                    None => scan.diagnostics.push(
                        Diagnostic::warning(
                            codes::W0302,
                            arg_node.span,
                            format!(
                                "`{spelled}` is not a registered diagnostic code, \
                                 so this allow suppresses nothing"
                            ),
                        )
                        .with_label("no such code")
                        .with_note(
                            "codes look like `w1301` (one code) or `w13xx` (a \
                             family); the catalog in docs/diagnostics.md lists \
                             every registered code."
                                .to_string(),
                        ),
                    ),
                }
            }
        }
    }
    for child in node.nodes() {
        walk(child, src, scan);
    }
}

fn text(src: &[u8], node: &GreenNode) -> String {
    let sp = node.span;
    String::from_utf8_lossy(&src[sp.lo as usize..sp.hi as usize]).into_owned()
}
