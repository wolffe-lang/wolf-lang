//! kw04 (STATUS #31: K1, K8(a), K10): `wolf build --target
//! x86_64-unknown-none --emit=obj` on both compiling tiers
//! (`[abi.target]`, `[abi.target.none]`, `[abi.target.entry]`,
//! `[abi.target.none.hooks]`, `[abi.target.none.alloc]`,
//! `[abi.target.none.codegen]`).
//!
//! The gate is M-KW's step 2 (kw00 report §3e) on three kernels under
//! `fixtures/freestanding/`: `kmain.lu` (M-KW's kernel, kw03's
//! `b as int as u8` included), `kmain_trap.lu` (one overflowing
//! addition of a value from outside) and `kmain_wide.lu` (aggregates by
//! value). On each object, native and release:
//!
//! 1. `nm -u` is a subset of the hook list (`wolf_trap`, `memcpy`,
//!    `memmove`, `memset`, `memcmp`) plus the program's own `extern`
//!    declarations — no `main` shim, no runtime symbol;
//! 2. `kmain` is defined, global, under its own name; nothing is `main`;
//! 3. the disassembly names no `xmm`/`ymm` register, stores nothing
//!    below `%rsp`, and every call to `wolf_trap` is followed by `ud2`;
//! 4. the release tier's IR carries `noredzone` and the no-SSE
//!    `"target-features"` on every function, under the
//!    `x86_64-unknown-none-elf` triple;
//! 5. linked with the gate's `start.S` and `rt_stub.S` by `ld.lld
//!    -static -nostdlib` — no libc, no `libwolf_rt.a` — the kernel runs:
//!    `KWC\n` and exit 33; the trap kernel reaches `wolf_trap` with its
//!    site and exits with the site's line.
//!
//! At trunk a2cb316a every row was red: `--target` was `unknown flag`
//! (exit 2) on both tiers, so no object existed (kasumi,
//! `~/lanes/kw04/evidence/`). A failed build FAILS here, whatever its
//! exit status — `wolf build` exits 2 on a compile error too, and a gate
//! that reads 2 as a host skip passes vacuously (wolf-lang#550). There
//! is no skip in this file: steps 3–5 need an x86-64 linux host's
//! `objdump`, `cc` and a linker and are compiled only there; the object
//! and refusal rows run on every host (the native tier emits the target
//! from any host).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TARGET: &str = "x86_64-unknown-none";

/// `[abi.target.none.hooks]`: the only symbols a freestanding object may
/// import besides the program's own `extern` declarations.
const HOOKS: &[&str] = &["wolf_trap", "memcpy", "memmove", "memset", "memcmp"];

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/freestanding")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("freestanding_target")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Copy a fixture into its own scratch directory (D32: every `.lu` in a
/// directory is one module, so each kernel builds alone).
fn staged(case: &str, name: &str) -> PathBuf {
    let dir = scratch(case);
    let dst = dir.join(name);
    std::fs::copy(fixture(name), &dst).expect("copy fixture");
    dst
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

/// Build `src` for the target on `tier` (`native` | `release`) to one
/// object beside it. Any failure fails the gate: there is no host this
/// build may be skipped on.
fn build_obj(src: &Path, tier: &str) -> PathBuf {
    let dir = src.parent().expect("dir");
    let name = src.file_name().unwrap().to_str().unwrap();
    let obj = dir.join(format!(
        "{}.{tier}.o",
        src.file_stem().unwrap().to_str().unwrap()
    ));
    let mut args = vec!["build", name, "--target", TARGET, "--emit=obj", "-o"];
    let o = obj.to_str().unwrap().to_string();
    args.push(&o);
    if tier == "release" {
        args.push("--release");
    }
    let out = wolf_in(dir, &args);
    assert!(
        out.status.success() && obj.is_file(),
        "wolf build {name} --target {TARGET} --emit=obj ({tier}) must build an object \
         (exit {:?}) — a refusal or a compile error here is the gate failing, never a \
         skip:\n{}",
        out.status.code(),
        text(&out.stderr)
    );
    obj
}

/// (undefined symbols, defined global symbols) of an ELF object.
fn symbols(obj: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    use object::{Object, ObjectSymbol};
    let bytes = std::fs::read(obj).expect("read object");
    let file = object::File::parse(&*bytes).expect("parse object");
    assert_eq!(
        file.format(),
        object::BinaryFormat::Elf,
        "{}: the target's objects are ELF on every host",
        obj.display()
    );
    assert_eq!(
        file.architecture(),
        object::Architecture::X86_64,
        "{}: an x86-64 object",
        obj.display()
    );
    let mut undef = BTreeSet::new();
    let mut global = BTreeSet::new();
    for s in file.symbols() {
        let Ok(name) = s.name() else { continue };
        if name.is_empty() {
            continue;
        }
        if s.is_undefined() {
            undef.insert(name.to_string());
        } else if s.is_global() {
            global.insert(name.to_string());
        }
    }
    (undef, global)
}

/// Rows 1 and 2 on one object: imports within the hook list plus
/// `externs`, `kmain` global, no `main`.
fn assert_step2_symbols(obj: &Path, externs: &[&str]) {
    let (undef, global) = symbols(obj);
    let allowed: BTreeSet<&str> = HOOKS.iter().chain(externs).copied().collect();
    let stray: Vec<&String> = undef
        .iter()
        .filter(|s| !allowed.contains(s.as_str()))
        .collect();
    assert!(
        stray.is_empty(),
        "{}: imports outside [abi.target.none.hooks] and the program's own externs: \
         {stray:?} (all imports: {undef:?})",
        obj.display()
    );
    assert!(
        global.contains("kmain"),
        "{}: `kmain` must be a defined global symbol under its own name \
         ([abi.target.entry]); globals: {global:?}",
        obj.display()
    );
    assert!(
        !global.contains("main") && !undef.contains("main"),
        "{}: a freestanding object has no `main` shim ([abi.target.none]); globals: \
         {global:?}",
        obj.display()
    );
}

/// Rows 1–2 on every host: the native tier emits the target from any
/// host (Cranelift's x86 backend is built everywhere).
#[test]
fn native_objects_import_the_hook_list_and_nothing_else_on_every_host() {
    let k = staged("native_kmain", "kmain.lu");
    assert_step2_symbols(&build_obj(&k, "native"), &["kw_outb"]);
    let t = staged("native_kmain_trap", "kmain_trap.lu");
    let obj = build_obj(&t, "native");
    assert_step2_symbols(&obj, &["kw_outb", "kw_big"]);
    let (undef, _) = symbols(&obj);
    assert!(
        undef.contains("wolf_trap"),
        "the trap kernel's overflow check reaches the one hook ([abi.target.none.hooks]); \
         imports: {undef:?}"
    );
    let w = staged("native_kmain_wide", "kmain_wide.lu");
    assert_step2_symbols(&build_obj(&w, "native"), &[]);
}

/// `[abi.target.none.hooks]`: on a hosted target `wolf_trap` is the
/// runtime's own report-and-exit (a link-time alias of the sited
/// reporter, made for a program that imports the hook), so a hosted
/// program that calls it traps as its own code does — on both tiers.
#[test]
fn the_hosted_runtime_defines_the_trap_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
    let dir = scratch("hosted_hook");
    std::fs::write(
        dir.join("h.lu"),
        "extern \"c\" fn wolf_trap(kind: i32, file: i64, file_len: i64, line: i64, col: i64)\n\n\
         fn main() -> int {\n    // # Safety: the hosted runtime defines the hook.\n    \
         unsafe {\n        wolf_trap(1, 0, 0, 0, 0)\n    }\n    0\n}\n",
    )
    .expect("write");
    // The release tier does not open on windows (s127's host gate).
    let lanes: &[&str] = if cfg!(windows) {
        &["--native"]
    } else {
        &["--native", "--release"]
    };
    for &lane in lanes {
        let out = wolf_in(&dir, &["conform-run", "h.lu", lane, "--json"]);
        let stdout = text(&out.stdout);
        let rec: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
            panic!("{lane}: one record ({e}): {stdout}\n{}", text(&out.stderr))
        });
        assert_eq!(
            rec["verdict"],
            "trap(overflow)",
            "{lane}: the hosted `wolf_trap` reports kind 1 and exits as a trap: {stdout}\n{}",
            text(&out.stderr)
        );
    }
}

/// `[abi.target.none]`: every construct that needs the hosted runtime,
/// every allocating construct, and every float is refused BY NAME at
/// the construct, on both tiers, before any backend runs.
#[test]
fn hosted_allocating_and_float_constructs_are_refused_by_name() {
    // (case, a helper item or "", the body of `fn work() -> int` that
    // `export fn kmain` returns, the name the refusal carries, the
    // reason class). Each program builds on the hosted target (a
    // `main` added) and imports the runtime there (kasumi, kw04
    // evidence/snippets-trunk.log); here it must be refused instead.
    let same = "fn same(a: str, b: str) -> bool {\n    a == b\n}\n\n";
    let rows: &[(&str, &str, &str, &str, &str)] = &[
        (
            "print",
            "",
            "print(\"hi\")\n    0",
            "`print`",
            "hosted runtime",
        ),
        (
            "env_args",
            "",
            "env_args().len",
            "`env_args`",
            "hosted runtime",
        ),
        (
            "str_compare",
            same,
            "if same(\"x\", \"y\") { 1 } else { 0 }",
            "`str` comparison",
            "hosted runtime",
        ),
        (
            "list",
            "",
            "var xs = [1, 2]\n    (mut xs).push(3)\n    xs.len",
            "`List`",
            "allocates",
        ),
        (
            "map",
            "",
            "var m = Map[str, int]()\n    m[\"a\"] = 1\n    0",
            "`Map`",
            "allocates",
        ),
        (
            "pool",
            "",
            "var p = Pool[int]()\n    0",
            "`Pool`",
            "allocates",
        ),
        (
            "interpolation",
            "",
            "let n = 5\n    let s = \"n={n}\"\n    s.len",
            "string interpolation",
            "allocates",
        ),
        (
            "closure",
            "",
            "let k = 3\n    let f = fn(x) x + k\n    f(1)",
            "a capturing closure",
            "allocates",
        ),
        (
            "region",
            "",
            "var n = 0\n    region scratch {\n        n = 1\n    }\n    n",
            "`region`",
            "allocates",
        ),
        (
            "spawn",
            "",
            "scope s {\n        s.spawn(fn() { })\n    }\n    0",
            "`spawn`",
            "hosted runtime",
        ),
        (
            "float",
            "",
            "let x = 1.5\n    if x > 1.0 { 1 } else { 0 }",
            "`f64`",
            "floating-point",
        ),
    ];
    let mut misses = Vec::new();
    for (case, prelude, body, name, class) in rows {
        let dir = scratch(&format!("refuse_{case}"));
        let src = format!(
            "{prelude}fn work() -> int {{\n    {body}\n}}\n\nexport fn kmain() -> int {{\n    work()\n}}\n"
        );
        std::fs::write(dir.join("k.lu"), &src).expect("write");
        for tier in ["native", "release"] {
            let mut args = vec![
                "build",
                "k.lu",
                "--target",
                TARGET,
                "--emit=obj",
                "-o",
                "k.o",
            ];
            if tier == "release" {
                args.push("--release");
            }
            let out = wolf_in(&dir, &args);
            let err = text(&out.stderr);
            let ok = out.status.code() == Some(4)
                && err.contains(name)
                && err.contains(class)
                && err.contains(&format!("target {TARGET}"));
            if !ok {
                misses.push(format!(
                    "{case} ({tier}): want exit 4 naming {name} ({class}, target {TARGET}); \
                     got exit {:?}: {}",
                    out.status.code(),
                    err.trim()
                ));
            }
        }
    }
    assert!(
        misses.is_empty(),
        "refusals by name:\n{}",
        misses.join("\n")
    );
}

/// The driver's half of `[abi.target.none]`: a freestanding build emits
/// objects only, `wolf run` cannot run one, an unknown triple is named,
/// and the machines that cannot run a kernel refuse the target by name.
#[test]
fn the_driver_refuses_bin_run_and_unknown_targets_by_name() {
    let k = staged("driver", "kmain.lu");
    let dir = k.parent().unwrap();
    let out = wolf_in(dir, &["build", "kmain.lu", "--target", TARGET]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("emits objects only"),
        "`bin` (the default) is refused by name on the target: {}",
        text(&out.stderr)
    );
    let out = wolf_in(dir, &["run", "--target", TARGET, "kmain.lu"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains(TARGET),
        "`wolf run` names the target it cannot run: {}",
        text(&out.stderr)
    );
    let out = wolf_in(
        dir,
        &[
            "build",
            "kmain.lu",
            "--target",
            "x86_64-unknown-bogus",
            "--emit=obj",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("unknown target `x86_64-unknown-bogus`")
            && text(&out.stderr).contains(TARGET),
        "an unknown triple is named beside the ones that build: {}",
        text(&out.stderr)
    );
    // F1 §4: the checked machine and the conform-run rungs never run a
    // freestanding program; the record names the target.
    for lane in ["--checked", "--native", "--release"] {
        let out = wolf_in(
            dir,
            &[
                "conform-run",
                "kmain.lu",
                lane,
                "--json",
                "--target",
                TARGET,
            ],
        );
        let stdout = text(&out.stdout);
        let rec: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
            panic!(
                "conform-run {lane} --target prints one record ({e}): {stdout}\n{}",
                text(&out.stderr)
            )
        });
        assert_eq!(rec["verdict"], "unsupported", "{lane}: {stdout}");
        assert_eq!(
            rec["x-unsupported-construct"],
            format!("the freestanding target {TARGET}"),
            "{lane}: {stdout}"
        );
    }
}

/// `[abi.target]`: the manifest key selects the target as the flag does,
/// and `cfg(target = "…")` reads the build's target, not the host's;
/// `main` is an ordinary function on the target.
#[test]
fn the_manifest_key_and_cfg_read_the_build_target() {
    let dir = scratch("manifest");
    std::fs::write(
        dir.join("wolf.pkg"),
        "pkg {\n    name: \"kw04/kern\",\n    version: \"0.1.0\",\n    target: \"x86_64-unknown-none\",\n}\n",
    )
    .expect("write manifest");
    std::fs::write(
        dir.join("k.lu"),
        "#[cfg(target = \"x86_64-unknown-none\")]\nexport fn only_freestanding() -> i64 {\n    1\n}\n\n\
         #[cfg(target = \"x86_64-unknown-linux-gnu\")]\nexport fn only_hosted() -> i64 {\n    2\n}\n\n\
         export fn kmain() -> i64 {\n    main()\n}\n\nfn main() -> i64 {\n    7\n}\n",
    )
    .expect("write source");
    let out = wolf_in(&dir, &["build", "k.lu", "--emit=obj", "-o", "k.o"]);
    assert!(
        out.status.success(),
        "the manifest's `target` builds the object: {}",
        text(&out.stderr)
    );
    let (undef, global) = symbols(&dir.join("k.o"));
    assert!(global.contains("only_freestanding"), "{global:?}");
    assert!(!global.contains("only_hosted"), "{global:?}");
    assert!(global.contains("kmain"), "{global:?}");
    assert!(
        !global.contains("main"),
        "on the target `main` is an ordinary function, not the entry: {global:?}"
    );
    assert!(
        undef.iter().all(|s| HOOKS.contains(&s.as_str())),
        "imports: {undef:?}"
    );
    // The manifest's target refuses `bin` exactly as the flag does.
    let out = wolf_in(&dir, &["build", "k.lu"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("emits objects only"));
}

/// Rows 3–5: the disassembly, the release IR, and the link with no libc,
/// on the x86-64 linux host where the object runs.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux_x86_64 {
    use super::*;

    /// The release IR's no-SSE feature string ([abi.target.none.codegen]).
    const FEATURES: &str = "\"target-features\"=\"-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,\
                            -sse4.2,-avx,-avx2,+soft-float\"";

    fn tool(name: &str, args: &[&str]) -> Output {
        Command::new(name)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("`{name}` is part of this gate's host (no skip): {e}"))
    }

    /// Row 3: no SSE/AVX register, no store below `%rsp`, `ud2` after
    /// every call to `wolf_trap`.
    fn assert_disassembly(obj: &Path) {
        let out = tool(
            "objdump",
            &["-dr", "--no-show-raw-insn", obj.to_str().unwrap()],
        );
        assert!(out.status.success(), "objdump: {}", text(&out.stderr));
        let dis = text(&out.stdout);
        let insns: Vec<&str> = dis
            .lines()
            .filter(|l| l.contains(":\t") || l.contains("R_X86_64"))
            .collect();
        assert!(
            insns.len() > 10,
            "objdump printed the code: {}",
            dis.lines().take(20).collect::<Vec<_>>().join("\n")
        );
        let sse: Vec<&&str> = insns
            .iter()
            .filter(|l| l.contains("%xmm") || l.contains("%ymm"))
            .collect();
        assert!(
            sse.is_empty(),
            "{}: SSE/AVX registers: {sse:?}",
            obj.display()
        );
        let below: Vec<&&str> = insns
            .iter()
            .filter(|l| {
                let Some((_, ops)) = l.split_once('\t') else {
                    return false;
                };
                let ops = ops.trim();
                ops.starts_with("mov")
                    && ops
                        .rsplit(',')
                        .next()
                        .is_some_and(|dst| dst.starts_with("-0x") && dst.ends_with("(%rsp)"))
            })
            .collect();
        assert!(
            below.is_empty(),
            "{}: stores below %rsp (red zone): {below:?}",
            obj.display()
        );
        // `objdump -dr` prints a relocation after the instruction it
        // patches. It names `wolf_trap` on the call itself (the release
        // tier's `call` + PLT32) or on the load of the hook's GOT slot
        // (`mov …(%rip),%rax` then `call *%rax`, the native tier's PIC
        // form): either way the hook's call is found, and the
        // instruction after it must be `ud2`.
        let code: Vec<&str> = insns
            .iter()
            .copied()
            .filter(|l| !l.contains("R_X86_64"))
            .collect();
        let check = |call: usize| {
            let next = code.get(call + 1).copied().unwrap_or("");
            assert!(
                next.contains("ud2"),
                "{}: `wolf_trap` must not return — ud2 follows `{}`; got `{next}`",
                obj.display(),
                code[call].trim()
            );
        };
        let mut calls = 0;
        let mut pending = false;
        let mut at: Option<usize> = None;
        for l in &insns {
            if l.contains("R_X86_64") {
                if l.contains("wolf_trap") {
                    match at {
                        Some(i) if code[i].contains("call") => {
                            check(i);
                            calls += 1;
                        }
                        _ => pending = true,
                    }
                }
                continue;
            }
            let i = at.map_or(0, |i| i + 1);
            at = Some(i);
            if pending && l.contains("call") {
                check(i);
                calls += 1;
                pending = false;
            }
        }
        let (undef, _) = symbols(obj);
        if undef.contains("wolf_trap") {
            assert!(
                calls > 0,
                "{}: a wolf_trap import with no call site",
                obj.display()
            );
        }
    }

    /// Row 4: `noredzone` and the no-SSE features on every define, the
    /// freestanding triple.
    fn assert_release_ir(src: &Path) {
        let dir = src.parent().unwrap();
        let name = src.file_name().unwrap().to_str().unwrap();
        let out = wolf_in(
            dir,
            &[
                "build",
                name,
                "--target",
                TARGET,
                "--release",
                "--emit=llvm-ir",
                "-o",
                "k.ll",
            ],
        );
        assert!(
            out.status.success(),
            "--emit=llvm-ir: {}",
            text(&out.stderr)
        );
        let ir = std::fs::read_to_string(dir.join("k.ll")).expect("read IR");
        assert!(
            ir.contains("target triple = \"x86_64-unknown-none-elf\""),
            "the freestanding triple: {}",
            ir.lines()
                .find(|l| l.starts_with("target triple"))
                .unwrap_or("(none)")
        );
        let defines: Vec<&str> = ir.lines().filter(|l| l.starts_with("define ")).collect();
        assert!(!defines.is_empty(), "the IR defines functions");
        for d in &defines {
            assert!(d.contains("noredzone"), "no `noredzone`: {d}");
            assert!(d.contains(FEATURES), "no no-SSE target-features: {d}");
            assert!(
                d.contains("\"frame-pointer\"=\"all\""),
                "frame pointers kept: {d}"
            );
        }
    }

    #[test]
    fn step2_objects_on_both_tiers() {
        for tier in ["native", "release"] {
            let k = staged(&format!("step2_kmain_{tier}"), "kmain.lu");
            let obj = build_obj(&k, tier);
            assert_step2_symbols(&obj, &["kw_outb"]);
            assert_disassembly(&obj);
            let t = staged(&format!("step2_trap_{tier}"), "kmain_trap.lu");
            let obj = build_obj(&t, tier);
            assert_step2_symbols(&obj, &["kw_outb", "kw_big"]);
            assert!(
                symbols(&obj).0.contains("wolf_trap"),
                "{tier}: the trap hook"
            );
            assert_disassembly(&obj);
            let w = staged(&format!("step2_wide_{tier}"), "kmain_wide.lu");
            let obj = build_obj(&w, tier);
            assert_step2_symbols(&obj, &[]);
            assert_disassembly(&obj);
            if tier == "release" {
                assert_release_ir(&k);
                assert_release_ir(&t);
                assert_release_ir(&w);
            }
        }
    }

    /// Assemble the gate's boot stub and hook stub once per test dir.
    fn stubs(dir: &Path) -> (PathBuf, PathBuf) {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let mut out = Vec::new();
        for s in ["start.S", "rt_stub.S"] {
            let o = dir.join(s.replace(".S", ".o"));
            let r = tool(
                &cc,
                &[
                    "-c",
                    fixture(s).to_str().unwrap(),
                    "-o",
                    o.to_str().unwrap(),
                ],
            );
            assert!(r.status.success(), "{cc} -c {s}: {}", text(&r.stderr));
            out.push(o);
        }
        (out[0].clone(), out[1].clone())
    }

    /// Link with no libc and no wolf runtime: `ld.lld` (the toolchain's
    /// linker, D1), else the system `ld`.
    fn link(objs: &[&Path], exe: &Path) {
        let mut args: Vec<String> = vec!["-static".into(), "-nostdlib".into(), "-o".into()];
        args.push(exe.to_str().unwrap().into());
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
    fn linked_without_libc_the_kernels_run_on_both_tiers() {
        for tier in ["native", "release"] {
            let k = staged(&format!("run_kmain_{tier}"), "kmain.lu");
            let dir = k.parent().unwrap();
            let obj = build_obj(&k, tier);
            let (start, stub) = stubs(dir);
            let exe = dir.join("kmain.elf");
            link(&[&start, &stub, &obj], &exe);
            let out = tool(exe.to_str().unwrap(), &[]);
            assert_eq!(
                text(&out.stdout),
                "KWC\n",
                "{tier}: the kernel's port writes"
            );
            assert_eq!(
                out.status.code(),
                Some(33),
                "{tier}: kmain's result is the status"
            );

            let t = staged(&format!("run_trap_{tier}"), "kmain_trap.lu");
            let dir = t.parent().unwrap();
            let obj = build_obj(&t, tier);
            let (start, stub) = stubs(dir);
            let exe = dir.join("kmain_trap.elf");
            link(&[&start, &stub, &obj], &exe);
            let out = tool(exe.to_str().unwrap(), &[]);
            let src = std::fs::read_to_string(&t).unwrap();
            let line = src
                .lines()
                .position(|l| l.contains("TRAP-SITE"))
                .expect("the fixture marks its trap site")
                + 1;
            let stdout = text(&out.stdout);
            assert!(
                stdout.starts_with("TRAP 1 ") && stdout.ends_with("kmain_trap.lu\n"),
                "{tier}: the overflow (kind 1) reaches wolf_trap with its file: {stdout:?}"
            );
            assert_eq!(
                out.status.code(),
                Some(line as i32),
                "{tier}: wolf_trap receives the site's line ({line}); stdout {stdout:?}"
            );
        }
    }
}
