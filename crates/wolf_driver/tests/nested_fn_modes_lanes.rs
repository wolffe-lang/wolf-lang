//! s186 (wolf-lang#466) — a nested named fn's parameters carry their
//! declared modes, as a module fn's do. The call site spells `mut` and
//! `take` (X1): spelled, the call runs on the native tiers with the
//! callee's writes reaching the caller; omitted, it is E1007. The
//! memory checker holds the nested body to its modes: a `mut` parameter
//! left moved-out at the NESTED fn's return is E1001 (#464's rule), a
//! write through a `read` parameter is E1014, and a `read` parameter
//! returned is s165's E1002. A moded nested fn used as a VALUE is
//! refused by name (a fn type carries no modes).
//!
//! Before s186 a spelled `mut`/`take` was E1007 on every lane, an
//! omitted `mut` ran (`2` on native and release — the callee grew the
//! caller's list through a `read` argument), and the `read`-parameter
//! write and return ran with the write reaching the caller (`2`,
//! `2 2`) where lupin 0.1.42 prints `1` and `1 2`.
//!
//! The checked executor refuses every nested fn by name (the #12
//! family, `typecheck/nested_fn_value.lu`), so the running row is
//! `unsupported` there; lupin 0.1.42 declines a nested fn with a moded
//! parameter (`unsupported`) and runs the modeless ones.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    codes: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|ds| {
                ds.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// One wolfgang lane. `None` means the host cannot run a native lane
/// (the s59 skip pattern), never a silent pass.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
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
    Some(parse_obs(&out.stdout, "the observation"))
}

/// The sibling lupin, found exactly as `pairing.rs` finds it: `LUPIN`
/// first, then a `wolf-interp` checkout beside an ancestor.
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

/// lupin's version and observation, or `None` when this box has no
/// sibling. `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying
/// "one was arranged here" (r10/#253): there an absent sibling is a
/// failed fetch, and a gate that answers broken plumbing with a green
/// skip is not a gate.
fn lupin_says(entry: &Path) -> Option<(String, Obs)> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of this gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the RULED answer here; only the oracle \
             leg is absent"
        );
        return None;
    };
    let v = Command::new(&lupin)
        .arg("--version")
        .output()
        .expect("lupin --version runs");
    let version = String::from_utf8_lossy(&v.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();
    assert!(!version.is_empty(), "lupin --version printed no version");
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some((version, parse_obs(&out.stdout, "lupin's observation")))
}

/// The corpus row itself is the program: one truth per file.
fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// What one machine must answer: a verdict, and for a run the bytes.
#[derive(Clone, Copy)]
struct Want<'a> {
    verdict: &'a str,
    stdout: Option<&'a str>,
}

const fn runs(stdout: &str) -> Want<'_> {
    Want {
        verdict: "exit(0)",
        stdout: Some(stdout),
    }
}

const fn verdict(v: &str) -> Want<'_> {
    Want {
        verdict: v,
        stdout: None,
    }
}

fn check(machine: &str, entry: &Path, obs: &Obs, want: Want<'_>) {
    assert_eq!(
        obs.verdict,
        want.verdict,
        "the {machine} verdict on {} (diagnostics {:?})",
        entry.display(),
        obs.codes
    );
    if let Some(bytes) = want.stdout {
        assert_eq!(
            obs.stdout,
            bytes,
            "the {machine} answer on {}",
            entry.display()
        );
    }
}

fn wolfgang(entry: &Path, checked: Want<'_>, native: Want<'_>) {
    let obs = lane(entry, "--checked").expect("the checked lane always runs");
    check("checked", entry, &obs, checked);
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(entry, flag) {
            check(flag, entry, &obs, native);
        }
    }
}

fn oracle(entry: &Path, want: Want<'_>) {
    if let Some((version, lupin)) = lupin_says(entry) {
        check(&format!("lupin {version}"), entry, &lupin, want);
    }
}

/// A refused row's only diagnostic is `code` on every wolfgang lane.
fn only(entry: &Path, code: &str) {
    for flag in ["--checked", "--native", "--release"] {
        if let Some(obs) = lane(entry, flag) {
            assert_eq!(
                obs.codes,
                vec![code.to_string()],
                "the {flag} diagnostics on {}",
                entry.display()
            );
        }
    }
}

/// `mut` and `take` spelled: push, `int`, element, store back, re-lend,
/// consume.
#[test]
fn a_nested_fn_mut_parameter_spelled_mut_runs() {
    let entry = corpus("nested_fn_mut_param.lu");
    wolfgang(&entry, verdict("unsupported"), runs("4 42 2 9 7 5\n4\n"));
    oracle(&entry, verdict("unsupported"));
}

/// `f(xs)` against `fn f(mut xs: …)`.
#[test]
fn a_nested_fn_mut_parameter_without_mut_is_e1007() {
    let entry = corpus("nested_fn_mut_omitted.lu");
    let e = verdict("fail(E1007)");
    wolfgang(&entry, e, e);
    only(&entry, "E1007");
    oracle(&entry, verdict("unsupported"));
}

/// #464's rule at the nested fn's own exit.
#[test]
fn a_nested_fn_mut_parameter_left_moved_out_is_e1001() {
    let entry = corpus("nested_fn_mut_moveout.lu");
    let e = verdict("fail(E1001)");
    wolfgang(&entry, e, e);
    only(&entry, "E1001");
    oracle(&entry, verdict("unsupported"));
}

/// A write through a nested fn's `read` parameter.
#[test]
fn a_nested_fn_read_parameter_write_is_e1014() {
    let entry = corpus("nested_fn_read_param_write.lu");
    let e = verdict("fail(E1014)");
    wolfgang(&entry, e, e);
    only(&entry, "E1014");
    oracle(&entry, runs("1\n"));
}

/// A nested fn's `read` parameter returned: lent (#366).
#[test]
fn a_nested_fn_read_parameter_returned_is_e1002() {
    let entry = corpus("nested_fn_read_param_return.lu");
    let e = verdict("fail(E1002)");
    wolfgang(&entry, e, e);
    only(&entry, "E1002");
    oracle(&entry, runs("1 2\n"));
}

/// A moded nested fn read as a value: its fn type would erase the
/// modes, so every lane refuses it by name — never runs it.
#[test]
fn a_moded_nested_fn_used_as_a_value_is_refused_by_name() {
    let dir = std::env::temp_dir().join(format!("wolf-s186-nested-value-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let entry = dir.join("main.lu");
    std::fs::write(
        &entry,
        "fn main() -> !int {\n    fn f(mut xs: List[int]) {\n        (mut xs).push(9)\n    }\n    \
         var xs = [1]\n    var g = f\n    g(xs)\n    print(\"{xs.len}\")\n    0\n}\n",
    )
    .expect("write the program");
    for flag in ["--checked", "--native", "--release"] {
        let out = Command::new(wolf())
            .arg("conform-run")
            .arg(&entry)
            .arg(flag)
            .arg("--json")
            .output()
            .expect("wolf runs");
        assert!(out.status.success(), "conform-run {flag}");
        let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record");
        assert_eq!(rec["verdict"], "unsupported", "{flag}: {rec}");
        let text = rec.to_string();
        assert!(
            text.contains("used as a value"),
            "{flag} names the refusal: {text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
