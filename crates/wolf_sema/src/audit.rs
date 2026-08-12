//! The unsafety audit surface (s22 — the I13 precursor `wolf
//! audit-surface`; `wolf audit` proper is s51).
//!
//! D11's deal is that unsafety is *greppable*: three rings — `unsafe`
//! blocks, the `#[trusted]` manifest, inline C/asm — plus exactly two
//! re-entry doors. This module renders the complete inventory of a
//! package (diffable, CI-logged, snapshot-tested) and enforces the
//! ring-2 declaration rule: every module containing `#[trusted]`
//! functions must be declared in the package manifest's `trusted`
//! entry (E1303, [mem.boundary.trusted]) — a dependency growing
//! trusted code is a visible diff event, never a silent one.
//!
//! The manifest format here is the s51 **stub**: a `wolf.pkg` line
//! `trusted = mod_a, mod_b.sub` (repeatable; the package root module
//! is spelled `root`). s51 owns the real manifest; the roster and the
//! rule are what this sprint pins.

use wolf_ast::{GreenNode, SyntaxKind};
use wolf_diag::{Diagnostic, codes};

use crate::graph::{ItemKind, Package};

/// One module's unsafety inventory.
#[derive(Debug, Default)]
pub struct ModuleSurface {
    /// Dotted module path ("" = the package root).
    pub module: String,
    /// `#[trusted]` fns: (name, declared obligation).
    pub trusted: Vec<(String, String)>,
    /// `unsafe { }` block count (ring 1).
    pub unsafe_blocks: usize,
    /// `assume noalias` sites.
    pub assumes: usize,
    /// `borrow … from …` re-entry doors (door 1; door 2 — checked
    /// handles — is not an unsafety site and is not counted here).
    pub doors: usize,
    /// `import c` declarations (ring 3's front door).
    pub c_imports: usize,
    /// Inline C blocks + `asm` expressions (ring 3; semantics c10).
    pub inline_c_asm: usize,
}

impl ModuleSurface {
    fn is_empty(&self) -> bool {
        self.trusted.is_empty()
            && self.unsafe_blocks == 0
            && self.assumes == 0
            && self.doors == 0
            && self.c_imports == 0
            && self.inline_c_asm == 0
    }
}

fn count_kinds(node: &GreenNode, s: &mut ModuleSurface) {
    match node.kind {
        SyntaxKind::UnsafeBlock => s.unsafe_blocks += 1,
        SyntaxKind::AssumeStmt => s.assumes += 1,
        SyntaxKind::BorrowExpr => s.doors += 1,
        SyntaxKind::ImportCDecl => s.c_imports += 1,
        SyntaxKind::InlineC | SyntaxKind::AsmExpr => s.inline_c_asm += 1,
        _ => {}
    }
    for child in node.nodes() {
        count_kinds(child, s);
    }
}

/// The per-module inventory of `pkg`, in topological (dependency-
/// first) order — deterministic by construction.
pub fn surface(pkg: &Package) -> Vec<ModuleSurface> {
    let mut out = Vec::new();
    for &m in &pkg.topo {
        let mut s = ModuleSurface {
            module: pkg.modules[m].dotted(),
            ..ModuleSurface::default()
        };
        for item in pkg.tables[m]
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Fn)
        {
            let node = crate::sig::item_node(pkg, item);
            let src = &pkg.files[item.file].raw.src;
            let text = |sp: wolf_span::Span| {
                String::from_utf8_lossy(&src[sp.lo as usize..sp.hi as usize]).into_owned()
            };
            if let Some(obl) = crate::sig::trusted_attr(node, text) {
                s.trusted.push((item.name.clone(), obl));
            }
        }
        s.trusted.sort();
        for &fi in &pkg.modules[m].files {
            count_kinds(&pkg.files[fi].parse.root, &mut s);
        }
        out.push(s);
    }
    out
}

/// The human/CI rendering (`wolf audit-surface`): the complete ring
/// inventory, one block per module, "(clean)" for a package with no
/// unsafety anywhere. Greppable and diffable on purpose.
pub fn render(surfaces: &[ModuleSurface]) -> String {
    use std::fmt::Write;
    let mut out = String::from("unsafety surface (D11 rings)\n");
    let mut any = false;
    for s in surfaces {
        if s.is_empty() {
            continue;
        }
        any = true;
        let name = if s.module.is_empty() {
            "root"
        } else {
            &s.module
        };
        let _ = writeln!(out, "module {name}:");
        for (fnname, obl) in &s.trusted {
            let obl = if obl.is_empty() {
                "(no stated obligation)".to_string()
            } else {
                format!("\"{obl}\"")
            };
            let _ = writeln!(out, "  trusted fn {fnname} — {obl}");
        }
        let _ = writeln!(
            out,
            "  unsafe blocks: {} · assume sites: {} · re-entry doors: {} · \
             import c: {} · inline c/asm: {}",
            s.unsafe_blocks, s.assumes, s.doors, s.c_imports, s.inline_c_asm
        );
    }
    if !any {
        out.push_str("(clean — no unsafety rings in this package)\n");
    }
    out
}

/// Parse the `trusted = …` entries of a `wolf.pkg` manifest (the s51
/// stub format). Absent manifest ⇒ empty declaration set.
pub fn manifest_trusted(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("trusted") else {
            continue;
        };
        let Some(list) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        out.extend(
            list.split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty()),
        );
    }
    out.sort();
    out.dedup();
    out
}

/// The ring-2 declaration rule ([mem.boundary.trusted]): every module
/// with `#[trusted]` functions must appear in the manifest's `trusted`
/// list (`manifest` = the `wolf.pkg` body, `None` when the package has
/// none — then any trusted module is undeclared). E1303 per undeclared
/// module, anchored at the first trusted fn's declaration.
pub fn manifest_check(pkg: &Package, manifest: Option<&str>) -> Vec<Diagnostic> {
    let declared = manifest.map(manifest_trusted).unwrap_or_default();
    let mut diags = Vec::new();
    for s in surface(pkg) {
        if s.trusted.is_empty() {
            continue;
        }
        let key = if s.module.is_empty() {
            "root".to_string()
        } else {
            s.module.clone()
        };
        if declared.contains(&key) {
            continue;
        }
        // Anchor: the first trusted fn's name span in this module.
        let m = pkg
            .topo
            .iter()
            .copied()
            .find(|&m| pkg.modules[m].dotted() == s.module)
            .unwrap_or(0);
        let first = &s.trusted[0].0;
        let span = pkg.tables[m]
            .get(first)
            .map(|item| item.name_span)
            .unwrap_or_else(|| wolf_span::Span::new(pkg.files[0].raw.file, 0, 0));
        let mut d = Diagnostic::error(
            codes::E1303,
            span,
            format!(
                "module `{key}` holds `#[trusted]` code, but the package manifest \
                 does not declare it"
            ),
        )
        .with_label("the first trusted function is here")
        .with_note(format!(
            "add `trusted = {key}` to `wolf.pkg` — the trusted roster is the \
             supply-chain surface `wolf audit` diffs (D11 ring 2), so every \
             trusted module is declared, visibly."
        ));
        if manifest.is_none() {
            d = d.with_note(
                "this package has no `wolf.pkg` yet; create one next to the entry file.",
            );
        }
        diags.push(d);
    }
    diags
}
