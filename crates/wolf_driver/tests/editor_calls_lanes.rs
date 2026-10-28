//! s248 (wolf-lang#796, #797, #798; ruling #93; yew `docs/MAP.md` §5 G3,
//! G4, G8) — what the editor asks of the runtime that needs no terminal:
//! a clock at nanoseconds (`[os.clock.ns]`), the canonical path
//! (`[os.fs.realpath]`), and the atomic save's syncs (`[os.fs.sync]`).
//!
//! Each witness runs on the checked machine and lupin under
//! `conform-run`, and as native and release binaries run directly — on
//! linux the binaries again under `taskset` on four CPUs of the allocation (wolf-lang#570) — and
//! every machine's standard output is printed, so a `--nocapture` log is
//! the table. Nothing here is signalled, faulted or forked: the fault
//! shim of the save's witness ends the program by `_exit` at a storage
//! boundary, which is a process ending, not a signal.
//!
//! **lupin 0.1.54** (the 0.2.33 pairing) and **0.1.55** (the 0.2.34
//! pairing, r39) predate every s248 name: each is "does not resolve",
//! pinned here by version exactly as measured (s248's §2; r39 on the
//! published 0.1.55 archive); a newer lupin is held to the mirror
//! (wolf-interp#268).

mod lane_exit;
mod test_cpus;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A `REPORT` line for the CI log even when the test passes: the
/// process's own standard error, past libtest's capture.
fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

/// The lupin releases that predate every s248 name: 0.1.54, the pairing
/// at this lane, and 0.1.55, the 0.2.34 pairing (r39, wolf-interp#268).
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.54", "0.1.55"];

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    reason: String,
    version: String,
    commit: String,
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
        commit: field("commit"),
    }
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/editor_calls")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// A fresh working directory of the test's own.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("editor_calls_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

/// The native or release binary of `entry`, as `dir/<stem>-<tier>`. A
/// compile error FAILS the gate (it is the defect at a pre-fix head,
/// never an environment); an environment that cannot build the tier
/// skips loudly.
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

/// The sibling lupin, found exactly as `pairing.rs` finds it, or a loud
/// skip — a failure under `WOLF_PAIRING_REQUIRE_SIBLING`.
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
             (LUPIN={}) — the oracle leg of s248's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

/// Only these published records retain the measured served-tree boundary.
fn realpath_served_tree_boundary(version: &str, commit: &str) -> bool {
    [("0.1.56", "28547b4"), ("0.1.57", "fb054c8"), ("0.1.58", "6240efe")]
        .iter()
        .any(|(v, c)| *v == version && commit.starts_with(c))
}

#[test]
fn only_measured_realpath_boundaries_are_pinned() {
    assert!(realpath_served_tree_boundary("0.1.56", "28547b4"));
    assert!(realpath_served_tree_boundary("0.1.57", "fb054c8"));
    assert!(!realpath_served_tree_boundary("0.1.57", "28547b4"));
    assert!(!realpath_served_tree_boundary("0.1.57", "unknown"));
    assert!(!realpath_served_tree_boundary("0.1.58", "fb054c8"));
    assert!(realpath_served_tree_boundary("0.1.58", "6240efe"));
    assert!(!realpath_served_tree_boundary("0.1.58", "unknown"));
    assert!(!realpath_served_tree_boundary("0.1.59", "6240efe"));
}

/// lupin 0.1.54 and 0.1.55 stop at one of `calls`, which they do not
/// resolve (the first s248 name each meets); a newer lupin is held to the
/// compiler's answer.
fn judge_lupin(obs: &Obs, name: &str, want: &str, calls: &[&str]) {
    eprintln!(
        "== lupin {} on {name}: {} {}\n{}",
        obs.version, obs.verdict, obs.reason, obs.stdout
    );
    if PRE_MIRROR_LUPIN.contains(&obs.version.as_str()) {
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
    } else if name == "realpath.lu" && realpath_served_tree_boundary(&obs.version, &obs.commit) {
        // is79 serves canonicalization only inside its tree. This fixture
        // deliberately asks for /tmp too; do not call that an absent name.
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("unsupported", "")
        );
        assert!(
            obs.reason
                .starts_with("`fs_realpath(\"/tmp\")` resolves outside the working directory"),
            "{obs:?}"
        );
        assert!(obs.reason.contains("[os.fs.path.domain]"), "{obs:?}");
        assert!(!obs.reason.contains("does not resolve"), "{obs:?}");
        report(&format!(
            "PINNED lupin {} {} served-tree boundary: {}",
            obs.version, obs.commit, obs.reason
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

/// `entry` on every machine, in `dir`: the checked machine and lupin
/// under `conform-run`, the native and release binaries run directly
/// (and under `taskset` on four CPUs of the allocation on linux). Each stdout is held to `want`
/// as `check` reads it; lupin 0.1.54 and 0.1.55 to the refusal of `call`.
fn every_lane(entry: &Path, dir: &Path, call: &[&str], check: &dyn Fn(&str, &str)) -> String {
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
        checked.stdout.trim_end()
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
            report(&format!("REPORT {lane} {name}: {}", stdout.trim_end()));
        }
    }
    if let Some(l) = lupin_or_skip() {
        let mut cmd = Command::new(&l);
        cmd.arg("conform-run").arg(entry).arg("--json");
        let out = run_in(cmd, dir);
        judge_lupin(
            &parse_obs(&out.stdout, "lupin's observation"),
            &name,
            &want,
            call,
        );
    }
    want
}

// ------------------------------------------------------------ #798 --

/// #798 (`[os.clock.ns]`): twenty rounds of about 100 us of work, each
/// timed in nanoseconds, none 0 — yw00's witness read 19 of 20 rounds as
/// 0 ms at 0.2.32 (and 19, 20, 20 at 0.2.33, s248's §2). And the clock is
/// the ms clock's. Red at s248's prediction commit: E0301, the name does
/// not exist.
#[test]
fn twenty_rounds_of_100_us_are_never_zero_ns() {
    let dir = scratch("clock_ns");
    every_lane(
        &fixture("clock_ns.lu"),
        &dir,
        &["os_clock_ns"],
        &|lane, stdout| {
            assert_eq!(
                stdout, "clock_ns zeros=0 of 20 behind=0 work=1199940\n",
                "the {lane} machine's clock"
            );
        },
    );
}

// ------------------------------------------------------------ #797 --

/// #797 (`[os.fs.realpath]`): the canonical path of a file, a directory
/// with a trailing slash, a `..` after a directory and after a LINK (taken
/// after the link: `rp/l1/..` is `rp/a`), a dangling link and a missing
/// name (`not_found`), a loop and a name under a file (`io`). Every row is
/// relative to the canonical working directory, so the record is one on
/// every unix host. Red at s248's prediction commit: E0301.
#[test]
fn the_canonical_path_on_every_machine() {
    let dir = scratch("realpath");
    let tmp = if cfg!(target_os = "macos") {
        "/private/tmp"
    } else {
        "/tmp"
    };
    let want_unix = format!(
        "file=rp/a/f\ndir_slash=rp/a\ndot_dot=rp/a/f\nmissing=not_found\n\
under_file=io\nempty=not_found\ndot=.\ntmp={tmp}\nlinks=made\nlink=rp/a/b\n\
link_dot_dot=rp/a\ndangling=not_found\nloop=io\n"
    );
    let got = every_lane(
        &fixture("realpath.lu"),
        &dir,
        &["fs_realpath"],
        &|lane, stdout| {
            if cfg!(unix) {
                assert_eq!(stdout, want_unix, "the {lane} machine's canonical paths");
            } else {
                assert!(
                    stdout.starts_with("file=rp\\a\\f\n") && stdout.contains("links=unsupported\n"),
                    "the {lane} machine's canonical paths (windows): {stdout}"
                );
            }
        },
    );
    report(&format!("REPORT realpath rows: {got:?}"));
}

// ------------------------------------------------------------ #796 --

/// #796 (`[os.fs.sync]`): `fs_sync` on a handle open for writing is ok,
/// a closed or forged handle and a pipe `io`; `fs_sync_dir` of the
/// working directory ok, of a missing name `not_found`, of a file
/// `invalid` (windows: `unsupported` by name for every argument); and the
/// exclusive create the save starts with answers `exists` for a taken
/// name. Red at s248's prediction commit: E0301.
#[test]
fn the_sync_rows_on_every_machine() {
    let dir = scratch("sync_rows");
    let want_unix = "written=ok\nexcl_again=exists\nclosed=io\nforged=io\nnegative=io\npipe=io\n\
dir_dot=ok\ndir_missing=not_found\ndir_file=invalid\ndir_empty=not_found\nagain=-1\n";
    let got = every_lane(
        &fixture("sync_rows.lu"),
        &dir,
        &["fs_sync", "fs_sync_dir"],
        &|lane, stdout| {
            if cfg!(unix) {
                assert_eq!(stdout, want_unix, "the {lane} machine's sync rows");
            } else {
                assert!(
                    stdout.starts_with("written=ok\nexcl_again=unsupported\nclosed=io\n")
                        && stdout.contains("dir_dot=unsupported\ndir_missing=unsupported\n"),
                    "the {lane} machine's sync rows (windows): {stdout}"
                );
            }
        },
    );
    report(&format!("REPORT sync rows: {got:?}"));
}

// ------------------------------------------- #796, the save's witness --

/// The fault shim (`fixtures/editor_calls/faultshim.c`) built for this
/// host: a shared object `LD_PRELOAD` loads on linux, a dylib dyld
/// interposes on macOS. No C compiler is a loud skip, and a failure on
/// CI's runners (`GITHUB_ACTIONS`), which carry one.
#[cfg(unix)]
fn build_shim(dir: &Path) -> Option<(PathBuf, &'static str)> {
    let src = fixture("faultshim.c");
    let (out, var) = if cfg!(target_os = "macos") {
        (dir.join("faultshim.dylib"), "DYLD_INSERT_LIBRARIES")
    } else {
        (dir.join("faultshim.so"), "LD_PRELOAD")
    };
    let mut cc = Command::new("cc");
    if cfg!(target_os = "macos") {
        cc.arg("-dynamiclib");
    } else {
        cc.args(["-shared", "-fPIC"]);
    }
    cc.args(["-O1", "-Wall", "-o"]).arg(&out).arg(&src);
    if cfg!(target_os = "linux") {
        cc.arg("-ldl");
    }
    match cc.output() {
        Ok(o) if o.status.success() => Some((out, var)),
        other => {
            let why = match other {
                Ok(o) => String::from_utf8_lossy(&o.stderr).into_owned(),
                Err(e) => e.to_string(),
            };
            assert!(
                std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true"),
                "CI's runner carries a C compiler and the save's fault shim did not build: {why}"
            );
            eprintln!(
                "SKIP: no C compiler built the save's fault shim ({why}) — the witness did not run"
            );
            None
        }
    }
}

/// One boundary the shim logged: its number and what it was.
#[cfg(unix)]
#[derive(Debug, Clone)]
struct Boundary {
    n: usize,
    what: String,
}

#[cfg(unix)]
fn read_log(path: &Path) -> Vec<Boundary> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (n, what) = l.split_once(' ')?;
            Some(Boundary {
                n: n.parse().ok()?,
                what: what.to_string(),
            })
        })
        .collect()
}

/// #796 (`[os.fs.sync]`): the atomic save under a fault shim that ends
/// the program (`_exit(77)`, a process ending, not a signal) BEFORE each
/// storage boundary in turn — the opens, writes, duplicates, syncs,
/// closes and the rename on the save's own files. Whole, the save's
/// boundaries are in the durable order: the temporary file opened
/// exclusively, its writes, a sync of IT after the last write and before
/// the rename, the rename over the target, a sync of the DIRECTORY after
/// it. Ended at boundary N, `doc.txt` holds the old bytes when N is at or
/// before the rename and the new ones after it — whole, never a mix and
/// never neither. On every machine (the checked machine is the `wolf`
/// process under `conform-run`, its own calls the shim's). Red at s248's
/// prediction commit: E0301; with `fs_sync` made a no-op, the order is
/// red (no sync of the temporary file).
#[cfg(unix)]
#[test]
fn the_save_survives_an_end_at_every_storage_boundary() {
    let dir = scratch("save");
    let Some((shim, var)) = build_shim(&dir) else {
        return;
    };
    let entry = fixture("save.lu");
    let old = "old\n";
    let new = "new 0\nnew 1\nnew 2\nnew 3\n";
    let mut machines: Vec<(String, Vec<String>)> = vec![(
        "checked".into(),
        vec![
            wolf().into(),
            "conform-run".into(),
            entry.to_string_lossy().into_owned(),
            "--checked".into(),
            "--json".into(),
        ],
    )];
    for release in [false, true] {
        if let Some(exe) = build(&entry, &dir, "save", release) {
            let tier = if release { "release" } else { "native" };
            machines.push((tier.into(), vec![exe.to_string_lossy().into_owned()]));
        }
    }
    let work = dir.join("w");
    let log = dir.join("faults.log");
    let reset = || {
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).expect("the save's directory");
        std::fs::write(work.join("doc.txt"), old).expect("the old file");
        let _ = std::fs::remove_file(&log);
    };
    let run = |argv: &[String], at: Option<usize>| -> Output {
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .env(var, &shim)
            .env("S248_FAULT_LOG", &log)
            .env_remove("S248_FAULT_AT");
        if let Some(n) = at {
            cmd.env("S248_FAULT_AT", n.to_string());
        }
        run_in(cmd, &work)
    };
    for (lane, argv) in &machines {
        reset();
        let out = run(argv, None);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.code(),
            Some(0),
            "the {lane} save, whole: {stderr}\n{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert_eq!(
            std::fs::read_to_string(work.join("doc.txt")).expect("doc.txt"),
            new,
            "the {lane} save, whole, wrote the new bytes"
        );
        let whole = read_log(&log);
        assert!(
            !whole.is_empty(),
            "the {lane} save: the shim saw no boundary"
        );
        let pos = |want: &dyn Fn(&str) -> bool| whole.iter().position(|b| want(&b.what));
        let rpos = |want: &dyn Fn(&str) -> bool| whole.iter().rposition(|b| want(&b.what));
        let open = pos(&|w| w == "open tmp excl=1");
        let last_write = rpos(&|w| w.starts_with("write tmp "));
        let sync_tmp = rpos(&|w| w.starts_with("sync tmp "));
        let rename = pos(&|w| w == "rename tmp target");
        let sync_dir = rpos(&|w| w.starts_with("sync dir "));
        let order = format!(
            "open={open:?} last_write={last_write:?} sync_tmp={sync_tmp:?} rename={rename:?} \
             sync_dir={sync_dir:?} in {whole:?}"
        );
        let (Some(open), Some(last_write), Some(sync_tmp), Some(rename), Some(sync_dir)) =
            (open, last_write, sync_tmp, rename, sync_dir)
        else {
            panic!("the {lane} save is missing a durable boundary: {order}");
        };
        assert!(
            open < last_write && last_write < sync_tmp && sync_tmp < rename && rename < sync_dir,
            "the {lane} save's boundaries are not in the durable order: {order}"
        );
        let times = stderr
            .lines()
            .find(|l| l.contains("s248-save "))
            .unwrap_or("")
            .to_string();
        report(&format!(
            "REPORT save {lane}: {} boundaries {:?}; {times}",
            whole.len(),
            whole.iter().map(|b| b.what.as_str()).collect::<Vec<_>>()
        ));
        let rename_n = whole[rename].n;
        for b in &whole {
            reset();
            let out = run(argv, Some(b.n));
            assert_eq!(
                out.status.code(),
                Some(77),
                "the {lane} save ended at boundary {} ({}): {}",
                b.n,
                b.what,
                String::from_utf8_lossy(&out.stderr)
            );
            let now = std::fs::read_to_string(work.join("doc.txt")).unwrap_or_else(|e| {
                panic!("the {lane} save ended at {} left no doc.txt: {e}", b.n)
            });
            let want = if b.n <= rename_n { old } else { new };
            assert_eq!(
                now, want,
                "the {lane} save ended before boundary {} ({}) left doc.txt torn or wrong",
                b.n, b.what
            );
        }
        eprintln!("PASS the {lane} save at each of {} boundaries", whole.len());
    }
    if let Some(l) = lupin_or_skip() {
        reset();
        let mut cmd = Command::new(&l);
        cmd.arg("conform-run").arg(&entry).arg("--json");
        let out = run_in(cmd, &work);
        judge_lupin(
            &parse_obs(&out.stdout, "lupin's observation"),
            "save.lu",
            "saved\n",
            &["os_clock_ns", "fs_sync", "fs_sync_dir"],
        );
    }
}

// ------------------------------------------------------------ #795 --

/// #795 (`[os.signal.wait.fds]`): the rows of the wait that need no
/// terminal, one truth on every host, windows too — a set the shape rule
/// refuses is `io`, a deadline below -1 `invalid`, each before any handle
/// is looked at. The live rows (the terminal, a pipe, `RESIZE`, the
/// deadline, the race) are `editor_wait_lanes.rs`'s (unix).
#[test]
fn the_wait_fds_rows_on_every_machine() {
    let dir = scratch("wait_fds_rows");
    every_lane(
        &fixture("wait_fds_rows.lu"),
        &dir,
        &["os_signal_wait_fds"],
        &|lane, stdout| {
            assert_eq!(
                stdout, "empty=io send_only=io past_table=io below=invalid far_below=invalid\n",
                "the {lane} machine's wait_fds rows"
            );
        },
    );
}

// ------------------------------------------------------------ #794 --

/// #794 (`[os.term.restore]`): the registration's rows that need no
/// terminal — a mode outside the bits `invalid` before the handle is read
/// (windows too), then windows `unsupported`, then a closed, forged or
/// non-terminal handle `io` (ruling asked R1). The restores themselves are
/// `editor_restore_lanes.rs`'s (unix, on pseudo-terminals).
#[test]
fn the_restore_rows_on_every_machine() {
    let dir = scratch("restore_rows");
    every_lane(
        &fixture("restore_rows.lu"),
        &dir,
        &["os_term_restore_at_exit"],
        &|lane, stdout| {
            let want = if cfg!(unix) {
                "bit24=invalid negative=invalid forged=io file=io\n"
            } else {
                "bit24=invalid negative=invalid forged=unsupported file=unsupported\n"
            };
            assert_eq!(stdout, want, "the {lane} machine's restore rows");
        },
    );
}
