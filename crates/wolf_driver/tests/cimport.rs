//! `wolf c-import` end to end (s46, c10).
//!
//! Drives the real binary against the real worker process, because the
//! thing under test *is* the process boundary: the compiler locates an
//! importer, talks to it over stdio, and never links a C frontend.

use std::path::{Path, PathBuf};
use std::process::Command;

fn target_dir() -> PathBuf {
    // `…/target/debug/deps/<test binary>` → `…/target/debug`
    let mut p = std::env::current_exe().expect("test binary path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p
}

fn wolf() -> PathBuf {
    target_dir().join(if cfg!(windows) { "wolf.exe" } else { "wolf" })
}

fn worker() -> PathBuf {
    let p = target_dir().join(if cfg!(windows) {
        "wolf-cimport-worker.exe"
    } else {
        "wolf-cimport-worker"
    });
    // `cargo test --workspace` (what CI runs) builds it; a targeted
    // `cargo test -p wolf_driver` does not, and the failure that
    // produces otherwise is baffling.
    assert!(
        p.exists(),
        "the importer worker is not built. Run `cargo build -p wolf_cimport` \
         (or `cargo test --workspace`, which is what CI runs)."
    );
    p
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wolf-cimport-it-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Run `wolf c-import`, with the cache pointed at `cache` and the
/// worker pinned (or deliberately absent).
fn run(args: &[&str], cache: &Path, with_worker: bool) -> Out {
    run_from(wolf(), args, cache, with_worker)
}

fn run_from(exe: PathBuf, args: &[&str], cache: &Path, with_worker: bool) -> Out {
    let mut c = Command::new(exe);
    c.arg("c-import")
        .args(args)
        .env("WOLF_CACHE", cache)
        // Keep the host's real cache and any ambient worker out of it.
        .env_remove("XDG_CACHE_HOME");
    if with_worker {
        c.env("WOLF_CIMPORT_WORKER", worker());
    } else {
        c.env_remove("WOLF_CIMPORT_WORKER");
        // An empty PATH so the search cannot find a worker there.
        c.env("PATH", "");
    }
    let o = c.output().expect("runs wolf");
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// A copy of the `wolf` binary in a directory with no worker beside it.
/// The search order looks next to the running binary (that is how a
/// toolchain ships a worker), so "no worker anywhere" means moving the
/// compiler, not just clearing `PATH`.
fn lonely_wolf(name: &str) -> (PathBuf, PathBuf) {
    let bin = scratch(name);
    let exe = bin.join(if cfg!(windows) { "wolf.exe" } else { "wolf" });
    std::fs::copy(wolf(), &exe).expect("copies the wolf binary");
    (bin, exe)
}

fn write_header(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).expect("writes header");
}

#[test]
fn imports_a_header_and_dumps_the_artifact() {
    let dir = scratch("dump");
    let cache = scratch("dump-cache");
    write_header(
        &dir,
        "sample.h",
        "#define EOF (-1)\nvoid *malloc(size_t n);\nvoid free(void *p);\n",
    );

    let o = run(
        &["--dump", "-I", dir.to_str().expect("utf8"), "sample.h"],
        &cache,
        true,
    );
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert!(
        o.stdout.contains("c-import artifact format 1"),
        "{}",
        o.stdout
    );
    assert!(
        o.stdout.contains("fn malloc : void *(size_t)"),
        "{}",
        o.stdout
    );
    assert!(o.stdout.contains("object EOF = -1"), "{}", o.stdout);
    assert!(
        o.stdout.contains("summary: 3 imported, 0 refused"),
        "{}",
        o.stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cache);
}

/// D7's discipline, asserted the only way that means anything: the
/// worker itself records every time it is started, and the second
/// import must not add a line — not even a `--version` probe.
#[test]
fn a_second_import_of_unchanged_inputs_spawns_no_worker_process() {
    let dir = scratch("cache");
    let cache = scratch("cache-cache");
    write_header(&dir, "h.h", "int f(int x);\n");
    let trace = dir.join("spawns.log");
    let args = ["-I", dir.to_str().expect("utf8"), "h.h"];

    let spawns = |t: &Path| {
        std::fs::read_to_string(t)
            .map(|s| s.lines().count())
            .unwrap_or(0)
    };

    let mut first = Command::new(wolf());
    first
        .arg("c-import")
        .args(args)
        .env("WOLF_CACHE", &cache)
        .env_remove("XDG_CACHE_HOME")
        .env("WOLF_CIMPORT_WORKER", worker())
        .env("WOLF_CIMPORT_WORKER_TRACE", &trace);
    let o = first.output().expect("runs wolf");
    assert!(
        o.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let after_first = spawns(&trace);
    assert!(
        after_first > 0,
        "the first import must actually run the worker"
    );

    let mut second = Command::new(wolf());
    second
        .arg("c-import")
        .args(args)
        .env("WOLF_CACHE", &cache)
        .env_remove("XDG_CACHE_HOME")
        .env("WOLF_CIMPORT_WORKER", worker())
        .env("WOLF_CIMPORT_WORKER_TRACE", &trace);
    let o = second.output().expect("runs wolf");
    let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("hit — no worker ran"), "{stdout}");
    assert_eq!(
        spawns(&trace),
        after_first,
        "a rebuild with unchanged inputs started a worker process (D7 says it \
         re-imports nothing, and that has to include the version probe)"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cache);
}

/// Changing one `-D` changes the key, so the cached answer is not
/// reused for a different translation unit.
#[test]
fn changing_a_define_changes_the_cache_key() {
    let dir = scratch("define");
    let cache = scratch("define-cache");
    write_header(&dir, "h.h", "int f(int x);\n");
    let inc = dir.to_str().expect("utf8");

    let first = run(&["-I", inc, "h.h"], &cache, true);
    assert!(first.stdout.contains("miss"), "{}", first.stdout);

    // Same headers, different define: a miss, and it must not silently
    // hand back the previous artifact.
    let second = run(&["-I", inc, "-D", "FEATURE=1", "h.h"], &cache, true);
    assert!(
        second.stdout.contains("miss — imported now"),
        "a new -D must not hit the previous key: {}",
        second.stdout
    );

    // And now that one is cached too.
    let third = run(&["-I", inc, "-D", "FEATURE=1", "h.h"], &cache, false);
    assert!(third.stdout.contains("hit"), "{}", third.stdout);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cache);
}

/// A missing worker is an honest refusal that says where to put one —
/// not a silent fallback to some other program on the host.
#[test]
fn a_missing_worker_refuses_by_name() {
    let dir = scratch("noworker");
    let cache = scratch("noworker-cache");
    let (bin, exe) = lonely_wolf("noworker-bin");
    write_header(&dir, "h.h", "int f(int x);\n");

    let o = run_from(
        exe,
        &["-I", dir.to_str().expect("utf8"), "h.h"],
        &cache,
        false,
    );
    assert_eq!(o.code, 1, "stdout: {}", o.stdout);
    assert!(o.stderr.contains("WOLF_CIMPORT_WORKER"), "{}", o.stderr);
    assert!(o.stderr.contains("PATH"), "{}", o.stderr);
    assert!(
        o.stderr.contains("never links"),
        "the refusal should explain the process boundary: {}",
        o.stderr
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cache);
    let _ = std::fs::remove_dir_all(&bin);
}

/// The demotion ladder, seen from the command line: one cursed
/// declaration is refused by name and its siblings still import.
#[test]
fn refusals_are_reported_by_name_with_a_way_out() {
    let dir = scratch("refuse");
    let cache = scratch("refuse-cache");
    write_header(
        &dir,
        "cursed.h",
        "int fine_before(int x);\n\
         union tagged { int i; float f; };\n\
         long double precise(long double x);\n\
         int fine_after(int x);\n",
    );

    let o = run(
        &["--refusals", "-I", dir.to_str().expect("utf8"), "cursed.h"],
        &cache,
        true,
    );
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert!(o.stdout.contains("union-active-member"), "{}", o.stdout);
    assert!(o.stdout.contains("long-double"), "{}", o.stdout);
    // Every refusal carries a note saying what to do instead.
    assert!(o.stdout.contains("inline C block"), "{}", o.stdout);

    // …and the siblings imported.
    let d = run(
        &["--dump", "-I", dir.to_str().expect("utf8"), "cursed.h"],
        &cache,
        true,
    );
    assert!(
        d.stdout.contains("fn fine_before : int(int) [external] ok"),
        "{}",
        d.stdout
    );
    assert!(
        d.stdout.contains("fn fine_after : int(int) [external] ok"),
        "{}",
        d.stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cache);
}

/// Cross-compilation is the reason widths live in the artifact: the
/// same header imports differently per target, and the two answers are
/// cached apart.
#[test]
fn the_same_header_imports_differently_per_target() {
    let dir = scratch("target");
    let cache = scratch("target-cache");
    write_header(&dir, "w.h", "long width(long x);\n");
    let inc = dir.to_str().expect("utf8");

    let lin = run(
        &[
            "--dump",
            "-I",
            inc,
            "--target",
            "x86_64-unknown-linux-gnu",
            "w.h",
        ],
        &cache,
        true,
    );
    let win = run(
        &[
            "--dump",
            "-I",
            inc,
            "--target",
            "x86_64-pc-windows-msvc",
            "w.h",
        ],
        &cache,
        true,
    );
    assert!(lin.stdout.contains("long=64"), "{}", lin.stdout);
    assert!(
        win.stdout.contains("long=32"),
        "LLP64: `long` is 32 bits on windows. {}",
        win.stdout
    );
    assert_ne!(lin.stdout, win.stdout);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cache);
}

/// An unknown target is refused rather than guessed at. Guessing a
/// width is how a cross-compiled program silently misbehaves.
#[test]
fn an_unknown_target_is_refused() {
    let dir = scratch("badtarget");
    let cache = scratch("badtarget-cache");
    write_header(&dir, "h.h", "int f(int x);\n");

    let o = run(
        &[
            "-I",
            dir.to_str().expect("utf8"),
            "--target",
            "pdp11-unknown-unix",
            "h.h",
        ],
        &cache,
        true,
    );
    assert_eq!(o.code, 1);
    assert!(o.stderr.contains("will not guess"), "{}", o.stderr);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cache);
}
