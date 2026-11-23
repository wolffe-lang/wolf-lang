//! wolf-lang#858 (`[os.hostname]`): the machine's name is machine state
//! that differs per host — `env`, with `os_name` — and refused at comptime
//! (the compiling host is not the running one). Its signature has no
//! parameter and the row `{io, unsupported, utf8}`.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn the_host_name_is_an_env_read() {
    for (name, cat) in [
        ("os_hostname", SandboxCategory::Env),
        // The neighbour it joins, unchanged.
        ("os_name", SandboxCategory::Env),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

#[test]
fn the_host_name_has_no_parameter_and_its_row() {
    let mut table = wolf_sema::TypeTable::new();
    let unresolved = |_: u32| -> Result<wolf_sema::TyId, &'static str> { Err("_") };
    let (params, ret) = wolf_sema::check::host_builtin_sig(&mut table, "os_hostname")
        .expect("os_hostname has a signature");
    assert!(params.is_empty(), "os_hostname takes nothing");
    let shown = wolf_sema::types::render(&table, ret, &unresolved);
    assert_eq!(
        shown, "str ! {io, unsupported, utf8}",
        "os_hostname's result"
    );
}
