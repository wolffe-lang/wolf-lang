//! s246 (D33; wolf-lang#783, #784; ruling asked, recommendation
//! implemented): the comptime sandbox categories of one meaning's
//! disposition and of the end of this process by a signal, which are
//! also the capabilities s217 charges a package for reaching them. A
//! disposition is signal control — `exec`, with `os_signal_ignore` and
//! `os_signal_default`. Ending this process is process control — `exec`,
//! with `os_exit` (at comptime it would end the compiler).

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s246_builtin_has_its_category() {
    for (name, cat) in [
        ("os_signal_ignore_one", SandboxCategory::Exec),
        ("os_signal_default_one", SandboxCategory::Exec),
        ("os_exit_signal", SandboxCategory::Exec),
        // The neighbours they join, unchanged.
        ("os_signal_ignore", SandboxCategory::Exec),
        ("os_signal_default", SandboxCategory::Exec),
        ("os_exit", SandboxCategory::Exec),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every s246 name is a host builtin with a signature of its arity and
/// the rows the clause gives it: the table the spec's host-table block is
/// rendered from knows each one.
#[test]
fn each_s246_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_signal_ignore_one", 1),
        ("os_signal_default_one", 1),
        ("os_exit_signal", 1),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
