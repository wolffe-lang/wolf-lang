//! s180 (wolf-lang#438, ruled "align" 2026-09-24) — the index store
//! follows `push` (`[mem.region.edge.elem]`): a plain `xs[i] = v` /
//! `m[k] = v` COPIES a non-`Copy` value, `xs[i] = take v` MOVES it
//! (`[gram.expr.assign]` admits `take` there and nowhere else in an
//! assignment), and a `read` `v` stored plainly is not a lend — while
//! `m[k] = take v` on one is E1014.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every `phase: run` entry on the NATIVE
//! lane only, and `lane-coverage` counts what the checked lane
//! EXECUTES, not what it answers. The defect a copy exists to prevent
//! is a silent cross-lane wrong answer — a store that shares instead
//! of copying prints `outs0=2 xs=2` with no diagnostic (what `push` did
//! on both wolfgang lanes at v0.2.14, `[mem.region.edge.elem]`). Every
//! case asserts the lanes agree AND agree on the ruled answer.
//!
//! lupin is the third machine and the ruling's mirror is is55's, not
//! this lane's. lupin 0.1.38 (the PAIRING stamp when this landed)
//! predates it, so for that exact version each case asserts the answer
//! 0.1.38 was MEASURED to give (s182's `expected.toml` `[trunk]` tables,
//! re-measured by s180 on kasumi); any other version must give the
//! RULED answer. A pin bump that carries lupin forward without the
//! mirror goes red here by name, which is the point.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin release that predates the #438 mirror (is55).
const PRE_MIRROR_LUPIN: &str = "0.1.38";

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
    if out.status.code() == Some(2) && flag != "--checked" {
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
             (LUPIN={}) — the oracle leg of #438's gate did not run",
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

/// Every wolfgang lane answers `wolfgang`; lupin answers `pre` at
/// 0.1.38 and `ruled` at any other version (`None`: the ruling leaves
/// lupin's answer to is55 beyond "it parses").
fn every_lane(name: &str, wolfgang: Want<'_>, pre: Want<'_>, ruled: Option<Want<'_>>) {
    let entry = corpus(name);
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    check("checked", &entry, &checked, wolfgang);
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            check(flag, &entry, &obs, wolfgang);
        }
    }
    let Some((version, lupin)) = lupin_says(&entry) else {
        return;
    };
    if version == PRE_MIRROR_LUPIN {
        check(
            &format!("lupin {version} (pre-mirror)"),
            &entry,
            &lupin,
            pre,
        );
    } else if let Some(ruled) = ruled {
        check(&format!("lupin {version}"), &entry, &lupin, ruled);
    } else {
        assert_ne!(
            lupin.verdict,
            "fail(E0201)",
            "lupin {version} does not parse the moded store on {} — \
             [gram.expr.assign] binds all three machines",
            entry.display()
        );
    }
}

/// `outs[0] = xs` then `(mut xs).push(4)`: the store copied, so `xs`
/// is live and grows alone. Before s180: E1001 on both wolfgang lanes.
#[test]
fn a_plain_list_store_copies_and_the_binding_stays_live() {
    let want = runs("outs0=1 xs=2\n");
    every_lane(
        "index_store_copies_list.lu",
        want,
        verdict("trap(use-after-move)"),
        Some(want),
    );
}

/// `m["k"] = xs` — the `Map` twin. Before s180: E1001 on both wolfgang
/// lanes.
#[test]
fn a_plain_map_store_copies_and_the_binding_stays_live() {
    let want = runs("m0=1 xs=2\n");
    every_lane(
        "index_store_copies_map.lu",
        want,
        verdict("trap(use-after-move)"),
        Some(want),
    );
}

/// `fn set_g[K, V](mut m: Map[K, V], k: K, v: V) { m[k] = v }` — a
/// plain store of a `read` parameter is a copy, not a lend. Before
/// s180: E1002 on both wolfgang lanes; lupin 0.1.38 already ran it.
#[test]
fn a_read_parameter_stored_plainly_is_copied_not_lent() {
    let want = runs("m0=1 xs=2\n");
    every_lane("index_store_read_param.lu", want, want, Some(want));
}

/// `outs[0] = take xs` moves; the later push is E1001. Before s180:
/// E0201 on all three machines (the grammar had no slot).
#[test]
fn a_take_list_store_moves() {
    every_lane(
        "index_store_take_list.lu",
        verdict("fail(E1001)"),
        verdict("fail(E0201)"),
        Some(verdict("trap(use-after-move)")),
    );
}

/// `m["k"] = take xs` moves; the later push is E1001.
#[test]
fn a_take_map_store_moves() {
    every_lane(
        "index_store_take_map.lu",
        verdict("fail(E1001)"),
        verdict("fail(E0201)"),
        Some(verdict("trap(use-after-move)")),
    );
}

/// `m[k] = take v` on a `read` `v` is E1014 (`push(take v)`'s answer).
/// lupin's posture on the lend rule is a documented conservatism
/// (`read_param_take.lu`: it runs that shape clean), so past 0.1.38 the
/// assertion on it is only that the moded store parses.
#[test]
fn a_take_store_of_a_read_parameter_is_e1014() {
    every_lane(
        "index_store_take_read_param.lu",
        verdict("fail(E1014)"),
        verdict("fail(E0201)"),
        None,
    );
}
