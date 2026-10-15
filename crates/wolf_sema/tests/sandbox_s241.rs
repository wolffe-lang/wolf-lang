//! s241 (D33; ruling #79 R6, #54's reading): the comptime sandbox
//! categories of the group send, the move into a group and the host's
//! signal table, which are also the capabilities s217 charges a package
//! for reaching them. A signal to a group or to every process is process
//! control — `exec`, with `os_signal_send`. Moving this process into a
//! group changes which group every later signal and terminal hand-over
//! reaches — `exec`, with `os_set_ids`. The host's last signal number,
//! its real-time range and a signal's words read the host's table and
//! reach no ambient surface — uncharged, `os_signal_number`'s shelf, and
//! refused at comptime with it (the compiling host is not the running
//! one).

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s241_builtin_has_its_category() {
    for (name, cat) in [
        ("os_signal_send_group", SandboxCategory::Exec),
        ("os_signal_send_all", SandboxCategory::Exec),
        ("os_set_pgid", SandboxCategory::Exec),
        // The neighbours they join, unchanged.
        ("os_signal_send", SandboxCategory::Exec),
        ("os_set_ids", SandboxCategory::Exec),
        ("os_pgid", SandboxCategory::Env),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
    }
    for name in [
        "os_signal_last",
        "os_signal_realtime",
        "os_signal_text",
        "os_signal_number",
    ] {
        assert_eq!(host_stub(name), None, "{name} carries no capability");
        assert!(pure_builtin(name), "{name} is on the pure shelf");
    }
}

/// Every s241 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s241_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_signal_send_group", 2),
        ("os_signal_send_all", 1),
        ("os_set_pgid", 1),
        ("os_signal_last", 0),
        ("os_signal_realtime", 0),
        ("os_signal_text", 1),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
