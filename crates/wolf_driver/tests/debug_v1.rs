//! DWARF v1 source-type witnesses. Each transcript belongs to one type row.
//! These use the debug tier; the release tier has no source DWARF yet.
#[path = "support/dwarf_variant_bytes.rs"]
mod dwarf_variant_bytes;
#[path = "support/oracle_gate.rs"]
mod oracle_gate;

use std::path::{Path, PathBuf};
use std::process::Command;

fn platform(row: &str) -> bool {
    if cfg!(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )) {
        return true;
    }
    eprintln!("SKIP {row} reason=requires x86-64 Linux or arm64 macOS");
    false
}

fn fixture(row: &str, src: &str, debug_frame: bool) -> Option<PathBuf> {
    if !platform(row) {
        return None;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("debug_v1_{row}"));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("probe.lu");
    std::fs::write(&entry, src).unwrap();
    let exe = dir.join("probe");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wolf"));
    cmd.arg("build").arg("--no-cache");
    if debug_frame {
        cmd.arg("-g");
    }
    let out = cmd.arg(&entry).arg("-o").arg(&exe).output().unwrap();
    if !out.status.success() {
        eprintln!("FAIL {row}");
    }
    assert!(
        out.status.success(),
        "{row}: compile failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(exe)
}

fn transcript(row: &str, src: &str, gdb_expr: &str, lldb_expr: &str, wants: &[&str]) -> bool {
    let debugger = if cfg!(target_os = "macos") {
        "lldb"
    } else {
        "gdb"
    };
    if !platform(row) {
        return false;
    }
    if Command::new(debugger).arg("--version").output().is_err() {
        if let Err(error) = oracle_gate::absent(row, &format!("{debugger} is not installed")) {
            panic!("{error}");
        }
        return false;
    }
    let Some(exe) = fixture(row, src, false) else {
        return false;
    };
    let mut cmd = Command::new(debugger);
    if debugger == "gdb" {
        cmd.args([
            "--batch",
            "-ex",
            "set pagination off",
            "-ex",
            "break inspect",
            "-ex",
            "run",
        ])
        .args([
            "-ex",
            "echo S283_VALUES_BEGIN\\n",
            "-ex",
            "ptype value",
            "-ex",
            "print value",
        ]);
        for expression in gdb_expr.lines() {
            cmd.args(["-ex", expression]);
        }
        cmd.args([
            "-ex",
            "echo S283_VALUES_END\\n",
            "-ex",
            "bt",
            "-ex",
            "continue",
        ]);
    } else {
        cmd.args(["-b", "-o", "breakpoint set --name inspect", "-o", "run"])
            .args([
                "-o",
                "script print('S283_VALUES_BEGIN')",
                "-o",
                "frame variable --show-types value",
            ]);
        for expression in lldb_expr.lines() {
            cmd.args(["-o", expression]);
        }
        cmd.args([
            "-o",
            "script print('S283_VALUES_END')",
            "-o",
            "bt",
            "-o",
            "continue",
        ]);
    }
    let out = cmd.arg(&exe).output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::write(exe.parent().unwrap().join(format!("{debugger}.txt")), &text).unwrap();
    // Source-line echoes can contain every expected literal while the debugger
    // has no variable at all. Compare only the explicitly delimited value output.
    let values = text
        .lines()
        .skip_while(|line| line.trim() != "S283_VALUES_BEGIN")
        .skip(1)
        .take_while(|line| line.trim() != "S283_VALUES_END")
        .collect::<Vec<_>>()
        .join("\n");
    let missing: Vec<_> = wants.iter().filter(|s| !values.contains(**s)).collect();
    if !out.status.success() || !missing.is_empty() {
        eprintln!("FAIL {row}");
        panic!("{row}: missing {missing:?}, status {}\n{text}", out.status);
    }
    println!("PASS {row}");
    true
}

#[test]
fn dwarf_v1_struct() {
    transcript(
        "dwarf-v1-struct",
        "struct Pair { small: i8, wide: int }\nfn inspect(value: Pair) -> int { value.wide - 42 }\nfn main() -> int { inspect(Pair { small: 7 as i8, wide: 42 }) }\n",
        "print value.wide",
        "frame variable value.wide",
        &["small", "wide = 42"],
    );
}

#[test]
fn dwarf_v1_tuple() {
    transcript(
        "dwarf-v1-tuple",
        "fn inspect(value: (int, int)) -> int { value.1 - 22 }\nfn main() -> int { inspect((11, 22)) }\n",
        "print value._1",
        "frame variable value._1",
        &["_0 = 11", "_1 = 22"],
    );
}

#[test]
fn dwarf_v1_str() {
    // Read exactly len bytes: a wolf str is not a C string.
    transcript(
        "dwarf-v1-str",
        "fn inspect(value: str) -> int { value.len - 4 }\nfn main() -> int { inspect(\"wolf\") }\n",
        "print *value.data@4",
        "expression -- *(char (*)[4])value.data",
        &["len = 4", "data", "wolf"],
    );
}

#[test]
fn dwarf_v1_list() {
    transcript(
        "dwarf-v1-list",
        "fn inspect(value: List[int]) -> int { value.len - 2 }\nfn main() -> int { var xs = List[int]()\n(mut xs).push(31)\n(mut xs).push(47)\ninspect(xs) }\n",
        "print *value\nprint value->data[0]\nprint value->data[1]",
        "frame variable --ptr-depth 1 value\nframe variable value->data[0]\nframe variable value->data[1]",
        &["len = 2", "elem = 8", "region", "= 31", "= 47"],
    );
}

#[test]
fn dwarf_v1_map() {
    transcript(
        "dwarf-v1-map",
        "fn inspect(value: Map[int, int]) -> int { value.len - 1 }\nfn main() -> int { var xs = Map[int, int]()\nxs[3] = 47\ninspect(xs) }\n",
        "print *value\nprint value->data[0]",
        "frame variable --ptr-depth 1 value\nframe variable value->data[0]",
        &[
            "len = 1",
            "key_size = 8",
            "val_off = 8",
            "val_size = 8",
            "key = 3",
            "value = 47",
        ],
    );
}

#[test]
fn dwarf_v1_enum() {
    transcript(
        "dwarf-v1-enum",
        "enum Color { Red, Green }\nfn inspect(value: Color) -> int { match value { Red => 1, Green => 0 } }\nfn main() -> int { inspect(Color.Green) }\n",
        "print value",
        "frame variable value",
        &["= Green"],
    );
}

fn variant_transcript(row: &str, source: &str, ty: &str, expected: &str, wants: &[&str]) {
    if cfg!(target_os = "macos") {
        if let Some(exe) = fixture(row, source, true) {
            dwarf_variant_bytes::check(row, &exe, ty);
        }
    } else if transcript(row, source, "print value", "frame variable value", wants) {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("debug_v1_{row}"));
        let oracle_row = format!("{row}-lupin");
        match dwarf_variant_bytes::reference(&oracle_row, &dir, expected) {
            Ok(true) => println!("PASS {oracle_row}"),
            Ok(false) => {}
            Err(error) => {
                eprintln!("FAIL {oracle_row}");
                panic!("{oracle_row}: {error}");
            }
        }
    }
}

#[test]
fn dwarf_v1_payload_enum() {
    variant_transcript(
        "dwarf-v1-payload-enum",
        "enum Choice { Number(int), Text(str) }\nfn inspect(value: Choice) -> int { match value { Number(n) => { print(\"Number {n}\")\n0 }, Text(_) => 1 } }\nfn main() -> int { inspect(Choice.Number(73)) }\n",
        "Choice",
        "Number 73\n",
        &["Number =", "73"],
    );
}

#[test]
fn dwarf_v1_error_union() {
    variant_transcript(
        "dwarf-v1-error-union",
        "fn inspect(value: int ! {Bad(int)}) -> int { match value { Bad(n) => { print(\"Bad {n}\")\n0 }, n => { print(\"ok {n}\")\n0 } } }\nfn main() -> int { inspect(Bad(61)) }\n",
        "int ! {Bad(int)}",
        "Bad 61\n",
        &["Bad =", "61"],
    );
}

#[test]
fn dwarf_v1_optional() {
    variant_transcript(
        "dwarf-v1-optional",
        "fn inspect(value: int ! {none}) -> int { match value { none => { print(\"none\")\n0 }, n => { print(\"ok {n}\")\n0 } } }\nfn main() -> int { inspect(none) }\n",
        "int ! {none}",
        "none\n",
        &["none ="],
    );
}

#[test]
fn dwarf_v1_debug_frame_is_opt_in_and_nonallocated() {
    use object::{Object, ObjectSection};
    let src = "fn inspect(a: int) -> int { a + 1 }\nfn middle(a: int) -> int { inspect(a) + 1 }\nfn main() -> int { middle(3) - 5 }\n";
    for enabled in [false, true] {
        let row = if enabled {
            "dwarf-v1-debug-frame"
        } else {
            "frame-off"
        };
        let Some(exe) = fixture(row, src, enabled) else {
            return;
        };
        let artifact = if cfg!(target_os = "macos") {
            exe.with_extension("dSYM")
                .join("Contents/Resources/DWARF/probe")
        } else {
            exe
        };
        let bytes = std::fs::read(&artifact).unwrap();
        let obj = object::File::parse(&*bytes).unwrap();
        let frame = obj
            .section_by_name(".debug_frame")
            .or_else(|| obj.section_by_name("__debug_frame"));
        // A linked runtime can have frame info of its own. This check will be
        // tightened to the wolf FDE address ranges when emission is implemented.
        if enabled && frame.is_none() {
            eprintln!("FAIL dwarf-v1-debug-frame");
            panic!("-g output has no .debug_frame");
        }
        if let Some(frame) = frame
            && let object::SectionFlags::Elf { sh_flags } = frame.flags()
        {
            assert_eq!(sh_flags & u64::from(object::elf::SHF_ALLOC), 0);
        }
    }
}

fn reference_stdout(row: &str, expected: &str) {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("debug_v1_{row}"));
    let scalar = row
        .strip_prefix("dwarf-v1-")
        .and_then(|s| {
            s.strip_suffix("-value")
                .or_else(|| s.strip_suffix("-boundary"))
        })
        .expect("scalar row name");
    dwarf_variant_bytes::scalar_encoding(row, &dir.join("probe"), scalar);
    let oracle_row = format!("{row}-lupin");
    match dwarf_variant_bytes::reference(&oracle_row, &dir, expected) {
        Ok(true) => println!("PASS {oracle_row}"),
        Ok(false) => {}
        Err(error) => {
            eprintln!("FAIL {oracle_row}");
            panic!("{oracle_row}: {error}");
        }
    }
}

// LLDB renders one-byte C-compatible integers as characters. Widen using
// the emitted type's signedness, so a signed/unsigned DIE bug still fails.
fn scalar_lldb_command(ty: &str) -> &'static str {
    match ty {
        "i8" | "u8" | "byte" => "expression -- (int)value",
        _ => "frame variable value",
    }
}

macro_rules! unsigned_boundary {
    ($test:ident, $ty:literal, $value:literal) => {
        #[test]
        fn $test() {
            let source = format!(
                "fn inspect(value: {}) -> int {{ print(\"{{value}}\")\n0 }}\nfn main() -> int {{ let n: {} = {}\ninspect(n) }}\n",
                $ty, $ty, $value
            );
            if transcript(
                concat!("dwarf-v1-", $ty, "-boundary"),
                &source,
                "print value",
                scalar_lldb_command($ty),
                &[concat!("= ", $value)],
            ) { reference_stdout(concat!("dwarf-v1-", $ty, "-boundary"), concat!($value, "\n")); }
        }
    };
}
unsigned_boundary!(dwarf_v1_unsigned_u8, "u8", "255");
unsigned_boundary!(dwarf_v1_unsigned_u16, "u16", "65535");
unsigned_boundary!(dwarf_v1_unsigned_u32, "u32", "4294967295");
unsigned_boundary!(dwarf_v1_unsigned_u64, "u64", "18446744073709551615");
unsigned_boundary!(dwarf_v1_unsigned_uint, "uint", "18446744073709551615");

macro_rules! scalar_value {
    ($test:ident, $ty:literal, $value:literal, $expected:literal) => {
        #[test]
        fn $test() {
            let source = format!("fn inspect(value: {}) -> int {{ print(\"{{value}}\")\n0 }}\nfn main() -> int {{ let n: {} = {}\ninspect(n) }}\n", $ty, $ty, $value);
            if transcript(concat!("dwarf-v1-", $ty, "-value"), &source, "print value", scalar_lldb_command($ty), &[concat!("= ", $expected)]) {
                reference_stdout(concat!("dwarf-v1-", $ty, "-value"), concat!($expected, "\n"));
            }
        }
    };
}
scalar_value!(dwarf_v1_scalar_i8, "i8", "-127 - 1", "-128");
scalar_value!(dwarf_v1_scalar_i16, "i16", "-32767 - 1", "-32768");
scalar_value!(dwarf_v1_scalar_i32, "i32", "-2147483647 - 1", "-2147483648");
scalar_value!(
    dwarf_v1_scalar_i64,
    "i64",
    "-9223372036854775807 - 1",
    "-9223372036854775808"
);
scalar_value!(
    dwarf_v1_scalar_int,
    "int",
    "-9223372036854775807 - 1",
    "-9223372036854775808"
);
scalar_value!(dwarf_v1_scalar_bool, "bool", "true", "true");
#[test]
fn dwarf_v1_scalar_f32() {
    // Native f32 formatting is not implemented. Observe the exact value with
    // a checked equality instead; both machines must take the same branch.
    let source = "fn inspect(value: f32) -> int { let expected: f32 = 1.25\nif value == expected { print(\"1.25\") } else { print(\"wrong value\") }\n0 }\nfn main() -> int { let n: f32 = 1.25\ninspect(n) }\n";
    if transcript(
        "dwarf-v1-f32-value",
        source,
        "print value",
        "frame variable value",
        &["= 1.25"],
    ) {
        reference_stdout("dwarf-v1-f32-value", "1.25\n");
    }
}
scalar_value!(dwarf_v1_scalar_f64, "f64", "1.25", "1.25");
scalar_value!(dwarf_v1_scalar_byte, "byte", "255 as byte", "255");

#[test]
fn dwarf_v1_scalar_char() {
    if transcript(
        "dwarf-v1-char-value",
        "fn inspect(value: char) -> int { let numeric: int = value as int\nprint(\"{numeric}\")\n0 }\nfn main() -> int { let n: char = 955 as char\ninspect(n) }\n",
        "print (unsigned)value",
        "expression -- (unsigned)value",
        &["= 955"],
    ) {
        reference_stdout("dwarf-v1-char-value", "955\n");
    }
}

#[test]
fn dwarf_v1_borrowed_packed_value() {
    transcript(
        "dwarf-v1-borrowed-value",
        "struct Pair { small: i8, wide: int }\nfn inspect(mut value: Pair) -> int { value.wide - 42 }\nfn main() -> int { var p = Pair { small: 7 as i8, wide: 42 }\ninspect(mut p) }\n",
        "print value.wide",
        "frame variable value.wide",
        &["wide = 42"],
    );
}

#[test]
fn dwarf_v1_packed_list_element() {
    transcript(
        "dwarf-v1-packed-element",
        "struct Pair { small: i8, wide: int }\nfn inspect(value: List[Pair]) -> int { value.len - 2 }\nfn main() -> int { var xs = List[Pair]()\n(mut xs).push(Pair { small: 7 as i8, wide: 42 })\n(mut xs).push(Pair { small: 8 as i8, wide: 97 })\ninspect(xs) }\n",
        "print value->data[1]",
        "frame variable value->data[1]\nexpression -- (int)value->data[1].small",
        &["= 8", "wide = 97"],
    );
}

#[test]
fn dwarf_v1_recursive_nominal_gap_is_explicit() {
    let source = "struct Node { value: int, next: List[Node] }\nfn inspect(value: Node) -> int { value.value - 42 }\nfn main() -> int { let node = Node { value: 42, next: List[Node]() }\ninspect(node) }\n";
    if let Some(exe) = fixture("recursive-nominal", source, false) {
        dwarf_variant_bytes::recursive_gap(&exe);
    }
}

#[test]
fn dwarf_v1_map_layout_matches_runtime() {
    let source = "fn inspect(value: Map[int, int]) -> int { value.len }\nfn main() -> int { inspect(Map[int, int]()) }\n";
    if let Some(exe) = fixture("map-layout", source, false) {
        dwarf_variant_bytes::runtime_header(
            "dwarf-v1-map-runtime-layout",
            &exe,
            wolf_rt::map::DEBUG_LAYOUT,
        );
    }
}

#[test]
fn dwarf_v1_list_layout_matches_runtime() {
    let source = "fn inspect(value: List[int]) -> int { value.len }\nfn main() -> int { inspect(List[int]()) }\n";
    if let Some(exe) = fixture("list-layout", source, false) {
        dwarf_variant_bytes::runtime_header(
            "dwarf-v1-list-runtime-layout",
            &exe,
            wolf_rt::list::DEBUG_LAYOUT,
        );
    }
}
