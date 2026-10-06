//! s214 (wolf-lang#598, P0): a call clobbers the memory its callee may
//! write. A store to a module `var`, to storage behind `extern "c" let`,
//! or through any raw pointer is never forwarded across a call, and a
//! load before a call is never reused after it (`[mem.static.2]`,
//! `[mem.unsafe.raw.1]`).
//!
//! The cause, at wolf 0.2.24 (294d626d), was the WIR builder, which every
//! compiled tier shares: module state, `extern "c" let` storage and raw
//! pointers all ride the function's foreign buffer region, a user call
//! threads none of that region's tokens, and the builder's store→load
//! forwarding and load GVN keyed on the token alone. So the load after
//! the call was the caller's own store (`ins_load`) or the load before
//! the call (hash-consing). Native runs that WIR as built; release
//! inlined the writer on top of the already-forwarded value. memopt's
//! own rule (a call kills every non-exhaustive region's availability)
//! was right and never saw the load.
//!
//! Rows, each seen red at 294d626d on kasumi
//! (`~/lanes/s214/evidence/`, digests in the PR):
//!
//! 1. the issue's witness and a read-call-read (`corpus/memory/
//!    static_var_call_writes.lu`): native and release printed `0 0 0`;
//! 2. a writer two calls deep, a direct writer, a writer through a fn
//!    value (`static_var_call_deep.lu`): `0 0 0 1`;
//! 3. a C allocation written through the pointer by a callee
//!    (`raw_store_call_writes.lu`): `1 1 1`;
//! 4. C writing `extern "c" let` storage, and C calling back into wolf
//!    to write the `var`, between two wolf reads (unix; the C link is
//!    `c_membrane_link.rs`'s). A GUARD, not a witness: right at 0.2.24,
//!    because a call to an `extern "c" fn` already threads both foreign
//!    tokens (`lower_extern_call`), so the token chain carried the
//!    write. It stays so the fix can never be narrowed past it;
//! 5. the freestanding kernel (`--target x86_64-unknown-none`, linked
//!    with no libc and run; x86-64 linux): native exited 12 at 0.2.24
//!    (bits 1 and 2, the wolf writers, wrong), 31 is every shape; and
//!    its object read with `objdump`: after a call to a doubly
//!    recursive wolf writer (never inlined, never folded) the `var` is
//!    loaded again on both tiers, where 0.2.24 returned the caller's
//!    own store.
//!
//! Rows 1–3 run on all four machines; the checked machine and lupin
//! were right before the fix and must stay so.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    codes: Vec<String>,
    stdout: String,
    version: String,
    refusal: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    let mut codes: Vec<String> = rec["diagnostics"]
        .as_array()
        .map(|ds| {
            ds.iter()
                .filter_map(|d| d["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    codes.sort();
    codes.dedup();
    let refusal = rec["x-unsupported-construct"]
        .as_str()
        .or(rec["x-unsupported"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned());
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        codes,
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        refusal,
    }
}

/// Build the runtime staticlib once (every lane needs it; its file
/// name is the host's: `libwolf_rt.a`, or `wolf_rt.lib` on windows).
fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

/// The unix runtime archive, for a link this gate does itself.
fn rt_staticlib() -> PathBuf {
    ensure_rt_staticlib();
    let rt = Path::new(wolf())
        .parent()
        .expect("wolf has a directory")
        .join("libwolf_rt.a");
    assert!(rt.is_file(), "libwolf_rt.a beside wolf: {}", rt.display());
    rt
}

/// One wolfgang lane; `None` is the s59 environment skip, named on
/// stderr (never an ICE, never the checked machine).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    ensure_rt_staticlib();
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, &out.stderr, "the observation"))
}

/// The sibling lupin, found exactly as `pairing.rs` finds it.
fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s214's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — only the oracle leg is absent");
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    assert!(
        !out.stdout.is_empty(),
        "lupin wrote no record for {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, &out.stderr, "lupin's observation"))
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stale_global")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("stale_global")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// The row runs and prints `want` on every machine: the checked
/// machine, native, release and lupin.
fn every_machine_prints(row: &str, want: &str) {
    let entry = corpus(row);
    let mut who = vec![("checked", lane(&entry, "--checked"))];
    who.push(("native", lane(&entry, "--native")));
    who.push(("release", lane(&entry, "--release")));
    who.push(("lupin", lupin_says(&entry)));
    for (name, obs) in who {
        let Some(obs) = obs else { continue };
        assert_eq!(
            (obs.verdict.as_str(), obs.codes.len(), obs.stdout.as_str()),
            ("exit(0)", 0, want),
            "{name} {} on {row}: a read after a call is the callee's write \
             (wolf-lang#598); refusal {:?}",
            obs.version,
            obs.refusal
        );
    }
}

/// Row 1: #598's witness, and a read, a writer, a read.
#[test]
fn a_module_var_written_by_a_callee_is_read_after_the_call() {
    every_machine_prints("memory/static_var_call_writes.lu", "99 99 1\n");
}

/// Row 2: two calls deep, a direct writer, a writer through a fn value.
#[test]
fn a_write_two_calls_deep_or_through_a_fn_value_is_read() {
    every_machine_prints("memory/static_var_call_deep.lu", "42 42 7 42\n");
}

/// Row 3: a callee writes through the caller's raw pointer.
#[test]
fn a_raw_pointer_written_by_a_callee_is_read_after_the_call() {
    every_machine_prints("memory/raw_store_call_writes.lu", "9 9 4\n");
}

/// Row 4: C writes the storage behind `extern "c" let`, and calls back
/// into wolf to write the module `var`, between two wolf reads — on
/// both compiling tiers, linked as a C program would link wolf objects.
/// The checked machine models no link and refuses by name; lupin has no
/// `extern "c" let` yet (wolf-interp#190) — both measured, never a skip.
#[test]
fn c_writes_between_two_wolf_reads_on_both_tiers() {
    let src = fixture("extern/extern_writer.lu");
    let checked = lane(&src, "--checked").expect("the checked machine always runs");
    assert_eq!(
        checked.verdict, "unsupported",
        "the checked machine refuses the link-time symbol by name: {checked:?}"
    );
    assert!(
        checked.refusal.contains("link-time symbol"),
        "the checked machine names the link-time symbol: {:?}",
        checked.refusal
    );
    if let Some(l) = lupin_says(&src) {
        assert_ne!(
            l.verdict, "exit(0)",
            "lupin {} runs no `extern \"c\" let` program (wolf-interp#190): {l:?}",
            l.version
        );
    }
    if cfg!(windows) {
        eprintln!("SKIP: the C link of row 4 is unix-shaped (win64 is s60a's bring-up)");
        return;
    }
    for tier in ["native", "release"] {
        let dir = scratch(&format!("extern-{tier}"));
        let mut cmd = Command::new(wolf());
        cmd.arg("build");
        if tier == "release" {
            cmd.arg("--release");
        }
        cmd.arg(&src)
            .arg("--emit=obj")
            .arg("-o")
            .arg(dir.join("prog.o"));
        let out = cmd.output().expect("wolf runs");
        let stderr = String::from_utf8_lossy(&out.stderr);
        if lane_exit::environment_refusal(&out, &format!("wolf build ({tier})"))
            && tier == "release"
            && stderr.contains("release tier targets")
        {
            eprintln!(
                "SKIP: the release tier refuses this host: {}",
                stderr.trim()
            );
            continue;
        }
        assert!(
            out.status.success(),
            "wolf build --emit=obj ({tier}) must build: {stderr}"
        );
        let mut objs: Vec<PathBuf> = std::fs::read_dir(&dir)
            .expect("read the object dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "o"))
            .collect();
        objs.sort();
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let c_obj = dir.join("c_half.o");
        let r = Command::new(&cc)
            .args(["-c", "-O1"])
            .arg(fixture("extern/extern_writer.c"))
            .arg("-o")
            .arg(&c_obj)
            .output()
            .unwrap_or_else(|e| panic!("no C compiler ({cc}: {e}): row 4 needs one"));
        assert!(
            r.status.success(),
            "{cc}: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let exe = dir.join("prog");
        let mut link = Command::new(&cc);
        link.arg("-o")
            .arg(&exe)
            .args(&objs)
            .arg(&c_obj)
            .arg(rt_staticlib());
        if cfg!(target_os = "macos") {
            link.args(["-lpthread", "-lm"]);
        } else {
            link.args(["-lpthread", "-ldl", "-lm"]);
        }
        let r = link.output().expect("the C driver links");
        assert!(
            r.status.success(),
            "linking row 4 ({tier}): {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let run = Command::new(&exe).output().expect("row 4 runs");
        assert_eq!(
            (
                run.status.code(),
                String::from_utf8_lossy(&run.stdout).as_ref()
            ),
            (Some(0), "99 99 5 42\n"),
            "the {tier} tier reads what C wrote (wolf-lang#598)"
        );
    }
}

/// Row 5, x86-64 linux: the freestanding kernel, linked with no libc and
/// run, and its object read with `objdump`.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux_x86_64 {
    use super::*;
    use std::process::Output;

    const TARGET: &str = "x86_64-unknown-none";

    fn text(b: &[u8]) -> String {
        String::from_utf8_lossy(b).into_owned()
    }

    fn tool(name: &str, args: &[&str]) -> Output {
        Command::new(name)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("`{name}` is part of this gate's host (no skip): {e}"))
    }

    fn sibling_fixture(dir: &str, name: &str) -> PathBuf {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(dir)
            .join(name);
        assert!(p.is_file(), "fixture missing: {}", p.display());
        p
    }

    /// The kernel object for one tier, in its own directory.
    fn kernel_obj(case: &str, tier: &str) -> (PathBuf, PathBuf) {
        let dir = scratch(&format!("{case}-{tier}"));
        let src = dir.join("kmain_stale.lu");
        std::fs::copy(fixture("kernel/kmain_stale.lu"), &src).expect("copy the kernel");
        let obj = dir.join("kmain_stale.o");
        let mut cmd = Command::new(wolf());
        cmd.current_dir(&dir).arg("build");
        if tier == "release" {
            cmd.arg("--release");
        }
        cmd.args([
            "kmain_stale.lu",
            "--no-cache",
            "--target",
            TARGET,
            "--emit=obj",
            "-o",
            obj.to_str().unwrap(),
        ]);
        let out = cmd.output().expect("wolf runs");
        assert!(
            out.status.success() && obj.is_file(),
            "the {tier} kernel object must build (exit {:?}): {}",
            out.status.code(),
            text(&out.stderr)
        );
        (dir, obj)
    }

    #[test]
    fn the_freestanding_kernel_reads_every_callees_write_on_both_tiers() {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        for tier in ["native", "release"] {
            let (dir, obj) = kernel_obj("kernel-run", tier);
            let mut objs = vec![obj];
            for src in [
                sibling_fixture("freestanding", "start.S"),
                sibling_fixture("freestanding", "rt_stub.S"),
                fixture("kernel/stale.S"),
            ] {
                let o = dir.join(src.file_name().unwrap()).with_extension("o");
                let r = tool(
                    &cc,
                    &["-c", src.to_str().unwrap(), "-o", o.to_str().unwrap()],
                );
                assert!(
                    r.status.success(),
                    "{cc} -c {}: {}",
                    src.display(),
                    text(&r.stderr)
                );
                objs.push(o);
            }
            let exe = dir.join("kmain_stale.elf");
            let script = sibling_fixture("freestanding_static", "static.ld");
            let mut args: Vec<String> = vec![
                "-static".into(),
                "-nostdlib".into(),
                "-T".into(),
                script.to_str().unwrap().into(),
                "-o".into(),
                exe.to_str().unwrap().into(),
            ];
            args.extend(objs.iter().map(|p| p.to_str().unwrap().to_string()));
            let argv: Vec<&str> = args.iter().map(String::as_str).collect();
            let r = match Command::new("ld.lld").args(&argv).output() {
                Ok(r) => r,
                Err(_) => tool("ld", &argv),
            };
            assert!(
                r.status.success(),
                "link the {tier} kernel: {}",
                text(&r.stderr)
            );
            let out = tool(exe.to_str().unwrap(), &[]);
            assert_eq!(
                out.status.code(),
                Some(31),
                "{tier}: one bit per shape read right — 1 a wolf writer, 2 two calls \
                 deep, 4 assembly writing s214_cell, 8 assembly calling back into wolf, \
                 16 a doubly recursive wolf writer (wolf-lang#598); stdout {:?}",
                text(&out.stdout)
            );
        }
    }

    /// The instructions of `func` after its call to `callee`, up to the
    /// return, from `objdump -dr`. The callee is named by a relocation:
    /// on the call itself (`R_X86_64_PLT32`, release) or on the GOT load
    /// the call goes through (`R_X86_64_GOTPCREL`, native) — the call is
    /// the first `call` at or after that relocation.
    fn after_call(disasm: &str, func: &str, callee: &str) -> Vec<String> {
        let head = format!("<{func}>:");
        let body: Vec<&str> = disasm
            .lines()
            .skip_while(|l| !l.ends_with(&head))
            .skip(1)
            .take_while(|l| !l.trim().is_empty())
            .collect();
        assert!(!body.is_empty(), "{func} in the object:\n{disasm}");
        let named = body
            .iter()
            .position(|l| l.contains("R_X86_64") && l.contains(&format!("\t{callee}")))
            .unwrap_or_else(|| panic!("{func} names {callee}:\n{}", body.join("\n")));
        let at = (named.saturating_sub(1)..body.len())
            .find(|&i| insn(body[i]).is_some_and(|(m, _)| m.starts_with("call")))
            .unwrap_or_else(|| panic!("{func} calls {callee}:\n{}", body.join("\n")));
        body[at + 1..]
            .iter()
            .filter(|l| !l.contains("R_X86_64"))
            .take_while(|l| insn(l).is_none_or(|(m, _)| !m.starts_with("ret")))
            .map(|l| l.to_string())
            .collect()
    }

    /// `(mnemonic, operands)` of one `objdump` instruction line.
    fn insn(line: &str) -> Option<(&str, &str)> {
        let (_, rest) = line.split_once(':')?;
        let rest = rest.trim();
        Some(match rest.split_once(char::is_whitespace) {
            Some((m, ops)) => (m, ops.trim()),
            None => (rest, ""),
        })
    }

    /// A read of memory that is not the frame (`rsp`/`rbp`) or a GOT
    /// slot (`rip`): the storage itself, loaded again.
    fn reads_storage(line: &str) -> bool {
        let Some((_, ops)) = insn(line) else {
            return false;
        };
        let Some((src, _)) = ops.rsplit_once(',') else {
            return false;
        };
        src.contains("(%") && !["(%rsp", "(%rbp", "(%rip"].iter().any(|r| src.contains(r))
    }

    #[test]
    fn the_object_loads_the_storage_again_after_the_call_on_both_tiers() {
        for tier in ["native", "release"] {
            let (_, obj) = kernel_obj("kernel-objdump", tier);
            let d = tool(
                "objdump",
                &["-dr", "--no-show-raw-insn", obj.to_str().unwrap()],
            );
            assert!(d.status.success(), "objdump: {}", text(&d.stderr));
            let disasm = text(&d.stdout);
            for (func, callee) in [
                ("s214_var_after_wolf_call", "_Wwalk"),
                ("s214_cell_after_call", "s214_poke"),
                ("s214_var_after_callback", "s214_call_back"),
            ] {
                let tail = after_call(&disasm, func, callee);
                assert!(
                    tail.iter().any(|l| reads_storage(l)),
                    "{tier}: {func} loads the storage again after calling {callee} \
                     (wolf-lang#598) — the instructions after the call:\n{}",
                    tail.join("\n")
                );
            }
        }
    }
}
