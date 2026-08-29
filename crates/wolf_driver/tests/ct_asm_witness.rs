//! s112 — the release-tier honesty gate ([ct.taint.gap]): the WIR
//! verifier is the SEMANTIC authority, but LLVM's instruction
//! selection could re-introduce a conditional branch from a
//! branch-free form behind our back. These two witnesses close the
//! gap for the flagship shapes, the vectorization-witness way: build
//! the corpus kernel with the release tier, disassemble the real
//! binary, and assert ZERO conditional-branch opcodes inside the
//! witness fn's body. If a toolchain bump breaks one, the failure
//! names the offending instructions — the finding then names the
//! transform and the mitigation lands measured, per the sprint
//! contract.
//!
//! The kernels are the corpus files (one source of truth):
//! `corpus/kernels/ct_tag_compare.lu` — the accumulate-then-single-
//! check tag compare (the std AEAD `open`'s load-bearing claim) —
//! and `corpus/kernels/ct_cswap.lu` — the arithmetic
//! conditional-select (the X25519 ladder's coming shape). Both fns
//! are `#[consttime]`, so the release tier emits them `noinline`
//! ([ct.attr.barrier]) and their symbols survive to the disassembly.
//!
//! The witness is per-target where the machine is (s127): the symbol
//! carries Mach-O's `_` prefix on macOS, the disassembly format is
//! GNU objdump's on linux and llvm-objdump's on macOS, and the
//! conditional-transfer opcode set is the Jcc family on x86-64 and
//! the `b.cond`/`cbz`/`tbz` family on aarch64.
//!
//! A host that cannot drive the release tier (its named refusal),
//! cannot link release binaries, or has no objdump SKIPs loudly at
//! runtime (the s59 pattern — environment or named refusal, never a
//! verdict).

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn corpus_kernel(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/kernels")
        .join(name);
    p.canonicalize().unwrap_or(p)
}

/// `wolf build --release` on one corpus kernel; the exe on success,
/// `None` (with a loud SKIP) when the environment cannot drive the
/// release tier.
fn build_release(case: &str, kernel: &str) -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    // Copy the kernel into its own package root: the corpus directory
    // holds sibling entry files a package build must not sweep in.
    let entry = dir.join("prog.lu");
    std::fs::copy(corpus_kernel(kernel), &entry).expect("copy kernel");
    let exe = dir.join("prog");
    let out = Command::new(wolf())
        .arg("build")
        .arg(&entry)
        .arg("--release")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("wolf runs");
    if String::from_utf8_lossy(&out.stderr).contains("release tier targets linux/x86-64") {
        // The tier's named host refusal (linux/x86-64 + macOS/aarch64
        // since s127; the message names both) — a loud skip (s59).
        eprintln!("SKIP: the release tier refuses this host");
        return None;
    }
    match out.status.code() {
        Some(0) => Some(exe),
        Some(2) => {
            eprintln!(
                "SKIP: environment cannot drive the release tier: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            None
        }
        other => panic!(
            "wolf build --release failed (exit {other:?}): {}",
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

/// The disassembled body of the one symbol starting `_W<stem>$`
/// (consttime fns keep their symbol — noinline — and release-tier
/// symbols carry a content-hash suffix). Instruction mnemonics only.
fn witness_body(exe: &Path, stem: &str) -> Option<Vec<String>> {
    let out = match Command::new("objdump").arg("-d").arg(exe).output() {
        Ok(o) if o.status.success() => o,
        Ok(o) => panic!("objdump failed: {}", String::from_utf8_lossy(&o.stderr)),
        Err(e) => {
            eprintln!("SKIP: no objdump on this host: {e}");
            return None;
        }
    };
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    // Mach-O symbols carry the `_` prefix at object grain (s59), so
    // the disassembly label does too.
    let prefix = if cfg!(target_os = "macos") {
        "<__W"
    } else {
        "<_W"
    };
    let label = format!("{prefix}{stem}$");
    let mut body = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.contains(&label) && line.ends_with(">:") {
            assert!(!inside, "two witness symbols match `{label}`");
            inside = true;
            continue;
        }
        if inside {
            // A body ends at the blank line objdump prints before the
            // next symbol.
            if line.trim().is_empty() {
                break;
            }
            // GNU objdump: "  1690:\t48 89 c8             \tmov    %rcx,%rax"
            // llvm-objdump: "1000002d8: d1000408    \tsub\tx8, x0, #0x1"
            // Either way the mnemonic is the first token of the first
            // tab field past the address that starts with a letter
            // (the encoding-bytes field never does).
            if let Some(mnemonic) = line
                .split('\t')
                .skip(1)
                .filter_map(|f| f.split_whitespace().next())
                .find(|t| t.starts_with(|c: char| c.is_ascii_alphabetic()))
            {
                body.push(mnemonic.to_string());
            }
        }
    }
    assert!(
        !body.is_empty(),
        "witness symbol `{label}` not found in the disassembly — \
         did the release tier stop emitting consttime fns noinline?"
    );
    Some(body)
}

/// Every conditional transfer on the HOST'S architecture. x86-64: the
/// Jcc family (everything spelled `j*` except the unconditional
/// `jmp*`) plus the LOOP family and the legacy `jcxz` shapes (matched
/// by the `j*` rule). aarch64: the `b.cond` family plus the
/// compare-and-branch/test-and-branch forms (`cbz`/`cbnz`/
/// `tbz`/`tbnz`); `csel`/`ccmp` are conditional but branch-FREE, which
/// is the point.
fn is_conditional_branch(mnemonic: &str) -> bool {
    if cfg!(target_arch = "aarch64") {
        mnemonic.starts_with("b.") || matches!(mnemonic, "cbz" | "cbnz" | "tbz" | "tbnz")
    } else {
        (mnemonic.starts_with('j') && mnemonic != "jmp" && mnemonic != "jmpq")
            || mnemonic.starts_with("loop")
    }
}

fn assert_branch_free(case: &str, body: &[String]) {
    let bad: Vec<&String> = body.iter().filter(|m| is_conditional_branch(m)).collect();
    assert!(
        bad.is_empty(),
        "{case}: conditional branch opcode(s) {bad:?} inside the witness body — \
         LLVM re-introduced a branch into a verified-branch-free kernel \
         ([ct.taint.gap]); full body: {body:?}"
    );
}

#[test]
fn tag_compare_release_body_is_branch_free() {
    let Some(exe) = build_release("ct_tag_asm", "ct_tag_compare.lu") else {
        return;
    };
    // The kernel still runs (and its own single check agrees).
    let run = Command::new(&exe).output().expect("run");
    assert_eq!(run.status.code(), Some(0), "the tag kernel must exit 0");
    let Some(body) = witness_body(&exe, "tag_diff") else {
        return;
    };
    // Anti-vacuity: the fold's work is really in this body (x86
    // spells it xor/or, aarch64 eor/orr).
    let (xor_m, or_m) = if cfg!(target_arch = "aarch64") {
        ("eor", "orr")
    } else {
        ("xor", "or")
    };
    assert!(
        body.iter().any(|m| m == xor_m) && body.iter().any(|m| m == or_m),
        "the XOR/OR fold must be present: {body:?}"
    );
    assert_branch_free("ct_tag_compare", &body);
}

#[test]
fn cswap_release_body_is_branch_free() {
    let Some(exe) = build_release("ct_cswap_asm", "ct_cswap.lu") else {
        return;
    };
    let run = Command::new(&exe).output().expect("run");
    assert_eq!(run.status.code(), Some(0), "the cswap kernel must exit 0");
    let Some(body) = witness_body(&exe, "ct_select") else {
        return;
    };
    // Anti-vacuity: the arithmetic select's mask really is arithmetic
    // (x86: the mask shape or cmov; aarch64: the and/eor mask or a
    // csel — all branch-free).
    assert!(
        body.iter().any(|m| m == "and"
            || m == "xor"
            || m == "eor"
            || m.starts_with("cmov")
            || m.starts_with("csel")),
        "the arithmetic select must be present: {body:?}"
    );
    assert_branch_free("ct_cswap", &body);
}
