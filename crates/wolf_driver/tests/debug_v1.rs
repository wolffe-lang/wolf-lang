//! DWARF v1 source-type witnesses. Each transcript belongs to one type row.
//! These use the debug tier; the release tier has no source DWARF yet.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(row: &str, src: &str, debug_frame: bool) -> Option<PathBuf> {
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
    if !String::from_utf8_lossy(&out.stderr).contains("unknown flag")
        && lane_exit::environment_refusal(&out, "wolf build")
    {
        eprintln!("SKIP: {row}: {}", String::from_utf8_lossy(&out.stderr));
        return None;
    }
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

fn transcript(row: &str, src: &str, gdb_expr: &str, lldb_expr: &str, wants: &[&str]) {
    let debugger = if cfg!(target_os = "macos") {
        "lldb"
    } else {
        "gdb"
    };
    if cfg!(windows) || Command::new(debugger).arg("--version").output().is_err() {
        eprintln!("SKIP: {row}: {debugger} unavailable on this platform");
        return;
    }
    let Some(exe) = fixture(row, src, false) else {
        return;
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

#[test]
fn dwarf_v1_payload_enum() {
    transcript(
        "dwarf-v1-payload-enum",
        "enum Choice { Number(int), Text(str) }\nfn inspect(value: Choice) -> int { match value { Number(n) => n - 73, Text(_) => 1 } }\nfn main() -> int { inspect(Choice.Number(73)) }\n",
        "print value",
        "frame variable value",
        &["Number", "73"],
    );
}

#[test]
fn dwarf_v1_error_union() {
    transcript(
        "dwarf-v1-error-union",
        "fn inspect(value: int ! {Bad(int)}) -> int { match value { Bad(n) => n - 61, n => n } }\nfn main() -> int { inspect(Bad(61)) }\n",
        "print value",
        "frame variable value",
        &["Bad", "61"],
    );
}

#[test]
fn dwarf_v1_optional() {
    transcript(
        "dwarf-v1-optional",
        "fn inspect(value: int ! {none}) -> int { value else { 0 } }\nfn main() -> int { inspect(none) }\n",
        "print value",
        "frame variable value",
        &["none"],
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

macro_rules! unsigned_boundary {
    ($test:ident, $ty:literal, $value:literal) => {
        #[test]
        fn $test() {
            let source = format!(
                "fn inspect(value: {}) -> int {{ 0 }}\nfn main() -> int {{ inspect({} as {}) }}\n",
                $ty, $value, $ty
            );
            transcript(
                concat!("dwarf-v1-", $ty, "-boundary"),
                &source,
                "print value",
                "frame variable value",
                &[concat!("= ", $value)],
            );
        }
    };
}
unsigned_boundary!(dwarf_v1_unsigned_u8, "u8", "255");
unsigned_boundary!(dwarf_v1_unsigned_u16, "u16", "65535");
unsigned_boundary!(dwarf_v1_unsigned_u32, "u32", "4294967295");
unsigned_boundary!(dwarf_v1_unsigned_u64, "u64", "18446744073709551615");
unsigned_boundary!(dwarf_v1_unsigned_uint, "uint", "18446744073709551615");

#[test]
fn dwarf_v1_scalar_values() {
    for (ty, value, expected) in [
        ("i8", "(0 - 128) as i8", "-128"),
        ("i16", "(0 - 32768) as i16", "-32768"),
        ("i32", "(0 - 2147483648) as i32", "-2147483648"),
        (
            "i64",
            "(0 - 9223372036854775807 - 1) as i64",
            "-9223372036854775808",
        ),
        ("int", "0 - 9223372036854775807 - 1", "-9223372036854775808"),
        ("bool", "true", "true"),
        ("f32", "1.25 as f32", "1.25"),
        ("f64", "1.25", "1.25"),
        ("byte", "255 as byte", "255"),
    ] {
        let source = format!(
            "fn inspect(value: {ty}) -> int {{ 0 }}\nfn main() -> int {{ inspect({value}) }}\n"
        );
        transcript(
            &format!("dwarf-v1-{ty}-value"),
            &source,
            "print value",
            "frame variable value",
            &[&format!("= {expected}")],
        );
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
        "frame variable value->data[1]",
        &["small = 8", "wide = 97"],
    );
}
