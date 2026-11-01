//! s272 (D33; wolf-lang#686, #921, #922, #975): the comptime sandbox
//! categories of the OS surface's calls, which are also the capabilities
//! s217 charges a package for reaching them (R-s272-10, the recommendation
//! implemented). `os_term_set_mode` (#976's bit) keeps its own.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s272_builtin_has_its_category() {
    for (name, cat) in [
        // #686: the file system's record reads the filesystem — `fs`, with
        // the stat family.
        ("fs_statfs", SandboxCategory::Fs),
        // #921: a change of root is process control over this process —
        // `exec`, with `os_set_ids`.
        ("os_chroot", SandboxCategory::Exec),
        // #922: the nice value read is process state — `env`, with
        // `os_core_limit`; its set is process control — `exec`, with
        // `os_set_core_limit`.
        ("os_getpriority", SandboxCategory::Env),
        ("os_setpriority", SandboxCategory::Exec),
        // #975: a meaning's action, read and never changed — `env`, with
        // `os_signal_ignored`.
        ("os_signal_action_one", SandboxCategory::Env),
        // The neighbours they join, unchanged.
        ("fs_stat", SandboxCategory::Fs),
        ("os_set_ids", SandboxCategory::Exec),
        ("os_core_limit", SandboxCategory::Env),
        ("os_set_core_limit", SandboxCategory::Exec),
        ("os_signal_ignored", SandboxCategory::Env),
        ("os_term_set_mode", SandboxCategory::Io),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every s272 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s272_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("fs_statfs", 1),
        ("os_chroot", 1),
        ("os_getpriority", 0),
        ("os_setpriority", 1),
        ("os_signal_action_one", 1),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
