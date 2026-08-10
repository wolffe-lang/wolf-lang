//! The s22 audit surface (D11's greppable rings, I13 precursor):
//! per-module unsafety inventory, the `#[trusted]` roster riding the
//! `wolfi` interface hashes, and the ring-2 manifest rule (E1303 —
//! an undeclared trusted module is a build error, red-tested here
//! and in `cargo xtask audit-surface`).

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, audit, build_interfaces, resolve_package_with};

const TRUSTED_MAIN: &str = "import c \"stdlib.h\"\n\
     #[trusted(\"the scratch pointer never leaves this frame\")]\n\
     fn scratch() -> int {\n    \
         var out = 0\n    \
         // # Safety: p is live for the block and freed once.\n    \
         unsafe {\n        \
             let p = c.malloc(8) as *u8\n        \
             p[0] = 9\n        \
             out = p[0] as int\n        \
             c.free(p)\n    \
         }\n    \
         out\n\
     }\n\
     fn main() -> !int { if scratch() == 9 { 0 } else { 1 } }\n";

fn resolve(files: &[(&[&str], &str, &str)]) -> wolf_sema::Resolution {
    let mut ml = MemoryLoader::new("audit");
    for (module, name, src) in files {
        ml.add_file(module, name, src);
    }
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "audit inputs resolve clean: {:?}",
        res.diagnostics
    );
    res
}

#[test]
fn surface_inventory_snapshot() {
    // A root with the full ring inventory plus a clean submodule —
    // clean modules stay out of the report (grep finds only signal).
    let res = resolve(&[
        (&[], "main.lu", TRUSTED_MAIN),
        (&["util"], "mod.lu", "pub fn id(x: int) -> int { x }\n"),
    ]);
    let s = audit::surface(&res.package);
    insta::assert_snapshot!("audit_surface_inventory", audit::render(&s));
}

#[test]
fn declared_trusted_module_audits_clean() {
    let res = resolve(&[(&[], "main.lu", TRUSTED_MAIN)]);
    let diags = audit::manifest_check(&res.package, Some("# stub\ntrusted = root\n"));
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn e1303_undeclared_trusted_module_is_a_build_error() {
    // The red half of the ring-2 rule: a manifest without the entry
    // (and no manifest at all) both report E1303 at the trusted fn.
    let res = resolve(&[(&[], "main.lu", TRUSTED_MAIN)]);
    let diags = audit::manifest_check(&res.package, Some("# stub — no trusted entry\n"));
    assert_eq!(diags.len(), 1);
    assert!(audit::manifest_check(&res.package, None).len() == 1);
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let rendered = render_human(&diags[0], &sources, &RenderOptions::default());
    insta::assert_snapshot!("audit_e1303_undeclared", rendered);
}

#[test]
fn manifest_stub_parser() {
    let m = "# comment\ntrusted = root, net.io\n trusted=util \nother = 1\n";
    assert_eq!(
        audit::manifest_trusted(m),
        vec!["net.io".to_string(), "root".to_string(), "util".to_string()]
    );
    assert!(audit::manifest_trusted("# nothing\n").is_empty());
}

#[test]
fn trusted_roster_rides_both_interface_hashes() {
    // Supply-chain surface: the roster (and its obligation TEXT) is
    // hashed — a dependency editing either is an interface change,
    // even though `scratch` is private and no `pub` item moved.
    let hash_of = |src: &str| {
        let res = resolve(&[(&[], "main.lu", src)]);
        let iface = &build_interfaces(&res.package)[0];
        (iface.export_hash, iface.pkg_hash, iface.trusted.clone())
    };
    let (e1, p1, roster) = hash_of(TRUSTED_MAIN);
    assert_eq!(
        roster,
        vec![(
            "scratch".to_string(),
            "the scratch pointer never leaves this frame".to_string()
        )]
    );
    let retext = TRUSTED_MAIN.replace("never leaves", "sometimes leaves");
    let (e2, p2, _) = hash_of(&retext);
    assert_ne!(e1, e2, "the obligation text is export surface");
    assert_ne!(p1, p2);
    let untrusted = TRUSTED_MAIN.replace(
        "#[trusted(\"the scratch pointer never leaves this frame\")]\n",
        "",
    );
    let (e3, _, roster3) = hash_of(&untrusted);
    assert!(roster3.is_empty());
    assert_ne!(e1, e3, "dropping the mark is an interface change too");
}
