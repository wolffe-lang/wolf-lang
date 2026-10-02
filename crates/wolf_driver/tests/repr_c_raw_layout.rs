//! kw01 (K4, STATUS #31 — wolf-lang#523): a `#[repr(c)]` struct stored
//! or loaded through a raw pointer has the C layout, held to a real C
//! compiler in both directions on both compiling tiers
//! (`[abi.layout.c]`).
//!
//! At trunk 12a56b22 the raw tier laid a pointee out with the packed
//! spill layout (`flat_offsets`): `{u8, u32, u8}` at offsets 0 1 5,
//! stride 6, where C has 0 4 8, size 12 — a silent wrong answer at the
//! C membrane on native and release, while the checked machine and
//! lupin refuse the aggregate store by name. kw00 found it with a
//! byte dump (`kw00-probes/f4_layout.lu`); this gate asks C itself:
//!
//! 1. **wolf writes, C reads.** A wolf program stores two `C3` values
//!    through `*C3` and prints the 24 bytes; a C program copies those
//!    bytes into `struct C3[2]` and prints the fields, `sizeof` and
//!    `offsetof`. They must be wolf's values.
//! 2. **C writes, wolf reads.** A C program prints the image of two
//!    `struct C3` values (padding zeroed); a wolf program lays those
//!    bytes down through `*u8` and loads `p[0]` and `p[1]` through
//!    `*C3`. The fields must be C's values.
//!
//! The checked machine and lupin answer `unsupported` for the store,
//! by name — the posture kw00 set for a whole-aggregate raw access
//! until the checked machine has a byte-level aggregate; the gate
//! asserts the refusal so a machine that starts guessing goes red.
//!
//! The C compiler is `$CC`, else `cc`. A host with none is a loud skip
//! on windows (no C toolchain on the runner) and a failure elsewhere.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const C_SRC: &str = r#"#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct C3 { uint8_t a; uint32_t b; uint8_t c; };

int main(int argc, char **argv) {
    struct C3 v[2];
    if (argc == 2 && strcmp(argv[1], "write") == 0) {
        memset(v, 0, sizeof v);
        v[0].a = 0x11; v[0].b = 0xdeadbeefu; v[0].c = 0x22;
        v[1].a = 0x44; v[1].b = 7; v[1].c = 0x55;
        const unsigned char *q = (const unsigned char *)v;
        for (size_t i = 0; i < sizeof v; i++) printf("%s%u", i ? " " : "", q[i]);
        printf("\n");
        return 0;
    }
    unsigned char buf[sizeof v];
    if ((size_t)(argc - 1) != sizeof buf) {
        printf("expected %zu bytes, got %d\n", sizeof buf, argc - 1);
        return 3;
    }
    for (size_t i = 0; i < sizeof buf; i++) buf[i] = (unsigned char)strtoul(argv[i + 1], 0, 10);
    memcpy(v, buf, sizeof v);
    printf("sizeof=%zu off=%zu,%zu,%zu\n", sizeof(struct C3), offsetof(struct C3, a),
           offsetof(struct C3, b), offsetof(struct C3, c));
    for (int k = 0; k < 2; k++) printf("%u %u %u\n", v[k].a, v[k].b, v[k].c);
    return 0;
}
"#;

const STRUCT: &str = "#[repr(c)]\nstruct C3 {\n    a: u8,\n    b: u32,\n    c: u8,\n}\n";

/// Direction 1's wolf half: two stores through `*C3`, then the bytes.
fn writer_src() -> String {
    format!(
        "import c \"stdlib.h\"\n\n{STRUCT}\nfn main() -> int {{\n    \
         // # Safety: 32 zeroed bytes; two 12-byte elements and 24 byte reads stay inside; freed once.\n    \
         unsafe {{\n        \
         let p = c.calloc(1, 32) as *C3\n        \
         p[0] = C3 {{ a: 17, b: 3735928559, c: 34 }}\n        \
         p[1] = C3 {{ a: 68, b: 7, c: 85 }}\n        \
         let q = p as *u8\n        \
         var i = 0\n        \
         var s = \"\"\n        \
         while i < 24 {{\n            \
         if i > 0 {{\n                s = s + \" \"\n            }}\n            \
         s = s + \"{{q[i] as int}}\"\n            \
         i += 1\n        \
         }}\n        \
         print(s)\n        \
         c.free(q)\n    \
         }}\n    \
         0\n}}\n"
    )
}

/// Direction 2's wolf half: C's bytes laid down through `*u8`, read
/// back through `*C3`.
fn reader_src(bytes: &[u8]) -> String {
    let stores: String = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| format!("        q[{i}] = {b}\n"))
        .collect();
    format!(
        "import c \"stdlib.h\"\n\n{STRUCT}\nfn main() -> int {{\n    \
         // # Safety: 32 zeroed bytes; 24 byte stores and two 12-byte loads stay inside; freed once.\n    \
         unsafe {{\n        \
         let q = c.calloc(1, 32) as *u8\n\
         {stores}        \
         let p = q as *C3\n        \
         let v = p[0]\n        \
         let w = p[1]\n        \
         print(\"{{v.a as int}} {{v.b as int}} {{v.c as int}}\")\n        \
         print(\"{{w.a as int}} {{w.b as int}} {{w.c as int}}\")\n        \
         c.free(q)\n    \
         }}\n    \
         0\n}}\n"
    )
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("repr_c_raw_layout").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// The C side, compiled once per test; `None` is the windows skip.
fn c_program() -> Option<PathBuf> {
    let dir = scratch("c");
    let src = dir.join("c3.c");
    std::fs::write(&src, C_SRC).expect("write c3.c");
    let exe = dir.join("c3");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    match Command::new(&cc).arg(&src).arg("-o").arg(&exe).output() {
        Ok(out) => {
            assert!(
                out.status.success(),
                "{cc} failed on the C half: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            Some(exe)
        }
        Err(e) if cfg!(windows) => {
            eprintln!("SKIP: no C compiler on this host ({cc}: {e}) — the C membrane witness needs one");
            None
        }
        Err(e) => panic!("no C compiler ({cc}: {e}): the C membrane witness needs one on this host"),
    }
}

/// One lane's record: (verdict, stdout, stderr). `None` is a loud skip
/// (environment exit 2, never an ICE; the release tier's host refusal).
fn lane(src: &Path, flag: &str) -> Option<(String, String, String)> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(src)
        .args([flag, "--json"])
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        eprintln!(
            "SKIP: environment cannot run {flag}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record parses");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if rec["verdict"] == "unsupported" && stderr.contains("release tier targets") {
        eprintln!("SKIP: the release tier refuses this host");
        return None;
    }
    Some((
        rec["verdict"].as_str().unwrap_or("").to_string(),
        rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        stderr,
    ))
}

fn write_prog(dir: &Path, src: &str) -> PathBuf {
    let p = dir.join("prog.lu");
    std::fs::write(&p, src).expect("write the wolf half");
    p
}

/// The sibling lupin, found as `pairing.rs` finds it.
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

/// Direction 1: what wolf stores through `*C3` is what C reads as
/// `struct C3[2]`, on native and on release.
#[test]
fn c_reads_the_struct_wolf_wrote() {
    let Some(c3) = c_program() else { return };
    let dir = scratch("wolf_writes");
    let prog = write_prog(&dir, &writer_src());
    for flag in ["--native", "--release"] {
        let Some((verdict, stdout, stderr)) = lane(&prog, flag) else {
            continue;
        };
        assert_eq!(verdict, "exit(0)", "the {flag} lane runs the writer: {stderr}");
        let bytes: Vec<&str> = stdout.split_whitespace().collect();
        let out = Command::new(&c3).args(&bytes).output().expect("the C half runs");
        let c_says = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            c_says,
            "sizeof=12 off=0,4,8\n17 3735928559 34\n68 7 85\n",
            "the {flag} lane's `*C3` stores, read by C as `struct C3[2]` (wolf-lang#523: \
             before kw01 they were packed, 0 1 5, stride 6); wolf printed {stdout:?}"
        );
    }
}

/// Direction 2: what C lays down as `struct C3[2]` is what wolf loads
/// through `*C3`, on native and on release.
#[test]
fn wolf_reads_the_struct_c_wrote() {
    let Some(c3) = c_program() else { return };
    let out = Command::new(&c3).arg("write").output().expect("the C half runs");
    assert!(out.status.success());
    let image: Vec<u8> = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .map(|b| b.parse().expect("a byte"))
        .collect();
    assert_eq!(image.len(), 24, "C's sizeof(struct C3[2])");
    let dir = scratch("c_writes");
    let prog = write_prog(&dir, &reader_src(&image));
    for flag in ["--native", "--release"] {
        let Some((verdict, stdout, stderr)) = lane(&prog, flag) else {
            continue;
        };
        assert_eq!(verdict, "exit(0)", "the {flag} lane runs the reader: {stderr}");
        assert_eq!(
            stdout, "17 3735928559 34\n68 7 85\n",
            "the {flag} lane's `*C3` loads of C's image {image:?} (wolf-lang#523: before \
             kw01 `b` was read from offset 1)"
        );
    }
}

/// The checked machine and lupin refuse the aggregate store by name.
#[test]
fn the_modelling_machines_refuse_the_aggregate_store_by_name() {
    let dir = scratch("machines");
    let prog = write_prog(&dir, &writer_src());
    let (verdict, _, stderr) = lane(&prog, "--checked").expect("the checked lane always runs");
    assert_eq!(
        verdict, "unsupported",
        "the CHECKED lane on a whole `#[repr(c)]` store through a raw pointer: it has no \
         byte-level aggregate, so it must refuse by name, never guess a layout: {stderr}"
    );
    assert!(
        stderr.contains("raw write of a non-scalar"),
        "the checked lane's refusal names the construct: {stderr}"
    );
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN)");
        return;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(&prog)
        .arg("--json")
        .output()
        .expect("lupin runs");
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("lupin's record");
    assert_eq!(
        rec["verdict"], "unsupported",
        "lupin on a whole `#[repr(c)]` store through a raw pointer refuses by name: {rec}"
    );
}

/// The corpus row is the native lane's truth too (`xtask corpus` runs
/// it there); the release lane must print the same bytes.
#[test]
fn the_corpus_row_agrees_on_both_compiling_tiers() {
    let row = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/memory/raw_repr_c_layout.lu");
    assert!(row.is_file(), "corpus row missing: {}", row.display());
    let want = "1 0 0 0 2 0 0 0 3 0 0 0 4 0 0 0 5 0 0 0 6 0 0 0\n17 3735928559 34\n";
    for flag in ["--native", "--release"] {
        let Some((verdict, stdout, stderr)) = lane(&row, flag) else {
            continue;
        };
        assert_eq!(verdict, "exit(0)", "the {flag} lane on the row: {stderr}");
        assert_eq!(stdout, want, "the {flag} lane's bytes on the row (wolf-lang#523)");
    }
}
