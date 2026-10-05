//! kw12 (STATUS #31 K8(b), spec/04 `[abi.target.none.alloc]`,
//! `[abi.target.none.hooks]`): the allocating constructs on
//! `x86_64-unknown-none`, against a program-supplied allocator hook,
//! through the `no_std` runtime archive `libwolf_rt_none.a`.
//!
//! The witness is one report built twice (`fixtures/freestanding_alloc/`):
//! `report.lu` builds five lines that allocate — a `List`, interpolation
//! under a sweep of format specs, a `Map`, a capturing closure, a
//! `region` — and `host_main.lu` prints them through the hosted runtime,
//! while `kmain_alloc.lu` writes them to the port from a kernel whose
//! allocator (`hooks.lu`) is a bump allocator written in wolf over
//! `heap.S`'s arena. Rows:
//!
//! 1. both tiers build the kernel; its object imports only the hook list,
//!    the runtime symbols the archive defines, and its own externs; the
//!    driver writes the archive beside it (and none beside a kernel that
//!    allocates nothing);
//! 2. the archive, read member by member, leaves exactly `wolf_alloc`,
//!    `wolf_free` and `wolf_trap` undefined, defines every runtime
//!    symbol the target admits (each one the hosted runtime's), and
//!    defines the C memory functions WEAK;
//! 3. (x86-64 linux) linked `ld.lld -static -nostdlib` with the gate's
//!    boot stub, the kernel prints the hosted build's bytes exactly, then
//!    proves a region gave every block back through `wolf_free`, and
//!    exits 33 — on both tiers; the linked image has no SSE register;
//! 4. (x86-64 linux) a region budget breached inside the runtime reaches
//!    the program's `wolf_trap` as `alloc-contract` (kind 9), site-less.
//!
//! At trunk a3465f87 every row was red: each kernel was refused, exit 4,
//! "`List` allocates, and target x86_64-unknown-none has no allocator"
//! (kasumi `~/lanes/kw12/evidence/`). A toolchain without the
//! x86_64-unknown-none target cannot build the archive: rows 1–4 then
//! SKIP LOUDLY, and fail under WOLF_RT_NONE_REQUIRE=1, which every CI job
//! sets.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

use wolf_backend::target::{ALLOC_HOOKS, FREESTANDING as TARGET, MEM_HOOKS, NONE_RT_SYMBOLS};

/// The report's lines, as the hosted runtime prints them.
const REPORT: &str = "list n=9 sum=385 last=100\n\
specs [     385] [385   ] [00000385] [-00385] [+385] [181] [181] [110000001] [601] [**ab***] [  true] [é] [ é]\n\
map wolf=3 pax=2 zzz=99 n=2\n\
closure 42\n\
region total=499500 tag-len=9\n";

/// What only the kernel prints after the report.
const KERNEL_TAIL: &str = "in scratch 5000\n\
scratch returned every block=true\n\
hooks frees>0=true frees<allocs=true bad=0\n";

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn fixture(dir: &str, name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// A fresh scratch directory holding `files` (D32: the `.lu` files of a
/// directory are one module, so each program gets its own).
fn staged(case: &str, files: &[&str]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("freestanding_alloc")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for f in files {
        std::fs::copy(fixture("freestanding_alloc", f), dir.join(f)).expect("copy fixture");
    }
    dir
}

fn wolf_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(wolf())
        .current_dir(dir)
        .args(args)
        .output()
        .expect("wolf runs")
}

/// The freestanding runtime archive, built fresh once per test binary by
/// `cargo xtask rt-none` (a stale archive would test old code). `None`
/// is the loud skip: this toolchain has no x86_64-unknown-none target.
fn none_rt() -> Option<&'static Path> {
    static LIB: OnceLock<Option<PathBuf>> = OnceLock::new();
    LIB.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let out = Command::new(env!("CARGO"))
            .current_dir(&root)
            .args(["xtask", "rt-none"])
            .output()
            .expect("cargo xtask rt-none runs");
        let err = text(&out.stderr);
        assert!(
            out.status.success(),
            "cargo xtask rt-none failed (a skip is never a failure; this is):\n{err}"
        );
        if err.contains("rt-none: SKIP") {
            assert!(
                std::env::var("WOLF_RT_NONE_REQUIRE").as_deref() != Ok("1"),
                "WOLF_RT_NONE_REQUIRE=1 and the archive was skipped:\n{err}"
            );
            eprintln!(
                "SKIP freestanding_alloc: no {TARGET} target for this toolchain, so no \
                 libwolf_rt_none.a ({})",
                err.trim()
            );
            return None;
        }
        // Where the driver looks from a checkout's `wolf`.
        let lib = Path::new(wolf())
            .parent()
            .and_then(Path::parent)
            .expect("target dir")
            .join(TARGET)
            .join("release/libwolf_rt_none.a");
        assert!(
            lib.is_file(),
            "rt-none built, but {} is missing:\n{err}",
            lib.display()
        );
        Some(lib)
    })
    .as_deref()
}

/// Build the `kmain*.lu` staged in `dir` (with its module mates) for the
/// target on `tier` with `-o k.<tier>.o`; any failure fails the row. The
/// release tier's whole-program phase may emit one object per cluster
/// (`k.<tier>.<unit>.o`), so the result is every object the build wrote;
/// the archive, when there is one, is `k.<tier>.rt-none.a` either way.
fn build_obj(dir: &Path, tier: &str) -> Vec<PathBuf> {
    let obj = dir.join(format!("k.{tier}.o"));
    let o = obj.to_str().unwrap().to_string();
    let entry = std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("kmain") && n.ends_with(".lu"))
        .expect("a kmain_*.lu in the dir");
    let mut args = vec![
        "build",
        entry.as_str(),
        "--target",
        TARGET,
        "--emit=obj",
        "-o",
        o.as_str(),
    ];
    if tier == "release" {
        args.push("--release");
    }
    let out = wolf_in(dir, &args);
    let prefix = format!("k.{tier}.");
    let mut objs: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n == format!("k.{tier}.o") || (n.starts_with(&prefix) && n.ends_with(".o"))
        })
        .collect();
    objs.sort();
    assert!(
        out.status.success() && !objs.is_empty(),
        "wolf build {entry} --target {TARGET} --emit=obj ({tier}) must build (exit {:?}) — \
         a refusal here is the row failing, never a skip:\n{}",
        out.status.code(),
        text(&out.stderr)
    );
    objs
}

/// The archive the build wrote beside `-o k.<tier>.o`.
fn archive_beside(dir: &Path, tier: &str) -> PathBuf {
    dir.join(format!("k.{tier}.rt-none.a"))
}

/// (undefined, defined global) symbols of one ELF object.
fn symbols_of(bytes: &[u8], what: &str) -> (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>) {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(bytes).unwrap_or_else(|e| panic!("{what}: parse: {e}"));
    assert_eq!(file.format(), object::BinaryFormat::Elf, "{what}: ELF");
    assert_eq!(
        file.architecture(),
        object::Architecture::X86_64,
        "{what}: x86-64"
    );
    let mut undef = BTreeSet::new();
    let mut global = BTreeSet::new();
    let mut weak = BTreeSet::new();
    for s in file.symbols() {
        let Ok(name) = s.name() else { continue };
        if name.is_empty() {
            continue;
        }
        if s.is_undefined() {
            undef.insert(name.to_string());
        } else if s.is_weak() {
            weak.insert(name.to_string());
        } else if s.is_global() {
            global.insert(name.to_string());
        }
    }
    (undef, global, weak)
}

/// The imports and globals of one build's objects taken together: a
/// symbol one cluster defines and another calls is no import.
fn object_symbols(objs: &[PathBuf]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut undef = BTreeSet::new();
    let mut defined = BTreeSet::new();
    for obj in objs {
        let bytes = std::fs::read(obj).expect("read object");
        let (u, g, w) = symbols_of(&bytes, &obj.display().to_string());
        undef.extend(u);
        defined.extend(g);
        defined.extend(w);
    }
    let undef = undef.difference(&defined).cloned().collect();
    (undef, defined)
}

/// Row 1 on every host: the native tier emits the target from any host,
/// and the release tier wherever it opens.
#[test]
fn allocating_kernels_build_and_import_only_the_runtime_and_the_hooks() {
    // A kernel that allocates nothing gets no archive (and needs none).
    let plain = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("freestanding_alloc")
        .join("plain");
    let _ = std::fs::remove_dir_all(&plain);
    std::fs::create_dir_all(&plain).expect("mkdir");
    std::fs::copy(fixture("freestanding", "kmain.lu"), plain.join("kmain.lu")).expect("copy");
    let objs = build_obj(&plain, "native");
    assert!(
        !archive_beside(&plain, "native").exists(),
        "an object that allocates nothing gets no runtime archive"
    );
    assert!(
        object_symbols(&objs)
            .0
            .iter()
            .all(|s| !s.starts_with("__wolf_rt_")),
        "and imports no runtime symbol"
    );

    let Some(lib) = none_rt() else { return };
    let fresh = std::fs::read(lib).expect("read archive");
    let tiers: &[&str] = if cfg!(windows) {
        &["native"]
    } else {
        &["native", "release"]
    };
    for &tier in tiers {
        for (case, files, externs) in [
            (
                "alloc",
                &["report.lu", "hooks.lu", "kmain_alloc.lu"][..],
                &["kw_outb", "kw_heap"][..],
            ),
            ("cap", &["hooks.lu", "kmain_cap.lu"][..], &["kw_heap"][..]),
        ] {
            let dir = staged(&format!("row1_{case}_{tier}"), files);
            let objs = build_obj(&dir, tier);
            let (undef, global) = object_symbols(&objs);
            let allowed: BTreeSet<&str> = ["wolf_trap"]
                .into_iter()
                .chain(MEM_HOOKS)
                .chain(NONE_RT_SYMBOLS.iter().copied())
                .chain(externs.iter().copied())
                .collect();
            let stray: Vec<&String> = undef
                .iter()
                .filter(|s| !allowed.contains(s.as_str()))
                .collect();
            assert!(
                stray.is_empty(),
                "{case} ({tier}): imports outside the hook list, the freestanding runtime \
                 and the program's externs: {stray:?} (all: {undef:?})"
            );
            assert!(
                undef.iter().any(|s| s == "__wolf_rt_list_new")
                    && undef.iter().any(|s| s == "__wolf_rt_region_new"),
                "{case} ({tier}): the kernel calls the runtime's list and region: {undef:?}"
            );
            for g in ["kmain", "wolf_alloc", "wolf_free"] {
                assert!(
                    global.contains(g),
                    "{case} ({tier}): `{g}` is a global under its own name: {global:?}"
                );
            }
            let beside = archive_beside(&dir, tier);
            let copied = std::fs::read(&beside).unwrap_or_else(|e| {
                panic!(
                    "{case} ({tier}): the archive beside the object, {}: {e}",
                    beside.display()
                )
            });
            assert!(
                copied == fresh,
                "{case} ({tier}): the archive beside the object is the one just built"
            );
        }
    }
}

/// Row 2 on every host: the archive's own imports are the hook list.
#[test]
fn the_runtime_archive_leaves_only_the_hooks_undefined() {
    let Some(lib) = none_rt() else { return };
    let bytes = std::fs::read(lib).expect("read archive");
    let ar = object::read::archive::ArchiveFile::parse(&*bytes).expect("parse archive");
    let mut undef = BTreeSet::new();
    let mut global = BTreeSet::new();
    let mut weak = BTreeSet::new();
    let mut members = 0;
    for m in ar.members() {
        let m = m.expect("member");
        let name = String::from_utf8_lossy(m.name()).into_owned();
        if !name.ends_with(".o") {
            continue;
        }
        let data = m.data(&*bytes).expect("member data");
        let (u, g, w) = symbols_of(data, &name);
        undef.extend(u);
        global.extend(g);
        weak.extend(w);
        members += 1;
    }
    assert!(members > 0, "the archive has object members");
    let unresolved: BTreeSet<&str> = undef
        .iter()
        .filter(|s| !global.contains(*s) && !weak.contains(*s))
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = ["wolf_trap"].into_iter().chain(ALLOC_HOOKS).collect();
    assert_eq!(
        unresolved, want,
        "libwolf_rt_none.a imports the hook list and nothing else \
         ([abi.target.none.hooks]); {members} members"
    );
    for s in NONE_RT_SYMBOLS {
        assert!(global.contains(*s), "the archive defines `{s}`");
        assert!(
            wolf_codegen_clif::RT_SYMBOLS.iter().any(|(n, ..)| n == s),
            "`{s}` is a hosted runtime symbol (RT_SYMBOLS): one contract, two archives"
        );
    }
    for m in MEM_HOOKS {
        assert!(
            weak.contains(m) && !global.contains(m),
            "`{m}` is defined WEAK, so a program's own wins at the link: weak={}",
            weak.contains(m)
        );
    }
}

/// Rows 3–4: linked with no libc, run, on the x86-64 linux host.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux_x86_64 {
    use super::*;

    fn tool(name: &str, args: &[&str]) -> Output {
        Command::new(name)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("`{name}` is part of this gate's host (no skip): {e}"))
    }

    /// Assemble the boot stub, the hook stub (kw04's) and the arena.
    fn stubs(dir: &Path) -> Vec<PathBuf> {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let mut out = Vec::new();
        for (d, s) in [
            ("freestanding", "start.S"),
            ("freestanding", "rt_stub.S"),
            ("freestanding_alloc", "heap.S"),
        ] {
            let o = dir.join(s.replace(".S", ".o"));
            let r = tool(
                &cc,
                &[
                    "-c",
                    fixture(d, s).to_str().unwrap(),
                    "-o",
                    o.to_str().unwrap(),
                ],
            );
            assert!(r.status.success(), "{cc} -c {s}: {}", text(&r.stderr));
            out.push(o);
        }
        out
    }

    /// Link with no libc and no hosted runtime: the stubs, the kernel and
    /// the freestanding runtime archive beside it.
    fn link_and_run(dir: &Path, tier: &str, objs: &[PathBuf]) -> Output {
        let exe = dir.join("kernel.elf");
        let mut args: Vec<String> = vec!["-static".into(), "-nostdlib".into(), "-o".into()];
        args.push(exe.to_str().unwrap().into());
        for s in stubs(dir) {
            args.push(s.to_str().unwrap().into());
        }
        for o in objs {
            args.push(o.to_str().unwrap().into());
        }
        args.push(archive_beside(dir, tier).to_str().unwrap().into());
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let r = match Command::new("ld.lld").args(&argv).output() {
            Ok(r) => r,
            Err(_) => tool("ld", &argv),
        };
        assert!(
            r.status.success(),
            "link {} with no libc: {}",
            exe.display(),
            text(&r.stderr)
        );
        // No SSE anywhere in the image: the kernel's code, the runtime's,
        // and every compiler_builtins member the link pulled in.
        let dis = tool(
            "objdump",
            &["-d", "--no-show-raw-insn", exe.to_str().unwrap()],
        );
        assert!(dis.status.success(), "objdump: {}", text(&dis.stderr));
        let dis = text(&dis.stdout);
        assert!(
            dis.contains("<__wolf_rt_list_push>:") && dis.contains("<wolf_alloc>:"),
            "the image carries the runtime and the hook"
        );
        let sse: Vec<&str> = dis
            .lines()
            .filter(|l| l.contains("%xmm") || l.contains("%ymm"))
            .collect();
        assert!(sse.is_empty(), "SSE/AVX registers in the image: {sse:?}");
        tool(exe.to_str().unwrap(), &[])
    }

    /// The hosted half: the same report through the hosted runtime, on
    /// both tiers, is the literal the kernel must match.
    #[test]
    fn the_hosted_build_prints_the_report() {
        let dir = staged("row3_host", &["report.lu", "host_main.lu"]);
        for tier in ["native", "release"] {
            let exe = dir.join(format!("host.{tier}"));
            let mut args = vec!["build", "host_main.lu", "-o", exe.to_str().unwrap()];
            if tier == "release" {
                args.push("--release");
            }
            let out = wolf_in(&dir, &args);
            assert!(out.status.success(), "{tier}: {}", text(&out.stderr));
            let run = tool(exe.to_str().unwrap(), &[]);
            assert_eq!(run.status.code(), Some(0), "{tier}");
            assert_eq!(
                text(&run.stdout),
                REPORT,
                "{tier}: the hosted runtime's bytes"
            );
        }
    }

    #[test]
    fn the_kernel_prints_the_hosted_bytes_on_both_tiers() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let dir = staged(
                &format!("row3_{tier}"),
                &["report.lu", "hooks.lu", "kmain_alloc.lu"],
            );
            let objs = build_obj(&dir, tier);
            let out = link_and_run(&dir, tier, &objs);
            assert_eq!(
                text(&out.stdout),
                format!("{REPORT}{KERNEL_TAIL}"),
                "{tier}: the freestanding runtime's bytes are the hosted runtime's; stderr: {}",
                text(&out.stderr)
            );
            assert_eq!(out.status.code(), Some(33), "{tier}: kmain's result");
        }
    }

    #[test]
    fn a_runtime_fault_reaches_the_trap_hook() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let dir = staged(&format!("row4_{tier}"), &["hooks.lu", "kmain_cap.lu"]);
            let objs = build_obj(&dir, tier);
            let out = link_and_run(&dir, tier, &objs);
            // rt_stub.S's hook prints `TRAP <kind> <file>` and exits with
            // the line: the runtime's trap is site-less (null, 0, 0, 0).
            assert_eq!(
                text(&out.stdout),
                "TRAP 9 \n",
                "{tier}: alloc-contract (9) through wolf_trap"
            );
            assert_eq!(out.status.code(), Some(0), "{tier}: line 0, no site");
        }
    }
}
