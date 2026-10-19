//! s248 (D33; wolf-lang#794–#798; ruling #93): the comptime sandbox
//! categories of what the editor asks of the runtime, which are also the
//! capabilities s217 charges a package for reaching them.

use wolf_sema::ctfe::SandboxCategory;
use wolf_sema::ctfe::intrinsics::{host_stub, pure_builtin};

#[test]
fn each_s248_builtin_has_its_category() {
    for (name, cat) in [
        // #798: the clock at nanoseconds is the clock — Clock, refused at
        // comptime and charged nothing in a manifest, as `time_now_ms`.
        ("os_clock_ns", SandboxCategory::Clock),
        // The neighbours they join, unchanged.
        ("time_now_ms", SandboxCategory::Clock),
    ] {
        assert_eq!(host_stub(name), Some(cat), "{name}");
        assert!(!pure_builtin(name), "{name} is not on the pure shelf");
    }
}

/// Every s248 name is a host builtin with a signature of its arity: the
/// table the spec's host-table block is rendered from knows each one.
#[test]
fn each_s248_builtin_has_a_signature() {
    let mut table = wolf_sema::TypeTable::new();
    for (name, arity) in [("os_clock_ns", 0)] {
        let (params, _) = wolf_sema::check::host_builtin_sig(&mut table, name)
            .unwrap_or_else(|| panic!("{name} has a signature"));
        assert_eq!(params.len(), arity, "{name}");
    }
}
