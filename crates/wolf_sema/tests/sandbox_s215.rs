//! s215 (D33): the four builtins' comptime sandbox categories, pinned
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

#[test]
fn the_pipe_chdir_and_isatty_are_io() {
    for name in ["os_pipe", "os_chdir", "os_isatty"] {
        assert_eq!(host_stub(name), Some(SandboxCategory::Io), "{name}");
    }
    assert!(
        SandboxCategory::Io
            .reason()
            .contains("must never act on the machine"),
        "{}",
        SandboxCategory::Io.reason()
    );
}
