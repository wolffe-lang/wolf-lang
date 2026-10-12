//! s234 (D33; #57 R7's reasoning, ruling owed R5): the comptime sandbox
//! categories of the line editor's three terminal calls, which are also
//! the capabilities s217 charges a package for reaching them. The
//! window's size and the special characters are read off a descriptor
//! the program holds and start nothing — `io`, with `os_term_mode`
//! (uncharged in a manifest, ruling #53). The wait on a descriptor and a
//! signal set is signal control — `exec`, with `os_signal_wait`.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::host_stub;

#[test]
fn each_s234_builtin_has_its_category() {
    for (name, cat) in [
        ("os_term_size", SandboxCategory::Io),
        ("os_term_chars", SandboxCategory::Io),
        ("os_signal_wait_fd", SandboxCategory::Exec),
        // The neighbours they join, unchanged.
        ("os_term_mode", SandboxCategory::Io),
        ("os_signal_wait", SandboxCategory::Exec),
        ("os_signal_poll", SandboxCategory::Exec),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
    }
}

/// Every s234 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s234_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_term_size", 1),
        ("os_term_chars", 1),
        ("os_signal_wait_fd", 2),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
