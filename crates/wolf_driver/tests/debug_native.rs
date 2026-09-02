//! s30 acceptance — a DEBUGGABLE native binary.
//!
//! Three gates over `wolf build`'s output:
//! 1. **Ledger**: the executable carries wolf's OWN `.debug_line` /
//!    `.debug_info` (asserted by parsing the ELF and finding the wolf
//!    producer string — not just `libwolf_rt.a`'s Rust debug info).
//! 2. **Debugger transcript**: a gdb batch session breaks on the wolf
//!    `main`, steps source lines in order, prints locals and
//!    parameters, and backtraces through two wolf frames — asserted on
//!    stable substrings (addresses are volatile, file:line and values
//!    are not). SKIPs loudly when gdb is absent; the linux CI lane
//!    installs it.
//! 3. **Issue #26 end to end**: two modules sharing an item name
//!    compile natively and run (module-qualified mangling).
//!
//! Platforms (s59): the runtime-SKIP pattern — `wolf build` exits 2
//! where the environment cannot link, and each transcript runs under
//! the debugger its host actually has: gdb on linux (not viable on
//! Apple Silicon), lldb on macOS. The ledger looks in the executable
//! on ELF hosts and in the linked `.dSYM` on Mach-O (Apple's static
//! link writes a debug map; `wolf build` runs dsymutil).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The stepping fixture: two functions, scalar params and lets, an
/// exit code that proves the arithmetic ran.
const DEBUGGABLE: &str = "\
fn helper(a: int, b: int) -> int {
    let s = a + b
    s * 2
}

fn main() -> int {
    let x = 3
    let y = 4
    let z = helper(x, y)
    z - 14
}
";

/// `wolf build` the fixture into `<tmp>/<case>/prog`. `None` (with a
/// loud SKIP) only for environment problems (no `cc`, no rt lib) —
/// compile refusals and errors FAIL the test.
fn build_fixture(case: &str, src: &str) -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("debuggable.lu");
    std::fs::write(&entry, src).expect("write fixture");
    let exe = dir.join("prog");
    let out = Command::new(wolf())
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("wolf runs");
    match out.status.code() {
        Some(0) => Some(exe),
        Some(2) => {
            eprintln!(
                "SKIP: environment cannot link native binaries: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            None
        }
        other => panic!(
            "wolf build failed (exit {other:?}): {}",
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

/// Gate 1 — the ledger: wolf's own DWARF is in the binary (ELF), or
/// in the `.dSYM` dsymutil linked from the debug map (Mach-O — the
/// executable itself never carries DWARF there).
#[test]
fn executable_carries_wolf_debug_sections() {
    let Some(exe) = build_fixture("dwarf_sections", DEBUGGABLE) else {
        return;
    };
    let debug_artifact = if cfg!(target_os = "macos") {
        let dsym = exe.with_extension("dSYM");
        let dwarf_dir = dsym.join("Contents/Resources/DWARF");
        let entry = std::fs::read_dir(&dwarf_dir)
            .unwrap_or_else(|e| panic!("no .dSYM after a debug build ({}): {e}", dsym.display()))
            .next()
            .expect(".dSYM DWARF entry")
            .expect("readable dir entry");
        entry.path()
    } else {
        exe.clone()
    };
    let bytes = std::fs::read(&debug_artifact).expect("read debug artifact");
    let obj = object::File::parse(&*bytes).expect("object parses");
    use object::{Object, ObjectSection};
    let mut names = Vec::new();
    let mut wolf_producer = false;
    let mut line_nonempty = false;
    for sec in obj.sections() {
        // Mach-O spells DWARF sections `__debug_*`; normalize to the
        // ELF spelling so one assertion serves both formats.
        let name = sec
            .name()
            .unwrap_or("")
            .replace("__debug_", ".debug_")
            .to_string();
        // The producer lands inline in .debug_info (gimli writes DIE
        // strings as DW_FORM_string) — but accept .debug_str too.
        if (name == ".debug_info" || name == ".debug_str")
            && let Ok(data) = sec.data()
            && data
                .windows(b"wolf v0 (wolfgang".len())
                .any(|w| w == b"wolf v0 (wolfgang")
        {
            wolf_producer = true;
        }
        if name == ".debug_line" && sec.size() > 0 {
            line_nonempty = true;
        }
        names.push(name);
    }
    for want in [".debug_info", ".debug_line", ".debug_abbrev"] {
        assert!(
            names.iter().any(|n| n == want),
            "missing {want}; sections: {names:?}"
        );
    }
    assert!(line_nonempty, ".debug_line is empty");
    assert!(
        wolf_producer,
        "wolf's compile unit (producer string) not found — only foreign debug info present"
    );
}

/// Gate 2 — the debugger transcript. `break main` resolves to two
/// locations (the C entry shim exports the unmangled `main` symbol;
/// the wolf `main` is the DWARF subprogram) — the shim hits first,
/// `continue` lands on the first user statement. Documented v0
/// behavior; frames display wolf source names (no linkage names until
/// a demangler exists, s54).
#[test]
fn gdb_breaks_steps_and_prints() {
    if cfg!(windows) {
        eprintln!(
            "SKIP: debugger transcripts on windows are s60's (DWARF-in-COFF + lldb) — the s60a bring-up links wolf's DWARF but reads none of it"
        );
        return;
    }
    if Command::new("gdb").arg("--version").output().is_err() {
        eprintln!("SKIP: gdb not installed (required on the linux CI lane)");
        return;
    }
    let Some(exe) = build_fixture("dwarf_gdb", DEBUGGABLE) else {
        return;
    };
    let dir = exe.parent().expect("dir");
    let script = dir.join("script.gdb");
    std::fs::write(
        &script,
        "set confirm off\n\
         set pagination off\n\
         break main\n\
         run\n\
         continue\n\
         next\n\
         print x\n\
         next\n\
         print y\n\
         break helper\n\
         continue\n\
         print a\n\
         print b\n\
         next\n\
         print s\n\
         bt\n\
         continue\n\
         quit\n",
    )
    .expect("write script");
    let out = Command::new("gdb")
        .arg("--batch")
        .arg("-x")
        .arg(&script)
        .arg(&exe)
        .output()
        .expect("gdb runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // Breakpoint on the wolf main lands on the first user statement…
    assert!(
        text.contains("main () at") && text.contains("debuggable.lu:7"),
        "break main did not land on the first user statement:\n{text}"
    );
    // …stepping visits source lines in order…
    assert!(
        text.contains("8\t    let y = 4"),
        "next did not reach line 8:\n{text}"
    );
    assert!(
        text.contains("9\t    let z = helper(x, y)"),
        "next did not reach line 9:\n{text}"
    );
    // …locals and params print correct values…
    assert!(text.contains("$1 = 3"), "print x != 3:\n{text}");
    assert!(text.contains("$2 = 4"), "print y != 4:\n{text}");
    assert!(
        text.contains("helper (a=3, b=4) at") && text.contains("debuggable.lu:2"),
        "break helper did not land with readable params:\n{text}"
    );
    assert!(text.contains("$3 = 3"), "print a != 3:\n{text}");
    assert!(text.contains("$4 = 4"), "print b != 4:\n{text}");
    assert!(text.contains("$5 = 7"), "print s != 7:\n{text}");
    // …and the backtrace walks helper → main (wolf frames with lines).
    assert!(
        text.contains("#0  helper (a=3, b=4)"),
        "bt frame 0 wrong:\n{text}"
    );
    assert!(
        text.contains("#1") && text.contains("in main () at") && text.contains("debuggable.lu:9"),
        "bt frame 1 (wolf main with file:line) wrong:\n{text}"
    );
}

/// Gate 3 — issue #26: `lista.len` and `stra.len` (same name, same
/// signature, different modules) link natively into one program.
#[test]
fn native_two_modules_share_an_item_name() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/resolve/same_name");
    let entry = root.join("main.lu");
    assert!(entry.is_file(), "corpus fixture present");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg("--native")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) {
        eprintln!(
            "SKIP: environment cannot link native binaries: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return;
    }
    let record: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record");
    assert_eq!(record["phase_reached"], "run", "record: {record}");
    assert_eq!(record["verdict"], "exit(0)", "record: {record}");
}

/// Gate 4 (s31, the M1 debug clause on a REAL corpus program):
/// `corpus/hello.lu` under gdb — `break main` lands on the first user
/// statement, `next` steps to the print, `bt` shows the wolf frame
/// with file:line, and the continued program prints its literal
/// output. (`errors.lu` joins when its sema surface lands — recorded
/// in the c06 closeout; lldb is s54's second consumer.)
#[test]
fn gdb_steps_hello_lu() {
    if Command::new("gdb").arg("--version").output().is_err() {
        eprintln!("SKIP: gdb not installed (required on the linux CI lane)");
        return;
    }
    let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/hello.lu");
    let src = std::fs::read_to_string(&hello).expect("corpus/hello.lu");
    let Some(exe) = build_fixture("dwarf_hello", &src) else {
        return;
    };
    let dir = exe.parent().expect("dir");
    let script = dir.join("hello.gdb");
    std::fs::write(
        &script,
        "set confirm off\n\
         set pagination off\n\
         break main\n\
         run\n\
         continue\n\
         next\n\
         bt\n\
         continue\n\
         quit\n",
    )
    .expect("write script");
    let out = Command::new("gdb")
        .arg("--batch")
        .arg("-x")
        .arg(&script)
        .arg(&exe)
        .output()
        .expect("gdb runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("main () at") && text.contains("debuggable.lu:10"),
        "break main did not land on `let who`:\n{text}"
    );
    assert!(
        text.contains("11\t    print(\"hello, {who}\")"),
        "next did not reach the print line:\n{text}"
    );
    assert!(
        text.contains("#0  main () at") && text.contains("debuggable.lu:11"),
        "bt frame 0 (wolf main at the print) wrong:\n{text}"
    );
    assert!(
        text.contains("hello, wolf"),
        "continued program did not print:\n{text}"
    );
    assert!(
        text.contains("exited normally"),
        "program did not exit 0 under gdb:\n{text}"
    );
}

/// Gate 2, the lldb half (s59 — gdb is not viable on Apple Silicon):
/// the same stepping fixture under an lldb batch session. The s30
/// floor and then some: a breakpoint lands with file:line, parameters
/// and locals read their true values, `next` walks source lines, and
/// the backtrace shows helper → main with wolf source coordinates.
/// SKIPs loudly where lldb is absent; the macOS CI lane has it.
#[test]
fn lldb_breaks_steps_and_prints() {
    if cfg!(windows) {
        eprintln!(
            "SKIP: debugger transcripts on windows are s60's (DWARF-in-COFF + lldb) — the s60a bring-up links wolf's DWARF but reads none of it"
        );
        return;
    }
    if Command::new("lldb").arg("--version").output().is_err() {
        eprintln!("SKIP: lldb not installed (required on the macOS CI lane)");
        return;
    }
    let Some(exe) = build_fixture("dwarf_lldb", DEBUGGABLE) else {
        return;
    };
    let out = Command::new("lldb")
        .arg("-b")
        .args(["-o", "breakpoint set --name helper"])
        .args(["-o", "run"])
        .args(["-o", "frame variable a b"])
        .args(["-o", "next"])
        .args(["-o", "frame variable s"])
        .args(["-o", "bt"])
        .args(["-o", "continue"])
        .arg(&exe)
        .output()
        .expect("lldb runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // The breakpoint resolves through wolf's DWARF (file:line, not a
    // bare symbol+offset) and stops with readable parameters…
    assert!(
        text.contains("helper(a=3, b=4) at debuggable.lu:2"),
        "break helper did not land with file:line and params:\n{text}"
    );
    assert!(text.contains("(long) a = 3"), "a != 3:\n{text}");
    assert!(text.contains("(long) b = 4"), "b != 4:\n{text}");
    // …stepping follows the line table…
    assert!(
        text.contains("at debuggable.lu:3"),
        "next did not reach line 3:\n{text}"
    );
    assert!(text.contains("(long) s = 7"), "s != 7:\n{text}");
    // …and the backtrace walks helper → main with wolf coordinates.
    assert!(
        text.contains("main at debuggable.lu:9"),
        "bt did not show the wolf main frame with file:line:\n{text}"
    );
    assert!(
        text.contains("exited with status = 0"),
        "the continued program did not exit 0:\n{text}"
    );
}

/// Gate 4, the lldb half: `corpus/hello.lu` — a breakpoint by source
/// line through wolf's line table, the stepped print line, and the
/// program's real output under the debugger.
#[test]
fn lldb_steps_hello_lu() {
    if cfg!(windows) {
        eprintln!(
            "SKIP: debugger transcripts on windows are s60's (DWARF-in-COFF + lldb) — the s60a bring-up links wolf's DWARF but reads none of it"
        );
        return;
    }
    if Command::new("lldb").arg("--version").output().is_err() {
        eprintln!("SKIP: lldb not installed (required on the macOS CI lane)");
        return;
    }
    let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/hello.lu");
    let src = std::fs::read_to_string(&hello).expect("corpus/hello.lu");
    let Some(exe) = build_fixture("dwarf_lldb_hello", &src) else {
        return;
    };
    let out = Command::new("lldb")
        .arg("-b")
        .args(["-o", "breakpoint set --file debuggable.lu --line 10"])
        .args(["-o", "run"])
        .args(["-o", "bt"])
        .args(["-o", "continue"])
        .arg(&exe)
        .output()
        .expect("lldb runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("at debuggable.lu:10"),
        "the source-line breakpoint did not land:\n{text}"
    );
    assert!(
        text.contains("main at debuggable.lu:10"),
        "bt frame 0 (wolf main with file:line) wrong:\n{text}"
    );
    assert!(
        text.contains("hello, wolf"),
        "continued program did not print:\n{text}"
    );
    assert!(
        text.contains("exited with status = 0"),
        "program did not exit 0 under lldb:\n{text}"
    );
}
