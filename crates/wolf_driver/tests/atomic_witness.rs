//! kw11 (K5 = A, STATUS #31): the concurrency witness for
//! `[conc.mm.atomic.raw]`. `corpus/conc/atomic_counter.lu` runs four
//! scope tasks adding 1 to one shared word 100000 times each with
//! `atomic_add`, then four more adding 1 25000 times each through a
//! compare-and-swap loop; it must print exactly `400000 100000` on the
//! native and release tiers on EVERY cpu set: all cores, `taskset -c
//! 0-3` and `taskset -c 0` (the runtime's pool on few cpus hid #570 from
//! a 16-cpu host). Each set runs three times.
//!
//! The plain-increment twin, `atomic_race_plain.lu`, is a data race
//! (`[conc.mm.race.1]`): it is built and run under the same sets and its
//! count is REPORTED, never asserted — any count is a permitted outcome
//! of undefined behaviour, and a lucky exact count proves nothing.
//!
//! On linux `taskset` is part of this gate's host (no skip); on macOS
//! there is no cpu-affinity tool, so only the all-cores set runs there
//! and the gate says so. The checked machine runs no task (C1) and is
//! `atomic_lanes.rs`'s.
//!
//! At trunk a3465f87 neither program built (fail(E0301): `Order` and
//! `fence`); the plain twin's shape (k4_scope_fn.lu, four tasks of 1000)
//! gave 3733 natively and 4000 on release.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
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

/// Build `corpus/conc/<row>` on `tier` into its own scratch directory.
/// `None` only on windows, whose release tier is dark by design (the
/// s59 environment refusal, named); everywhere else a failed build
/// fails the gate.
fn build(row: &str, tier: &str) -> Option<PathBuf> {
    ensure_rt_staticlib();
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/conc")
        .join(row);
    assert!(src.is_file(), "corpus row missing: {}", src.display());
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("atomic_witness")
        .join(format!("{}_{tier}", row.trim_end_matches(".lu")));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::copy(&src, dir.join(row)).expect("copy row");
    let exe = dir.join(if cfg!(windows) { "w.exe" } else { "w" });
    let mut cmd = Command::new(wolf());
    cmd.current_dir(&dir)
        .arg("build")
        .arg(row)
        .arg("-o")
        .arg(&exe);
    if tier == "release" {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf runs");
    if cfg!(windows) && lane_exit::environment_refusal(&out, &format!("wolf build ({tier})")) {
        eprintln!(
            "SKIP: this host cannot build the {tier} tier: {}",
            text(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success() && exe.is_file(),
        "wolf build {row} ({tier}) must build (exit {:?}) — a refusal is the gate failing:\n{}",
        out.status.code(),
        text(&out.stderr)
    );
    Some(exe)
}

/// The cpu sets this host can pin to: `None` is all cores.
fn cpu_sets() -> Vec<Option<&'static str>> {
    if cfg!(target_os = "linux") {
        let ok = Command::new("taskset")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(
            ok,
            "`taskset` (util-linux) is part of this gate's linux host: the witness must run \
             on 1 and 4 cpus as well as all cores, never skip them"
        );
        vec![None, Some("0-3"), Some("0")]
    } else {
        eprintln!(
            "NOTE: no cpu-affinity tool on this host; the counter runs on all cores only \
             (the 1- and 4-cpu sets are the linux host's)"
        );
        vec![None]
    }
}

fn run(exe: &Path, set: Option<&str>) -> (String, Option<i32>) {
    let out = match set {
        Some(cpus) => Command::new("taskset").args(["-c", cpus]).arg(exe).output(),
        None => Command::new(exe).output(),
    }
    .expect("the witness runs");
    (text(&out.stdout), out.status.code())
}

/// `[conc.mm.atomic.raw]`: exact counts on every tier, every cpu set,
/// three runs each.
#[test]
fn the_atomic_counter_is_exact_on_every_cpu_set() {
    for tier in ["native", "release"] {
        let Some(exe) = build("atomic_counter.lu", tier) else {
            continue;
        };
        for set in cpu_sets() {
            for n in 0..3 {
                let (stdout, code) = run(&exe, set);
                let label = set.map_or("all cores".to_string(), |c| format!("taskset -c {c}"));
                eprintln!(
                    "REPORT counter {tier} {label} run {n}: {:?} exit {code:?}",
                    stdout.trim()
                );
                assert_eq!(
                    (stdout.as_str(), code),
                    ("400000 100000\n", Some(0)),
                    "atomic_counter.lu on {tier} under {label}, run {n}"
                );
            }
        }
    }
}

/// `[conc.mm.race.1]`: the plain twin compiles and runs; its count is
/// reported, not asserted.
#[test]
fn the_plain_counter_runs_and_its_count_is_reported_only() {
    for tier in ["native", "release"] {
        let Some(exe) = build("atomic_race_plain.lu", tier) else {
            continue;
        };
        for set in cpu_sets() {
            let (stdout, code) = run(&exe, set);
            let label = set.map_or("all cores".to_string(), |c| format!("taskset -c {c}"));
            eprintln!(
                "REPORT (not asserted) racy counter {tier} {label}: {:?} exit {code:?}",
                stdout.trim()
            );
            assert_eq!(
                code,
                Some(0),
                "the racy counter exits 0 on {tier} under {label}"
            );
        }
    }
}
