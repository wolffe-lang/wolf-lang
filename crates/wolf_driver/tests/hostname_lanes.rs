//! wolf-lang#858 (`[os.hostname]`): `os_hostname() -> str ! {io,
//! unsupported, utf8}` answers the kernel's node name, read on every call.
//!
//! - **The host names itself** (`fixtures/hostname/host.lu`, every host, on
//!   the host): every machine prints the name and its length. The test holds
//!   them to its own `uname` (written here apart from the runtime), on linux
//!   to `/proc/sys/kernel/hostname`, and to what the host's own `hostname`
//!   program prints: the first of `/bin/hostname` and `/usr/bin/hostname`
//!   that is a file, started by that absolute path with no argument and no
//!   shell. No such program is a loud `SKIP:`, and a failure on CI
//!   (`GITHUB_ACTIONS`). Windows: `name=unsupported` on every machine.
//! - **Renamed while running, not UTF-8, the longest name** (`rename.lu`,
//!   linux only): each machine in a throwaway `--network none` container of
//!   its own, started with `--hostname wolf-before` and `--cap-add
//!   SYS_ADMIN` so that it may rename its own UTS namespace and nothing
//!   else. The program reads the name, starts `/usr/bin/hostname` by its
//!   absolute path with no shell, waits, and reads again. Where the
//!   container may not rename itself the program prints `rename=refused N`
//!   and the row is a loud `SKIP:` naming the engine, a failure where
//!   `WOLF_JOBWAIT_REQUIRE_CONTAINER` is set (CI's linux job). The engine
//!   switches are the job-wait witnesses' (`WOLF_JOBWAIT_ENGINE`,
//!   `WOLF_JOBWAIT_IMAGE`); no engine is a loud skip, the same failure on CI.
//!
//! **lupin** does not resolve `os_hostname` yet: its answer is pinned to
//! `unsupported` naming "does not resolve", and a lupin that resolves the
//! name is held to the compiler's stdout. Its mirror is wolf-interp's.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn loud(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hostname")
}

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("hostname_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn piped(mut cmd: Command) -> Output {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn")
}

/// The native or release binary of `entry`, or `None` (a loud skip) where
/// this host cannot build the tier. A compile error fails the gate.
fn build(entry: &Path, dir: &Path, release: bool) -> Option<PathBuf> {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("a-{tier}{}", std::env::consts::EXE_SUFFIX));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = piped(cmd);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("error["),
        "wolf build {} refused the program (a compile error is the defect, never a skip):\n{stderr}",
        entry.display()
    );
    if lane_exit::environment_refusal(&out, "wolf build") {
        loud(&format!(
            "SKIP: {} --{tier}: this host cannot build the tier: {}",
            entry.display(),
            stderr.trim()
        ));
        return None;
    }
    assert!(out.status.success(), "wolf build --{tier} failed: {stderr}");
    Some(exe)
}

/// The verdict and stdout of one record (`conform-run --json`).
fn record(out: &Output, what: &str) -> (String, String, String, String) {
    let text = String::from_utf8_lossy(&out.stdout).replace('\r', "");
    let rec: serde_json::Value = text
        .lines()
        .rev()
        .find_map(|l| {
            l.find('{')
                .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
        })
        .unwrap_or_else(|| {
            panic!(
                "{what} record parses: {text}{}",
                String::from_utf8_lossy(&out.stderr)
            )
        });
    let field = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    (
        field("verdict"),
        field("stdout_inline"),
        field("x-unsupported"),
        field("impl_version"),
    )
}

fn lupin_or_skip() -> Option<PathBuf> {
    let found = if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        p.is_file().then_some(p)
    } else {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut hit = None;
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                hit = Some(candidate);
                break;
            }
        }
        hit
    };
    if found.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found — the oracle leg did not run"
        );
        loud("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

/// The node name the test reads itself, apart from the runtime.
#[cfg(unix)]
fn own_node_name() -> String {
    // SAFETY: `uname` fills a zeroed struct; the field is NUL-terminated.
    let u = unsafe {
        let mut u: libc::utsname = std::mem::zeroed();
        assert_eq!(libc::uname(&mut u), 0, "uname answers");
        u
    };
    let b: Vec<u8> = u
        .nodename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(b).expect("this host's name is UTF-8")
}

/// What the host's own `hostname` program prints, started by an absolute
/// path with no shell, or `None` where neither path holds one.
#[cfg(unix)]
fn hostname_program() -> Option<(String, String)> {
    let path = ["/bin/hostname", "/usr/bin/hostname"]
        .into_iter()
        .find(|p| Path::new(p).is_file())?;
    let out = piped(Command::new(path));
    assert!(out.status.success(), "{path} answers: {out:?}");
    let text = String::from_utf8(out.stdout).expect("hostname prints UTF-8");
    Some((
        path.to_string(),
        text.strip_suffix('\n').unwrap_or(&text).to_string(),
    ))
}

/// #858: every machine answers the name the kernel holds, the same as the
/// test's own `uname`, linux's `/proc` file and the host's `hostname`.
/// Red at trunk: E0301 (`os_hostname` does not resolve).
#[test]
fn the_host_names_itself() {
    let dir = scratch("host");
    let entry = fixtures().join("host.lu");
    #[cfg(unix)]
    let want = {
        let name = own_node_name();
        #[cfg(target_os = "linux")]
        {
            let proc = std::fs::read_to_string("/proc/sys/kernel/hostname")
                .expect("/proc/sys/kernel/hostname reads");
            assert_eq!(proc.trim_end_matches('\n'), name, "/proc and uname agree");
        }
        match hostname_program() {
            Some((path, printed)) => {
                loud(&format!(
                    "REPORT {path} prints {printed:?}; uname says {name:?}"
                ));
                assert_eq!(printed, name, "{path} and uname agree");
            }
            None => {
                assert!(
                    std::env::var_os("GITHUB_ACTIONS").is_none(),
                    "no hostname program at /bin/hostname or /usr/bin/hostname on CI"
                );
                loud("SKIP: the_host_names_itself: no hostname program at /bin or /usr/bin");
            }
        }
        format!("name={name}\nbytes={}\n", name.len())
    };
    #[cfg(not(unix))]
    let want = "name=unsupported\n".to_string();

    let mut wrong = Vec::new();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(&entry)
        .arg("--checked")
        .arg("--json")
        .current_dir(&dir);
    let (verdict, stdout, _, _) = record(&piped(cmd), "the checked machine's");
    loud(&format!("== checked: {verdict}\n{stdout}"));
    if (verdict.as_str(), stdout.as_str()) != ("exit(0)", want.as_str()) {
        wrong.push(format!("checked: {verdict} {stdout:?}"));
    }
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        let mut cmd = Command::new(&exe);
        cmd.current_dir(&dir);
        let out = piped(cmd);
        let got = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        loud(&format!("== {tier}: exit {:?}\n{got}", out.status.code()));
        if out.status.code() != Some(0) || got != want {
            wrong.push(format!("{tier}: exit {:?} {got:?}", out.status.code()));
        }
    }
    if let Some(l) = lupin_or_skip() {
        let mut cmd = Command::new(&l);
        cmd.arg("conform-run")
            .arg(&entry)
            .arg("--json")
            .current_dir(&dir);
        let (verdict, stdout, reason, version) = record(&piped(cmd), "lupin's");
        loud(&format!("== lupin {version}: {verdict} {reason}\n{stdout}"));
        let unresolved = verdict == "unsupported"
            && reason.contains("`os_hostname`")
            && reason.contains("does not resolve");
        let mirrors = verdict == "exit(0)" && stdout == want;
        if !(unresolved || mirrors) {
            wrong.push(format!("lupin {version}: {verdict} {reason} {stdout:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "the host's name (want {want:?}):\n- {}",
        wrong.join("\n- ")
    );
}

#[cfg(target_os = "linux")]
mod in_a_container {
    use super::*;

    fn image() -> String {
        std::env::var("WOLF_JOBWAIT_IMAGE")
            .unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
    }

    /// The first engine that runs the image, or `None` (a loud skip, a
    /// failure where CI requires the container).
    fn engine(test: &str) -> Option<String> {
        let candidates: Vec<String> = match std::env::var("WOLF_JOBWAIT_ENGINE") {
            Ok(e) if !e.is_empty() => vec![e],
            _ => vec!["docker".into(), "podman".into()],
        };
        let mut why = Vec::new();
        for e in &candidates {
            let mut cmd = Command::new(e);
            cmd.args(["run", "--rm", "--network", "none"])
                .arg(image())
                .arg("/usr/bin/true");
            match cmd.stdin(Stdio::null()).output() {
                Ok(o) if o.status.success() => {
                    loud(&format!(
                        "PLACE: {test}: a throwaway {e} container of {}",
                        image()
                    ));
                    return Some(e.clone());
                }
                Ok(o) => why.push(format!(
                    "{e}: {}",
                    String::from_utf8_lossy(&o.stderr)
                        .lines()
                        .next()
                        .unwrap_or("")
                )),
                Err(err) => why.push(format!("{e}: {err}")),
            }
        }
        assert!(
            std::env::var_os("WOLF_JOBWAIT_REQUIRE_CONTAINER").is_none(),
            "WOLF_JOBWAIT_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: {why:?}",
            image()
        );
        loud(&format!(
            "SKIP: {test}: no container engine ran {} ({why:?})",
            image()
        ));
        None
    }

    fn hex(s: &[u8]) -> String {
        s.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// `rename.lu` with `args` on every machine, each in its own container
    /// named `wolf-before`; every stdout held to `before=wolf-before`, then
    /// `after`'s line and its length.
    fn renamed(test: &str, args: &[&str], files: &[(&str, &[u8])], after: &str, bytes: usize) {
        let Some(engine) = engine(test) else {
            return;
        };
        let dir = scratch(test);
        for (name, body) in files {
            std::fs::write(dir.join(name), body).expect("a mounted file is written");
        }
        let entry = fixtures().join("rename.lu");
        let want = format!("before=wolf-before\nafter={after}\nbytes={bytes}\n");
        let mut wrong = Vec::new();
        let mut ran = 0;
        for machine in ["checked", "native", "release"] {
            let argv: Vec<String> = match machine {
                "checked" => {
                    let mut v = vec![
                        "/machines/wolf".to_string(),
                        "conform-run".into(),
                        "/fx/rename.lu".into(),
                        "--json".into(),
                        "--checked".into(),
                        format!("--argv-hex={}", hex(b"rename")),
                    ];
                    v.extend(
                        args.iter()
                            .map(|a| format!("--argv-hex={}", hex(a.as_bytes()))),
                    );
                    v
                }
                tier => {
                    let Some(exe) = build(&entry, &dir, tier == "release") else {
                        continue;
                    };
                    let mut v = vec![format!("/w/{}", exe.file_name().unwrap().to_string_lossy())];
                    v.extend(args.iter().map(|a| a.to_string()));
                    v
                }
            };
            let mut cmd = Command::new(&engine);
            cmd.args(["run", "--rm", "--network", "none"])
                .args(["--hostname", "wolf-before", "--cap-add", "SYS_ADMIN"])
                .arg("-v")
                .arg(format!("{}:/w:ro", dir.display()))
                .arg("-v")
                .arg(format!("{}:/fx:ro", fixtures().display()))
                .arg("-v")
                .arg(format!("{}:/machines/wolf:ro", wolf()))
                .args(["-w", "/tmp"])
                .arg(image())
                .args(&argv);
            let out = piped(cmd);
            let stdout = if machine == "checked" {
                record(&out, "the checked machine's").1
            } else {
                String::from_utf8_lossy(&out.stdout).replace('\r', "")
            };
            loud(&format!(
                "REPORT {test} {machine}: {stdout:?} | {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
            if let Some(line) = stdout.lines().find(|l| l.starts_with("rename=")) {
                assert!(
                    std::env::var_os("WOLF_JOBWAIT_REQUIRE_CONTAINER").is_none(),
                    "{test} {machine}: the {engine} container may not rename itself ({line})"
                );
                loud(&format!(
                    "SKIP: {test} {machine}: the {engine} container may not rename itself ({line})"
                ));
                continue;
            }
            ran += 1;
            if stdout != want {
                let first = stdout.lines().nth(1).unwrap_or("");
                wrong.push(format!(
                    "{machine}: the name read after the rename is the new one (read on every \
                     call): want {want:?}, got {stdout:?} ({first})"
                ));
            }
        }
        loud(&format!("RAN {test} on {ran} of 3 machines"));
        assert!(wrong.is_empty(), "{test}:\n- {}", wrong.join("\n- "));
    }

    /// #858: a name kept from the first call would print `wolf-before`
    /// twice. Red at trunk: E0301.
    #[test]
    fn renamed_while_running() {
        renamed(
            "renamed_while_running",
            &["wolf-after"],
            &[],
            "wolf-after",
            10,
        );
    }

    /// #858: a name that is not UTF-8 is `utf8` on every machine, never a
    /// lossy decode. The bytes reach `hostname` through a file (`-F`), as
    /// an engine's argument path may re-encode them.
    #[test]
    fn a_name_that_is_not_utf8_is_the_utf8_row() {
        renamed(
            "a_name_that_is_not_utf8_is_the_utf8_row",
            &["-F", "/w/name"],
            &[("name", &[0xff, b'f', b'o', b'\n'])],
            "<utf8>",
            6,
        );
    }

    /// #858: the longest name linux holds, 64 bytes, answered whole.
    #[test]
    fn the_longest_name_is_answered_whole() {
        let name = "a".repeat(64);
        renamed(
            "the_longest_name_is_answered_whole",
            &[&name],
            &[],
            &name,
            64,
        );
    }
}
