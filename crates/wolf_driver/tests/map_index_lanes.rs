//! s280 (#1003, #894): runtime indexing, not std.map.get's pairs walk.
//! Linux-only gates: RLIMIT_AS 3 GiB, 240 s per child, output checked on
//! every run. A 2x workload may cost at most 2.5x, by the child's own CPU
//! time and the minimum of seven interleaved pairs, so a busy host cannot
//! turn a linear index red and a quadratic one (4x) stays red. 100k and
//! 236k may cost at most 20x the same-run C bar, by the median of three
//! paired wall samples.
//! Runtime unit tests separately gate equality-comparison counts.
#![cfg(target_os = "linux")]

mod lane_exit;

use std::io::Read;
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

/// One child's cost: wall seconds, and the CPU seconds (user plus system)
/// the kernel charged to it. A child kept off the CPU by a busy host grows
/// in wall time only.
#[derive(Clone, Copy)]
struct Cost {
    wall: f64,
    cpu: f64,
}

fn seconds(t: libc::timeval) -> f64 {
    t.tv_sec as f64 + t.tv_usec as f64 / 1e6
}

fn sample(exe: &Path, n: usize) -> Cost {
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
    let pid = child.id() as libc::pid_t;
    // wait4 reaps the child here, with its rusage, because std's wait does not
    // return one. The timeout branch reaps through std; the normal path hands
    // the Child over once its pipes are read (see below).
    let (status, usage) = loop {
        let mut status = 0;
        // SAFETY: an all-zero rusage is a valid value of that plain C struct.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        // SAFETY: status and usage are live out-parameters, and pid names
        // this function's own child, which nothing else reaps.
        let got = unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut usage) };
        assert!(
            got == 0 || got == pid,
            "wait4: {}",
            std::io::Error::last_os_error()
        );
        if got == pid {
            break (status, usage);
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
    };
    let cost = Cost {
        wall: start.elapsed().as_secs_f64(),
        cpu: seconds(usage.ru_utime) + seconds(usage.ru_stime),
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    child
        .stdout
        .take()
        .expect("piped stdout")
        .read_to_string(&mut stdout)
        .expect("read map witness stdout");
    child
        .stderr
        .take()
        .expect("piped stderr")
        .read_to_string(&mut stderr)
        .expect("read map witness stderr");
    // wait4 already reaped the pid, so std's Child owns nothing live; dropping
    // it here, after the pipes are read, closes the handle on every path.
    drop(child);
    assert!(
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "{} N={n}: wait status {status:#x} {stderr}",
        exe.display(),
    );
    assert_eq!(stdout.trim(), n.to_string(), "all values round-trip");
    cost
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

/// Host noise only ever adds time, so the least of several samples is the
/// best estimate of what the code itself costs.
fn least(samples: &[f64]) -> f64 {
    samples.iter().copied().fold(f64::INFINITY, f64::min)
}

/// The doubling ratio of a take: the large size's cost over the small size's.
fn doubling_ratio(small: &[f64], large: &[f64]) -> f64 {
    least(large) / least(small)
}

/// Interleaved pairs per take: a slow spell of the host lands on both sizes.
const DOUBLING_PAIRS: usize = 7;
/// Below this the 50k point is too close to the CPU clock's grain to divide by.
const DOUBLING_FLOOR_SECONDS: f64 = 0.005;

#[test]
fn native_string_index_doubling_costs_at_most_two_point_five() {
    let dir = scratch("doubling");
    let Some(exe) = build(&dir) else { return };
    let mut small = Vec::with_capacity(DOUBLING_PAIRS);
    let mut large = Vec::with_capacity(DOUBLING_PAIRS);
    let mut walls = Vec::with_capacity(2 * DOUBLING_PAIRS);
    for _ in 0..DOUBLING_PAIRS {
        let s = sample(&exe, 50_000);
        let l = sample(&exe, 100_000);
        small.push(s.cpu);
        large.push(l.cpu);
        walls.push((s.wall, l.wall));
    }
    let ratio = doubling_ratio(&small, &large);
    eprintln!(
        "REPORT map-index doubling: cpu 50k={small:?} 100k={large:?} ratio={ratio:.4} bound=2.5 wall pairs={walls:?}"
    );
    assert!(
        least(&small) >= DOUBLING_FLOOR_SECONDS,
        "50k costs {:.6} s of CPU, under the {DOUBLING_FLOOR_SECONDS} s floor: grow both sizes",
        least(&small)
    );
    assert!(
        ratio <= 2.5,
        "native string index is not linear: ratio={ratio:.4}, bound=2.5"
    );
}

/// A take recorded beside a CPU hog: six 50k samples near 0.0855 s and one
/// fast one at 0.0721 s, with every 100k sample near 0.183 s. Linear code,
/// and one lone fast sample must not turn it red.
#[test]
fn one_fast_outlier_does_not_turn_linear_doubling_red() {
    let small = [
        0.085516, 0.072079, 0.084538, 0.084883, 0.085633, 0.085593, 0.085799,
    ];
    let large = [
        0.183548, 0.184557, 0.183019, 0.183419, 0.180776, 0.184297, 0.181577,
    ];
    let ratio = doubling_ratio(&small, &large);
    assert!(ratio <= 2.5, "linear take read as {ratio:.4}, bound=2.5");
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
            reference[i] = sample(&c, n).wall;
            wolf[i] = sample(&exe, n).wall;
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
