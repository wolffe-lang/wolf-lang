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
    require_file(
        step,
        "compiler",
        wolf_exe(),
        "build it with `cargo build -p wolf_driver`",
    )
}

/// A file cargo's release profile built: `<target dir>/release/<name>`
/// (the follow-up to wolf-lang#1187: the release artifacts move with the
/// target directory too).
pub fn release(name: &str) -> PathBuf {
    target_dir().join("release").join(name)
}

/// `path` when it is a file, or an error naming it: a step whose built
/// artifact is not where cargo put it fails and says where it looked.
pub fn require_file(step: &str, what: &str, path: PathBuf, how: &str) -> Result<PathBuf, String> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{step}: no {what} at {} (cargo's target directory); {how}",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A built artifact that is not there is an error that names where it
    /// was looked for.
    #[test]
    fn a_missing_artifact_is_an_error_naming_its_path() {
        let path = PathBuf::from("/nonexistent/wolf-release-paths/release/examples/fmt_fuzz");
        let err = require_file("fmt-fuzz", "fuzzer", path, "build it")
            .expect_err("a missing file is not required");
        assert!(
            err.contains("/nonexistent/wolf-release-paths/release/examples/fmt_fuzz"),
            "the error names the path: {err}"
        );
        assert!(err.starts_with("fmt-fuzz: no fuzzer at "), "{err}");
    }

    #[test]
    fn a_release_artifact_is_under_the_release_profile_of_the_target_directory() {
        assert_eq!(
            release("libwolf_rt.a"),
            target_dir().join("release/libwolf_rt.a")
        );
    }
}
