//! s199 (wolf-lang#426, #424) — a handle's offset, and the three
//! descriptors a program did not open. `[os.fs.seek]`, `[os.fs.tell]`,
//! `[os.fs.read_at]`, `[os.fs.std]`.
//!
//! Before this lane nothing in wolf named an offset: a program could
//! only read forward from byte zero, so `tail -n 10` of a file read the
//! whole file (boreutils, 111 ms against GNU's 0.2 ms on 256 MiB). And
//! `fs_fstat(0)`, `(1)` and `(2)` answered `io` whatever those
//! descriptors were, because the handle number was an index into the
//! runtime's own table and the first open was handle 0 (lupin's was 1).
//!
//! Two kinds of witness, because the second depends on what the
//! harness wires to descriptor 0:
//!
//! - the corpus rows `fs/seek_tell.lu` and `fs/read_at.lu`, run by
//!   `conform-run` on the checked machine, the native and release tiers
//!   and lupin — the s171 rule: `cargo xtask corpus` runs a row on the
//!   native lane only, and the checked machine and release lowering are
//!   what this gate watches;
//! - the fixtures under `fixtures/fs_std/`, run with standard input a
//!   regular file and then a pipe. `conform-run --native/--release`
//!   hands its child the null device for stdin (`Command::output`), so
//!   those two tiers run the binary `wolf build` made; the checked
//!   machine (`conform-run --checked`, in the `wolf` process) and lupin
//!   (`lupin conform-run`, in its process) see the test's stdin.
//!
//! lupin 0.1.43 predates the mirror (it does not resolve the three new
//! names, and its first handle is 1), so its answers are pinned by
//! version as pre-mirror (s180's design), never widened — with its
//! release commit, so a development build that still calls itself
//! 0.1.43 is held to the ruled answers; 0.1.44 (r26, the pairing since
//! 0.2.21) is pinned beside it with its own release commit: r26 cut it
//! from a wolf-interp tree whose fs tier is byte-identical to 0.1.43's
//! (`src/eval/fs.rs` and `src/eval/builtin.rs`, no diff between
//! `v0.1.43` and `6ce7bc8`), and its answers here were measured equal.
//! The mirror is wolf-interp's s199 PR (wolf-interp#171).

mod lane_exit;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin releases that predate the mirror, each with the commit its
/// release archive reports. A development build of wolf-interp still
/// says the last released version (the mirror's own branch reports
/// `0.1.44` at its own commit before a release), so the commit is what
/// holds that build to the ruled answers. 0.1.44's archive (r26,
/// release sha256 e44aae06…) reports `ba47627`, measured.
const PRE_MIRROR_LUPIN: &[(&str, Option<&str>)] =
    &[("0.1.43", Some("6d6cde5")), ("0.1.44", Some("ba47627"))];

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

/// What a test hands the program on descriptor 0.
#[derive(Clone, Copy, Debug)]
enum Stdin {
    /// The null device (`conform-run`'s own choice for a native child).
    Null,
    /// A regular file holding [`INPUT`].
    File,
    /// A pipe the test writes [`INPUT`] into and then closes.
    Pipe,
}

/// The bytes the stdin witnesses read: 19 of them.
const INPUT: &[u8] = b"wolves at the door\n";

/// A working directory of the test's own, holding the `target/` every
/// witness writes under (`[os.fs.path]`: relative paths resolve against
/// the process's cwd), so tests running in parallel never share a file.
fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("fs_std_lanes")
        .join(test);
    std::fs::create_dir_all(d.join("target")).expect("scratch dir");
    d
}

/// Run `cmd` in `dir` with descriptor 0 wired as `stdin` says, stdout
/// and stderr piped (so descriptor 2 is a pipe on every machine).
fn run_with(mut cmd: Command, dir: &Path, stdin: Stdin, tag: &str) -> Output {
    cmd.current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match stdin {
        Stdin::Null => {
            cmd.stdin(Stdio::null());
        }
        Stdin::File => {
            let p = dir.join(format!("{tag}.input"));
            std::fs::write(&p, INPUT).expect("write the input file");
            cmd.stdin(std::fs::File::open(&p).expect("open the input file"));
        }
        Stdin::Pipe => {
            cmd.stdin(Stdio::piped());
        }
    }
    let mut child = cmd.spawn().expect("spawn");
    if let Some(mut w) = child.stdin.take() {
        // The program never reads descriptor 0 — it asks what it IS —
        // so it may exit before the write lands; EPIPE then is the
        // race, not a failure (CI run 37062798818 met it on linux).
        if let Err(e) = w.write_all(INPUT) {
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::BrokenPipe,
                "write the pipe: {e}"
            );
        }
        // Dropped here: the program sees the pipe's end after INPUT.
    }
    child.wait_with_output().expect("wait")
}

/// One wolfgang lane under `conform-run`. `None` means the host cannot
/// run that lane (the s59 skip pattern), never a silent pass.
fn conform(entry: &Path, dir: &Path, flag: &str, stdin: Stdin) -> Option<Obs> {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run").arg(entry).arg(flag).arg("--json");
    let out = run_with(cmd, dir, stdin, &format!("conform{flag}"));
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

/// The native or release binary of `entry`, built once per test. A
/// compile error FAILS the gate (it is the defect at a pre-fix head,
/// never an environment); `None` is a host that cannot build the tier.
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

/// The sibling lupin, found exactly as `pairing.rs` finds it: `LUPIN`
/// first, then a `wolf-interp` checkout beside an ancestor.
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
/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying one was
/// arranged here (r10/#253): an absent sibling is then a failure.
fn lupin_says(entry: &Path, dir: &Path, stdin: Stdin) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #426/#424's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
        return None;
    };
    let mut cmd = Command::new(&lupin);
    cmd.arg("conform-run").arg(entry).arg("--json");
    let out = run_with(cmd, dir, stdin, "lupin");
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
        .join("tests/fixtures/fs_std")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// lupin's answer: `want` from a mirrored lupin, `pre` (verdict,
/// stdout) from a version named in [`PRE_MIRROR_LUPIN`].
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
            "lupin {} at {} on {what} (wolf-lang#426/#424 mirrored)",
            lupin.version,
            lupin.commit
        );
    }
}

/// A corpus row: every wolfgang lane under `conform-run` prints `want`,
/// and lupin agrees or answers its pinned pre-mirror verdict.
fn every_lane_says(rel: &str, want: &str, pre: (&str, &str)) {
    let entry = corpus(rel);
    let dir = scratch(&rel.replace(['/', '.'], "_"));
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = conform(&entry, &dir, flag, Stdin::Null) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "the {flag} lane on {rel} (wolf-lang#426)"
        );
    }
    if let Some(lupin) = lupin_says(&entry, &dir, Stdin::Null) {
        lupin_agrees(&lupin, want, pre, rel);
    }
}

/// A stdin fixture: run with descriptor 0 as `stdin` says on the
/// checked machine, the native and release binaries, and lupin; every
/// wolfgang lane prints `want`.
fn every_lane_with_stdin(name: &str, stdin: Stdin, want: &str, pre: (&str, &str)) {
    let entry = fixture(name);
    let what = format!("{name} with stdin {stdin:?}");
    let dir = scratch(&format!("{}_{stdin:?}", name.replace('.', "_")));
    let checked = conform(&entry, &dir, "--checked", stdin).expect("the checked lane always runs");
    assert_eq!(
        (checked.verdict.as_str(), checked.stdout.as_str()),
        ("exit(0)", want),
        "the CHECKED lane on {what} (wolf-lang#424)"
    );
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let Some(exe) = build(&entry, &dir, release) else {
            continue;
        };
        let out = run_with(Command::new(&exe), &dir, stdin, tier);
        assert_eq!(
            out.status.code(),
            Some(0),
            "the {tier} binary on {what}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            want,
            "the {tier} lane on {what} (wolf-lang#424)"
        );
    }
    if let Some(lupin) = lupin_says(&entry, &dir, stdin) {
        lupin_agrees(&lupin, want, pre, &what);
    }
}

/// seek to the end, tell after reads, whence 0/1/2, past the end,
/// `invalid` for a bad whence and a negative result, `io` for a closed
/// or forged handle, and the first handle above 2. Red at trunk
/// 12a56b22: E0301 (`fs_seek`, `fs_tell` unknown) on every wolfgang
/// lane, `unsupported` at resolve on lupin 0.1.43.
#[test]
fn seek_and_tell_move_and_read_the_offset() {
    every_lane_says(
        "fs/seek_tell.lu",
        "first_handle_above_2=true\ntell_after_reads=7 read=0123456\n\
end=10 size=10 at_end_is_eof=true\nback=7 last=789\nstart=2 cur=5 next=56\n\
past_end=14 past_end_is_eof=true\n\
bad_whence=invalid before_start=invalid closed=io forged=io\ncleaned=true\n",
        ("unsupported", ""),
    );
}

/// A positional read answers the bytes at the offset and leaves the
/// cursor alone; `eof` at the size, the empty list at `max` 0,
/// `invalid` below zero, `io` closed or forged. Red at trunk: E0301
/// (`fs_read_at`, `fs_tell` unknown); lupin 0.1.43 `unsupported`.
#[test]
fn read_at_leaves_the_cursor_alone() {
    every_lane_says(
        "fs/read_at.lu",
        "head=01 at5=567 tell=2 next=23\nat_end=eof empty_len=0 negative=invalid\n\
tell_unmoved=4 closed=io forged=io\ncleaned=true\n",
        ("unsupported", ""),
    );
}

/// `fs_fstat(0)` with stdin a regular file: kind 0 and its size.
/// Red at trunk on all four machines: `io`, and the first handle 0
/// (lupin 1).
#[test]
fn fstat_sees_stdin_as_a_file() {
    every_lane_with_stdin(
        "std_fstat.lu",
        Stdin::File,
        "fstat0 kind=0 size=19\nfstat2 kind=2\nfirst_handle_above_2=true\n",
        (
            "exit(0)",
            "fstat0 io\nfstat2 io\nfirst_handle_above_2=false\n",
        ),
    );
}

/// `fs_fstat(0)` with stdin a pipe: kind 2. Red at trunk: `io`.
#[test]
fn fstat_sees_stdin_as_a_pipe() {
    every_lane_with_stdin(
        "std_fstat.lu",
        Stdin::Pipe,
        "fstat0 kind=2\nfstat2 kind=2\nfirst_handle_above_2=true\n",
        (
            "exit(0)",
            "fstat0 io\nfstat2 io\nfirst_handle_above_2=false\n",
        ),
    );
}

/// stdin a regular file: seek to the end answers the size and back
/// to 0, a positional read answers the first four bytes, the cursor
/// stays at 0. Red at trunk: E0301.
#[test]
fn seek_tell_and_read_at_serve_stdin_as_a_file() {
    every_lane_with_stdin(
        "std_descriptors.lu",
        Stdin::File,
        "fstat0 kind=0 size=19\nseek0 end=19 back=0\nread_at0 wolv\ntell0 0\nfstat2 kind=2\n",
        ("unsupported", ""),
    );
}

/// stdin a pipe: every offset call is `unseekable`, by name. Red at
/// trunk: E0301.
#[test]
fn seek_on_a_pipe_is_unseekable() {
    every_lane_with_stdin(
        "std_descriptors.lu",
        Stdin::Pipe,
        "fstat0 kind=2\nseek0 unseekable\nread_at0 unseekable\ntell0 unseekable\nfstat2 kind=2\n",
        ("unsupported", ""),
    );
}
