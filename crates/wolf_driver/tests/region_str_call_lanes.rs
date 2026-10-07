//! s216 (wolf-lang#618) — a call's `str` result is an allocation in
//! the caller's ambient region, so it cannot outlive the region it was
//! made in.
//!
//! `[mem.region.escape]` names "non-`Copy` call results" as sites, and
//! s160 added the materializing `str` builtins by name. A call to a
//! DECLARED fn returning `str` was neither — `str` is `Copy` — so the
//! callee's bytes, built in its caller's region (`[mem.region.create.3]`,
//! D12), left a `region` block with no E1010: the block's value, a
//! binding outside, an index store through a `mut` parameter (#612's
//! own witness), a view the callee returned of its argument, and a
//! builtin container method's element (`xs.last()`). Checked, native
//! and release ran every one from freed bytes at 0.2.24 and at trunk
//! 85de08ff; lupin traps `region-fault`.
//!
//! The legal side is gated too: a call used inside its region, a call
//! made outside any region, and a fn whose every result is a literal
//! (boreutils' `bore.io_error()` shape), whose bytes are static.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every entry on the NATIVE lane only; this
//! gate asks checked, native, release and lupin.

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
    version: String,
    codes: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// One wolfgang lane. `None` means the host cannot run the native lane
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

/// lupin's observation, or `None` when this box has no sibling.
/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying "one was
/// arranged here" (r10/#253): there an absent sibling is a failed
/// fetch, and a gate that answers broken plumbing with a green skip is
/// not a gate.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of wolf-lang#618's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the EXPECTED answer here; only the oracle \
             leg is absent"
        );
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

/// The corpus row itself is the program: one truth per file.
fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.exists(), "corpus row missing: {}", p.display());
    p
}

const ISSUE: &str = "wolf-lang#618";

const RULED_LUPIN: &str = "trap(region-fault)";

/// Checked, native and release refuse the program E1010; lupin traps
/// `region-fault` (every paired lupin since 0.1.45 does: the dynamic
/// machine tracks where a `str`'s bytes live, so nothing is pinned).
fn refused_everywhere(entry: &Path) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "fail(E1010)",
            "{flag} admitted a call's str escaping its region: {} ({ISSUE})",
            entry.display()
        );
        assert!(
            obs.codes.iter().any(|c| c == "E1010"),
            "{flag} refused {} without E1010: {:?}",
            entry.display(),
            obs.codes
        );
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            RULED_LUPIN,
            "lupin {}'s verdict on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// Every lane, lupin included, runs the program to `exit(0)` printing
/// `want`.
fn runs_everywhere(entry: &Path, want: &str) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "exit(0)",
            "{flag} on the legal row {}: {:?}",
            entry.display(),
            obs.codes
        );
        assert_eq!(obs.stdout, want, "{flag} stdout on {}", entry.display());
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin {} on the legal row {}",
            lupin.version,
            entry.display()
        );
        assert_eq!(
            lupin.stdout,
            want,
            "lupin {} stdout on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// The block's own value. Red at trunk 85de08ff: `exit(0)` on checked,
/// native and release.
#[test]
fn a_call_s_str_as_the_region_block_s_value_is_refused() {
    refused_everywhere(&corpus("memory/region_str_call_block_value.lu"));
}

/// Held by a binding declared outside the region. Red at trunk
/// 85de08ff.
#[test]
fn a_call_s_str_held_outside_its_region_is_refused() {
    refused_everywhere(&corpus("memory/region_str_call_held.lu"));
}

/// #612's own first witness: stored through a `mut` parameter's map.
/// Red at trunk 85de08ff.
#[test]
fn a_call_s_str_stored_through_a_mut_parameter_is_refused() {
    refused_everywhere(&corpus("memory/region_str_call_index_store.lu"));
}

/// The callee hands back a view of its argument's region bytes. Red at
/// trunk 85de08ff.
#[test]
fn a_call_returning_a_view_of_its_argument_is_refused() {
    refused_everywhere(&corpus("memory/region_str_call_view.lu"));
}

/// A builtin container method's `str` element. Red at trunk 85de08ff.
#[test]
fn a_container_method_s_str_element_held_outside_is_refused() {
    refused_everywhere(&corpus("memory/region_str_elem_call_held.lu"));
}

/// Literal-only helpers are static: their results leave the region.
/// Green at trunk 85de08ff, and the conservative rule alone would have
/// refused it (boreutils `cat`, measured): it must stay green.
#[test]
fn a_literal_only_helper_s_result_leaves_its_region() {
    runs_everywhere(
        &corpus("memory/region_str_call_static.lu"),
        "[Input/output error] [zero] [static]\n",
    );
}

/// Used inside the region, or made outside any region. Green at trunk
/// 85de08ff, and it must stay green.
#[test]
fn a_call_s_str_inside_its_region_or_outside_any_still_runs() {
    runs_everywhere(
        &corpus("memory/region_str_call_inside.lu"),
        "built-1-tail\nbuilt-2-tail\n[built-3-tail]\n",
    );
}
