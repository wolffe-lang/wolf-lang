//! s277 (D33; wolf-lang#707, #1029, #1030, #1031, #1033): the comptime
//! sandbox categories of the descriptor walk's calls, which are also the
//! capabilities s217 charges a package for reaching them (R-s277-10, the
//! recommendation implemented): every one `fs`, with the family.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

const NAMES: [(&str, usize); 10] = [
    ("fs_open_at", 4),
    ("fs_stat_at", 2),
    ("fs_lstat_at", 2),
    ("fs_remove_at", 2),
    ("fs_remove_dir_at", 2),
    ("fs_create_dir_at", 3),
    ("fs_symlink_at", 3),
    ("fs_link_at", 5),
    ("fs_read_dir_entries_at", 1),
    ("fs_set_mode_fd", 2),
];

#[test]
fn each_s277_builtin_is_fs() {
    for (name, _) in NAMES {
        assert_eq!(host_stub(name), Some(SandboxCategory::Fs), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
    // The neighbours they join, unchanged.
    for name in [
        "fs_open_perm",
        "fs_lstat",
        "fs_set_owner_fd",
        "fs_read_dir_entries",
    ] {
        assert_eq!(host_stub(name), Some(SandboxCategory::Fs), "{name}");
    }
}

/// Every s277 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s277_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in NAMES {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
