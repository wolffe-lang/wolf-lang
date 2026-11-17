//! A bounded source addition must not reshuffle unrelated functions at link.
//!
//! Read both relocatable object symbols and final executable symbols. The
//! fixture has multiple source modules and release clusters; its added function
//! is smaller than half a page. The standalone probe can run against a released
//! compiler, so the same row demonstrates the regression before a fix exists.
//! ELF reads st_size; Mach-O reads symbol atom extents (including padding).
//! Both read real object and executable symbol/section tables.

#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::path::Path;
use std::process::Command;

fn placement(tier: &str, cold: bool, linker: Option<&str>, cache: bool) {
    static RUNTIME: std::sync::Once = std::sync::Once::new();
    RUNTIME.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("build the hosted runtime");
        assert!(status.success(), "hosted runtime build failed");
    });
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "function-layout-{tier}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let probe = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/layout/probe.py");
    let mut command = Command::new("python3");
    command
        .arg("-I")
        .arg(probe)
        .arg("--wolf")
        .arg(env!("CARGO_BIN_EXE_wolf"))
        .arg("--output")
        .arg(&root)
        .args(["--tier", tier]);
    if cache {
        command.arg("--check-cache");
    }
    if cold {
        command.args(["--cold-function", "m00.f00", "--cold-function", "m04.f04"]);
    }
    if let Some(linker) = linker {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        let wrapper = root.with_extension("linker");
        std::fs::write(
            &wrapper,
            format!("#!/bin/sh\nexec cc -fuse-ld={linker} \"$@\"\n"),
        )
        .unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        command.arg("--linker").arg(wrapper);
    }
    let output = command
        .output()
        .expect("python3 for the object symbol witness");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "function-placement-stability ({tier}): exit {:?}; evidence {}",
        output.status.code(),
        root.display()
    );
}

#[test]
fn release_function_placement_is_local_under_additions() {
    placement("release", false, None, false);
}

#[test]
fn native_function_placement_is_local_under_additions() {
    placement("native", false, None, false);
}

#[test]
fn cold_functions_are_in_a_separate_trailing_region_on_both_tiers() {
    for tier in ["native", "release"] {
        placement(tier, true, None, false);
    }
}

#[test]
#[cfg(target_os = "linux")]
fn explicitly_selected_gnu_and_lld_links_keep_the_same_layout_contract() {
    for linker in ["bfd", "lld"] {
        for tier in ["native", "release"] {
            placement(tier, true, Some(linker), false);
        }
    }
}

#[test]
fn removal_and_cache_preserve_placement_and_cold_policy() {
    for tier in ["native", "release"] {
        placement(tier, true, None, true);
    }
}

#[test]
fn invalid_cold_requests_are_named_errors_not_ignored() {
    let root =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cold-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let src = root.join("main.lu");
    std::fs::write(
        &src,
        "fn helper(x: int) -> int { x }\nfn main() -> int { helper(0) }\n",
    )
    .unwrap();
    for flags in [
        vec!["--cold-function"],
        vec!["--cold-function="],
        vec!["--cold-function", "absent"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .arg("build")
            .arg(&src)
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stderr).contains("--cold-function"));
    }
    std::fs::write(&src, "#[section(\".text.custom\")]\nfn helper(x: int) -> int { x }\nfn main() -> int { helper(0) }\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(&src)
        .args(["--cold-function", "helper"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("conflicts with its explicit section"));
    std::fs::write(&src, "export fn helper(x: i64) -> i64 { x }\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(&src)
        .args([
            "--target",
            "x86_64-unknown-none",
            "--emit=obj",
            "--cold-function",
            "helper",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("requires a hosted layout"));
}
