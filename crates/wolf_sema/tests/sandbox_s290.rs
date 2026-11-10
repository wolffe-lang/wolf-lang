//! s290 (D33; `[os.proc.session]`, `[os.term.session]`): the comptime
//! sandbox categories of the session calls, which are also the
//! capabilities a package is charged for reaching them (R6, as ruled).

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s290_builtin_has_its_category() {
    for (name, cat) in [
        // A spawn is process control — `exec`, with `os_spawn_job`.
        ("os_spawn_session", SandboxCategory::Exec),
        // The move into a session is process control over this process —
        // `exec`, with `os_set_pgid`.
        ("os_setsid", SandboxCategory::Exec),
        // The session id is a read of this process — `env`, with `os_pgid`.
        ("os_sid", SandboxCategory::Env),
        // The neighbours they join, unchanged.
        ("os_spawn_job", SandboxCategory::Exec),
        ("os_set_pgid", SandboxCategory::Exec),
        ("os_pgid", SandboxCategory::Env),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every s290 name is a host builtin with a signature of its arity.
#[test]
fn each_s290_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [("os_spawn_session", 4), ("os_setsid", 0), ("os_sid", 0)] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
