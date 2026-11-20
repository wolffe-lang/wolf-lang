//! wolf-std#156 (D33): the comptime sandbox categories of the bounded
//! inventory calls, which are also the capabilities s217 charges a package
//! for reaching them: every one `fs`, with the family.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

const NAMES: [(&str, usize); 6] = [
    ("fs_dir_cursor_open", 5),
    ("fs_dir_cursor_next", 1),
    ("fs_dir_cursor_close", 1),
    ("fs_lstat_component", 2),
    ("fs_open_component", 5),
    ("fs_read_link_component", 5),
];

#[test]
fn each_inventory_builtin_is_fs() {
    for (name, _) in NAMES {
        assert_eq!(host_stub(name), Some(SandboxCategory::Fs), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every name is a host builtin with a signature of its arity, and the two
/// marks it adds are declared only where they are produced.
#[test]
fn each_inventory_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in NAMES {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
