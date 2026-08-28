//! s45 acceptance at the driver: the PGO round trip, the
//! never-required posture, stale tolerance, and D7's behaviour when a
//! profile is the input that changed.
//!
//! The round trip under test is the whole sprint in five steps:
//! instrument → run on a TRAINING input → read the `.wprof` back →
//! rebuild with `--profile=` → check the build changed performance and
//! not behaviour. Everything else here exists to pin the properties
//! that make that trip safe:
//!
//! 1. **Never required** (D4). A build with no profile says nothing,
//!    warns about nothing, and is the build it always was. A build
//!    whose profile matches NOTHING is byte-identical to it.
//! 2. **Content-hash keying.** An edit invalidates exactly what it
//!    changed: a profile taken before an unrelated edit still applies
//!    to the bodies the edit did not touch.
//! 3. **Version discipline.** A `.wprof` this compiler cannot read is
//!    a loud build error, never a silently ignored file.
//! 4. **The instrumentation stamp.** An instrumented binary carries
//!    the profile runtime's symbols; a release binary does not, and no
//!    instrumented object is ever written to the release object cache.
//! 5. **D7.** The profile is an input, so it keys the build: the same
//!    profile reuses, a different profile misses, and a stale profile
//!    cannot silently produce a different binary.
//!
//! Environment problems (no cc/clang, no rt lib) SKIP loudly (exit 2
//! from `wolf build`); refusals and compile errors FAIL.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A branchy loop with a lopsided arm — the shape a profile has
/// something to say about — plus a second function the edit tests move
/// independently.
const MAIN: &str = "\
fn work(n: int) -> int {
    var s = 0
    var i = 0
    while i < n {
        if i % 7 == 0 { s = s + i } else { s = s + 1 }
        i = i + 1
    }
    s
}

fn other(n: int) -> int {
    n + 1
}

fn main() -> !int {
    let n = 4000
    print(\"{work(n) + other(n)}\")
    0
}
";

/// Two recursive helpers, which the inliner refuses by construction,
/// so THREE bodies survive the whole-program phase into the profile.
///
/// The fixture has to be built this way, and that is itself the
/// finding: content-hash keying invalidates at the granularity of the
/// bodies that SURVIVE optimization, not of source functions. In a
/// program small enough to collapse into one body, any edit
/// invalidates the whole profile — precisely, and harmlessly (the
/// records are ignored, never misapplied), but wholly. Recorded in
/// `crates/wolf_wir/wprof-format.md`.
const RECURSIVE: &str = "\
fn ping(n: int) -> int {
    if n <= 0 { 0 } else { ping(n - 1) + 1 }
}

fn pong(n: int) -> int {
    if n <= 0 { 0 } else { pong(n - 1) + 2 }
}

fn main() -> !int {
    print(\"{ping(300) + pong(300)}\")
    0
}
";

/// `pong` alone rewritten: `ping`'s body — and hence its content hash,
/// and hence its profile record — is untouched, and so is `main`'s
/// (it calls the same two names).
const RECURSIVE_EDITED: &str = "\
fn ping(n: int) -> int {
    if n <= 0 { 0 } else { ping(n - 1) + 1 }
}

fn pong(n: int) -> int {
    if n <= 0 { 0 } else { pong(n - 1) + 3 }
}

fn main() -> !int {
    print(\"{ping(300) + pong(300)}\")
    0
}
";

fn fixture(case: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir case");
    std::fs::write(dir.join("main.lu"), src).expect("write main");
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

/// One `wolf build`; `None` is the environment SKIP. Returns stderr.
fn build(dir: &Path, out: &str, extra: &[&str]) -> Option<String> {
    let o = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join(out))
        .arg("--release")
        .arg("--verbose")
        .args(extra)
        .output()
        .expect("wolf runs");
    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
    match o.status.code() {
        Some(0) => Some(stderr),
        Some(2) => {
            eprintln!("SKIP: environment cannot build natively: {}", stderr.trim());
            None
        }
        _ if stderr.contains("release tier targets linux/x86-64") => {
            // The tier's named host refusal (linux/x86-64 only until
            // its own c13 sprint) — a loud skip (s59).
            eprintln!("SKIP: the release tier refuses this host");
            None
        }
        other => panic!("wolf build failed (exit {other:?}): {stderr}"),
    }
}

/// Run a built binary, returning its stdout.
fn run(bin: &Path) -> String {
    let o = Command::new(bin).output().expect("the binary runs");
    assert!(
        o.status.success(),
        "{} exited {:?}",
        bin.display(),
        o.status
    );
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn sha(p: &Path) -> String {
    wolf_wir::sha256_hex(&std::fs::read(p).expect("read artifact"))
}

// ---------------------------------------------------------------------
// 1. The round trip
// ---------------------------------------------------------------------

/// instrument → train → rebuild: the profile is produced, is readable,
/// applies in full, fills the reserved `hot=` slot, and does not change
/// what the program prints.
#[test]
fn the_round_trip_produces_a_profile_that_applies() {
    ensure_rt_staticlib();
    let dir = fixture("pgo_round_trip", MAIN);
    let gen_dir = dir.join("prof");
    std::fs::create_dir_all(&gen_dir).expect("mkdir prof");
    let gen_arg = format!("--profile-gen={}", gen_dir.display());
    let Some(_) = build(&dir, "gen", &[&gen_arg]) else {
        return;
    };
    // The TRAINING run. Its output is also the behavioural oracle:
    // instrumentation changes performance, never behaviour.
    let trained = run(&dir.join("gen"));
    let wprof = gen_dir.join("default.wprof");
    let text = std::fs::read_to_string(&wprof).expect("the run wrote a profile");
    assert!(
        text.starts_with("wprof 1\nproducer instr\n"),
        "the runtime writes canonical v1:\n{text}"
    );
    let p = wolf_wir::profile::Profile::parse(&text)
        .expect("the compiler reads what the runtime wrote");
    assert!(p.samples() > 0, "the training run counted something");

    // Rebuild consuming it.
    let use_arg = format!("--profile={}", wprof.display());
    let stderr = build(&dir, "pgo", &["--codegen-report", &use_arg]).expect("built once already");
    assert!(
        !stderr.contains("no longer match"),
        "a profile from this very build is not stale:\n{stderr}"
    );
    assert!(
        stderr.contains("profile: ") && stderr.contains("100.0%"),
        "the whole profile applied:\n{stderr}"
    );
    // The reserved slot is filled — s43 left it for exactly this.
    let hot: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("fn "))
        .filter_map(|l| l.split_whitespace().find(|t| t.starts_with("hot=")))
        .collect();
    assert!(
        !hot.is_empty(),
        "the report prints the summary index:\n{stderr}"
    );
    assert!(
        hot.contains(&"hot=1000"),
        "the hottest body ranks 1000: {hot:?}"
    );
    assert_eq!(
        run(&dir.join("pgo")),
        trained,
        "PGO changes performance, never behaviour"
    );
}

/// The whole point of the block counts: LLVM sees the measured weights.
#[test]
fn measured_branch_weights_reach_the_llvm_ir() {
    ensure_rt_staticlib();
    let dir = fixture("pgo_branch_weights", MAIN);
    let gen_arg = format!("--profile-gen={}", dir.display());
    let Some(_) = build(&dir, "gen", &[&gen_arg]) else {
        return;
    };
    run(&dir.join("gen"));
    let wprof = dir.join("default.wprof");
    let use_arg = format!("--profile={}", wprof.display());

    // Without a profile: only the two SEMANTIC cold nodes exist (trap
    // arcs and error arcs, report 10 delta 3), and they are the fixed
    // 1-vs-2000 shape.
    build(&dir, "plain.ll", &["--emit=llvm-ir"]).expect("built once already");
    let plain = std::fs::read_to_string(dir.join("plain.ll")).expect("ir");
    for line in plain.lines().filter(|l| l.contains("branch_weights")) {
        assert!(
            line.contains("i32 1, i32 2000"),
            "an unprofiled build's only weights are the semantic cold ones: {line}"
        );
    }

    // With one: measured weights appear beside them.
    build(&dir, "pgo.ll", &["--emit=llvm-ir", &use_arg]).expect("built once already");
    let pgo = std::fs::read_to_string(dir.join("pgo.ll")).expect("ir");
    let measured: Vec<&str> = pgo
        .lines()
        .filter(|l| l.contains("branch_weights") && !l.contains("i32 1, i32 2000"))
        .collect();
    assert!(
        !measured.is_empty(),
        "the profile's block counts became !prof weights:\n{pgo}"
    );
    // Never zero: "did not run on the training input" is not "cannot
    // run", and a zero weight tells LLVM the latter.
    for line in &measured {
        assert!(!line.contains("i32 0"), "no weight is ever zero: {line}");
    }
    // And the sentinel still silences the channel with all the others.
    let o = Command::new(wolf())
        .arg("build")
        .arg(dir.join("main.lu"))
        .arg("-o")
        .arg(dir.join("strip.ll"))
        .args(["--release", "--emit=llvm-ir"])
        .arg(&use_arg)
        .env("WOLF_STRIP_FACTS", "1")
        .output()
        .expect("wolf runs");
    assert!(o.status.success(), "stripped build: {:?}", o.status);
    let stripped = std::fs::read_to_string(dir.join("strip.ll")).expect("ir");
    assert!(
        !stripped.contains("branch_weights"),
        "!prof is a fact channel and WOLF_STRIP_FACTS silences it too"
    );
}

// ---------------------------------------------------------------------
// 2. Never required, and stale tolerance
// ---------------------------------------------------------------------

/// A build with no profile is a normal build: no warning, no nag, no
/// mention of PGO anywhere. A build whose profile matches nothing is
/// byte-identical to it, plus exactly one summary line.
#[test]
fn no_profile_is_a_normal_build_and_a_stale_one_is_the_same_binary() {
    ensure_rt_staticlib();
    let dir = fixture("pgo_never_required", MAIN);
    let Some(stderr) = build(&dir, "plain", &["--no-cache"]) else {
        return;
    };
    for word in ["profile", "PGO", "wprof"] {
        assert!(
            !stderr.contains(word),
            "an unprofiled build never mentions `{word}`:\n{stderr}"
        );
    }

    // A syntactically valid profile for a body that does not exist.
    let stale = dir.join("stale.wprof");
    std::fs::write(
        &stale,
        "wprof 1\nproducer instr\nruns 1\nsamples 12\n\
         fn 0000000000000000000000000000000000000000000000000000000000000001 2 5 7\n",
    )
    .expect("write stale profile");
    let arg = format!("--profile={}", stale.display());
    let stderr = build(&dir, "stale", &["--no-cache", &arg]).expect("built once already");
    assert!(
        stderr.contains("1 of 1 profile record(s) no longer match"),
        "one summary line, counted:\n{stderr}"
    );
    assert!(
        stderr.contains("identical to one with no profile"),
        "and it says exactly what a fully stale profile means:\n{stderr}"
    );
    assert_eq!(
        sha(&dir.join("plain")),
        sha(&dir.join("stale")),
        "a fully stale profile produces the no-profile build, byte for byte"
    );
}

/// Content-hash keying, at the driver: an edit to one body invalidates
/// that body's record and NOTHING else. The bodies that did not move
/// keep their profile across a recompilation of the program.
#[test]
fn an_edit_invalidates_exactly_what_it_changed() {
    ensure_rt_staticlib();
    let dir = fixture("pgo_stale_precision", RECURSIVE);
    let gen_arg = format!("--profile-gen={}", dir.display());
    let Some(_) = build(&dir, "gen", &[&gen_arg]) else {
        return;
    };
    run(&dir.join("gen"));
    let wprof = dir.join("default.wprof");
    let use_arg = format!("--profile={}", wprof.display());
    let before =
        wolf_wir::profile::Profile::parse(&std::fs::read_to_string(&wprof).expect("profile"))
            .expect("parses");
    assert_eq!(
        before.funcs.len(),
        3,
        "three bodies survive: two recursive helpers and main"
    );

    // Recompiling UNCHANGED source keeps every record applicable —
    // the hash is over content, so a rebuild is not an invalidation.
    let same = build(&dir, "same", &["--no-cache", "--codegen-report", &use_arg]).expect("builds");
    assert!(
        same.contains("3/3 record(s) matched"),
        "recompiling an untouched program keeps its whole profile:\n{same}"
    );
    assert!(
        !same.contains("no longer match"),
        "and says nothing, because nothing is stale:\n{same}"
    );

    // Edit `pong`. What goes stale is `pong` and every body that
    // CONTAINS a copy of it — here `main`, which inlined one level of
    // it. That is not over-invalidation: those bodies did change, and
    // their old block counts describe blocks that no longer exist.
    // `ping`, which changed in no way, keeps its record across the
    // edit, which is the property name-keyed profiles cannot offer.
    std::fs::write(dir.join("main.lu"), RECURSIVE_EDITED).expect("edit");
    let stderr =
        build(&dir, "after", &["--no-cache", "--codegen-report", &use_arg]).expect("builds");
    assert!(
        stderr.contains("1/3 record(s) matched this build (2 stale)"),
        "the edited body and the one that inlined it go stale; the third survives:\n{stderr}"
    );
    assert!(
        stderr.contains("2 of 3 profile record(s) no longer match"),
        "and the build says so, once, without erroring:\n{stderr}"
    );
    assert!(
        !stderr.contains("identical to one with no profile"),
        "a PARTIALLY stale profile still applies:\n{stderr}"
    );
    // The surviving record is the untouched body's, named by hash.
    let ping_hash = stderr
        .lines()
        .find(|l| l.starts_with("fn ping "))
        .and_then(|l| l.split_whitespace().find(|t| t.starts_with("hash=")))
        .map(|t| t.trim_start_matches("hash=").to_string())
        .expect("the report names ping's hash");
    assert!(
        before.get(&ping_hash).is_some(),
        "the surviving match is the body the edit did not touch"
    );
}

/// The version discipline reaches the driver: a profile this compiler
/// cannot read stops the build and says why.
#[test]
fn an_unreadable_profile_is_a_loud_build_error() {
    let dir = fixture("pgo_bad_profile", MAIN);
    for (name, body, want) in [
        ("future.wprof", "wprof 2\nproducer instr\n", "refusing"),
        (
            "garbage.wprof",
            "hello, i am not a profile\n",
            "wprof <version>",
        ),
        (
            "sampled.wprof",
            "wprof 1\nproducer sample\n",
            "producer `sample`",
        ),
    ] {
        let p = dir.join(name);
        std::fs::write(&p, body).expect("write");
        let o = Command::new(wolf())
            .arg("build")
            .arg(dir.join("main.lu"))
            .arg("-o")
            .arg(dir.join("out"))
            .arg("--release")
            .arg(format!("--profile={}", p.display()))
            .output()
            .expect("wolf runs");
        let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
        assert_eq!(o.status.code(), Some(2), "{name}: {stderr}");
        assert!(
            stderr.contains(want),
            "{name}: the refusal says what is wrong: {stderr}"
        );
    }
}

/// PGO is release-tier surface and says so rather than instrumenting a
/// build that would never use the result.
#[test]
fn the_pgo_flags_refuse_outside_the_release_tier() {
    let dir = fixture("pgo_flag_shape", MAIN);
    for (args, want) in [
        (vec!["--profile-gen"], "pass --release"),
        (vec!["--profile=x.wprof"], "pass --release"),
        (
            vec!["--release", "--profile-gen", "--profile=x.wprof"],
            "two halves of PGO",
        ),
    ] {
        let o = Command::new(wolf())
            .arg("build")
            .arg(dir.join("main.lu"))
            .arg("-o")
            .arg(dir.join("out"))
            .args(&args)
            .output()
            .expect("wolf runs");
        let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
        assert_eq!(o.status.code(), Some(2), "{args:?}: {stderr}");
        assert!(stderr.contains(want), "{args:?}: {stderr}");
    }
}

// ---------------------------------------------------------------------
// 3. The stamp, and D7
// ---------------------------------------------------------------------

/// An instrumented binary is marked by construction, a release binary
/// is not, and no instrumented object reaches the release object cache.
#[test]
fn instrumented_builds_are_marked_and_never_cached() {
    ensure_rt_staticlib();
    let dir = fixture("pgo_stamp", MAIN);
    // A plain release build first, so the cache exists and is warm.
    let Some(_) = build(&dir, "plain", &[]) else {
        return;
    };
    let cached_before = cached_objects(&dir);
    assert!(
        !cached_before.is_empty(),
        "the release build populated the object cache"
    );

    let gen_arg = format!("--profile-gen={}", dir.display());
    build(&dir, "gen", &[&gen_arg]).expect("built once already");
    assert_eq!(
        cached_objects(&dir),
        cached_before,
        "an instrumented build writes no objects into the release cache"
    );

    // The stamp: the profile runtime's symbols are in the instrumented
    // binary and in no other.
    let stamp = "__wolf_rt_prof_init";
    let Some(gen_syms) = symbols(&dir.join("gen")) else {
        eprintln!("SKIP: no `nm` on this host");
        return;
    };
    let plain_syms = symbols(&dir.join("plain")).expect("nm worked once");
    assert!(
        gen_syms.contains(stamp),
        "an instrumented binary carries `{stamp}`"
    );
    assert!(
        !plain_syms.contains(stamp),
        "a release binary carries no profile runtime at all"
    );
}

/// D7: a profile is a build INPUT, so it keys the build. The same
/// profile reuses; a different one misses and names the toolchain
/// component; and a cached hit is byte-identical to a cold build.
#[test]
fn a_profile_is_an_input_and_keys_the_rebuild() {
    ensure_rt_staticlib();
    let dir = fixture("pgo_cache_key", MAIN);
    let gen_arg = format!("--profile-gen={}", dir.display());
    let Some(_) = build(&dir, "gen", &[&gen_arg]) else {
        return;
    };
    run(&dir.join("gen"));
    let wprof = dir.join("default.wprof");
    let use_arg = format!("--profile={}", wprof.display());

    // Cold with the profile, then warm with the same profile.
    build(&dir, "a", &[&use_arg]).expect("builds");
    let warm = build(&dir, "b", &[&use_arg]).expect("builds");
    assert!(
        warm.contains("reused"),
        "the same profile hits the cache:\n{warm}"
    );
    assert_eq!(
        sha(&dir.join("a")),
        sha(&dir.join("b")),
        "a cached hit is byte-identical to the cold build"
    );

    // A DIFFERENT profile is a different input: the key moves and the
    // miss is attributed to the toolchain/profile component, which is
    // where the `.wprof` hash lives. A stale profile can therefore
    // never silently reuse an object built under another one.
    let other = dir.join("other.wprof");
    let text = std::fs::read_to_string(&wprof).expect("profile");
    let doubled = double_counts(&text);
    std::fs::write(&other, &doubled).expect("write");
    let other_arg = format!("--profile={}", other.display());
    let miss = build(&dir, "c", &[&other_arg]).expect("builds");
    assert!(
        miss.contains("toolchain/profile changed"),
        "a changed profile misses on the component that carries it:\n{miss}"
    );

    // And back to no profile at all: also a distinct key, also correct.
    let plain = build(&dir, "d", &[]).expect("builds");
    assert!(
        plain.contains("toolchain/profile changed") || plain.contains("new"),
        "dropping the profile is an input change too:\n{plain}"
    );
}

/// Double every count in a `.wprof`, keeping it well-formed — a
/// genuinely different profile over the same bodies.
fn double_counts(text: &str) -> String {
    let mut p = wolf_wir::profile::Profile::parse(text).expect("the fixture profile parses");
    for r in p.funcs.values_mut() {
        for c in &mut r.blocks {
            *c *= 2;
        }
    }
    p.render()
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

fn symbols(bin: &Path) -> Option<String> {
    let o = Command::new("nm").arg("-a").arg(bin).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

// ---------------------------------------------------------------------
// 4. `wolf profile show|merge`
// ---------------------------------------------------------------------

#[test]
fn profile_show_and_merge() {
    let dir = fixture("pgo_subcommand", MAIN);
    let a = dir.join("a.wprof");
    let b = dir.join("b.wprof");
    let h1 = "1".repeat(64);
    let h2 = "2".repeat(64);
    std::fs::write(
        &a,
        format!("wprof 1\nproducer instr\nruns 1\nsamples 13\nfn {h1} 2 10 3\n"),
    )
    .expect("write a");
    std::fs::write(
        &b,
        format!("wprof 1\nproducer instr\nruns 1\nsamples 9\nfn {h1} 2 4 0\nfn {h2} 1 5\n"),
    )
    .expect("write b");

    let o = Command::new(wolf())
        .args(["profile", "show"])
        .arg(&a)
        .output()
        .expect("wolf runs");
    assert!(o.status.success());
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.contains("1 record(s), 13 sample(s)"), "{out}");
    assert!(out.contains(&h1[..16]), "{out}");

    let merged = dir.join("m.wprof");
    let o = Command::new(wolf())
        .args(["profile", "merge"])
        .arg(&merged)
        .arg(&a)
        .arg(&b)
        .output()
        .expect("wolf runs");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let p = wolf_wir::profile::Profile::parse(&std::fs::read_to_string(&merged).expect("merged"))
        .expect("parses");
    assert_eq!(p.funcs[&h1].blocks, vec![14, 3], "compatible records sum");
    assert_eq!(p.funcs[&h2].blocks, vec![5], "one-sided records survive");
    assert_eq!(p.runs, 2);

    // A corrupt pair refuses rather than picking one.
    let bad = dir.join("bad.wprof");
    std::fs::write(
        &bad,
        format!("wprof 1\nproducer instr\nruns 1\nsamples 1\nfn {h1} 1 1\n"),
    )
    .expect("write bad");
    let o = Command::new(wolf())
        .args(["profile", "merge"])
        .arg(dir.join("x.wprof"))
        .arg(&a)
        .arg(&bad)
        .output()
        .expect("wolf runs");
    assert_eq!(o.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("corrupt"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}
