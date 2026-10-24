//! s264 / #914: inspect real non-inlined caller frames on both tiers.
//! A functional test alone cannot see this copy: the old compiler gave
//! the right answer while reserving kilobytes for each read lend.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(cmd: &mut Command) -> String {
    let out = cmd.output().expect("run command");
    assert!(
        out.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("UTF-8 output")
}

fn number(s: &str) -> u64 {
    let s = s.trim().trim_start_matches(['#', '$']);
    let s = s
        .split(|c: char| !c.is_ascii_hexdigit() && c != 'x')
        .next()
        .unwrap();
    if let Some(hex) = s.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).expect("hex immediate")
    } else {
        s.parse().expect("decimal immediate")
    }
}

/// Explicit stack reservation, excluding saved-register pushes. The
/// all-public consttime boundaries keep these symbols out of inlining.
fn frame(dis: &str, name: &str) -> u64 {
    let mut found = false;
    let mut active = false;
    let mut bytes = 0;
    let mut x16 = 0;
    for line in dis.lines() {
        if line.trim_end().ends_with(':') {
            active = line.contains(&format!("W{name}$"));
            found |= active;
            continue;
        }
        if !active {
            continue;
        }
        if let Some(rest) = line.split("mov\tx16, ").nth(1) {
            x16 = number(rest);
        }
        if let Some(rest) = line.split("sub\tsp, sp, ").nth(1) {
            bytes += if rest.trim() == "x16" {
                x16
            } else {
                let n = number(rest);
                if rest.contains("lsl #12") { n << 12 } else { n }
            };
        }
        if line.contains("sub")
            && line.contains(",%rsp")
            && let Some(rest) = line.split('$').nth(1)
        {
            bytes += number(rest);
        }
    }
    assert!(found, "missing non-inlined {name} symbol");
    bytes
}

#[test]
fn read_lends_do_not_reserve_a_struct_sized_frame() {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        eprintln!(
            "SKIP: frame disassembler gate runs on Linux and macOS; semantic rows run on every host"
        );
        return;
    }
    run(Command::new(env!("CARGO"))
        .current_dir(root())
        .args(["build", "-j4", "-p", "wolf_rt"]));
    let dir = std::env::temp_dir().join(format!("wolf-read-lend-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch directory");
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        for (file, expected) in [
            ("read_lend_frame", "read-lend-frame: ok\n"),
            ("read_lend_value_semantics", "read-lend-values: ok\n"),
        ] {
            let bin = dir.join(format!("{file}-{tier}"));
            let mut build = Command::new(env!("CARGO_BIN_EXE_wolf"));
            build
                .current_dir(root())
                .arg("build")
                .arg(root().join(format!("corpus/memory/{file}.lu")))
                .args(["--no-cache", "--deny-warnings", "-o"])
                .arg(&bin);
            if release {
                build.arg("--release");
            }
            run(&mut build); // Exit 2 is a compile error, never a skip.
            assert_eq!(run(&mut Command::new(&bin)), expected, "{tier}/{file}");
            if file == "read_lend_frame" {
                let dis = if cfg!(target_os = "macos") {
                    run(Command::new("otool").arg("-tvV").arg(&bin))
                } else {
                    run(Command::new("objdump")
                        .args(["-d", "--no-show-raw-insn"])
                        .arg(&bin))
                };
                for name in ["step_read", "step_mut", "step_inline"] {
                    if release
                        && name == "step_inline"
                        && !dis
                            .lines()
                            .any(|l| l.ends_with(':') && l.contains("Wstep_inline$"))
                    {
                        eprintln!("read-lend-frame release step_inline: inlined");
                        continue;
                    }
                    let bytes = frame(&dis, name);
                    eprintln!("read-lend-frame {tier} {name}: reserved={bytes} bytes");
                    assert!(
                        bytes <= 256,
                        "{tier} {name} copies the 3208-byte struct: frame={bytes}"
                    );
                }
            }
        }
    }
    std::fs::remove_dir_all(dir).expect("remove scratch directory");
}

#[test]
fn frame_parser_recognizes_both_target_prologues() {
    assert_eq!(
        frame(
            "__Wstep_read$abc:\n0 mov\tx16, #0x1880\n1 sub\tsp, sp, x16\n",
            "step_read"
        ),
        6272
    );
    assert_eq!(
        frame("0 <_Wstep_read$abc>:\n 0: sub $0x1910,%rsp\n", "step_read"),
        6416
    );
}
