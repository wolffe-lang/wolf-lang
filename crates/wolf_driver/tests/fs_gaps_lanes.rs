//! s254 (wolf-lang#820, #818; yew yw02 G30, G32) — what an editor's save
//! path, its journal and its clipboard met as prelude absences, as rows on
//! every machine: `access(2)` (`[os.fs.access]`), the open that does not
//! follow a final link (`[os.fs.open.nofollow]`), the advisory lock
//! (`[os.fs.lock]`), the full stat on a handle (`[os.fs.fstat.record]`),
//! the fifo (`[os.fs.mkfifo]`), and the pipe a clipboard writer feeds: a
//! non-blocking write end, a write that answers what it took, and a wait
//! for write readiness (`[os.fs.nonblock]`, `[os.signal.wait.write]`).
//!
//! **Two places.** The rows that touch no fifo, no lock and no pipe
//! (access, the no-follow open, the handle's record) run on the host:
//! the checked machine and lupin under `conform-run`, the native and
//! release binaries directly (on linux again under `taskset` on four CPUs of the allocation,
//! wolf-lang#570). The rows that make a fifo, take a lock or fill a pipe
//! run where a park cannot cost a host anything: on linux inside a
//! throwaway `--network none` container of their own (the fork
//! witnesses' place: `WOLF_FORK_ENGINE` names the engine, otherwise the
//! first of `docker` and `podman` that answers, `WOLF_FORK_IMAGE` the
//! image; **`WOLF_FORK_REQUIRE_CONTAINER=1`, CI's linux job, makes a
//! missing engine a failure**), each binary under `timeout 20` there, the
//! checked machine there too (the `wolf` under test mounted read-only);
//! on macOS on CI's throwaway runner only (`GITHUB_ACTIONS`), under the
//! same `timeout`'s watchdog in perl; on windows on CI's runner, where
//! the calls that need a descriptor's flags answer `unsupported` and
//! nothing can park. lupin runs on the host: 0.1.55, the pairing,
//! resolves none of the names, so it parks on nothing.
//!
//! **Red at s254's gate commit**: every fixture is E0301 (the names do not
//! exist; the no-follow bit is `invalid` and the write bit `io` at 0.2.34,
//! which `nofollow_rows.lu` and `nonblock_rows.lu` would also read red), a
//! compile error the build step fails on, never a skip.

mod lane_exit;
mod test_cpus;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A `REPORT` line for the CI log even when the test passes.
fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

/// The lupin releases that predate every s254 name: 0.1.55 is the 0.2.34
/// pairing. A newer lupin is held to the compiler's answer.
// r40: every s254 refusal re-measured on the published 0.1.56 archive.
// r41: the published .57 retains these measured pre-mirror answers.
// r41 resume: published .58 re-measured the same named gaps.
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.55", "0.1.56", "0.1.57", "0.1.58"];

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    reason: String,
    version: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let text = String::from_utf8_lossy(bytes).replace('\r', "");
    let rec: serde_json::Value = text
        .lines()
        .rev()
        .find_map(|l| {
            l.find('{')
                .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
        })
        .unwrap_or_else(|| panic!("{what} record parses: {text}"));
    let field = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: field("verdict"),
        stdout: field("stdout_inline"),
        reason: {
            let r = field("x-unsupported");
            if r.is_empty() {
                field("x-unsupported-construct")
            } else {
                r
            }
        },
        version: field("impl_version"),
    }
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fs_gaps")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// A fresh working directory of the test's own.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("fs_gaps_lanes")
        .join(test);
    if d.exists() {
        make_writable(&d);
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// A tree a red run may have left read-only (`access_rows.lu`'s 0500
/// directory) is made removable first.
fn make_writable(d: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700));
                    make_writable(&p);
                }
            }
        }
    }
    #[cfg(not(unix))]
    let _ = d;
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

/// The native or release binary of `entry`, as `dir/<stem>-<tier>`. A
/// compile error FAILS the gate (it is the defect at a pre-fix head, never
/// an environment); an environment that cannot build the tier skips
/// loudly.
fn build(entry: &Path, dir: &Path, stem: &str, release: bool) -> Option<PathBuf> {
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("{stem}-{tier}{}", std::env::consts::EXE_SUFFIX));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(entry)
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
        "wolf build {} refused the program (a compile error is the defect, never a skip):\n{stderr}",
        entry.display()
    );
    if lane_exit::environment_refusal(&out, "wolf build") {
        eprintln!(
            "SKIP: environment cannot build the {tier} tier: {}",
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
}

/// The sibling lupin, found as `pairing.rs` finds it, or a loud skip — a
/// failure under `WOLF_PAIRING_REQUIRE_SIBLING`.
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
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s254's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

/// lupin 0.1.55 stops at one of `calls`, which it does not resolve; a newer
/// lupin is held to the compiler's answer. No `calls` is the no-follow
/// bit's case: 0.1.55 resolves `fs_open_mode` and refuses the bit as a mode
/// outside its set (`invalid`, measured at s254's prediction commit), so it
/// runs the program and its first row reads `invalid`.
fn judge_lupin(entry: &Path, dir: &Path, want: &str, calls: &[&str]) {
    let Some(l) = lupin_or_skip() else {
        return;
    };
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(&l);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let obs = parse_obs(&run_in(cmd, dir).stdout, "lupin's observation");
    eprintln!(
        "== lupin {} on {name}: {} {}\n{}",
        obs.version, obs.verdict, obs.reason, obs.stdout
    );
    if PRE_MIRROR_LUPIN.contains(&obs.version.as_str()) && calls.is_empty() {
        assert!(
            obs.verdict == "exit(0)" && obs.stdout.starts_with("file_read=invalid\n"),
            "lupin {} (pre-mirror) refuses the no-follow bit as a mode outside its set: {obs:?}",
            obs.version
        );
        report(&format!(
            "REPORT PINNED pre-mirror lupin {} on {name}: the bit `invalid`: {}",
            obs.version,
            obs.stdout.trim_end().replace('\n', " | ")
        ));
    } else if PRE_MIRROR_LUPIN.contains(&obs.version.as_str()) {
        assert_eq!(
            obs.verdict, "unsupported",
            "lupin {} refuses {name}: {obs:?}",
            obs.version
        );
        assert!(
            calls.iter().any(|c| obs.reason.contains(&format!("`{c}`")))
                && obs.reason.contains("does not resolve"),
            "lupin {} (pre-mirror) does not resolve one of {calls:?}: {}",
            obs.version,
            obs.reason
        );
        report(&format!(
            "REPORT PINNED pre-mirror lupin {} on {name}: {}",
            obs.version, obs.reason
        ));
    } else {
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "lupin {} on {name}: {obs:?}",
            obs.version
        );
    }
}

/// `entry` on every machine, on the host, in `dir`: the checked machine
/// under `conform-run`, the native and release binaries run directly (and
/// under `taskset` on four CPUs of the allocation on linux), lupin under `conform-run`. Each
/// stdout is held to `check`; the checked machine's is the one lupin is
/// held to past the pre-mirror releases.
fn every_lane(entry: &Path, dir: &Path, calls: &[&str], check: &dyn Fn(&str, &str)) -> String {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    let out = run_in(cmd, dir);
    let checked = parse_obs(&out.stdout, "the observation");
    eprintln!(
        "== checked on {name}: {}\n{}{}",
        checked.verdict,
        checked.stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        checked.verdict, "exit(0)",
        "the CHECKED machine on {name}: {checked:?}"
    );
    check("checked", &checked.stdout);
    report(&format!(
        "REPORT checked {name}: {}",
        checked.stdout.trim_end().replace('\n', " | ")
    ));
    let want = checked.stdout.clone();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, dir, "a", release) else {
            continue;
        };
        let mut runs = vec![(tier.to_string(), Command::new(&exe))];
        if let Some(pin) = test_cpus::pin(4, module_path!()) {
            runs.push((format!("{tier} {}", pin.label()), pin.command(&exe)));
        }
        for (lane, cmd) in runs {
            let out = run_in(cmd, dir);
            let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
            eprintln!(
                "== {lane} on {name}: exit {:?}\n{stdout}{}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.status.code(), Some(0), "the {lane} binary on {name}");
            check(&lane, &stdout);
            report(&format!(
                "REPORT {lane} {name}: {}",
                stdout.trim_end().replace('\n', " | ")
            ));
        }
    }
    judge_lupin(entry, dir, &want, calls);
    want
}

/// Is this process root? (`access_rows.lu`'s 0500 directory is writable
/// to root, which writes anywhere.)
fn is_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: a plain query of this process's ids.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

// ------------------------------------------------- #820: on the host --

/// `[os.fs.access]`: `access(2)` with the real ids on every unix machine;
/// windows `unsupported` by name, the shape's `invalid` before it.
#[test]
fn access_on_every_machine() {
    let dir = scratch("access");
    let ro = if is_root() { "ok" } else { "denied" };
    let want_unix = format!(
        "exists=ok\nread=ok\nwrite=ok\nread_write=ok\nexec=denied\ndir_write=ok\n\
dir_search=ok\nro_write={ro}\nro_read=ok\nmissing=not_found\nunder_file=io\n\
dangle=not_found\nmode8=invalid\nnegative=invalid\n"
    );
    let want_windows = "exists=unsupported\nread=unsupported\nwrite=unsupported\n\
read_write=unsupported\nexec=unsupported\ndir_write=unsupported\ndir_search=unsupported\n\
ro_write=unsupported\nro_read=unsupported\nmissing=unsupported\nunder_file=unsupported\n\
dangle=no_link\nmode8=invalid\nnegative=invalid\n";
    every_lane(
        &fixture("access_rows.lu"),
        &dir,
        &["fs_access"],
        &|lane, stdout| {
            let want = if cfg!(unix) {
                want_unix.as_str()
            } else {
                want_windows
            };
            assert_eq!(stdout, want, "the {lane} machine's access rows");
        },
    );
}

/// `[os.fs.open.nofollow]`: bit 16 refuses a final link `invalid` and
/// follows a middle one; mode 4 on a link is `exists` with or without it;
/// a refused write open truncates nothing. windows refuses the bit.
#[test]
fn the_open_that_does_not_follow_on_every_machine() {
    let dir = scratch("nofollow");
    let want_unix = "file_read=ok\nfile_rw=ok\nfile_nonblock=ok\nfile_append=ok\nbit32=invalid\n\
bit48=invalid\nnegative=invalid\nlinks=made\nlink_follows=ok\nlink_refused=invalid\n\
link_errno_set=true\nlink_rw_refused=invalid\nlink_write_refused=invalid\n\
middle_followed=ok\nexcl_on_link=exists\nexcl_nofollow_on_link=exists\n\
perm_link_refused=invalid\nperm_excl_on_link=exists\ntarget_intact=true\n";
    let want_windows = "file_read=invalid\nfile_rw=invalid\nfile_nonblock=invalid\n\
file_append=invalid\nbit32=invalid\nbit48=invalid\nnegative=invalid\nlinks=unsupported\n";
    every_lane(&fixture("nofollow_rows.lu"), &dir, &[], &|lane, stdout| {
        let want = if cfg!(unix) { want_unix } else { want_windows };
        assert_eq!(stdout, want, "the {lane} machine's no-follow rows");
    });
}

/// `[os.fs.fstat.record]`: the twenty words from the handle, the path's
/// identity, the same file after a rename; a pipe kind 4 (windows 2); a
/// directory kind 1 (unix).
#[test]
fn the_record_on_a_handle_on_every_machine() {
    let dir = scratch("fstat_record");
    let want_unix = "len=20\nkind=0 size=12\nsame_mode=true same_nlink=true\n\
same_identity=true\nsame_mtime=true\nsame_have=true\ndev_have=1\nafter_rename=true\n\
closed=-1\nforged=-1\npipe_kind=4\ndir_kind=1\n";
    every_lane(
        &fixture("fstat_record_rows.lu"),
        &dir,
        &["fs_fstat_record"],
        &|lane, stdout| {
            if cfg!(unix) {
                assert_eq!(stdout, want_unix, "the {lane} machine's handle record");
            } else {
                for line in [
                    "len=20\n",
                    "kind=0 size=12\n",
                    "dev_have=0\n",
                    "closed=-1\n",
                    "forged=-1\n",
                    "pipe_kind=2\n",
                    "dir_kind=not_opened\n",
                ] {
                    assert!(
                        stdout.contains(line),
                        "the {lane} machine's handle record (windows) lacks {line:?}: {stdout}"
                    );
                }
            }
        },
    );
}

// ------------------------ #820, #818: where a park costs the host nothing --

fn image() -> String {
    std::env::var("WOLF_FORK_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
}

/// Where the fifo, lock and pipe rows run: a container on linux, CI's
/// runner on macOS and windows.
struct Place {
    engine: Option<String>,
    dir: PathBuf,
}

impl Place {
    /// The place to run, or a loud skip (a failure where CI requires it).
    fn new(test: &str) -> Option<Place> {
        let dir = scratch(test);
        if !cfg!(target_os = "linux") {
            if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
                eprintln!(
                    "SKIP: {test}: not CI's throwaway runner — the fifo, lock and pipe rows run \
                     off linux only there"
                );
                return None;
            }
            return Some(Place { engine: None, dir });
        }
        let required = std::env::var_os("WOLF_FORK_REQUIRE_CONTAINER").is_some();
        let candidates: Vec<String> = match std::env::var("WOLF_FORK_ENGINE") {
            Ok(e) => vec![e],
            Err(_) => vec!["docker".into(), "podman".into()],
        };
        let mut why = Vec::new();
        for e in &candidates {
            let out = Command::new(e)
                .args(["run", "--rm", "--network", "none"])
                .arg(image())
                .args(["sh", "-c", "echo up"])
                .stdin(Stdio::null())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    return Some(Place {
                        engine: Some(e.clone()),
                        dir,
                    });
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
            !required,
            "WOLF_FORK_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: {why:?}",
            image()
        );
        eprintln!(
            "SKIP: {test}: no container engine ran {} ({why:?}) — the rows did not run",
            image()
        );
        None
    }

    /// A file in the place's directory as the program sees it.
    fn at(&self, name: &str) -> String {
        match self.engine {
            Some(_) => format!("/w/{name}"),
            None => self.dir.join(name).display().to_string(),
        }
    }

    /// The `wolf` under test as the place sees it.
    fn wolf(&self) -> String {
        match self.engine {
            Some(_) => "/wolf".to_string(),
            None => wolf().to_string(),
        }
    }

    /// `argv` in the place's directory under a watchdog of `secs`: `timeout`
    /// in the container (the shell is its process 1, the program its child),
    /// perl's `alarm` on CI's macOS runner, plain on windows (nothing there
    /// can park: the descriptor flags are `unsupported`). The answer is the
    /// exit code (124 the watchdog), stdout and stderr.
    fn run(&self, argv: &[String], secs: u32) -> (Option<i32>, String, String) {
        let mut cmd = match &self.engine {
            Some(e) => {
                let mut c = Command::new(e);
                c.args(["run", "--rm", "--network", "none"]);
                if e == "podman" {
                    c.arg("--userns=keep-id");
                }
                c.arg("-v").arg(format!("{}:/w", self.dir.display()));
                c.arg("-v").arg(format!("{}:/wolf:ro", wolf()));
                c.args(["-w", "/w"])
                    .arg(image())
                    .arg("timeout")
                    .arg(secs.to_string());
                c.args(argv);
                c
            }
            None if cfg!(target_os = "macos") => {
                let mut c = Command::new("perl");
                c.args(["-e", &format!("alarm {secs}; exec @ARGV or die")]);
                c.args(argv);
                c.current_dir(&self.dir);
                c
            }
            None => {
                let mut c = Command::new(&argv[0]);
                c.args(&argv[1..]);
                c.current_dir(&self.dir);
                c
            }
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let out = cmd.output().expect("the engine (or the program) runs");
        // perl's alarm ends the program by SIGALRM: no exit code, the red.
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).replace('\r', ""),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// Four cpus where `taskset` on four CPUs of the allocation can be asked from inside.
    fn four_cpus(&self) -> bool {
        if !cfg!(target_os = "linux") {
            return false;
        }
        let (_, out, _) = self.run(&["nproc".to_string()], 20);
        out.trim().parse::<usize>().unwrap_or(1) >= 4
    }
}

/// The watchdogs: a compiled binary's rows take milliseconds, the checked
/// machine's interpretation of the same program seconds.
const BINARY_SECS: u32 = 20;
const CHECKED_SECS: u32 = 60;

/// `name` on every machine in the place: the checked machine under
/// `conform-run` there, the native and release binaries (and under
/// `taskset` on four CPUs of the allocation on linux with four cpus); lupin on the host. Each
/// stdout is held to `check`; a watchdog's end is the red.
fn every_place_lane(test: &str, name: &str, calls: &[&str], check: &dyn Fn(&str, &str)) {
    let Some(place) = Place::new(test) else {
        return;
    };
    let entry = fixture(name);
    let local = place.dir.join(name);
    std::fs::copy(&entry, &local).expect("the fixture copied into the place");
    // The checked machine interprets: 60 s, a park's bound and not its
    // speed's (the gauntlet runs it beside every other test).
    let (code, out, err) = place.run(
        &[
            place.wolf(),
            "conform-run".into(),
            place.at(name),
            "--checked".into(),
            "--json".into(),
        ],
        CHECKED_SECS,
    );
    assert_ne!(
        code,
        Some(124),
        "the checked machine on {name} was still running at {CHECKED_SECS} s (the watchdog \
         ended it): {err}"
    );
    assert!(
        out.contains('{'),
        "the checked machine on {name} left no record (exit {code:?}): {err}"
    );
    let checked = parse_obs(out.as_bytes(), "the checked observation");
    eprintln!(
        "== checked on {name} (exit {code:?}): {}\n{}{err}",
        checked.verdict, checked.stdout
    );
    assert_eq!(
        checked.verdict, "exit(0)",
        "the CHECKED machine on {name}: {checked:?} {err}"
    );
    check("checked", &checked.stdout);
    report(&format!(
        "REPORT checked {name}: {}",
        checked.stdout.trim_end().replace('\n', " | ")
    ));
    // The pinned rows: four of the allocation (`test_cpus`, wolf-lang#925),
    // asked of the place's own `taskset`.
    let four = if place.four_cpus() {
        test_cpus::subset(4, test)
    } else {
        if cfg!(target_os = "linux") {
            eprintln!("SKIP: {test}: fewer than four cpus — the pinned rows did not run");
        }
        None
    };
    let stem = name.trim_end_matches(".lu");
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &place.dir, stem, release) else {
            continue;
        };
        let prog = place.at(&exe.file_name().unwrap().to_string_lossy());
        let mut rows = vec![(tier.to_string(), vec![prog.clone()])];
        if let Some(list) = &four {
            rows.push((
                format!("{tier} taskset -c {list}"),
                vec!["taskset".into(), "-c".into(), list.clone(), prog.clone()],
            ));
        }
        for (lane, argv) in rows {
            let (code, stdout, stderr) = place.run(&argv, BINARY_SECS);
            report(&format!(
                "REPORT {lane} {name} (exit {code:?}): {} || {}",
                stdout.trim_end().replace('\n', " | "),
                stderr.trim_end().replace('\n', " | ")
            ));
            assert_ne!(
                code,
                Some(124),
                "the {lane} binary on {name} was still parked at {BINARY_SECS} s (the watchdog \
                 ended it)"
            );
            assert_eq!(code, Some(0), "the {lane} binary on {name}: {stderr}");
            check(&lane, &stdout);
        }
    }
    judge_lupin(&local, &place.dir, &checked.stdout, calls);
}

/// The lines of `stdout` that are not `REPORT` lines (a row the host
/// decides, printed for the log and never held).
fn held(stdout: &str) -> String {
    stdout
        .lines()
        .filter(|l| !l.starts_with("REPORT "))
        .map(|l| format!("{l}\n"))
        .collect()
}

/// `[os.fs.lock]`: flock-shaped, per open file, never waiting; `busy` with
/// the host's number; kind 0 and 3 `invalid`; a closed and a forged handle
/// `io`. windows (LockFileEx) answers the same rows; closing a handle
/// releases its lock there too, possibly late, so that one row may read
/// `busy` on windows.
#[test]
fn the_advisory_lock_on_every_machine() {
    let want = "a_excl=ok\nb_excl=busy\nbusy_errno_set=true\nb_shared=busy\na_unlock=ok\n\
b_shared_again=ok\na_shared=ok\nc_excl=busy\na_unlock_again=ok\nb_unlock=ok\nc_excl_free=ok\n\
a_excl_after_close=ok\na_unlock_third=ok\na_unlock_unlocked=ok\nkind0=invalid\nkind3=invalid\n\
closed=io\nforged_unlock=io\nforged_lock=io\n";
    every_place_lane(
        "lock",
        "lock_rows.lu",
        &["fs_lock", "fs_unlock"],
        &|lane, stdout| {
            if cfg!(windows) {
                let relaxed =
                    stdout.replace("a_excl_after_close=busy\n", "a_excl_after_close=ok\n");
                assert_eq!(
                    relaxed.replace("a_unlock_third=io\n", "a_unlock_third=ok\n"),
                    want,
                    "the {lane} machine's lock rows (windows)"
                );
            } else {
                assert_eq!(stdout, want, "the {lane} machine's lock rows");
            }
        },
    );
}

/// `[os.fs.mkfifo]`: made, its kind and bits, `exists`, `not_found`,
/// `invalid`; and mode 5 on it — the open answers with no writer, a read
/// is `eof`, a writer's byte is read back. windows `unsupported`.
#[test]
fn the_fifo_on_every_machine() {
    let want_unix = "made=ok\nagain=exists\nmissing_parent=not_found\nperm512=invalid\n\
negative=invalid\nstat_kind=4 bits=384\nopen_nonblock=ok fstat_kind=2 record_kind=4\n\
no_writer_read=eof\nread_back=x\n";
    let want_windows = "made=unsupported\nagain=unsupported\nmissing_parent=unsupported\n\
perm512=invalid\nnegative=invalid\n";
    every_place_lane(
        "fifo",
        "fifo_rows.lu",
        &["fs_mkfifo", "fs_fstat_record"],
        &|lane, stdout| {
            let want = if cfg!(unix) { want_unix } else { want_windows };
            assert_eq!(stdout, want, "the {lane} machine's fifo rows");
        },
    );
}

/// `[os.fs.nonblock]`, `[os.signal.wait.write]`: the clipboard writer's
/// pipe. linux's pipe takes 65536 bytes (the host's capacity; macOS's is
/// its own, reported and not held).
#[test]
fn the_pipe_a_clipboard_writer_feeds_on_every_machine() {
    let want_unix = "set_nonblock=ok\nfull_at=busy taken_positive=true\nbusy_errno_set=true\n\
wait_full=-1,0\nwaited_25ms=true\nread_wait=0,0\ndrained=true\nwait_room=0,0\nboth_lowest=0,0\n\
again=1\nset_blocking=ok\nempty=0\nwait_reader_gone=0,0\nwrite_reader_gone=io\n\
std0=invalid std1=invalid std2=invalid\nforged_nb=io\nforged_some=io\nforged_wait=io\n";
    let want_windows = "set_nonblock=unsupported\nwrite_some=1\nwait=unsupported\n\
std0=invalid std1=invalid std2=invalid\nforged_nb=unsupported\nforged_some=io\n\
forged_wait=unsupported\n";
    every_place_lane(
        "nonblock",
        "nonblock_rows.lu",
        &["fs_set_nonblock", "fs_write_some", "os_signal_wait_fds"],
        &|lane, stdout| {
            let want = if cfg!(unix) { want_unix } else { want_windows };
            assert_eq!(held(stdout), want, "the {lane} machine's pipe rows");
            if cfg!(target_os = "linux") {
                assert!(
                    stdout.contains("REPORT taken=65536\n"),
                    "the {lane} machine: linux's pipe holds 65536 bytes: {stdout}"
                );
            }
        },
    );
}

/// yew yw02's `clip-pipe-write-blocks` (rg1), written with the calls: 1 MiB
/// into a pipe nobody reads ends in a deadline, not a park (at 0.2.34 it
/// exited 124 under `timeout 3`). unix: windows has no non-blocking pipe.
#[cfg(unix)]
#[test]
fn yews_clipboard_writer_gives_up_instead_of_parking() {
    every_place_lane(
        "clip",
        "clip_write.lu",
        &["fs_set_nonblock", "fs_write_some"],
        &|lane, stdout| {
            assert!(
                stdout.starts_with("rg1: writing 1 MiB into a pipe nobody reads\nrg1: took ")
                    && stdout.ends_with(
                        " of 1048576, then busy; the write wait answered the deadline\n"
                    ),
                "the {lane} machine's clipboard writer: {stdout}"
            );
            if cfg!(target_os = "linux") {
                assert!(
                    stdout.contains("took 65536 of"),
                    "the {lane} machine: linux's pipe takes 65536 bytes: {stdout}"
                );
            }
        },
    );
}
