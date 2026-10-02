//! Reproducible release builds (#503, s195): the release tier's LLVM IR
//! is a function of the source and the compiler, never of the process
//! that ran it. Three builds of one fixed corpus set, each in its own
//! process, must agree byte for byte — and so must the linked binaries.
//!
//! The defect this gate was seen red on: the mid-end walked a natural
//! loop's block set (a std `HashSet`, so a per-process `RandomState`
//! order) to collect the bounds guards its versioner plans around, and
//! the guard chain was emitted in that order. Two builds of one program
//! with one compiler then differed in the guard order, the bodies hashed
//! differently, and dedup, clustering and the whole LLVM text followed:
//! three builds of lobo gave three binaries. The witness below is a
//! loop whose versioner mints four relation guards (one per container
//! len); at trunk `abf4e5cf` eight builds of it gave six distinct
//! modules, and the two corpus entries named here differed in three.
//!
//! The release tier needs clang. Where it is missing the build fails
//! with the toolchain message and this gate skips LOUDLY (the
//! `bench_hatches` rule): any other failure is a failure.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn out_dir() -> PathBuf {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release-determinism-tests");
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

/// The fixed corpus set. The first two are the entries a sweep of the
/// corpus at `abf4e5cf` found differing across three release builds:
/// 861 files, 515 the release tier built standalone, 513 identical,
/// 2 not (346 refused standalone — members, comptime refusals, the
/// one-lane programs). The rest are controls that were identical then
/// and must stay so: two kernels the versioner works on, the
/// multi-module entry, and hello.
const CORPUS_SET: &[&str] = &[
    "memory/carried_quotient_pair.lu",
    "memory/push_grow_guarded_index.lu",
    "kernels/hot_header.lu",
    "kernels/guarded_stencil.lu",
    "conc/proc_cross_module/main.lu",
    "hello.lu",
];

/// Four containers read in one loop under one invariant trip count:
/// four bounds guards, four relation plans, one guard chain whose
/// order is the versioner's walk order over the loop's blocks.
const WITNESS: &str = "\
//! member: false

fn mix(a: List[int], b: List[int], c: List[int], d: List[int], n: int) -> int {
    var i = 0
    var acc = 0
    while i < n {
        acc = (acc + a[i] * b[i] + c[i] * d[i]) & 65535
        i = i + 1
    }
    acc
}

fn main() -> !int {
    var a = List[int]()
    var b = List[int]()
    var c = List[int]()
    var d = List[int]()
    var i = 0
    while i < 64 {
        (mut a).push(i)
        (mut b).push(63 - i)
        (mut c).push(i * 2)
        (mut d).push(1)
        i = i + 1
    }
    var n = a.len
    if b.len < n { n = b.len }
    if c.len < n { n = c.len }
    if d.len < n { n = d.len }
    if mix(a, b, c, d, n) == 45696 { 0 } else { 1 }
}
";

/// The binary witness links `libwolf_rt.a` from next to the `wolf`
/// binary; `cargo test` alone does not build the staticlib on a fresh
/// target (the `release_native` rule). Build it once.
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

fn witness_path() -> PathBuf {
    let p = out_dir().join("four_lists.lu");
    std::fs::write(&p, WITNESS).expect("write witness");
    p
}

/// One release build in a fresh process, `--no-cache` so the second
/// and third builds cannot be served the first one's output. `None`
/// means the release tier is unavailable here, said out loud.
fn build(src: &Path, out: &Path, emit: Option<&str>) -> Option<Vec<u8>> {
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(src)
        .arg("-o")
        .arg(out)
        .arg("--release")
        .arg("--no-cache");
    if let Some(e) = emit {
        cmd.arg(format!("--emit={e}"));
    }
    let res = cmd.output().expect("wolf runs");
    if !res.status.success() {
        let err = String::from_utf8_lossy(&res.stderr);
        assert!(
            err.contains("release tier requires clang") || err.contains("targets linux/x86-64"),
            "release build of {} failed for a reason that is not a missing toolchain:\n{err}",
            src.display()
        );
        eprintln!(
            "release_determinism: release tier unavailable — skipped {}",
            src.display()
        );
        return None;
    }
    let bytes = std::fs::read(out).expect("read the output");
    assert!(
        !bytes.is_empty(),
        "{} built to an empty file",
        out.display()
    );
    Some(bytes)
}

/// Three builds, three processes, one answer. Names the entry and the
/// first differing byte offset on a miss, with the three digests.
fn assert_three_identical(src: &Path, tag: &str, emit: Option<&str>) {
    let ext = emit.map(|_| "ll").unwrap_or("bin");
    let mut outs: Vec<Vec<u8>> = Vec::new();
    for i in 1..=3 {
        let out = out_dir().join(format!("{tag}-{i}.{ext}"));
        let Some(b) = build(src, &out, emit) else {
            return;
        };
        outs.push(b);
    }
    let first = &outs[0];
    for (i, o) in outs.iter().enumerate().skip(1) {
        if o != first {
            let off = first
                .iter()
                .zip(o.iter())
                .position(|(a, b)| a != b)
                .unwrap_or(first.len().min(o.len()));
            panic!(
                "{}: release {} build {} differs from build 1 at byte {off} ({} vs {} bytes) — the \
                 release tier is not reproducible (#503); digests {}",
                src.display(),
                ext,
                i + 1,
                first.len(),
                o.len(),
                outs.iter().map(|b| digest(b)).collect::<Vec<_>>().join(" "),
            );
        }
    }
}

/// A short, stable digest for the failure message (FNV-1a over the
/// bytes; the test needs a name for each build, not a hash function).
fn digest(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[test]
fn corpus_set_release_ir_is_identical_across_three_builds() {
    for entry in CORPUS_SET {
        let src = corpus().join(entry);
        assert!(src.is_file(), "corpus entry missing: {}", src.display());
        let tag = entry.replace(['/', '.'], "_");
        assert_three_identical(&src, &tag, Some("llvm-ir"));
    }
}

#[test]
fn four_list_witness_release_ir_is_identical_across_three_builds() {
    assert_three_identical(&witness_path(), "four_lists", Some("llvm-ir"));
}

#[test]
fn four_list_witness_binary_is_identical_across_three_builds() {
    ensure_rt_staticlib();
    assert_three_identical(&witness_path(), "four_lists_bin", None);
}
