//! s247 (D33; ruling #90, `[os.proc.fork]`; ruling asked, recommendation
//! implemented): the comptime sandbox category of the fork, which is also
//! the capability s217 charges a package for reaching it. A fork is
//! process control — `exec`, with `os_spawn` and `os_exec` (at comptime it
//! would fork the compiler).

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};
use wolf_sema::{TyId, TypeTable, check::host_builtin_sig, types::render};

#[test]
fn the_fork_has_its_category() {
    for (name, cat) in [
        ("os_fork", SandboxCategory::Exec),
        // The neighbours it joins, unchanged.
        ("os_spawn", SandboxCategory::Exec),
        ("os_exec", SandboxCategory::Exec),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// The fork is a host builtin of no parameters answering a pair, its row
/// the clause's `{busy, io, unsupported}`: the table the spec's host-table
/// block is rendered from knows it.
#[test]
fn the_fork_has_a_signature() {
    let mut table = TypeTable::new();
    let (params, ret) = host_builtin_sig(&mut table, "os_fork")
        .unwrap_or_else(|| panic!("os_fork has a signature"));
    assert!(params.is_empty(), "os_fork takes nothing");
    let unresolved = |_: u32| -> Result<TyId, &'static str> { Err("_") };
    assert_eq!(
        render(&table, ret, &unresolved),
        "(int, int) ! {busy, io, unsupported}"
    );
}
