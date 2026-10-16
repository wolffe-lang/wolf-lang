//! s242 (D33; wolf-lang#680, ruling owed, #54's reading): the comptime
//! sandbox category of the poll of the children, which is also the
//! capability s217 charges a package for reaching it. The poll reaps a
//! child that ended, as `os_wait_status` does — process control, `exec`,
//! with it — and is refused at comptime (a child is the running
//! program's, never the compiler's).

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn the_poll_is_exec_with_the_wait() {
    for (name, cat) in [
        ("os_wait_poll", SandboxCategory::Exec),
        // The neighbours it joins, unchanged.
        ("os_wait_status", SandboxCategory::Exec),
        ("os_wait", SandboxCategory::Exec),
        ("os_signal_wait", SandboxCategory::Exec),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
    }
    assert!(
        !pure_builtin("os_wait_poll"),
        "the poll is not on the pure shelf"
    );
}

/// The poll takes nothing and answers a pair or `io`: the table the
/// spec's host-table block is rendered from knows it.
#[test]
fn the_poll_has_its_signature() {
    let mut table = wolf_sema::TypeTable::new();
    let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, "os_wait_poll")
        .expect("os_wait_poll has a signature");
    assert!(params.is_empty());
}
