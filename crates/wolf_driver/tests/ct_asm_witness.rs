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
//! Off-target the whole file compiles away; a host that cannot link
//! release binaries or has no objdump SKIPs loudly (environment,
//! never a verdict) — the same posture as no_spawn_binary.rs.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

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
    let label = format!("<_W{stem}$");
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
            // "  1690:\t48 89 c8             \tmov    %rcx,%rax"
            if let Some(insn) = line.splitn(3, '\t').nth(2)
                && let Some(mnemonic) = insn.split_whitespace().next()
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

/// Every x86-64 conditional transfer: the Jcc family (everything
/// spelled `j*` except the unconditional `jmp*`) plus the LOOP family
/// and the legacy `jcxz` shapes (matched by the `j*` rule).
fn is_conditional_branch(mnemonic: &str) -> bool {
    (mnemonic.starts_with('j') && mnemonic != "jmp" && mnemonic != "jmpq")
        || mnemonic.starts_with("loop")
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
    // Anti-vacuity: the fold's work is really in this body.
    assert!(
        body.iter().any(|m| m == "xor") && body.iter().any(|m| m == "or"),
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
    // (cmov is acceptable; the mask shape is what today's LLVM picks).
    assert!(
        body.iter()
            .any(|m| m == "and" || m == "xor" || m.starts_with("cmov")),
        "the arithmetic select must be present: {body:?}"
    );
    assert_branch_free("ct_cswap", &body);
}
