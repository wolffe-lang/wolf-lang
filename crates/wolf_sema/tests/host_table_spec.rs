//! `[os.host.sigs]` (s163, wolf-lang#181): the spec's host builtin
//! table and the checker's are ONE table. The fenced `host-table` block
//! in `spec/11-os.md` must equal, line for line and in prelude order,
//! what `host_builtin_sig` renders for every prelude name it types — so
//! a builtin that gains, loses or retags a row reds here until the
//! clause moves with it, in either direction.

use std::path::Path;
use wolf_sema::{TyId, TypeTable, check::host_builtin_sig, prelude::PRELUDE, types::render};

fn spec_block() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/11-os.md");
    let text = std::fs::read_to_string(&path).expect("spec/11-os.md reads");
    let start = text
        .find("```text host-table\n")
        .expect("spec/11 carries a ```text host-table fence");
    let body = &text[start + "```text host-table\n".len()..];
    let end = body.find("\n```").expect("the host-table fence closes");
    body[..end].lines().map(str::to_string).collect()
}

fn compiler_table() -> Vec<String> {
    let mut t = TypeTable::new();
    let unresolved = |_: u32| -> Result<TyId, &'static str> { Err("_") };
    PRELUDE
        .iter()
        .filter_map(|name| {
            let (params, ret) = host_builtin_sig(&mut t, name)?;
            let ps: Vec<String> = params.iter().map(|p| render(&t, *p, &unresolved)).collect();
            Some(format!(
                "{name}({}) -> {}",
                ps.join(", "),
                render(&t, ret, &unresolved)
            ))
        })
        .collect()
}

#[test]
fn the_spec_host_table_is_the_checker_table() {
    let spec = spec_block();
    let compiler = compiler_table();
    let only_spec: Vec<&String> = spec.iter().filter(|l| !compiler.contains(l)).collect();
    let only_compiler: Vec<&String> = compiler.iter().filter(|l| !spec.contains(l)).collect();
    assert!(
        only_spec.is_empty() && only_compiler.is_empty(),
        "[os.host.sigs] and host_builtin_sig part:\n  only in spec/11: {only_spec:#?}\n  only in the checker: {only_compiler:#?}"
    );
    assert_eq!(
        spec, compiler,
        "same lines, different order: the table follows PRELUDE's order"
    );
    assert_eq!(
        compiler.len(),
        75,
        "the host builtin count moved — say so in [os.host.sigs]"
    );
}

/// A name the table does not type has no signature here, and the
/// checker's fallback (an error type) is not a row.
#[test]
fn a_non_host_prelude_name_has_no_host_signature() {
    let mut t = TypeTable::new();
    for name in ["print", "assert", "channel", "List", "worker"] {
        assert!(host_builtin_sig(&mut t, name).is_none(), "{name}");
    }
}
