//! s209 (ruling #36 = A, wolf-lang#574): O12, the optimization row L4
//! licenses (`[mem.unsafe.raw.4]`), held in the release tier's IR. Every
//! ordinary raw access is emitted at its pointee's natural alignment —
//! `align 2` / `4` / `8` on an `i16` / `i32` / `i64` load or store — and
//! a packed struct's field at the alignment its offset guarantees
//! (`align 1` for a `u64` at offset 2, `[abi.layout.packed]`). The
//! ruling keeps the compiled tiers exactly as they were: the checked
//! machine and lupin report the row, the compiling tiers assume the
//! alignment. A tier that started emitting `align 1` for every raw
//! access (ruling #36's option B) would turn this gate red.
//!
//! The functions are `tests/fixtures/raw_align/align.lu`'s; the mid-end
//! may inline them into `main`, so the gate counts the accesses of the
//! whole module (the runtime is a static library, never in this IR).
//! The second row is the corpus's own misaligned `*u32` read: its IR
//! still says `align 4` — the address is wrong, not the claim.
//!
//! A failed build fails the gate; there is no skip in this file.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// `src` alone in a scratch directory (D32: a directory is one module),
/// built `--release --emit=llvm-ir`; the IR's text.
fn release_ir(case: &str, src: &Path) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("raw_align_ir")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let name = src.file_name().expect("a file name");
    std::fs::copy(src, dir.join(name)).expect("copy the source");
    let ll = dir.join("out.ll");
    let out = Command::new(wolf())
        .current_dir(&dir)
        .arg("build")
        .arg(name)
        .args(["--release", "--emit=llvm-ir", "-o"])
        .arg(&ll)
        .output()
        .expect("wolf runs");
    assert!(
        out.status.success() && ll.is_file(),
        "wolf build {} --release --emit=llvm-ir must succeed (exit {:?}) — a refusal is the \
         gate failing, never a skip:\n{}",
        src.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(&ll).expect("read the IR")
}

/// Every `load`/`store` of an `i16`, `i32` or `i64` in the IR, as
/// `"load i64 align 8"`, sorted.
fn accesses(ir: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in ir.lines() {
        let l = line.trim_start();
        let l = l.split_once(" = ").map_or(l, |(_, rhs)| rhs);
        for op in ["load", "store"] {
            let Some(rest) = l.strip_prefix(op).and_then(|r| r.strip_prefix(' ')) else {
                continue;
            };
            let ty = rest.split([' ', ',']).next().unwrap_or("");
            if !matches!(ty, "i16" | "i32" | "i64") {
                continue;
            }
            let align = rest
                .split(", align ")
                .nth(1)
                .and_then(|a| a.split([',', ' ']).next())
                .unwrap_or("?");
            out.push(format!("{op} {ty} align {align}"));
        }
    }
    out.sort();
    out
}

fn fixture() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/raw_align/align.lu");
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// O12: `*u16`, `*u32`, `*u64` loads and stores at their natural
/// alignment, the packed `u64` field at `align 1`, nothing else.
#[test]
fn release_ir_keeps_the_natural_alignment_of_every_raw_access() {
    let ir = release_ir("fixture", &fixture());
    let mut want: Vec<String> = [
        "store i16 align 2",
        "store i32 align 4",
        "store i64 align 8",
        "load i16 align 2",
        "load i32 align 4",
        "load i64 align 8",
        // `g[i].base`: a packed `u64` at offset 2 — `[abi.layout.packed]`.
        "load i64 align 1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    want.sort();
    assert_eq!(
        accesses(&ir),
        want,
        "[mem.unsafe.raw.4] O12: the release IR's raw accesses and their alignment"
    );
}

/// The corpus's misaligned `*u32` read (row L4 on the checked machine)
/// compiles to a load that still claims `align 4`: the compiled tiers do
/// not weaken the claim for an address they cannot prove.
#[test]
fn a_misaligned_row_still_claims_the_natural_alignment() {
    let row = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory/raw_ub_misaligned_u32_read.lu");
    assert!(row.is_file(), "corpus row missing: {}", row.display());
    let ir = release_ir("misaligned_u32_read", &row);
    let got = accesses(&ir);
    assert!(
        got.iter().any(|a| a == "load i32 align 4"),
        "the misaligned *u32 read is a `load i32 … align 4`: {got:?}"
    );
    assert!(
        got.iter().all(|a| a.ends_with(" align 4") || !a.contains(" i32 ")),
        "no i32 access weakened below its natural alignment: {got:?}"
    );
}

/// The instrument: the parser reads LLVM's two access spellings.
#[test]
fn the_instrument_reads_both_access_spellings() {
    let ir = "  %t10 = load i32, ptr %t9, align 4, !alias.scope !2\n\
              \x20 store i16 513, ptr %t2, align 2, !alias.scope !2\n\
              \x20 %t3 = load i64, ptr %t2, align 1\n\
              \x20 store i8 1, ptr %t2, align 1\n\
              \x20 %p = load ptr, ptr %t2, align 8\n";
    assert_eq!(
        accesses(ir),
        vec!["load i32 align 4", "load i64 align 1", "store i16 align 2"]
    );
}
