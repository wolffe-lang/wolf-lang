//! s267 (wolf-lang#962, `[abi.target.main]`): the hosted entry hands
//! `argc`/`argv` to the runtime itself, so a static musl link answers the
//! argv readers exactly as a glibc link does.
//!
//! Twenty-four cells: four readers (`env_args`, `env_arg0`,
//! `env_args_bytes` with a non-UTF-8 argument, `env_vars` under a fixed
//! environment) x two tiers (native, release) x three links of ONE object
//! per reader and tier: `cc` against glibc and the host `libwolf_rt.a`
//! (what `wolf build` runs), and the musl link bundle's sysroot (ligature's
//! `link-bundle-2026-10-25`, fetched by digest by `ci/fetch-entry-bundle`)
//! with a musl `libwolf_rt.a` built from this checkout, by `ld.lld` and by
//! GNU `ld`. Each link is its own test, so a red run names the links that
//! moved; each prints every cell (`cell ...` on stderr) before it judges.
//!
//! Inputs: `WOLF_STATIC_ENTRY_SYSROOT` (`crt1.o`, `crti.o`, `crtn.o`,
//! `libc.a`, `libunwind.a`), `WOLF_STATIC_ENTRY_MUSL_RT` (the musl
//! runtime), `WOLF_STATIC_ENTRY_LLD` and `WOLF_STATIC_ENTRY_BFD` (default
//! `ld.lld`, `ld.bfd`). A missing input skips its cells LOUDLY by name and
//! counts them; with `WOLF_STATIC_ENTRY_REQUIRE=1` (CI) it is a failure.
//! `WOLF_STATIC_ENTRY_TABLE=DIR` also writes each link's cells as JSON
//! there (the lane note's before and after tables). Linux x86-64 only;
//! no signals, process groups or host process-table probes.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::OnceLock;

    /// The invocation name every cell runs under: not the path, and with a
    /// space, so a reader that answers the path or splits it is wrong.
    const NAME: &str = "s267 name";
    /// The arguments: a flag, a word with a space, an empty argument.
    const ARGS: [&[u8]; 3] = [b"--version", b"space word", b""];
    /// `env_args_bytes`' extra argument: not UTF-8.
    const RAW: &[u8] = b"\xff\xfe";
    /// The whole environment of every cell. `S267_D` is not UTF-8, so
    /// `env_vars` leaves it out.
    const ENV: [(&str, &[u8]); 4] = [
        ("S267_A", b"1"),
        ("S267_B", b"space word"),
        ("S267_C", b""),
        ("S267_D", b"\xff"),
    ];

    struct Reader {
        name: &'static str,
        source: &'static str,
        raw_arg: bool,
    }

    const READERS: [Reader; 4] = [
        Reader {
            name: "env_args",
            source: "fn main() {\n    let args = env_args()\n    print(\"argc={args.len}\")\n    for arg in args { print(\"[{arg}]\") }\n}\n",
            raw_arg: false,
        },
        Reader {
            name: "env_arg0",
            source: "fn main() {\n    let name = env_arg0()\n    print(\"[{name}]\")\n}\n",
            raw_arg: false,
        },
        Reader {
            name: "env_args_bytes",
            source: "fn main() {\n    let argv = env_args_bytes()\n    print(\"argc={argv.len}\")\n    for arg in argv {\n        for b in arg { print(\"{b as int}\") }\n        print(\"end\")\n    }\n}\n",
            raw_arg: true,
        },
        Reader {
            name: "env_vars",
            source: "fn main() {\n    let vars = env_vars()\n    print(\"count={vars.len}\")\n    for v in vars { print(\"[{v}]\") }\n}\n",
            raw_arg: false,
        },
    ];

    const TIERS: [&str; 2] = ["native", "release"];

    fn expected(reader: &str) -> String {
        match reader {
            "env_args" => {
                let mut s = format!("argc={}\n", ARGS.len());
                for a in ARGS {
                    s.push_str(&format!("[{}]\n", String::from_utf8_lossy(a)));
                }
                s
            }
            "env_arg0" => format!("[{NAME}]\n"),
            "env_args_bytes" => {
                let mut argv: Vec<&[u8]> = vec![NAME.as_bytes()];
                argv.extend(ARGS);
                argv.push(RAW);
                let mut s = format!("argc={}\n", argv.len());
                for a in argv {
                    for b in a {
                        s.push_str(&format!("{b}\n"));
                    }
                    s.push_str("end\n");
                }
                s
            }
            "env_vars" => {
                let mut kv: Vec<String> = ENV
                    .iter()
                    .filter_map(|(k, v)| Some(format!("{k}={}", std::str::from_utf8(v).ok()?)))
                    .collect();
                kv.sort();
                let mut s = format!("count={}\n", kv.len());
                for e in kv {
                    s.push_str(&format!("[{e}]\n"));
                }
                s
            }
            other => unreachable!("reader {other}"),
        }
    }

    fn required() -> bool {
        std::env::var("WOLF_STATIC_ENTRY_REQUIRE").as_deref() == Ok("1")
    }

    /// A skipped cell set: loud, counted, and a failure under REQUIRE.
    fn skip(link: &str, why: &str) {
        let line = format!("SKIP static_entry {link}: {why} (4 readers x 2 tiers = 8 cells)");
        eprintln!("{line}");
        assert!(!required(), "{line}; WOLF_STATIC_ENTRY_REQUIRE=1");
    }

    fn root() -> PathBuf {
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("s267-static-entry")
    }

    /// One reader's object set on one tier: `wolf build --emit=obj` into
    /// a directory of its own (a build that splits into several objects
    /// writes them all there).
    struct Objects {
        tier: &'static str,
        reader: &'static str,
        files: Vec<PathBuf>,
    }

    /// Every (tier, reader) object, built once for the three links. A
    /// tier that cannot build (no clang for the release tier) is `Err`.
    fn objects() -> &'static Vec<Result<Objects, String>> {
        static OBJECTS: OnceLock<Vec<Result<Objects, String>>> = OnceLock::new();
        OBJECTS.get_or_init(|| {
            let mut all = Vec::new();
            for tier in TIERS {
                for r in &READERS {
                    all.push(build(tier, r));
                }
            }
            all
        })
    }

    fn build(tier: &'static str, r: &Reader) -> Result<Objects, String> {
        let dir = root().join(format!("{tier}-{}", r.name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("obj")).unwrap();
        let src = dir.join("main.lu");
        std::fs::write(&src, r.source).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_wolf"));
        cmd.arg("build")
            .arg(&src)
            .args(["--no-cache", "--emit=obj", "-o"])
            .arg(dir.join("obj/main.o"));
        if tier == "release" {
            cmd.arg("--release");
        }
        let out = cmd.output().expect("wolf build runs");
        if !out.status.success() {
            return Err(format!(
                "wolf build {tier} {}: status {:?}: {}",
                r.name,
                out.status.code(),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("obj"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension() == Some(OsStr::new("o")))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "wolf build {tier} {} wrote no object", r.name);
        Ok(Objects {
            tier,
            reader: r.name,
            files,
        })
    }

    /// The host runtime `wolf build` links: next to the `wolf` binary, as
    /// the driver finds it, or `WOLF_RT_LIB`.
    fn glibc_rt() -> PathBuf {
        if let Some(p) = std::env::var_os("WOLF_RT_LIB").filter(|v| !v.is_empty()) {
            return PathBuf::from(p);
        }
        Path::new(env!("CARGO_BIN_EXE_wolf"))
            .parent()
            .unwrap()
            .join("libwolf_rt.a")
    }

    enum Link {
        Glibc,
        Musl { linker: PathBuf },
    }

    struct Musl {
        sysroot: PathBuf,
        rt: PathBuf,
    }

    /// The musl inputs, or the reason the musl cells skip.
    fn musl() -> Result<Musl, String> {
        let sysroot = std::env::var_os("WOLF_STATIC_ENTRY_SYSROOT")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or("WOLF_STATIC_ENTRY_SYSROOT is unset (ci/fetch-entry-bundle)")?;
        for f in ["crt1.o", "crti.o", "crtn.o", "libc.a", "libunwind.a"] {
            if !sysroot.join(f).is_file() {
                return Err(format!("{} has no {f}", sysroot.display()));
            }
        }
        let rt = std::env::var_os("WOLF_STATIC_ENTRY_MUSL_RT")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or("WOLF_STATIC_ENTRY_MUSL_RT is unset (a musl libwolf_rt.a from this checkout)")?;
        if !rt.is_file() {
            return Err(format!("{} is not a file", rt.display()));
        }
        Ok(Musl { sysroot, rt })
    }

    /// A linker by variable or default name, or the reason it is absent.
    fn linker(var: &str, default: &str) -> Result<PathBuf, String> {
        let name = std::env::var(var).unwrap_or_else(|_| default.to_string());
        match Command::new(&name).arg("--version").output() {
            Ok(o) if o.status.success() => {
                let first = String::from_utf8_lossy(&o.stdout);
                eprintln!(
                    "static_entry: {var}={name}: {}",
                    first.lines().next().unwrap_or("")
                );
                Ok(PathBuf::from(name))
            }
            Ok(o) => Err(format!("{name} --version: status {:?}", o.status.code())),
            Err(e) => Err(format!("{name} ({var}): {e}")),
        }
    }

    fn link(o: &Objects, how: &Link, out: &Path) -> Result<(), String> {
        let mut cmd = match how {
            Link::Glibc => {
                // What the driver's unix link passes (`link_staged`), less
                // the `-fuse-ld=lld` it adds where lld is found.
                let mut c = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()));
                c.arg("-o").arg(out).args(&o.files).arg(glibc_rt());
                c.args(["-lpthread", "-ldl", "-lm", "-Wl,--gc-sections"]);
                c
            }
            Link::Musl { linker } => {
                let m = musl().expect("musl inputs were checked");
                let s = |f: &str| m.sysroot.join(f);
                let mut c = Command::new(linker);
                c.args(["-static", "--gc-sections", "-o"]).arg(out);
                c.arg(s("crt1.o")).arg(s("crti.o")).arg("--start-group");
                c.args(&o.files).arg(&m.rt).arg(s("libc.a")).arg(s("libunwind.a"));
                c.arg("--end-group").arg(s("crtn.o"));
                c
            }
        };
        let r = cmd.output().map_err(|e| format!("link: {e}"))?;
        if r.status.success() {
            Ok(())
        } else {
            Err(format!(
                "link status {:?}: {}",
                r.status.code(),
                String::from_utf8_lossy(&r.stderr)
            ))
        }
    }

    fn escape(b: &[u8]) -> String {
        b.escape_ascii().to_string()
    }

    /// One link's eight cells: build (shared), link, run, judge; every cell
    /// printed before the verdict.
    fn cells(label: &str, how: Link) {
        let dir = root().join(label);
        std::fs::create_dir_all(&dir).unwrap();
        let mut wrong = Vec::new();
        let mut rows = Vec::new();
        let mut skipped = 0;
        for built in objects() {
            let o = match built {
                Ok(o) => o,
                Err(e) if e.contains("clang") && !required() => {
                    eprintln!("SKIP static_entry {label}: release tier unavailable: {e}");
                    skipped += 1;
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            let bin = dir.join(format!("{}-{}", o.tier, o.reader));
            let want = expected(o.reader);
            let (status, stdout, verdict) = match link(o, &how, &bin) {
                Err(e) => (None, Vec::new(), format!("LINK FAILED: {e}")),
                Ok(()) => {
                    let reader = READERS.iter().find(|r| r.name == o.reader).unwrap();
                    let mut run = Command::new(&bin);
                    run.arg0(NAME)
                        .args(ARGS.iter().map(|a| OsStr::from_bytes(a)))
                        .env_clear()
                        .envs(ENV.iter().map(|(k, v)| (k, OsStr::from_bytes(v))));
                    if reader.raw_arg {
                        run.arg(OsStr::from_bytes(RAW));
                    }
                    let r = run.output().expect("the cell's binary runs");
                    let ok = r.status.success() && r.stdout == want.as_bytes();
                    (
                        r.status.code(),
                        r.stdout,
                        if ok { "right".to_string() } else { "WRONG".to_string() },
                    )
                }
            };
            eprintln!(
                "cell {} {label} {}: {verdict} exit={status:?} stdout=\"{}\"",
                o.tier,
                o.reader,
                escape(&stdout)
            );
            if verdict != "right" {
                wrong.push(format!(
                    "{} {label} {}: {verdict} (wanted \"{}\")",
                    o.tier,
                    o.reader,
                    escape(want.as_bytes())
                ));
            }
            rows.push(serde_json::json!({
                "tier": o.tier,
                "link": label,
                "reader": o.reader,
                "verdict": verdict,
                "exit": status,
                "stdout": escape(&stdout),
            }));
        }
        if let Some(t) = std::env::var_os("WOLF_STATIC_ENTRY_TABLE").filter(|v| !v.is_empty()) {
            let t = PathBuf::from(t);
            std::fs::create_dir_all(&t).unwrap();
            std::fs::write(
                t.join(format!("{label}.json")),
                serde_json::to_string_pretty(&rows).unwrap() + "\n",
            )
            .unwrap();
        }
        eprintln!(
            "static_entry {label}: {} cells, {} right, {} wrong, {skipped} skipped",
            rows.len(),
            rows.len() - wrong.len(),
            wrong.len()
        );
        assert!(wrong.is_empty(), "{label}: wrong cells:\n{}", wrong.join("\n"));
    }

    #[test]
    fn glibc_cells_answer_the_invocation() {
        cells("cc-glibc", Link::Glibc);
    }

    #[test]
    fn musl_cells_linked_by_lld_answer_the_invocation() {
        if let Err(e) = musl() {
            return skip("lld-musl", &e);
        }
        match linker("WOLF_STATIC_ENTRY_LLD", "ld.lld") {
            Ok(l) => cells("lld-musl", Link::Musl { linker: l }),
            Err(e) => skip("lld-musl", &e),
        }
    }

    #[test]
    fn musl_cells_linked_by_gnu_ld_answer_the_invocation() {
        if let Err(e) = musl() {
            return skip("gnu-ld-musl", &e);
        }
        match linker("WOLF_STATIC_ENTRY_BFD", "ld.bfd") {
            Ok(l) => cells("gnu-ld-musl", Link::Musl { linker: l }),
            Err(e) => skip("gnu-ld-musl", &e),
        }
    }
}
