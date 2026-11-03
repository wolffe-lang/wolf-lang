//! s277 (wolf-lang#707, #1029, #1030, #1031, #1032, #1033) — the descriptor
//! walk: calls relative to a directory the program HOLDS (`openat`,
//! `fstatat`, `unlinkat`, `mkdirat`, `symlinkat`, `linkat`, a held
//! directory's listing), an open that refuses a non-directory (bit 32,
//! `O_DIRECTORY`) and the mode set on a handle (`fchmod`). `[os.fs.at]`,
//! `[os.fs.open.directory]`, `[os.fs.mode.fd]`.
//!
//! Each witness runs on the checked machine under `conform-run` and as
//! native and release binaries, and lupin is held to its pairing; every
//! machine's answer is printed, so a `--nocapture` log is the table. A
//! wrong answer prints `FAIL <row> …` on a line of its own before the test
//! fails, so a planted break is red BY NAME in a gauntlet record.
//!
//! - EVERY HOST (windows too): the six corpus rows, one per issue, each
//!   asserted on both sides in the program (served on unix; `unsupported`
//!   or `invalid` by name on windows).
//! - UNIX: the race row (the test is the second process that swaps a
//!   directory for a symbolic link between the walk's checks and its
//!   uses: the handle walk stays inside the tree, the path walk in the
//!   same row escapes); a tree deeper than `PATH_MAX` made and removed by
//!   handle where the path form fails; every refusal by name with the
//!   host's number.
//! - LIVE (linux): the privileged forms in a throwaway `--network none`
//!   container, once as a user (4242) and once as root inside it — s242's
//!   engine (`WOLF_JOBWAIT_ENGINE`, else docker then podman;
//!   `WOLF_JOBWAIT_IMAGE`, default ubuntu:24.04), and
//!   `WOLF_JOBWAIT_REQUIRE_CONTAINER=1` makes a missing engine a failure,
//!   never a skip. boreutils' `tools/roottest` is the shape (read, not
//!   moved), as s272's container rows are.
//!
//! **lupin 0.1.58**, the pairing, predates every new name: each is pinned
//! here by version, exactly as measured ("does not resolve"); a newer lupin
//! is held to the mirror (the wolf-interp issue this lane drafts).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A `REPORT` line for the CI log even when the test passes.
#[cfg(unix)]
fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

/// A wrong answer, BY NAME, on a line of its own — the plant record's
/// evidence (`FAIL <row>` inside a red step).
#[cfg(unix)]
fn fail_row(row: &str, why: &str) -> String {
    let line = format!("FAIL {row} {why}");
    report(&line);
    line
}

/// The pairing at this lane, which predates the mirror of the ten names:
/// its measured answers are pinned per row.
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.58"];

/// Every name this lane adds, for lupin's "does not resolve" refusal.
const NEW_NAMES: &[&str] = &[
    "fs_open_at",
    "fs_stat_at",
    "fs_lstat_at",
    "fs_remove_at",
    "fs_remove_dir_at",
    "fs_create_dir_at",
    "fs_symlink_at",
    "fs_link_at",
    "fs_read_dir_entries_at",
    "fs_set_mode_fd",
];

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

#[cfg(unix)]
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/descriptor_walk")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// The stdout a corpus row's own header declares.
fn declared_stdout(entry: &Path) -> String {
    let text = std::fs::read_to_string(entry).expect("corpus row reads");
    let line = text
        .lines()
        .find(|l| l.starts_with("//! check: run("))
        .unwrap_or_else(|| panic!("{} declares a run", entry.display()));
    let start = line.find("stdout=\"").expect("a declared stdout") + "stdout=\"".len();
    let end = line.rfind("\")").expect("the directive closes");
    line[start..end].replace("\\n", "\n") + "\n"
}

/// A fresh working directory of the test's own.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("descriptor_walk_lanes")
        .join(test);
    if d.exists() {
        // A row that failed half-way may leave a mode it set; open them up.
        let _ = Command::new("chmod")
            .arg("-R")
            .arg("u+rwx")
            .arg(&d)
            .status();
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
             (LUPIN={}) — the oracle leg of s277's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    found
}

/// lupin's answer to `entry`: the pairing (0.1.58) stops at a name it does
/// not resolve, by name; a newer lupin mirrors the compiler (`want`, when
/// the row has one stdout for every machine).
fn lupin_leg(entry: &Path, dir: &Path, want: Option<&str>) {
    let Some(l) = lupin_or_skip() else {
        return;
    };
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(&l);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let out = run_in(cmd, dir);
    let obs = parse_obs(&out.stdout, "lupin's observation");
    eprintln!(
        "== lupin {} on {name}: {} {}\n{}",
        obs.version, obs.verdict, obs.reason, obs.stdout
    );
    if PRE_MIRROR_LUPIN.contains(&obs.version.as_str()) {
        assert_eq!(
            obs.verdict, "unsupported",
            "lupin {} refuses {name} by name: {obs:?}",
            obs.version
        );
        assert!(
            NEW_NAMES
                .iter()
                .any(|c| obs.reason.contains(&format!("`{c}`"))),
            "lupin {}'s refusal of {name} names one of the new calls: {}",
            obs.version,
            obs.reason
        );
        assert!(
            obs.reason.contains("does not resolve"),
            "lupin {} (pre-mirror) does not resolve the new calls: {}",
            obs.version,
            obs.reason
        );
    } else if let Some(w) = want {
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", w),
            "lupin {} on {name}: {obs:?}",
            obs.version
        );
    }
}

/// The checked machine's record of `entry`, run in `dir`.
fn checked_in(entry: &Path, dir: &Path) -> Obs {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    let out = run_in(cmd, dir);
    let obs = parse_obs(&out.stdout, "the observation");
    eprintln!(
        "== checked on {name}: {} {}\n{}{}",
        obs.verdict,
        obs.reason,
        obs.stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    obs
}

/// `entry` on every machine, ON THE HOST, in `dir`: the checked machine
/// under `conform-run`, the native and release binaries run directly, each
/// stdout held to `want`; then lupin's leg.
fn every_lane_answers_on_the_host(entry: &Path, dir: &Path, want: &str) {
    let name = entry.file_name().unwrap().to_string_lossy().to_string();
    let checked = checked_in(entry, dir);
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_str()),
        ("exit(0)", want),
        "the CHECKED machine on {name}: {checked:?}"
    );
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(entry, dir, "a", release) else {
            continue;
        };
        let out = run_in(Command::new(&exe), dir);
        let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        eprintln!(
            "== {tier} on {name}: exit {:?}\n{stdout}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(0), "the {tier} binary on {name}");
        assert_eq!(stdout, want, "the {tier} binary on {name}");
    }
    lupin_leg(entry, dir, Some(want));
}

/// The machines a unix row runs on the host: the checked machine (its
/// stdout from the record) and the native and release binaries of
/// `entry`, built into `dir`.
#[cfg(unix)]
fn host_machines(entry: &Path, dir: &Path, stem: &str) -> Vec<(String, Command, bool)> {
    let mut rows = Vec::new();
    let mut c = Command::new(wolf());
    c.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    rows.push(("checked".to_string(), c, true));
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        if let Some(exe) = build(entry, dir, stem, release) {
            rows.push((tier.to_string(), Command::new(exe), false));
        }
    }
    rows
}

/// A program's answer: the record's stdout for the checked machine, the
/// process's own for a binary.
#[cfg(unix)]
fn answer_of(out: &Output, record: bool, what: &str) -> String {
    if record {
        let obs = parse_obs(&out.stdout, what);
        assert_eq!(obs.verdict, "exit(0)", "{what}: {obs:?}");
        obs.stdout
    } else {
        assert_eq!(
            out.status.code(),
            Some(0),
            "{what}: {out:?}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).replace('\r', "")
    }
}

// ------------------------------------------------------ every host --

/// #707: the calls through a held handle reach the directory it names
/// after its path is renamed — the open, both stats, both removals, the
/// listing (twice); bu21's census witness `fs_remove_at(3, "name")` is a
/// forged handle's `io`. windows: the bit `invalid`, every call
/// `unsupported`. Red at `0d6b3a64`: E0301.
#[test]
fn the_at_rows_on_every_host() {
    let dir = scratch("at_rows");
    let entry = corpus("fs/at_rows.lu");
    every_lane_answers_on_the_host(&entry, &dir, &declared_stdout(&entry));
}

/// #1029: a directory made under a held parent after the parent's path
/// was renamed; `exists`, `not_found` and the shape's `invalid`. Red at
/// `0d6b3a64`: E0301.
#[test]
fn the_mkdir_at_rows_on_every_host() {
    let dir = scratch("mkdir_at");
    let entry = corpus("fs/mkdir_at.lu");
    every_lane_answers_on_the_host(&entry, &dir, &declared_stdout(&entry));
}

/// #1030: a link made under a held parent after the parent moved, its
/// target read back verbatim by the path form. Red at `0d6b3a64`: E0301.
#[test]
fn the_symlink_at_rows_on_every_host() {
    let dir = scratch("symlink_at");
    let entry = corpus("fs/symlink_at.lu");
    every_lane_answers_on_the_host(&entry, &dir, &declared_stdout(&entry));
}

/// #1031 (and #910's follow flag): a second name between two held
/// handles after both parents moved; a link itself and a link followed.
/// Red at `0d6b3a64`: E0301.
#[test]
fn the_link_at_rows_on_every_host() {
    let dir = scratch("link_at");
    let entry = corpus("fs/link_at.lu");
    every_lane_answers_on_the_host(&entry, &dir, &declared_stdout(&entry));
}

/// #1032: bit 32 refuses a regular file (`invalid`, ENOTDIR) and opens a
/// directory, which `fs_fstat_record` calls kind 1; it rides the read
/// modes only. Red at `0d6b3a64`: the bit was `invalid` on a directory.
#[test]
fn the_directory_bit_rows_on_every_host() {
    let dir = scratch("open_directory");
    let entry = corpus("fs/open_directory.lu");
    every_lane_answers_on_the_host(&entry, &dir, &declared_stdout(&entry));
}

/// #1033: the mode set through a handle changes the held file, not the
/// file renamed over its name. Red at `0d6b3a64`: E0301.
#[test]
fn the_mode_fd_rows_on_every_host() {
    let dir = scratch("mode_fd");
    let entry = corpus("fs/mode_fd.lu");
    every_lane_answers_on_the_host(&entry, &dir, &declared_stdout(&entry));
}

// ----------------------------------------------------------- unix --

/// One race case's tree, fresh: `tree/{d/x, e/y, f/z}` and
/// `outside/{x, y, z}`.
#[cfg(unix)]
fn race_tree(case: &Path) {
    for (sub, file) in [("d", "x"), ("e", "y"), ("f", "z")] {
        std::fs::create_dir_all(case.join("tree").join(sub)).expect("tree dir");
        std::fs::write(case.join("tree").join(sub).join(file), "inside").expect("inside");
    }
    std::fs::create_dir_all(case.join("outside")).expect("outside");
    for file in ["x", "y", "z"] {
        std::fs::write(case.join("outside").join(file), "outside").expect("outside file");
    }
}

/// The second process: wait for `READY`, then move `d`, `e` and `f` aside
/// and put a link to `../outside` in each place, then write `GO`.
#[cfg(unix)]
fn race_swap(case: &Path, what: &str) -> Result<(), String> {
    let ready = case.join("READY");
    let start = std::time::Instant::now();
    while !ready.exists() {
        if start.elapsed() > std::time::Duration::from_secs(120) {
            return Err(format!("{what}: no READY within 120 s"));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let tree = case.join("tree");
    for n in ["d", "e", "f"] {
        std::fs::rename(tree.join(n), tree.join(format!("{n}.moved")))
            .map_err(|e| format!("{what}: move {n}: {e}"))?;
        std::os::unix::fs::symlink("../outside", tree.join(n))
            .map_err(|e| format!("{what}: link {n}: {e}"))?;
    }
    std::fs::write(case.join("GO"), "go").map_err(|e| format!("{what}: GO: {e}"))
}

/// #707's safety half: a walk that checks `tree/d` and then uses it by
/// name follows whatever `tree/d` has become in between. The program makes
/// its checks, the test swaps each directory for a link out of the tree,
/// and the program makes its uses: through the HELD handle of `d` the
/// removal is the moved directory's; the open of `e` by handle with the
/// directory and no-follow bits is refused (`invalid`, ENOTDIR 20 on both
/// hosts, measured) and touches nothing; the PATH walk's removal of
/// `tree/f/z` follows the link and removes the outside's `z` — the contrast,
/// shown in the same row. Red at `0d6b3a64`: E0301.
#[cfg(unix)]
#[test]
fn the_race_row_the_handle_walk_stays_inside_the_tree() {
    let dir = scratch("race");
    let entry = fixture("race.lu");
    let mut bad = Vec::new();
    for (machine, mut cmd, record) in host_machines(&entry, &dir, "race") {
        let case = dir.join(format!("case-{}", machine.replace(' ', "-")));
        std::fs::create_dir_all(&case).expect("case dir");
        race_tree(&case);
        let child = cmd
            .current_dir(&case)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the race program starts");
        let swapped = race_swap(&case, &machine);
        if let Err(why) = &swapped {
            bad.push(fail_row("race_harness", why));
            // GO anyway, so the program ends rather than waiting out its 60 s.
            let _ = std::fs::write(case.join("GO"), "go");
        }
        let out = child.wait_with_output().expect("the race program ends");
        let got = answer_of(&out, record, &format!("race on {machine}"));
        eprintln!(
            "== race on {machine}:\n{got}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let o = case.join("outside");
        let t = case.join("tree");
        let want_lines = [
            "checked: d=true e_kind=1 f_kind=1",
            "held_remove=ok",
            "asked_open_refused=true number=20 refused",
            "path_remove=ok",
        ];
        let lines: Vec<&str> = got.lines().collect();
        if lines != want_lines {
            bad.push(fail_row(
                "race_handle_walk",
                &format!("machine={machine}: got {lines:?}, want {want_lines:?}"),
            ));
        }
        let inside_x_gone = !t.join("d.moved/x").exists();
        let outside_kept = o.join("x").exists() && o.join("y").exists();
        if !(inside_x_gone && outside_kept) {
            bad.push(fail_row(
                "race_handle_walk",
                &format!(
                    "machine={machine}: the moved d's x gone={inside_x_gone}, outside x and y kept={outside_kept}"
                ),
            ));
        }
        let escaped = !o.join("z").exists() && t.join("f.moved/z").exists();
        if !escaped {
            bad.push(fail_row(
                "race_path_contrast",
                &format!("machine={machine}: the path walk did not escape (outside z kept, or the moved f's z gone)"),
            ));
        }
    }
    lupin_leg(&entry, &dir, None);
    assert!(bad.is_empty(), "race rows:\n{}", bad.join("\n"));
}

/// #707's depth half: 80 levels of 64-byte names (5204 bytes, past linux's
/// 4096 and macOS's 1024) made and removed by handle, where the whole
/// path's stat and mkdir are refused `io` with the host's number set. Red
/// at `0d6b3a64`: E0301.
#[cfg(unix)]
#[test]
fn a_tree_past_path_max_is_made_and_removed_by_handle() {
    let dir = scratch("depth");
    let entry = fixture("depth.lu");
    let want = "levels=80 path_bytes=5204\nbottom_is_a_directory: true\npath_stat=io number_set=true\npath_mkdir=io number_set=true\nmade_at_the_bottom: true\nremoved=80 gone=true\n";
    let mut bad = Vec::new();
    for (machine, cmd, record) in host_machines(&entry, &dir, "depth") {
        let out = run_in(cmd, &dir);
        let got = answer_of(&out, record, &format!("depth on {machine}"));
        eprintln!("== depth on {machine}:\n{got}");
        if got != want {
            bad.push(fail_row(
                "depth_past_path_max",
                &format!("machine={machine}: got {got:?}, want {want:?}"),
            ));
        }
        if dir.join("deep").exists() {
            bad.push(fail_row(
                "depth_past_path_max",
                &format!("machine={machine}: deep/ left behind"),
            ));
        }
    }
    lupin_leg(&entry, &dir, None);
    assert!(bad.is_empty(), "depth rows:\n{}", bad.join("\n"));
}

/// Each host's answer to every row of `refusals.lu` (measured on nomad,
/// Darwin 26.4.1, and on hasu, linux 7.2.4: `notes/s277-evidence/probe/`).
#[cfg(unix)]
fn refusal_rows() -> Vec<(&'static str, String)> {
    let mac = cfg!(target_os = "macos");
    let eloop = if mac {
        62
    } else if cfg!(target_os = "freebsd") {
        31
    } else {
        40
    };
    let notempty = if mac { 66 } else { 39 };
    let rows: Vec<(&str, String)> = vec![
        ("open_dir_file", "invalid number=20".into()),
        ("open_dir_nofollow_link", "invalid number=20".into()),
        ("open_dir_link_followed", "ok number=0".into()),
        ("open_nofollow_link", format!("invalid number={eloop}")),
        ("open_dir_write", "invalid number=0".into()),
        ("open_bit_64", "invalid number=0".into()),
        ("open_at_nofollow", format!("invalid number={eloop}")),
        ("open_at_dir_file", "invalid number=20".into()),
        ("open_at_dir_nofollow_link", "invalid number=20".into()),
        ("open_at_dir_write", "invalid number=0".into()),
        ("open_at_absolute", "invalid number=0".into()),
        ("open_at_empty", "invalid number=0".into()),
        ("open_at_perm", "invalid number=0".into()),
        ("open_at_mode", "invalid number=0".into()),
        ("open_at_file_handle", "invalid number=20".into()),
        ("open_at_forged", "io number=0".into()),
        ("open_at_missing", "not_found number=2".into()),
        ("open_at_exists", "exists number=17".into()),
        ("open_at_dangling_create", format!("invalid number={eloop}")),
        ("open_at_created", "ok number=0".into()),
        ("open_at_created_mode", "true number=0".into()),
        ("open_at_dotdot", "ok number=0".into()),
        ("lstat_at_missing", "not_found number=2".into()),
        ("lstat_at_link", "kind3 number=0".into()),
        ("stat_at_link", "kind0 number=0".into()),
        ("stat_at_dangling", "not_found number=2".into()),
        ("lstat_at_dangling", "kind3 number=0".into()),
        ("stat_at_under_a_file", "invalid number=20".into()),
        ("lstat_at_file_handle", "invalid number=20".into()),
        ("lstat_at_forged", "io number=0".into()),
        ("lstat_at_absolute", "invalid number=0".into()),
        (
            "remove_at_a_directory",
            if mac {
                "denied number=1"
            } else {
                "io number=21"
            }
            .into(),
        ),
        ("remove_at_missing", "not_found number=2".into()),
        ("remove_at_file_handle", "invalid number=20".into()),
        ("remove_dir_at_a_file", "invalid number=20".into()),
        ("remove_dir_at_a_link", "invalid number=20".into()),
        ("remove_dir_at_not_empty", format!("io number={notempty}")),
        ("remove_dir_at_missing", "not_found number=2".into()),
        ("mkdir_at_exists", "exists number=17".into()),
        ("mkdir_at_dangling", "exists number=17".into()),
        ("mkdir_at_missing_parent", "not_found number=2".into()),
        ("mkdir_at_file_handle", "invalid number=20".into()),
        ("mkdir_at_mode", "invalid number=0".into()),
        ("symlink_at_empty", "invalid number=0".into()),
        ("symlink_at_exists", "exists number=17".into()),
        ("symlink_at_file_handle", "invalid number=20".into()),
        ("link_at_a_directory", "denied number=1".into()),
        ("link_at_exists", "exists number=17".into()),
        ("link_at_dangling_followed", "not_found number=2".into()),
        ("link_at_dangling_itself", "ok number=0".into()),
        ("link_at_file_handle", "invalid number=20".into()),
        ("link_at_missing", "not_found number=2".into()),
        (
            "mode_fd_pipe",
            if mac { "io number=22" } else { "ok number=0" }.into(),
        ),
        ("mode_fd_directory", "ok number=0".into()),
        ("mode_fd_bits", "ok number=0".into()),
        ("mode_fd_bits_kept", "true number=0".into()),
        ("mode_fd_past_07777", "invalid number=0".into()),
        ("mode_fd_forged", "io number=0".into()),
        ("listing_file_handle", "invalid number=20".into()),
        ("listing_forged", "io number=0".into()),
    ];
    rows
}

/// Every refusal of the family by name, each with the host's number
/// (`os_error()`), on every machine: the two bits' refusals on the path
/// opens, each `…_at` call's shape (`invalid` before the host: an absolute
/// or empty name, a mode outside its set, the directory bit with a write
/// mode), a file handle passed as a directory and every no-follow refusal
/// (`invalid`, ENOTDIR or ELOOP), the hosts' own rows (`[os.fs.remove]`'s
/// split, `ENOTEMPTY`, `exists`, `not_found`, `denied`), a forged handle's
/// `io`, and `fchmod` on a pipe (linux ok, Darwin `io` 22, measured). Red
/// at `0d6b3a64`: E0301.
#[cfg(unix)]
#[test]
fn every_refusal_by_name_with_the_hosts_number() {
    if as_root() {
        eprintln!("SKIP: every_refusal_by_name: the test runs as root — these are a user's rows");
        return;
    }
    let dir = scratch("refusals");
    let entry = fixture("refusals.lu");
    let rows = refusal_rows();
    let mut bad = Vec::new();
    for (machine, cmd, record) in host_machines(&entry, &dir, "refusals") {
        let out = run_in(cmd, &dir);
        let got = answer_of(&out, record, &format!("refusals on {machine}"));
        eprintln!("== refusals on {machine}:\n{got}");
        let lines: Vec<&str> = got.lines().collect();
        for (i, (row, want)) in rows.iter().enumerate() {
            let want_line = format!("{row}={want}");
            match lines.get(i) {
                Some(l) if *l == want_line => {}
                other => bad.push(fail_row(
                    row,
                    &format!("machine={machine}: got {other:?}, want {want_line:?}"),
                )),
            }
        }
        if lines.len() != rows.len() {
            bad.push(fail_row(
                "refusal_rows",
                &format!(
                    "machine={machine}: {} lines, want {}",
                    lines.len(),
                    rows.len()
                ),
            ));
        }
    }
    lupin_leg(&entry, &dir, None);
    assert!(bad.is_empty(), "refusal rows:\n{}", bad.join("\n"));
}

/// Whether the test runs as the super-user (a user's refusal is then a
/// different row, and the test says so rather than guessing).
#[cfg(unix)]
fn as_root() -> bool {
    // SAFETY: geteuid cannot fail.
    unsafe { libc::geteuid() == 0 }
}

// ------------------------------ live: the privileged forms (linux) --

#[cfg(target_os = "linux")]
fn image() -> String {
    std::env::var("WOLF_JOBWAIT_IMAGE").unwrap_or_else(|_| "docker.io/library/ubuntu:24.04".into())
}

/// The engine that runs a throwaway container here, or a loud skip — a
/// failure under `WOLF_JOBWAIT_REQUIRE_CONTAINER` (s272's `engine`).
#[cfg(target_os = "linux")]
fn engine(test: &str) -> Option<String> {
    let required = std::env::var_os("WOLF_JOBWAIT_REQUIRE_CONTAINER").is_some();
    let candidates: Vec<String> = match std::env::var("WOLF_JOBWAIT_ENGINE") {
        Ok(e) => vec![e],
        Err(_) => vec!["docker".into(), "podman".into()],
    };
    let mut why = Vec::new();
    for e in &candidates {
        let out = Command::new(e)
            .args(["run", "--rm", "--network", "none"])
            .arg(image())
            .args(["sh", "-c", "echo $$"])
            .stdin(Stdio::null())
            .output();
        match out {
            Ok(o) if o.status.success() => {
                report(&format!("REPORT {test}: engine {e}, image {}", image()));
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
        !required,
        "WOLF_JOBWAIT_REQUIRE_CONTAINER is set and no engine ran {} — {test} did not run: {why:?}",
        image()
    );
    eprintln!(
        "SKIP: {test}: no container engine ran {} ({why:?}) — the live witnesses did not run",
        image()
    );
    None
}

/// The privileged case's layout, made by the container's root before the
/// program runs (`privilege.lu`'s header).
#[cfg(target_os = "linux")]
const PRIV_SETUP: &str = "set -e; P=/tmp/s277-priv; \
    mkdir -p $P/rootdir $P/sticky $P/userdir; \
    echo root > $P/rootdir/rootfile; echo root2 > $P/rootdir/rootfile2; \
    chmod 644 $P/rootdir/rootfile $P/rootdir/rootfile2; chmod 755 $P $P/rootdir; \
    chmod 1777 $P/sticky; echo root > $P/sticky/rootfile; chmod 644 $P/sticky/rootfile; \
    echo user > $P/userdir/userfile; chown -R 4242:4242 $P/userdir; chmod 755 $P/userdir; \
    set +e; ";

/// `cmd` in a throwaway container as its root, after [`PRIV_SETUP`]: no
/// network, the test's directory at `/w` read-only, the fixtures at `/fx`
/// and the `wolf` binary read-only; as the user (4242), the command is run
/// through `setpriv` with every group and capability dropped.
#[cfg(target_os = "linux")]
fn priv_container(engine: &str, dir: &Path, user: bool, cmd: &str) -> Output {
    let run = if user {
        format!(
            "setpriv --reuid=4242 --regid=4242 --clear-groups --inh-caps=-all --bounding-set=-all {cmd}"
        )
    } else {
        cmd.to_string()
    };
    let mut c = Command::new(engine);
    c.args([
        "run",
        "--rm",
        "--network",
        "none",
        "--user",
        "0:0",
        "-e",
        "HOME=/tmp",
    ]);
    c.arg("-v").arg(format!("{}:/w:ro", dir.display()));
    c.arg("-v").arg(format!(
        "{}:/fx:ro",
        fixture("privilege.lu").parent().unwrap().display()
    ));
    c.arg("-v").arg(format!("{}:/machines/wolf:ro", wolf()));
    c.args(["-w", "/w"])
        .arg(image())
        .args(["sh", "-c", &format!("{PRIV_SETUP}{run}")]);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c.output().expect("the engine runs")
}

/// The privileged forms in throwaway containers, once as a user (4242) and
/// once as root inside the container, on every machine: a hard link by
/// `fs_link_at` to a file another user owns (the user `denied` where the
/// host protects hard links — `/proc/sys/fs/protected_hardlinks`, read on
/// this host — root ok), a link into root's directory (the user `denied`,
/// EACCES), `fs_set_mode_fd` on root's file through a read handle (the user
/// `denied`, EPERM; root ok, the mode read back), and a directory, a
/// removal and a link made under root's 0755 directory and a removal of
/// root's file in a sticky directory (the user `denied`; root ok). Red at
/// `0d6b3a64`: E0301.
#[cfg(target_os = "linux")]
#[test]
fn the_privileged_forms_in_a_container_as_root_and_as_a_user() {
    let test = "privilege";
    let Some(engine) = engine(test) else {
        return;
    };
    let dir = scratch(test);
    let entry = fixture("privilege.lu");
    let protected = std::fs::read_to_string("/proc/sys/fs/protected_hardlinks")
        .map(|s| s.trim() == "1")
        .unwrap_or(true);
    report(&format!("REPORT {test}: protected_hardlinks={protected}"));
    let mut machines = vec![(
        "checked".to_string(),
        "/machines/wolf conform-run /fx/privilege.lu --checked --json".to_string(),
        true,
    )];
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        if let Some(exe) = build(&entry, &dir, "priv", release) {
            let name = exe.file_name().unwrap().to_string_lossy().to_string();
            machines.push((tier.to_string(), format!("/w/{name}"), false));
        }
    }
    let mut bad = Vec::new();
    for (who, user) in [("user", true), ("root", false)] {
        let want = if user {
            let across = if protected {
                "denied number=1"
            } else {
                "ok number=0"
            };
            format!(
                "euid=4242\nlink_across_owners={across}\nlink_into_roots_dir=denied number=13\n\
                 mode_on_roots_file=denied number=1\nmode_now=420\nmkdir_in_roots_dir=denied number=13\n\
                 remove_in_roots_dir=denied number=13\nsymlink_in_roots_dir=denied number=13\n\
                 remove_in_sticky=denied number=1\n"
            )
        } else {
            "euid=0\nlink_across_owners=ok number=0\nlink_into_roots_dir=ok number=0\n\
             mode_on_roots_file=ok number=0\nmode_now=384\nmkdir_in_roots_dir=ok number=0\n\
             remove_in_roots_dir=ok number=0\nsymlink_in_roots_dir=ok number=0\n\
             remove_in_sticky=ok number=0\n"
                .to_string()
        };
        for (machine, cmd, record) in &machines {
            let out = priv_container(&engine, &dir, user, cmd);
            let got = answer_of(&out, *record, &format!("privilege {who} {machine}"));
            eprintln!(
                "== privilege as {who}, {machine}:\n{got}{}",
                String::from_utf8_lossy(&out.stderr)
            );
            if got != want {
                bad.push(fail_row(
                    "privilege",
                    &format!("as {who}, {machine}: got {got:?}, want {want:?}"),
                ));
            }
        }
    }
    // lupin's refusal by name is the other rows'; a newer lupin would run
    // this program on the host, where its layout is not.
    assert!(bad.is_empty(), "privilege rows:\n{}", bad.join("\n"));
}
