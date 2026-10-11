//! s232 (D33; ruling #54's reading): the comptime sandbox categories of
//! the shell's signal and process calls, which are also the capabilities
//! s217 charges a package for reaching them. A signal to another
//! process is process control — `exec`, with `os_kill`. This process's
//! id, its parent's and the dispositions it holds are process context
//! read, starting nothing — `env`, with `os_pgid`. A meaning's number on
//! this host is the host's numbering, `os_error_text`'s shelf: no
//! category, still refused at comptime.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s232_builtin_has_its_category() {
    for (name, cat) in [
        ("os_signal_send", SandboxCategory::Exec),
        ("os_pid", SandboxCategory::Env),
        ("os_ppid", SandboxCategory::Env),
        ("os_signal_ignored", SandboxCategory::Env),
        // The neighbours they join, unchanged.
        ("os_kill", SandboxCategory::Exec),
        ("os_pgid", SandboxCategory::Env),
        ("os_signal_ignore", SandboxCategory::Exec),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
    }
    assert_eq!(host_stub("os_signal_number"), None);
    assert!(pure_builtin("os_signal_number"));
}

/// Every s232 name is a host builtin with a signature: the table the
/// spec's host-table block is rendered from knows each one.
#[test]
fn each_s232_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_signal_ignored", 0),
        ("os_signal_send", 2),
        ("os_signal_number", 1),
        ("os_pid", 0),
        ("os_ppid", 0),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
