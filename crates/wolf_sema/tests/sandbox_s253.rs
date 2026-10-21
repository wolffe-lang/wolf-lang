//! s253 (D33; wolf-lang#811, #815, #819): the comptime sandbox categories
//! of the process calls, which are also the capabilities s217 charges a
//! package for reaching them (ruling asked R6, the recommendation
//! implemented).

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s253_builtin_has_its_category() {
    for (name, cat) in [
        // #819: the OS and the architecture are machine state that differs
        // per host — `env`, with `os_cpus`; refused at comptime (the
        // compiling host is not the running one).
        ("os_name", SandboxCategory::Env),
        ("os_arch", SandboxCategory::Env),
        // #811: the core limit is this process's state — `env`, with the
        // other reads; its set and the end with no core are process
        // control over how this process ends — `exec`, with
        // `os_exit_signal`.
        ("os_core_limit", SandboxCategory::Env),
        ("os_set_core_limit", SandboxCategory::Exec),
        ("os_core_off", SandboxCategory::Exec),
        // #815: an inherited descriptor made an fs handle — `fs`.
        ("fs_adopt", SandboxCategory::Fs),
        // The neighbours they join, unchanged.
        ("os_cpus", SandboxCategory::Env),
        ("os_exit_signal", SandboxCategory::Exec),
        ("fs_open", SandboxCategory::Fs),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every s253 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s253_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [
        ("os_name", 0),
        ("os_arch", 0),
        ("os_core_limit", 0),
        ("os_set_core_limit", 2),
        ("os_core_off", 0),
        ("fs_adopt", 1),
    ] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
