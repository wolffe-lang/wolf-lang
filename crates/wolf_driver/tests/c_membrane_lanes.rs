//! kw02 (wolf-lang#513, #521, #514; STATUS #31 K9(b) = B, #32 R1): the
//! membrane rows on every machine.
//!
//! Measured at trunk 10a16d87 (kasumi, `~/lanes/kw02/evidence/
//! rows-trunk.log`): `raw_ptr_private_sig` and `raw_ptr_mut_param` were
//! E1302 on all four machines; `extern_c_outside_unsafe` was
//! `unsupported` on all four (the compiled tiers refused the call at
//! codegen, so nothing asked for an `unsafe` block); `extern_libc` was
//! E1302 on all four. At head the compiler answers what the clauses say
//! (`[mem.unsafe.sig]`, `[abi.c.import]`, `[abi.c.export]`), and the two
//! machines with no C membrane — the checked machine and lupin — refuse a
//! call into hand-declared C by name rather than model it.
//!
//! lupin 0.1.44 (the 0.2.21 pairing) and 0.1.45 (the 0.2.22 pairing,
//! r27) applied E1302 to every signature and had no C ABI, each parting
//! pinned by version. 0.1.46 (the 0.2.23 pairing, r28) carries is70's
//! mirror (wolf-interp#181) and answers what the compiler answers on
//! every row: the four pins are dropped, and lupin is held to its
//! column like every other machine.

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
    unsupported: String,
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
    let unsupported = rec["x-unsupported-construct"]
        .as_str()
        .or(rec["x-unsupported"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned());
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        codes,
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        unsupported,
    }
}

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

/// One wolfgang lane; `None` is the s59 environment skip (never an ICE).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    ensure_rt_staticlib();
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
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
             (LUPIN={}) — the oracle leg of kw02's gate did not run",
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
    Some(parse_obs(&out.stdout, &out.stderr, "lupin's observation"))
}

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// What a machine must answer. `named` is a substring of the refusal's
/// construct when the verdict is `unsupported` (refused BY NAME).
#[derive(Clone, Copy)]
struct Want<'a> {
    verdict: &'a str,
    codes: &'a [&'a str],
    stdout: &'a str,
    named: &'a str,
}

const fn runs(stdout: &str) -> Want<'_> {
    Want {
        verdict: "exit(0)",
        codes: &[],
        stdout,
        named: "",
    }
}

/// The checked machine's refusal of a call into hand-declared C.
const NO_C_MEMBRANE: Want<'static> = Want {
    verdict: "unsupported",
    codes: &[],
    stdout: "",
    named: "hand-declared `extern \"c\" fn`",
};

fn assert_obs(who: &str, row: &str, obs: &Obs, want: Want<'_>) {
    assert_eq!(
        obs.verdict, want.verdict,
        "{who} on {row}; codes {:?}, stdout {:?}, refusal {:?}",
        obs.codes, obs.stdout, obs.unsupported
    );
    let codes: Vec<String> = want.codes.iter().map(|c| c.to_string()).collect();
    assert_eq!(obs.codes, codes, "{who}'s diagnostics on {row}");
    assert_eq!(obs.stdout, want.stdout, "{who}'s stdout on {row}");
    if want.verdict == "unsupported" {
        assert!(
            obs.unsupported.contains(want.named),
            "{who} on {row} refuses by name ({:?}), never a guess: {:?}",
            want.named,
            obs.unsupported
        );
    }
}

fn every_machine(row: &str, checked: Want<'_>, compiled: Want<'_>, lupin_want: Want<'_>) {
    let entry = corpus(row);
    for (flag, want) in [
        ("--checked", checked),
        ("--native", compiled),
        ("--release", compiled),
    ] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        let who = if flag == "--checked" {
            "the CHECKED lane"
        } else {
            flag
        };
        assert_obs(who, row, &obs, want);
    }
    let Some(lupin) = lupin_says(&entry) else {
        return;
    };
    assert_obs(
        &format!("lupin {} (the mirror is wolf-interp#181)", lupin.version),
        row,
        &lupin,
        lupin_want,
    );
}

/// `[mem.unsafe.sig]`: a module-private fn takes `*T` and writes through it.
#[test]
fn a_private_signature_carries_a_raw_pointer() {
    every_machine(
        "memory/raw_ptr_private_sig.lu",
        runs("42\n"),
        runs("42\n"),
        runs("42\n"),
    );
}

/// `[mem.unsafe.sig]` with `mut`: the pointer variable is the caller's.
#[test]
fn a_mut_raw_pointer_parameter_is_the_callers_variable() {
    every_machine(
        "memory/raw_ptr_mut_param.lu",
        runs("42\n"),
        runs("42\n"),
        runs("42\n"),
    );
}

/// The control: a `pub` signature still refuses `*T` everywhere, lupin
/// included.
#[test]
fn a_pub_signature_still_refuses_a_raw_pointer() {
    let e1302 = Want {
        verdict: "fail(E1302)",
        codes: &["E1302"],
        stdout: "",
        named: "",
    };
    every_machine("memory/unsafe_sig.lu", e1302, e1302, e1302);
}

/// `[abi.c.import]`: a hand-declared C call outside `unsafe` is E1301.
/// lupin 0.1.44 and 0.1.45 refused the bodyless declaration by name
/// before they looked (pinned until the 0.1.46 pairing, r28).
#[test]
fn a_call_into_hand_declared_c_needs_unsafe() {
    let e1301 = Want {
        verdict: "fail(E1301)",
        codes: &["E1301"],
        stdout: "",
        named: "",
    };
    every_machine("memory/extern_c_outside_unsafe.lu", e1301, e1301, e1301);
}

/// `[abi.c.import]`: libc's own `strlen` and `llabs` through
/// hand-declared externs, a `*u8` crossing.
#[test]
fn wolf_reaches_the_c_library_through_hand_declared_externs() {
    let lupin_refuses = Want {
        verdict: "unsupported",
        codes: &[],
        stdout: "",
        named: "no body",
    };
    every_machine(
        "membrane/extern_libc.lu",
        NO_C_MEMBRANE,
        runs("8 34\n"),
        lupin_refuses,
    );
}

/// `[abi.c.export]`: kw00's probe — an export called from wolf runs on
/// every machine; the symbol is the gate `c_membrane_link.rs`'s.
#[test]
fn an_export_called_from_wolf_runs_everywhere() {
    every_machine("membrane/export_called.lu", runs(""), runs(""), runs(""));
}

/// `[abi.c.export]`: an export in a child module.
#[test]
fn an_export_in_a_child_module_runs_everywhere() {
    every_machine("membrane/export_child.lu", runs(""), runs(""), runs(""));
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("c_membrane_lanes")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// What the compiled tiers cannot lower at the membrane they refuse BY
/// NAME — never a guessed layout, a silently missing symbol, or two
/// bodies under one symbol.
fn refused_by_both_tiers(name: &str, src: &str, construct: &str) {
    let dir = scratch(name);
    let prog = dir.join("prog.lu");
    std::fs::write(&prog, src).expect("write the program");
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(&prog, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict, "unsupported",
            "{flag} on {name}: refused, not built; codes {:?}",
            obs.codes
        );
        assert!(
            obs.unsupported.contains(construct),
            "{flag} on {name} names the construct {construct:?}: {:?}",
            obs.unsupported
        );
    }
}

#[test]
fn a_generic_export_is_refused_by_name() {
    refused_by_both_tiers(
        "generic",
        "export fn kx_zero[T](x: T) -> i64 {\n    0\n}\n\nfn main() -> int {\n    kx_zero(1) as int\n}\n",
        "a generic `export fn`",
    );
}

#[test]
fn a_str_at_the_membrane_is_refused_by_name() {
    refused_by_both_tiers(
        "str_param",
        "export fn kx_len(s: str) -> i64 {\n    0\n}\n\n\
         fn main() -> int {\n    kx_len(\"ab\") as int\n}\n",
        "a parameter type that does not cross the C membrane",
    );
}

#[test]
fn an_error_union_result_at_the_membrane_is_refused_by_name() {
    refused_by_both_tiers(
        "eu_result",
        "export fn kx_try(x: i64) -> !i64 {\n    x\n}\n\n\
         fn main() -> !int {\n    let v = kx_try(0)?\n    v as int\n}\n",
        "a result type that does not cross the C membrane",
    );
}

#[test]
fn a_non_repr_c_struct_at_the_membrane_is_refused_by_name() {
    refused_by_both_tiers(
        "plain_struct",
        "struct P {\n    x: i64,\n}\n\nexport fn kx_px(p: P) -> i64 {\n    p.x\n}\n\n\
         fn main() -> int {\n    kx_px(P { x: 0 }) as int\n}\n",
        "a parameter type that does not cross the C membrane",
    );
}

#[test]
fn a_mut_parameter_at_the_membrane_is_refused_by_name() {
    refused_by_both_tiers(
        "mut_param",
        "export fn kx_bump(mut x: i64) {\n    x += 1\n}\n\n\
         fn main() -> int {\n    var v = 0\n    kx_bump(mut v)\n    v as int - 1\n}\n",
        "a `mut`/`take` parameter at the C membrane",
    );
}

#[test]
fn an_export_as_a_value_is_refused_by_name() {
    refused_by_both_tiers(
        "fn_value",
        "export fn kx_one() -> i64 {\n    1\n}\n\nfn call(f: fn() -> i64) -> i64 {\n    f()\n}\n\n\
         fn main() -> int {\n    call(kx_one) as int - 1\n}\n",
        "function as a value",
    );
}

#[test]
fn an_export_named_main_is_refused_by_name() {
    refused_by_both_tiers(
        "export_main",
        "export fn main() -> int {\n    0\n}\n",
        "two functions under one C symbol",
    );
}
