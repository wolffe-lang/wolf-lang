//! kw05 (STATUS #31, K2 = C: linked assembly now, inline `asm` later):
//! assembly sources listed in `wolf.pkg` (`[abi.asm]`, `[abi.asm.link]`,
//! `[abi.asm.roster]`, `[abi.asm.machines]`).
//!
//! Rows, each built from a scratch package the test writes:
//!
//! 1. the freestanding kernel (kw04's `kmain.lu`) with `asm:
//!    ["rt_stub.S"]`, built `--target x86_64-unknown-none --emit=obj -o
//!    K.o` on both tiers, leaves `K.asm-rt_stub.o` beside `K.o`: an ELF
//!    x86-64 object defining the stub's three routines;
//! 2. on an x86-64 linux host, those two objects and the consumer's own
//!    boot stub (`start.S`, not listed: the boot code links the object,
//!    K6(b)) link with `ld.lld -nostdlib` — no libc, no runtime — and the
//!    kernel runs: `KWC\n`, exit 33, both tiers;
//! 3. a call to a routine no listed source defines is E1306 on the
//!    freestanding target, naming it;
//! 4. a hosted program reaching the host's own listed routine links it
//!    and exits 7 on both tiers (x86-64 linux, arm64 macOS);
//! 5. the checked machine refuses that call naming assembly and the
//!    routine — never the generic C-membrane text;
//! 6. the manifest's own refusals: a missing listed file, an `asm` that
//!    is not a list of strings, and a dependency that lists `asm`.
//!
//! At trunk a2e16bf8 every row was red: `asm` was `error[E1502]: unknown
//! manifest key` (exit 2) on every build, and the checked machine said
//! `a call into C through a hand-declared extern "c" fn` (kasumi
//! `~/lanes/kw05/evidence/`). A failed build FAILS here, whatever its
//! exit status: `wolf build` exits 2 on a compile error as well as on a
//! tool error, and a gate that reads 2 as a skip passes vacuously
//! (wolf-lang#550). There is no skip in this file. Rows that assemble
//! need a unix C driver (`cc`) and are compiled only on unix; rows that
//! link and run need the matching host and are compiled only there.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TARGET: &str = "x86_64-unknown-none";

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(dir: &str, name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir)
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("asm_link")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// A scratch package: `files` copied from fixtures as (fixture dir,
/// name, name in the package), plus `wolf.pkg` holding `manifest`.
fn package(case: &str, files: &[(&str, &str, &str)], manifest: &str) -> PathBuf {
    let dir = scratch(case);
    for (from, name, to) in files {
        std::fs::copy(fixture(from, name), dir.join(to)).expect("copy fixture");
    }
    std::fs::write(dir.join("wolf.pkg"), manifest).expect("write wolf.pkg");
    dir
}

fn manifest(asm: &str) -> String {
    format!("pkg {{\n    name: \"kw05/gate\"\n    version: \"0.0.1\"\n    asm: {asm}\n}}\n")
}

fn wolf_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(wolf())
        .current_dir(dir)
        .args(args)
        .output()
        .expect("wolf runs")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// `wolf build SRC --target x86_64-unknown-none --emit=obj -o K.o` on
/// `tier`; any failure fails the gate.
#[cfg(unix)]
fn build_kernel(dir: &Path, src: &str, tier: &str) -> PathBuf {
    let obj = dir.join(format!("k.{tier}.o"));
    let o = obj.to_str().unwrap().to_string();
    let mut args = vec!["build", src, "--target", TARGET, "--emit=obj", "-o", &o];
    if tier == "release" {
        args.push("--release");
    }
    let out = wolf_in(dir, &args);
    assert!(
        out.status.success() && obj.is_file(),
        "wolf build {src} --target {TARGET} --emit=obj ({tier}) with listed assembly must \
         build (exit {:?}) — a refusal here is the gate failing (#550):\n{}",
        out.status.code(),
        text(&out.stderr)
    );
    obj
}

/// Global symbols an ELF object defines.
#[cfg(unix)]
fn defined_globals(obj: &Path) -> std::collections::BTreeSet<String> {
    use object::{Object, ObjectSymbol};
    let bytes = std::fs::read(obj).expect("read object");
    let file = object::File::parse(&*bytes).expect("parse object");
    assert_eq!(
        file.format(),
        object::BinaryFormat::Elf,
        "{}: the freestanding target's assembly objects are ELF on every host",
        obj.display()
    );
    assert_eq!(
        file.architecture(),
        object::Architecture::X86_64,
        "{}: assembled for x86-64, whatever the host",
        obj.display()
    );
    file.symbols()
        .filter(|s| s.is_global() && !s.is_undefined())
        .filter_map(|s| s.name().ok().map(str::to_string))
        .collect()
}

/// Row 1: the listed source is assembled for the target and placed
/// beside the object, on both tiers, from any unix host.
#[cfg(unix)]
#[test]
fn listed_assembly_is_assembled_for_the_target_beside_the_object() {
    for tier in ["native", "release"] {
        let dir = package(
            &format!("beside_{tier}"),
            &[
                ("freestanding", "kmain.lu", "kmain.lu"),
                ("freestanding", "rt_stub.S", "rt_stub.S"),
            ],
            &manifest("[\"rt_stub.S\"]"),
        );
        let obj = build_kernel(&dir, "kmain.lu", tier);
        let beside = dir.join(format!("k.{tier}.asm-rt_stub.o"));
        assert!(
            beside.is_file(),
            "{tier}: `--emit=obj -o {}` places the listed source's object beside it as \
             `{}` ([abi.asm.link]); the directory holds: {:?}",
            obj.display(),
            beside.display(),
            std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect::<Vec<_>>()
        );
        let globals = defined_globals(&beside);
        for routine in ["wolf_trap", "kw_outb", "kw_big"] {
            assert!(
                globals.contains(routine),
                "{tier}: the assembled stub defines `{routine}`; globals: {globals:?}"
            );
        }
    }
}

/// Row 3: on the freestanding target the roster is the listed sources'
/// `.globl` names, and a call into anything else is E1306 by name.
#[test]
fn a_call_no_listed_source_defines_is_e1306() {
    for tier in ["native", "release"] {
        let dir = package(
            &format!("unlisted_{tier}"),
            &[
                ("asm", "kmain_unlisted.lu", "kmain.lu"),
                ("freestanding", "rt_stub.S", "rt_stub.S"),
            ],
            &manifest("[\"rt_stub.S\"]"),
        );
        let mut args = vec![
            "build", "kmain.lu", "--target", TARGET, "--emit=obj", "-o", "k.o",
        ];
        if tier == "release" {
            args.push("--release");
        }
        let out = wolf_in(&dir, &args);
        let err = text(&out.stderr);
        assert!(
            !out.status.success() && err.contains("error[E1306]") && err.contains("`kw_missing`"),
            "{tier}: a call into `kw_missing`, which no listed `asm` source defines, is E1306 \
             naming it ([abi.asm.roster]); exit {:?}:\n{err}",
            out.status.code()
        );
        assert!(
            !err.contains("`kw_outb`"),
            "{tier}: `kw_outb` is on the roster (rt_stub.S's .globl) and is not refused:\n{err}"
        );
        assert!(
            !dir.join("k.o").exists(),
            "{tier}: a refused build writes no object"
        );
    }
}

/// The host's listed routine for rows 4 and 5.
fn seven_source() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "seven_aarch64_macos.S"
    } else {
        "seven_x86_64.S"
    }
}

fn hosted_package(case: &str) -> PathBuf {
    let src = seven_source();
    package(
        case,
        &[("asm", "hosted.lu", "main.lu"), ("asm", src, src)],
        &manifest(&format!("[\"{src}\"]")),
    )
}

/// Row 4: a hosted build hands the listed object to its link.
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
))]
#[test]
fn a_hosted_program_links_its_listed_assembly_on_both_tiers() {
    for tier in ["native", "release"] {
        let dir = hosted_package(&format!("hosted_{tier}"));
        let mut args = vec!["build", "main.lu", "-o", "seven"];
        if tier == "release" {
            args.push("--release");
        }
        let out = wolf_in(&dir, &args);
        assert!(
            out.status.success(),
            "{tier}: the hosted build links the listed routine (exit {:?}):\n{}",
            out.status.code(),
            text(&out.stderr)
        );
        let run = Command::new(dir.join("seven")).output().expect("run");
        assert_eq!(
            run.status.code(),
            Some(7),
            "{tier}: `main` returns the assembly routine's 7"
        );
    }
}

/// Row 5: the checked machine has no assembly membrane; it refuses the
/// call by name, naming assembly and the routine ([abi.asm.machines]).
/// Every host: the roster is read from the source text, nothing is
/// assembled.
#[test]
fn the_checked_machine_refuses_a_call_into_assembly_by_name() {
    let dir = hosted_package("checked");
    let out = wolf_in(&dir, &["conform-run", "--json", "--checked", "main.lu"]);
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "conform-run prints one record ({e}):\n{}\n{}",
            text(&out.stdout),
            text(&out.stderr)
        )
    });
    assert_eq!(rec["verdict"], "unsupported", "record: {rec}");
    let construct = rec["x-unsupported-construct"].as_str().unwrap_or_default();
    assert!(
        construct.starts_with("a call into assembly `kw_seven`"),
        "the refusal names assembly and the routine, not the generic C membrane; record: {rec}"
    );
}

/// Row 6: what the manifest refuses, each by name.
#[test]
fn the_manifest_refuses_a_missing_source_a_non_list_and_a_dependency_by_name() {
    // A listed file that does not exist.
    let dir = package(
        "missing",
        &[("asm", "hosted.lu", "main.lu")],
        &manifest("[\"boot/nope.S\"]"),
    );
    let out = wolf_in(&dir, &["build", "main.lu", "-o", "x"]);
    let err = text(&out.stderr);
    assert!(
        !out.status.success() && err.contains("boot/nope.S") && !err.contains("unknown manifest key"),
        "a listed `asm` source that does not exist is refused naming it (exit {:?}):\n{err}",
        out.status.code()
    );
    // Not a list of strings.
    let dir = package(
        "not_a_list",
        &[("asm", "hosted.lu", "main.lu")],
        &manifest("\"seven.S\""),
    );
    let out = wolf_in(&dir, &["build", "main.lu", "-o", "x"]);
    let err = text(&out.stderr);
    assert!(
        !out.status.success() && err.contains("error[E1502]") && err.contains("`asm` takes a list"),
        "`asm` takes a list of strings (exit {:?}):\n{err}",
        out.status.code()
    );
    // A dependency listing `asm`: its sources are outside the package's
    // content address ([pkg.sum]), so only the root manifest lists them.
    let dir = scratch("dependency");
    let dep = dir.join("dep");
    std::fs::create_dir_all(&dep).unwrap();
    std::fs::write(
        dep.join("wolf.pkg"),
        "pkg {\n    name: \"kw05/dep\"\n    version: \"0.0.1\"\n    asm: [\"x.S\"]\n}\n",
    )
    .unwrap();
    std::fs::write(dep.join("lib.lu"), "pub fn one() -> int {\n    1\n}\n").unwrap();
    std::fs::write(dep.join("x.S"), "").unwrap();
    let app = dir.join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("wolf.pkg"),
        "pkg {\n    name: \"kw05/app\"\n    version: \"0.0.1\"\n    deps: {\n        dep: { path: \"../dep\" }\n    }\n}\n",
    )
    .unwrap();
    std::fs::write(
        app.join("main.lu"),
        "use dep\n\nfn main() -> int {\n    dep.one()\n}\n",
    )
    .unwrap();
    let out = wolf_in(&app, &["build", "main.lu", "-o", "x"]);
    let err = text(&out.stderr);
    assert!(
        !out.status.success() && err.contains("error[E1502]") && err.contains("`asm`"),
        "a dependency's `asm` is refused, naming the key (exit {:?}):\n{err}",
        out.status.code()
    );
}

/// Row 2: linked with the consumer's boot stub and no libc, the kernel
/// runs on both tiers.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linked {
    use super::*;

    fn tool(name: &str, args: &[&str]) -> Output {
        Command::new(name)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("{name} must exist on an x86-64 linux host: {e}"))
    }

    #[test]
    fn the_kernel_and_its_listed_assembly_link_without_libc_and_run() {
        for tier in ["native", "release"] {
            let dir = package(
                &format!("linked_{tier}"),
                &[
                    ("freestanding", "kmain.lu", "kmain.lu"),
                    ("freestanding", "rt_stub.S", "rt_stub.S"),
                    ("freestanding", "start.S", "start.S"),
                ],
                &manifest("[\"rt_stub.S\"]"),
            );
            let obj = build_kernel(&dir, "kmain.lu", tier);
            let beside = dir.join(format!("k.{tier}.asm-rt_stub.o"));
            // The boot stub is the consumer's (K6(b)): assembled here.
            let start = dir.join("start.o");
            let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
            let r = tool(
                &cc,
                &["-c", dir.join("start.S").to_str().unwrap(), "-o", start.to_str().unwrap()],
            );
            assert!(r.status.success(), "assemble start.S: {}", text(&r.stderr));
            let exe = dir.join("kmain.elf");
            let args = [
                "-static",
                "-nostdlib",
                "-o",
                exe.to_str().unwrap(),
                start.to_str().unwrap(),
                beside.to_str().unwrap(),
                obj.to_str().unwrap(),
            ];
            let r = match Command::new("ld.lld").args(args).output() {
                Ok(r) => r,
                Err(_) => tool("ld", &args),
            };
            assert!(r.status.success(), "{tier}: link: {}", text(&r.stderr));
            let out = tool(exe.to_str().unwrap(), &[]);
            assert_eq!(text(&out.stdout), "KWC\n", "{tier}: the kernel's port writes");
            assert_eq!(out.status.code(), Some(33), "{tier}: kmain's result");
        }
    }
}
