//! s200 (wolf-lang#405, #411, #417, #407) — the byte surface boreutils'
//! B4 utilities wait on: bytes on descriptors 0 and 1 through the
//! descriptors themselves (`[os.fs.std]`), the bulk byte scan
//! (`[mem.list.bytes]`), the copy the host makes (`[os.fs.copy]`) and the
//! host's number beside the row (`[os.fs.error]`).
//!
//! Before this lane the byte calls answered `io` on 0, 1 and 2 (s199
//! served only the offset calls there), so `cat` reopened `/dev/stdin`
//! and `/dev/stdout` — on linux a fresh description that loses the
//! shell's offset, refuses a socket, and with descriptor 1 closed hands
//! the number to the program's own input (#405). No builtin scanned
//! bytes in bulk (`wc -l` 0.13x GNU, #411), every byte of a copy crossed
//! the user boundary twice (`cat` 0.16x GNU to `/dev/null`, #417), and a
//! failure could say THAT it failed but not why (#407).
//!
//! Three kinds of witness, as in `fs_std_lanes.rs`:
//!
//! - the corpus rows `fs/std_write_bytes.lu`, `fs/copy_chunk.lu`,
//!   `fs/os_error.lu` and `memory/bytes_scan.lu`, under `conform-run` on
//!   the checked machine, the native and release tiers and lupin;
//! - the fixtures under `fixtures/byte_surface/`, run with standard
//!   input a regular file, a pipe, and a file the parent has already
//!   read into (the offset is shared, so the program starts where the
//!   parent stopped and the parent sees where the program stopped). The
//!   native and release binaries are the ones `wolf build` made
//!   (`conform-run` hands its child the null device); the checked
//!   machine and lupin run in their own processes under `conform-run`
//!   and read the test's descriptor 0;
//! - the host's refusals, on the native and release binaries only
//!   (the checked machine and lupin capture descriptors 1 and 2 into
//!   their record): a descriptor 1 that cannot be written (`EBADF`, 9 on
//!   linux and macOS) and `/dev/full` (`ENOSPC`, 28, linux), each
//!   reported with the host's own words, as GNU reports them.
//!
//! lupin 0.1.47 and 0.1.48 predated the mirror and were pinned by
//! version AND release commit as pre-mirror (s180's design, s199's
//! precedent); 0.1.49 (the 0.2.26 pairing, r31) carries the mirror, so
//! every lupin is held to the ruled answers.

mod lane_exit;

use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin releases that predate the mirror, each with the commit its
/// release archive reports, so a development build of the mirror (which
/// still calls itself the last released version) is held to the ruled
/// answers.
///
/// 0.1.47 and 0.1.48 were listed; emptied at the 0.1.49 pairing (r31),
/// which carries the mirror.
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] = &[];

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
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
    }
}

/// The text the stdin witnesses copy: 19 bytes.
const INPUT: &[u8] = b"wolves at the door\n";

/// Every octet, twice, then a newline: what a byte-exact `cat` must
/// hand back unchanged (and what a UTF-8 decode would mangle).
fn binary_input() -> Vec<u8> {
    let mut v: Vec<u8> = (0..=255u8).chain(0..=255u8).collect();
    v.push(b'\n');
    v
}

/// What a test hands the program on descriptor 0.
#[derive(Clone, Copy, Debug)]
enum Stdin<'a> {
    /// A regular file holding the bytes.
    File(&'a [u8]),
    /// A regular file holding the bytes, of which the parent has already
    /// read the first `n` through the SAME description it hands over.
    FileAt(&'a [u8], u64),
    /// A pipe the test writes the bytes into and then closes.
    Pipe(&'a [u8]),
}

/// Where the program's descriptor 1 goes.
#[derive(Clone, Copy, Debug)]
enum Stdout {
    /// A pipe the test reads.
    Piped,
    /// A regular file the test reads afterwards.
    File,
    /// A file opened READ-ONLY: every write is the host's `EBADF`.
    #[cfg(unix)]
    ReadOnly,
    /// `/dev/null`.
    #[cfg(unix)]
    Null,
    /// `/dev/full`: every write is `ENOSPC`.
    #[cfg(target_os = "linux")]
    Full,
}

/// A run's outcome: the process's output, the bytes descriptor 1
/// received, and where the stdin description's offset stood afterwards
/// (`FileAt` only).
struct Ran {
    out: Output,
    stdout: Vec<u8>,
    offset_after: Option<u64>,
}

/// A working directory of the test's own (`[os.fs.path]`: relative
/// paths resolve against the process's cwd), so parallel tests never
/// share a file.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("byte_surface_lanes")
        .join(test);
    std::fs::create_dir_all(d.join("target")).expect("scratch dir");
    d
}

/// Run `cmd` in `dir` with descriptors 0 and 1 wired as asked, stderr
/// piped.
fn run_with(mut cmd: Command, dir: &Path, stdin: Stdin<'_>, stdout: Stdout, tag: &str) -> Ran {
    cmd.current_dir(dir).stderr(Stdio::piped());
    let mut held: Option<std::fs::File> = None;
    let mut pipe_bytes: Option<&[u8]> = None;
    match stdin {
        Stdin::File(bytes) | Stdin::FileAt(bytes, _) => {
            let p = dir.join(format!("{tag}.input"));
            std::fs::write(&p, bytes).expect("write the input file");
            let mut f = std::fs::File::open(&p).expect("open the input file");
            if let Stdin::FileAt(_, n) = stdin {
                let mut skip = vec![0u8; n as usize];
                f.read_exact(&mut skip).expect("the parent reads first");
                held = Some(f.try_clone().expect("dup the input"));
            }
            cmd.stdin(f);
        }
        Stdin::Pipe(bytes) => {
            cmd.stdin(Stdio::piped());
            pipe_bytes = Some(bytes);
        }
    }
    let out_path = dir.join(format!("{tag}.stdout"));
    match stdout {
        Stdout::Piped => {
            cmd.stdout(Stdio::piped());
        }
        Stdout::File => {
            cmd.stdout(std::fs::File::create(&out_path).expect("create the output file"));
        }
        #[cfg(unix)]
        Stdout::ReadOnly => {
            std::fs::write(&out_path, b"").expect("create the read-only file");
            cmd.stdout(std::fs::File::open(&out_path).expect("open read-only"));
        }
        #[cfg(unix)]
        Stdout::Null => {
            cmd.stdout(Stdio::null());
        }
        #[cfg(target_os = "linux")]
        Stdout::Full => {
            cmd.stdout(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .expect("open /dev/full"),
            );
        }
    }
    let mut child = cmd.spawn().expect("spawn");
    if let (Some(mut w), Some(bytes)) = (child.stdin.take(), pipe_bytes) {
        // A program that fails early may close the pipe first; EPIPE is
        // then the race, not the failure under test.
        if let Err(e) = w.write_all(bytes) {
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::BrokenPipe,
                "write the pipe: {e}"
            );
        }
    }
    let out = child.wait_with_output().expect("wait");
    let stdout = match stdout {
        Stdout::Piped => out.stdout.clone(),
        Stdout::File => std::fs::read(&out_path).expect("read the output file"),
        #[cfg(unix)]
        _ => Vec::new(),
    };
    let offset_after = held.map(|mut f| f.stream_position().expect("the shared offset"));
    Ran {
        out,
        stdout,
        offset_after,
    }
}

/// One wolfgang lane under `conform-run`. `None` means the host cannot
/// run that lane (the s59 skip pattern), never a silent pass.
fn conform(entry: &Path, dir: &Path, flag: &str, stdin: Stdin<'_>) -> Option<(Obs, Ran)> {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run").arg(entry).arg(flag).arg("--json");
    let ran = run_with(cmd, dir, stdin, Stdout::Piped, &format!("conform{flag}"));
    if flag != "--checked" && lane_exit::environment_refusal(&ran.out, &format!("wolf {flag}")) {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&ran.out.stderr).trim()
        );
        return None;
    }
    assert!(
        ran.out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&ran.out.stderr)
    );
    Some((parse_obs(&ran.out.stdout, "the observation"), ran))
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

/// lupin's observation, or `None` when this box has no sibling.
/// `WOLF_PAIRING_REQUIRE_SIBLING` makes an absent sibling a failure.
fn lupin_says(entry: &Path, dir: &Path, stdin: Stdin<'_>) -> Option<(Obs, Ran)> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s200's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return None;
    };
    let mut cmd = Command::new(&lupin);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let ran = run_with(cmd, dir, stdin, Stdout::Piped, "lupin");
    Some((parse_obs(&ran.out.stdout, "lupin's observation"), ran))
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
        .join("tests/fixtures/byte_surface")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// lupin's answer: `want` from a mirrored lupin, `pre` (verdict, stdout)
/// from a release named in [`PRE_MIRROR_LUPIN`].
fn lupin_agrees(lupin: &Obs, want: &str, pre: (&str, &str), what: &str) {
    if pre_mirror(lupin) {
        assert_eq!(
            (lupin.verdict.as_str(), lupin.stdout.as_str()),
            pre,
            "lupin {} at {} (pre-mirror, pinned by version) on {what}",
            lupin.version,
            lupin.commit
        );
    } else {
        assert_eq!(
            (lupin.verdict.as_str(), lupin.stdout.as_str()),
            ("exit(0)", want),
            "lupin {} at {} on {what} (s200 mirrored)",
            lupin.version,
            lupin.commit
        );
    }
}

/// A corpus row: every wolfgang lane under `conform-run` prints `want`
/// (the row's own `check:` line, read here so the two cannot part), and
/// lupin agrees or answers its pinned pre-mirror verdict.
fn every_lane_says(rel: &str, pre: (&str, &str)) {
    let entry = corpus(rel);
    let src = std::fs::read_to_string(&entry).expect("read the row");
    let want = check_stdout(&src);
    let dir = scratch(&rel.replace(['/', '.'], "_"));
    for flag in ["--checked", "--native", "--release"] {
        let Some((obs, _)) = conform(&entry, &dir, flag, Stdin::File(b"")) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want.as_str()),
            "the {flag} lane on {rel} (s200)"
        );
    }
    if let Some((lupin, _)) = lupin_says(&entry, &dir, Stdin::File(b"")) {
        lupin_agrees(&lupin, &want, pre, rel);
    }
}

/// The `stdout="…"` of a row's `//! check: run(…)` line, unescaped, with
/// the trailing newline the record keeps.
fn check_stdout(src: &str) -> String {
    let line = src
        .lines()
        .find(|l| l.starts_with("//! check: run("))
        .expect("a check line");
    let start = line.find("stdout=\"").expect("a stdout") + "stdout=\"".len();
    let body = &line[start..line.rfind("\")").expect("the closing quote")];
    let mut out = body.replace("\\n", "\n").replace("\\\"", "\"");
    out.push('\n');
    out
}

/// A stdin fixture on every lane: the checked machine and lupin under
/// `conform-run` (their stdout is the record's), the native and release
/// binaries run directly. Every wolfgang lane prints `want`, and where
/// the parent handed a file it had read into, the parent's offset
/// afterwards is `offset_after` on every lane (the description is
/// shared). Returns nothing; asserts.
fn every_lane_with_stdin(
    name: &str,
    stdin: Stdin<'_>,
    want: &[u8],
    offset_after: Option<u64>,
    pre: (&str, &str),
) {
    let entry = fixture(name);
    let what = format!("{name} with stdin {}", stdin_label(stdin));
    let dir = scratch(&format!(
        "{}_{}",
        name.replace('.', "_"),
        stdin_label(stdin)
    ));
    let lossy = String::from_utf8_lossy(want).into_owned();
    let (checked, ran) =
        conform(&entry, &dir, "--checked", stdin).expect("the checked lane always runs");
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_str()),
        ("exit(0)", lossy.as_str()),
        "the CHECKED lane on {what} (wolf-lang#405)"
    );
    assert_eq!(
        ran.offset_after, offset_after,
        "the checked lane's offset on {what}"
    );
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        let ran = run_with(Command::new(&exe), &dir, stdin, Stdout::Piped, tier);
        assert_eq!(
            ran.out.status.code(),
            Some(0),
            "the {tier} binary on {what}: {}",
            String::from_utf8_lossy(&ran.out.stderr)
        );
        assert!(
            ran.stdout == want,
            "the {tier} lane on {what} is not byte-identical (wolf-lang#405): {} bytes against {}",
            ran.stdout.len(),
            want.len()
        );
        assert_eq!(
            ran.offset_after, offset_after,
            "the {tier} lane's offset on {what}"
        );
    }
    if let Some((lupin, ran)) = lupin_says(&entry, &dir, stdin) {
        lupin_agrees(&lupin, &lossy, pre, &what);
        if !pre_mirror(&lupin) {
            assert_eq!(ran.offset_after, offset_after, "lupin's offset on {what}");
        }
    }
}

fn stdin_label(s: Stdin<'_>) -> String {
    match s {
        Stdin::File(b) => format!("file{}", b.len()),
        Stdin::FileAt(b, n) => format!("file{}at{n}", b.len()),
        Stdin::Pipe(b) => format!("pipe{}", b.len()),
    }
}

/// A fixture's native and release binaries, run with descriptor 1 as
/// `stdout` says: each tier's name and its [`Ran`].
fn compiled_with_stdout(name: &str, stdin: Stdin<'_>, stdout: Stdout) -> Vec<(String, Ran)> {
    let entry = fixture(name);
    let dir = scratch(&format!(
        "{}_{stdout:?}_{}",
        name.replace('.', "_"),
        stdin_label(stdin)
    ));
    let mut runs = Vec::new();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        runs.push((
            tier.to_string(),
            run_with(Command::new(&exe), &dir, stdin, stdout, tier),
        ));
    }
    runs
}

// ------------------------------------------------ the corpus rows --

/// A byte write to descriptor 1 lands between the `print`s around it,
/// and `fs_close` never closes a standard stream. Red at trunk
/// 294d626d: `fs_write_chunk(1, …)` is `io` on every machine (`one`,
/// then `error: io`, exit 1).
#[test]
fn byte_writes_to_stdout_keep_program_order() {
    every_lane_says("fs/std_write_bytes.lu", ("exit(1)", "one\nerror: io\n"));
}

/// The fused copy: a total, a copy onto descriptor 1 in program order,
/// `max` 0, `eof`, closed and forged handles. Red at trunk: E0301
/// (`fs_copy_chunk` unknown) on every wolfgang lane.
#[test]
fn copy_chunk_moves_every_byte_once() {
    every_lane_says("fs/copy_chunk.lu", ("unsupported", ""));
}

/// The host's number beside the row: 2 for a missing path on every
/// tier-1 host, 0 after a success and after a failure decided before the
/// host. Red at trunk: E0301 (`os_error` unknown).
#[test]
fn os_error_is_the_hosts_number_for_the_last_fs_call() {
    every_lane_says("fs/os_error.lu", ("unsupported", ""));
}

/// The bulk byte scan's edges and a 20000-byte count across the vector
/// loop's block edges. Red at trunk: E0301 (`bytes_find` unknown).
#[test]
fn the_byte_scan_answers_the_scalar_definition() {
    every_lane_says("memory/bytes_scan.lu", ("unsupported", ""));
}

// ---------------------------------------------- the stdin fixtures --

const LUPIN_PRE_CAT: (&str, &str) = ("exit(4)", "");

/// `cat` through descriptors 0 and 1 with standard input a regular
/// file. Red at trunk: the read of 0 is `io` (exit 4, nothing printed).
#[test]
fn cat_bytes_reads_stdin_as_a_file() {
    every_lane_with_stdin(
        "cat_bytes.lu",
        Stdin::File(INPUT),
        INPUT,
        None,
        LUPIN_PRE_CAT,
    );
}

/// … and with standard input a pipe: no offset, no reopen, the bytes.
#[test]
fn cat_bytes_reads_stdin_as_a_pipe() {
    every_lane_with_stdin(
        "cat_bytes.lu",
        Stdin::Pipe(INPUT),
        INPUT,
        None,
        LUPIN_PRE_CAT,
    );
}

/// The parent has read 2 bytes of the file it hands over: the program
/// starts at byte 2, and the parent's offset afterwards is the end —
/// one description, shared, as GNU `cat` shares it. (The `/dev/stdin`
/// reopen boreutils used read from byte 0 on linux, #405's first row.)
#[test]
fn cat_bytes_shares_the_offset_it_was_handed() {
    every_lane_with_stdin(
        "cat_bytes.lu",
        Stdin::FileAt(INPUT, 2),
        &INPUT[2..],
        Some(INPUT.len() as u64),
        LUPIN_PRE_CAT,
    );
}

/// Every octet survives the round trip, byte for byte, on the native
/// and release tiers (the checked machine's and lupin's records decode
/// lossily, as the driver decodes a native child's, and agree in that
/// decoding).
#[test]
fn cat_bytes_is_byte_exact() {
    let bin = binary_input();
    every_lane_with_stdin("cat_bytes.lu", Stdin::File(&bin), &bin, None, LUPIN_PRE_CAT);
}

/// `cat` as `fs_copy_chunk(0, 1, …)`: a file, a pipe, a shared offset,
/// every octet. Red at trunk: E0301 (`fs_copy_chunk` unknown).
#[test]
fn cat_copy_moves_stdin_to_stdout() {
    let pre = ("unsupported", "");
    let bin = binary_input();
    every_lane_with_stdin("cat_copy.lu", Stdin::File(INPUT), INPUT, None, pre);
    every_lane_with_stdin("cat_copy.lu", Stdin::Pipe(INPUT), INPUT, None, pre);
    every_lane_with_stdin(
        "cat_copy.lu",
        Stdin::FileAt(INPUT, 2),
        &INPUT[2..],
        Some(INPUT.len() as u64),
        pre,
    );
    every_lane_with_stdin("cat_copy.lu", Stdin::File(&bin), &bin, None, pre);
}

/// The copy onto a regular file (on linux the `copy_file_range` rung)
/// and onto `/dev/null`: the bytes arrive, exactly, and nothing fails.
#[test]
fn cat_copy_onto_a_file_and_the_null_device() {
    let bin = binary_input();
    for (tier, ran) in compiled_with_stdout("cat_copy.lu", Stdin::File(&bin), Stdout::File) {
        assert_eq!(
            ran.out.status.code(),
            Some(0),
            "{tier}: {}",
            String::from_utf8_lossy(&ran.out.stderr)
        );
        assert!(
            ran.stdout == bin,
            "{tier}: the copy onto a file is not byte-identical"
        );
    }
    #[cfg(unix)]
    for (tier, ran) in compiled_with_stdout("cat_copy.lu", Stdin::File(&bin), Stdout::Null) {
        assert_eq!(
            ran.out.status.code(),
            Some(0),
            "{tier}: {}",
            String::from_utf8_lossy(&ran.out.stderr)
        );
    }
}

/// `read_line`'s read-ahead comes first: the first line through
/// `read_line`, the rest through `fs_read_chunk(0, 3)`, nothing lost or
/// doubled. Native and release only: the checked machine's `read_line`
/// reads the input buffer a test hands it, never the process's stdin.
#[test]
fn bytes_after_read_line_continue_where_it_stopped() {
    let input = b"first\nsecond\nthird\n";
    for stdin in [Stdin::File(input), Stdin::Pipe(input)] {
        for (tier, ran) in compiled_with_stdout("line_then_bytes.lu", stdin, Stdout::Piped) {
            assert_eq!(ran.out.status.code(), Some(0), "{tier} {stdin:?}");
            assert_eq!(
                String::from_utf8_lossy(&ran.stdout),
                "line=first\nsecond\nthird\n",
                "{tier} with stdin {stdin:?}"
            );
        }
    }
}

/// The host's number is the TASK's (`[os.fs.error]`): eight tasks fail fs
/// calls side by side, the even ones with the host's `ENOENT` (2), the odd
/// ones before the host (0), and each reads its own number back 300 times.
/// The native and release binaries count 0 sightings of another task's
/// number, also under `taskset -c 0-3`, where the tasks really run at once
/// (strict evidence, wolf-lang#571); lupin, whose machine is one per task,
/// agrees. The checked machine runs the eight tasks one at a time and
/// keeps the word with each task's frames (s226, `[exec.checked.task]`;
/// it declined the program by name while C1 was deferred), so it counts
/// 0 as well. Red at trunk: E0301 (`os_error` unknown); a
/// process-wide word is the defect this catches (seen red locally with the
/// word made a process-wide atomic, the PR's evidence index).
#[test]
fn os_error_is_the_tasks_own() {
    let want: &[u8] = b"tasks=8 calls=2400 saw_another_tasks_number=0\n";
    let entry = fixture("os_error_tasks.lu");
    let dir = scratch("os_error_tasks");
    let (checked, _) =
        conform(&entry, &dir, "--checked", Stdin::File(b"")).expect("the checked lane always runs");
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_bytes()),
        ("exit(0)", want),
        "the checked machine keeps the host's number per task"
    );
    let mut runs: Vec<(&str, Command)> = Vec::new();
    for release in [false, true] {
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        let tier = if release { "release" } else { "native" };
        runs.push((tier, Command::new(&exe)));
        #[cfg(target_os = "linux")]
        {
            let mut pinned = Command::new("taskset");
            pinned.arg("-c").arg("0-3").arg(&exe);
            runs.push((tier, pinned));
        }
    }
    for (tier, cmd) in runs {
        let shown = format!("{cmd:?}");
        let ran = run_with(cmd, &dir, Stdin::File(b""), Stdout::Piped, tier);
        assert_eq!(
            (ran.out.status.code(), String::from_utf8_lossy(&ran.stdout)),
            (Some(0), String::from_utf8_lossy(want)),
            "{tier}: {shown}: {}",
            String::from_utf8_lossy(&ran.out.stderr)
        );
    }
    if let Some((lupin, _)) = lupin_says(&entry, &dir, Stdin::File(b"")) {
        lupin_agrees(
            &lupin,
            &String::from_utf8_lossy(want),
            ("unsupported", ""),
            "os_error_tasks.lu",
        );
    }
}

// ------------------------------------------- the host's refusals --

/// A descriptor 1 that cannot be written is the host's `EBADF` (9 on
/// linux and macOS), reported in the host's words: never a silent
/// success, never a write into whatever the number names (#405's last
/// row). Red at trunk: E0301 (`os_error` unknown); the same program
/// without the reason (`cat_bytes.lu`) exits 4 there, the read of 0
/// being `io`.
#[cfg(unix)]
#[test]
fn a_write_to_an_unwritable_stdout_says_why() {
    for (fixture_name, word) in [("cat_why.lu", "write"), ("cat_copy.lu", "copy")] {
        for (tier, ran) in compiled_with_stdout(fixture_name, Stdin::File(INPUT), Stdout::ReadOnly)
        {
            assert_eq!(ran.out.status.code(), Some(3), "{fixture_name} {tier}");
            assert_eq!(
                String::from_utf8_lossy(&ran.out.stderr),
                format!("{word}: 9 Bad file descriptor\n"),
                "{fixture_name} {tier}: EBADF in the host's words (wolf-lang#407)"
            );
        }
    }
}

/// `/dev/full`: `ENOSPC`, 28, "No space left on device" — GNU's
/// `cat: write error: No space left on device` has the same words.
#[cfg(target_os = "linux")]
#[test]
fn a_write_to_dev_full_says_no_space() {
    for (fixture_name, word) in [("cat_why.lu", "write"), ("cat_copy.lu", "copy")] {
        for (tier, ran) in compiled_with_stdout(fixture_name, Stdin::File(INPUT), Stdout::Full) {
            assert_eq!(ran.out.status.code(), Some(3), "{fixture_name} {tier}");
            assert_eq!(
                String::from_utf8_lossy(&ran.out.stderr),
                format!("{word}: 28 No space left on device\n"),
                "{fixture_name} {tier}: ENOSPC in the host's words (wolf-lang#407)"
            );
        }
    }
}
