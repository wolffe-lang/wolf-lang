//! s127 — the struct-layout witness, release tier vs the checked lane.
//!
//! The datalayout string is the classic silent-miscompile site: a
//! wrong one changes field offsets and padding UNDER the emitted IR
//! without a single diagnostic. `tests/target_header.rs` (codegen_llvm)
//! pins the string to clang's own emission; this test witnesses the
//! CONSEQUENCE end-to-end on aggregate-heavy shapes — nested structs,
//! mixed int/float fields, a > 16-byte composite (memory class), and
//! an HFA shape — by holding the release tier's observed behavior to
//! the CHECKED lane's (the semantics reference), not just to hello.
//!
//! Skips follow the s59 posture: environment exit-2 and the tier's
//! named host refusal are loud skips, never verdicts.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// Nested aggregates with mixed field classes around every layout
/// seam the plans distinguish: `Inner` (i32 + i64: interior padding),
/// `Big` (28 bytes of fields, > 16-byte memory class, float amid
/// ints), `Hfa` (3 × f64 — the FP-register shape). Field reads cross
/// every offset; the printed values are the layout's fingerprint.
const SRC: &str = "//! check: run(exit=0)\n\
//! phase: run\n\
struct Inner {\n    a: i32,\n    b: i64,\n}\n\
struct Big {\n    x: i64,\n    inner: Inner,\n    y: f64,\n    z: i32,\n}\n\
struct Hfa {\n    p: f64,\n    q: f64,\n    r: f64,\n}\n\
fn sum_big(v: Big) -> i64 {\n    v.x + v.inner.a as i64 + v.inner.b + v.z as i64\n}\n\
fn mass(h: Hfa) -> f64 {\n    h.p + h.q * 2.0 + h.r * 4.0\n}\n\
fn make(seed: i64) -> Big {\n    Big {\n        x: seed,\n        inner: Inner { a: 11, b: seed * 3 },\n        y: 2.5,\n        z: 7,\n    }\n}\n\
fn main() -> !int {\n    let b = make(5)\n    let h = Hfa { p: 0.5, q: 1.25, r: 3.0 }\n    print(\"{sum_big(b)} {b.y} {mass(h)}\")\n    let c = make(-9)\n    print(\"{sum_big(c)} {c.inner.b}\")\n    0\n}\n";

/// What the field arithmetic prints when every offset is right.
const EXPECTED: &str = "38 2.5 15\n-18 -27\n";

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("release_struct_layout");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("prog.lu"), SRC).expect("write witness");
    dir
}

fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

/// One conform-run lane: (verdict, stdout sha), `None` on a loud skip
/// (environment exit-2 or the release tier's named host refusal).
fn lane(src: &Path, flag: &str) -> Option<(String, String)> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(src)
        .args([flag, "--json"])
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) {
        eprintln!(
            "SKIP: environment cannot run {flag}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record parses");
    if rec["verdict"] == "unsupported"
        && String::from_utf8_lossy(&out.stderr).contains("release tier targets")
    {
        eprintln!("SKIP: the release tier refuses this host");
        return None;
    }
    Some((
        rec["verdict"].as_str().unwrap_or("").to_string(),
        rec["stdout_sha256"].as_str().unwrap_or("").to_string(),
    ))
}

/// All three lanes observe one behavior, and the release BINARY's
/// bytes-on-stdout are the checked lane's bytes.
#[test]
fn release_layout_matches_the_checked_lane() {
    ensure_rt_staticlib();
    let dir = fixture();
    let src = dir.join("prog.lu");

    let Some(checked) = lane(&src, "--checked") else {
        return;
    };
    assert_eq!(checked.0, "exit(0)", "the checked lane runs the witness");
    assert_eq!(
        checked.1,
        wolf_wir::sha256_hex(EXPECTED.as_bytes()),
        "the checked lane computes the expected field arithmetic"
    );
    for flag in ["--native", "--release"] {
        let Some(got) = lane(&src, flag) else {
            return;
        };
        assert_eq!(
            got, checked,
            "{flag} diverges from the checked lane on the layout witness"
        );
    }

    // And the real `wolf build --release` binary, byte-for-byte.
    let exe = dir.join("prog");
    let out = Command::new(wolf())
        .arg("build")
        .arg(&src)
        .args(["--release", "-o"])
        .arg(&exe)
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) {
        eprintln!(
            "SKIP: environment cannot build the release tier: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return;
    }
    assert!(
        out.status.success(),
        "wolf build --release failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("witness runs");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        EXPECTED,
        "the release binary's stdout is the checked lane's, byte for byte"
    );
}
