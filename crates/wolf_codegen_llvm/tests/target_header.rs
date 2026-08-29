//! s127 — the module header comes from the target, witnessed.
//!
//! Three pins around the classic silent-miscompile site (the
//! datalayout string):
//! - the LINUX header is byte-identical to s41's, pinned on every
//!   host (with the goldens, the linux-emission-unchanged witness);
//! - the macOS header carries the s127 constants, pinned on every
//!   host (emission is pure text — cross-emission needs no Mach-O);
//! - on a macOS/aarch64 HOST, the macOS datalayout is re-derived from
//!   the host clang's own emission, so a clang whose datalayout moves
//!   is a loud test failure here, never a drifted constant.

mod common;

use common::module_ir;
use wolf_codegen_llvm::{EmitOptions, ReleaseTarget};
use wolf_wir::ir::{Aux, Function, Module};
use wolf_wir::ops::Opcode;
use wolf_wir::types;

/// A minimal module (one `main` returning 0) for header inspection.
fn tiny() -> Module {
    let mut m = Module::new();
    let sig = m.make_sig(vec![], vec![types::I64]);
    let mut f = Function::new("main", sig);
    let b0 = f.make_block(&[]);
    let (_, z) = f.append_inst(b0, Opcode::Iconst, &[], &[types::I64], Aux::Int(0));
    f.append_inst(b0, Opcode::Ret, &[z[0]], &[], Aux::None);
    m.add_func(f);
    m
}

fn header_of(target: ReleaseTarget) -> Option<(String, String)> {
    let ir = module_ir(
        &tiny(),
        None,
        EmitOptions {
            target: Some(target),
            ..EmitOptions::default()
        },
    )?;
    let grab = |prefix: &str| -> String {
        ir.lines()
            .find_map(|l| l.strip_prefix(prefix))
            .unwrap_or_else(|| panic!("no `{prefix}` line:\n{ir}"))
            .trim_matches('"')
            .to_string()
    };
    Some((grab("target datalayout = \""), grab("target triple = \"")))
}

/// The linux header never moves (s41's verbatim bytes) — the header
/// half of the linux-emission-unchanged witness; the goldens pin the
/// bodies.
#[test]
fn linux_header_is_byte_identical_to_s41() {
    let Some((dl, triple)) = header_of(ReleaseTarget::LinuxX64) else {
        return;
    };
    assert_eq!(
        dl,
        "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
    );
    assert_eq!(triple, "x86_64-unknown-linux-gnu");
}

/// The macOS header carries the s127 constants — pinned on every host
/// (the triple deliberately unversioned; provenance in `lib.rs`).
#[test]
fn macos_header_carries_the_s127_constants() {
    let Some((dl, triple)) = header_of(ReleaseTarget::MacosArm64) else {
        return;
    };
    assert_eq!(
        dl,
        "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32"
    );
    assert_eq!(triple, "arm64-apple-macosx");
}

/// The drift witness: on a macOS/aarch64 host, ask the HOST clang what
/// datalayout it emits for its default (this) target and hold the
/// constant to it. The datalayout is never hand-composed and never
/// allowed to drift silently — a clang that changes the string fails
/// HERE, by name, with both strings in the message.
#[test]
fn macos_datalayout_matches_the_host_clangs_own_emission() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        eprintln!("SKIP: the clang drift witness runs on macOS/aarch64 hosts only");
        return;
    }
    let clang = std::env::var("WOLF_CLANG").unwrap_or_else(|_| "clang".to_string());
    let out = std::process::Command::new(&clang)
        .args(["-x", "c", "/dev/null", "-S", "-emit-llvm", "-o", "-"])
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => {
            eprintln!("SKIP: `{clang}` cannot emit LLVM IR on this host");
            return;
        }
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let dl = text
        .lines()
        .find_map(|l| l.strip_prefix("target datalayout = \""))
        .map(|l| l.trim_end_matches('"'))
        .expect("clang emits a datalayout line");
    assert_eq!(
        dl,
        ReleaseTarget::MacosArm64.datalayout(),
        "the host clang's datalayout moved away from the s127 constant — \
         update ReleaseTarget::MacosArm64 (provenance note in lib.rs) \
         after a clean fact_fuzz run, never silently"
    );
    let triple = text
        .lines()
        .find_map(|l| l.strip_prefix("target triple = \""))
        .map(|l| l.trim_end_matches('"'))
        .expect("clang emits a triple line");
    assert!(
        triple.starts_with("arm64-apple-macosx"),
        "host clang targets {triple}, not arm64-apple-macosx"
    );
}
