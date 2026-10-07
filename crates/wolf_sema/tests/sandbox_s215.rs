//! s215 (D33, ruling #54): the four builtins' comptime sandbox categories, pinned
//! where no corpus row can reach them. `os_spawn_fds` takes `List`
//! arguments, which the comptime engine cannot build, so its `exec`
//! refusal is unreachable from a program today
//! (`corpus/comptime/sandbox_spawn_fds.lu` is a forward pin); the table
//! entry is what decides it the day the engine can, and this test holds
//! the entry and the reason a refusal would print.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::host_stub;

#[test]
fn the_spawn_with_a_map_is_exec() {
    assert_eq!(host_stub("os_spawn_fds"), Some(SandboxCategory::Exec));
    assert!(
        SandboxCategory::Exec
            .reason()
            .contains("must never run or terminate programs"),
        "{}",
        SandboxCategory::Exec.reason()
    );
}

/// Ruling #54: each builtin's category, pinned one by one — s217 charges
/// package capabilities from them, so a category is a contract.
#[test]
fn each_s215_builtin_has_its_ruled_category() {
    let want = [
        ("os_spawn_fds", SandboxCategory::Exec),
        ("os_pipe", SandboxCategory::Exec),
        ("os_chdir", SandboxCategory::Env),
        ("os_isatty", SandboxCategory::Io),
    ];
    for (name, cat) in want {
        assert_eq!(host_stub(name), Some(cat), "{name}");
    }
}

#[test]
fn the_reasons_a_refusal_prints() {
    assert!(
        SandboxCategory::Env
            .reason()
            .contains("environment contents differ per"),
        "{}",
        SandboxCategory::Env.reason()
    );
    assert!(
        SandboxCategory::Io
            .reason()
            .contains("must never act on the machine"),
        "{}",
        SandboxCategory::Io.reason()
    );
}
