//! #238 — a diagnostic on a long line must not cost the record.
//!
//! `conform-run` exists to emit one JSON line per program. It renders
//! the human report to stderr FIRST and writes that record afterwards,
//! so a panic in the renderer is not an ugly message: it is silence.
//! `wolf_diag::render::render_line` had one — a group pointing twice at
//! one long line put the second underline's start past the window's cut
//! while its end was clamped to the window, and `hi - lo` underflowed a
//! `usize`. `str::repeat` then asked for near-`usize::MAX` bytes and
//! aborted the process at exit 101 with an empty stdout. Downstream that
//! reads as a clean file: wolf-std's sc35 lost a refusing row past two
//! full-tree scans that way, both grepping for `^error[`, neither
//! getting one.
//!
//! The witness lives here rather than in `corpus/` on purpose. The
//! shape needs a source line wider than the renderer's 100-column
//! window, and `cargo xtask fmt-lu` holds every corpus file canonical
//! under `wolf fmt`, which reflows every over-width line it can break —
//! an `if`, a `match`, a call, an operator chain, a type. What survives
//! formatting is the unbreakable single token (the long f-string the
//! ledger witnesses print), and that carries exactly ONE annotation. So
//! the corpus cannot hold this geometry, which is also the answer to
//! why 473 corpus entries never found the bug. A test that writes its
//! own fixture can.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// One `let` whose annotation and whose failing arm sit far apart on a
/// single line: `bool` at column 15, and the `else` arm past column 250.
///
/// The renderer windows the line around the FIRST underline it owes —
/// the annotation, leftmost — so the arm's underline starts hundreds of
/// columns right of anything shown. That is the whole ingredient; the
/// same file with a short arithmetic chain renders normally, which is
/// why #238 reads "a long line" but is really "two spans, far apart".
fn long_line_two_spans() -> Vec<u8> {
    let chain = std::iter::repeat_n("1", 60).collect::<Vec<_>>().join(" + ");
    let src = format!(
        "fn main() -> !int {{\n    let ready: bool = if 1 == 1 {{ {chain} }} else {{ 0 }}\n    \
         print(\"{{ready}}\")\n    0\n}}\n"
    );
    let longest = src.lines().map(str::len).max().unwrap_or(0);
    assert!(
        longest > 250,
        "the fixture's point is a line past the 100-column window, got {longest}"
    );
    src.into_bytes()
}

fn fixture(case: &str, src: &[u8]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir fixture");
    std::fs::write(dir.join("main.lu"), src).expect("write main");
    dir
}

/// (exit code, stdout, stderr) from `conform-run` in one lane.
fn conform_run(dir: &Path, lane: &str) -> (i32, String, String) {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(dir.join("main.lu"))
        .arg(lane)
        .current_dir(dir)
        .output()
        .expect("run wolf conform-run");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The letter of #238, in both run-reaching lanes: the process lives,
/// the record lands, and the report says `error[E0401]` out loud.
#[test]
fn a_long_line_group_still_emits_its_record() {
    let dir = fixture("i238_long_line", &long_line_two_spans());
    for lane in ["--native", "--checked"] {
        let (code, stdout, stderr) = conform_run(&dir, lane);
        assert_eq!(
            code, 0,
            "{lane}: conform-run must not die rendering a diagnostic\n{stderr}"
        );
        assert!(
            !stderr.contains("panicked"),
            "{lane}: the renderer panicked\n{stderr}"
        );
        let record: serde_json::Value = {
            let line = stdout
                .lines()
                .find(|l| l.starts_with('{'))
                .unwrap_or_else(|| {
                    panic!("{lane}: no record on stdout — #238's silence\n{stderr}")
                });
            serde_json::from_str(line).expect("the record is JSON")
        };
        assert_eq!(
            record["verdict"], "fail(E0401)",
            "{lane}: the refusal is the record's verdict"
        );
        assert!(
            record["diagnostics"]
                .as_array()
                .is_some_and(|d| !d.is_empty()),
            "{lane}: the record carries its diagnostics ([proto.record.diag])"
        );
        // The other half: a record is not a substitute for the report.
        assert!(
            stderr.contains("error[E0401]"),
            "{lane}: the human report renders too\n{stderr}"
        );
    }
}

/// The underflow itself, at the layout: no underline may claim more
/// columns than the window shows. Before the fix the second row's run
/// was `hi - lo` with `hi` LEFT of `lo`; this is the invariant that
/// makes that unrepresentable rather than merely unobserved.
#[test]
fn no_underline_is_wider_than_the_window() {
    let dir = fixture("i238_underline_width", &long_line_two_spans());
    let (_, _, stderr) = conform_run(&dir, "--native");
    for line in stderr.lines() {
        let marks = line.chars().filter(|c| *c == '^' || *c == '-').count();
        assert!(
            marks <= 100,
            "an underline of {marks} columns against a 100-column window:\n{line}"
        );
    }
}
