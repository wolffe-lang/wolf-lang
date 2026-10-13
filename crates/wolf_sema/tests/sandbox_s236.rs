//! s236 (D33; rulings #54, #67 R8 and #74's reasoning; ruling owed R5):
//! the comptime sandbox categories of the id and owner calls, which are
//! also the capabilities s217 charges a package for reaching them.
//! Reading who the process runs as is process state read and nothing
//! started — `env`, with `os_pid`. A file's owner is a change to the
//! filesystem — `fs`, with `fs_set_mode`. Becoming another user is
//! process control that changes what every later spawn, exec and signal
//! may do — `exec`, with `os_exec` and `os_signal_send`.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::host_stub;

#[test]
fn each_s236_builtin_has_its_category() {
    for (name, cat) in [
        ("os_uid", SandboxCategory::Env),
        ("os_euid", SandboxCategory::Env),
        ("os_gid", SandboxCategory::Env),
        ("os_egid", SandboxCategory::Env),
        ("os_groups", SandboxCategory::Env),
        ("os_set_ids", SandboxCategory::Exec),
        ("fs_set_owner", SandboxCategory::Fs),
        ("fs_set_owner_fd", SandboxCategory::Fs),
        // The neighbours they join, unchanged.
        ("os_pid", SandboxCategory::Env),
        ("os_exec", SandboxCategory::Exec),
        ("fs_set_mode", SandboxCategory::Fs),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
    }
}

/// Every s236 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s236_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_uid", 0),
        ("os_euid", 0),
        ("os_gid", 0),
        ("os_egid", 0),
        ("os_groups", 0),
        ("os_set_ids", 3),
        ("fs_set_owner", 4),
        ("fs_set_owner_fd", 3),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
