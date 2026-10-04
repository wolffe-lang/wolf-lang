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
//! on windows (where a runner may have no C toolchain) and a failure
//! elsewhere; the windows runner has one, so the witness runs there too.

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

/// The native and release lanes link `libwolf_rt.a` from beside the
/// `wolf` binary; `cargo test -p wolf_driver` does not build it, and
/// without it both lanes are an environment SKIP — which made this
/// gate green at trunk, vacuously, the first time it ran (kasumi,
/// `red-gate-trunk-12a56b22-cc.log`). Build it first, as
/// `release_struct_layout.rs` does.
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

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("repr_c_raw_layout")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// The C side, compiled once per test into its own directory (two
/// tests sharing one executable race: "Text file busy"); `None` is the
/// windows skip.
fn c_program(tag: &str) -> Option<PathBuf> {
    let dir = scratch(&format!("c-{tag}"));
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
            eprintln!(
                "SKIP: no C compiler on this host ({cc}: {e}) — the C membrane witness needs one"
            );
            None
        }
        Err(e) => {
            panic!("no C compiler ({cc}: {e}): the C membrane witness needs one on this host")
        }
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
    ensure_rt_staticlib();
    let Some(c3) = c_program("read") else { return };
    let dir = scratch("wolf_writes");
    let prog = write_prog(&dir, &writer_src());
    for flag in ["--native", "--release"] {
        let Some((verdict, stdout, stderr)) = lane(&prog, flag) else {
            continue;
        };
        assert_eq!(
            verdict, "exit(0)",
            "the {flag} lane runs the writer: {stderr}"
        );
        let bytes: Vec<&str> = stdout.split_whitespace().collect();
        let out = Command::new(&c3)
            .args(&bytes)
            .output()
            .expect("the C half runs");
        // A C program's text-mode stdout ends lines with `\r\n` on
        // windows (CI run 37087868222 found a `cc` there); the layout
        // is what is compared, not the line ending.
        let c_says = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(
            c_says, "sizeof=12 off=0,4,8\n17 3735928559 34\n68 7 85\n",
            "the {flag} lane's `*C3` stores, read by C as `struct C3[2]` (wolf-lang#523: \
             before kw01 they were packed, 0 1 5, stride 6); wolf printed {stdout:?}"
        );
    }
}

/// Direction 2: what C lays down as `struct C3[2]` is what wolf loads
/// through `*C3`, on native and on release.
#[test]
fn wolf_reads_the_struct_c_wrote() {
    ensure_rt_staticlib();
    let Some(c3) = c_program("write") else { return };
    let out = Command::new(&c3)
        .arg("write")
        .output()
        .expect("the C half runs");
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
        assert_eq!(
            verdict, "exit(0)",
            "the {flag} lane runs the reader: {stderr}"
        );
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
    ensure_rt_staticlib();
    let row =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/memory/raw_repr_c_layout.lu");
    assert!(row.is_file(), "corpus row missing: {}", row.display());
    let want = "1 0 0 0 2 0 0 0 3 0 0 0 4 0 0 0 5 0 0 0 6 0 0 0\n17 3735928559 34\n";
    for flag in ["--native", "--release"] {
        let Some((verdict, stdout, stderr)) = lane(&row, flag) else {
            continue;
        };
        assert_eq!(verdict, "exit(0)", "the {flag} lane on the row: {stderr}");
        assert_eq!(
            stdout, want,
            "the {flag} lane's bytes on the row (wolf-lang#523)"
        );
    }
}

/// wolf-lang#542 (found beside #523, same lowering arm): `p[i] op= v`
/// through a raw pointer is a read-modify-write on every lane. At trunk
/// 12a56b22 native and release stored `v` (`5`, `31`, `3`) and the
/// checked machine always added and mis-sized a `*i64` element (`8`,
/// `39`, `0`); lupin 0.1.43 already printed the row's answer.
#[test]
fn a_raw_compound_assignment_applies_its_operator_on_every_lane() {
    ensure_rt_staticlib();
    let row =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/memory/raw_compound_assign.lu");
    assert!(row.is_file(), "corpus row missing: {}", row.display());
    for flag in ["--checked", "--native", "--release"] {
        let Some((verdict, stdout, stderr)) = lane(&row, flag) else {
            continue;
        };
        assert_eq!(verdict, "exit(0)", "the {flag} lane on the row: {stderr}");
        assert_eq!(
            stdout, "8\n248\n12\n",
            "the {flag} lane on `p[i] op= v` (wolf-lang#542)"
        );
    }
}

// ---------------------------------------------------------------------
// kw08 (K4 — `[abi.layout.packed]`, `[abi.layout.align]`,
// `[abi.layout.query]`): every packed and aligned layout, held to gcc
// AND clang in both directions on both compiling tiers, and wolf's
// comptime `size_of` / `align_of` / `offset_of` held to C's `sizeof` /
// `_Alignof` / `offsetof`. At trunk 9bf6a5d5 every wolf half was E0817
// (`packed`, `align(N)` refused by name) or E0301 (no `align_of`,
// `offset_of`), so every shape below was red.
// ---------------------------------------------------------------------

/// One layout under test: its wolf and C declarations (every shape's
/// dependencies included), two element values in each language, the
/// fields printed (a nested one by path) and the top-level fields whose
/// offsets are asked.
struct Shape {
    name: &'static str,
    wolf_decl: &'static str,
    c_decl: &'static str,
    wolf_vals: [&'static str; 2],
    c_vals: [&'static str; 2],
    fields: &'static [&'static str],
    offsets: &'static [&'static str],
    /// The fields' values, one line per element, as both sides print.
    want: &'static str,
}

const A16_W: &str = "#[repr(c, align(16))]\nstruct A16 {\n    x: u32,\n}\n";
const A16_C: &str = "struct __attribute__((aligned(16))) A16 { uint32_t x; };\n";
const P3_W: &str = "#[repr(c, packed)]\nstruct P3 {\n    a: u8,\n    b: u32,\n    c: u8,\n}\n";
const P3_C: &str = "struct __attribute__((packed)) P3 { uint8_t a; uint32_t b; uint8_t c; };\n";

const SHAPES: &[Shape] = &[
    Shape {
        name: "P3",
        wolf_decl: P3_W,
        c_decl: P3_C,
        wolf_vals: [
            "P3 { a: 17, b: 3735928559, c: 34 }",
            "P3 { a: 68, b: 7, c: 85 }",
        ],
        c_vals: ["{17, 3735928559u, 34}", "{68, 7, 85}"],
        fields: &["a", "b", "c"],
        offsets: &["a", "b", "c"],
        want: "17 3735928559 34\n68 7 85\n",
    },
    Shape {
        name: "Gdtr",
        wolf_decl: "#[repr(c, packed)]\nstruct Gdtr {\n    limit: u16,\n    base: u64,\n}\n",
        c_decl: "struct __attribute__((packed)) Gdtr { uint16_t limit; uint64_t base; };\n",
        wolf_vals: [
            "Gdtr { limit: 4095, base: 1311768467463790320 }",
            "Gdtr { limit: 7, base: 4096 }",
        ],
        c_vals: ["{4095, 1311768467463790320ull}", "{7, 4096}"],
        fields: &["limit", "base"],
        offsets: &["limit", "base"],
        want: "4095 1311768467463790320\n7 4096\n",
    },
    Shape {
        name: "A16",
        wolf_decl: A16_W,
        c_decl: A16_C,
        wolf_vals: ["A16 { x: 3735928559 }", "A16 { x: 9 }"],
        c_vals: ["{3735928559u}", "{9}"],
        fields: &["x"],
        offsets: &["x"],
        want: "3735928559\n9\n",
    },
    Shape {
        name: "Outer",
        wolf_decl: "#[repr(c)]\nstruct Outer {\n    tag: u8,\n    inner: A16,\n    z: u16,\n}\n",
        c_decl: "struct Outer { uint8_t tag; struct A16 inner; uint16_t z; };\n",
        wolf_vals: [
            "Outer { tag: 7, inner: A16 { x: 9 }, z: 513 }",
            "Outer { tag: 1, inner: A16 { x: 65537 }, z: 3 }",
        ],
        c_vals: ["{7, {9}, 513}", "{1, {65537}, 3}"],
        fields: &["tag", "inner.x", "z"],
        offsets: &["tag", "inner", "z"],
        want: "7 9 513\n1 65537 3\n",
    },
    Shape {
        name: "PNest",
        wolf_decl: "#[repr(c)]\nstruct PNest {\n    a: u8,\n    p: P3,\n    z: u16,\n}\n",
        c_decl: "struct PNest { uint8_t a; struct P3 p; uint16_t z; };\n",
        wolf_vals: [
            "PNest { a: 5, p: P3 { a: 6, b: 258, c: 7 }, z: 772 }",
            "PNest { a: 1, p: P3 { a: 2, b: 4294967295, c: 4 }, z: 5 }",
        ],
        c_vals: ["{5, {6, 258, 7}, 772}", "{1, {2, 4294967295u, 4}, 5}"],
        fields: &["a", "p.a", "p.b", "p.c", "z"],
        offsets: &["a", "p", "z"],
        want: "5 6 258 7 772\n1 2 4294967295 4 5\n",
    },
    Shape {
        name: "Pf",
        wolf_decl: "#[repr(c, packed)]\nstruct Pf {\n    t: u8,\n    d: f64,\n    h: i16,\n}\n",
        c_decl: "struct __attribute__((packed)) Pf { uint8_t t; double d; int16_t h; };\n",
        wolf_vals: [
            "Pf { t: 9, d: 2.5, h: -2 }",
            "Pf { t: 1, d: -0.125, h: 300 }",
        ],
        c_vals: ["{9, 2.5, -2}", "{1, -0.125, 300}"],
        fields: &["t", "d", "h"],
        offsets: &["t", "d", "h"],
        want: "9 2.5 -2\n1 -0.125 300\n",
    },
];

/// Every declaration the shapes need, in dependency order, once.
fn all_decls(pick: impl Fn(&Shape) -> &'static str, base: [&'static str; 2]) -> String {
    let mut out = String::new();
    for d in base {
        out.push_str(d);
        out.push('\n');
    }
    for s in SHAPES {
        let d = pick(s);
        if !base.contains(&d) {
            out.push_str(d);
            out.push('\n');
        }
    }
    out
}

/// The C half for every shape: `write NAME` prints the bytes of two
/// elements (statically initialized, so padding is zero); `read NAME
/// b0 b1 …` copies the bytes into two elements and prints `sizeof`,
/// `_Alignof`, the asked offsets, then each element's fields.
fn kw08_c_src() -> String {
    let mut s = String::from(
        "#include <stddef.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n\
         #include <string.h>\n\n",
    );
    s.push_str(&all_decls(|sh| sh.c_decl, [A16_C, P3_C]));
    s.push_str(
        "static int bytes_in(int argc, char **argv, unsigned char *buf, size_t n) {\n    \
         if ((size_t)(argc - 3) != n) { printf(\"expected %zu bytes, got %d\\n\", n, argc - 3); return 0; }\n    \
         for (size_t i = 0; i < n; i++) buf[i] = (unsigned char)strtoul(argv[i + 3], 0, 10);\n    \
         return 1;\n}\n\n\
         static void bytes_out(const void *p, size_t n) {\n    \
         const unsigned char *q = (const unsigned char *)p;\n    \
         for (size_t i = 0; i < n; i++) printf(\"%s%u\", i ? \" \" : \"\", q[i]);\n    \
         printf(\"\\n\");\n}\n\n\
         int main(int argc, char **argv) {\n    if (argc < 3) return 2;\n    \
         int w = strcmp(argv[1], \"write\") == 0;\n",
    );
    for sh in SHAPES {
        let n = sh.name;
        s.push_str(&format!(
            "    if (strcmp(argv[2], \"{n}\") == 0) {{\n        \
             static const struct {n} init[2] = {{{}, {}}};\n        \
             struct {n} v[2];\n        \
             if (w) {{ memcpy(v, init, sizeof v); bytes_out(v, sizeof v); return 0; }}\n        \
             unsigned char buf[sizeof v];\n        \
             if (!bytes_in(argc, argv, buf, sizeof buf)) return 3;\n        \
             memcpy(v, buf, sizeof v);\n        \
             printf(\"sizeof=%zu align=%zu off=",
            sh.c_vals[0], sh.c_vals[1]
        ));
        let fmt: Vec<&str> = sh.offsets.iter().map(|_| "%zu").collect();
        s.push_str(&fmt.join(","));
        s.push_str(&format!("\\n\", sizeof(struct {n}), _Alignof(struct {n})"));
        for o in sh.offsets {
            s.push_str(&format!(", offsetof(struct {n}, {o})"));
        }
        s.push_str(");\n        for (int k = 0; k < 2; k++) printf(\"");
        let ffmt: Vec<&str> = sh
            .fields
            .iter()
            .map(|f| {
                if *f == "d" {
                    "%g"
                } else if *f == "h" {
                    "%lld"
                } else {
                    "%llu"
                }
            })
            .collect();
        s.push_str(&ffmt.join(" "));
        s.push_str("\\n\"");
        for f in sh.fields {
            let cast = if *f == "d" {
                "(double)"
            } else if *f == "h" {
                "(long long)"
            } else {
                "(unsigned long long)"
            };
            s.push_str(&format!(", {cast}v[k].{f}"));
        }
        s.push_str(");\n        return 0;\n    }\n");
    }
    s.push_str("    return 4;\n}\n");
    s
}

/// The wolf declarations every shape program carries.
fn wolf_decls() -> String {
    all_decls(|sh| sh.wolf_decl, [A16_W, P3_W])
}

/// How a field interpolates on the wolf side: integers through `int`
/// (every value here fits), the float as itself.
fn wolf_field(var: &str, f: &str) -> String {
    if f == "d" {
        format!("{{{var}.{f}}}")
    } else {
        format!("{{{var}.{f} as int}}")
    }
}

/// Direction 1's wolf half for one shape: the comptime queries in C's
/// header format, then two stores through `*T` and their bytes.
fn kw08_writer(sh: &Shape) -> String {
    let n = sh.name;
    let offs: Vec<String> = sh
        .offsets
        .iter()
        .map(|o| format!("{{offset_of({n}, {o})}}"))
        .collect();
    format!(
        "import c \"stdlib.h\"\n\n{}\nfn main() -> int {{\n    \
         print(\"sizeof={{size_of({n})}} align={{align_of({n})}} off={}\")\n    \
         // # Safety: 256 zeroed bytes; two elements of at most 48 bytes and their bytes stay inside; freed once.\n    \
         unsafe {{\n        \
         let p = c.calloc(1, 256) as *{n}\n        \
         p[0] = {}\n        \
         p[1] = {}\n        \
         let q = p as *u8\n        \
         var i = 0\n        \
         var s = \"\"\n        \
         while i < 2 * size_of({n}) {{\n            \
         if i > 0 {{\n                s = s + \" \"\n            }}\n            \
         s = s + \"{{q[i] as int}}\"\n            \
         i += 1\n        \
         }}\n        \
         print(s)\n        \
         c.free(q)\n    \
         }}\n    \
         0\n}}\n",
        wolf_decls(),
        offs.join(","),
        sh.wolf_vals[0],
        sh.wolf_vals[1],
    )
}

/// Direction 2's wolf half for one shape: C's bytes laid down through
/// `*u8`, both elements loaded through `*T`, their fields printed.
fn kw08_reader(sh: &Shape, bytes: &[u8]) -> String {
    let n = sh.name;
    let stores: String = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| format!("        q[{i}] = {b}\n"))
        .collect();
    let line = |v: &str| -> String {
        let parts: Vec<String> = sh.fields.iter().map(|f| wolf_field(v, f)).collect();
        format!("        print(\"{}\")\n", parts.join(" "))
    };
    format!(
        "import c \"stdlib.h\"\n\n{}\nfn main() -> int {{\n    \
         // # Safety: 256 zeroed bytes; C's image and two element loads stay inside; freed once.\n    \
         unsafe {{\n        \
         let q = c.calloc(1, 256) as *u8\n\
         {stores}        \
         let p = q as *{n}\n        \
         let v = p[0]\n        \
         let w = p[1]\n\
         {}{}        \
         c.free(q)\n    \
         }}\n    \
         0\n}}\n",
        wolf_decls(),
        line("v"),
        line("w"),
    )
}

/// The C compilers this host holds the layouts to: gcc and clang (both
/// required on linux, where CI and kasumi carry both), plus `$CC`.
fn c_compilers() -> Vec<String> {
    let mut ccs: Vec<String> = Vec::new();
    if let Ok(cc) = std::env::var("CC") {
        ccs.push(cc);
    }
    for cc in ["gcc", "clang"] {
        let found = Command::new(cc)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if found {
            if !ccs.iter().any(|c| c == cc) {
                ccs.push(cc.to_string());
            }
        } else if cfg!(target_os = "linux") {
            panic!(
                "no `{cc}` on this linux host: kw08's witness holds every layout to gcc AND clang"
            );
        } else {
            eprintln!(
                "SKIP: no `{cc}` on this host — the layouts are held to the other compiler only"
            );
        }
    }
    if ccs.is_empty() && cfg!(windows) {
        eprintln!("SKIP: no C compiler on this host — the kw08 layout witness needs one");
    }
    assert!(
        !ccs.is_empty() || cfg!(windows),
        "no C compiler: the kw08 layout witness needs gcc or clang"
    );
    ccs
}

/// The kw08 C half compiled by `cc` into its own directory.
fn kw08_c_program(cc: &str, tag: &str) -> PathBuf {
    let dir = scratch(&format!("kw08-c-{tag}-{}", cc.replace(['/', '\\'], "_")));
    let src = dir.join("shapes.c");
    std::fs::write(&src, kw08_c_src()).expect("write shapes.c");
    let exe = dir.join("shapes");
    let out = Command::new(cc)
        .arg("-std=c11")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap_or_else(|e| panic!("{cc} runs: {e}"));
    assert!(
        out.status.success(),
        "{cc} failed on the kw08 C half: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

fn c_says(exe: &Path, args: &[&str]) -> String {
    let out = Command::new(exe)
        .args(args)
        .output()
        .expect("the C half runs");
    assert!(
        out.status.success(),
        "the C half {args:?} exited {:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

/// Direction 1, every shape, both tiers, gcc and clang: C reads the
/// bytes wolf stored through `*T` as the same values, and C's
/// `sizeof`/`_Alignof`/`offsetof` are wolf's comptime answers.
#[test]
fn c_reads_every_packed_and_aligned_layout_wolf_wrote() {
    ensure_rt_staticlib();
    let ccs = c_compilers();
    let exes: Vec<(String, PathBuf)> = ccs
        .iter()
        .map(|cc| (cc.clone(), kw08_c_program(cc, "read")))
        .collect();
    let mut checked = 0;
    for sh in SHAPES {
        let dir = scratch(&format!("kw08-w-{}", sh.name));
        let prog = write_prog(&dir, &kw08_writer(sh));
        for flag in ["--native", "--release"] {
            let Some((verdict, stdout, stderr)) = lane(&prog, flag) else {
                continue;
            };
            assert_eq!(
                verdict, "exit(0)",
                "the {flag} lane runs {}'s writer: {stderr}",
                sh.name
            );
            let mut lines = stdout.lines();
            let header = lines.next().unwrap_or_default().to_string();
            let bytes: Vec<&str> = lines
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .collect();
            for (cc, exe) in &exes {
                let mut args = vec!["read", sh.name];
                args.extend(bytes.iter().copied());
                let said = c_says(exe, &args);
                assert_eq!(
                    said,
                    format!("{header}\n{}", sh.want),
                    "{cc} reading {}'s two elements as wolf's {flag} lane stored them \
                     (first line: C's sizeof/_Alignof/offsetof against wolf's size_of/\
                     align_of/offset_of); wolf printed {stdout:?}",
                    sh.name
                );
                checked += 1;
            }
        }
    }
    eprintln!("kw08: {checked} (shape, tier, compiler) rows held, wolf writes and C reads");
}

/// Direction 2, every shape, both tiers, gcc and clang: wolf loads
/// through `*T` exactly the values C laid down.
#[test]
fn wolf_reads_every_packed_and_aligned_layout_c_wrote() {
    ensure_rt_staticlib();
    let ccs = c_compilers();
    let mut checked = 0;
    for cc in &ccs {
        let exe = kw08_c_program(cc, "write");
        for sh in SHAPES {
            let image: Vec<u8> = c_says(&exe, &["write", sh.name])
                .split_whitespace()
                .map(|b| b.parse().expect("a byte"))
                .collect();
            let dir = scratch(&format!(
                "kw08-r-{}-{}",
                sh.name,
                cc.replace(['/', '\\'], "_")
            ));
            let prog = write_prog(&dir, &kw08_reader(sh, &image));
            for flag in ["--native", "--release"] {
                let Some((verdict, stdout, stderr)) = lane(&prog, flag) else {
                    continue;
                };
                assert_eq!(
                    verdict, "exit(0)",
                    "the {flag} lane runs {}'s reader: {stderr}",
                    sh.name
                );
                assert_eq!(
                    stdout, sh.want,
                    "the {flag} lane loading {}'s two elements from {cc}'s image {image:?}",
                    sh.name
                );
                checked += 1;
            }
        }
    }
    eprintln!("kw08: {checked} (shape, tier, compiler) rows held, C writes and wolf reads");
}

/// The checked machine refuses a whole packed or aligned aggregate
/// through a raw pointer by name, as it does a plain `#[repr(c)]` one.
#[test]
fn the_checked_machine_refuses_a_packed_aggregate_store_by_name() {
    let sh = &SHAPES[0];
    let dir = scratch("kw08-checked");
    let prog = write_prog(&dir, &kw08_writer(sh));
    let (verdict, _, stderr) = lane(&prog, "--checked").expect("the checked lane always runs");
    assert_eq!(verdict, "unsupported", "{stderr}");
    assert!(stderr.contains("raw write of a non-scalar"), "{stderr}");
}

/// The membrane (`[abi.c.types]`, `[abi.layout.align]`): a packed or
/// aligned struct by value is refused by name on both compiling tiers;
/// a pointer to one crosses.
#[test]
fn a_packed_struct_by_value_at_the_membrane_is_refused_by_name() {
    ensure_rt_staticlib();
    let dir = scratch("kw08-membrane");
    let by_value = write_prog(
        &dir,
        &format!(
            "{P3_W}\nexport fn first(p: P3) -> u8 {{\n    p.a\n}}\n\n\
             fn main() -> int {{\n    0\n}}\n"
        ),
    );
    for flag in ["--native", "--release"] {
        let Some((verdict, _, stderr)) = lane(&by_value, flag) else {
            continue;
        };
        assert_eq!(verdict, "unsupported", "the {flag} lane: {stderr}");
        assert!(
            stderr.contains("packed or aligned struct by value") && stderr.contains("`P3`"),
            "the {flag} lane names the struct: {stderr}"
        );
    }
    let dir = scratch("kw08-membrane-ptr");
    let by_ptr = write_prog(
        &dir,
        &format!(
            "{P3_W}\nexport fn first(p: *P3) -> u8 {{\n    \
             // # Safety: the C caller passes a live P3.\n    \
             unsafe {{\n        p[0].a\n    }}\n}}\n\n\
             fn main() -> int {{\n    0\n}}\n"
        ),
    );
    for flag in ["--native", "--release"] {
        let Some((verdict, _, stderr)) = lane(&by_ptr, flag) else {
            continue;
        };
        assert_eq!(verdict, "exit(0)", "a `*P3` crosses on {flag}: {stderr}");
    }
}

/// An aligned struct inside a packed one is E0820 on every machine: the
/// C compilers disagree on its layout by target. Measured in CI run
/// 37177942003 (windows-latest): clang for the MSVC ABI laid
/// `struct __attribute__((packed)) PA { uint8_t a; struct A16 b; uint8_t
/// c; }` out at size 48 with `b` at 16, where gcc and clang on SysV
/// (kasumi, `c-layouts-aligned-in-packed.log`) put `b` at 1, size 18.
#[test]
fn an_aligned_struct_inside_a_packed_one_is_refused_on_every_machine() {
    ensure_rt_staticlib();
    let dir = scratch("kw08-aligned-in-packed");
    let prog = write_prog(
        &dir,
        &format!(
            "{A16_W}\n#[repr(c, packed)]\nstruct PA {{\n    a: u8,\n    b: A16,\n    c: u8,\n}}\n\n\
             #[repr(c)]\nstruct Holds {{\n    t: u8,\n    a: A16,\n}}\n\n\
             #[repr(c, packed)]\nstruct Deep {{\n    t: u8,\n    h: Holds,\n}}\n\n\
             fn main() -> int {{\n    let p = PA {{ a: 1, b: A16 {{ x: 2 }}, c: 3 }}\n    \
             let d = Deep {{ t: 1, h: Holds {{ t: 2, a: A16 {{ x: 3 }} }} }}\n    \
             p.a as int + d.t as int - 2\n}}\n"
        ),
    );
    for flag in ["--checked", "--native", "--release"] {
        let Some((verdict, _, stderr)) = lane(&prog, flag) else {
            continue;
        };
        assert_eq!(verdict, "fail(E0820)", "the {flag} lane: {stderr}");
        assert!(
            stderr.contains("`PA` holds the aligned struct `A16`")
                && stderr.contains("`Deep` holds the aligned struct `A16`"),
            "the {flag} lane names both packed structs, the deep one too: {stderr}"
        );
    }
}
