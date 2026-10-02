//! s193 (wolf-lang#494, found by s192). A view-set receiver
//! (`mut self.{x}`, `[mem.tier0.excl.3]`) holds only its view set's
//! fields; the caller keeps the rest. Its call's arguments run before
//! the claim (`[mem.tier0.excl.4]`), so a write they make to a field
//! OUTSIDE the view set stands, and the callee's writes to its view
//! fields stand beside it.
//!
//! Before s193 native and release spilled a by-value receiver whole
//! before the arguments ran and wrote the whole slot back after the
//! call, so the argument's write was overwritten by the stale copy:
//! `(mut p).set_x({ p.z = 9; p.z })` printed `10 3` where checked
//! printed `10 9` (trunk `c51a314d` and the 0.2.19 archive alike;
//! kasumi `~/lanes/s193/evidence/probes-trunk-c51a314d.log`). The
//! write-back now carries the view set's fields only, each rebuilt
//! into the variable's current value (`wolf_wir`'s `WriteBackShape::
//! View`). An element receiver and a re-lent `mut self` lend an
//! address and never spilled; their rows pin that.
//!
//! Every row asserts the checked machine's bytes on all three
//! wolfgang lanes: equality alone would pass with every lane wrong
//! together (s171's lesson, wave 45). `cargo xtask corpus` runs a row
//! on the native lane only, which is why this gate exists beside the
//! rows.
//!
//! lupin's column is wolf-interp's: 0.1.42 writes the receiver back
//! whole too, on every row here, the element and re-lent forms
//! included. It is pinned pre-mirror by version with those answers;
//! any later lupin must give the checked bytes (is62 moves lupin).

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

/// lupin releases measured before the mirror of wolf-lang#494: 0.1.42.
/// Emptied at the 0.1.43 pairing (r25): 0.1.43 carries is62's view-set
/// write-back and gives the checked bytes on every row.
const PRE_VIEW_LUPIN: &[&str] = &[];

/// A row that runs: the ruled bytes on every wolfgang lane; lupin
/// answers `pre` at a pinned version and the ruled bytes at any other.
fn agrees(name: &str, ruled: &str, pre: &str) {
    let entry = corpus(name);
    let want = runs(ruled);
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    check("checked", &entry, &checked, want);
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            check(flag, &entry, &obs, want);
        }
    }
    let Some((version, lupin)) = lupin_says(&entry) else {
        return;
    };
    if PRE_VIEW_LUPIN.contains(&version.as_str()) {
        check(
            &format!("lupin {version} (pre-mirror)"),
            &entry,
            &lupin,
            runs(pre),
        );
    } else {
        check(&format!("lupin {version}"), &entry, &lupin, want);
    }
}

// ------------------------------------- red at trunk on native/release --

/// The issue: `(mut p).set_x({ p.z = 9; p.z })` under `mut self.{x}`
/// (`10 3`).
#[test]
fn an_argument_write_outside_the_view_set_stands() {
    agrees("recv_view_arg_outside_write.lu", "10 9\n", "10 3\n");
}

/// Two fields outside the view, both written (`10 2 3`).
#[test]
fn two_argument_writes_outside_the_view_set_stand() {
    agrees("recv_view_arg_two_fields.lu", "10 7 9\n", "10 2 3\n");
}

/// `p.w.a = 9`, a nested struct field outside the view (`10 2 3`).
#[test]
fn a_nested_field_written_outside_the_view_set_stands() {
    agrees("recv_view_arg_nested_field.lu", "10 9 3\n", "10 2 3\n");
}

/// `mut self.{x, y}` with `z` written (`10 2 3`).
#[test]
fn the_third_field_written_beside_a_two_field_view_set_stands() {
    agrees("recv_view_arg_two_view_third.lu", "10 2 9\n", "10 2 3\n");
}

/// The callee writes both view fields; the argument wrote `z`
/// (`10 20 3`). Guards a write-back narrower than the view set.
#[test]
fn the_callees_view_field_writes_stand_beside_the_arguments() {
    agrees(
        "recv_view_arg_callee_view_write.lu",
        "10 20 9\n",
        "10 20 3\n",
    );
}

/// `(mut o.v).set_x(..)`, a field receiver with a view set; `o.v.z`
/// and the sibling `o.k` written (`10 3 8`).
#[test]
fn a_field_receivers_view_set_keeps_the_arguments_writes() {
    agrees("recv_view_arg_field_recv.lu", "10 9 8\n", "10 3 8\n");
}

/// A `List` field outside the view replaced (`4 1 5`: the replaced
/// buffer came back).
#[test]
fn a_list_field_replaced_outside_the_view_set_stands() {
    agrees("recv_view_arg_list_field.lu", "4 3 7\n", "4 1 5\n");
}

// ------------------------------------ already right on every wolfgang lane --

/// `(mut ps[0]).set_x({ ps[0].z = 9; .. })`: the element is lent by
/// address, so this never spilled.
#[test]
fn an_element_receiver_with_a_view_set_keeps_the_arguments_write() {
    agrees("recv_view_arg_elem.lu", "10 9 6\n", "10 3 6\n");
}

/// `(mut self).set_x({ self.z = 9; .. })` inside a `mut self` method:
/// the parameter is an address.
#[test]
fn a_relent_mut_self_with_a_view_set_keeps_the_arguments_write() {
    agrees("recv_view_arg_relend.lu", "10 9\n", "10 3\n");
}

/// The existing view-set litmus (`view_set_norm.lu`): the callee's own
/// writes come back, `z` read after the call. Every machine.
#[test]
fn the_view_set_litmus_still_runs() {
    let entry = corpus("view_set_norm.lu");
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    check("checked", &entry, &checked, verdict("exit(0)"));
    for flag in ["--native", "--release"] {
        if let Some(obs) = lane(&entry, flag) {
            check(flag, &entry, &obs, verdict("exit(0)"));
        }
    }
}
