//! s32 Target 1 — "a wolf binary that never spawns is a C binary":
//! the symbol half. A no-spawn corpus-shaped program, built by `wolf
//! build` through the `--gc-sections` link, must carry NO scheduler
//! symbols — no scope/spawn entry points, no pool, nothing of the
//! task layer. (The no-background-threads half is wolf_rt's own
//! tests/no_spawn.rs, over the lazily-initialized pool.)
//!
//! Off-target the whole file compiles away (native codegen is
//! linux/x86-64 only in c06/c07 v0).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A program that prints and traps-nothing — uses the print path (so
/// SOME wolf_rt symbols are live) but never spawns.
const NO_SPAWN: &str = "\
fn main() -> int {
    let who = \"wolf\"
    print(\"hello, {who}\")
    0
}
";

fn build_fixture(case: &str, src: &str) -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("prog.lu");
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

#[test]
fn no_spawn_binary_has_no_scheduler_symbols() {
    let Some(exe) = build_fixture("no_spawn_symbols", NO_SPAWN) else {
        return;
    };
    // The binary still runs (gc-sections broke nothing).
    let run = Command::new(&exe).output().expect("run");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello, wolf\n");

    let bytes = std::fs::read(&exe).expect("read exe");
    let obj = object::File::parse(&*bytes).expect("ELF parses");
    use object::{Object, ObjectSymbol};
    let syms: Vec<String> = obj
        .symbols()
        .filter_map(|s| s.name().ok().map(str::to_string))
        .collect();
    // Sanity: the check can see wolf_rt symbols at all (print is
    // live in this program).
    assert!(
        syms.iter().any(|s| s == "__wolf_rt_print_str"),
        "symbol table unreadable or stripped — the assertion below \
         would be vacuous"
    );
    // The law: no scheduler surface in a no-spawn binary — and no
    // proc registry/supervisor surface in a no-proc binary (the s34
    // half of the same D15 check: a no-spawn program is a fortiori
    // no-proc).
    for banned in [
        "__wolf_rt_scope_new",
        "__wolf_rt_scope_spawn",
        "__wolf_rt_scope_join_free",
        "__wolf_rt_task_checkpoint",
        "__wolf_rt_region_transfer",
        "__wolf_rt_dump_tasks",
        "__wolf_rt_proc_spawn",
        "__wolf_rt_proc_self",
        "__wolf_rt_proc_monitor",
        "__wolf_rt_proc_link",
        "__wolf_rt_proc_kill",
        "__wolf_rt_proc_cancel",
        "__wolf_rt_region_adopt",
    ] {
        assert!(
            !syms.iter().any(|s| s == banned),
            "no-spawn binary carries scheduler symbol {banned}"
        );
    }
}
