//! ld02b: opt-in cc-shaped linker protocol and process-level refusal witnesses.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn package(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("linker_select")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("wolf.pkg"),
        "pkg {\n name: \"ld02b/test\"\n version: \"0.0.1\"\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("main.lu"), "fn main() -> int { 7 }\n").unwrap();
    dir
}

fn build(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_wolf"));
    c.current_dir(dir)
        .env_remove("WOLF_LD")
        .env_remove("CC")
        .args(["build", "main.lu", "--no-cache", "-o", "output program"]);
    c
}

fn refused(out: Output, fragments: &[&str]) {
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    for fragment in fragments {
        assert!(err.contains(fragment), "missing {fragment:?}: {err}");
    }
}

#[test]
fn linker_option_requires_a_nonempty_path() {
    let dir = package("usage");
    for args in [vec!["--linker"], vec!["--linker="], vec!["--linker", ""]] {
        refused(
            build(&dir).args(args).output().unwrap(),
            &["--linker needs a path"],
        );
    }
}

#[test]
fn selected_linker_does_not_enable_foreign_targets() {
    let dir = package("foreign target");
    refused(
        build(&dir)
            .args(["--target=not-a-wolf-target", "--linker=missing"])
            .output()
            .unwrap(),
        &["unknown target `not-a-wolf-target`"],
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn selected_linker_refuses_unsupported_host_by_name() {
    let dir = package("unsupported host");
    let message = if cfg!(windows) {
        "hosted Windows is unsupported"
    } else {
        "only native Linux ELF targets are supported"
    };
    refused(
        build(&dir).arg("--linker=missing").output().unwrap(),
        &[message],
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tool(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("linker with spaces and 'quote");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn success(out: Output) {
        assert!(
            out.status.success(),
            "exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn fallback(dir: &Path) -> PathBuf {
        let path = dir.join("cc fallback");
        std::fs::write(&path, "#!/bin/sh\ntouch cc-was-called\nexec cc \"$@\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn configured_path_and_output_spaces_preserve_cc_argv() {
        let dir = package("argv");
        let path = tool(&dir, "printf '%s\\n' \"$@\" > argv.txt\nexec cc \"$@\"");
        success(build(&dir).arg("--linker").arg(path).output().unwrap());
        let argv = std::fs::read_to_string(dir.join("argv.txt")).unwrap();
        let args: Vec<_> = argv.lines().collect();
        assert_eq!(&args[..2], &["-o", "output program"]);
        assert!(args[2].ends_with("00-root.o"), "{argv}");
        assert!(args[3].ends_with("libwolf_rt.a"), "{argv}");
        assert_eq!(
            &args[4..],
            &["-lpthread", "-ldl", "-lm", "-Wl,--gc-sections"]
        );
        assert_eq!(
            Command::new(dir.join("output program"))
                .status()
                .unwrap()
                .code(),
            Some(7)
        );
    }

    #[test]
    fn cli_linker_overrides_environment() {
        let dir = package("precedence");
        let path = tool(&dir, "exec cc \"$@\"");
        success(
            build(&dir)
                .env("WOLF_LD", "/missing/env-linker")
                .arg(format!("--linker={}", path.display()))
                .output()
                .unwrap(),
        );
    }

    #[test]
    fn release_uses_selected_linker_without_replacing_clang_codegen() {
        let dir = package("release selection");
        success(
            build(&dir)
                .args(["--release", "--linker=cc"])
                .env("CC", "/missing/cc")
                .output()
                .unwrap(),
        );
        assert_eq!(
            Command::new(dir.join("output program"))
                .status()
                .unwrap()
                .code(),
            Some(7)
        );
    }

    #[test]
    fn environment_linker_overrides_cc() {
        let dir = package("environment");
        let path = tool(&dir, "exec cc \"$@\"");
        success(
            build(&dir)
                .env("WOLF_LD", path)
                .env("CC", "/missing/cc")
                .output()
                .unwrap(),
        );
    }

    #[test]
    fn empty_environment_and_no_selection_keep_cc() {
        let dir = package("default cc");
        for empty in [false, true] {
            let mut c = build(&dir);
            c.env("CC", fallback(&dir));
            if empty {
                c.env("WOLF_LD", "");
            }
            success(c.output().unwrap());
            assert!(dir.join("cc-was-called").is_file());
            std::fs::remove_file(dir.join("cc-was-called")).unwrap();
        }
    }

    #[test]
    fn absent_selected_linker_is_final() {
        let dir = package("absent");
        refused(
            build(&dir)
                .env("CC", fallback(&dir))
                .arg("--linker=/missing/ld02b-tool")
                .output()
                .unwrap(),
            &["selected linker: cannot run", "/missing/ld02b-tool"],
        );
        assert!(!dir.join("cc-was-called").exists());
    }

    #[test]
    fn failed_selected_linker_is_final() {
        let dir = package("failed");
        let path = tool(&dir, "echo LD02B_TOOL_ERROR >&2\nexit 37");
        refused(
            build(&dir)
                .env("CC", fallback(&dir))
                .arg("--linker")
                .arg(path)
                .output()
                .unwrap(),
            &["LD02B_TOOL_ERROR", "failed linking", "37"],
        );
        assert!(!dir.join("cc-was-called").exists());
    }

    #[test]
    fn successful_no_output_cannot_reuse_stale_executable() {
        let dir = package("stale");
        success(build(&dir).arg("--linker=cc").output().unwrap());
        let path = tool(&dir, "exit 0");
        refused(
            build(&dir).arg("--linker").arg(path).output().unwrap(),
            &["selected linker: read output program"],
        );
        assert!(!dir.join("output program").exists());
    }

    fn corrupt_output(case: &str, bytes: &[u8]) {
        let dir = package(case);
        std::fs::write(dir.join("bad output"), bytes).unwrap();
        let path = tool(&dir, "cp 'bad output' \"$2\"");
        refused(
            build(&dir)
                .env("CC", fallback(&dir))
                .arg("--linker")
                .arg(path)
                .output()
                .unwrap(),
            &["did not produce an ELF64 little-endian X86_64 executable"],
        );
        assert!(!dir.join("cc-was-called").exists());
    }

    #[test]
    fn malformed_selected_output_is_refused() {
        corrupt_output("malformed", b"not an executable");
        let mut bytes = vec![0; 20];
        bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
        bytes[18] = 62;
        corrupt_output("truncated", &bytes);
    }

    #[test]
    fn foreign_selected_output_is_refused() {
        let dir = package("foreign seed");
        success(build(&dir).output().unwrap());
        let original = std::fs::read(dir.join("output program")).unwrap();
        let mut bytes = original.clone();
        bytes[18] = 183; // EM_AARCH64.
        corrupt_output("wrong machine", &bytes);
        bytes = original.clone();
        bytes[4] = 1; // ELFCLASS32.
        corrupt_output("wrong class", &bytes);
        bytes = original;
        bytes[16] = 1; // ET_REL, not an executable.
        corrupt_output("relocatable", &bytes);
        corrupt_output("macho", &[0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1]);
        corrupt_output("coff", &[0x64, 0x86, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn object_emission_does_not_invoke_linker() {
        let dir = package("object only");
        success(
            build(&dir)
                .args(["--emit=obj", "--linker=/missing/unused"])
                .output()
                .unwrap(),
        );
    }

    #[test]
    fn run_uses_selected_linker_before_entry() {
        let dir = package("run");
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .current_dir(&dir)
            .env("WOLF_LD", "/missing/env")
            .args(["run", "--linker=cc", "main.lu"])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(7),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn every_root_manifest_program_links_with_real_ligature_on_both_tiers() {
        let Some(linker) = std::env::var_os("WOLF_TEST_LIGATURE") else {
            assert_ne!(
                std::env::var("WOLF_TEST_LIGATURE_REQUIRE").as_deref(),
                Ok("1"),
                "real ligature is required"
            );
            eprintln!(
                "SKIP: real-ligature rows require WOLF_TEST_LIGATURE plus WOLF_TEST_LIGATURE_RT and LIGATURE_SYSROOT (pin 3f206413; matching musl bundle, pax#167)"
            );
            return;
        };
        let rt = std::env::var_os("WOLF_TEST_LIGATURE_RT").expect("matching musl runtime required");
        assert!(Path::new(&linker).is_file());
        assert!(Path::new(&rt).is_file());
        let cases = [
            ("exit", "fn main() -> int { 7 }\n", 7, ""),
            (
                "hello",
                "fn main() { print(\"ld02b hello\") }\n",
                0,
                "ld02b hello\n",
            ),
            (
                "arithmetic",
                "fn main() -> int { var n = 0; for i in 0..4 { n += i }; n }\n",
                6,
                "",
            ),
        ];
        for (name, source, code, stdout) in cases {
            let dir = package(&format!("real {name}"));
            std::fs::write(dir.join("main.lu"), source).unwrap();
            for release in [false, true] {
                let mut c = build(&dir);
                c.arg("--linker")
                    .arg(&linker)
                    .env("WOLF_RT_LIB", &rt)
                    .env("CC", "/missing/cc");
                if release {
                    c.arg("--release");
                }
                success(c.output().unwrap());
                let exe = dir.join("output program");
                success(
                    Command::new(&linker)
                        .arg("verify")
                        .arg(&exe)
                        .output()
                        .unwrap(),
                );
                let out = Command::new(&exe).output().unwrap();
                assert_eq!(out.status.code(), Some(code), "{name} release={release}");
                assert_eq!(out.stdout, stdout.as_bytes());
                assert!(
                    out.stderr.is_empty(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                eprintln!("PASS: real ligature root-manifest program {name}, release={release}");
            }
        }
    }
}
