//! s248 (D33; wolf-lang#794–#798; ruling #93): the comptime sandbox
//! categories of what the editor asks of the runtime, which are also the
//! capabilities s217 charges a package for reaching them.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s248_builtin_has_its_category() {
    for (name, cat) in [
        // #798: the clock at nanoseconds is the clock — Clock, refused at
        // comptime and charged nothing in a manifest, as `time_now_ms`.
        ("os_clock_ns", SandboxCategory::Clock),
        // #796, #797: the syncs and the canonical path read and flush the
        // filesystem — `fs`, with the family.
        ("fs_sync", SandboxCategory::Fs),
        ("fs_sync_dir", SandboxCategory::Fs),
        ("fs_realpath", SandboxCategory::Fs),
        // #795: the wait over several descriptors is signal control —
        // `exec`, with the wait on one.
        ("os_signal_wait_fds", SandboxCategory::Exec),
        // #794: the restore at exit acts on a descriptor the program
        // holds and starts nothing — `io`, with `os_term_set_mode`.
        ("os_term_restore_at_exit", SandboxCategory::Io),
        // The neighbours they join, unchanged.
        ("os_term_set_mode", SandboxCategory::Io),
        ("time_now_ms", SandboxCategory::Clock),
        ("fs_truncate", SandboxCategory::Fs),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every s248 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s248_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_clock_ns", 0),
        ("fs_sync", 1),
        ("fs_sync_dir", 1),
        ("fs_realpath", 1),
        ("os_signal_wait_fds", 3),
        ("os_term_restore_at_exit", 3),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
