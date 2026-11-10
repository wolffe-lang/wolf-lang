//! s292 (D33; `[os.term.pty]`, `[os.term.size.set]`, `[os.term.attr]`): the
//! comptime sandbox categories of the pty calls, which are also the
//! capabilities a package is charged for reaching them.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s292_builtin_has_its_category() {
    for (name, cat) in [
        // A pty is made to run a program on — `exec`, with `os_pipe`.
        ("os_term_open_pty", SandboxCategory::Exec),
        // The name, the size set and the settings act on a descriptor the
        // program holds and start nothing — `io`, with `os_term_set_mode`.
        ("os_term_pty_name", SandboxCategory::Io),
        ("os_term_set_size", SandboxCategory::Io),
        ("os_term_attr", SandboxCategory::Io),
        ("os_term_set_attr", SandboxCategory::Io),
        // The neighbours they join, unchanged.
        ("os_pipe", SandboxCategory::Exec),
        ("os_term_set_mode", SandboxCategory::Io),
        ("os_term_size", SandboxCategory::Io),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every s292 name is a host builtin with a signature of its arity.
#[test]
fn each_s292_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_term_open_pty", 0),
        ("os_term_pty_name", 1),
        ("os_term_set_size", 5),
        ("os_term_attr", 2),
        ("os_term_set_attr", 3),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
