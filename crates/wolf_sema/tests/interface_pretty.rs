//! Snapshot of the `wolf interface` rendering for a representative
//! corpus module (s01's interface-dump snapshot family, activated by
//! s12). Runs the real corpus case through the same entry rules the
//! driver uses: membership per D59 (plain siblings join; standalone
//! entries stay out).

use std::path::Path;

use wolf_sema::{AliasTable, DiskLoader, build_interfaces, pretty, resolve_package_with};

#[test]
fn two_mod_interface_dump() {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/resolve/two_mod/main.lu");
    let mut sm = wolf_span::SourceMap::new();
    let mut loader = DiskLoader::from_entry(&entry, &mut sm).expect("corpus entry exists");
    let res =
        resolve_package_with(&mut loader, &AliasTable::default(), true).expect("package loads");
    assert!(
        res.diagnostics.is_empty(),
        "the corpus case resolves clean: {:?}",
        res.diagnostics
    );
    let mut out = String::new();
    for (i, iface) in build_interfaces(&res.package).iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&pretty(iface));
    }
    insta::assert_snapshot!("two_mod_interfaces", out);
}
