//! s256 (#866, #867). Faults run only in a container or on a disposable
//! macOS CI runner. A compiler refusal is never a successful overflow.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn unavailable(out: &Output, tier: &str) -> bool {
    if !lane_exit::environment_refusal(out, tier) { return false; }
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(msg.contains("release tier targets") || msg.contains("cannot run the release tier"),
        "{tier}: compile error is not a skip: {msg}");
    true
}

#[test]
fn indexed_read_keeps_the_callers_field() {
    for lane in ["--checked", "--native", "--release"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["conform-run", "--json", lane])
            .arg(root().join("corpus/memory/read_param_indexed_caller.lu"))
            .output()
            .expect("wolf runs");
        if lane != "--checked" && unavailable(&out, lane) {
            eprintln!("SKIP: indexed_read_keeps_the_callers_field {lane}");
            continue;
        }
        assert!(out.status.success(), "{lane}: {out:?}");
        let record: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(record["verdict"], "exit(0)", "{lane}: {record}");
        assert_eq!(record["stdout_inline"], "1 1 1\n", "{lane}: {record}");
    }
}

#[cfg(unix)]
fn fault_run(dir: &Path, exe: &str, depth: &str) -> Option<Output> {
    if cfg!(target_os = "macos") {
        if std::env::var_os("GITHUB_ACTIONS").is_none() {
            eprintln!("SKIP: stack_overflow on macOS requires a disposable CI runner");
            return None;
        }
        return Some(
            Command::new("sh")
                .current_dir(dir)
                .args(["-c", "ulimit -c 0; ulimit -s 8192; \"$1\" \"$2\"", "s256"])
                .arg(format!("./{exe}"))
                .arg(depth)
                .output()
                .unwrap(),
        );
    }
    let engines = std::env::var("WOLF_JOBWAIT_ENGINE")
        .map(|e| vec![e])
        .unwrap_or_else(|_| vec!["docker".into(), "podman".into()]);
    let image = std::env::var("WOLF_JOBWAIT_IMAGE")
        .unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into());
    for engine in engines {
        if !Command::new(&engine)
            .args(["run", "--rm", "--network", "none", &image, "true"])
            .output()
            .is_ok_and(|o| o.status.success())
        {
            continue;
        }
        return Some(Command::new(engine)
            .args(["run", "--rm", "--network", "none", "--cpuset-cpus", "0-3", "-v"])
            .arg(format!("{}:/w", dir.display()))
            .args(["-w", "/w", &image, "sh", "-c",
                "ulimit -c 0; ulimit -s 8192; grep -E 'SigBlk|SigIgn' /proc/self/status; \"$1\" \"$2\"", "s256"])
            .arg(format!("./{exe}")).arg(depth).output().unwrap());
    }
    assert!(
        std::env::var_os("WOLF_JOBWAIT_REQUIRE_CONTAINER").is_none(),
        "s256 stack_overflow: required container unavailable"
    );
    eprintln!("SKIP: stack_overflow: no container ran {image}");
    None
}

#[test]
#[cfg(unix)]
fn stack_overflow_is_named_on_both_tiers_and_threads() {
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "wolf_rt"])
        .status()
        .unwrap();
    assert!(status.success());
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s256-overflow");
    for fixture in ["deep", "task"] {
        let dir = base.join(fixture);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(
            root().join(format!(
                "crates/wolf_driver/tests/fixtures/two_machines/{fixture}.lu"
            )),
            dir.join("main.lu"),
        )
        .unwrap();
        for tier in ["native", "release"] {
            let mut build = Command::new(env!("CARGO_BIN_EXE_wolf"));
            build
                .current_dir(&dir)
                .args(["build", "main.lu", "-o", tier]);
            if tier == "release" {
                build.arg("--release");
            }
            let out = build.output().unwrap();
            if unavailable(&out, tier) {
                eprintln!("SKIP: stack_overflow {tier}");
                continue;
            }
            assert!(out.status.success(), "{fixture}/{tier} build: {out:?}");
            for depth in ["200000", "250000", "300000"] {
                let Some(out) = fault_run(&dir, tier, depth) else {
                    continue;
                };
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!("s256 {fixture}/{tier}/{depth}: {:?}\n{stderr}", out.status);
                assert_eq!(
                    out.status.code(),
                    Some(134),
                    "{fixture}/{tier}/{depth}: {stderr}"
                );
                assert!(
                    stderr.lines().any(|l| l == "wolf-trap: stack-overflow"),
                    "{stderr}"
                );
            }
        }
    }
}
