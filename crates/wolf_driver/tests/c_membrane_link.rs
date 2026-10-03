//! kw02 (wolf-lang#513, #521, #514; STATUS #31 K9(b) = B, #32 R1): a
//! wolf function is called from C, and calls C, by the C ABI on both
//! compiling tiers, with `*T` crossing in both directions
//! (`[abi.c.export]`, `[abi.c.import]`, `[abi.c.types]`).
//!
//! Each fixture under `fixtures/c_membrane/` is built `--emit=obj` on
//! the native and the release tier and linked by this gate with its C
//! half (compiled by `$CC`, else `cc`) and `libwolf_rt.a`, exactly as a
//! C program embedding wolf objects would link them:
//!
//! 1. **C calls wolf** (`export.lu` + `export_harness.c`). wolf's `main`
//!    calls the C driver `kw_c_main`, which calls 21 `export fn`s by
//!    their C prototypes — eight mixed-width integers (two on the stack
//!    under SysV), nine floats (one on the stack), mixed integer/float
//!    positions, narrow and `bool` and `u64` results, `#[repr(c)]`
//!    structs by value in every class (40-byte MEMORY, `{i32, f32}`,
//!    `{f64, f64}`, `{u8, u32, u8}`) both ways, a `*Big` read and written
//!    back, a `*u8` string — and checks each value against C's own
//!    computation. Every export is a global symbol under its own name in
//!    both tiers' objects, `kx_unused` (no wolf caller) included.
//! 2. **wolf calls C** (`import.lu` + `import_callee.c`). Fourteen
//!    bodyless `extern "c" fn`s of the same shapes; two C callees write
//!    through wolf pointers (a `calloc`'d `*u8` and a `*Big`) and wolf
//!    reads the writes back. The narrow arguments are the ones clang's
//!    callees read as already extended (kasumi measured `kc_ints` wrong
//!    under clang and right under gcc before the C plan extended them).
//!
//! At trunk 10a16d87 neither fixture built (E1302 on the pointer
//! parameters); with the pointers removed, `export fn` was a mangled,
//! wolf-convention symbol and `--release` dropped it, and a call into a
//! hand-declared extern was refused at codegen (kasumi,
//! `~/lanes/kw02/evidence/cwit-trunk.log`).
//!
//! The link is unix-shaped (`cc`, the platform's pthread/dl/m set), so
//! windows is a loud skip: its C plan is the s60a BRING-UP (aggregates
//! by value refused by shape) and its link is a COFF linker's.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/c_membrane")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `libwolf_rt.a` beside the test's `wolf` (see `repr_c_raw_layout.rs`:
/// without it every native lane is an environment skip — a vacuous
/// green).
fn rt_staticlib() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
    let rt = Path::new(wolf())
        .parent()
        .expect("wolf has a directory")
        .join("libwolf_rt.a");
    assert!(rt.is_file(), "libwolf_rt.a beside wolf: {}", rt.display());
    rt
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("c_membrane_link")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn cc() -> String {
    std::env::var("CC").unwrap_or_else(|_| "cc".to_string())
}

/// Build `src` on one tier into `dir` as relocatable objects; `None` is
/// the release tier's host refusal (a loud skip). Anything else that
/// stops the build fails the gate: `wolf build` exits 2 for a compile
/// error too, and at trunk 10a16d87 that exit read as an environment
/// skip and both witnesses passed vacuously (kasumi,
/// `evidence/red-0fece716-gcc.log`, four `SKIP` lines naming E1302).
fn wolf_objects(src: &Path, tier: &str, dir: &Path) -> Option<Vec<PathBuf>> {
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(src)
        .arg("--emit=obj")
        .arg("-o")
        .arg(dir.join("prog.o"));
    if tier == "release" {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let refusal = lane_exit::environment_refusal(&out, &format!("wolf build --emit=obj ({tier})"));
    if refusal && tier == "release" && stderr.contains("release tier targets") {
        eprintln!(
            "SKIP: the release tier refuses this host: {}",
            stderr.trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "wolf build --emit=obj ({tier}) on {} must build — a compile error or an \
         `unsupported` here is the witness failing, never a skip: {stderr}",
        src.display()
    );
    let mut objs: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read the object dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "o") && p.is_file())
        .collect();
    objs.sort();
    assert!(!objs.is_empty(), "no object written for {tier}");
    Some(objs)
}

/// The global symbols the objects DEFINE (`nm -g`, text section), with
/// the Mach-O leading underscore removed.
fn defined_text_symbols(objs: &[PathBuf]) -> Vec<String> {
    let out = Command::new("nm")
        .arg("-g")
        .args(objs)
        .output()
        .expect("nm runs");
    assert!(out.status.success(), "nm failed");
    let mut syms: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace().rev();
            let name = f.next()?;
            let kind = f.next()?;
            (kind == "T").then(|| {
                if cfg!(target_os = "macos") {
                    name.strip_prefix('_').unwrap_or(name).to_string()
                } else {
                    name.to_string()
                }
            })
        })
        .collect();
    syms.sort();
    syms.dedup();
    syms
}

/// Compile the C half, link it with the wolf objects and the runtime,
/// run the program: (exit code, stdout).
fn link_and_run(objs: &[PathBuf], c_src: &Path, dir: &Path) -> (i32, String) {
    let c_obj = dir.join("c_half.o");
    let out = Command::new(cc())
        .args(["-c", "-O1", "-ffp-contract=off"])
        .arg(c_src)
        .arg("-o")
        .arg(&c_obj)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "no C compiler ({}: {e}): the membrane witness needs one",
                cc()
            )
        });
    assert!(
        out.status.success(),
        "{} on {}: {}",
        cc(),
        c_src.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = dir.join("prog");
    let mut link = Command::new(cc());
    link.arg("-o")
        .arg(&exe)
        .args(objs)
        .arg(&c_obj)
        .arg(rt_staticlib());
    if cfg!(target_os = "macos") {
        link.args(["-lpthread", "-lm"]);
    } else {
        link.args(["-lpthread", "-ldl", "-lm"]);
    }
    let out = link.output().expect("the C driver links");
    assert!(
        out.status.success(),
        "linking the wolf objects with {}: {}",
        c_src.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("the program runs");
    (
        run.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&run.stdout).into_owned(),
    )
}

const EXPORTS: &[&str] = &[
    "kx_big_d",
    "kx_big_make",
    "kx_big_ptr",
    "kx_big_sum",
    "kx_c3_make",
    "kx_c3_sum",
    "kx_floats",
    "kx_floats32",
    "kx_half",
    "kx_inc16",
    "kx_ints",
    "kx_mix_swap",
    "kx_mixed",
    "kx_mixed32",
    "kx_neg8",
    "kx_pos",
    "kx_strlen",
    "kx_two_dot",
    "kx_two_make",
    "kx_u64",
    "kx_unused",
];

/// Direction 1: C calls every export and every value checks, on both
/// tiers; every export is a plain global symbol in both tiers' objects.
#[test]
fn c_calls_every_export_on_both_tiers() {
    if cfg!(windows) {
        eprintln!("SKIP: the C-membrane link witness is unix-shaped (win64 is s60a's bring-up)");
        return;
    }
    rt_staticlib();
    for tier in ["native", "release"] {
        let dir = scratch(&format!("export-{tier}"));
        let Some(objs) = wolf_objects(&fixture("export.lu"), tier, &dir) else {
            continue;
        };
        let syms = defined_text_symbols(&objs);
        for want in EXPORTS {
            assert!(
                syms.iter().any(|s| s == want),
                "the {tier} tier's objects define `{want}` unmangled and global \
                 ([abi.c.export]; wolf-lang#513: before kw02 native mangled it and release \
                 dropped it); defined: {syms:?}"
            );
        }
        let (code, stdout) = link_and_run(&objs, &fixture("export_harness.c"), &dir);
        assert!(
            !stdout.contains("BAD"),
            "the {tier} tier: a C caller saw a wrong value through the membrane:\n{stdout}"
        );
        assert!(
            stdout.ends_with("x1 checks ok=36 bad=0\n"),
            "the {tier} tier: every check ran and held:\n{stdout}"
        );
        assert_eq!(code, 0, "the {tier} tier's exit status:\n{stdout}");
    }
}

/// Direction 2: wolf calls fourteen C functions, two of which write
/// through wolf pointers, on both tiers.
#[test]
fn wolf_calls_c_on_both_tiers() {
    if cfg!(windows) {
        eprintln!("SKIP: the C-membrane link witness is unix-shaped (win64 is s60a's bring-up)");
        return;
    }
    rt_staticlib();
    let want = "ints 4788999889595\nneg8 100\npos ok\nbig_make -7 99 -300 200\n\
                big_sum 2469135780339\nmix 42\nc3 17 3735928559 18\nfill 376 46\n\
                big_write 5 77 -2 250\nstrlen 8\nx2 float-and-bool misses 0\n";
    for tier in ["native", "release"] {
        let dir = scratch(&format!("import-{tier}"));
        let Some(objs) = wolf_objects(&fixture("import.lu"), tier, &dir) else {
            continue;
        };
        let (code, stdout) = link_and_run(&objs, &fixture("import_callee.c"), &dir);
        assert_eq!(
            stdout, want,
            "the {tier} tier: what C returned and wrote, as wolf read it ([abi.c.import]; \
             wolf-lang#521)"
        );
        assert_eq!(code, 0, "the {tier} tier's exit status");
    }
}
