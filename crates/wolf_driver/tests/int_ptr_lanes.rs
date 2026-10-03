//! kw06 (STATUS #31 K9(a) and K9(c); wolf-lang#531, #561): integers and
//! pointers on every machine, and foreign memory on both targets.
//!
//! `[mem.prov.expose]`: `int as *T`, `*T as int`, `addr`, `with_addr`,
//! `expose`, `with_exposed`, `is_null` and prefix `*p` lower on the
//! compiling tiers and agree with the checked machine and lupin on round
//! trips into an allocation. `[mem.prov.device]`: an access no
//! allocation owns is UB row L2 on a hosted target (the checked machine
//! and lupin say so) and the platform's on the freestanding one, where
//! the machines refuse the program by name and never answer UB; a
//! kernel that writes `0xb8000` through every spelling builds on both
//! tiers and, on an x86-64 linux host, runs with the address mapped.
//!
//! Measured at trunk 8e36bc1a (kasumi, `~/lanes/kw06/evidence/
//! rows-trunk-8e36bc1a.log`): native and release refused every cast
//! ("raw casts that change the machine shape") and every method ("a
//! method call without an elaborated impl"); all three wolfgang tiers
//! refused `*p = v` ("assignment through this place"); the checked
//! machine read a signed pointee unsigned (#561) and handed back a whole
//! address under a `u8` (`300 as *u8 as u8` exited 44); the kernel did
//! not build on either tier.
//!
//! lupin 0.1.45 (the 0.2.22 pairing) has no `addr`/`with_addr`/
//! `expose`/`with_exposed` and forgets the address of an integer-made
//! pointer no allocation owns (wolf-interp#184), and answers
//! `--target` with a usage error and no record (wolf-interp#182). Each
//! parting is pinned below by version; a newer lupin must answer the
//! compiler's column.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    codes: Vec<String>,
    stdout: String,
    version: String,
    unsupported: String,
    ub_row: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!(
            "{what} record parses ({e}): {}\n{}",
            String::from_utf8_lossy(bytes),
            String::from_utf8_lossy(stderr)
        )
    });
    let mut codes: Vec<String> = rec["diagnostics"]
        .as_array()
        .map(|ds| {
            ds.iter()
                .filter_map(|d| d["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    codes.sort();
    codes.dedup();
    let unsupported = rec["x-unsupported-construct"]
        .as_str()
        .or(rec["x-unsupported"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned());
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        codes,
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        unsupported,
        ub_row: rec["x-ub-row"].as_str().unwrap_or("").to_string(),
    }
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

/// One wolfgang lane with extra flags; `None` is the s59 environment
/// skip for a compiling lane (never an ICE, never the checked lane).
fn lane_with(entry: &Path, flag: &str, extra: &[&str]) -> Option<Obs> {
    ensure_rt_staticlib();
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .args(extra)
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} {extra:?} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, &out.stderr, "the observation"))
}

fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    lane_with(entry, flag, &[])
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

fn lupin_raw(entry: &Path, extra: &[&str]) -> Option<Output> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of kw06's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — only the oracle leg is absent");
        return None;
    };
    Some(
        Command::new(&lupin)
            .arg("conform-run")
            .arg(entry)
            .arg("--json")
            .args(extra)
            .output()
            .expect("lupin runs"),
    )
}

fn lupin_says(entry: &Path) -> Option<Obs> {
    let out = lupin_raw(entry, &[])?;
    Some(parse_obs(&out.stdout, &out.stderr, "lupin's observation"))
}

/// lupin's version, asked of the binary (a run with no record has none).
fn lupin_version() -> Option<String> {
    let lupin = sibling_lupin()?;
    let out = Command::new(&lupin)
        .arg("--version")
        .output()
        .expect("lupin --version");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.split_whitespace().nth(1).map(str::to_string)
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("int_ptr_lanes")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn program(name: &str, src: &str) -> PathBuf {
    let p = scratch(name).join(format!("{name}.lu"));
    std::fs::write(&p, src).expect("write the program");
    p
}

/// What a machine must answer. `named` is a substring of the refusal's
/// construct when the verdict is `unsupported` (refused BY NAME).
#[derive(Clone, Copy)]
struct Want<'a> {
    verdict: &'a str,
    codes: &'a [&'a str],
    stdout: &'a str,
    named: &'a str,
}

const fn runs(stdout: &str) -> Want<'_> {
    Want {
        verdict: "exit(0)",
        codes: &[],
        stdout,
        named: "",
    }
}

const OVERFLOW: Want<'static> = Want {
    verdict: "trap(overflow)",
    codes: &[],
    stdout: "",
    named: "",
};

/// lupin's measured answer where it parts (pre-mirror).
struct Pin<'a> {
    version: &'a str,
    verdict: &'a str,
    stdout: &'a str,
}

fn assert_obs(who: &str, row: &str, obs: &Obs, want: Want<'_>) {
    assert_eq!(
        obs.verdict, want.verdict,
        "{who} on {row}; codes {:?}, stdout {:?}, refusal {:?}",
        obs.codes, obs.stdout, obs.unsupported
    );
    let codes: Vec<String> = want.codes.iter().map(|c| c.to_string()).collect();
    assert_eq!(obs.codes, codes, "{who}'s diagnostics on {row}");
    assert_eq!(obs.stdout, want.stdout, "{who}'s stdout on {row}");
    if want.verdict == "unsupported" {
        assert!(
            obs.unsupported.contains(want.named),
            "{who} on {row} refuses by name ({:?}), never a guess: {:?}",
            want.named,
            obs.unsupported
        );
    }
}

/// The three wolfgang lanes answer `wolfgang`; lupin answers `lupin_want`
/// unless its version is pinned pre-mirror (wolf-interp#184).
fn every_machine(entry: &Path, row: &str, wolfgang: Want<'_>, lupin_pre_mirror: &[Pin<'_>]) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            continue;
        };
        let who = if flag == "--checked" {
            "the CHECKED lane"
        } else {
            flag
        };
        assert_obs(who, row, &obs, wolfgang);
    }
    let Some(lupin) = lupin_says(entry) else {
        return;
    };
    match lupin_pre_mirror.iter().find(|p| p.version == lupin.version) {
        Some(pin) => {
            assert_eq!(
                lupin.verdict, pin.verdict,
                "lupin {} (pre-mirror, wolf-interp#184) on {row}",
                lupin.version
            );
            assert_eq!(
                lupin.stdout, pin.stdout,
                "lupin {}'s stdout (pre-mirror, wolf-interp#184) on {row}",
                lupin.version
            );
        }
        None => assert_obs(
            &format!("lupin {} (the mirror is wolf-interp#184)", lupin.version),
            row,
            &lupin,
            wolfgang,
        ),
    }
}

fn row(rel: &str, wolfgang: Want<'_>, pins: &[Pin<'_>]) {
    every_machine(&corpus(rel), rel, wolfgang, pins);
}

/// lupin 0.1.45 has no provenance methods (wolf-interp#184).
const NO_METHOD_045: &[Pin<'static>] = &[Pin {
    version: "0.1.45",
    verdict: "unsupported",
    stdout: "",
}];

#[test]
fn an_address_round_trips_through_int_into_its_allocation() {
    row("memory/prov_cast_round_trip.lu", runs("3 7\n"), &[]);
}

#[test]
fn expose_and_with_exposed_round_trip() {
    row(
        "memory/prov_expose_round_trip.lu",
        runs("9\n"),
        NO_METHOD_045,
    );
}

#[test]
fn with_addr_keeps_the_receivers_provenance() {
    row(
        "memory/prov_addr_with_addr.lu",
        runs("5 11\n"),
        NO_METHOD_045,
    );
}

#[test]
fn zero_is_the_null_pointer() {
    row("memory/prov_is_null.lu", runs("true false\n"), &[]);
}

/// The integer side: widening by the source's signedness, narrowing
/// as `uint`. lupin 0.1.45 prints `0 0 0` (wolf-interp#184).
#[test]
fn the_integer_side_widens_by_its_sign_and_narrows_as_uint() {
    row(
        "memory/prov_narrow_cast.lu",
        runs("200 -1 4294967295\n"),
        &[Pin {
            version: "0.1.45",
            verdict: "exit(0)",
            stdout: "0 0 0\n",
        }],
    );
}

/// `300 as *u8 as u8` traps; at trunk the checked machine exited 44.
#[test]
fn a_pointer_cast_to_a_narrower_integer_traps_out_of_range() {
    row(
        "memory/prov_narrow_cast_trap.lu",
        OVERFLOW,
        &[Pin {
            version: "0.1.45",
            verdict: "exit(0)",
            stdout: "",
        }],
    );
}

#[test]
fn prefix_deref_reads_writes_and_compounds() {
    row("memory/raw_deref.lu", runs("7 7\n"), &[]);
}

/// wolf-lang#561: a signed pointee reads back signed on the checked
/// machine too, by `*s`, by `s[i]`, and inside `s[i] += v`.
#[test]
fn a_signed_pointee_reads_back_signed() {
    row("memory/raw_deref_signed.lu", runs("-5 -6 -4\n"), &[]);
}

/// kw00's probes, as they stand at head.
#[test]
fn kw00s_f9_probes() {
    let exposed = program(
        "f9_exposed",
        "import c \"stdlib.h\"\n\nfn main() -> int {\n    var a = 0\n    \
         // # Safety: q is p's own address, round-tripped.\n    unsafe {\n        \
         let p = c.malloc(8) as *u8\n        let n = p.expose()\n        \
         let q = p.with_exposed(n)\n        q[0] = 5\n        a = p[0] as int\n        \
         c.free(p)\n    }\n    a - 5\n}\n",
    );
    every_machine(&exposed, "f9_exposed", runs(""), NO_METHOD_045);
    let deref = program(
        "f9_deref",
        "import c \"stdlib.h\"\n\nfn main() -> int {\n    var a = 0\n    \
         // # Safety: p is a live 8-byte allocation, freed once.\n    unsafe {\n        \
         let p = c.malloc(8) as *u8\n        *p = 5\n        a = *p as int\n        \
         c.free(p)\n    }\n    a - 5\n}\n",
    );
    every_machine(&deref, "f9_deref", runs(""), &[]);
}

/// The ring: `*p` outside `unsafe` is E1301 on every machine. lupin
/// 0.1.45 gates `p[0]` but runs `*p` there (wolf-interp#184, item 3).
#[test]
fn a_dereference_outside_unsafe_is_e1301() {
    let p = program(
        "deref_outside",
        "import c \"stdlib.h\"\n\nfn main() -> int {\n    \
         // # Safety: 8 zeroed bytes, never freed (the program ends).\n    \
         let p = unsafe {\n        c.calloc(1, 8) as *u8\n    }\n    *p = 3\n    0\n}\n",
    );
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&p, flag) else {
            continue;
        };
        assert_eq!(obs.verdict, "fail(E1301)", "{flag}: {obs:?}");
    }
    if let Some(l) = lupin_says(&p) {
        let want = if l.version == "0.1.45" {
            "exit(0)"
        } else {
            "fail(E1301)"
        };
        assert_eq!(
            l.verdict, want,
            "lupin {} (0.1.45 pre-mirror, wolf-interp#184): {l:?}",
            l.version
        );
    }
}

/// `*` on anything but a raw pointer is E0409 on the compiler's three
/// lanes (lupin refuses by name, which the differential excludes).
#[test]
fn a_dereference_of_a_non_pointer_is_e0409() {
    let p = program(
        "deref_int",
        "fn main() -> int {\n    let n = 5\n    // # Safety: none needed; this is refused.\n    \
         unsafe {\n        *n\n    }\n}\n",
    );
    let want = Want {
        verdict: "fail(E0409)",
        codes: &["E0409"],
        stdout: "",
        named: "",
    };
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&p, flag) else {
            continue;
        };
        assert_obs(flag, "deref_int", &obs, want);
    }
}

/// The VGA text buffer written from a hosted `main`: foreign memory.
const DEVICE_HOSTED: &str = "fn main() -> int {\n    \
    // # Safety: DELIBERATELY foreign: 0xb8000 is the VGA text buffer\n    \
    // on a PC, and no allocation owns it on a hosted target.\n    \
    unsafe {\n        let p = 0xb8000 as *u16\n        *p = 0x0f4b\n    }\n    0\n}\n";

/// `[mem.prov.device]`, hosted: UB row L2 on the checked machine and
/// lupin alike. The compiled tiers' behaviour is undefined, so nothing
/// is asserted of them here.
#[test]
fn foreign_memory_on_a_hosted_target_is_ub_row_l2() {
    let p = program("device_hosted", DEVICE_HOSTED);
    let obs = lane(&p, "--checked").expect("the checked lane always runs");
    assert_eq!(obs.verdict, "ub(mem.ub)", "the CHECKED lane: {obs:?}");
    assert_eq!(obs.ub_row, "L2", "the CHECKED lane's row: {obs:?}");
    assert_eq!(obs.codes, vec!["E1401".to_string()], "{obs:?}");
    if let Some(l) = lupin_says(&p) {
        assert_eq!(l.verdict, "ub(mem.ub)", "lupin {}: {l:?}", l.version);
        assert_eq!(l.ub_row, "L2", "lupin {}'s row: {l:?}", l.version);
    }
}

/// `[mem.prov.device]`, freestanding: the platform's meaning, which no
/// machine here models. Every wolfgang rung refuses the program by
/// naming the target before any access — never `ub` — and lupin must
/// do the same; 0.1.45 answers `--target` with a usage error and no
/// record, pinned by version (wolf-interp#182).
#[test]
fn foreign_memory_on_the_freestanding_target_is_refused_by_name_never_ub() {
    let p = program("device_freestanding", DEVICE_HOSTED);
    for flag in ["--checked", "--native", "--release"] {
        let obs = lane_with(&p, flag, &["--target", "x86_64-unknown-none"])
            .expect("a refusal is a record on every host");
        assert_eq!(obs.verdict, "unsupported", "{flag} --target: {obs:?}");
        assert_eq!(
            obs.unsupported, "the freestanding target x86_64-unknown-none",
            "{flag} --target names the target: {obs:?}"
        );
        assert!(obs.ub_row.is_empty(), "{flag} --target answers no UB row");
    }
    let Some(out) = lupin_raw(&p, &["--target", "x86_64-unknown-none"]) else {
        return;
    };
    let version = lupin_version().unwrap_or_default();
    if version == "0.1.45" {
        assert_eq!(
            out.status.code(),
            Some(2),
            "lupin 0.1.45 (pre-mirror, wolf-interp#182) refuses `--target` as a usage error"
        );
        assert!(
            out.stdout.is_empty(),
            "lupin 0.1.45 (pre-mirror, wolf-interp#182) writes no record"
        );
        return;
    }
    let l = parse_obs(&out.stdout, &out.stderr, "lupin's --target observation");
    assert_eq!(
        l.verdict, "unsupported",
        "lupin {} (the mirror is wolf-interp#182) --target: {l:?}",
        l.version
    );
    assert!(
        l.unsupported.contains("x86_64-unknown-none"),
        "lupin {} names the freestanding target: {l:?}",
        l.version
    );
    assert!(l.ub_row.is_empty(), "lupin {} answers no UB row", l.version);
}

// ---- the freestanding kernel ----------------------------------------

const TARGET: &str = "x86_64-unknown-none";
const HOOKS: &[&str] = &["wolf_trap", "memcpy", "memmove", "memset", "memcmp"];

fn fixture(dir: &str, name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Build the device kernel for the target on `tier`. Any failure fails
/// the gate — a compile error's exit 2 is never a skip (wolf-lang#550).
fn build_kernel(case: &str, tier: &str) -> PathBuf {
    let dir = scratch(case);
    let src = dir.join("kmain_device.lu");
    std::fs::copy(fixture("freestanding_device", "kmain_device.lu"), &src).expect("copy");
    let obj = dir.join(format!("kmain_device.{tier}.o"));
    let mut cmd = Command::new(wolf());
    cmd.current_dir(&dir).args([
        "build",
        "kmain_device.lu",
        "--target",
        TARGET,
        "--emit=obj",
        "-o",
        obj.to_str().unwrap(),
    ]);
    if tier == "release" {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf runs");
    assert!(
        out.status.success() && obj.is_file(),
        "wolf build kmain_device.lu --target {TARGET} --emit=obj ({tier}) must build an \
         object (exit {:?}):\n{}",
        out.status.code(),
        text(&out.stderr)
    );
    obj
}

/// The object imports only the hook list and its own extern, and
/// defines `kmain` under its own name.
fn assert_kernel_symbols(obj: &Path) {
    use object::{Object, ObjectSymbol};
    let bytes = std::fs::read(obj).expect("read object");
    let file = object::File::parse(&*bytes).expect("parse object");
    let mut undef = Vec::new();
    let mut global = Vec::new();
    for s in file.symbols() {
        let Ok(name) = s.name() else { continue };
        if name.is_empty() {
            continue;
        }
        if s.is_undefined() {
            undef.push(name.to_string());
        } else if s.is_global() {
            global.push(name.to_string());
        }
    }
    let stray: Vec<&String> = undef
        .iter()
        .filter(|s| !HOOKS.contains(&s.as_str()) && s.as_str() != "kw_outb")
        .collect();
    assert!(
        stray.is_empty(),
        "{}: imports outside the hooks and `kw_outb`: {stray:?}",
        obj.display()
    );
    assert!(
        global.iter().any(|g| g == "kmain"),
        "{}: `kmain` defined: {global:?}",
        obj.display()
    );
}

/// The native tier emits the target from any host.
#[test]
fn the_device_kernel_builds_on_every_host() {
    let obj = build_kernel("kernel_any_host", "native");
    assert_kernel_symbols(&obj);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux_x86_64 {
    use super::*;

    fn tool(name: &str, args: &[&str]) -> Output {
        Command::new(name)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("`{name}` is part of this gate's host (no skip): {e}"))
    }

    /// Link `objs` with the device window at 0xb8000 (`device.ld`), no
    /// libc and no runtime: `ld.lld`, else the system `ld`.
    fn link(objs: &[PathBuf], exe: &Path) {
        let script = fixture("freestanding_device", "device.ld");
        let mut args: Vec<String> = vec![
            "-static".into(),
            "-nostdlib".into(),
            "-T".into(),
            script.to_str().unwrap().into(),
            "-o".into(),
            exe.to_str().unwrap().into(),
        ];
        args.extend(objs.iter().map(|p| p.to_str().unwrap().to_string()));
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
    }

    fn assemble(dir: &Path, src: PathBuf) -> PathBuf {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let o = dir.join(
            src.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .replace(".S", ".o"),
        );
        let r = tool(
            &cc,
            &["-c", src.to_str().unwrap(), "-o", o.to_str().unwrap()],
        );
        assert!(r.status.success(), "{cc} -c: {}", text(&r.stderr));
        o
    }

    /// The release tier addresses the device directly: `0xb8000` is an
    /// absolute operand in the code, not a value loaded from anywhere.
    /// (How many stores survive `-O2` is volatile's question — kw07,
    /// `[mem.unsafe.volatile]` — and is not asserted here.)
    #[test]
    fn the_release_kernel_addresses_the_device_directly() {
        let obj = build_kernel("kernel_disasm", "release");
        let out = tool(
            "objdump",
            &["-d", "--no-show-raw-insn", obj.to_str().unwrap()],
        );
        assert!(out.status.success(), "objdump: {}", text(&out.stderr));
        let dis = text(&out.stdout);
        assert!(
            dis.lines()
                .any(|l| l.contains("mov") && l.trim_end().ends_with(",0xb8000")),
            "a store to the absolute address 0xb8000 in:\n{dis}"
        );
    }

    /// Linked with no libc beside a page at 0xb8000, the kernel runs on
    /// both tiers: it writes the four cells through every spelling,
    /// reads them back through `vga[i]`, prints their low bytes and
    /// returns 33 (the cell's attribute byte, the pointers' distance).
    #[test]
    fn linked_beside_a_device_window_the_kernel_runs_on_both_tiers() {
        for tier in ["native", "release"] {
            let obj = build_kernel(&format!("kernel_run_{tier}"), tier);
            assert_kernel_symbols(&obj);
            let dir = obj.parent().unwrap().to_path_buf();
            let start = assemble(&dir, fixture("freestanding", "start.S"));
            let stub = assemble(&dir, fixture("freestanding", "rt_stub.S"));
            let window = assemble(&dir, fixture("freestanding_device", "dev_window.S"));
            let exe = dir.join("kmain_device.elf");
            link(&[start, stub, window, obj], &exe);
            let out = tool(exe.to_str().unwrap(), &[]);
            assert_eq!(
                text(&out.stdout),
                "KWC\n",
                "{tier}: the four cells' low bytes"
            );
            assert_eq!(
                out.status.code(),
                Some(33),
                "{tier}: kmain's result is the status"
            );
        }
    }
}
