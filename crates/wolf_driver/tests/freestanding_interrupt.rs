//! kw10 (K7 = B, STATUS #31): `[abi.interrupt]`. Wolf has no interrupt
//! calling convention; a vector enters through an assembly trampoline
//! that builds a `#[repr(c)]` frame, calls an `export fn` handler under
//! the C convention and returns with `iretq`.
//!
//! Rows:
//!
//! 1. the clause, the frame and the trampoline agree (every host): the
//!    spec's `[abi.interrupt]` names the 22 fields in order with
//!    `size_of` 176 and `rip` at 136; the witness's `struct Frame` lists
//!    the same fields in the same order; `isr.S`'s common path pushes
//!    the fifteen general registers in the reverse of the struct's
//!    first fifteen fields;
//! 2. the objects (unix hosts: assembling needs a C driver): the native
//!    tier builds the witness for `x86_64-unknown-none` and leaves
//!    `K.asm-isr.o` beside it; the wolf object defines `kw_interrupt`
//!    and `kmain` as global functions and imports the trampolines' table
//!    and the IDT by name; in the assembly object every trampoline
//!    starts with the pushes its vector's error-code shape needs (a zero
//!    then the vector for 3 and 6, the vector alone for 13) and the
//!    common path ends in `iretq` (48 cf);
//! 3. the image (x86-64 linux): both tiers' objects, the consumer's own
//!    PVH boot path (`boot.S`, not listed) and `isr.S`'s object link
//!    with `ld.lld -T interrupt.ld` (no libc, no runtime), and QEMU
//!    boots it with `-kernel`: wolf fills the IDT, loads it, takes an
//!    `int3` (a trap, returns), a `ud2` and a `#GP` with error code
//!    0x1234 (faults, resumed past the instruction), and COM1 reads
//!
//!        KWI frame 176 rip 136 vectors 3 6 13 error 0x1234 cs 0x8 ok
//!
//!    with QEMU's status 33 (the kernel's 16 through isa-debug-exit).
//!    Without `qemu-system-x86_64` on the PATH this row prints one SKIP
//!    line and returns, unless `WOLF_INTERRUPT_REQUIRE_QEMU=1` (the
//!    linux CI job sets it), when it fails: a skip is never a pass where
//!    the instrument is supposed to be.
//!
//! At trunk 6a4e6151 the compiler already builds and boots the witness
//! (K7 = B asks nothing new of it); what is red there is row 1, the
//! clause it holds the trampoline to (`[abi.interrupt]` did not exist).
//! A failed build FAILS here, whatever its exit status (wolf-lang#550).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The frame's fields, lowest address first ([abi.interrupt] (3)).
const FRAME: [&str; 22] = [
    "r15", "r14", "r13", "r12", "r11", "r10", "r9", "r8", "rbp", "rdi", "rsi", "rdx", "rcx", "rbx",
    "rax", "vector", "error", "rip", "cs", "rflags", "rsp", "ss",
];

const SERIAL: &str = "KWI frame 176 rip 136 vectors 3 6 13 error 0x1234 cs 0x8 ok";

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/freestanding_interrupt")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The `[abi.interrupt]` paragraph of spec/04-abi.md, whitespace folded.
fn clause() -> String {
    let spec = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/04-abi.md"));
    let start = spec
        .find("- `[abi.interrupt]`")
        .expect("spec/04-abi.md has an `[abi.interrupt]` clause");
    let rest = &spec[start..];
    let end = rest[2..].find("\n- `[").map_or(rest.len(), |i| i + 2);
    let end = rest[..end].find("\n---").unwrap_or(end);
    rest[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_clause_the_frame_and_the_trampoline_agree() {
    // The clause states the layout and the numbers.
    let c = clause();
    let order = FRAME.join(" ");
    assert!(
        c.contains(&format!("`{order}`")),
        "[abi.interrupt] lists the frame's fields in order (`{order}`): {c}"
    );
    for want in [
        "`size_of` is 176",
        "120 for `vector`",
        "128 for `error`",
        "136 for `rip`",
        "8, 10, 11, 12, 13, 14, 17 and 21",
        "29 and 30",
        "`iretq`",
    ] {
        assert!(c.contains(want), "[abi.interrupt] states `{want}`: {c}");
    }
    // The witness's struct lists the same fields in the same order.
    let src = read(&fixture("kmain_interrupt.lu"));
    let body = src
        .split("struct Frame {")
        .nth(1)
        .and_then(|s| s.split('}').next())
        .expect("kmain_interrupt.lu declares struct Frame");
    let fields: Vec<&str> = body
        .lines()
        .filter_map(|l| l.trim().strip_suffix(": u64,"))
        .collect();
    assert_eq!(
        fields, FRAME,
        "the witness's Frame is the clause's, field for field"
    );
    assert!(
        src.contains("#[repr(c)]\nstruct Frame {"),
        "the frame is #[repr(c)]"
    );
    // The trampoline's common path pushes rax..r15, the reverse of the
    // struct's first fifteen fields.
    let isr = read(&fixture("isr.S"));
    let common = isr
        .split("kw_isr_common:\n")
        .nth(1)
        .and_then(|s| s.split("movq    %rsp, %rdi").next())
        .expect("isr.S has kw_isr_common's pushes");
    let pushed: Vec<&str> = common
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pushq   %"))
        .collect();
    let mut want: Vec<&str> = FRAME[..15].to_vec();
    want.reverse();
    assert_eq!(
        pushed, want,
        "kw_isr_common pushes %rax first and %r15 last"
    );
}

#[cfg(unix)]
mod objects {
    use super::*;

    pub const TARGET: &str = "x86_64-unknown-none";

    fn wolf() -> &'static str {
        env!("CARGO_BIN_EXE_wolf")
    }

    fn text(b: &[u8]) -> String {
        String::from_utf8_lossy(b).into_owned()
    }

    /// A scratch package: the witness, its trampolines and its manifest.
    pub fn package(case: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("freestanding_interrupt")
            .join(case);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        for name in ["kmain_interrupt.lu", "isr.S", "wolf.pkg"] {
            std::fs::copy(fixture(name), dir.join(name)).expect("copy fixture");
        }
        dir
    }

    /// `wolf build kmain_interrupt.lu --target x86_64-unknown-none
    /// --emit=obj -o K.o [--release]`; any failure fails the gate.
    /// Returns the wolf objects (K.o, or one per module/cluster, #562)
    /// and the assembly object.
    pub fn build(dir: &Path, tier: &str) -> (Vec<PathBuf>, PathBuf) {
        let mut cmd = Command::new(wolf());
        cmd.current_dir(dir).args([
            "build",
            "kmain_interrupt.lu",
            "--no-cache",
            "--target",
            TARGET,
            "--emit=obj",
            "-o",
            "K.o",
        ]);
        if tier == "release" {
            cmd.arg("--release");
        }
        let out: Output = cmd.output().expect("wolf runs");
        assert!(
            out.status.success(),
            "{tier}: wolf build --target {TARGET} --emit=obj must build the witness (exit {:?}):\n{}",
            out.status.code(),
            text(&out.stderr)
        );
        let asm = dir.join("K.asm-isr.o");
        assert!(asm.is_file(), "{tier}: K.asm-isr.o beside the object");
        let mut objs: Vec<PathBuf> = std::fs::read_dir(dir)
            .expect("read dir")
            .map(|e| e.expect("entry").path())
            .filter(|p| {
                let n = p.file_name().unwrap().to_str().unwrap();
                n.starts_with("K.") && n.ends_with(".o") && !n.starts_with("K.asm-")
            })
            .collect();
        objs.sort();
        assert!(!objs.is_empty(), "{tier}: a wolf object was written");
        (objs, asm)
    }

    /// (name, defined, global, function) for every named symbol.
    fn symbols(obj: &Path) -> Vec<(String, bool, bool, bool)> {
        use object::{Object, ObjectSymbol, SymbolKind};
        let bytes = std::fs::read(obj).expect("read object");
        let file = object::File::parse(&*bytes).expect("parse object");
        file.symbols()
            .filter_map(|s| {
                let n = s.name().ok()?;
                (!n.is_empty()).then(|| {
                    (
                        n.to_string(),
                        !s.is_undefined(),
                        s.is_global(),
                        s.kind() == SymbolKind::Text,
                    )
                })
            })
            .collect()
    }

    /// The bytes of the function symbol `name` in `obj`.
    fn code(obj: &Path, name: &str) -> Vec<u8> {
        use object::{Object, ObjectSection, ObjectSymbol, SymbolSection};
        let bytes = std::fs::read(obj).expect("read object");
        let file = object::File::parse(&*bytes).expect("parse object");
        let sym = file
            .symbols()
            .find(|s| s.name() == Ok(name))
            .unwrap_or_else(|| panic!("{}: no symbol {name}", obj.display()));
        let SymbolSection::Section(i) = sym.section() else {
            panic!("{name} is not defined in a section")
        };
        let sec = file.section_by_index(i).expect("section");
        let data = sec.data().expect("section data");
        let at = (sym.address() - sec.address()) as usize;
        assert!(sym.size() > 0, "{name} is sized (.size)");
        data[at..at + sym.size() as usize].to_vec()
    }

    #[test]
    fn the_trampolines_and_the_handler_are_in_the_objects() {
        let dir = package("objects_native");
        let (wobjs, asm) = build(&dir, "native");
        let mut wsyms = Vec::new();
        for o in &wobjs {
            wsyms.extend(symbols(o));
        }
        for f in ["kw_interrupt", "kmain"] {
            assert!(
                wsyms.iter().any(|(n, d, g, t)| n == f && *d && *g && *t),
                "the wolf object defines `{f}` as a global function, unmangled: {wsyms:?}"
            );
        }
        for imp in ["kw_isr_table", "kw_idt", "kw_idtr", "kw_lidt"] {
            assert!(
                wsyms.iter().any(|(n, d, _, _)| n == imp && !*d),
                "the wolf object imports `{imp}` by name: {wsyms:?}"
            );
        }
        let asyms = symbols(&asm);
        for g in ["kw_isr_common", "kw_isr_table", "kw_idt", "kw_lidt"] {
            assert!(
                asyms.iter().any(|(n, d, gl, _)| n == g && *d && *gl),
                "isr.S's object defines `{g}`, global: {asyms:?}"
            );
        }
        // push imm8: 6a ib. No error code from the CPU: push $0, then
        // the vector; #GP's CPU pushes one: the vector alone.
        assert_eq!(&code(&asm, "kw_isr3")[..4], &[0x6a, 0x00, 0x6a, 0x03]);
        assert_eq!(&code(&asm, "kw_isr6")[..4], &[0x6a, 0x00, 0x6a, 0x06]);
        assert_eq!(&code(&asm, "kw_isr13")[..2], &[0x6a, 0x0d]);
        let common = code(&asm, "kw_isr_common");
        assert_eq!(
            &common[common.len() - 2..],
            &[0x48, 0xcf],
            "kw_isr_common returns with iretq (REX.W CF)"
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod image {
    use super::objects::{build, package};
    use super::*;
    use std::time::{Duration, Instant};

    fn tool(name: &str, args: &[&str]) -> Output {
        Command::new(name)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("`{name}` is part of this gate's host (no skip): {e}"))
    }

    fn have(name: &str) -> bool {
        Command::new(name)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// Assemble the consumer's boot path and link the image with its
    /// script: ld.lld, else the system ld.
    fn link(dir: &Path, wobjs: &[PathBuf], asm: &Path) -> PathBuf {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let boot = dir.join("boot.o");
        let r = tool(
            &cc,
            &[
                "-c",
                fixture("boot.S").to_str().unwrap(),
                "-o",
                boot.to_str().unwrap(),
            ],
        );
        assert!(
            r.status.success(),
            "{cc} -c boot.S: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let elf = dir.join("K.elf");
        let script = fixture("interrupt.ld");
        let mut args: Vec<String> = vec![
            "-static".into(),
            "-nostdlib".into(),
            "-T".into(),
            script.to_str().unwrap().into(),
            "-o".into(),
            elf.to_str().unwrap().into(),
            boot.to_str().unwrap().into(),
            asm.to_str().unwrap().into(),
        ];
        args.extend(wobjs.iter().map(|p| p.to_str().unwrap().to_string()));
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let r = match Command::new("ld.lld").args(&argv).output() {
            Ok(r) => r,
            Err(_) => tool("ld", &argv),
        };
        assert!(
            r.status.success(),
            "link {} with no libc: {}",
            elf.display(),
            String::from_utf8_lossy(&r.stderr)
        );
        elf
    }

    /// Boot `elf` under QEMU (TCG, q35, COM1 to a file, isa-debug-exit
    /// at 0xf4) and return (status, serial). Killed after 120 s.
    fn boot(dir: &Path, elf: &Path) -> (Option<i32>, String) {
        let serial = dir.join("serial.log");
        let mut child = Command::new("qemu-system-x86_64")
            .args([
                "-machine",
                "q35",
                "-cpu",
                "max",
                "-m",
                "64M",
                "-accel",
                "tcg",
                "-display",
                "none",
                "-monitor",
                "none",
                "-no-reboot",
                "-device",
                "isa-debug-exit,iobase=0xf4,iosize=0x04",
                "-serial",
                &format!("file:{}", serial.display()),
                "-kernel",
                elf.to_str().unwrap(),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("qemu-system-x86_64 starts");
        let t0 = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().expect("wait on qemu") {
                break s.code();
            }
            if t0.elapsed() > Duration::from_secs(120) {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let out = child.wait_with_output().expect("qemu output");
        let log = std::fs::read_to_string(&serial)
            .unwrap_or_default()
            .replace('\r', "");
        eprintln!(
            "freestanding_interrupt: qemu status {status:?} after {:.1}s; stderr: {}",
            t0.elapsed().as_secs_f64(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        (status, log)
    }

    #[test]
    fn the_witness_boots_and_every_exception_returns_on_both_tiers() {
        if !have("qemu-system-x86_64") {
            assert!(
                std::env::var("WOLF_INTERRUPT_REQUIRE_QEMU").as_deref() != Ok("1"),
                "WOLF_INTERRUPT_REQUIRE_QEMU=1 and no qemu-system-x86_64 on the PATH"
            );
            eprintln!(
                "SKIP freestanding_interrupt: no qemu-system-x86_64 on the PATH — the image row did not run (rows 1 and 2 did)"
            );
            return;
        }
        for tier in ["native", "release"] {
            let dir = package(&format!("image_{tier}"));
            let (wobjs, asm) = build(&dir, tier);
            let elf = link(&dir, &wobjs, &asm);
            let (status, log) = boot(&dir, &elf);
            let last = log.lines().last().unwrap_or("");
            assert_eq!(
                last, SERIAL,
                "{tier}: the kernel's line on COM1 (whole log: {log:?})"
            );
            assert_eq!(
                status,
                Some(33),
                "{tier}: QEMU exits (16 << 1) | 1 through isa-debug-exit"
            );
            eprintln!("freestanding_interrupt: {tier}: {last} (status 33)");
        }
    }
}
