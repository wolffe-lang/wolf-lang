//! kw09 (K6 and K11 = A, STATUS #31): module state and linker control in
//! the object — `[mem.static]`, `[abi.link.section]`, `[abi.link.extern]`
//! and `[abi.link.script]` — on both compiling tiers, hosted and
//! freestanding.
//!
//! What each object must hold, read from its symbol table: a module `let`
//! in read-only data, a nonzero `var` in `.data`, an all-zero `var` in
//! `.bss`, every `#[section(".x")]` item and function in `.x`, an `extern
//! "c" let` an undefined symbol, and a `const` nowhere (it has no
//! storage). Then the freestanding kernel is linked with the consuming
//! build's script (`static.ld`: the boot section first, the image's end
//! as a symbol) and run with no libc on both tiers: its entry sits at the
//! image's first text address, and its result folds the module state and
//! both link-time symbols together.
//!
//! Measured at trunk 9bf6a5d5 (kasumi, `~/lanes/kw09/evidence/
//! probes-trunk.log`): no object existed — `#[section]` was E0817 on
//! both tiers, `extern "c" let` was E0201 at parse, and a module
//! `let`/`var`/`const` stopped the build at `item-initializer lowering
//! (globals)`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TARGET: &str = "x86_64-unknown-none";

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/freestanding_static")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

#[cfg_attr(not(all(target_os = "linux", target_arch = "x86_64")), allow(dead_code))]
fn freestanding(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/freestanding")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("static_sections")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Copy one fixture into its own directory (a directory is a module).
fn staged(case: &str, name: &str) -> PathBuf {
    let dst = scratch(case).join(name);
    std::fs::copy(fixture(name), &dst).expect("copy fixture");
    dst
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// The hosted runtime archive the hosted links need (as every lanes
/// gate builds it first).
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

/// `wolf build <src> [--target T] --emit=obj [--release]` beside the
/// source; the whole output, for the caller to judge.
fn build(src: &Path, tier: &str, target: Option<&str>, emit: &str) -> (Output, PathBuf) {
    let dir = src.parent().expect("dir");
    let name = src.file_name().unwrap().to_str().unwrap();
    let stem = src.file_stem().unwrap().to_str().unwrap();
    let out_path = dir.join(match emit {
        "obj" => format!("{stem}.{tier}.o"),
        _ => format!("{stem}.{tier}.exe"),
    });
    let mut cmd = Command::new(wolf());
    cmd.current_dir(dir).args(["build", name, "--no-cache"]);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    if emit == "obj" {
        cmd.arg("--emit=obj");
    }
    cmd.arg("-o").arg(&out_path);
    if tier == "release" {
        cmd.arg("--release");
    }
    (cmd.output().expect("wolf runs"), out_path)
}

/// Build an object; any failure fails the gate (no host skips this).
fn build_obj(src: &Path, tier: &str, target: Option<&str>) -> PathBuf {
    let (out, obj) = build(src, tier, target, "obj");
    assert!(
        out.status.success() && obj.is_file(),
        "wolf build {} {target:?} --emit=obj ({tier}) must build an object (exit {:?}):\n{}",
        src.display(),
        out.status.code(),
        text(&out.stderr)
    );
    obj
}

/// Every named symbol of an object, with the section it is defined in
/// (`*UND*` when undefined).
fn placements(obj: &Path) -> BTreeMap<String, String> {
    use object::{Object, ObjectSection, ObjectSymbol, SymbolSection};
    let bytes = std::fs::read(obj).expect("read object");
    let file = object::File::parse(&*bytes).expect("parse object");
    let mut out = BTreeMap::new();
    for s in file.symbols() {
        let Ok(name) = s.name() else { continue };
        if name.is_empty() {
            continue;
        }
        let sec = match s.section() {
            SymbolSection::Undefined => "*UND*".to_string(),
            SymbolSection::Section(i) => file
                .section_by_index(i)
                .ok()
                .and_then(|sec| sec.name().ok().map(str::to_string))
                .unwrap_or_default(),
            other => format!("{other:?}"),
        };
        out.insert(name.to_string(), sec);
    }
    out
}

/// The section of the one symbol named `name`, or of the one module
/// item whose mangled name starts `_W<name>.s` (`[abi.link.script]`:
/// module state is defined under its mangled name).
fn section_of<'a>(syms: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    if let Some(s) = syms.get(name) {
        return Some(s);
    }
    let prefix = format!("_W{name}.s");
    let hits: Vec<&String> = syms.keys().filter(|k| k.starts_with(&prefix)).collect();
    assert!(hits.len() <= 1, "`{name}` is defined once: {hits:?}");
    hits.first().map(|k| syms[*k].as_str())
}

/// Each (item, section) holds in `obj`; `None` means "no symbol at all"
/// (a `const`).
fn assert_placed(obj: &Path, who: &str, want: &[(&str, Option<&str>)]) {
    let syms = placements(obj);
    for (item, sec) in want {
        let got = section_of(&syms, item);
        match sec {
            Some(".rodata") => assert!(
                got.is_some_and(|g| g == ".rodata" || g.starts_with(".rodata.cst")),
                "{who}: `{item}` is read-only data in `.rodata` (got {got:?}; symbols {syms:?})"
            ),
            Some(s) => assert_eq!(
                got,
                Some(*s),
                "{who}: `{item}` placed in `{s}` (symbols {syms:?})"
            ),
            None => assert_eq!(
                got, None,
                "{who}: `{item}` is a `const` — no storage, no symbol (symbols {syms:?})"
            ),
        }
    }
}

const KERNEL: &[(&str, Option<&str>)] = &[
    ("kmain", Some(".text.boot")),
    ("placed", Some(".data.kw")),
    ("MAGIC", Some(".rodata.kw")),
    ("ticks", Some(".bss")),
    ("boots", Some(".data")),
    ("BANNER_LEN", Some(".rodata")),
    ("COM1", None),
    ("__kw_image_end", Some("*UND*")),
    ("kw_table", Some("*UND*")),
];

#[cfg_attr(not(all(target_os = "linux", target_arch = "x86_64")), allow(dead_code))]
const HOSTED: &[(&str, Option<&str>)] = &[
    ("kw_hot", Some(".text.kwhot")),
    ("placed", Some(".data.kw")),
    ("MAGIC", Some(".rodata.kw")),
    ("zero", Some(".bss")),
    ("start", Some(".data")),
    ("PLAIN", Some(".rodata")),
    ("LIMIT", None),
];

/// The native tier emits the freestanding target from every host, so
/// the kernel object's placements are checked on every host.
#[test]
fn the_kernel_object_places_module_state_and_sections_on_every_host() {
    let k = staged("kernel_native_any_host", "kmain_static.lu");
    let obj = build_obj(&k, "native", Some(TARGET));
    assert_placed(&obj, "native freestanding", KERNEL);
}

/// `[abi.link.section]`: Mach-O and COFF hosts refuse `#[section]` by
/// name on both tiers (the release tier is environment-refused on a
/// host it does not serve); an ELF host builds and runs the program.
#[test]
fn a_hosted_program_places_or_is_refused_by_name() {
    ensure_rt_staticlib();
    let elf = cfg!(target_os = "linux");
    for tier in ["native", "release"] {
        let src = staged(&format!("hosted_{tier}"), "hosted_sections.lu");
        let (out, exe) = build(&src, tier, None, "bin");
        let stderr = text(&out.stderr);
        if out.status.code() == Some(2) && !elf && tier == "release" {
            // The release tier does not serve this host at all (an
            // environment refusal, named) — nothing about sections.
            assert!(
                stderr.contains("release tier"),
                "{tier}: an environment refusal names the tier: {stderr}"
            );
            continue;
        }
        if elf {
            assert!(
                out.status.success(),
                "{tier}: an ELF host builds the placed program: {stderr}"
            );
            let run = Command::new(&exe).output().expect("run");
            assert_eq!(
                text(&run.stdout),
                "24 100\n",
                "{tier}: the program's answer"
            );
            assert_eq!(run.status.code(), Some(0), "{tier}: exit status");
        } else {
            let format = if cfg!(target_os = "macos") {
                "Mach-O"
            } else {
                "COFF"
            };
            assert!(
                !out.status.success()
                    && stderr.contains(&format!("section placement on a {format} target")),
                "{tier}: `#[section]` on a {format} host is refused by name: exit {:?} {stderr}",
                out.status.code()
            );
        }
    }
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

    #[test]
    fn objects_place_module_state_on_both_tiers_hosted_and_freestanding() {
        for tier in ["native", "release"] {
            let k = staged(&format!("kernel_obj_{tier}"), "kmain_static.lu");
            let obj = build_obj(&k, tier, Some(TARGET));
            assert_placed(&obj, &format!("{tier} freestanding"), KERNEL);
            let h = staged(&format!("hosted_obj_{tier}"), "hosted_sections.lu");
            let obj = build_obj(&h, tier, None);
            assert_placed(&obj, &format!("{tier} hosted"), HOSTED);
        }
    }

    /// Assemble the boot stub, the hook stub and the table.
    fn stubs(dir: &Path) -> Vec<PathBuf> {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let mut out = Vec::new();
        for src in [
            freestanding("start.S"),
            freestanding("rt_stub.S"),
            fixture("table.S"),
        ] {
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
            assert!(
                r.status.success(),
                "{cc} -c {}: {}",
                src.display(),
                text(&r.stderr)
            );
            out.push(o);
        }
        out
    }

    /// `[abi.link.script]`: the consuming build links — `static.ld`, no
    /// libc, no wolf runtime: `ld.lld`, else the system `ld`.
    fn link(objs: &[PathBuf], exe: &Path) {
        let script = fixture("static.ld");
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

    #[test]
    fn the_kernel_links_with_its_script_and_runs_on_both_tiers() {
        use object::{Object, ObjectSection, ObjectSymbol};
        for tier in ["native", "release"] {
            let k = staged(&format!("kernel_run_{tier}"), "kmain_static.lu");
            let dir = k.parent().unwrap().to_path_buf();
            let obj = build_obj(&k, tier, Some(TARGET));
            let mut objs = stubs(&dir);
            objs.push(obj);
            let exe = dir.join("kmain_static.elf");
            link(&objs, &exe);
            // The entry is the first byte of the image's `.text.boot`
            // output section, which the script placed first.
            let bytes = std::fs::read(&exe).expect("read image");
            let file = object::File::parse(&*bytes).expect("parse image");
            let boot = file
                .section_by_name(".text.boot")
                .unwrap_or_else(|| panic!("{tier}: the image has a .text.boot"));
            let kmain = file
                .symbols()
                .find(|s| s.name() == Ok("kmain"))
                .unwrap_or_else(|| panic!("{tier}: kmain in the image"));
            assert_eq!(
                kmain.address(),
                boot.address(),
                "{tier}: kmain is placed at .text.boot's start ({:#x})",
                boot.address()
            );
            assert_eq!(
                boot.address(),
                0x400000,
                "{tier}: .text.boot is the image's first"
            );
            let out = tool(exe.to_str().unwrap(), &[]);
            assert_eq!(
                text(&out.stdout),
                "KWC\n",
                "{tier}: the kernel's port writes"
            );
            // placed 5+4, MAGIC 9, boots 2+1, kw_table[1] 33, and the
            // image's end lies past the table: 9 + 9 + 3 + 33 + 1.
            assert_eq!(
                out.status.code(),
                Some(55),
                "{tier}: kmain's result folds the module state and both link-time symbols"
            );
        }
    }
}
