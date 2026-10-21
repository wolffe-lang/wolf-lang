//! s254 (D33; wolf-lang#820, #818): the comptime sandbox categories of what
//! an editor's save path, journal and clipboard met, which are also the
//! capabilities s217 charges a package for reaching them.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s254_builtin_has_its_category() {
    for (name, cat) in [
        // #820: access, the lock, the handle's record and the fifo read
        // and change the filesystem — `fs`, with the family.
        ("fs_access", SandboxCategory::Fs),
        ("fs_lock", SandboxCategory::Fs),
        ("fs_unlock", SandboxCategory::Fs),
        ("fs_fstat_record", SandboxCategory::Fs),
        ("fs_mkfifo", SandboxCategory::Fs),
        // #818: the descriptor's flag and the one write act on a handle
        // the fs family minted — `fs`, with `fs_write_chunk`.
        ("fs_set_nonblock", SandboxCategory::Fs),
        ("fs_write_some", SandboxCategory::Fs),
        // The neighbours they join, unchanged.
        ("fs_fstat", SandboxCategory::Fs),
        ("fs_write_chunk", SandboxCategory::Fs),
        ("os_signal_wait_fds", SandboxCategory::Exec),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every s254 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s254_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("fs_access", 2),
        ("fs_lock", 2),
        ("fs_unlock", 1),
        ("fs_fstat_record", 1),
        ("fs_mkfifo", 2),
        ("fs_set_nonblock", 2),
        ("fs_write_some", 2),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
