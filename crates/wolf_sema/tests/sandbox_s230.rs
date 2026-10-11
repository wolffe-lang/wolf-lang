//! s230 (D33; ruling #54's reading): the comptime sandbox categories of
//! the calls that change a file, which are also the capabilities s217
//! charges a package for reaching them. The seven `fs_*` calls change
//! the filesystem — `fs`, refused at comptime with the family
//! (`corpus/comptime/sandbox_set_mode.lu` reaches one). `os_umask`
//! changes process state every later creation reads, as `os_chdir`
//! changes the directory every later path reads — `env`
//! (`corpus/comptime/sandbox_umask.lu`).

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::host_stub;

#[test]
fn each_s230_builtin_has_its_category() {
    for (name, cat) in [
        ("fs_set_mode", SandboxCategory::Fs),
        ("fs_open_perm", SandboxCategory::Fs),
        ("fs_create_dir_mode", SandboxCategory::Fs),
        ("fs_symlink", SandboxCategory::Fs),
        ("fs_link", SandboxCategory::Fs),
        ("fs_set_times", SandboxCategory::Fs),
        ("fs_truncate", SandboxCategory::Fs),
        ("os_umask", SandboxCategory::Env),
        // The neighbours they join, unchanged.
        ("fs_read_link", SandboxCategory::Fs),
        ("os_chdir", SandboxCategory::Env),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
    }
}

/// Every s230 name is a host builtin with a signature: the table the
/// spec's host-table block is rendered from knows each one.
#[test]
fn each_s230_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("fs_set_mode", 2),
        ("fs_open_perm", 3),
        ("fs_create_dir_mode", 2),
        ("fs_symlink", 2),
        ("fs_link", 2),
        ("fs_set_times", 6),
        ("fs_truncate", 2),
        ("os_umask", 1),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
