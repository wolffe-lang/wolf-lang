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

    fn script(at: &Path, name: &str, body: &str) -> PathBuf {
        std::fs::create_dir_all(at).unwrap();
        let path = at.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// The seam's own tool: an executable named `ligature`, under a
    /// directory whose name has spaces and a quote. ligature cannot order
    /// input sections yet (ligature#38), so it receives exactly the
    /// cc-shaped argv these rows assert, and objects in today's layout.
    fn tool(dir: &Path, body: &str) -> PathBuf {
        script(&dir.join("linker with spaces and 'quote"), "ligature", body)
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
        bytes = original.clone();
        bytes[24..32].fill(0); // No executable entry, as in a shared library.
        corrupt_output("missing entry", &bytes);
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

    fn assembly_package(dir: &Path) {
        std::fs::write(dir.join("wolf.pkg"), "pkg {\n name: \"ld02b/assembly\"\n version: \"0.0.1\"\n capabilities: [ffi]\n asm: [\"seven source.S\"]\n}\n").unwrap();
        std::fs::write(dir.join("seven source.S"), ".text\n.globl kw_seven\nkw_seven:\n mov $7, %eax\n ret\n.section .note.GNU-stack,\"\",@progbits\n").unwrap();
    }

    #[test]
    fn root_assembly_objects_and_trap_aliases_reach_selected_linker() {
        let dir = package("assembly and trap");
        assembly_package(&dir);
        std::fs::write(dir.join("main.lu"), "extern \"c\" fn kw_seven() -> i64\nextern \"c\" fn wolf_trap(kind: i32, file: i64, file_len: i64, line: i64, col: i64)\nfn main() -> int {\n let n = unsafe { kw_seven() }\n unsafe { wolf_trap(1, 0, 0, 0, 0) }\n n as int\n}\n").unwrap();
        let path = tool(&dir, "printf '%s\\n' \"$@\" > argv.txt\nexec cc \"$@\"");
        for release in [false, true] {
            let mut c = build(&dir);
            c.arg("--linker")
                .arg(&path)
                .arg("--assembler=as")
                .env("CC", "/missing/cc");
            if release {
                c.arg("--release");
            }
            success(c.output().unwrap());
            let argv = std::fs::read_to_string(dir.join("argv.txt")).unwrap();
            let args: Vec<_> = argv.lines().collect();
            let module = if release { "00-c00.o" } else { "00-root.o" };
            assert!(args[2].ends_with(module), "{argv}");
            assert!(args[3].ends_with("01-asm-seven source.o"), "{argv}");
            assert_eq!(
                &args[args.len() - 2..],
                &[
                    "-Wl,--undefined=__wolf_rt_trap_at",
                    "-Wl,--defsym=wolf_trap=__wolf_rt_trap_at"
                ]
            );
        }
    }

    /// Two functions, so a cold request names something besides the entry.
    const HELPER: &str = "fn helper() -> int { 7 }\nfn main() -> int { helper() }\n";

    /// A recording driver: its argv, a copy of each staged object and each
    /// generated linker script (they exist only during the link), then cc.
    const RECORD: &str = "printf '%s\\n' \"$@\" > argv.txt\nrm -rf seen\nmkdir seen\nfor a in \"$@\"; do case $a in *.o|*.ld) cp \"$a\" \"seen/${a##*/}\";; esac; done\nexec cc \"$@\"";

    /// `build` with the object cache on.
    fn cached(dir: &Path) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_wolf"));
        c.current_dir(dir)
            .env_remove("WOLF_LD")
            .env_remove("CC")
            .args(["build", "main.lu", "-o", "output program"]);
        c
    }

    fn argv(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("argv.txt"))
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    /// The cc-shaped line from `-o` on, exactly as before stable layout. The
    /// platform's default link (`link_staged`) also links GCC's unwinder
    /// statically on linux-gnu, `-static-libgcc` after the libraries: the same
    /// flag `unwind_foreign_lanes` reads from the other side, from the line its
    /// own CC receives. A selected linker's line does not carry it.
    fn assert_cc_shaped(args: &[String], all: &str, default_link: bool) {
        assert_eq!(&args[..2], ["-o", "output program"], "{all}");
        assert!(args[2].ends_with("00-root.o"), "{all}");
        assert!(args[3].ends_with("libwolf_rt.a"), "{all}");
        let mut want = vec!["-lpthread", "-ldl", "-lm"];
        if default_link && cfg!(target_env = "gnu") {
            want.push("-static-libgcc");
        }
        want.push("-Wl,--gc-sections");
        assert_eq!(&args[4..], want.as_slice(), "{all}");
    }

    /// Stable-layout input sections (hot or cold) in one staged object.
    fn ordered_sections(path: &Path) -> usize {
        use object::{Object, ObjectSection};
        let bytes = std::fs::read(path).unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        file.sections()
            .filter(|s| {
                s.name()
                    .is_ok_and(|n| n.starts_with(".text.wolf.") || n.starts_with(".wolf.cold."))
            })
            .count()
    }

    fn runs_seven(dir: &Path) {
        assert_eq!(
            Command::new(dir.join("output program"))
                .status()
                .unwrap()
                .code(),
            Some(7)
        );
    }

    #[test]
    fn ligature_refuses_cold_requests_by_name_whatever_the_verbosity() {
        let dir = package("ligature cold");
        std::fs::write(dir.join("main.lu"), HELPER).unwrap();
        let path = tool(&dir, "touch ligature-was-called\nexec cc \"$@\"");
        for (verbose, environment) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut c = build(&dir);
            c.args(["--cold-function", "helper"])
                .env("CC", fallback(&dir));
            if environment {
                c.env("WOLF_LD", &path);
            } else {
                c.arg("--linker").arg(&path);
            }
            if verbose {
                c.arg("--verbose");
            }
            let out = c.output().unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert_eq!(
                out.status.code(),
                Some(4),
                "verbose={verbose} environment={environment}: {err}"
            );
            assert!(
                err.contains(
                    "cold function placement with ligature is not supported (`--cold-function`)"
                ),
                "{err}"
            );
            assert!(!dir.join("ligature-was-called").exists());
            assert!(!dir.join("cc-was-called").exists());
            assert!(!dir.join("output program").exists());
        }
    }

    #[test]
    fn ligature_keeps_todays_layout_and_notes_it_only_under_verbose() {
        let dir = package("ligature layout");
        std::fs::write(dir.join("main.lu"), HELPER).unwrap();
        let ligature = tool(&dir, RECORD);
        let system = script(&dir.join("system driver"), "system cc", RECORD);
        // Through the object cache, both ways: a system driver's ordered
        // objects never reach ligature, and ligature's never reach the driver.
        for round in 0..2 {
            success(cached(&dir).arg("--linker").arg(&system).output().unwrap());
            assert!(
                ordered_sections(&dir.join("seen/00-root.o")) > 0,
                "round {round}: a system driver links ordered objects"
            );
            let out = cached(&dir)
                .arg("--linker")
                .arg(&ligature)
                .output()
                .unwrap();
            let err = String::from_utf8_lossy(&out.stderr).into_owned();
            assert!(out.status.success(), "{err}");
            assert_eq!(
                err, "",
                "round {round}: an ordinary ligature build is silent"
            );
            let args = argv(&dir);
            assert_cc_shaped(&args, &args.join("\n"), false);
            assert_eq!(
                ordered_sections(&dir.join("seen/00-root.o")),
                0,
                "round {round}: ligature receives today's layout"
            );
            assert!(!dir.join("seen/hot.ld").exists() && !dir.join("seen/cold.ld").exists());
            runs_seven(&dir);
        }
        let out = build(&dir)
            .arg("--verbose")
            .arg("--linker")
            .arg(&ligature)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(out.status.success(), "{err}");
        assert_eq!(
            err.matches("wolf build: note: the stable layout guarantee does not apply to ligature")
                .count(),
            1,
            "{err}"
        );
        runs_seven(&dir);
    }

    #[test]
    fn system_linker_paths_receive_the_ordering_scripts() {
        let dir = package("system layout");
        std::fs::write(dir.join("main.lu"), HELPER).unwrap();
        let driver = script(
            &dir.join("linker with spaces and 'quote"),
            "system cc",
            RECORD,
        );
        for (default_path, cold) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut c = build(&dir);
            if default_path {
                c.env("CC", &driver);
            } else {
                c.arg("--linker").arg(&driver).env("CC", "/missing/cc");
            }
            if cold {
                c.args(["--cold-function", "helper"]);
            }
            success(c.output().unwrap());
            let args = argv(&dir);
            let all = format!("default={default_path} cold={cold}\n{}", args.join("\n"));
            // The ordering block first: section sorting, then each script.
            assert_eq!(
                &args[..4],
                ["-Wl,--sort-section=name", "-Xlinker", "-T", "-Xlinker"],
                "{all}"
            );
            assert!(args[4].ends_with("/hot.ld"), "{all}");
            let mut rest = &args[5..];
            if cold {
                assert_eq!(&rest[..3], ["-Xlinker", "-T", "-Xlinker"], "{all}");
                assert!(rest[3].ends_with("/cold.ld"), "{all}");
                rest = &rest[4..];
            }
            // Then the cc-shaped line unchanged; the default link may ask for lld.
            if default_path && rest.last().is_some_and(|a| a == "-fuse-ld=lld") {
                rest = &rest[..rest.len() - 1];
            }
            assert_cc_shaped(rest, &all, default_path);
            let hot = std::fs::read_to_string(dir.join("seen/hot.ld")).unwrap();
            assert!(
                hot.contains("*(SORT_BY_NAME(.text.wolf.*))")
                    && hot.contains("INSERT BEFORE .text"),
                "{hot}"
            );
            let cold_script = dir.join("seen/cold.ld");
            if cold {
                let text = std::fs::read_to_string(&cold_script).unwrap();
                assert!(
                    text.contains("*(SORT_BY_NAME(.wolf.cold.*))")
                        && text.contains("INSERT AFTER .text"),
                    "{text}"
                );
            } else {
                assert!(!cold_script.exists(), "{all}");
            }
            assert!(ordered_sections(&dir.join("seen/00-root.o")) > 0, "{all}");
            runs_seven(&dir);
        }
    }

    #[test]
    fn every_root_manifest_program_links_with_real_ligature_on_both_tiers() {
        let linker = std::env::var_os("WOLF_TEST_LIGATURE")
            .expect("real ligature is required: run ci/build-test-ligature and ci/fetch-test-link-bundle (ligature#13)");
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
                "assembly",
                "extern \"c\" fn kw_seven() -> i64\nfn main() -> int {\n let n = unsafe { kw_seven() }\n n as int\n}\n",
                7,
                "",
            ),
        ];
        // Compile every fixture on both tiers before the expensive links.
        // A source error must not hide behind several long real-tool calls.
        for (name, source, _, _) in cases {
            let dir = package(&format!("preflight {name}"));
            std::fs::write(dir.join("main.lu"), source).unwrap();
            if name == "assembly" {
                assembly_package(&dir);
            }
            for release in [false, true] {
                let mut c = build(&dir);
                c.args(["--emit=obj", "--assembler=as"]);
                if release {
                    c.arg("--release");
                }
                success(c.output().unwrap());
            }
        }
        for (name, source, code, stdout) in cases {
            let dir = package(&format!("real {name}"));
            std::fs::write(dir.join("main.lu"), source).unwrap();
            if name == "assembly" {
                assembly_package(&dir);
            }
            for release in [false, true] {
                let mut c = build(&dir);
                c.arg("--assembler=as")
                    .arg("--linker")
                    .arg(&linker)
                    .env("WOLF_RT_LIB", &rt)
                    .env("CC", "/missing/cc");
                if release {
                    c.arg("--release");
                }
                let tier = if release { "release" } else { "native" };
                let started = std::time::Instant::now();
                let start_unix_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                eprintln!(
                    "LINK_CASE_START scope=BUILD+LINK program={name} tier={tier} start_unix_ms={start_unix_ms}"
                );
                let output = c.output();
                eprintln!(
                    "LINK_CASE_END scope=BUILD+LINK program={name} tier={tier} elapsed_ms={} status={:?}",
                    started.elapsed().as_millis(),
                    output.as_ref().map(|out| out.status.code())
                );
                success(output.unwrap());
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
