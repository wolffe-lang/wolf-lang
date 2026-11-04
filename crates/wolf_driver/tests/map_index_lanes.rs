//! s280 (#1003, #894): runtime indexing, not std.map.get's pairs walk.
//! Linux-only wall gates: median of three paired samples, RLIMIT_AS 3 GiB,
//! 240 s per child, output checked on every run. A 2x workload may cost
//! at most 2.5x; 100k and 236k may cost at most 20x the same-run C bar.
//! Runtime unit tests separately gate equality-comparison counts.
#![cfg(target_os = "linux")]

mod lane_exit;

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn scratch(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("map_index_lanes")
        .join(name);
    if p.exists() {
        std::fs::remove_dir_all(&p).expect("remove old scratch");
    }
    std::fs::create_dir_all(&p).expect("create scratch");
    p
}

fn build(dir: &Path) -> Option<PathBuf> {
    let exe = dir.join("map-index");
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map_index/main.lu"))
        .arg("--no-cache")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build map-index");
    if lane_exit::environment_refusal(&out, "map-index native build") {
        eprintln!(
            "SKIP map-index: native environment refused: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    assert!(
        out.status.success(),
        "map-index build: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(exe)
}

fn sample(exe: &Path, n: usize) -> f64 {
    let mut cmd = Command::new(exe);
    cmd.arg(n.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: this pre-exec closure calls only setrlimit and constructs an
    // OS error on failure; it neither allocates nor locks before exec.
    unsafe {
        cmd.pre_exec(|| {
            let limit = libc::rlimit {
                rlim_cur: 3 * 1024 * 1024 * 1024,
                rlim_max: 3 * 1024 * 1024 * 1024,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let start = Instant::now();
    let mut child = cmd.spawn().expect("spawn capped map witness");
    loop {
        if child.try_wait().expect("poll map witness").is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(240) {
            child.kill().expect("kill only the timed-out witness pid");
            let out = child.wait_with_output().expect("reap timed-out witness");
            panic!(
                "map witness {} N={n} exceeded 240 s: {}",
                exe.display(),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let seconds = start.elapsed().as_secs_f64();
    let out = child.wait_with_output().expect("reap map witness");
    assert!(
        out.status.success(),
        "{} N={n}: {:?} {}",
        exe.display(),
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        n.to_string(),
        "all values round-trip"
    );
    seconds
}

#[test]
fn indexed_map_observations_agree_with_checked_and_lupin() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map_index_semantics/main.lu");
    let want = "0 128 299 -1 300 298 true 46322 901 -1 1\n";
    let mut commands = Vec::new();
    for tier in ["--checked", "--native", "--release"] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_wolf"));
        cmd.args(["conform-run", "--json", tier]).arg(&fixture);
        commands.push((tier, cmd));
    }
    if let Some(lupin) = std::env::var_os("LUPIN") {
        let mut cmd = Command::new(lupin);
        cmd.args(["conform-run", "--json"]).arg(&fixture);
        commands.push(("lupin", cmd));
    } else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "LUPIN is required"
        );
        eprintln!("SKIP map-index lupin: set LUPIN to run the independent interpreter");
    }
    for (tier, mut cmd) in commands {
        let out = cmd.output().expect("run map semantics");
        if tier != "--checked" && tier != "lupin" && lane_exit::environment_refusal(&out, tier) {
            eprintln!(
                "SKIP map-index semantics {tier}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            continue;
        }
        assert!(
            out.status.success(),
            "{tier}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let obs: serde_json::Value = serde_json::from_slice(&out.stdout).expect("one observation");
        eprintln!("REPORT map-index semantics {tier}: {obs}");
        assert_eq!(obs["verdict"], "exit(0)", "{tier}: {obs}");
        assert_eq!(
            obs["stdout_inline"], want,
            "{tier}: insertion order, snapshot and independence"
        );
    }
}

fn median(mut samples: [f64; 3]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[1]
}

#[test]
fn native_string_index_doubling_costs_at_most_two_point_five() {
    let dir = scratch("doubling");
    let Some(exe) = build(&dir) else { return };
    let mut small = [0.0; 3];
    let mut large = [0.0; 3];
    for i in 0..3 {
        small[i] = sample(&exe, 50_000);
        large[i] = sample(&exe, 100_000);
    }
    let ratio = median(large) / median(small);
    eprintln!("REPORT map-index doubling: 50k={small:?} 100k={large:?} ratio={ratio:.4} bound=2.5");
    assert!(
        ratio <= 2.5,
        "native string index is not linear: ratio={ratio:.4}, bound=2.5"
    );
}

#[test]
fn native_string_index_stays_within_twenty_times_same_run_c() {
    let dir = scratch("c-relative");
    let Some(exe) = build(&dir) else { return };
    let c = dir.join("c972");
    let out = Command::new("cc")
        .args(["-O2", "-o"])
        .arg(&c)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../notes/s273-evidence/witness/c972.c"))
        .output()
        .expect("cc is required for the same-run C bar");
    assert!(
        out.status.success(),
        "C bar build: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for n in [100_000, 236_000] {
        let mut wolf = [0.0; 3];
        let mut reference = [0.0; 3];
        for i in 0..3 {
            reference[i] = sample(&c, n);
            wolf[i] = sample(&exe, n);
        }
        let ratio = median(wolf) / median(reference);
        eprintln!(
            "REPORT map-index C-relative N={n}: wolf={wolf:?} C={reference:?} ratio={ratio:.4} bound=20"
        );
        assert!(
            ratio <= 20.0,
            "native string index N={n}: {ratio:.4}x C, bound=20"
        );
    }
}
