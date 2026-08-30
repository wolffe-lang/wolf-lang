//! `wolfi` v0 — the binary module interface (s12, the D7 spine).
//!
//! Every later sprint keys off the two hashes minted here: external
//! dependents invalidate on `export_hash` (the `pub` partition),
//! same-package dependents on `pkg_hash` (which also covers `pub(pkg)`).
//! A body-only edit changes neither; adding a `pub(pkg)` item
//! invalidates the package, not the world.
//!
//! The encoding may version; the *stability properties* may not:
//! items sort canonically by `(kind, name)`, signatures carry resolved
//! paths with spans stripped, nothing serialized ever passes through
//! hash-map iteration order, and no absolute path or timestamp exists
//! anywhere in the format — two builds of the same source on any host
//! are byte-identical (D33 synergy, CI-checked).
//!
//! Interfaces are *outputs* this sprint: [`encode`] produces the
//! artifact bytes, [`decode`] is the loader API that s31's separate
//! compilation will consume (exercised by round-trip tests today), and
//! [`pretty`] backs `wolf interface`.
//!
//! s14 extends the format with impl rows (headers + associated-type
//! rewrites, canonical order) and per-trait dyn-safety records (the
//! dyn-safe method set, canonical order). Both ride in BOTH hash
//! partitions: coherence makes an impl addition anywhere an
//! interface change, while member *bodies* — like all bodies — never
//! serialize, so generic/impl body edits keep every hash fixed.

use std::collections::BTreeMap;

use wolf_ast::{
    ConstDecl, EnumDecl, EnumVariant, FnDecl, GenericParamList, GreenNode, Param, ParamList,
    PathType, RetType, StructDecl, StructField, SyntaxKind, TraitDecl, TypeDecl, is_type_kind,
};

use crate::graph::{BindTarget, ItemKind, Package, Vis, item_vis};
use crate::prelude;

/// Magic bytes opening every interface file.
pub const MAGIC: &[u8; 5] = b"WOLFI";
/// Format version (v0 — encoding may change, properties may not).
pub const FORMAT_VERSION: u32 = 0;
/// Language edition stamped into interfaces (pre-1.0 placeholder).
pub const EDITION: &str = "v1";

/// One item row of an interface: name, kind, visibility, stable id,
/// and the fully elaborated signature (resolved paths, no spans).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfaceItem {
    pub name: String,
    pub kind: ItemKind,
    pub vis: Vis,
    /// Stable item id — reserved headroom for function-grain hashing
    /// (I7); v0 assigns ordinals in canonical order.
    pub id: u32,
    pub sig: String,
}

/// One impl row (s14). Impls have no names and no visibility: every
/// impl is interface surface, because coherence makes an impl
/// addition anywhere an observable change — adding or removing one
/// moves both hashes; editing member *bodies* moves neither.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IfaceImpl {
    /// Rendered header, resolved paths, canonical:
    /// `impl[T: self.Show] self.Show for self.Cover`.
    pub header: String,
    /// Associated-type rewrites (`type Item = int`), canonical order.
    pub rewrites: Vec<String>,
}

/// A trait's recorded dyn-safety (s14, RFC 0255): the verdict plus
/// the dyn-safe method set in canonical order (c05 builds witness
/// tables from exactly this list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfaceTraitDyn {
    pub name: String,
    pub dyn_safe: bool,
    /// Canonically ordered method names (empty when not dyn-safe).
    pub methods: Vec<String>,
}

/// A decoded (or freshly built) module interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    pub package: String,
    /// Module path segments from the package root (empty = root).
    pub module_path: Vec<String>,
    pub toolchain: String,
    pub edition: String,
    /// Direct deps: (dotted module path, that module's export hash).
    pub deps: Vec<(String, [u8; 32])>,
    /// Exported items (`pub` and `pub(pkg)`), canonically sorted by
    /// `(kind, name)`. Private items never appear.
    pub items: Vec<IfaceItem>,
    /// Every impl of the module (s14), canonically sorted; hashed
    /// into both partitions.
    pub impls: Vec<IfaceImpl>,
    /// Dyn-safety records for the module's exported traits (s14),
    /// sorted by name.
    pub dyns: Vec<IfaceTraitDyn>,
    /// s22 — the module's `#[trusted]` roster (D11 ring 2): every
    /// trusted function with its declared obligation, sorted by name.
    /// **All visibilities appear** — trust is supply-chain surface,
    /// not signature surface — and the roster rides in BOTH hash
    /// partitions, so a dependency growing trusted code is an
    /// interface change (`wolf audit`'s diff event,
    /// [mem.boundary.trusted]).
    pub trusted: Vec<(String, String)>,
    pub export_hash: [u8; 32],
    pub pkg_hash: [u8; 32],
}

/// Build the interface of every module in `pkg`, dependency-first
/// (topological order — dep hashes feed dependents).
pub fn build_interfaces(pkg: &Package) -> Vec<Interface> {
    let mut export_hashes: BTreeMap<usize, [u8; 32]> = BTreeMap::new();
    let mut out = Vec::new();
    for &m in &pkg.topo {
        let iface = build_one(pkg, m, &export_hashes);
        export_hashes.insert(m, iface.export_hash);
        out.push(iface);
    }
    out
}

fn build_one(pkg: &Package, m: usize, export_hashes: &BTreeMap<usize, [u8; 32]>) -> Interface {
    let module = &pkg.modules[m];
    let mut deps: Vec<(String, [u8; 32])> = module
        .deps
        .iter()
        .map(|&d| {
            (
                pkg.modules[d].dotted(),
                export_hashes.get(&d).copied().unwrap_or([0; 32]),
            )
        })
        .collect();
    deps.sort();
    // Canonical item order: (kind, name). Private items are not
    // interface surface at all.
    let mut rows: Vec<(&crate::graph::Item, ItemKind, String)> = pkg.tables[m]
        .items
        .iter()
        .filter(|i| i.vis != Vis::Private)
        .map(|i| (i, i.kind, i.name.clone()))
        .collect();
    rows.sort_by(|a, b| (a.1, a.2.as_str()).cmp(&(b.1, b.2.as_str())));
    let items: Vec<IfaceItem> = rows
        .iter()
        .enumerate()
        .map(|(id, (item, _, _))| IfaceItem {
            name: item.name.clone(),
            kind: item.kind,
            vis: item.vis,
            id: id as u32,
            sig: render_item_sig(pkg, m, item),
        })
        .collect();
    let impls = render_impls(pkg, m);
    let dyns = render_dyns(pkg, m, &rows);
    let trusted = trusted_roster(pkg, m);
    let mut head = Vec::new();
    write_header(&mut head, pkg, module, &deps);
    // Impls, dyn records, and the trusted roster ride in BOTH
    // partitions: coherence makes an impl addition anywhere an
    // interface change (s14), and trust marks are supply-chain
    // surface regardless of visibility (s22, [mem.boundary.trusted]).
    let mut export_bytes = head.clone();
    write_items(
        &mut export_bytes,
        items.iter().filter(|i| i.vis == Vis::Pub),
    );
    write_impls(&mut export_bytes, &impls);
    write_dyns(&mut export_bytes, &dyns);
    write_trusted(&mut export_bytes, &trusted);
    let mut pkg_bytes = head;
    write_items(&mut pkg_bytes, items.iter());
    write_impls(&mut pkg_bytes, &impls);
    write_dyns(&mut pkg_bytes, &dyns);
    write_trusted(&mut pkg_bytes, &trusted);
    Interface {
        package: pkg.name.clone(),
        module_path: module.path.clone(),
        toolchain: env!("CARGO_PKG_VERSION").to_string(),
        edition: EDITION.to_string(),
        deps,
        items,
        impls,
        dyns,
        trusted,
        export_hash: *blake3::hash(&export_bytes).as_bytes(),
        pkg_hash: *blake3::hash(&pkg_bytes).as_bytes(),
    }
}

/// The module's `#[trusted]` function roster (s22): every trusted fn
/// — any visibility — with its declared obligation, sorted by name.
fn trusted_roster(pkg: &Package, m: usize) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = pkg.tables[m]
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Fn)
        .filter_map(|item| {
            let node = crate::sig::item_node(pkg, item);
            let src = &pkg.files[item.file].raw.src;
            let text = |sp: wolf_span::Span| {
                String::from_utf8_lossy(&src[sp.lo as usize..sp.hi as usize]).into_owned()
            };
            crate::sig::trusted_attr(node, text).map(|obl| (item.name.clone(), obl))
        })
        .collect();
    out.sort();
    out
}

/// Render every impl of module `m` (headers + assoc rewrites),
/// canonically sorted. Bodies never appear — the golden rule keeps
/// generic bodies out of the interface.
fn render_impls(pkg: &Package, m: usize) -> Vec<IfaceImpl> {
    let mut out = Vec::new();
    for &fi in &pkg.modules[m].files {
        let root = &pkg.files[fi].parse.root;
        for node in root.nodes().filter(|n| n.kind == SyntaxKind::ImplDecl) {
            let Some(d) = wolf_ast::ImplDecl::cast(node) else {
                continue;
            };
            let mut ctx = SigCtx {
                pkg,
                module: m,
                file: fi,
                generics: Vec::new(),
            };
            let mut header = String::from("impl");
            header.push_str(&bind_generics(&mut ctx, d.generics()));
            header.push(' ');
            match d.trait_path() {
                Some(p) => header.push_str(&render_path(&ctx, p.syntax())),
                None => header.push_str("<error>"),
            }
            if let Some(args) = node.nodes().find_map(wolf_ast::TypeArgList::cast) {
                let parts: Vec<String> = args
                    .args()
                    .filter(|a| is_type_kind(a.kind))
                    .map(|a| render_type(&ctx, a))
                    .collect();
                if !parts.is_empty() {
                    header.push_str(&format!("[{}]", parts.join(", ")));
                }
            }
            if let Some(t) = d.self_ty() {
                header.push_str(" for ");
                header.push_str(&render_type(&ctx, t));
            }
            let mut rewrites: Vec<String> = d
                .members()
                .filter(|mn| mn.kind == SyntaxKind::TypeDecl)
                .filter_map(|mn| {
                    let t = TypeDecl::cast(mn)?;
                    let name = t.name().map(|tok| ctx.text(tok.span))?;
                    let rhs = t
                        .def()
                        .map(|def| render_type(&ctx, def))
                        .unwrap_or_else(|| "<error>".to_string());
                    Some(format!("type {name} = {rhs}"))
                })
                .collect();
            rewrites.sort();
            out.push(IfaceImpl { header, rewrites });
        }
    }
    out.sort();
    out
}

/// Dyn-safety records for the module's exported traits (s14).
fn render_dyns(
    pkg: &Package,
    m: usize,
    rows: &[(&crate::graph::Item, ItemKind, String)],
) -> Vec<IfaceTraitDyn> {
    let mut out: Vec<IfaceTraitDyn> = rows
        .iter()
        .filter(|(_, kind, _)| *kind == ItemKind::Trait)
        .filter_map(|(item, _, _)| {
            let node = crate::sig::item_node(pkg, item);
            if node.kind != SyntaxKind::TraitDecl {
                return None;
            }
            let report = crate::traits::dyn_report(node, &pkg.files[item.file].raw.src);
            Some(IfaceTraitDyn {
                name: item.name.clone(),
                dyn_safe: report.safe(),
                methods: report.methods,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    let _ = m;
    out
}

// ------------------------------------------------------------ encoding --

fn w_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn w_str(out: &mut Vec<u8>, s: &str) {
    w_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn write_header(
    out: &mut Vec<u8>,
    pkg: &Package,
    module: &crate::graph::ModuleData,
    deps: &[(String, [u8; 32])],
) {
    out.extend_from_slice(MAGIC);
    w_u32(out, FORMAT_VERSION);
    w_str(out, env!("CARGO_PKG_VERSION"));
    w_str(out, EDITION);
    w_str(out, &pkg.name);
    w_u32(out, module.path.len() as u32);
    for seg in &module.path {
        w_str(out, seg);
    }
    w_u32(out, deps.len() as u32);
    for (path, hash) in deps {
        w_str(out, path);
        out.extend_from_slice(hash);
    }
}

fn write_items<'a>(out: &mut Vec<u8>, items: impl Iterator<Item = &'a IfaceItem>) {
    let items: Vec<&IfaceItem> = items.collect();
    w_u32(out, items.len() as u32);
    for i in items {
        w_str(out, &i.name);
        out.push(i.kind.as_u8());
        out.push(i.vis.as_u8());
        w_u32(out, i.id);
        w_str(out, &i.sig);
    }
}

fn write_impls(out: &mut Vec<u8>, impls: &[IfaceImpl]) {
    w_u32(out, impls.len() as u32);
    for i in impls {
        w_str(out, &i.header);
        w_u32(out, i.rewrites.len() as u32);
        for r in &i.rewrites {
            w_str(out, r);
        }
    }
}

fn write_trusted(out: &mut Vec<u8>, trusted: &[(String, String)]) {
    w_u32(out, trusted.len() as u32);
    for (name, obligation) in trusted {
        w_str(out, name);
        w_str(out, obligation);
    }
}

fn write_dyns(out: &mut Vec<u8>, dyns: &[IfaceTraitDyn]) {
    w_u32(out, dyns.len() as u32);
    for d in dyns {
        w_str(out, &d.name);
        out.push(u8::from(d.dyn_safe));
        w_u32(out, d.methods.len() as u32);
        for m in &d.methods {
            w_str(out, m);
        }
    }
}

/// Serialize an interface to its on-disk `wolfi` v0 bytes.
pub fn encode(iface: &Interface) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    w_u32(&mut out, FORMAT_VERSION);
    w_str(&mut out, &iface.toolchain);
    w_str(&mut out, &iface.edition);
    w_str(&mut out, &iface.package);
    w_u32(&mut out, iface.module_path.len() as u32);
    for seg in &iface.module_path {
        w_str(&mut out, seg);
    }
    w_u32(&mut out, iface.deps.len() as u32);
    for (path, hash) in &iface.deps {
        w_str(&mut out, path);
        out.extend_from_slice(hash);
    }
    write_items(&mut out, iface.items.iter());
    write_impls(&mut out, &iface.impls);
    write_dyns(&mut out, &iface.dyns);
    write_trusted(&mut out, &iface.trusted);
    out.extend_from_slice(&iface.export_hash);
    out.extend_from_slice(&iface.pkg_hash);
    out
}

/// The loader half of the D7 spine: parse `wolfi` v0 bytes back into an
/// [`Interface`]. s31's separate compilation consumes this; today the
/// round-trip tests do.
pub fn decode(bytes: &[u8]) -> Result<Interface, String> {
    struct R<'a> {
        b: &'a [u8],
        pos: usize,
    }
    impl R<'_> {
        fn take(&mut self, n: usize) -> Result<&[u8], String> {
            if self.pos + n > self.b.len() {
                return Err("interface file truncated".to_string());
            }
            let s = &self.b[self.pos..self.pos + n];
            self.pos += n;
            Ok(s)
        }
        fn u32(&mut self) -> Result<u32, String> {
            let b = self.take(4)?;
            Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        fn str(&mut self) -> Result<String, String> {
            let n = self.u32()? as usize;
            let b = self.take(n)?;
            String::from_utf8(b.to_vec()).map_err(|_| "invalid UTF-8 in interface".to_string())
        }
        fn hash(&mut self) -> Result<[u8; 32], String> {
            let b = self.take(32)?;
            let mut h = [0u8; 32];
            h.copy_from_slice(b);
            Ok(h)
        }
    }
    let mut r = R { b: bytes, pos: 0 };
    if r.take(5)? != MAGIC {
        return Err("not a wolfi interface (bad magic)".to_string());
    }
    let ver = r.u32()?;
    if ver != FORMAT_VERSION {
        return Err(format!(
            "unsupported wolfi format version {ver} (this toolchain reads v{FORMAT_VERSION})"
        ));
    }
    let toolchain = r.str()?;
    let edition = r.str()?;
    let package = r.str()?;
    let nseg = r.u32()?;
    let mut module_path = Vec::new();
    for _ in 0..nseg {
        module_path.push(r.str()?);
    }
    let ndeps = r.u32()?;
    let mut deps = Vec::new();
    for _ in 0..ndeps {
        let p = r.str()?;
        let h = r.hash()?;
        deps.push((p, h));
    }
    let nitems = r.u32()?;
    let mut items = Vec::new();
    for _ in 0..nitems {
        let name = r.str()?;
        let kind = ItemKind::from_u8(r.take(1)?[0]).ok_or("bad item kind")?;
        let vis = Vis::from_u8(r.take(1)?[0]).ok_or("bad visibility")?;
        let id = r.u32()?;
        let sig = r.str()?;
        items.push(IfaceItem {
            name,
            kind,
            vis,
            id,
            sig,
        });
    }
    let nimpls = r.u32()?;
    let mut impls = Vec::new();
    for _ in 0..nimpls {
        let header = r.str()?;
        let nrw = r.u32()?;
        let mut rewrites = Vec::new();
        for _ in 0..nrw {
            rewrites.push(r.str()?);
        }
        impls.push(IfaceImpl { header, rewrites });
    }
    let ndyns = r.u32()?;
    let mut dyns = Vec::new();
    for _ in 0..ndyns {
        let name = r.str()?;
        let dyn_safe = r.take(1)?[0] != 0;
        let nm = r.u32()?;
        let mut methods = Vec::new();
        for _ in 0..nm {
            methods.push(r.str()?);
        }
        dyns.push(IfaceTraitDyn {
            name,
            dyn_safe,
            methods,
        });
    }
    let ntrusted = r.u32()?;
    let mut trusted = Vec::new();
    for _ in 0..ntrusted {
        let name = r.str()?;
        let obligation = r.str()?;
        trusted.push((name, obligation));
    }
    let export_hash = r.hash()?;
    let pkg_hash = r.hash()?;
    if r.pos != bytes.len() {
        return Err("trailing bytes after interface".to_string());
    }
    Ok(Interface {
        package,
        module_path,
        toolchain,
        edition,
        deps,
        items,
        impls,
        dyns,
        trusted,
        export_hash,
        pkg_hash,
    })
}

fn hex(h: &[u8; 32]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// The human rendering behind `wolf interface` — snapshot-stable.
pub fn pretty(iface: &Interface) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let module = if iface.module_path.is_empty() {
        "(root)".to_string()
    } else {
        iface.module_path.join(".")
    };
    let _ = writeln!(out, "module {} :: {}", iface.package, module);
    let _ = writeln!(
        out,
        "  wolfi v{FORMAT_VERSION} \u{b7} toolchain {} \u{b7} edition {}",
        iface.toolchain, iface.edition
    );
    let _ = writeln!(out, "  export_hash {}", hex(&iface.export_hash));
    let _ = writeln!(out, "  pkg_hash    {}", hex(&iface.pkg_hash));
    if iface.deps.is_empty() {
        let _ = writeln!(out, "  deps: (none)");
    } else {
        let _ = writeln!(out, "  deps:");
        for (p, h) in &iface.deps {
            let _ = writeln!(out, "    {p}  {}", hex(h));
        }
    }
    if iface.items.is_empty() {
        let _ = writeln!(out, "  items: (none)");
    } else {
        let _ = writeln!(out, "  items:");
        for i in &iface.items {
            let _ = writeln!(
                out,
                "    [{}] {}{} \u{2014} {}",
                i.id,
                i.vis.render(),
                i.name,
                i.sig
            );
        }
    }
    if !iface.impls.is_empty() {
        let _ = writeln!(out, "  impls:");
        for i in &iface.impls {
            let _ = writeln!(out, "    {}", i.header);
            for r in &i.rewrites {
                let _ = writeln!(out, "      {r}");
            }
        }
    }
    if !iface.dyns.is_empty() {
        let _ = writeln!(out, "  dyn:");
        for d in &iface.dyns {
            if d.dyn_safe {
                let _ = writeln!(
                    out,
                    "    {} dyn-safe {{ {} }}",
                    d.name,
                    d.methods.join(", ")
                );
            } else {
                let _ = writeln!(out, "    {} not dyn-safe", d.name);
            }
        }
    }
    if !iface.trusted.is_empty() {
        let _ = writeln!(out, "  trusted (D11 ring 2, hashed):");
        for (name, obligation) in &iface.trusted {
            if obligation.is_empty() {
                let _ = writeln!(out, "    fn {name} — (no stated obligation)");
            } else {
                let _ = writeln!(out, "    fn {name} — \"{obligation}\"");
            }
        }
    }
    out
}

// ----------------------------------------------------- sig rendering ----

/// Signature-rendering context: enough resolution to qualify the first
/// segment of every path, spans stripped by construction.
struct SigCtx<'a> {
    pkg: &'a Package,
    module: usize,
    file: usize,
    generics: Vec<String>,
}

impl SigCtx<'_> {
    fn src(&self) -> &[u8] {
        &self.pkg.files[self.file].raw.src
    }

    fn text(&self, span: wolf_span::Span) -> String {
        let src = self.src();
        String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn bindings(&self) -> &[crate::graph::Binding] {
        let module = &self.pkg.modules[self.module];
        let slot = module
            .files
            .iter()
            .position(|&f| f == self.file)
            .expect("file belongs to module");
        &module.bindings[slot]
    }

    /// Fully qualify a path's first segment by its resolution origin.
    fn qualify(&self, name: &str) -> String {
        if self.generics.iter().any(|g| g == name) {
            return name.to_string();
        }
        if let Some(b) = self.bindings().iter().find(|b| b.name == name) {
            return match &b.target {
                BindTarget::PkgModule(m) => self.pkg.modules[*m].dotted(),
                BindTarget::StdModule(mi) => prelude::STD_MODULES[*mi].path.join("."),
                BindTarget::Item { module, name } => {
                    format!("{}.{}", self.pkg.modules[*module].dotted(), name)
                }
                BindTarget::StdItem | BindTarget::CNamespace | BindTarget::Poisoned => {
                    name.to_string()
                }
            };
        }
        if self.pkg.tables[self.module].get(name).is_some() {
            return format!("self.{name}");
        }
        if prelude::is_builtin_type(name) {
            return name.to_string();
        }
        if prelude::in_prelude(name) {
            return format!("prelude.{name}");
        }
        name.to_string()
    }
}

/// The elaborated signature of one module item — the SAME renderer the
/// `wolfi` interface and its hashes are built from, exposed so that
/// documentation quotes the compiler's own answer rather than
/// re-parsing source (s53: docs cannot lie about types). Private items
/// render too, which interfaces deliberately do not carry: `wolf doc
/// --private` documents them, and nothing about that reaches a hash.
pub fn item_signature(pkg: &Package, module: usize, item: &crate::graph::Item) -> String {
    render_item_sig(pkg, module, item)
}

/// Render the elaborated signature of a module item.
fn render_item_sig(pkg: &Package, module: usize, item: &crate::graph::Item) -> String {
    let file = item.file;
    let root = &pkg.files[file].parse.root;
    let node = root
        .nodes()
        .filter(|n| n.kind.is_item())
        .nth(item.decl)
        .expect("item decl index valid");
    let mut ctx = SigCtx {
        pkg,
        module,
        file,
        generics: Vec::new(),
    };
    match node.kind {
        SyntaxKind::FnDecl => render_fn(&mut ctx, node),
        SyntaxKind::StructDecl => render_struct(&mut ctx, node),
        SyntaxKind::EnumDecl => render_enum(&mut ctx, node),
        SyntaxKind::TraitDecl => render_trait(&mut ctx, node),
        SyntaxKind::TypeDecl => render_type_alias(&mut ctx, node),
        SyntaxKind::ConstDecl => {
            let d = ConstDecl::cast(node).expect("kind");
            let name = d.name().map(|t| ctx.text(t.span)).unwrap_or_default();
            match d.ty() {
                Some(t) => format!("const {name}: {}", render_type(&ctx, t)),
                None => format!("const {name}"),
            }
        }
        SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
            let kw = if node.kind == SyntaxKind::LetDecl {
                "let"
            } else {
                "var"
            };
            // The binder that declares THIS name (a comma group has
            // several — D63); fall back to the first.
            let binders = wolf_ast::binding_binders(node);
            let owner = binders
                .iter()
                .find(|b| {
                    b.pattern.is_some_and(|p| {
                        p.tokens()
                            .any(|t| t.kind == SyntaxKind::Ident && ctx.text(t.span) == item.name)
                    })
                })
                .or(binders.first());
            let ty = owner.and_then(|b| b.ty).map(|t| render_type(&ctx, t));
            match ty {
                Some(t) => format!("{kw} {}: {t}", item.name),
                None => format!("{kw} {}", item.name),
            }
        }
        _ => item.name.clone(),
    }
}

fn bind_generics(ctx: &mut SigCtx<'_>, list: Option<GenericParamList<'_>>) -> String {
    let Some(list) = list else {
        return String::new();
    };
    let mut parts = Vec::new();
    for p in list.params() {
        let Some(name) = p.name() else { continue };
        let name = ctx.text(name.span);
        ctx.generics.push(name.clone());
        let mut s = name;
        if p.is_type_param() {
            s.push_str(": type");
        } else if let Some(bound) = p.bound() {
            let bounds: Vec<String> = bound
                .paths()
                .map(|b| render_path(ctx, b.syntax()))
                .collect();
            if !bounds.is_empty() {
                s.push_str(": ");
                s.push_str(&bounds.join(" + "));
            }
        }
        parts.push(s);
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("[{}]", parts.join(", "))
    }
}

fn render_fn(ctx: &mut SigCtx<'_>, node: &GreenNode) -> String {
    let d = FnDecl::cast(node).expect("kind");
    let mut out = String::new();
    if d.is_comptime() {
        out.push_str("comptime ");
    }
    if let Some(abi) = d.extern_abi() {
        out.push_str(&format!("extern {} ", ctx.text(abi.syntax().span)));
    }
    if d.is_export() {
        out.push_str("export ");
    }
    out.push_str("fn ");
    if let Some(n) = d.name() {
        let name = ctx.text(n.span);
        out.push_str(&name);
    }
    let saved = ctx.generics.len();
    out.push_str(&bind_generics(ctx, d.generics()));
    out.push('(');
    if let Some(params) = d.params() {
        out.push_str(&render_params(ctx, params));
    }
    out.push(')');
    if let Some(ret) = d.ret_ty() {
        out.push_str(" -> ");
        out.push_str(&render_ret(ctx, ret));
    }
    if let Some(scheme) = region_scheme(ctx, d) {
        out.push_str(&scheme);
    }
    ctx.generics.truncate(saved);
    out
}

/// The fn's derived region scheme (s20 — the scheme-carrying
/// interface). The Cyclone defaults, spelled explicitly and hashed
/// with the signature: `-` is a Copy-shaped position (no region
/// content), `ρN` a fresh generalized region per heap-reaching
/// parameter, `ρN region` a first-class region value (the parameter
/// *is* its region's handle), and a heap result lands in `ρ_caller`
/// (D12); a returned region value is callee-created, fresh on the
/// caller's side. Today every scheme IS the default — this makes the
/// contract an interface artifact, so call-site instantiation errors
/// have a signature-side fact to cite, and a future annotation
/// surface moves the hash exactly like any signature edit. Derived
/// from the *declared* type shape (spans-stripped render): a
/// `distinct` alias of a Copy type conservatively derives a region
/// nothing ever constrains, which is harmless. All-Copy signatures
/// derive no scheme and render unchanged.
fn region_scheme(ctx: &SigCtx<'_>, d: FnDecl<'_>) -> Option<String> {
    fn copy_shaped(rendered: &str) -> bool {
        crate::types::Prim::from_name(rendered).is_some()
            || rendered.starts_with("fn(")
            || rendered.starts_with("fn[")
            || rendered.starts_with("handle ")
            || rendered.starts_with('*')
            || rendered.starts_with("wrapping[")
            || rendered == "<error>"
            || rendered.is_empty()
    }
    let mut fresh = 0u32;
    let mut any = false;
    let mut parts: Vec<String> = Vec::new();
    if let Some(params) = d.params() {
        for p in params.params() {
            let rendered = p
                .ty()
                .map(|t| render_type(ctx, t))
                .unwrap_or_else(|| "<error>".to_string());
            let part = if rendered == "region" {
                fresh += 1;
                any = true;
                format!("\u{3c1}{fresh} region")
            } else if copy_shaped(&rendered) {
                "-".to_string()
            } else {
                fresh += 1;
                any = true;
                format!("\u{3c1}{fresh}")
            };
            parts.push(part);
        }
    }
    let ret = match d.ret_ty().and_then(|r| r.ty()) {
        Some(t) => {
            let rendered = render_type(ctx, t);
            if rendered == "region" {
                any = true;
                "\u{3c1}_fresh region".to_string()
            } else if copy_shaped(&rendered) || rendered == "()" {
                "-".to_string()
            } else {
                any = true;
                "\u{3c1}_caller".to_string()
            }
        }
        None => "-".to_string(),
    };
    if !any {
        return None;
    }
    Some(format!(" \u{b7} regions ({}) -> {ret}", parts.join(", ")))
}

fn render_params(ctx: &SigCtx<'_>, params: ParamList<'_>) -> String {
    let mut parts = Vec::new();
    for p in params.params() {
        let mut s = String::new();
        match p.mode() {
            Some(wolf_ast::ParamMode::Mut) => s.push_str("mut "),
            Some(wolf_ast::ParamMode::Take) => s.push_str("take "),
            None => {}
        }
        if p.is_self() {
            s.push_str("self");
            if let Some(vs) = p.view_set() {
                let fields: Vec<String> = vs.fields().map(|t| ctx.text(t.span)).collect();
                s.push_str(&format!(".{{{}}}", fields.join(", ")));
            }
        } else {
            if let Some(n) = Param::name(p) {
                s.push_str(&ctx.text(n.span));
            }
            if let Some(t) = p.ty() {
                s.push_str(": ");
                s.push_str(&render_type(ctx, t));
            }
        }
        parts.push(s);
    }
    parts.join(", ")
}

fn render_ret(ctx: &SigCtx<'_>, ret: RetType<'_>) -> String {
    let mut out = ret
        .ty()
        .map(|t| render_type(ctx, t))
        .unwrap_or_else(|| "<error>".to_string());
    if let Some(row) = ret.error_row() {
        // Canonical tag-set order — sorted by name, `..` last — is
        // fixed NOW in the interface format (s15): source-order churn
        // inside a row never moves a hash.
        let mut entries: Vec<String> = row
            .entries()
            .map(|e| {
                let path = e
                    .path()
                    .map(|p| raw_path(ctx, p.syntax()))
                    .unwrap_or_default();
                let payload: Vec<String> = e.payload().map(|t| render_type(ctx, t)).collect();
                if payload.is_empty() {
                    path
                } else {
                    format!("{path}({})", payload.join(", "))
                }
            })
            .collect();
        entries.sort();
        if row.is_open() {
            entries.push("..".to_string());
        }
        out.push_str(&format!(" ! {{{}}}", entries.join(", ")));
    }
    out
}

fn render_struct(ctx: &mut SigCtx<'_>, node: &GreenNode) -> String {
    let d = StructDecl::cast(node).expect("kind");
    let name = d.name().map(|t| ctx.text(t.span)).unwrap_or_default();
    let saved = ctx.generics.len();
    let generics = bind_generics(ctx, d.generics());
    let body = render_fields(ctx, d.fields());
    ctx.generics.truncate(saved);
    format!("struct {name}{generics} {{ {body} }}")
}

fn render_fields<'a>(ctx: &SigCtx<'_>, fields: impl Iterator<Item = StructField<'a>>) -> String {
    let parts: Vec<String> = fields
        .map(|f| {
            let vis = item_vis(f.syntax());
            let name = f.name().map(|t| ctx.text(t.span)).unwrap_or_default();
            let ty = f
                .ty()
                .map(|t| render_type(ctx, t))
                .unwrap_or_else(|| "<error>".to_string());
            format!("{}{name}: {ty}", vis.render())
        })
        .collect();
    parts.join(", ")
}

fn render_enum(ctx: &mut SigCtx<'_>, node: &GreenNode) -> String {
    let d = EnumDecl::cast(node).expect("kind");
    let name = d.name().map(|t| ctx.text(t.span)).unwrap_or_default();
    let saved = ctx.generics.len();
    let generics = bind_generics(ctx, d.generics());
    let body = render_variants(ctx, d.variants());
    ctx.generics.truncate(saved);
    format!("enum {name}{generics} {{ {body} }}")
}

fn render_variants<'a>(
    ctx: &SigCtx<'_>,
    variants: impl Iterator<Item = EnumVariant<'a>>,
) -> String {
    let parts: Vec<String> = variants
        .map(|v| {
            let name = v.name().map(|t| ctx.text(t.span)).unwrap_or_default();
            let payload: Vec<String> = v.payload().map(|t| render_type(ctx, t)).collect();
            if payload.is_empty() {
                name
            } else {
                format!("{name}({})", payload.join(", "))
            }
        })
        .collect();
    parts.join(", ")
}

fn render_trait(ctx: &mut SigCtx<'_>, node: &GreenNode) -> String {
    let d = TraitDecl::cast(node).expect("kind");
    let name = d.name().map(|t| ctx.text(t.span)).unwrap_or_default();
    let saved = ctx.generics.len();
    let generics = bind_generics(ctx, d.generics());
    let members: Vec<String> = d
        .members()
        .map(|m| match m.kind {
            SyntaxKind::FnDecl => render_fn(ctx, m),
            SyntaxKind::ConstDecl => {
                let c = ConstDecl::cast(m).expect("kind");
                let n = c.name().map(|t| ctx.text(t.span)).unwrap_or_default();
                match c.ty() {
                    Some(t) => format!("const {n}: {}", render_type(ctx, t)),
                    None => format!("const {n}"),
                }
            }
            SyntaxKind::TypeDecl => render_type_alias(ctx, m),
            _ => String::new(),
        })
        .filter(|s| !s.is_empty())
        .collect();
    ctx.generics.truncate(saved);
    format!("trait {name}{generics} {{ {} }}", members.join("; "))
}

fn render_type_alias(ctx: &mut SigCtx<'_>, node: &GreenNode) -> String {
    let d = TypeDecl::cast(node).expect("kind");
    let name = d.name().map(|t| ctx.text(t.span)).unwrap_or_default();
    let saved = ctx.generics.len();
    let generics = bind_generics(ctx, d.generics());
    let rhs = match d.def() {
        Some(def) if def.kind == SyntaxKind::StructDef => {
            let fields = render_fields(ctx, def.nodes().filter_map(StructField::cast));
            format!("struct {{ {fields} }}")
        }
        Some(def) if def.kind == SyntaxKind::EnumDef => {
            let variants = render_variants(ctx, def.nodes().filter_map(EnumVariant::cast));
            format!("enum {{ {variants} }}")
        }
        Some(def) => render_type(ctx, def),
        None => "<error>".to_string(),
    };
    ctx.generics.truncate(saved);
    format!("type {name}{generics} = {rhs}")
}

/// A path with its first segment qualified by resolution origin.
fn render_path(ctx: &SigCtx<'_>, path: &GreenNode) -> String {
    let mut segs = path
        .tokens()
        .filter(|t| t.kind == SyntaxKind::Ident)
        .map(|t| ctx.text(t.span));
    let Some(first) = segs.next() else {
        return "<error>".to_string();
    };
    let mut out = ctx.qualify(&first);
    for s in segs {
        out.push('.');
        out.push_str(&s);
    }
    out
}

/// A path rendered verbatim (row tags — structural, never resolved).
fn raw_path(ctx: &SigCtx<'_>, path: &GreenNode) -> String {
    path.tokens()
        .filter(|t| t.kind == SyntaxKind::Ident)
        .map(|t| ctx.text(t.span))
        .collect::<Vec<_>>()
        .join(".")
}

fn render_type(ctx: &SigCtx<'_>, node: &GreenNode) -> String {
    match node.kind {
        SyntaxKind::PathType => {
            let d = PathType::cast(node).expect("kind");
            let mut out = d
                .path()
                .map(|p| render_path(ctx, p.syntax()))
                .unwrap_or_else(|| "<error>".to_string());
            if let Some(args) = d.args() {
                let parts: Vec<String> = args
                    .args()
                    .map(|a| {
                        if a.kind == SyntaxKind::TypeArgPending {
                            // Expression-shaped const generic: token
                            // text, whitespace-normalized (D29 defers
                            // evaluation to s13+).
                            a.tokens()
                                .map(|t| ctx.text(t.span))
                                .collect::<Vec<_>>()
                                .join(" ")
                        } else {
                            render_type(ctx, a)
                        }
                    })
                    .collect();
                out.push_str(&format!("[{}]", parts.join(", ")));
            }
            out
        }
        SyntaxKind::ErrorUnionType => {
            let inner = node
                .nodes()
                .find(|n| is_type_kind(n.kind))
                .map(|t| render_type(ctx, t))
                .unwrap_or_else(|| "<error>".to_string());
            format!("!{inner}")
        }
        SyntaxKind::PrefixType => {
            let kw = node
                .tokens()
                .next()
                .map(|t| ctx.text(t.span))
                .unwrap_or_default();
            let inner = node
                .nodes()
                .find(|n| is_type_kind(n.kind))
                .map(|t| render_type(ctx, t))
                .unwrap_or_else(|| "<error>".to_string());
            format!("{kw} {inner}")
        }
        SyntaxKind::PtrType => {
            let inner = node
                .nodes()
                .find(|n| is_type_kind(n.kind))
                .map(|t| render_type(ctx, t))
                .unwrap_or_else(|| "<error>".to_string());
            format!("*{inner}")
        }
        SyntaxKind::DynType => {
            let p = node
                .child_node(SyntaxKind::Path)
                .map(|p| render_path(ctx, p))
                .unwrap_or_else(|| "<error>".to_string());
            format!("dyn {p}")
        }
        SyntaxKind::TupleType => {
            let parts: Vec<String> = node
                .nodes()
                .filter(|n| is_type_kind(n.kind))
                .map(|t| render_type(ctx, t))
                .collect();
            format!("({})", parts.join(", "))
        }
        SyntaxKind::FnType => {
            let parts: Vec<String> = node
                .nodes()
                .filter(|n| is_type_kind(n.kind))
                .map(|t| render_type(ctx, t))
                .collect();
            let ret = node
                .nodes()
                .find_map(RetType::cast)
                .map(|r| format!(" -> {}", render_ret(ctx, r)))
                .unwrap_or_default();
            format!("fn({}){ret}", parts.join(", "))
        }
        SyntaxKind::TypeType => "type".to_string(),
        SyntaxKind::RegionType => "region".to_string(),
        _ => "<error>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{AliasTable, MemoryLoader, load_package};

    fn pkg(files: &[(&[&str], &str, &str)]) -> Package {
        let mut ml = MemoryLoader::new("t");
        for (m, n, s) in files {
            ml.add_file(m, n, s);
        }
        load_package(&mut ml, &AliasTable::default()).expect("loads")
    }

    #[test]
    fn private_items_never_serialize() {
        let p = pkg(&[(
            &[],
            "main.lu",
            "pub fn shown() -> int { 1 }\nfn hidden() -> int { 2 }\nfn main() -> !int { shown() + hidden() }\n",
        )]);
        let ifaces = build_interfaces(&p);
        let names: Vec<&str> = ifaces[0].items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["shown"]);
    }

    #[test]
    fn round_trip_is_lossless() {
        let p = pkg(&[
            (
                &[],
                "main.lu",
                "use geometry\npub fn go(r: f64) -> f64 { geometry.area(r) }\nfn main() -> !int { 0 }\n",
            ),
            (
                &["geometry"],
                "shapes.lu",
                "pub fn area(r: f64) -> f64 { r * r }\npub(pkg) const SIDES: int = 4\n",
            ),
        ]);
        for iface in build_interfaces(&p) {
            let bytes = encode(&iface);
            let back = decode(&bytes).expect("decodes");
            assert_eq!(back, iface);
            assert_eq!(encode(&back), bytes, "re-encode is byte-identical");
        }
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode(b"WOLFX").is_err());
        assert!(decode(b"WOLFI\x01\x00\x00\x00").is_err());
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn signatures_qualify_resolved_paths() {
        let p = pkg(&[
            (
                &[],
                "main.lu",
                "use geometry\npub fn go(c: geometry.Circle) -> f64 { 0.0 }\n\
                 pub struct Wrap { inner: Local }\nstruct Local { x: int }\nfn main() -> !int { 0 }\n",
            ),
            (
                &["geometry"],
                "shapes.lu",
                "pub struct Circle { pub r: f64 }\n",
            ),
        ]);
        let ifaces = build_interfaces(&p);
        let root = ifaces.last().expect("root last");
        let go = root.items.iter().find(|i| i.name == "go").expect("go");
        // s20: heap-reaching signatures carry their derived region
        // scheme (the Cyclone defaults, made interface surface).
        assert_eq!(
            go.sig,
            "fn go(c: geometry.Circle) -> f64 \u{b7} regions (\u{3c1}1) -> -"
        );
        let wrap = root.items.iter().find(|i| i.name == "Wrap").expect("wrap");
        assert_eq!(wrap.sig, "struct Wrap { inner: self.Local }");
    }
}
