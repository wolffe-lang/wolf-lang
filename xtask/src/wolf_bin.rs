//! wolf-lang#1187: where the compiler xtask runs lives.
//!
//! The steps build `wolf_driver` with cargo and then run the `wolf` it made.
//! Cargo puts that binary under its target directory, which `CARGO_TARGET_DIR`,
//! `CARGO_BUILD_TARGET_DIR` or `build.target-dir` can move; a fixed
//! `target/debug/wolf` then names nothing, and a step that skipped what it
//! could not start passed having run nothing. So the directory is cargo's own
//! answer (`cargo metadata`'s `target_directory`), asked once a process.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Cargo's target directory for this workspace, as `cargo metadata` reports
/// it, which accounts for every way of moving it. Falls back to `target` (the
/// default) only when cargo cannot be asked at all, and says so.
pub fn target_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let out = Command::new(cargo)
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .output();
        match out {
            Ok(o) if o.status.success() => serde_json::from_slice::<serde_json::Value>(&o.stdout)
                .ok()
                .and_then(|v| v["target_directory"].as_str().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("target")),
            _ => {
                eprintln!(
                    "xtask: `cargo metadata` did not answer; looking for wolf under `target`"
                );
                PathBuf::from("target")
            }
        }
    })
}

/// The debug `wolf` the steps build and run.
pub fn wolf_exe() -> PathBuf {
    target_dir()
        .join("debug")
        .join(format!("wolf{}", std::env::consts::EXE_SUFFIX))
}

/// The debug `wolf`, or an error naming where it was looked for: a step that
/// cannot run the compiler fails, it never skips.
pub fn require(step: &str) -> Result<PathBuf, String> {
    let wolf = wolf_exe();
    if wolf.is_file() {
        Ok(wolf)
    } else {
        Err(format!(
            "{step}: no compiler at {} (cargo's target directory); build it with \
             `cargo build -p wolf_driver`",
            wolf.display()
        ))
    }
}
