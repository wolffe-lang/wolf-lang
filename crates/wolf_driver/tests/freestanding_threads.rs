//! s223 (wolf-lang#611, spec/04 `[abi.target.none.ambient]`,
//! `[abi.target.none.hooks]` (e)): the freestanding runtime's ambient
//! region follows a thread of control across a switch, when the
//! program's switch carries it with `wolf_rt_ambient_get` /
//! `wolf_rt_ambient_set`.
//!
//! The witness is pax's px06 shape (`kernel/heapthreads`) without a
//! kernel (`fixtures/freestanding_threads/`): one user-space process,
//! two stacks, a cooperative switch in assembly (`threads.S`), kw12's
//! bump allocator hook in wolf, and two threads of control, each inside
//! its own `region` across the switch. A enters `ra`, yields; B enters
//! `rb`, yields; A builds a `List[int]` of 64 and yields; B leaves `rb`
//! and exits; A sums its List and leaves `ra`. The package lists its
//! assembly, so the roster judges every extern call ([abi.asm.roster]).
//! Rows:
//!
//! 1. both tiers build the kernel: the roster admits the pair as hooks
//!    (no E1306), the object imports it, and the runtime archive is
//!    written beside it; the archive defines the pair and still imports
//!    only `wolf_alloc`, `wolf_free` and `wolf_trap`;
//! 2. (x86-64 linux) linked `-nostdlib` and run, on both tiers: with the
//!    pair the List is charged to `ra` and `rb` holds nothing; without
//!    it (the control, `mode_shared.lu`), the List lands in `rb` — the
//!    proof that the gate's switch reaches the hazard at all, so the
//!    first answer is not vacuous.
//!
//! At trunk ac0ac498 row 1 was red on both tiers: `error[E1306]:
//! `wolf_rt_ambient_get` is called through `extern "c"`, but no assembly
//! source the manifest lists defines it` (exit 2), and the control with
//! the two calls removed printed `region ra 0 bytes, region rb 1008
//! bytes` on both tiers — px06's numbers (kasumi `~/lanes/s223/`).
//! A toolchain without the x86_64-unknown-none target cannot build the
//! archive: the rows then SKIP LOUDLY, and fail under
//! WOLF_RT_NONE_REQUIRE=1, which every CI job sets. A failed build
//! FAILS here, whatever its exit status (wolf-lang#550).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

use wolf_backend::target::{
    ALLOC_HOOKS, AMBIENT_HOOKS, FREESTANDING as TARGET, MEM_HOOKS, NONE_RT_SYMBOLS,
};

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

/// A scratch package: the kernel, the scheduler, the hook, the
/// assembly it lists and its manifest, plus `mode` (`save` or
/// `shared`), which sets `SAVE_AMBIENT`.
fn staged(case: &str, mode: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("freestanding_threads")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mode_file = format!("mode_{mode}.lu");
    for (from, name) in [
        ("freestanding_threads", "kmain_threads.lu"),
        ("freestanding_threads", "sched.lu"),
        ("freestanding_threads", "hooks.lu"),
        ("freestanding_threads", "threads.S"),
        ("freestanding_threads", "wolf.pkg"),
        ("freestanding_threads", mode_file.as_str()),
        ("freestanding", "rt_stub.S"),
    ] {
        std::fs::copy(fixture(from, name), dir.join(name)).expect("copy fixture");
    }
    dir
}

/// The freestanding runtime archive, built fresh once per test binary by
/// `cargo xtask rt-none`. `None` is the loud skip: no
/// x86_64-unknown-none target for this toolchain.
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
                "SKIP freestanding_threads: no {TARGET} target for this toolchain, so no \
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

/// Build `kmain_threads.lu` in `dir` on `tier` with `-o k.<tier>.o`;
/// every object the build wrote (the release tier may write one per
/// cluster; the listed assembly lands as `k.<tier>.asm-<stem>.o`).
fn build_obj(dir: &Path, tier: &str) -> Vec<PathBuf> {
    let o = format!("k.{tier}.o");
    let mut args = vec![
        "build",
        "kmain_threads.lu",
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
        "wolf build kmain_threads.lu --target {TARGET} --emit=obj ({tier}) must build \
         (exit {:?}) — a refusal here is the row failing, never a skip:\n{}",
        out.status.code(),
        text(&out.stderr)
    );
    objs
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

/// Row 1 on every unix host (the listed assembly is assembled with
/// the release tier's clang, as `asm_link.rs`'s rows are).
#[cfg(unix)]
#[test]
fn the_roster_admits_the_pair_and_the_archive_defines_it() {
    let Some(lib) = none_rt() else { return };
    for tier in ["native", "release"] {
        let dir = staged(&format!("row1_{tier}"), "save");
        let objs = build_obj(&dir, tier);
        let mut undef = BTreeSet::new();
        let mut defined = BTreeSet::new();
        for obj in &objs {
            let (u, g, w) = symbols_of(
                &std::fs::read(obj).expect("read"),
                &obj.display().to_string(),
            );
            undef.extend(u);
            defined.extend(g);
            defined.extend(w);
        }
        let undef: BTreeSet<String> = undef.difference(&defined).cloned().collect();
        for h in AMBIENT_HOOKS {
            assert!(
                undef.contains(h),
                "{tier}: the kernel's objects import `{h}`: {undef:?}"
            );
        }
        let allowed: BTreeSet<&str> = ["wolf_trap"]
            .into_iter()
            .chain(MEM_HOOKS)
            .chain(AMBIENT_HOOKS)
            .chain(NONE_RT_SYMBOLS.iter().copied())
            .collect();
        let stray: Vec<&String> = undef
            .iter()
            .filter(|s| !allowed.contains(s.as_str()))
            .collect();
        assert!(
            stray.is_empty(),
            "{tier}: imports outside the hook list: {stray:?}"
        );
        let beside = dir.join(format!("k.{tier}.rt-none.a"));
        assert!(
            std::fs::read(&beside).ok() == std::fs::read(lib).ok(),
            "{tier}: the archive just built is beside the object, {}",
            beside.display()
        );
    }
    // The archive defines the pair, globally, and imports no more than
    // it did.
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
    for h in AMBIENT_HOOKS {
        assert!(
            global.contains(h),
            "libwolf_rt_none.a defines `{h}` globally"
        );
    }
    let unresolved: BTreeSet<&str> = undef
        .iter()
        .filter(|s| !global.contains(*s) && !weak.contains(*s))
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = ["wolf_trap"].into_iter().chain(ALLOC_HOOKS).collect();
    assert_eq!(
        unresolved, want,
        "the archive imports the hook list and nothing else"
    );
}

/// Row 2: linked with no libc, run, on the x86-64 linux host.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux_x86_64 {
    use super::*;

    fn tool(name: &str, args: &[&str]) -> Output {
        Command::new(name)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("`{name}` is part of this gate's host (no skip): {e}"))
    }

    /// Link the boot stub (kw04's `start.S`, not listed: the boot code
    /// links the kernel), the kernel's objects (its listed assembly
    /// among them) and the archive; run it.
    fn link_and_run(dir: &Path, tier: &str, objs: &[PathBuf]) -> Output {
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
        let exe = dir.join("threads.elf");
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
        tool(exe.to_str().unwrap(), &[])
    }

    /// `(ra bytes, rb bytes)` from the kernel's transcript, which must
    /// otherwise be the fixed text.
    fn ledgers(out: &str, tier: &str, mode: &str) -> (u64, u64) {
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3, "{tier} {mode}: three lines:\n{out}");
        let l = lines[0];
        let num = |after: &str| -> u64 {
            let i = l
                .find(after)
                .unwrap_or_else(|| panic!("{tier} {mode}: `{after}` in {l}"));
            l[i + after.len()..]
                .split(' ')
                .next()
                .and_then(|w| w.parse().ok())
                .unwrap_or_else(|| panic!("{tier} {mode}: a number after `{after}` in {l}"))
        };
        assert!(
            l.starts_with("threads: A list 64, region ra "),
            "{tier} {mode}: {l}"
        );
        assert!(
            l.ends_with(", sum 2016, both exited"),
            "{tier} {mode}: A's List read back whole after B's exit, both exited: {l}"
        );
        assert_eq!(
            lines[2], "books: every block back=true",
            "{tier} {mode}: every block back through wolf_free"
        );
        (num("region ra "), num("region rb "))
    }

    #[test]
    fn the_list_lands_in_its_own_threads_region_on_both_tiers() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let dir = staged(&format!("row2_save_{tier}"), "save");
            let objs = build_obj(&dir, tier);
            let out = link_and_run(&dir, tier, &objs);
            let s = text(&out.stdout);
            assert_eq!(out.status.code(), Some(33), "{tier}: kmain's result:\n{s}");
            let (ra, rb) = ledgers(&s, tier, "save");
            assert!(
                ra >= 512 && rb == 0,
                "{tier}: with the pair, A's 64-int List is charged to ra (>= 512 bytes) and rb \
                 holds nothing; got ra {ra}, rb {rb} (wolf-lang#611)"
            );
            assert_eq!(
                s.lines().nth(1),
                Some("ambient: the List landed in ra"),
                "{tier}"
            );
        }
    }

    #[test]
    fn without_the_pair_the_threads_share_one_slot() {
        let Some(_) = none_rt() else { return };
        for tier in ["native", "release"] {
            let dir = staged(&format!("row2_shared_{tier}"), "shared");
            let objs = build_obj(&dir, tier);
            let out = link_and_run(&dir, tier, &objs);
            let s = text(&out.stdout);
            assert_eq!(out.status.code(), Some(33), "{tier}: kmain's result:\n{s}");
            let (ra, rb) = ledgers(&s, tier, "shared");
            assert!(
                ra == 0 && rb >= 512,
                "{tier}: the control's switch leaves the one slot alone, so A allocates in \
                 B's rb (the gate reaches the hazard); got ra {ra}, rb {rb}"
            );
            assert_eq!(
                s.lines().nth(1),
                Some("ambient: the List landed in rb"),
                "{tier}"
            );
        }
    }
}
