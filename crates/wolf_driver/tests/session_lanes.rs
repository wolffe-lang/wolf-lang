//! s290 (`[os.proc.session]`, `[os.term.session]`) — a child started in
//! a new session, and this process into a session of its own, on the
//! checked machine (`conform-run --json --checked`) and as native and
//! release binaries, linux also under `taskset` on four CPUs.
//!
//! Every live row runs on a pseudo-terminal this test opens, so the
//! program has a controlling terminal to leave: the test's child leads a
//! new session there and takes the terminal (`setsid`, `TIOCSCTTY`), as a
//! login shell is started. Nothing here signals a process, and nothing
//! moves any process but the test's own children, so the rows run on the
//! host itself: a linux gauntlet's container, CI's macOS runner.
//!
//! - `a_spawned_child_leads_a_new_session` (`fixtures/session/spawn.lu`,
//!   its child `child.lu` built natively): the child's session, group and
//!   process ids are one number, `/dev/tty` fails with 6 in it, the map
//!   (its stdout onto a pipe) and the defaults (`QUIT` set back) are
//!   placed, and the parent's session and terminal are unchanged.
//! - `this_process_into_a_session_of_its_own` (`mover.lu`): as the
//!   session's leader the call is `denied` with 1 and nothing moves; as a
//!   child of the leader's shell it moves, and after it `/dev/tty` fails
//!   with 6, a second call is `denied` and the old terminal's foreground
//!   is `io`.
//! - `a_forked_child_moves_from_a_task_and_its_restore_restores`
//!   (`forked.lu`, native and release: the checked machine has no fork).
//! - `lupin_does_not_resolve_the_session_calls_before_its_mirror`: the
//!   corpus rows on a sibling lupin, pinned by version.
//!
//! At the base every fixture is E0301 at `wolf build` (exit 2): a red,
//! never a skip.

use std::path::{Path, PathBuf};
use std::process::Command;

#[allow(dead_code)]
mod test_cpus;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace")
        .to_path_buf()
}

/// The sibling lupin, found exactly as `pairing.rs` finds it.
fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The lupin releases that predate wolf-interp's mirror of s290: each
/// answers a program naming the three calls "does not resolve".
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.60"];

/// lupin's record: verdict, by-name reason, version.
fn record_fields(text: &str) -> (String, String, String) {
    text.lines()
        .rev()
        .find_map(|l| {
            l.find('{').and_then(|i| {
                serde_json::from_str::<serde_json::Value>(l[i..].trim())
                    .ok()
                    .map(|v| {
                        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
                        (s("verdict"), s("x-unsupported"), s("impl_version"))
                    })
            })
        })
        .unwrap_or_default()
}

#[test]
fn lupin_does_not_resolve_the_session_calls_before_its_mirror() {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return;
    };
    let entry = workspace().join("corpus/os/session_rows.lu");
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(&entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (verdict, reason, version) = record_fields(&text);
    eprintln!("s290 lupin {version}: {verdict} ({reason})");
    if PRE_MIRROR_LUPIN.contains(&version.as_str()) {
        assert_eq!(verdict, "unsupported", "lupin {version}: {text}");
        assert!(
            reason.contains("does not resolve"),
            "lupin {version} predates the mirror: {reason}"
        );
    } else {
        // The mirror: the shape rows are its own, and the first call past
        // its shape (the forged source's) is refused by name.
        assert_eq!(verdict, "unsupported", "lupin {version}: {text}");
        assert!(
            reason.contains("os_spawn_session"),
            "lupin {version} refuses the spawn by name: {reason}"
        );
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::process::CommandExt as _;

    fn fixture(name: &str) -> PathBuf {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/session")
            .join(name);
        assert!(p.is_file(), "fixture missing: {}", p.display());
        p
    }

    fn scratch(test: &str) -> PathBuf {
        let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("session_lanes")
            .join(test);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("scratch dir");
        d
    }

    /// `wolf build` of a fixture into `dir`; a refusal (E0301 at the base:
    /// exit 2) is a failure, never a skip.
    fn build(entry: &Path, dir: &Path, release: bool) -> PathBuf {
        let stem = entry.file_stem().unwrap().to_string_lossy().into_owned();
        let bin = dir.join(format!("{stem}{}", if release { "-release" } else { "" }));
        let mut cmd = Command::new(wolf());
        cmd.arg("build")
            .arg(entry)
            .arg("--no-cache")
            .arg("-o")
            .arg(&bin);
        if release {
            cmd.arg("--release");
        }
        let out = cmd.output().expect("wolf build runs");
        assert!(
            out.status.success(),
            "wolf build {} (release={release}) exit {:?}: {}",
            entry.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        bin
    }

    /// A machine: its name and the argv that runs a fixture on it (the
    /// fixture's path is the last word for the checked machine).
    struct Lane {
        name: String,
        argv: Vec<String>,
        checked: bool,
    }

    /// The checked machine, native, release, and native under `taskset`
    /// (linux) for `entry`.
    fn lanes(entry: &Path, dir: &Path, checked: bool) -> Vec<Lane> {
        let mut out = Vec::new();
        if checked {
            out.push(Lane {
                name: "checked".into(),
                argv: vec![
                    wolf().into(),
                    "conform-run".into(),
                    "--json".into(),
                    "--checked".into(),
                    entry.to_string_lossy().into_owned(),
                ],
                checked: true,
            });
        }
        for (tier, release) in [("native", false), ("release", true)] {
            let bin = build(entry, dir, release).to_string_lossy().into_owned();
            out.push(Lane {
                name: tier.into(),
                argv: vec![bin.clone()],
                checked: false,
            });
            if release {
                continue;
            }
            if let Some(pin) = test_cpus::pin(4, module_path!()) {
                let mut argv = pin.argv();
                argv.push(bin);
                out.push(Lane {
                    name: format!("{tier} {}", pin.label()),
                    argv,
                    checked: false,
                });
            }
        }
        out
    }

    /// Run `argv` in `dir` on a fresh pseudo-terminal, as the leader of a
    /// new session whose controlling terminal it is, and read the
    /// terminal until the last holder of its slave is gone.
    fn pty_run(argv: &[String], dir: &Path) -> (std::process::ExitStatus, String) {
        use std::io::Read as _;
        use std::os::fd::FromRawFd as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        use std::process::Stdio;
        // SAFETY: pty calls on descriptors this test owns; the child's hook
        // makes only async-signal-safe calls.
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
            let mut cmd = Command::new(&argv[0]);
            cmd.args(&argv[1..])
                .current_dir(dir)
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

    /// What the program printed: the checked machine's record's
    /// `stdout_inline` (its `print` stream is a buffer), a binary's
    /// terminal transcript otherwise.
    fn printed(lane: &Lane, transcript: &str) -> String {
        if !lane.checked {
            return transcript.to_string();
        }
        let start = transcript
            .find("{\"")
            .unwrap_or_else(|| panic!("{}: no record on the terminal:\n{transcript}", lane.name));
        let record = serde_json::Deserializer::from_str(&transcript[start..])
            .into_iter::<serde_json::Value>()
            .next()
            .and_then(Result::ok)
            .unwrap_or_else(|| panic!("{}: an unreadable record:\n{transcript}", lane.name));
        record["stdout_inline"].as_str().unwrap_or("").to_string()
    }

    fn expect_lines(lane: &Lane, got: &str, want: &[&str], transcript: &str) {
        for w in want {
            assert!(
                got.lines().any(|l| l == *w),
                "{}: missing line {w:?}\n--- printed\n{got}\n--- terminal\n{transcript}",
                lane.name
            );
        }
    }

    #[test]
    fn a_spawned_child_leads_a_new_session() {
        let dir = scratch("spawn");
        let child = build(&fixture("child.lu"), &dir, false);
        std::fs::write(dir.join("s290.child"), child.to_string_lossy().as_bytes())
            .expect("name the child");
        for lane in lanes(&fixture("spawn.lu"), &dir, true) {
            let mut argv = lane.argv.clone();
            if !lane.checked {
                argv.push(child.to_string_lossy().into_owned());
            }
            let (status, transcript) = pty_run(&argv, &dir);
            let got = printed(&lane, &transcript);
            eprintln!("s290 {} spawn: {status:?}\n{got}", lane.name);
            expect_lines(
                &lane,
                &got,
                &[
                    "parent: leads its session true",
                    "child: sid=pid true, pgid=pid true, tty 6, quit ignored false",
                    "parent: defaults 4, child status 0, child is another process true",
                    "child: sid=pid true, pgid=pid true, tty 6, quit ignored true",
                    "parent: defaults 0, child status 0, child is another process true",
                    "parent: session unchanged true",
                    "parent: /dev/tty opens true",
                ],
                &transcript,
            );
            assert!(status.success(), "{}: {status:?}\n{transcript}", lane.name);
        }
    }

    #[test]
    fn this_process_into_a_session_of_its_own() {
        let dir = scratch("mover");
        for lane in lanes(&fixture("mover.lu"), &dir, true) {
            // As the session's leader: refused, nothing moves.
            let (status, transcript) = pty_run(&lane.argv, &dir);
            let got = printed(&lane, &transcript);
            eprintln!("s290 {} mover as the leader: {status:?}\n{got}", lane.name);
            expect_lines(
                &lane,
                &got,
                &[
                    "mover: a terminal before true, leads its session true",
                    "mover: setsid denied, error 1",
                    "mover: sid=pid true, pgid=pid true",
                    "mover: a terminal after true, error 0",
                    "mover: again denied, error 1",
                    "mover: the old terminal's foreground ok",
                ],
                &transcript,
            );
            assert!(status.success(), "{}: {status:?}\n{transcript}", lane.name);
            // As a child of the leader's shell, in the leader's group: it
            // moves. `; :` keeps the shell from running the program in
            // its own place.
            let mut argv = vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "\"$@\"; :".to_string(),
                "sh".to_string(),
            ];
            argv.extend(lane.argv.iter().cloned());
            let (status, transcript) = pty_run(&argv, &dir);
            let got = printed(&lane, &transcript);
            eprintln!(
                "s290 {} mover in the leader's group: {status:?}\n{got}",
                lane.name
            );
            expect_lines(
                &lane,
                &got,
                &[
                    "mover: a terminal before true, leads its session false",
                    "mover: setsid ok, error 0",
                    "mover: sid=pid true, pgid=pid true",
                    "mover: a terminal after false, error 6",
                    "mover: again denied, error 1",
                    "mover: the old terminal's foreground io",
                ],
                &transcript,
            );
            assert!(status.success(), "{}: {status:?}\n{transcript}", lane.name);
        }
    }

    #[test]
    fn a_forked_child_moves_from_a_task_and_its_restore_restores() {
        let dir = scratch("forked");
        for lane in lanes(&fixture("forked.lu"), &dir, false) {
            let (status, transcript) = pty_run(&lane.argv, &dir);
            eprintln!("s290 {} forked: {status:?}\n{transcript}", lane.name);
            expect_lines(
                &lane,
                &transcript,
                &[
                    "forked: setsid from a task ok, main reads sid=pid true",
                    "RESTORED",
                    "parent: child status 0, mode back true",
                ],
                &transcript,
            );
            assert!(status.success(), "{}: {status:?}\n{transcript}", lane.name);
        }
    }
}
