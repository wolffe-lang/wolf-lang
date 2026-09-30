//! s188 (wolf-lang#484) — a module fn with a `mut` or `take` parameter
//! is called by name; read as a VALUE it is refused by name on every
//! lane.
//!
//! A fn type carries no modes (`[gram.type]`: `fn(T, …) -> R`), and a
//! named function read as a value is "a value of its fn type … the same
//! `fn(int) -> int` to every callee" (`[type.fn.value]`). So a call
//! through the value is a call site that cannot spell `mut`, which
//! `[mem.tier0.mode.mut]` requires (X1), and that passes the argument
//! as a `read` (`[mem.tier0.mode.read]`: immutable for the whole call)
//! into a body that writes it. Neither "the value carries its mode" nor
//! "the call runs modeless" is a reading the spec allows; the refusal
//! is by name (`[proto.record.unsupported]`), as s186 refuses a moded
//! NESTED fn used as a value (#466).
//!
//! Before s188 the lanes disagreed with no diagnostic, on trunk
//! `e3ae62b5` and the published 0.2.19 alike: the issue's program
//! printed `2` on checked (the callee grew the caller's list through a
//! `read` argument) and `1` on native and release; a `take` parameter
//! through a value never moved (`2 2`); a `mut int` parameter's fn
//! returned as a value lost its write on checked (`1`) and was an ICE
//! on native and release. lupin 0.1.42 refuses every one of these
//! (`unsupported`, at resolve: "a call through a function value is
//! refused, not guessed"), so no lupin row is pinned pre-mirror.
//!
//! The other polarity: a modeless fn read as a value, and the moded
//! fns called by name, run with the same bytes on every lane and lupin.

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
    record: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        record: rec.to_string(),
    }
}

/// One program in a directory of its own (the driver treats a directory
/// as a package); `extra` are further files, by relative path.
fn program(case: &str, main: &str, extra: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("s188-fn-value")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for (rel, src) in extra {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir module");
        std::fs::write(&p, src).expect("write module");
    }
    let entry = dir.join("main.lu");
    std::fs::write(&entry, main).expect("write program");
    entry
}

/// One wolfgang lane. `None` means the host cannot run that lane (the
/// s59 skip pattern), never a silent pass.
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
/// sibling (a failure where `WOLF_PAIRING_REQUIRE_SIBLING` says one was
/// arranged, r10/#253).
fn lupin_says(entry: &Path) -> Option<(String, Obs)> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of this gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN); the oracle leg is absent");
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

/// Refused by name on checked, native and release; lupin refuses too.
fn refused(case: &str, main: &str, extra: &[(&str, &str)]) {
    let entry = program(case, main, extra);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict, "unsupported",
            "{case} {flag}: a moded fn read as a value must be refused, never run: {}",
            obs.record
        );
        assert!(
            obs.record.contains("used as a value"),
            "{case} {flag} names the refusal: {}",
            obs.record
        );
    }
    if let Some((version, obs)) = lupin_says(&entry) {
        assert_eq!(
            obs.verdict, "unsupported",
            "{case}: lupin {version} refuses a call through a moded fn value: {}",
            obs.record
        );
    }
}

/// Runs with `bytes` on every lane and on lupin.
fn runs(case: &str, main: &str, bytes: &str) {
    let entry = program(case, main, &[]);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "{case} {flag}: {}", obs.record);
        assert_eq!(obs.stdout, bytes, "{case} {flag}: the answer");
    }
    if let Some((version, obs)) = lupin_says(&entry) {
        assert_eq!(
            obs.verdict, "exit(0)",
            "{case}: lupin {version}: {}",
            obs.record
        );
        assert_eq!(obs.stdout, bytes, "{case}: lupin {version}'s answer");
    }
}

/// The issue's program: bound to a `var` and called through it.
#[test]
fn a_mut_parameter_fn_bound_as_a_value_is_refused() {
    refused(
        "mut_value",
        "fn f(mut xs: List[int]) {\n    (mut xs).push(9)\n}\n\n\
         fn main() -> !int {\n    var xs = [1]\n    var g = f\n    g(xs)\n    \
         print(\"{xs.len}\")\n    0\n}\n",
        &[],
    );
}

/// A `take` parameter: through a value the move never happened (`2 2`).
#[test]
fn a_take_parameter_fn_bound_as_a_value_is_refused() {
    refused(
        "take_value",
        "fn f(take xs: List[int]) -> int {\n    xs.len\n}\n\n\
         fn main() -> !int {\n    var xs = [1, 2]\n    let g = f\n    let n = g(xs)\n    \
         print(\"{n} {xs.len}\")\n    0\n}\n",
        &[],
    );
}

/// Passed as an argument to a fn-typed parameter.
#[test]
fn a_mut_parameter_fn_passed_as_an_argument_is_refused() {
    refused(
        "mut_value_arg",
        "fn f(mut xs: List[int]) {\n    (mut xs).push(9)\n}\n\n\
         fn apply(h: fn(List[int]), xs: List[int]) {\n    h(xs)\n}\n\n\
         fn main() -> !int {\n    var xs = [1]\n    apply(f, xs)\n    \
         print(\"{xs.len}\")\n    0\n}\n",
        &[],
    );
}

/// Returned as a value: checked lost the write (`1`), native and
/// release were an ICE.
#[test]
fn a_mut_parameter_fn_returned_as_a_value_is_refused() {
    refused(
        "mut_value_return",
        "fn f(mut n: int) {\n    n = n + 1\n}\n\n\
         fn pick() -> fn(int) {\n    f\n}\n\n\
         fn main() -> !int {\n    var n = 1\n    let g = pick()\n    g(n)\n    \
         print(\"{n}\")\n    0\n}\n",
        &[],
    );
}

/// Namespace-qualified from another module (`util.grow`).
#[test]
fn a_qualified_mut_parameter_fn_read_as_a_value_is_refused() {
    refused(
        "mut_value_qualified",
        "use util\n\nfn main() -> !int {\n    var xs = [1]\n    var g = util.grow\n    \
         g(xs)\n    print(\"{xs.len}\")\n    0\n}\n",
        &[(
            "util/u.lu",
            "pub fn grow(mut xs: List[int]) {\n    (mut xs).push(9)\n}\n",
        )],
    );
}

/// The other polarity: a modeless fn is still a value, bound and passed.
#[test]
fn a_modeless_fn_read_as_a_value_still_runs() {
    runs(
        "read_value",
        "fn total(xs: List[int]) -> int {\n    var t = 0\n    for x in xs {\n        \
         t = t + x\n    }\n    t\n}\n\n\
         fn apply(h: fn(List[int]) -> int, xs: List[int]) -> int {\n    h(xs)\n}\n\n\
         fn main() -> !int {\n    let xs = [1, 2, 3]\n    let g = total\n    \
         print(\"{g(xs)} {apply(total, xs)}\")\n    0\n}\n",
        "6 6\n",
    );
}

/// And the moded fns themselves, called by name, run with their writes.
#[test]
fn a_moded_fn_called_by_name_still_runs() {
    runs(
        "mut_by_name",
        "fn f(mut xs: List[int]) {\n    (mut xs).push(9)\n}\n\n\
         fn bump(mut n: int) {\n    n = n + 1\n}\n\n\
         fn main() -> !int {\n    var xs = [1]\n    f(mut xs)\n    var n = 1\n    \
         bump(mut n)\n    print(\"{xs.len} {n}\")\n    0\n}\n",
        "2 2\n",
    );
}
