//! kw11 (K5 = A, STATUS #31): the concurrency witness for
//! `[conc.mm.atomic.raw]`. `corpus/conc/atomic_counter.lu` runs four
//! scope tasks adding 1 to one shared word 100000 times each with
//! `atomic_add`, then four more adding 1 25000 times each through a
//! compare-and-swap loop; it must print exactly `400000 100000` on the
//! native and release tiers on EVERY cpu set: all cores, four and one of
//! the allocation under `taskset` (`test_cpus`, wolf-lang#925: the first
//! four and the first one of `WOLF_TEST_CPUS` or this process's own; the
//! runtime's pool on few cpus hid #570 from a 16-cpu host). Each set runs
//! three times.
//!
//! The plain-increment twin, `atomic_race_plain.lu`, is a data race
//! (`[conc.mm.race.1]`): it is built and run under the same sets and its
//! count is REPORTED, never asserted — any count is a permitted outcome
//! of undefined behaviour, and a lucky exact count proves nothing.
//!
//! On linux `taskset` is part of this gate's host (no skip); on macOS
//! there is no cpu-affinity tool, so only the all-cores set runs there
//! and the gate says so. The checked machine runs its tasks one at a
//! time whatever the cpu set (s226) and is `atomic_lanes.rs`'s.
//!
//! At trunk a3465f87 the counter did not build (`unsupported` — "this
//! raw-pointer operation"); the plain twin built and ran (105556 natively,
//! 400000 on release, `rows-trunk-a3465f87.log`).

mod lane_exit;
mod test_cpus;

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

/// The cpu sets this host can pin to: `None` is all cores. On linux the
/// witness runs on four and one of the allocation as well, never skipping
/// them.
fn cpu_sets(test: &str) -> Vec<Option<test_cpus::Pin>> {
    if cfg!(target_os = "linux") {
        vec![
            None,
            Some(test_cpus::pin_required(4, test)),
            Some(test_cpus::pin_required(1, test)),
        ]
    } else {
        eprintln!(
            "NOTE: no cpu-affinity tool on this host; the counter runs on all cores only \
             (the 1- and 4-cpu sets are the linux host's)"
        );
        vec![None]
    }
}

fn run(exe: &Path, set: Option<&test_cpus::Pin>) -> (String, Option<i32>) {
    let out = match set {
        Some(pin) => pin.command(exe).output(),
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
        for set in cpu_sets("the_atomic_counter_is_exact_on_every_cpu_set") {
            for n in 0..3 {
                let (stdout, code) = run(&exe, set.as_ref());
                let label = set.as_ref().map_or("all cores".to_string(), |p| p.label());
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
        for set in cpu_sets("the_plain_counter_runs_and_its_count_is_reported_only") {
            let (stdout, code) = run(&exe, set.as_ref());
            let label = set.as_ref().map_or("all cores".to_string(), |p| p.label());
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
