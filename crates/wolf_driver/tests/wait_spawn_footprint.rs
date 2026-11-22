//! wolf-lang#1171: a program that names a runtime wait (`time_sleep_ms`,
//! `fs_sync`, `fs_sync_dir`, the three connects, `net_wait`) but never
//! starts a task links no thread machinery for it.
//!
//! Every one of those waits runs through the runtime's compensated wait, whose worker
//! branch can start a compensation thread (`std::thread::Builder`, the
//! stack-overflow guard, and on static glibc `pthread_getattr_np` with its
//! `sscanf`). That branch runs only on a pool worker, so a program with no
//! task never takes it; it must not be linked into one either. The gate
//! builds, on the native and release tiers, one program naming each wait
//! and a control naming none.
//! It reads each binary's symbol table with `nm` and requires that a
//! waiting program define no more `spawn_unchecked` symbols than the
//! control (and, on linux, name no `pthread_getattr_np`). A control with
//! no symbol table at all fails the gate, so a stripped build can never
//! pass for a clean one. Every count is printed as a `REPORT` line.

#![cfg(unix)]

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("wait_spawn_footprint")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// `src` built as `dir/<name>/main.lu` on one tier; `None` where the host
/// cannot build the tier (a loud skip, refused under `WOLF_TEST_NATIVE_REQUIRE`).
fn build(dir: &Path, name: &str, src: &str, release: bool) -> Option<PathBuf> {
    let at = dir.join(name);
    std::fs::create_dir_all(&at).expect("program dir");
    let entry = at.join("main.lu");
    std::fs::write(&entry, src).expect("program written");
    let exe = at.join(if release { "a-release" } else { "a-native" });
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("error["),
        "wolf build {name} refused the program:\n{stderr}"
    );
    if lane_exit::environment_refusal(&out, "wolf build") {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required tier unavailable: {stderr}"
        );
        eprintln!(
            "SKIP: {name}: this host cannot build the tier: {}",
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build {name} failed: {stderr}");
    Some(exe)
}

/// `nm`'s lines for `exe`: every symbol, defined or not.
fn symbols(exe: &Path) -> Vec<String> {
    let out = Command::new("nm")
        .arg(exe)
        .output()
        .expect("nm runs (binutils on linux, the developer tools on macOS)");
    assert!(
        out.status.success(),
        "nm {}: {}",
        exe.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// (`spawn_unchecked` symbols defined, `pthread_getattr_np` named at all,
/// symbols in the table).
fn census(exe: &Path) -> (usize, usize, usize) {
    let all = symbols(exe);
    let spawn = all
        .iter()
        .filter(|l| l.contains("spawn_unchecked") && !l.trim_start().starts_with('U'))
        .count();
    let getattr = all
        .iter()
        .filter(|l| l.contains("pthread_getattr_np"))
        .count();
    (spawn, getattr, all.len())
}

const CONTROL: &str = "fn main() -> int {\n    print(\"x\")\n    0\n}\n";
const SLEEP: &str = "fn main() -> int {\n    time_sleep_ms(1)\n    print(\"x\")\n    0\n}\n";
const SYNC: &str = "fn main() -> int {\n    fs_sync(1) else |_| {}\n    print(\"x\")\n    0\n}\n";
const SYNC_DIR: &str =
    "fn main() -> int {\n    fs_sync_dir(\".\") else |_| {}\n    print(\"x\")\n    0\n}\n";
const CONNECT: &str = "fn main() -> int {\n    let c = net_connect(\"127.0.0.1:1\") else |_| { -1 }\n    print(\"x\")\n    c\n}\n";
const CONNECT_WITHIN: &str = "fn main() -> int {\n    let c = net_connect_within(\"127.0.0.1:1\", 10) else |_| { -1 }\n    print(\"x\")\n    c\n}\n";
const CONNECT_UNIX: &str = "fn main() -> int {\n    let c = net_connect_unix(\"no.sock\") else |_| { -1 }\n    print(\"x\")\n    c\n}\n";
const WAIT: &str = "fn main() -> int {\n    let r = net_wait([0], 0) else |_| { [0] }\n    print(\"x\")\n    r.len\n}\n";

fn tier(release: bool) {
    let name = if release { "release" } else { "native" };
    let dir = scratch(name);
    let Some(control) = build(&dir, "control", CONTROL, release) else {
        return;
    };
    let (c_spawn, c_getattr, c_all) = census(&control);
    eprintln!(
        "REPORT {name} control: spawn_unchecked {c_spawn}, pthread_getattr_np {c_getattr}, {c_all} symbols"
    );
    assert!(
        c_all > 0,
        "the {name} control has no symbol table: a stripped build cannot witness this"
    );
    let mut wrong = Vec::new();
    for (prog, src) in [
        ("time_sleep_ms", SLEEP),
        ("fs_sync", SYNC),
        ("fs_sync_dir", SYNC_DIR),
        ("net_connect", CONNECT),
        ("net_connect_within", CONNECT_WITHIN),
        ("net_connect_unix", CONNECT_UNIX),
        ("net_wait", WAIT),
    ] {
        let Some(exe) = build(&dir, prog, src, release) else {
            continue;
        };
        let (spawn, getattr, all) = census(&exe);
        eprintln!(
            "REPORT {name} {prog}: spawn_unchecked {spawn}, pthread_getattr_np {getattr}, {all} symbols"
        );
        if spawn > c_spawn {
            wrong.push(format!(
                "{name} {prog}: {spawn} spawn_unchecked symbols defined, the control {c_spawn}"
            ));
        }
        if cfg!(target_os = "linux") && getattr > c_getattr {
            wrong.push(format!(
                "{name} {prog}: names pthread_getattr_np {getattr} time(s), the control {c_getattr}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn naming_a_wait_links_no_thread_spawn_native() {
    tier(false);
}

#[test]
fn naming_a_wait_links_no_thread_spawn_release() {
    tier(true);
}
