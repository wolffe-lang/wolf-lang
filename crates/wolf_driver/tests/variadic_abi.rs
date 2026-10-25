//! Source-to-object C variadics on both compiling tiers, including the
//! real freestanding muzzle target. Clang is an oracle, never source.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}
fn clang() -> String {
    std::env::var("WOLF_CLANG").unwrap_or_else(|_| "clang".into())
}
fn checked(command: &mut Command) -> Output {
    let out = command
        .output()
        .unwrap_or_else(|e| panic!("{command:?}: {e}"));
    assert!(
        out.status.success(),
        "{command:?}: {:?}\n{}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/variadic")
        .join(name)
}
fn stage(name: &str, hosted: bool) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("s263_source_variadic")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut source = std::fs::read_to_string(fixture("exports.lu")).unwrap();
    if hosted {
        source.push('\n');
        source.push_str(&std::fs::read_to_string(fixture("hosted.lu")).unwrap());
    }
    std::fs::write(dir.join("exports.lu"), source).unwrap();
    if !hosted {
        std::fs::write(dir.join("probe.S"), ".text\n.globl s263_marker\n.type s263_marker,@function\ns263_marker:\nret\n.section .note.GNU-stack,\"\",@progbits\n").unwrap();
        std::fs::write(dir.join("wolf.pkg"), "pkg {\n name: \"s263/variadic\"\n version: \"0.0.1\"\n asm: [\"probe.S\"]\n capabilities: [ffi]\n}\n").unwrap();
    }
    dir
}
fn objects(dir: &Path, release: bool, none: bool) -> Vec<PathBuf> {
    let mut command = Command::new(wolf());
    command
        .current_dir(dir)
        .args(["build", "exports.lu", "--emit=obj", "-o", "wolf.o"]);
    if release {
        command.arg("--release");
    }
    if none {
        command.args(["--target", "x86_64-unknown-none"]);
    }
    checked(&mut command);
    let mut objects: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "o"))
        .collect();
    objects.sort();
    assert!(!objects.is_empty(), "no compiler objects");
    objects
}
fn rt_archive() -> PathBuf {
    let directory = Path::new(wolf()).parent().unwrap();
    let mut command = Command::new(env!("CARGO"));
    command.args(["build", "-p", "wolf_rt"]);
    if directory.file_name().is_some_and(|n| n == "release") {
        command.arg("--release");
    }
    checked(&mut command);
    let archive = directory.join("libwolf_rt.a");
    assert!(
        archive.is_file(),
        "runtime archive missing: {}",
        archive.display()
    );
    archive
}
fn supported_host() -> bool {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        true
    } else {
        eprintln!("SKIP: source C variadic execution requires x86-64 Linux");
        false
    }
}

#[test]
fn source_variadics_both_directions_promotions_and_forwarding() {
    if !supported_host() {
        return;
    }
    checked(Command::new(clang()).arg("--version"));
    let runtime = rt_archive();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let dir = stage(&format!("hosted-{tier}"), true);
        let objects = objects(&dir, release, false);
        let executable = dir.join("run");
        checked(
            Command::new(clang())
                .args(["-std=c17", "-O1", "-ffp-contract=off"])
                .arg(fixture("caller.c"))
                .args(&objects)
                .arg(&runtime)
                .args(["-lpthread", "-ldl", "-lm", "-o"])
                .arg(&executable),
        );
        let out = checked(&mut Command::new(&executable));
        assert_eq!(out.stdout, b"source variadic ABI ok\n", "{tier}");
        eprintln!(
            "PASS source variadic ABI {tier}: all promotions, both banks, C list copy and nested forwarding"
        );
    }
}

// Shared fail-closed proof: no SSE instruction executes when incoming AL
// is zero, including when LLVM copies AL into another low-byte register.
#[path = "../../wolf_backend/tests/support/variadic_guard.rs"]
mod variadic_guard;

#[test]
fn real_none_target_links_c17_without_libc_or_runtime() {
    if !supported_host() {
        return;
    }
    checked(Command::new(clang()).arg("--version"));
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let dir = stage(&format!("none-{tier}"), false);
        let objects = objects(&dir, release, true);
        let mut saves = 0;
        for (i, object) in objects.iter().enumerate() {
            let out = checked(
                Command::new("objdump")
                    .args(["-d", "--no-show-raw-insn"])
                    .arg(object),
            );
            let text = String::from_utf8(out.stdout).unwrap();
            std::fs::write(dir.join(format!("{i}.objdump")), &text).unwrap();
            saves += variadic_guard::check(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        }
        if !release {
            assert!(
                saves >= 8,
                "native variadic entry proof must see its save block"
            );
        }
        let executable = dir.join("run");
        checked(
            Command::new(clang())
                .args([
                    "-std=c17",
                    "-O1",
                    "-DFREESTANDING",
                    "-ffreestanding",
                    "-fno-builtin",
                    "-fno-stack-protector",
                    "-nostdlib",
                    "-no-pie",
                    "-Wl,-e,_start",
                ])
                .arg(fixture("caller.c"))
                .arg(fixture("start.S"))
                .args(&objects)
                .arg("-o")
                .arg(&executable),
        );
        let out = checked(&mut Command::new(&executable));
        assert!(
            out.stdout.is_empty() && out.stderr.is_empty(),
            "{tier}: {out:?}"
        );
        eprintln!(
            "PASS real none target {tier}: C17 -nostdlib, no libc/runtime, {saves} guarded XMM saves only (objdump proof)"
        );
    }
}
