//! s43 acceptance at the driver: the release tier compiles the module
//! graph as ONE program, in clusters, with a content-keyed object
//! cache — and D7's rebuild semantics survive it.
//!
//! Over the same 3-module fixture the s31 incremental suite uses
//! (root → alpha → beta):
//! 1. **Granularity, both tiers**: the debug tier still splits per
//!    SOURCE MODULE (`root`/`alpha`/`beta` — D7 untouched, the dev
//!    loop is Tier-F's); the release tier splits per CLUSTER (`c00`…),
//!    compiler-chosen and invisible in the source.
//! 2. **Cluster cache**: a warm release rebuild reuses; a semantic
//!    edit recompiles, and the miss reason names the summary/cluster
//!    component (the s43 key half). An edit that leaves the OPTIMIZED
//!    WIR identical legitimately reuses — that is D8 content
//!    addressing doing its job, not a stale hit.
//! 3. **Stale objects are unaddressable**: object names embed the key
//!    and stale keys are pruned, so a poisoned object at an old key is
//!    never linked.
//! 4. **Reproducibility (D4)**: two clean release builds of the same
//!    source are byte-identical — including across DIFFERENT THREAD
//!    COUNTS, because cluster assignment and every budget are
//!    count-fixed, not schedule-derived.
//! 5. **`--codegen-report`**: the frozen summary format reaches the
//!    driver surface, devirt headroom (`impls=[]`) and all.
//!
//! Environment problems (no cc/clang, no rt lib) SKIP loudly (exit 2
//! from `wolf build`); refusals and compile errors FAIL.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const MAIN: &str =
    "use alpha\n\nfn main() -> !int {\n    if alpha.combine(3) == 12 { 0 } else { 1 }\n}\n";
const ALPHA: &str = "//! member: true\nuse beta\n\n/// Combine.\npub fn combine(x: int) -> int {\n    beta.triple(x) + 3\n}\n";
const BETA: &str =
    "//! member: true\n\n/// Triple.\npub fn triple(x: int) -> int {\n    x * 3\n}\n";
/// A semantic change: the program's answer moves, so the optimized
/// WIR moves, so the cluster key moves.
const BETA_SEMANTIC: &str =
    "//! member: true\n\n/// Triple.\npub fn triple(x: int) -> int {\n    x * 5\n}\n";

fn fixture(case: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("alpha")).expect("mkdir alpha");
    std::fs::create_dir_all(dir.join("beta")).expect("mkdir beta");
    std::fs::write(dir.join("main.lu"), MAIN).expect("write main");
    std::fs::write(dir.join("alpha/alpha.lu"), ALPHA).expect("write alpha");
    std::fs::write(dir.join("beta/beta.lu"), BETA).expect("write beta");
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

/// One build; returns `(unit, verdict)` pairs off `--verbose`, or
/// `None` on the environment SKIP.
fn build(dir: &Path, out: &str, extra: &[&str]) -> Option<Vec<(String, String)>> {
    let o = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join(out))
        .arg("--verbose")
        .args(extra)
        .output()
        .expect("wolf runs");
    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
    match o.status.code() {
        Some(0) => {}
        Some(2) => {
            eprintln!("SKIP: environment cannot build natively: {}", stderr.trim());
            return None;
        }
        _ if stderr.contains("release tier targets linux/x86-64") => {
            // The tier's named host refusal (linux/x86-64 +
            // macOS/aarch64 since s127) — a loud skip (s59).
            eprintln!("SKIP: the release tier refuses this host");
            return None;
        }
        other => panic!("wolf build failed (exit {other:?}): {stderr}"),
    }
    let mut acct = Vec::new();
    for line in stderr.lines() {
        let Some(rest) = line.strip_prefix("wolf build: ") else {
            continue;
        };
        let Some((name, verdict)) = rest.split_once(": ") else {
            continue;
        };
        acct.push((name.to_string(), verdict.to_string()));
    }
    Some(acct)
}

fn cached_objects(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir.join(".lu-cache/obj"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Granularity per tier, the cluster cache, and the poisoned-object
/// proof — one fixture, one cache, walked through its states.
#[test]
fn release_clusters_and_the_coarse_cache() {
    ensure_rt_staticlib();
    let dir = fixture("wp_clusters");
    // Debug tier: per-module units, D7 granularity untouched.
    let Some(debug) = build(&dir, "prog-debug", &[]) else {
        return;
    };
    let names: Vec<&str> = debug.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"root") && names.contains(&"alpha") && names.contains(&"beta"),
        "the debug tier still splits per source module: {debug:?}"
    );

    // Release tier: cluster units, compiler-named.
    let dir = fixture("wp_clusters_rel");
    let Some(cold) = build(&dir, "prog", &["--release"]) else {
        return;
    };
    assert!(
        cold.iter()
            .all(|(n, _)| n.starts_with('c') && n[1..].chars().all(|c| c.is_ascii_digit())),
        "release units are clusters, not modules: {cold:?}"
    );
    assert!(
        cold.iter().all(|(_, v)| v.starts_with("compiled")),
        "{cold:?}"
    );
    let run = Command::new(dir.join("prog")).output().expect("runs");
    assert_eq!(run.status.code(), Some(0), "the program is correct");

    // Warm: every cluster reuses.
    let warm = build(&dir, "prog", &["--release"]).expect("env ok");
    assert!(
        warm.iter().all(|(_, v)| v.starts_with("reused")),
        "warm release rebuild reuses every cluster: {warm:?}"
    );
    let stale = cached_objects(&dir);
    assert_eq!(stale.len(), cold.len(), "one live object per cluster");

    // Semantic edit: the cluster recompiles, and the miss reason names
    // the responsible key component. Since s125 the cluster's `src`
    // component carries the site tables (path + line starts) of every
    // file the cluster lowers — the span-key staleness fix — so an
    // edit INSIDE the cluster is attributed to `source changed` (src
    // is checked before sum; this fixture is a single cluster, on
    // every machine — D4 clustering is machine-independent). The
    // summary/cluster component still drives misses whose source did
    // not move — pgo.rs's `a_profile_is_an_input_and_keys_the_rebuild`
    // holds that path through its toolchain/profile component.
    std::fs::write(dir.join("beta/beta.lu"), BETA_SEMANTIC).expect("edit beta");
    let edited = build(&dir, "prog", &["--release"]).expect("env ok");
    assert!(
        edited
            .iter()
            .any(|(_, v)| v.contains("source changed") || v.contains("summary/cluster changed")),
        "the edit's key component drives the miss: {edited:?}"
    );
    let run = Command::new(dir.join("prog")).output().expect("runs");
    assert_eq!(run.status.code(), Some(1), "the edit reached the binary");
    // The old key's object was pruned: stale objects are not merely
    // unused, they are gone.
    let fresh = cached_objects(&dir);
    assert!(
        fresh.iter().all(|f| !stale.contains(f)),
        "stale cluster objects pruned: {stale:?} -> {fresh:?}"
    );

    // Poison an object at an OLD key: content-addressed names mean it
    // can never be selected again.
    let objdir = dir.join(".lu-cache/obj");
    for name in &stale {
        std::fs::write(objdir.join(name), b"POISON").expect("poison write");
    }
    let after = build(&dir, "prog", &["--release"]).expect("env ok");
    assert!(
        after.iter().all(|(_, v)| v.starts_with("reused")),
        "the honest objects still hit: {after:?}"
    );
    let run = Command::new(dir.join("prog")).output().expect("runs");
    assert_eq!(
        run.status.code(),
        Some(1),
        "a poisoned stale object never reaches the link"
    );
}

/// D4 reproducibility: two clean release builds are byte-identical,
/// twice, at different thread counts.
#[test]
fn release_builds_are_reproducible() {
    ensure_rt_staticlib();
    let dir = fixture("wp_reproducible");
    // Same basename (`prog`) in sibling directories: the Mach-O linker
    // signature folds the output basename in as the code-signing
    // identifier (s59; incremental.rs pins the same property), so
    // differently named outputs could never be bit-identical on macOS
    // however deterministic the build.
    for (a, b) in [("r1", "r2"), ("r3", "r4")] {
        for sub in [a, b] {
            std::fs::create_dir_all(dir.join(sub)).expect("mkdir");
        }
        let _ = std::fs::remove_dir_all(dir.join(".lu-cache"));
        if build(&dir, &format!("{a}/prog"), &["--release"]).is_none() {
            return;
        }
        let _ = std::fs::remove_dir_all(dir.join(".lu-cache"));
        // A different thread count must not change a single byte.
        let out = Command::new(wolf())
            .arg("build")
            .arg(dir.join("main.lu"))
            .arg("-o")
            .arg(dir.join(b).join("prog"))
            .arg("--release")
            .env("RAYON_NUM_THREADS", "1")
            .output()
            .expect("wolf runs");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let x = std::fs::read(dir.join(a).join("prog")).expect("read a");
        let y = std::fs::read(dir.join(b).join("prog")).expect("read b");
        assert_eq!(
            x, y,
            "two clean release builds must be byte-identical ({a} vs {b}, different thread counts)"
        );
    }
}

/// The frozen summary format reaches the driver's own diagnostic
/// surface, reserved slots included.
#[test]
fn codegen_report_dumps_the_frozen_summary() {
    ensure_rt_staticlib();
    let dir = fixture("wp_report");
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("prog"))
        .arg("--release")
        .arg("--codegen-report")
        .output()
        .expect("wolf runs");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if out.status.code() == Some(2) {
        eprintln!("SKIP: environment cannot build natively: {}", stderr.trim());
        return;
    }
    if stderr.contains("release tier targets linux/x86-64") {
        // The tier's named host refusal — a loud skip (s59).
        eprintln!("SKIP: the release tier refuses this host");
        return;
    }
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(
        stderr.contains("summary-format 3"),
        "the frozen format is versioned on the wire:\n{stderr}"
    );
    for line in stderr.lines().filter(|l| l.starts_with("fn ")) {
        assert!(
            line.contains("impls=[]") && line.contains("hot=-"),
            "the reserved slots (D42 devirt headroom, s45 hotness) print empty: {line}"
        );
        assert!(line.contains("home=") && line.contains("hash="), "{line}");
    }
    assert!(
        stderr.lines().any(|l| l.starts_with("cluster c")),
        "clusters are reported:\n{stderr}"
    );
    assert!(
        stderr.contains("whole-program:"),
        "the counters are reported:\n{stderr}"
    );
}
