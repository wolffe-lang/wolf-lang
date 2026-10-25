//! s261: invocation names and non-UTF-8 argv on the real native tiers
//! and on the checked machine (conform-run --checked, never wolf run).
//! Local runs belong inside the lane's own container; CI's runner is
//! disposable. No signals, process groups or host process table probes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Pool runs set an image with cc and clang (the lane's own image).
/// Re-exec ONLY this test in its own network-less PID namespace. CI's
/// disposable hosted runners need no host opt-in and execute normally.
fn isolated(test: &str) -> bool {
    if !cfg!(target_os = "linux") || std::env::var_os("WOLF_ARGV_IN_CONTAINER").is_some() {
        return false;
    }
    let Some(image) = std::env::var_os("WOLF_ARGV_IMAGE") else {
        assert_ne!(
            std::env::var("WOLF_ARGV_REQUIRE_CONTAINER").as_deref(),
            Ok("1"),
            "WOLF_ARGV_IMAGE is required for argv witnesses on the pool"
        );
        return false;
    };
    let engine = std::env::var("WOLF_ARGV_ENGINE").unwrap_or_else(|_| "podman".into());
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut cmd = Command::new(engine);
    cmd.args(["run", "--rm", "--network", "none"]);
    if let Ok(cpus) = std::env::var("WOLF_ARGV_CPUSET") {
        cmd.args(["--cpuset-cpus", &cpus]);
    }
    cmd.arg("-v")
        .arg(format!("{0}:{0}", workspace.display()))
        .arg("-w")
        .arg(workspace)
        .args(["-e", "WOLF_ARGV_IN_CONTAINER=1"])
        .arg(image)
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"]);
    let out = cmd.output().expect("argv witness container starts");
    eprint!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "isolated argv test {test}: {:?}",
        out.status
    );
    true
}

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}
fn root(case: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("s261-{case}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn source(dir: &Path, text: &str) -> PathBuf {
    let file = dir.join("main.lu");
    std::fs::write(&file, text).unwrap();
    file
}
fn checked(file: &Path, argv: &[&[u8]]) -> (serde_json::Value, Output) {
    let mut cmd = Command::new(wolf());
    cmd.args(["conform-run", "--json", "--checked"]).arg(file);
    for arg in argv {
        cmd.arg(format!("--argv-hex={}", hex(arg)));
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "checked: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let record = serde_json::from_slice(&out.stdout).expect("one record");
    (record, out)
}
fn build(file: &Path, release: bool) -> PathBuf {
    let binary = file.parent().unwrap().join(if cfg!(windows) {
        "program.exe"
    } else {
        "program"
    });
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(file)
        .arg("--no-cache")
        .arg("-o")
        .arg(&binary);
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "build release={release}, status={:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    binary
}
fn success(out: &Output, expected: &[u8]) {
    assert!(
        out.status.success(),
        "status={:?}, stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, expected);
}

#[test]
fn native_utf8_and_empty_arguments_agree_on_every_host() {
    if isolated("native_utf8_and_empty_arguments_agree_on_every_host") {
        return;
    }
    let dir = root("native-portable");
    let file = source(
        &dir,
        "fn main() -> !int {\n    let text = env_args()\n    let argv = env_args_bytes()\n    let name = str_from_utf8(argv[0])?\n    let first = str_from_utf8(argv[1])?\n    let same = name == env_arg0() && first == text[0]\n    print(\"{same} {text.len} {argv.len} {text[1].len} {argv[2].len}\")\n    0\n}\n",
    );
    let binary = build(&file, false);
    success(
        &Command::new(binary).args(["snow é", ""]).output().unwrap(),
        b"true 2 3 0 0\n",
    );
}

#[test]
fn checked_byte_argv_and_text_traps_are_exact() {
    if isolated("checked_byte_argv_and_text_traps_are_exact") {
        return;
    }
    let dir = root("checked");
    let file = source(
        &dir,
        "fn main() {\n    let a = env_args_bytes()\n    for arg in a {\n        for b in arg { print(\"{b as int}\") }\n        print(\"end\")\n    }\n}\n",
    );
    let (rec, _) = checked(&file, &[b"sh", b"x\xffy", b"", "é".as_bytes()]);
    assert_eq!(rec["verdict"], "exit(0)");
    assert_eq!(
        rec["stdout_inline"],
        "115\n104\nend\n120\n255\n121\nend\nend\n195\n169\nend\n"
    );
    source(&dir, "fn main() { print(\"{env_args().len}\") }\n");
    let (rec, _) = checked(&file, &[b"sh", b"ok", b"x\xffy"]);
    assert_eq!(rec["verdict"], "trap(argument-utf8)");
    assert_eq!(
        rec["trap_message"],
        "argument 2 is not UTF-8; read it with env_args_bytes()"
    );
    let (rec, _) = checked(&file, &[b"\xff", b"ok"]);
    assert_eq!(
        rec["stdout_inline"], "1\n",
        "env_args must not decode argv[0]"
    );
    source(&dir, "fn main() { print(env_arg0()) }\n");
    let (rec, _) = checked(&file, &[b"\xff", b"ok"]);
    assert_eq!(rec["verdict"], "trap(argument-utf8)");
    assert_eq!(
        rec["trap_message"],
        "argument 0 is not UTF-8; read it with env_args_bytes()"
    );
    let (rec, _) = checked(&file, &[b"sh", b"\xff"]);
    assert_eq!(
        rec["stdout_inline"], "sh\n",
        "env_arg0 must not decode later arguments"
    );
    let (rec, _) = checked(&file, &[]);
    assert_eq!(rec["stdout_inline"], "\n", "absent name is empty text");
    source(&dir, "fn main() { print(\"{env_args_bytes().len}\") }\n");
    let (rec, _) = checked(&file, &[]);
    assert_eq!(rec["stdout_inline"], "0\n");
}

#[test]
fn checked_argv_option_rejects_invalid_and_wrong_lane_inputs() {
    if isolated("checked_argv_option_rejects_invalid_and_wrong_lane_inputs") {
        return;
    }
    let dir = root("option");
    let file = source(&dir, "fn main() {}\n");
    for flags in [
        vec!["--checked", "--argv-hex=f"],
        vec!["--checked", "--argv-hex=gg"],
        vec!["--checked", "--argv-hex=+1"],
        vec!["--checked", "--argv-hex=00"],
        vec!["--checked", "--argv-hex=é"],
        vec!["--argv-hex="],
        vec!["--native", "--argv-hex=61"],
        vec!["--checked", "--release", "--argv-hex=61"],
    ] {
        let out = Command::new(wolf())
            .arg("conform-run")
            .arg(&file)
            .args(&flags)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "flags={flags:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("--argv-hex"));
    }
}

#[test]
fn program_name_cannot_escape_its_region() {
    if isolated("program_name_cannot_escape_its_region") {
        return;
    }
    let dir = root("region");
    let file = source(
        &dir,
        "fn name() -> str {\n    region scratch { env_arg0() }\n}\nfn main() { print(name()) }\n",
    );
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg("--phase=mem")
        .arg("--json")
        .arg(file)
        .output()
        .unwrap();
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rec["verdict"], "fail(E1010)");
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::os::unix::process::CommandExt;

    // The checked machine has no executable image of its own. A test
    // subprocess captures the kernel's argv and supplies that exact argv
    // to the explicit machine input. It is invoked through the SAME
    // five invocation forms as each native executable below.
    #[test]
    fn checked_invocation_shim() {
        let Some(file) = std::env::var_os("WOLF_S261_SHIM") else {
            return;
        };
        let name = std::env::args_os().next().unwrap();
        let (rec, _) = checked(Path::new(&file), &[name.as_bytes()]);
        assert_eq!(rec["verdict"], "exit(0)");
        assert_eq!(
            rec["stdout_inline"],
            format!("{}\n", std::env::var("WOLF_S261_EXPECT").unwrap())
        );
    }

    fn invocations(dir: &Path, binary: &Path, shim: bool, source: &Path) {
        let link = dir.join("hound");
        if std::fs::symlink_metadata(&link).is_ok() {
            std::fs::remove_file(&link).unwrap();
        }
        std::os::unix::fs::symlink(binary, &link).unwrap();
        let basename = binary.file_name().unwrap().to_str().unwrap();
        for (command, expected, renamed) in [
            (basename.to_string(), basename.to_string(), false),
            (
                binary.display().to_string(),
                binary.display().to_string(),
                false,
            ),
            (format!("./{basename}"), format!("./{basename}"), false),
            ("./hound".to_string(), "./hound".to_string(), false),
            (binary.display().to_string(), "NAME".to_string(), true),
        ] {
            let mut cmd = if renamed {
                let mut c = Command::new("bash");
                c.args(["-c", "exec -a NAME \"$@\"", "s261"]).arg(&command);
                c
            } else {
                Command::new(&command)
            };
            cmd.current_dir(dir).env(
                "PATH",
                format!(
                    "{}:{}",
                    dir.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
            if shim {
                cmd.args(["--exact", "unix::checked_invocation_shim", "--nocapture"])
                    .env("WOLF_S261_SHIM", source)
                    .env("WOLF_S261_EXPECT", &expected);
                let out = cmd.output().unwrap();
                assert!(
                    out.status.success(),
                    "checked invocation {expected}: {} {}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
            } else {
                success(&cmd.output().unwrap(), format!("{expected}\n").as_bytes());
            }
            eprintln!(
                "s261 {} argv0={expected:?}: PASS",
                if shim {
                    "checked"
                } else if dir.ends_with("s261-release") {
                    "release"
                } else {
                    "native"
                }
            );
        }
    }

    #[test]
    fn invocation_name_is_not_the_resolved_executable_on_all_tiers() {
        if isolated("unix::invocation_name_is_not_the_resolved_executable_on_all_tiers") {
            return;
        }
        for (tier, release) in [("native", false), ("release", true)] {
            let dir = root(tier);
            let file = source(&dir, "fn main() { print(env_arg0()) }\n");
            let binary = build(&file, release);
            invocations(&dir, &binary, false, &file);
        }
        let dir = root("checked-name");
        let file = source(&dir, "fn main() { print(env_arg0()) }\n");
        let shim = dir.join("program");
        std::fs::copy(std::env::current_exe().unwrap(), &shim).unwrap();
        invocations(&dir, &shim, true, &file);
    }

    #[test]
    fn non_utf8_arguments_survive_or_trap_without_rust_panics() {
        if isolated("unix::non_utf8_arguments_survive_or_trap_without_rust_panics") {
            return;
        }
        let bad = std::ffi::OsString::from_vec(b"x\xffy".to_vec());
        for (tier, release) in [("native-bytes", false), ("release-bytes", true)] {
            let dir = root(tier);
            let file = source(
                &dir,
                "fn main() {\n    let a = env_args_bytes()\n    let b = a[1]\n    print(\"{a.len} {b.len} {b[0] as int} {b[1] as int} {b[2] as int}\")\n}\n",
            );
            let binary = build(&file, release);
            success(
                &Command::new(&binary).arg(&bad).output().unwrap(),
                b"2 3 120 255 121\n",
            );
            // wolf run must not panic or corrupt its forwarded suffix.
            let mut cmd = Command::new(wolf());
            cmd.arg("run");
            if release {
                cmd.arg("--release");
            }
            success(
                &cmd.arg(&file).arg(&bad).output().unwrap(),
                b"2 3 120 255 121\n",
            );
            source(&dir, "fn main() { print(\"{env_args().len}\") }\n");
            let binary = build(&file, release);
            let out = Command::new(&binary).arg("ok").arg(&bad).output().unwrap();
            assert_eq!(out.status.code(), Some(134));
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(err.contains("wolf-trap: argument-utf8\n"), "{err}");
            assert!(
                err.contains("argument 2 is not UTF-8; read it with env_args_bytes()"),
                "{err}"
            );
            assert!(!err.contains("panicked"), "{err}");
            source(&dir, "fn main() { print(env_arg0()) }\n");
            let binary = build(&file, release);
            let out = Command::new(&binary).arg0(&bad).output().unwrap();
            assert_eq!(out.status.code(), Some(134));
            assert!(String::from_utf8_lossy(&out.stderr).contains("argument 0 is not UTF-8"));
            // The byte reader retains a non-UTF-8 name too.
            source(
                &dir,
                "fn main() {\n    let a = env_args_bytes()\n    let b = a[0]\n    print(\"{b.len} {b[1] as int}\")\n}\n",
            );
            let binary = build(&file, release);
            success(
                &Command::new(&binary).arg0(&bad).output().unwrap(),
                b"3 255\n",
            );
            eprintln!("s261 {tier}: invalid arguments/name preserved or named trap, PASS");
        }
    }

    /// Run `binary` with `args` on a fresh pseudo-terminal (its own
    /// session, the terminal its controlling one) and read the terminal
    /// until the program is gone: the path an interactive user takes.
    fn pty_run(binary: &Path, args: &[&std::ffi::OsStr]) -> (std::process::ExitStatus, String) {
        use std::io::Read as _;
        use std::os::fd::FromRawFd as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        use std::process::Stdio;
        // SAFETY: pty calls on descriptors this test owns; the child's
        // hook makes only async-signal-safe calls.
        unsafe {
            let m = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(m >= 0, "posix_openpt");
            assert_eq!(libc::grantpt(m), 0, "grantpt");
            assert_eq!(libc::unlockpt(m), 0, "unlockpt");
            let name = std::ffi::CStr::from_ptr(libc::ptsname(m))
                .to_string_lossy()
                .into_owned();
            let mut master = std::fs::File::from_raw_fd(m);
            let slave = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY)
                .open(&name)
                .expect("open the pty's slave");
            let reader = std::thread::spawn(move || {
                let mut text = Vec::new();
                let mut b = [0u8; 4096];
                loop {
                    match master.read(&mut b) {
                        Ok(0) | Err(_) => return text,
                        Ok(n) => text.extend_from_slice(&b[..n]),
                    }
                }
            });
            let mut cmd = Command::new(binary);
            cmd.args(args)
                .stdin(Stdio::from(slave.try_clone().expect("dup")))
                .stdout(Stdio::from(slave.try_clone().expect("dup")))
                .stderr(Stdio::from(slave));
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
            let status = cmd.status().expect("run under the pty");
            // The last slave copy goes with the command; the master then
            // reads the end of the transcript.
            drop(cmd);
            let text = reader.join().expect("pty reader");
            (status, String::from_utf8_lossy(&text).replace('\r', ""))
        }
    }

    /// wolf-lang#923 (scent#8, ct03): on macOS 0.2.35's native process
    /// aborted in Rust's `env::args` (`called Result::unwrap() on an Err
    /// value: "\xFF"`, SIGABRT) before the program could read a byte
    /// argument. `[os.env.argv]` says what the text reader owes instead,
    /// and s261 made the runtime do it: the trap by index, exit 134, no
    /// Rust panic. Asked here through a terminal, as a user meets it, on
    /// both native tiers; the byte reader beside it keeps the byte.
    #[test]
    fn a_terminal_invocation_with_a_non_utf8_argument_traps_by_index() {
        if isolated("unix::a_terminal_invocation_with_a_non_utf8_argument_traps_by_index") {
            return;
        }
        let bad = std::ffi::OsString::from_vec(b"\xff".to_vec());
        let ok = std::ffi::OsStr::new("-F");
        for (tier, release) in [("pty-native", false), ("pty-release", true)] {
            let dir = root(tier);
            let file = source(&dir, "fn main() { print(\"{env_args().len}\") }\n");
            let binary = build(&file, release);
            let (status, text) = pty_run(&binary, &[ok, &bad]);
            assert_eq!(status.code(), Some(134), "{tier}: {status:?} {text:?}");
            assert!(
                text.contains("wolf-trap: argument-utf8\n"),
                "{tier}: {text:?}"
            );
            assert!(
                text.contains("argument 2 is not UTF-8; read it with env_args_bytes()"),
                "{tier}: {text:?}"
            );
            assert!(!text.contains("panicked"), "{tier}: {text:?}");
            assert!(!text.contains("fatal runtime error"), "{tier}: {text:?}");
            source(
                &dir,
                "fn main() {\n    let a = env_args_bytes()\n    print(\"{a.len} {a[2].len} {a[2][0] as int}\")\n}\n",
            );
            let binary = build(&file, release);
            let (status, text) = pty_run(&binary, &[ok, &bad]);
            assert!(status.success(), "{tier}: {status:?} {text:?}");
            assert_eq!(text, "3 1 255\n", "{tier}");
            eprintln!(
                "s266 {tier}: a terminal's byte argument traps by index or reads as bytes, PASS"
            );
        }
    }

    #[test]
    fn non_utf8_environment_keeps_its_existing_rows() {
        if isolated("unix::non_utf8_environment_keeps_its_existing_rows") {
            return;
        }
        for (tier, release) in [
            ("native-env", false),
            ("release-env", true),
            ("checked-env", false),
        ] {
            let dir = root(tier);
            let file = source(
                &dir,
                "fn main() {\n    let v = env_get(\"WOLF_S261_BAD\") else |e| match e { utf8 => \"utf8\", missing => \"wrong\" }\n    print(v)\n    var count = 0\n    for kv in env_vars() {\n        if kv.starts_with(\"WOLF_S261_BAD=\") { count = count + 1 }\n    }\n    print(\"{count}\")\n}\n",
            );
            let mut cmd = if tier == "checked-env" {
                let mut c = Command::new(wolf());
                c.args(["conform-run", "--checked", "--json"]).arg(&file);
                c
            } else {
                Command::new(build(&file, release))
            };
            let out = cmd
                .env("WOLF_S261_BAD", std::ffi::OsString::from_vec(vec![255]))
                .output()
                .unwrap();
            if tier == "checked-env" {
                let rec: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                assert_eq!(rec["verdict"], "exit(0)");
                assert_eq!(rec["stdout_inline"], "utf8\n0\n");
            } else {
                success(&out, b"utf8\n0\n");
            }
            eprintln!("s261 {tier}: env_get=utf8, env_vars omits entry, PASS");
        }
    }
}
