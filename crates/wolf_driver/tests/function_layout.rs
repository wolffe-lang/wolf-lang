//! A bounded source addition must not reshuffle unrelated functions at link.
//!
//! Read both relocatable object symbols and final executable symbols. The
//! fixture has multiple source modules and release clusters; its added function
//! is smaller than half a page. The standalone probe can run against a released
//! compiler, so the same row demonstrates the regression before a fix exists.
//! ELF is the first witness; the platform-specific Mach-O witness follows with
//! its layout implementation rather than pretending ELF tools cover that host.

#![cfg(target_os = "linux")]

use std::path::Path;
use std::process::Command;

fn placement(tier: &str) {
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
    let output = Command::new("python3")
        .arg("-I")
        .arg(probe)
        .arg("--wolf")
        .arg(env!("CARGO_BIN_EXE_wolf"))
        .arg("--output")
        .arg(&root)
        .args(["--tier", tier])
        .output()
        .expect("python3 for the ELF symbol witness");
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
    placement("release");
}

#[test]
fn native_function_placement_is_local_under_additions() {
    placement("native");
}
