//! as02a: an explicit assembler is a program path, not a C driver or shell
//! command. Failures are final. Real-clef rows are strict when
//! WOLF_TEST_CLEF_REQUIRE=1; ordinary platform runs report their skip loudly.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn package(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("assembler_select")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("wolf.pkg"), "pkg {\n name: \"as02a/test\"\n version: \"0.0.1\"\n asm: [\"seven source.S\"]\n capabilities: [ffi]\n}\n").unwrap();
    std::fs::write(dir.join("main.lu"), "extern \"c\" fn kw_seven() -> i64\n\nfn main() -> int {\n let n = unsafe { kw_seven() }\n n as int\n}\n").unwrap();
    std::fs::write(dir.join("seven source.S"), ".text\n.globl kw_seven\nkw_seven:\n mov $7, %eax\n ret\n.section .note.GNU-stack,\"\",@progbits\n").unwrap();
    dir
}

fn build(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_wolf"));
    c.current_dir(dir).env_remove("WOLF_AS").args([
        "build",
        "main.lu",
        "--no-cache",
        "--emit=obj",
        "-o",
        "output object.o",
    ]);
    c
}

fn refused(out: Output, fragments: &[&str]) {
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "must refuse, stdout={:?}",
        out.stdout
    );
    for fragment in fragments {
        assert!(err.contains(fragment), "missing {fragment:?}: {err}");
    }
}

#[test]
fn assembler_option_requires_a_nonempty_path() {
    let dir = package("usage");
    for args in [
        vec!["--assembler"],
        vec!["--assembler="],
        vec!["--assembler", ""],
    ] {
        refused(
            build(&dir).args(args).output().unwrap(),
            &["--assembler needs a path"],
        );
    }
}

#[test]
fn an_explicit_assembler_does_not_enable_foreign_targets() {
    let dir = package("foreign target");
    refused(
        build(&dir)
            .args(["--target=not-a-wolf-target", "--assembler=missing"])
            .output()
            .unwrap(),
        &["unknown target `not-a-wolf-target`"],
    );
}

#[cfg(windows)]
#[test]
fn an_explicit_assembler_does_not_enable_hosted_windows() {
    let dir = package("windows");
    refused(
        build(&dir).arg("--assembler=missing").output().unwrap(),
        &["a hosted windows build has no assembler step yet"],
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tool(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("assembler with spaces");
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

    #[test]
    fn configured_path_and_source_and_output_spaces_preserve_argv() {
        let dir = package("quoted paths");
        let as_path = tool(&dir, "printf '%s\\n' \"$@\" > argv.txt\nexec as \"$@\"");
        success(
            build(&dir)
                .arg("--assembler")
                .arg(as_path)
                .output()
                .unwrap(),
        );
        let argv = std::fs::read_to_string(dir.join("argv.txt")).unwrap();
        let args: Vec<_> = argv.lines().collect();
        assert_eq!(args.len(), 4, "{argv}");
        assert_eq!(
            &args[..3],
            &["--64", "-o", "output object.asm-seven source.o"]
        );
        assert_eq!(args[3], "seven source.S", "{argv}");
        assert!(dir.join("output object.asm-seven source.o").is_file());
    }

    #[test]
    fn cli_assembler_overrides_environment() {
        let dir = package("precedence");
        let as_path = tool(&dir, "exec as \"$@\"");
        success(
            build(&dir)
                .env("WOLF_AS", "/missing/env-assembler")
                .arg(format!("--assembler={}", as_path.display()))
                .output()
                .unwrap(),
        );
    }

    #[test]
    fn environment_assembler_overrides_legacy_driver() {
        let dir = package("environment");
        let as_path = tool(&dir, "exec as \"$@\"");
        success(
            build(&dir)
                .env("WOLF_AS", as_path)
                .env("WOLF_CLANG", "/missing/clang")
                .arg("--target=x86_64-unknown-none")
                .output()
                .unwrap(),
        );
    }

    #[test]
    fn empty_environment_keeps_legacy_driver() {
        let dir = package("empty environment");
        success(build(&dir).env("WOLF_AS", "").output().unwrap());
    }

    #[test]
    fn missing_selected_assembler_is_final() {
        let dir = package("missing");
        refused(
            build(&dir)
                .arg("--assembler=/missing/as02a-tool")
                .output()
                .unwrap(),
            &["seven source.S", "cannot run", "/missing/as02a-tool"],
        );
    }

    #[test]
    fn failed_selected_assembler_is_final() {
        let dir = package("nonzero");
        let as_path = tool(&dir, "echo as02a-planted-tool-failure >&2\nexit 37");
        // The legacy driver would succeed: retrying it makes this row red.
        refused(
            build(&dir)
                .arg("--assembler")
                .arg(as_path)
                .output()
                .unwrap(),
            &[
                "seven source.S",
                "did not assemble",
                "37",
                "as02a-planted-tool-failure",
            ],
        );
        assert!(!dir.join("output object.asm-seven source.o").exists());
    }

    fn corrupt_output(case: &str, bytes: &[u8]) {
        let dir = package(case);
        std::fs::write(dir.join("bad.o"), bytes).unwrap();
        let as_path = tool(&dir, "cp bad.o \"$3\"");
        for target in [None, Some("--target=x86_64-unknown-none")] {
            let mut command = build(&dir);
            command.arg("--assembler").arg(&as_path);
            if let Some(target) = target {
                command.arg(target);
            }
            refused(
                command.output().unwrap(),
                &["seven source.S", "did not produce an ELF x86-64 object"],
            );
        }
    }

    #[test]
    fn malformed_selected_output_is_refused() {
        corrupt_output("malformed", b"not an object");
        let mut truncated = vec![0; 20];
        truncated[..6].copy_from_slice(b"\x7fELF\x02\x01");
        truncated[18] = 62;
        corrupt_output("truncated header", &truncated);
    }

    #[test]
    fn coff_selected_output_is_refused() {
        corrupt_output(
            "coff",
            &[
                0x64, 0x86, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        );
    }

    #[test]
    fn macho_selected_output_is_refused() {
        corrupt_output(
            "macho",
            &[0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0],
        );
    }

    #[test]
    fn wrong_machine_and_class_selected_output_are_refused() {
        let dir = package("wrong machine seed");
        success(
            Command::new("as")
                .current_dir(&dir)
                .args(["--64", "-o", "seed.o", "seven source.S"])
                .output()
                .unwrap(),
        );
        let original = std::fs::read(dir.join("seed.o")).unwrap();
        let mut bytes = original.clone();
        bytes[18] = 183; // EM_AARCH64, not the selected target.
        corrupt_output("wrong machine", &bytes);
        bytes = original;
        bytes[4] = 1; // ELFCLASS32.
        corrupt_output("wrong class", &bytes);
    }

    fn clef() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("WOLF_TEST_CLEF") {
            let path = PathBuf::from(path);
            assert!(
                path.is_file(),
                "WOLF_TEST_CLEF must name the built clef: {}",
                path.display()
            );
            Some(path)
        } else {
            assert_ne!(
                std::env::var("WOLF_TEST_CLEF_REQUIRE").as_deref(),
                Ok("1"),
                "real clef is required"
            );
            eprintln!(
                "SKIP: real-clef assembler rows require WOLF_TEST_CLEF (tools/build-test-clef, pin 996a3ccd)"
            );
            None
        }
    }

    #[test]
    fn every_root_manifest_source_uses_real_clef_on_both_tiers() {
        let Some(clef) = clef() else { return };
        let dir = package("real clef all sources");
        std::fs::write(
            dir.join("other.S"),
            ".text\n.globl another\nanother:\n ret\n.section .note.GNU-stack,\"\",@progbits\n",
        )
        .unwrap();
        let manifest = std::fs::read_to_string(dir.join("wolf.pkg"))
            .unwrap()
            .replace("[\"seven source.S\"]", "[\"seven source.S\", \"other.S\"]");
        std::fs::write(dir.join("wolf.pkg"), manifest).unwrap();
        for release in [false, true] {
            for target in [None, Some("--target=x86_64-unknown-none")] {
                let mut c = build(&dir);
                c.env("WOLF_AS", &clef);
                if release {
                    c.arg("--release");
                }
                if let Some(target) = target {
                    c.arg(target);
                }
                success(c.output().unwrap());
                for source in ["seven source", "other"] {
                    use object::{Object, ObjectSymbol};
                    let bytes =
                        std::fs::read(dir.join(format!("output object.asm-{source}.o"))).unwrap();
                    let file = object::File::parse(&*bytes).unwrap();
                    let symbol = if source == "other" {
                        "another"
                    } else {
                        "kw_seven"
                    };
                    assert!(
                        file.symbols()
                            .any(|s| s.name() == Ok(symbol) && !s.is_undefined())
                    );
                }
            }
        }
    }

    #[test]
    fn pinned_corpus_sources_all_pass_through_the_root_manifest() {
        let Some(clef) = clef() else { return };
        let root = clef.parent().unwrap().parent().unwrap();
        let inventory: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("census/repositories.json")).unwrap())
                .unwrap();
        let dir = package("real clef pinned corpus");
        // No assembly is executed by this corpus row: it emits objects only.
        std::fs::write(dir.join("main.lu"), "export fn answer() -> i64 { 7 }\n").unwrap();
        let mut listed = Vec::new();
        for repo in inventory.as_array().unwrap() {
            for source in repo["files"].as_array().unwrap() {
                let source = source.as_str().unwrap();
                if source.ends_with("seven_aarch64_macos.S") {
                    eprintln!("REFUSED BY SCOPE: seven_aarch64_macos.S (clef is ELF x86-64 only)");
                    continue;
                }
                let source_path = root
                    .join("census/sources")
                    .join(repo["repository"].as_str().unwrap())
                    .join(source);
                let name = format!("corpus {}.S", listed.len());
                std::fs::copy(source_path, dir.join(&name)).unwrap();
                listed.push(format!("\"{name}\""));
            }
        }
        assert_eq!(listed.len(), 43, "the pinned source census changed");
        std::fs::write(
            dir.join("wolf.pkg"),
            format!(
                "pkg {{\n name: \"as02a/corpus\"\n version: \"0.0.1\"\n asm: [{}]\n}}\n",
                listed.join(", ")
            ),
        )
        .unwrap();
        for release in [false, true] {
            let mut c = build(&dir);
            c.env("WOLF_AS", &clef).arg("--target=x86_64-unknown-none");
            if release {
                c.arg("--release");
            }
            success(c.output().unwrap());
            for i in 0..43 {
                use object::Object;
                let bytes =
                    std::fs::read(dir.join(format!("output object.asm-corpus {i}.o"))).unwrap();
                let file = object::File::parse(&*bytes).unwrap();
                assert_eq!(file.architecture(), object::Architecture::X86_64);
                assert_eq!(file.kind(), object::ObjectKind::Relocatable);
            }
            eprintln!("PASS: all 43 root-manifest sources via real clef; release={release}");
        }
    }

    #[test]
    fn fresh_package_links_and_runs_with_real_clef() {
        let Some(clef) = clef() else { return };
        assert!(
            Command::new(env!("CARGO"))
                .args(["build", "-p", "wolf_rt"])
                .status()
                .unwrap()
                .success()
        );
        let dir = package("fresh real clef package");
        for release in [false, true] {
            let mut c = Command::new(env!("CARGO_BIN_EXE_wolf"));
            c.current_dir(&dir)
                .env("WOLF_AS", "/missing/env-tool")
                .args([
                    "build",
                    "main.lu",
                    "--no-cache",
                    "-o",
                    "fresh program",
                    "--assembler",
                ])
                .arg(&clef);
            if release {
                c.arg("--release");
            }
            success(c.output().unwrap());
            assert_eq!(
                Command::new(dir.join("fresh program"))
                    .status()
                    .unwrap()
                    .code(),
                Some(7)
            );
        }
    }
}
