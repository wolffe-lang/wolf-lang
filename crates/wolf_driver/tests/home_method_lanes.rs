//! s237 (wolf-lang#745, `[type.method.resolve]` step 2) — a home-module
//! method call runs on the checked machine, and is the free call.
//!
//! `recv.name(args)` on a `List`, `Map` or `str` whose `name` is not a
//! builtin resolves to the home module's `pub fn` and IS
//! `home.name(recv, args)` (s166). Native, release and lupin ran it;
//! the checked machine refused the method spelling by name ("home-module
//! method calls in checked execution") and ran the free spelling, which
//! kept every program that writes `m.keys()` off the reference machine
//! (pelt, the largest wolf program outside the compiler, among them).
//!
//! The fixture is self-contained (`fixtures/home_method/`): a small std
//! tree written for it (`std.list`, `std.map`, `std.str`) and one
//! program in two spellings, method and free. The candidates cover a
//! generic bound through the receiver (`any[T]`, `pick[T]`, `keys[K,
//! V]`), a non-generic one with a second argument (`sum_over`), a `mut`
//! receiver (`(mut xs).double_all()`), a fn-typed argument, and a `str`
//! receiver. Both spellings print the same two lines on the checked
//! machine, native, release and the sibling lupin.
//!
//! Red at trunk `b407c73b`: the method spelling is `unsupported` on the
//! checked machine (the free spelling and the other three machines
//! already agreed; `~/lanes/s237/hmfix` on kasumi, wolf 0.2.29's
//! archive).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const WANT: &str = "true 10 2 6 4\n2 a 2 abab\n";

fn fixture(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/home_method")
        .join(rel);
    assert!(p.exists(), "fixture missing: {}", p.display());
    p
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    refusal: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!(
            "{what} record parses: {e}; stderr {}",
            String::from_utf8_lossy(stderr)
        )
    });
    let refusal = rec["x-unsupported-construct"]
        .as_str()
        .or(rec["x-unsupported"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned());
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        refusal,
    }
}

/// Build the runtime staticlib once (the compiled lanes link it).
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

/// One wolfgang lane over the fixture's own std tree; `None` is the s59
/// environment skip, named on stderr (never the checked machine).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    ensure_rt_staticlib();
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .arg("--std-root")
        .arg(fixture("std"))
        .env_remove("WOLF_STD")
        .output()
        .expect("wolf runs");
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, &out.stderr, "the observation"))
}

/// The sibling lupin, found exactly as `pairing.rs` finds it.
fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s237's #745 gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — only the oracle leg is absent");
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .arg("--std-root")
        .arg(fixture("std"))
        .output()
        .expect("lupin runs");
    assert!(
        !out.stdout.is_empty(),
        "lupin wrote no record for {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, &out.stderr, "lupin's observation"))
}

fn every_machine_prints(spelling: &str) {
    let entry = fixture(&format!("{spelling}/main.lu"));
    let mut who = vec![("checked", lane(&entry, "--checked"))];
    who.push(("native", lane(&entry, "--native")));
    who.push(("release", lane(&entry, "--release")));
    who.push(("lupin", lupin_says(&entry)));
    for (name, obs) in who {
        let Some(obs) = obs else { continue };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", WANT),
            "{name} {} on the {spelling} spelling ([type.method.resolve], wolf-lang#745); \
             refusal {:?}",
            obs.version,
            obs.refusal
        );
    }
}

/// `xs.any(big)`, `(mut xs).double_all()`, `m.keys()`, `s.twice()`.
#[test]
fn the_method_spelling_runs_on_every_machine() {
    every_machine_prints("method");
}

/// `list.any(xs, big)`, `list.double_all(mut xs)`, `map.keys(m)`: the
/// same call written out, the same two lines.
#[test]
fn the_free_spelling_is_the_same_call() {
    every_machine_prints("free");
}
