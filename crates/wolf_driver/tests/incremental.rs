//! s31 acceptance — the `.lu-cache` interface-hash rebuild skeleton
//! (D7, coarse and batch) plus `wolf run` and the determinism oracle.
//!
//! Over a 3-module fixture (root → alpha → beta):
//! 1. **Cold/warm**: first build compiles every module; an unchanged
//!    rebuild reuses every object (cache-hit accounting asserted via
//!    `--verbose`).
//! 2. **Body edit**: touching a leaf function BODY recompiles exactly
//!    that module — dependents' objects (and the sema their keys rest
//!    on) stay valid.
//! 3. **Pub-surface edit**: adding a `pub` item to the leaf recompiles
//!    the leaf and its direct dependent; the root (which does not
//!    import the leaf) is untouched.
//! 4. **Determinism**: a cached rebuild and a `--no-cache` rebuild are
//!    bit-identical (the fuzz-an-edit oracle of s57 starts life here).
//! 5. **`wolf run`**: build + exec with the exit code propagated; the
//!    binary lands in `.lu-cache/bin/`.
//!
//! Environment problems (no `cc`, no rt lib) SKIP loudly (exit 2 from
//! `wolf build` is the environment signal, per the s30 test contract);
//! refusals and compile errors FAIL. Off-target the file compiles away.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const MAIN: &str =
    "use alpha\n\nfn main() -> !int {\n    if alpha.combine(3) == 12 { 0 } else { 1 }\n}\n";
const ALPHA: &str =
    "//! member: true\nuse beta\n\npub fn combine(x: int) -> int {\n    beta.triple(x) + 3\n}\n";
const BETA: &str = "//! member: true\n\npub fn triple(x: int) -> int {\n    x * 3\n}\n";
/// Same result, different body — the body-edit probe.
const BETA_BODY_EDIT: &str =
    "//! member: true\n\npub fn triple(x: int) -> int {\n    x + x + x\n}\n";
/// Adds a `pub` item — the interface-surface probe.
const BETA_PUB_EDIT: &str = "//! member: true\n\npub fn triple(x: int) -> int {\n    x + x + x\n}\n\npub fn quadruple(x: int) -> int {\n    x * 4\n}\n";

/// Lay the fixture out under a fresh tmp dir.
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

/// One `wolf build --verbose`; returns per-module accounting
/// (`name` → `compiled`/`reused`), or `None` on the environment SKIP.
fn build_verbose(dir: &Path, extra: &[&str]) -> Option<Vec<(String, String)>> {
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("prog"))
        .arg("--verbose")
        .args(extra)
        .output()
        .expect("wolf runs");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    match out.status.code() {
        Some(0) => {}
        Some(2) => {
            eprintln!(
                "SKIP: environment cannot link native binaries: {}",
                stderr.trim()
            );
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
        let kind = if verdict.starts_with("reused") {
            "reused"
        } else if verdict.starts_with("compiled") {
            "compiled"
        } else {
            continue;
        };
        acct.push((name.to_string(), kind.to_string()));
    }
    Some(acct)
}

fn kinds(acct: &[(String, String)]) -> std::collections::HashMap<&str, &str> {
    acct.iter().map(|(n, k)| (n.as_str(), k.as_str())).collect()
}

#[test]
fn cache_granularity_and_determinism() {
    let dir = fixture("incremental");
    // 1. Cold build: everything compiles.
    let Some(cold) = build_verbose(&dir, &[]) else {
        return;
    };
    assert_eq!(cold.len(), 3, "three module objects: {cold:?}");
    assert!(cold.iter().all(|(_, k)| k == "compiled"), "{cold:?}");
    // Interface files persisted (s12 emission).
    for m in ["root", "alpha", "beta"] {
        assert!(
            dir.join(".lu-cache/ifaces")
                .join(format!("{m}.wolfi"))
                .is_file(),
            "missing {m}.wolfi"
        );
    }
    // Warm rebuild: everything reuses.
    let warm = build_verbose(&dir, &[]).expect("env ok after cold build");
    assert!(warm.iter().all(|(_, k)| k == "reused"), "{warm:?}");

    // 2. Leaf BODY edit: exactly beta recompiles — dependents' sema
    //    and objects stay valid (D7 granularity one).
    std::fs::write(dir.join("beta/beta.lu"), BETA_BODY_EDIT).expect("edit beta");
    let body = build_verbose(&dir, &[]).expect("env ok");
    let k = kinds(&body);
    assert_eq!(k["beta"], "compiled", "{body:?}");
    assert_eq!(
        k["alpha"], "reused",
        "body edit must not rebuild dependents: {body:?}"
    );
    assert_eq!(k["root"], "reused", "{body:?}");

    // 3. Leaf PUB edit: beta + its direct dependent alpha recompile;
    //    root (no beta import) is untouched (D7 granularity two).
    std::fs::write(dir.join("beta/beta.lu"), BETA_PUB_EDIT).expect("edit beta");
    let publ = build_verbose(&dir, &[]).expect("env ok");
    let k = kinds(&publ);
    assert_eq!(k["beta"], "compiled", "{publ:?}");
    assert_eq!(
        k["alpha"], "compiled",
        "pub edit must rebuild dependents: {publ:?}"
    );
    assert_eq!(k["root"], "reused", "{publ:?}");

    // The program still runs correctly through every cache state.
    let run = Command::new(dir.join("prog")).output().expect("prog runs");
    assert_eq!(run.status.code(), Some(0));

    // 4. Determinism: cached rebuild ≡ --no-cache rebuild, byte for
    //    byte (CI's fuzz-an-edit oracle seed).
    let cached = std::fs::read(dir.join("prog")).expect("read cached binary");
    // The fresh build keeps the SAME basename in a sibling directory:
    // Mach-O's linker signature folds the output's basename in as the
    // code-signing identifier, so `prog-fresh` could never be
    // bit-identical to `prog` on macOS however deterministic the
    // build (s59); a same-named output compares the real property.
    let fresh_dir = dir.join("fresh");
    std::fs::create_dir_all(&fresh_dir).expect("mkdir fresh");
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(fresh_dir.join("prog"))
        .arg("--no-cache")
        .output()
        .expect("wolf runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let fresh = std::fs::read(fresh_dir.join("prog")).expect("read fresh binary");
    assert_eq!(
        cached, fresh,
        "cached and --no-cache builds must be bit-identical"
    );
}

#[test]
fn wolf_run_builds_and_propagates_exit() {
    let dir = fixture("wolf_run");
    // A probe for the environment first (same skip discipline).
    if build_verbose(&dir, &[]).is_none() {
        return;
    }
    let out = Command::new(wolf())
        .arg("run")
        .arg(dir.join("main.lu"))
        .output()
        .expect("wolf runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "wolf run exit: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        dir.join(".lu-cache/bin/main").is_file(),
        "wolf run caches the binary under .lu-cache/bin/"
    );
    // Exit codes propagate: break the program's expectation so main
    // returns 1.
    std::fs::write(
        dir.join("beta/beta.lu"),
        "//! member: true\n\npub fn triple(x: int) -> int {\n    x * 5\n}\n",
    )
    .expect("edit beta");
    let out = Command::new(wolf())
        .arg("run")
        .arg(dir.join("main.lu"))
        .output()
        .expect("wolf runs");
    assert_eq!(
        out.status.code(),
        Some(1),
        "edited program exits 1 via wolf run"
    );
}

/// hello.lu end to end under `wolf build` (the M1 headline): the
/// f-string print lowers natively and the binary prints the literal
/// output.
#[test]
fn hello_prints_natively() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hello_native");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let hello = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("corpus/hello.lu");
    std::fs::copy(&hello, dir.join("hello.lu")).expect("copy corpus/hello.lu");
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join("hello.lu"))
        .arg("-o")
        .arg(dir.join("hello"))
        .output()
        .expect("wolf runs");
    match out.status.code() {
        Some(0) => {}
        Some(2) => {
            eprintln!(
                "SKIP: environment cannot link native binaries: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            return;
        }
        other => panic!(
            "wolf build failed (exit {other:?}): {}",
            String::from_utf8_lossy(&out.stderr)
        ),
    }
    let run = Command::new(dir.join("hello"))
        .output()
        .expect("hello runs");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello, wolf\n");
}
/// Concurrent links of BYTE-IDENTICAL object sets must not interfere
/// (the s59 regression this pins: a content-hash staging dir made two
/// identical links share a directory, and the first finisher's
/// post-link cleanup deleted `00-root.o` out from under the second's
/// `cc` — surfacing as a bogus environment exit 2). The staging dir
/// is unique per link again; determinism rides `-oso_prefix` instead
/// and is pinned by `cache_granularity_and_determinism` above.
///
/// The crash itself needs a milliseconds-wide interleaving (one
/// build's cleanup between a sibling's object write and its `cc`
/// open), so this asserts the ISOLATION INVARIANT deterministically
/// instead: with `TMPDIR` pointed at a test-owned directory, two
/// simultaneous identical builds must stage under two DISTINCT
/// `wolf-link-*` directories — the shared-dir scheme shows exactly
/// one, every time — and both builds must succeed.
#[test]
// The children ARE reaped — `try_wait` returning `Some` is the reap —
// but clippy's zombie analysis cannot see through the polling loop.
#[allow(clippy::zombie_processes)]
fn concurrent_identical_links_stage_in_distinct_dirs() {
    let dir = fixture("concurrent_links");
    // Environment probe first (the file's skip discipline).
    if build_verbose(&dir, &[]).is_none() {
        return;
    }
    let tmp = dir.join("tmp");
    std::fs::create_dir_all(&tmp).expect("mkdir tmp");
    let spawn = |tag: &str| {
        Command::new(wolf())
            .arg("build")
            .arg(dir.join("main.lu"))
            .arg("-o")
            .arg(dir.join(format!("race-{tag}")))
            .arg("--no-cache")
            // unix reads TMPDIR; windows reads TMP/TEMP (s60a) — the
            // staging dir must land under the test-owned directory on
            // both.
            .env("TMPDIR", &tmp)
            .env("TMP", &tmp)
            .env("TEMP", &tmp)
            .spawn()
            .expect("spawn wolf")
    };
    let mut a = spawn("a");
    let mut b = spawn("b");
    // Sample the staging dirs while both builds live. Each staging
    // dir exists for its build's whole cc+dsymutil span, so a few-ms
    // poll cannot miss it.
    let mut seen = std::collections::BTreeSet::new();
    let (mut sa, mut sb) = (None, None);
    while sa.is_none() || sb.is_none() {
        if let Ok(rd) = std::fs::read_dir(&tmp) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("wolf-link-") {
                    seen.insert(name);
                }
            }
        }
        if sa.is_none() {
            sa = a.try_wait().expect("poll a");
        }
        if sb.is_none() {
            sb = b.try_wait().expect("poll b");
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let (sa, sb) = (sa.unwrap(), sb.unwrap());
    assert!(
        sa.success() && sb.success(),
        "concurrent identical links interfered (a: {sa:?}, b: {sb:?})"
    );
    assert!(
        seen.len() >= 2,
        "both builds staged under the SAME link dir ({seen:?}) — the \
         s59 content-hash collision is back; staging must be unique \
         per link, with determinism carried by -oso_prefix instead"
    );
    // Both products actually run.
    for tag in ["a", "b"] {
        let st = Command::new(dir.join(format!("race-{tag}")))
            .status()
            .expect("run product");
        assert_eq!(st.code(), Some(0), "race-{tag}: product broken");
    }
}
