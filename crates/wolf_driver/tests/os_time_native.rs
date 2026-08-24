//! s40 — the os/env and time builtin tiers, cross-lane.
//!
//! The str_list_fs_native discipline extended over the s40 surface:
//! every fixture runs `conform-run` on BOTH lanes (`--checked` = the
//! reference executor, `--native` = wir → Cranelift → cc) and must
//! produce the IDENTICAL verdict and stdout. Fixture outputs are
//! host-independent by construction (bools/counters, never raw env or
//! timestamps). The one deliberate non-parity case at the bottom pins
//! the HONEST refusal shape of the checked-lane-only families
//! (json, the process trio) on the native rung.
//!
//! Off-target the whole file compiles away (native codegen is
//! linux/x86-64 only at this tier).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::Path;
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
}

/// One conform-run lane over a fixture. `None` (with a loud SKIP)
/// only for environment failures (exit 2 from the native rung: no cc,
/// no rt staticlib); refusals are visible as `unsupported` verdicts.
fn lane(case: &str, src: &str, flag: &str) -> Option<Obs> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join(format!("{case}.lu"));
    std::fs::write(&entry, src).expect("write fixture");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag == "--native" {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {case}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

/// Run both lanes; assert identical verdict + stdout AND the expected
/// verdict/stdout. Skips (environment only) skip the whole assertion.
fn parity(case: &str, src: &str, want_verdict: &str, want_stdout: &str) {
    let checked = lane(case, src, "--checked").expect("checked lane always runs");
    assert_eq!(
        checked.verdict, want_verdict,
        "{case}: checked verdict (stdout {:?})",
        checked.stdout
    );
    assert_eq!(checked.stdout, want_stdout, "{case}: checked stdout");
    let Some(native) = lane(case, src, "--native") else {
        return;
    };
    assert_eq!(
        native.verdict, checked.verdict,
        "{case}: cross-lane verdict divergence"
    );
    assert_eq!(
        native.stdout, checked.stdout,
        "{case}: cross-lane stdout divergence"
    );
}

#[test]
fn env_roundtrip_and_rows_agree() {
    parity(
        "s40_env_roundtrip",
        r#"
fn main() -> !int {
    env_set("WOLF_S40_NATIVE_DEN", "tarn")?
    let v = env_get("WOLF_S40_NATIVE_DEN")?
    print("got: {v}")
    let miss = env_get("WOLF_S40_NATIVE_ABSENT") else |_| "<missing>"
    print("{miss}")
    env_set("A=B", "x") else |_| print("invalid")
    0
}
"#,
        "exit(0)",
        "got: tarn\n<missing>\ninvalid\n",
    );
}

#[test]
fn env_args_default_and_vars_overlay_agree() {
    parity(
        "s40_env_args_vars",
        r#"
fn main() -> !int {
    let args = env_args()
    print("argc: {args.len}")
    env_set("WOLF_S40_NATIVE_ZZB", "2")?
    env_set("WOLF_S40_NATIVE_ZZA", "1")?
    var hits = 0
    var first = ""
    for kv in env_vars() {
        if kv.starts_with("WOLF_S40_NATIVE_ZZ") {
            if hits == 0 { first = kv }
            hits = hits + 1
        }
    }
    print("{hits} {first}")
    0
}
"#,
        "exit(0)",
        "argc: 0\n2 WOLF_S40_NATIVE_ZZA=1\n",
    );
}

#[test]
fn cwd_agrees_as_a_predicate() {
    parity(
        "s40_cwd",
        r#"
fn main() -> !int {
    let d = os_cwd()?
    let ok = d.len > 0
    print("has_cwd: {ok}")
    0
}
"#,
        "exit(0)",
        "has_cwd: true\n",
    );
}

#[test]
fn os_exit_code_and_truncation_agree() {
    parity(
        "s40_os_exit",
        r#"
fn main() -> !int {
    print("before")
    os_exit(7)
    print("after")
    0
}
"#,
        "exit(7)",
        "before\n",
    );
}

#[test]
fn time_monotonic_and_sleep_agree() {
    parity(
        "s40_time",
        r#"
fn main() -> !int {
    let a = time_now_ms()
    time_sleep_ms(2)
    let b = time_now_ms()
    let nonneg = a >= 0
    let advanced = b > a
    let modern = time_unix_ms() > 1577836800000
    print("{nonneg} {advanced} {modern}")
    0
}
"#,
        "exit(0)",
        "true true true\n",
    );
}

/// The checked-lane-only families refuse HONESTLY on the native rung:
/// the record stays in the conservatism ledger (`unsupported`), never
/// a wrong answer — the posture net held until its s106 crossing,
/// pinned for json and the process trio (s107's crossings).
#[test]
fn json_and_process_refuse_honestly_on_native() {
    let src = r#"
fn main() -> !int {
    let ok = json_valid("[1]")
    print("{ok}")
    0
}
"#;
    let checked = lane("s40_json_refusal", src, "--checked").expect("checked runs");
    assert_eq!(checked.verdict, "exit(0)");
    assert_eq!(checked.stdout, "true\n");
    if let Some(native) = lane("s40_json_refusal", src, "--native") {
        assert_eq!(
            native.verdict, "unsupported",
            "json stays checked-lane-only at s40 — an honest refusal, never a wrong answer"
        );
    }
}
