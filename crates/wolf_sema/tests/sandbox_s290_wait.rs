//! s290, the one wait (D33; `[os.ready.wait]`, `[os.ready.set]`,
//! `[os.net.write.some]`, `[os.net.peer.cred]`): the comptime sandbox
//! categories of the seven calls, which are also the capabilities a
//! package is charged for reaching them.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_one_wait_builtin_has_its_category() {
    for (name, cat) in [
        // The waits and the set are signal control over the program's
        // handles — `exec`, with `os_signal_wait_fds`.
        ("os_wait_ready", SandboxCategory::Exec),
        ("os_ready_open", SandboxCategory::Exec),
        ("os_ready_set", SandboxCategory::Exec),
        ("os_ready_wait", SandboxCategory::Exec),
        ("os_ready_close", SandboxCategory::Exec),
        // The send and the credentials are the socket family's — `net`.
        ("net_write_some", SandboxCategory::Net),
        ("net_peer_cred", SandboxCategory::Net),
        // The neighbours they join, unchanged.
        ("os_signal_wait_fds", SandboxCategory::Exec),
        ("net_wait", SandboxCategory::Net),
        ("net_write_bytes", SandboxCategory::Net),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every one-wait name is a host builtin with a signature of its arity.
#[test]
fn each_one_wait_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_wait_ready", 3),
        ("os_ready_open", 0),
        ("os_ready_set", 4),
        ("os_ready_wait", 3),
        ("os_ready_close", 1),
        ("net_write_some", 2),
        ("net_peer_cred", 1),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
