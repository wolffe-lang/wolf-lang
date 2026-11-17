//! Whole-script cache hits must not bypass a changed layout request or tier.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::path::Path;
use std::process::{Command, Output};

#[test]
fn script_cache_distinguishes_cold_selections_and_compilation_tiers() {
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "wolf_rt"])
        .status()
        .unwrap();
    assert!(status.success());
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "script-layout-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let source = root.join("policy.lu");
    std::fs::write(
        &source,
        "#!/usr/bin/env wolf\nfn helper(x: int) -> int { x }\nfn main() -> int { helper(0) }\n",
    )
    .unwrap();
    let call = |flags: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["run", "--verbose"])
            .args(flags)
            .arg(&source)
            .env("WOLF_CACHE", root.join("cache"))
            .env("HOME", &home)
            .env("SHELL", "/bin/sh")
            .output()
            .unwrap()
    };
    let good = |out: &Output| {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    good(&call(&[]));
    let selected = call(&["--cold-function", "helper"]);
    good(&selected);
    assert!(!String::from_utf8_lossy(&selected.stderr).contains("warm —"));
    let warm = call(&["--cold-function", "helper", "--cold-function", "helper"]);
    good(&warm);
    assert!(String::from_utf8_lossy(&warm.stderr).contains("warm —"));
    let missing = call(&["--cold-function", "absent"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("matched 0"));
    let release = call(&["--release", "--cold-function", "helper"]);
    good(&release);
    assert!(!String::from_utf8_lossy(&release.stderr).contains("warm —"));
    let warm = call(&["--release", "--cold-function", "helper"]);
    good(&warm);
    assert!(String::from_utf8_lossy(&warm.stderr).contains("warm —"));
}
