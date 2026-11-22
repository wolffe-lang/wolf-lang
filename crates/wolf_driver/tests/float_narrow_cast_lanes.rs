//! wolf-lang#1160: a cast from a float to a narrow integer (`u8`, `u16`,
//! `i8`, `i16`) on every machine.
//!
//! `[type.numlit.cast.trunc]`: the cast truncates toward zero, and traps
//! `overflow` when the truncated value does not fit or the input is a NaN.
//! At 0.2.40 the x86-64 native compiler panicked in code generation on every
//! such cast, in range or not (cranelift's x64 emitter has no 8- or 16-bit
//! saturating conversion), while macOS arm64, release and the checked
//! machine answered. No corpus row cast a float to a narrow integer.
//!
//! Each row runs on the checked machine, native and release with one
//! verdict and stdout, and on lupin when it is present (`LUPIN`, or a
//! sibling wolf-interp build). The f32 row is the exception the checked
//! machine makes by design: it runs `f64` only and refuses an `f32`
//! binding by name, which this gate pins.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

fn corpus(row: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/typecheck")
        .join(row)
}

/// The record a machine wrote, or why there is none: a compiler that
/// panicked or a run that exited non-zero is an `Err` naming its stderr.
fn record(out: &Output, row: &str, machine: &str) -> Result<serde_json::Value, String> {
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().rev().take(4).collect();
        return Err(format!(
            "{machine} {row}: {} ({})",
            out.status,
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| {
        format!(
            "{machine} {row}: {e}: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// One machine's record: `Ok(None)` when the host cannot run that tier (a
/// loud SKIP, refused under `WOLF_TEST_NATIVE_REQUIRE`).
fn conform(row: &str, machine: &str) -> Result<Option<serde_json::Value>, String> {
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .args(["conform-run", "--json", machine])
        .arg(corpus(row))
        .output()
        .expect("run wolf");
    if machine != "--checked" && lane_exit::environment_refusal(&out, machine) {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required {machine} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!(
            "SKIP: {machine} {row}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return Ok(None);
    }
    record(&out, row, machine).map(Some)
}

fn lupin() -> Option<PathBuf> {
    let found = std::env::var_os("LUPIN").map(PathBuf::from).or_else(|| {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    });
    if found.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "required lupin missing"
        );
    }
    found
}

fn lupin_record(row: &str) -> Result<Option<serde_json::Value>, String> {
    let Some(lupin) = lupin() else {
        eprintln!("SKIP: lupin {row}: set LUPIN to run the fourth machine");
        return Ok(None);
    };
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(corpus(row))
        .output()
        .expect("run lupin");
    record(&out, row, "lupin").map(Some)
}

/// `verdict` and `stdout` (`None`: nothing printed) from every machine in
/// `machines` and from lupin. Every machine runs before the verdict, so a
/// red run reports each one's answer (`REPORT` lines).
fn answers(row: &str, machines: &[&str], verdict: &str, stdout: Option<&str>) {
    let want = stdout.map_or(serde_json::Value::Null, |s| s.into());
    let mut wrong = Vec::new();
    let runs = machines
        .iter()
        .map(|&m| (m, conform(row, m)))
        .chain(std::iter::once(("lupin", lupin_record(row))));
    for (machine, got) in runs {
        match got {
            Err(why) => {
                eprintln!("REPORT {machine} {row}: no record: {why}");
                wrong.push(why);
            }
            Ok(None) => {}
            Ok(Some(rec)) => {
                eprintln!(
                    "REPORT {machine} {row}: {} {}",
                    rec["verdict"], rec["stdout_inline"]
                );
                if rec["verdict"] != verdict || rec["stdout_inline"] != want {
                    wrong.push(format!("{machine} {row}: {rec}"));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

const IN_RANGE: &str = "255 0 65535 -128 32767\n";

#[test]
fn f64_to_narrow_in_range() {
    answers(
        "cast_float_narrow_in_range.lu",
        &MACHINES,
        "exit(0)",
        Some(IN_RANGE),
    );
}

#[test]
fn f32_to_narrow_in_range() {
    answers(
        "cast_f32_narrow_in_range.lu",
        &MACHINES[1..],
        "exit(0)",
        Some(IN_RANGE),
    );
    // The checked machine's own posture: `f32` is refused by name.
    let rec = conform("cast_f32_narrow_in_range.lu", "--checked")
        .expect("checked writes a record")
        .expect("checked runs");
    assert_eq!(rec["verdict"], "unsupported", "{rec}");
    let construct = rec["x-unsupported-construct"].as_str().unwrap_or("");
    assert!(construct.contains("f32"), "{rec}");
}

#[test]
fn f64_to_u8_over_traps() {
    answers("cast_float_u8_over.lu", &MACHINES, "trap(overflow)", None);
}

#[test]
fn negative_f64_to_u16_traps() {
    answers(
        "cast_float_u16_negative.lu",
        &MACHINES,
        "trap(overflow)",
        None,
    );
}

#[test]
fn f64_to_i8_under_traps() {
    answers("cast_float_i8_under.lu", &MACHINES, "trap(overflow)", None);
}

#[test]
fn nan_to_i16_traps() {
    answers("cast_float_i16_nan.lu", &MACHINES, "trap(overflow)", None);
}
