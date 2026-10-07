//! s215 (pelt's H2) — a child's descriptors, a pipe, the working
//! directory and the terminal question, on every machine.
//! `[os.proc.fds]`, `[os.proc.pipe]`, `[os.fs.chdir]`, `[os.fs.isatty]`.
//!
//! Before this lane a wolf program could start a child only with 0 the
//! null device and 1 and 2 its own (`[os.proc.spawn]`): no pipe, no
//! redirection, no way to read what a child said, no `cd`, and no way
//! to ask whether a stream is a terminal — the plumbing a shell is made
//! of. Two kinds of witness:
//!
//! - the corpus rows `os/pipe_round_trip.lu`, `os/chdir_relative.lu`
//!   and `os/spawn_fds_rows.lu`, which spawn nothing and run on every
//!   host, under `conform-run` on the checked machine, the native and
//!   release tiers and lupin (`cargo xtask corpus` runs a row on the
//!   native lane only — the s171 rule);
//! - the fixtures under `fixtures/proc_fd/`, which start host programs
//!   (`printf`, `wc`, `ls`, `cat`), so they run on unix only: the
//!   checked machine and lupin under `conform-run`, the native and
//!   release binaries `wolf build` made — plain, and on linux again
//!   under `taskset -c 0-3` (wolf-lang#570: a spawn-heavy program
//!   green on a 16-cpu host can starve on a 4-cpu runner).
//!
//! lupin 0.1.47 (the pairing) predates the mirror: it resolves none of
//! the four names, so its answers are pinned by version and release
//! commit as pre-mirror (s180's design), never widened. The mirror is
//! wolf-interp's s215 PR; at it, every row agrees except `fd_five`,
//! which lupin refuses by name (a descriptor above 2 needs `unsafe`,
//! which lupin forbids).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin releases that predate the mirror, with the commit each
/// release archive reports (a development build of the mirror still
/// says the last released version; the commit holds it apart).
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] = &[("0.1.47", Some("b3228cb"))];

fn pre_mirror(lupin: &Obs) -> bool {
    PRE_MIRROR_LUPIN
        .iter()
        .any(|(v, c)| *v == lupin.version && c.is_none_or(|c| lupin.commit.starts_with(c)))
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    commit: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    // Under `script(1)` the record arrives with `\r\n` line ends and
    // whatever the pseudo-terminal echoed first — BSD `script` on macOS
    // writes `^D` ahead of it when its input ends (run 37555223637, job
    // 112579792734): the record is the line that parses from its first
    // `{`.
    let text = String::from_utf8_lossy(bytes).replace('\r', "");
    let rec: serde_json::Value = text
        .lines()
        .rev()
        .find_map(|l| {
            l.find('{')
                .and_then(|i| serde_json::from_str(l[i..].trim()).ok())
        })
        .unwrap_or_else(|| panic!("{what} record parses: {text}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
    }
}

/// A working directory of the test's own (`[os.fs.path]`), holding the
/// `target/` the witnesses write under.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("proc_fd_lanes")
        .join(test);
    std::fs::create_dir_all(d.join("target")).expect("scratch dir");
    d
}

/// Run `cmd` in `dir` with 0 the null device and 1, 2 piped.
fn run_in(mut cmd: Command, dir: &Path) -> Output {
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("spawn")
}

/// `argv` under `script(1)`, so that 0, 1 and 2 are a pseudo-terminal.
/// The two hosts spell it differently (util-linux `-qec CMD FILE`, BSD
/// `-q FILE CMD...`); `None` is a host without `script`.
fn under_script(argv: &[String]) -> Option<Command> {
    if !Path::new("/usr/bin/script").exists() && !Path::new("/bin/script").exists() {
        return None;
    }
    let mut c = Command::new("script");
    if cfg!(target_os = "linux") {
        let line = argv
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', r"'\''")))
            .collect::<Vec<_>>()
            .join(" ");
        c.args(["-qec", &line, "/dev/null"]);
    } else {
        c.args(["-q", "/dev/null"]).args(argv);
    }
    Some(c)
}

/// `taskset -c 0-3` in front of `argv` on linux, when the host has it.
fn under_taskset(argv: &[String]) -> Option<Command> {
    if !cfg!(target_os = "linux") || !Path::new("/usr/bin/taskset").exists() {
        return None;
    }
    let mut c = Command::new("taskset");
    c.args(["-c", "0-3"]).args(argv);
    Some(c)
}

/// One wolfgang lane under `conform-run`. `None` is a host that cannot
/// run that lane (the s59 skip pattern), never a silent pass.
fn conform(entry: &Path, dir: &Path, flag: &str) -> Option<Obs> {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run").arg(entry).arg(flag).arg("--json");
    let out = run_in(cmd, dir);
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, "the observation"))
}

/// The native or release binary of `entry`. A compile error FAILS the
/// gate (it is the defect at a pre-fix head, never an environment).
fn build(entry: &Path, dir: &Path, release: bool) -> Option<PathBuf> {
    let exe = dir.join(format!(
        "a-{}{}",
        if release { "release" } else { "native" },
        std::env::consts::EXE_SUFFIX
    ));
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
            "SKIP: environment cannot build the {} tier: {}",
            if release { "release" } else { "native" },
            stderr.trim()
        );
        return None;
    }
    assert!(out.status.success(), "wolf build failed: {stderr}");
    Some(exe)
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

/// lupin's observation, or `None` when this box has no sibling —
/// a failure when `WOLF_PAIRING_REQUIRE_SIBLING` says one was arranged.
fn lupin_says(entry: &Path, dir: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s215's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return None;
    };
    let mut cmd = Command::new(&lupin);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let out = run_in(cmd, dir);
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/proc_fd")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// What lupin must answer: `Ran(stdout)` — a run to `exit(0)` printing
/// it — or `Refused` — the whole-program `unsupported` a by-name
/// refusal is.
#[derive(Clone, Copy, Debug)]
enum Lupin<'a> {
    Ran(&'a str),
    Refused,
}

/// lupin's answer: `mirrored` from a mirrored lupin, `pre` (verdict,
/// stdout) from a version named in [`PRE_MIRROR_LUPIN`].
fn lupin_agrees(lupin: &Obs, mirrored: Lupin<'_>, pre: (&str, &str), what: &str) {
    let got = (lupin.verdict.as_str(), lupin.stdout.as_str());
    if pre_mirror(lupin) {
        assert_eq!(
            got, pre,
            "lupin {} at {} (pre-mirror, pinned by version) on {what}",
            lupin.version, lupin.commit
        );
        return;
    }
    match mirrored {
        Lupin::Ran(want) => assert_eq!(
            got,
            ("exit(0)", want),
            "lupin {} at {} on {what} (s215 mirrored)",
            lupin.version,
            lupin.commit
        ),
        Lupin::Refused => assert_eq!(
            lupin.verdict, "unsupported",
            "lupin {} at {} on {what}: a descriptor above 2 is refused by name",
            lupin.version, lupin.commit
        ),
    }
}

/// The verdict lupin 0.1.47 gives a program that calls a name it does
/// not resolve (measured at trunk 294d626d; `red-trunk-*.log`).
const PRE: (&str, &str) = ("unsupported", "");

/// A corpus row: every wolfgang lane under `conform-run` prints `want`,
/// and lupin agrees or answers its pinned pre-mirror verdict.
fn every_lane_says(rel: &str, want: &str) {
    let entry = corpus(rel);
    let dir = scratch(&rel.replace(['/', '.'], "_"));
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = conform(&entry, &dir, flag) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "the {flag} lane on {rel} (s215)"
        );
    }
    if let Some(lupin) = lupin_says(&entry, &dir) {
        lupin_agrees(&lupin, Lupin::Ran(want), PRE, rel);
    }
}

/// A unix fixture on every machine: the checked machine and lupin under
/// `conform-run`, the native and release binaries run directly — and on
/// linux again under `taskset -c 0-3`. `check` judges each stdout.
#[cfg(unix)]
fn every_lane_runs(name: &str, mirrored: Lupin<'_>, check: &dyn Fn(&str, &str)) {
    let entry = fixture(name);
    let dir = scratch(&name.replace('.', "_"));
    let checked = conform(&entry, &dir, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked.verdict, "exit(0)",
        "the CHECKED lane on {name}: {checked:?}"
    );
    check(&checked.stdout, &format!("checked on {name}"));
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        let argv = vec![exe.to_string_lossy().into_owned()];
        let mut runs = vec![("plain", Command::new(&exe))];
        if let Some(t) = under_taskset(&argv) {
            runs.push(("taskset -c 0-3", t));
        }
        for (how, cmd) in runs {
            let out = run_in(cmd, &dir);
            assert_eq!(
                out.status.code(),
                Some(0),
                "the {tier} binary ({how}) on {name}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            check(
                &String::from_utf8_lossy(&out.stdout),
                &format!("{tier} ({how}) on {name}"),
            );
        }
    }
    if let Some(lupin) = lupin_says(&entry, &dir) {
        if pre_mirror(&lupin) {
            lupin_agrees(&lupin, mirrored, PRE, name);
        } else if let Lupin::Ran(_) = mirrored {
            assert_eq!(lupin.verdict, "exit(0)", "lupin on {name}: {lupin:?}");
            check(&lupin.stdout, &format!("lupin {} on {name}", lupin.version));
        } else {
            lupin_agrees(&lupin, mirrored, PRE, name);
        }
    }
}

/// A fixture whose output must be exactly `want` on every machine.
#[cfg(unix)]
fn every_lane_prints(name: &str, want: &str, mirrored: Lupin<'_>) {
    every_lane_runs(name, mirrored, &|got, what| {
        assert_eq!(got, want, "{what} (s215)");
    });
}

// ------------------------------------------------ the corpus rows --

/// A pipe inside one process: the ends are fs handles above 2, the
/// bytes cross, closing the writer is the reader's `eof`, neither end
/// is a terminal, closed and forged handles are `io`. Red at trunk
/// 294d626d: E0301 (`os_pipe`, `os_isatty` unknown) on every wolfgang
/// lane; lupin 0.1.47 `unsupported`.
#[test]
fn a_pipe_round_trips_inside_one_process() {
    every_lane_says(
        "os/pipe_round_trip.lu",
        "ends_above_2=true distinct=true\nread=howl\nthen=eof\n\
write_end_tty=false read_end_tty=false\nclosed_tty=io forged_tty=io\nwrite_after_close=io\n",
    );
}

/// `cd`, then a relative open: the file under the new directory is
/// found, `os_cwd` answers it, `..` walks up, the first directory comes
/// back exactly; missing is `not_found`, a file is `io`. Red at trunk:
/// E0301 (`os_chdir` unknown); lupin 0.1.47 `unsupported`.
#[test]
fn chdir_then_a_relative_open() {
    every_lane_says(
        "os/chdir_relative.lu",
        "moved=true\nrelative_read=found\ncwd_ends_inner=true\nup_sees_inner=true\nback=true\n\
missing=not_found\nonto_a_file=io\ncleaned=true\n",
    );
}

/// The map's rows that make no child: its shape is `invalid` before
/// anything is read; an empty map is the plain spawn. Red at trunk:
/// E0301 (`os_spawn_fds` unknown); lupin 0.1.47 `unsupported`.
#[test]
fn a_bad_map_is_invalid_before_any_child() {
    every_lane_says(
        "os/spawn_fds_rows.lu",
        "odd=invalid\nout_of_range=invalid\nnegative_target=invalid\nrepeated=invalid\n\
bad_source=invalid\nempty_exe=not_found\nno_program=not_found\n",
    );
}

// ------------------------------------------- the unix fixtures --

/// `printf 'a\nb\nc\n' | wc -l`, built from argv: three lines, both
/// stages exit 0. `wc -l` pads its count on macOS; the fixture trims.
#[cfg(unix)]
#[test]
fn a_two_stage_pipeline_from_argv() {
    every_lane_prints(
        "pipeline.lu",
        "lines=3 printf=0 wc=0\n",
        Lupin::Ran("lines=3 printf=0 wc=0\n"),
    );
}

/// `2>&1` onto a pipe: `ls`'s complaint about a missing path arrives in
/// the pipe when 1 and 2 share it, and not when the map names 1 alone.
#[cfg(unix)]
#[test]
fn stderr_joins_stdout_on_a_pipe() {
    let want = "merged named=true failed=true\nstdout_only named=false failed=true\n";
    every_lane_prints("stderr_merge.lu", want, Lupin::Ran(want));
}

/// A child that inherits an explicit descriptor 5 reads what this
/// program put there. lupin refuses it by name.
#[cfg(unix)]
#[test]
fn a_child_reads_an_explicit_fd_five() {
    every_lane_prints("fd_five.lu", "fd5=five code=0\n", Lupin::Refused);
}

/// No descriptor leaks: the child's `/dev/fd` listing equals the
/// listing of the same `ls` started by this test, with a file, a pipe
/// and a listener open in the program.
#[cfg(unix)]
#[test]
fn no_descriptor_leaks_into_a_child() {
    let dir = scratch("no_leak_control");
    let control = run_in(
        {
            let mut c = Command::new("ls");
            c.arg("/dev/fd/");
            c
        },
        &dir,
    );
    assert!(control.status.success(), "the control ls runs");
    let listing: Vec<String> = String::from_utf8_lossy(&control.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let want = format!("listing={} code=0\n", listing.join(","));
    eprintln!("control /dev/fd listing: {}", listing.join(","));
    every_lane_prints("no_leak.lu", &want, Lupin::Ran(&want));
}

/// A child starts in the directory the program moved to — on the
/// checked machine, its machine-local one.
#[cfg(unix)]
#[test]
fn a_child_starts_in_the_moved_directory() {
    let want = "child_lists=mark.txt code=0\n";
    every_lane_prints("chdir_spawn.lu", want, Lupin::Ran(want));
}

/// `isatty` false everywhere with 0 the null device and 2 a pipe.
#[cfg(unix)]
#[test]
fn isatty_is_false_under_a_pipe() {
    let want = "in=false err=false pipe=false\n";
    every_lane_prints("isatty.lu", want, Lupin::Ran(want));
}

/// `isatty` true for 0 and 2 under `script(1)`'s pseudo-terminal, and
/// still false for the pipe end. The native and release binaries run
/// under `script` directly; the checked machine and lupin run their
/// `conform-run` under it.
#[cfg(unix)]
#[test]
fn isatty_is_true_under_a_pty() {
    let want = "in=true err=true pipe=false\n";
    let entry = fixture("isatty.lu");
    let dir = scratch("isatty_pty");
    let Some(_) = under_script(&["true".to_string()]) else {
        eprintln!("SKIP: no script(1) on this host — the pty leg is absent");
        return;
    };
    let script_obs = |argv: Vec<String>, what: &str| -> Obs {
        let out = run_in(under_script(&argv).expect("script"), &dir);
        assert!(
            out.status.success(),
            "{what} under script: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        parse_obs(&out.stdout, what)
    };
    let w = wolf().to_string();
    let e = entry.to_string_lossy().into_owned();
    let checked = script_obs(
        vec![
            w.clone(),
            "conform-run".into(),
            e.clone(),
            "--checked".into(),
            "--json".into(),
        ],
        "checked",
    );
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_str()),
        ("exit(0)", want),
        "the CHECKED lane under a pty"
    );
    for release in [false, true] {
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        let out = run_in(
            under_script(&[exe.to_string_lossy().into_owned()]).expect("script"),
            &dir,
        );
        let got = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        assert!(
            got.lines().any(|l| format!("{l}\n") == want),
            "the {} binary under a pty printed {got:?}",
            if release { "release" } else { "native" }
        );
    }
    if let Some(lupin) = sibling_lupin() {
        let obs = script_obs(
            vec![
                lupin.to_string_lossy().into_owned(),
                "conform-run".into(),
                e,
                "--json".into(),
            ],
            "lupin",
        );
        lupin_agrees(&obs, Lupin::Ran(want), PRE, "isatty.lu under a pty");
    } else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
    }
}
