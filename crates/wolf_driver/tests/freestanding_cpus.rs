//! wolf-lang#1111 (spec/04 `[abi.target.none.ambient.storage]`,
//! `[abi.target.none.ambient.concurrent]`, `[abi.target.none.hooks]` (f)): a
//! freestanding program can give each processor its own ambient region, so two
//! processors can allocate at once.
//!
//! The kernel (`fixtures/freestanding_cpus/`) runs on two processors of one
//! user-space process with no libc: processor 0 is the thread `_start` runs
//! on, processor 1 a second thread made with a raw `clone` (`cpus.S`), each
//! with its own 64-byte record as its GS base (`arch_prctl(ARCH_SET_GS)`), the
//! way a kernel addresses its per-processor record. `slot.S` supplies the
//! program's storage pair, `wolf_ambient_load` and `wolf_ambient_store`, each
//! one instruction on the record's word. The allocator hook is lock-free and
//! poisons every freed block, so data left in a freed region reads wrong.
//! Both tiers build the kernel; it is linked `-nostdlib` with the runtime
//! archive written beside it and run with a 60 s deadline.
//!
//! Rows (x86-64 linux), each on both tiers:
//!
//! - the issue's witness through `wolf_rt_ambient_get`/`_set`: processor 0
//!   publishes A, processor 1 publishes B, each reads its own;
//! - two regions allocating at once (List, Map, interpolation, closure):
//!   each region's ledger is that of the same work done alone, and the reads
//!   come back whole;
//! - one region ending (its blocks poisoned) while the other allocates;
//! - a thread of control moving to the other processor at a switch with its
//!   region open, while another thread allocates in its own region;
//! - the same thread moving in the middle of its region's entry (parked inside
//!   the program's load, as a timer preemption would park it);
//! - the four allocating rows with the whole process on one CPU;
//! - the control: the same kernel without `slot.S` keeps the runtime's single
//!   word, so the witness fails as the issue measured it.
//!
//! On every unix host: the archive defines the storage pair weak and imports
//! only the hook list.
//!
//! A toolchain without the x86_64-unknown-none target cannot build the
//! archive: the rows then SKIP LOUDLY, and fail under WOLF_RT_NONE_REQUIRE=1,
//! which every CI job sets. A failed build FAILS here, whatever its exit
//! status.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use wolf_backend::target::FREESTANDING as TARGET;

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

/// The freestanding runtime archive, built fresh once per test binary by
/// `cargo xtask rt-none`. `None` is the loud skip: no x86_64-unknown-none
/// target for this toolchain.
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
                "SKIP freestanding_cpus: no {TARGET} target for this toolchain, so no \
                 libwolf_rt_none.a ({})",
                err.trim()
            );
            return None;
        }
        let lib = Path::new(wolf())
            .parent()
            .and_then(Path::parent)
            .expect("target dir")
            .join(TARGET)
            .join("release/libwolf_rt_none.a");
        assert!(
            lib.is_file(),
            "rt-none built, but {} is missing",
            lib.display()
        );
        Some(lib)
    })
    .as_deref()
}

/// (undefined, defined global, defined weak) symbols of one ELF object.
fn symbols_of(bytes: &[u8], what: &str) -> (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>) {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(bytes).unwrap_or_else(|e| panic!("{what}: parse: {e}"));
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

/// On every unix host: the archive defines `wolf_ambient_load` and
/// `wolf_ambient_store` WEAK (a program's own win at the link), still
/// defines `wolf_rt_ambient_get`/`_set` globally, and leaves exactly the hook
/// list undefined.
#[cfg(unix)]
#[test]
fn the_archive_defines_the_storage_pair_weak() {
    let Some(lib) = none_rt() else { return };
    let bytes = std::fs::read(lib).expect("read archive");
    let ar = object::read::archive::ArchiveFile::parse(&*bytes).expect("parse archive");
    let (mut undef, mut global, mut weak) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for m in ar.members() {
        let m = m.expect("member");
        let name = String::from_utf8_lossy(m.name()).into_owned();
        if !name.ends_with(".o") {
            continue;
        }
        let (u, g, w) = symbols_of(m.data(&*bytes).expect("member data"), &name);
        undef.extend(u);
        global.extend(g);
        weak.extend(w);
    }
    for s in ["wolf_ambient_load", "wolf_ambient_store"] {
        assert!(
            weak.contains(s) && !global.contains(s),
            "libwolf_rt_none.a defines `{s}` weak and not global: weak={} global={}",
            weak.contains(s),
            global.contains(s)
        );
    }
    for s in ["wolf_rt_ambient_get", "wolf_rt_ambient_set"] {
        assert!(
            global.contains(s),
            "libwolf_rt_none.a defines `{s}` globally"
        );
    }
    let unresolved: BTreeSet<&str> = undef
        .iter()
        .filter(|s| !global.contains(*s) && !weak.contains(*s))
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = ["wolf_trap", "wolf_alloc", "wolf_free"]
        .into_iter()
        .collect();
    assert_eq!(
        unresolved, want,
        "the archive imports the hook list and nothing else"
    );
}

/// The kernel's runs, on the x86-64 linux host.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux_x86_64 {
    use super::*;
    use std::process::{Output, Stdio};
    use std::time::{Duration, Instant};

    /// Every scenario's processor 1 has ended and kmain has printed within this.
    const DEADLINE: Duration = Duration::from_secs(60);

    /// A scratch package: the kernel, its assembly, `mode.lu` (MODE and PIN)
    /// and the manifest: `wolf.pkg` (with slot.S) or, for the control,
    /// `wolf_defaults.pkg` (without).
    fn staged(case: &str, mode: u32, pin: bool, pair: bool) -> PathBuf {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("freestanding_cpus")
            .join(case);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        for name in ["kmain_cpus.lu", "ctx.lu", "work.lu", "cpus.S"] {
            std::fs::copy(fixture("freestanding_cpus", name), dir.join(name))
                .expect("copy fixture");
        }
        if pair {
            std::fs::copy(fixture("freestanding_cpus", "slot.S"), dir.join("slot.S"))
                .expect("copy slot.S");
        }
        let manifest = if pair {
            "wolf.pkg"
        } else {
            "wolf_defaults.pkg"
        };
        std::fs::copy(fixture("freestanding_cpus", manifest), dir.join("wolf.pkg"))
            .expect("copy manifest");
        std::fs::write(
            dir.join("mode.lu"),
            format!(
                "// Written by the gate: the scenario, and whether the process runs on one CPU.\n\
                 let MODE: int = {mode}\nlet PIN: bool = {pin}\n"
            ),
        )
        .expect("write mode.lu");
        dir
    }

    /// Build the kernel on `tier`; every object the build wrote (the release
    /// tier may write one per cluster; the listed assembly lands as
    /// `k.<tier>.asm-<stem>.o`).
    fn build(dir: &Path, tier: &str) -> Vec<PathBuf> {
        let o = format!("k.{tier}.o");
        let mut args = vec![
            "build",
            "kmain_cpus.lu",
            "--target",
            TARGET,
            "--emit=obj",
            "-o",
            o.as_str(),
        ];
        if tier == "release" {
            args.push("--release");
        }
        let out = Command::new(wolf())
            .current_dir(dir)
            .args(&args)
            .output()
            .expect("wolf runs");
        let prefix = format!("k.{tier}.");
        let mut objs: Vec<PathBuf> = std::fs::read_dir(dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().unwrap().to_string_lossy();
                n == o || (n.starts_with(&prefix) && n.ends_with(".o"))
            })
            .collect();
        objs.sort();
        assert!(
            out.status.success() && !objs.is_empty(),
            "wolf build kmain_cpus.lu --target {TARGET} --emit=obj ({tier}) in {} must build \
             (exit {:?}): a refusal here is the row failing, never a skip:\n{}",
            dir.display(),
            out.status.code(),
            text(&out.stderr)
        );
        objs
    }

    fn tool(name: &str, args: &[&str]) -> Output {
        Command::new(name)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("`{name}` is part of this gate's host (no skip): {e}"))
    }

    /// Link the boot stub (kw04's `start.S`), the kernel's objects and the
    /// archive beside them; run the result with the deadline. `None` for the
    /// exit status is a run the gate ended at the deadline.
    fn link_and_run(dir: &Path, tier: &str, objs: &[PathBuf]) -> (Option<i32>, String) {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let start = dir.join("start.o");
        let r = tool(
            &cc,
            &[
                "-c",
                fixture("freestanding", "start.S").to_str().unwrap(),
                "-o",
                start.to_str().unwrap(),
            ],
        );
        assert!(r.status.success(), "{cc} -c start.S: {}", text(&r.stderr));
        let exe = dir.join(format!("cpus.{tier}.elf"));
        let mut args: Vec<String> = vec!["-static".into(), "-nostdlib".into(), "-o".into()];
        args.push(exe.to_str().unwrap().into());
        args.push(start.to_str().unwrap().into());
        for o in objs {
            args.push(o.to_str().unwrap().into());
        }
        args.push(
            dir.join(format!("k.{tier}.rt-none.a"))
                .to_str()
                .unwrap()
                .into(),
        );
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let r = match Command::new("ld.lld").args(&argv).output() {
            Ok(r) => r,
            Err(_) => tool("ld", &argv),
        };
        assert!(
            r.status.success(),
            "link {}: {}",
            exe.display(),
            text(&r.stderr)
        );
        let mut child = Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("run {}: {e}", exe.display()));
        let begun = Instant::now();
        let mut stalled = false;
        loop {
            if child.try_wait().expect("try_wait").is_some() {
                break;
            }
            if begun.elapsed() > DEADLINE {
                // The gate's own child, by its handle.
                stalled = true;
                let _ = child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let status = child.wait().expect("wait for the kernel");
        let mut out = String::new();
        if let Some(mut so) = child.stdout.take() {
            use std::io::Read;
            let mut b = Vec::new();
            let _ = so.read_to_end(&mut b);
            out = text(&b);
        }
        (if stalled { None } else { status.code() }, out)
    }

    /// Every `key=value` word of the transcript.
    fn words(out: &str) -> BTreeMap<String, String> {
        out.split_whitespace()
            .filter_map(|w| w.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn num(w: &BTreeMap<String, String>, k: &str, ctx: &str) -> u64 {
        w.get(k)
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("{ctx}: a number for `{k}`"))
    }

    fn flag(w: &BTreeMap<String, String>, k: &str) -> bool {
        w.get(k).map(String::as_str) == Some("true")
    }

    /// Build and run one scenario on one tier; its transcript, after the checks
    /// every run shares: kmain's 33 before the deadline, processor 1 started,
    /// and (with the pin) the process on one CPU.
    fn run(case: &str, mode: u32, pin: bool, pair: bool, tier: &str) -> String {
        let dir = staged(&format!("{case}_{tier}"), mode, pin, pair);
        let objs = build(&dir, tier);
        let (code, out) = link_and_run(&dir, tier, &objs);
        let ctx = format!("{case} ({tier}, mode {mode}, pin {pin})");
        assert_eq!(
            code,
            Some(33),
            "{ctx}: kmain returns 33 within {DEADLINE:?} (None: ended at the deadline); \
             transcript:\n{out}"
        );
        let w = words(&out);
        assert!(flag(&w, "cpu1"), "{ctx}: processor 1 started:\n{out}");
        if pin {
            assert!(
                flag(&w, "pinned"),
                "{ctx}: the process is on one CPU:\n{out}"
            );
        }
        out
    }

    /// What every allocating scenario must show once the two processors have
    /// their own storage: the books balanced, every live region gone, no bad
    /// free, and both processors' words back at null.
    fn after_checks(w: &BTreeMap<String, String>, ctx: &str, out: &str) -> (u64, u64) {
        assert!(
            flag(w, "books_back"),
            "{ctx}: every block back through wolf_free:\n{out}"
        );
        assert!(
            flag(w, "live_back"),
            "{ctx}: live_region_bytes back where it began:\n{out}"
        );
        assert_eq!(
            num(w, "bad", ctx),
            0,
            "{ctx}: no free named a block it was not given:\n{out}"
        );
        assert_eq!(
            num(w, "w0", ctx),
            0,
            "{ctx}: processor 0's word is null at the end:\n{out}"
        );
        assert_eq!(
            num(w, "w1", ctx),
            0,
            "{ctx}: processor 1's word is null at the end:\n{out}"
        );
        let solo = num(w, "solo", ctx);
        assert!(
            solo >= 64_000,
            "{ctx}: the solo ledger is real work: {solo}\n{out}"
        );
        (solo, num(w, "solo_sum", ctx))
    }

    fn check_simultaneous(out: &str, ctx: &str) {
        let w = words(out);
        let (solo, sum) = after_checks(&w, ctx, out);
        let (ra, rb) = (num(&w, "ra", ctx), num(&w, "rb", ctx));
        assert!(
            ra == solo && rb == solo,
            "{ctx}: each region's ledger is that of the same work alone ({solo}); got ra {ra}, \
             rb {rb}\n{out}"
        );
        assert!(
            num(&w, "sa", ctx) == sum && num(&w, "sb", ctx) == sum,
            "{ctx}: both read their work back whole ({sum}):\n{out}"
        );
    }

    fn check_one_ends(out: &str, ctx: &str) {
        let w = words(out);
        let (solo, sum) = after_checks(&w, ctx, out);
        let mid = num(&w, "ra_mid", ctx);
        assert_eq!(
            num(&w, "keep_sum", ctx),
            1_999_000,
            "{ctx}: processor 0's List reads back whole after rb is freed and poisoned:\n{out}"
        );
        assert_eq!(
            num(&w, "rb", ctx),
            mid,
            "{ctx}: the two Lists cost the same:\n{out}"
        );
        assert_eq!(
            num(&w, "ra", ctx),
            mid + solo,
            "{ctx}: ra grew by the work done while rb ended:\n{out}"
        );
        assert_eq!(
            num(&w, "sa", ctx),
            sum,
            "{ctx}: the work read back whole:\n{out}"
        );
    }

    fn check_follow(out: &str, ctx: &str, inside: bool) {
        let w = words(out);
        let (solo, sum) = after_checks(&w, ctx, out);
        if inside {
            assert!(
                flag(&w, "fired"),
                "{ctx}: the region entry loaded through the program's pair, which parked the \
                 thread inside it:\n{out}"
            );
        }
        assert_eq!(
            num(&w, "t_cpu0", ctx),
            0,
            "{ctx}: T began on processor 0:\n{out}"
        );
        assert_eq!(
            num(&w, "t_cpu1", ctx),
            1,
            "{ctx}: T went on on processor 1:\n{out}"
        );
        let base = if inside {
            0
        } else {
            let l64 = num(&w, "l64", ctx);
            assert_eq!(
                num(&w, "rt1", ctx),
                l64,
                "{ctx}: T's first List is in rt:\n{out}"
            );
            assert_eq!(
                num(&w, "sumx", ctx),
                2016,
                "{ctx}: T's first List reads back:\n{out}"
            );
            l64
        };
        assert_eq!(
            num(&w, "rt", ctx),
            base + solo,
            "{ctx}: what T allocated on processor 1 landed in rt:\n{out}"
        );
        assert_eq!(
            num(&w, "ru", ctx),
            solo,
            "{ctx}: U's region holds exactly U's work:\n{out}"
        );
        assert!(
            num(&w, "st", ctx) == sum && num(&w, "su", ctx) == sum,
            "{ctx}: both threads read their work back whole ({sum}):\n{out}"
        );
    }

    fn check(mode: u32, out: &str, ctx: &str) {
        match mode {
            2 => check_simultaneous(out, ctx),
            3 => check_one_ends(out, ctx),
            4 => check_follow(out, ctx, false),
            5 => check_follow(out, ctx, true),
            _ => unreachable!(),
        }
    }

    #[test]
    fn two_processors_keep_their_own_ambient() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let out = run("witness", 1, false, true, tier);
            for line in [
                "PASS CPU-AMBIENT-ISOLATION A retains A",
                "PASS CPU-AMBIENT-ISOLATION B retains B",
            ] {
                assert!(
                    out.lines().any(|l| l == line),
                    "witness ({tier}): `{line}`; transcript:\n{out}"
                );
            }
        }
    }

    #[test]
    fn simultaneous_regions_allocate_independently() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let out = run("simultaneous", 2, false, true, tier);
            check(2, &out, &format!("simultaneous ({tier})"));
        }
    }

    #[test]
    fn one_region_ends_while_the_other_allocates() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let out = run("one_ends", 3, false, true, tier);
            check(3, &out, &format!("one_ends ({tier})"));
        }
    }

    #[test]
    fn a_region_follows_its_thread_across_processors() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let out = run("follow", 4, false, true, tier);
            check(4, &out, &format!("follow ({tier})"));
        }
    }

    #[test]
    fn a_thread_moved_inside_a_runtime_call_keeps_its_region() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let out = run("inside", 5, false, true, tier);
            check(5, &out, &format!("inside ({tier})"));
        }
    }

    #[test]
    fn the_same_rows_on_one_processor() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            for mode in [2, 3, 4, 5] {
                let out = run(&format!("pinned{mode}"), mode, true, true, tier);
                check(mode, &out, &format!("pinned mode {mode} ({tier})"));
            }
        }
    }

    #[test]
    fn without_a_pair_the_processors_share_one_word() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let out = run("control", 1, false, false, tier);
            let line = "FAIL CPU-AMBIENT-ISOLATION A reads B after concurrent CPU publication";
            assert!(
                out.lines().any(|l| l == line),
                "control ({tier}): with the runtime's defaults one word serves both \
                 processors, so the witness fails as measured (the gate reaches the hazard); \
                 transcript:\n{out}"
            );
        }
    }
}
