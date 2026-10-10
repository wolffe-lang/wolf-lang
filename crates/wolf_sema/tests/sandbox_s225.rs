//! s225 (D33; rulings #53/#54's reading): the two builtins' comptime
//! sandbox categories, pinned where a corpus row cannot reach one of
//! them. `os_exec` takes `List` arguments, which the comptime engine
//! cannot build, so its `exec` refusal is unreachable from a program
//! today (`corpus/comptime/sandbox_os_exec.lu` is a forward pin); the
//! table entry decides it the day the engine can. `env_unset` takes a
//! `str`, and `corpus/comptime/sandbox_env_unset.lu` reaches it.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::host_stub;

/// An exec replaces the program — at comptime, the compiler — so it is
/// process control, `exec` with the spawn family. Removing a variable
/// mutates the environment `env_set` writes, so it is `env` with it.
#[test]
fn each_s225_builtin_has_its_category() {
    for (name, cat) in [
        ("os_exec", SandboxCategory::Exec),
        ("env_unset", SandboxCategory::Env),
        // The neighbours they join, unchanged.
        ("os_spawn_fds", SandboxCategory::Exec),
        ("env_set", SandboxCategory::Env),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
    }
}

#[test]
fn the_reasons_a_refusal_prints() {
    assert!(
        SandboxCategory::Exec
            .reason()
            .contains("must never run or terminate programs"),
        "{}",
        SandboxCategory::Exec.reason()
    );
    assert!(
        SandboxCategory::Env
            .reason()
            .contains("environment contents differ per"),
        "{}",
        SandboxCategory::Env.reason()
    );
}
