//! s216 (wolf-lang#612, `[mem.region.copyout]`) — `copy region { … }`:
//! a block whose value is copied into the region it was entered from
//! before its own region is freed, so a long-running loop keeps each
//! turn's result and frees the turn's scratch.
//!
//! Before s216 every way to hand a region block's result to longer-lived
//! state was E1010 — or, through #618, an unsound pass. These tests hold
//! the form to four things on checked, native and release, and lupin:
//! the copy is deep and right (strings, lists, maps, tuples, structs,
//! read after a clobbering region reuses the freed chunk); the region
//! is really freed (a raw pointer into it reads row P4 on the checked
//! machine); every OTHER escape is still E1010 (a `return`, an outer
//! binding) and a value with no independent copy is refused (a channel,
//! a fn value); and the loop runs in bounded memory — #612's measured
//! shape, at 20,000 and 40,000 turns.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every entry on the NATIVE lane only.

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
             (LUPIN={}) — the oracle leg of wolf-lang#612's gate did not run",
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

const ISSUE: &str = "wolf-lang#612";

/// lupin releases that predate the mirror (wolf-interp PR for s216):
/// they read `copy region { … }` as `copy` of a plain block, whose
/// value they trap `region-fault` at the `}` — or, for a value with no
/// site, run. Emptied at the pairing that carries the mirror.
const PRE_COPYOUT_LUPIN: &[&str] = &["0.1.47", "0.1.48"];

/// Checked, native and release run the row to `exit(0)` printing
/// `want`; lupin does too, or — for a version in `PRE_COPYOUT_LUPIN` —
/// traps `region-fault` (it frees the block's value with the region).
fn copies_out_everywhere(entry: &Path, want: &str) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "exit(0)",
            "{flag} on {} ({ISSUE}): {:?}",
            entry.display(),
            obs.codes
        );
        assert_eq!(obs.stdout, want, "{flag} stdout on {}", entry.display());
    }
    if let Some(lupin) = lupin_says(entry) {
        if lupin.verdict == "exit(0)" {
            assert_eq!(
                lupin.stdout,
                want,
                "lupin {} stdout on {}",
                lupin.version,
                entry.display()
            );
            return;
        }
        assert!(
            PRE_COPYOUT_LUPIN.contains(&lupin.version.as_str()),
            "lupin {}'s verdict on {}: {}, want exit(0)",
            lupin.version,
            entry.display(),
            lupin.verdict
        );
        assert_eq!(
            lupin.verdict,
            "trap(region-fault)",
            "lupin {} (pre-mirror) verdict on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// Checked, native and release refuse the row E1010. lupin traps
/// `region-fault`, or — pre-mirror, for a value lupin's plain `copy`
/// lets through — runs it.
fn refused_everywhere(entry: &Path, lupin_pre_runs: bool) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "fail(E1010)",
            "{flag} admitted {} ({ISSUE})",
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
        if lupin.verdict == "trap(region-fault)" {
            return;
        }
        assert!(
            lupin_pre_runs && PRE_COPYOUT_LUPIN.contains(&lupin.version.as_str()),
            "lupin {}'s verdict on {}: {}, want trap(region-fault)",
            lupin.version,
            entry.display(),
            lupin.verdict
        );
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin {} (pre-mirror) on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// #612's loop: twenty `str`s and a `List` per turn in `scratch`, one
/// `str` kept per turn. At trunk 85de08ff the spelling already parsed
/// (`copy` of a block) and, through #618, ran the block's value from
/// freed bytes — printing the right answer by luck, so this row alone
/// is not red there; with #618 fixed and no copy-out it is
/// `fail(E1010)` (s216's e84d369e), and the struct and map rows below,
/// whose values are non-`Copy`, are `fail(E1010)` at trunk itself.
#[test]
fn the_loop_keeps_each_turn_s_result() {
    copies_out_everywhere(
        &corpus("memory/region_copyout_loop.lu"),
        "piece 49 3-49 50\n",
    );
}

/// The copy is deep: a struct of a `List[str]`, a view and an `int`,
/// read after a clobbering region reuses the freed chunk.
#[test]
fn a_struct_of_strings_and_a_list_copies_out_deep() {
    copies_out_everywhere(
        &corpus("memory/region_copyout_struct.lu"),
        "plan-7 8 w7-0 w7-2 3 26890\n",
    );
}

/// Maps (string keys and values), a `List[str]` and a tuple.
#[test]
fn maps_and_tuples_copy_out() {
    copies_out_everywhere(
        &corpus("memory/region_copyout_map.lu"),
        "value-20 value-30 v1 3 4 (1, two) 26890\n",
    );
}

/// `continue`, `break` and `?` leave the block without a copy, freeing
/// the region on the way.
#[test]
fn every_other_exit_frees_and_copies_nothing() {
    copies_out_everywhere(&corpus("memory/region_copyout_exits.lu"), "12 7 none\n");
}

/// A `return` and an outer binding are still E1010.
#[test]
fn every_other_escape_is_still_refused() {
    refused_everywhere(&corpus("memory/region_copyout_return.lu"), false);
    refused_everywhere(&corpus("memory/region_copyout_binding.lu"), false);
}

/// A channel and a fn value have no independent copy. lupin before the
/// mirror runs the channel row (its `copy` of a handle is the handle).
#[test]
fn a_value_with_no_independent_copy_is_refused() {
    refused_everywhere(&corpus("memory/region_copyout_handle.lu"), true);
    refused_everywhere(&corpus("memory/region_copyout_closure.lu"), false);
}

/// The region is freed: a raw pointer into its backing, read after the
/// `}`, is row P4 on the checked machine.
#[test]
fn the_region_is_freed_a_raw_read_after_is_p4() {
    let entry = corpus("memory/region_copyout_raw_ub.lu");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg("--checked")
        .arg("--json")
        .output()
        .expect("wolf runs");
    assert!(out.status.success(), "conform-run --checked failed");
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record parses");
    assert_eq!(rec["verdict"], "ub(mem.ub)", "checked verdict: {rec}");
    assert_eq!(rec["x-ub-row"], "P4", "checked row: {rec}");
}

// ------------------------------------------------- bounded memory --

/// #612's witness at `n` turns, built on one tier, its peak resident
/// set (`VmHWM`, kB) read by the program itself from
/// `/proc/self/status` — linux only; elsewhere `None`, printed as a
/// SKIP. `kept` is what each turn keeps: `"str"` (#612's shape, one
/// `str` per turn stored into a map) or `"none"` (the turn's work
/// discarded but its length).
fn hwm_kb(case: &str, src: &str, release: bool, n: u32) -> Option<u64> {
    if !cfg!(target_os = "linux") {
        eprintln!("SKIP: {case}: the bounded-memory witness reads /proc/self/status (linux only)");
        return None;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("region_copyout_lanes")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, src).expect("write fixture");
    let bin = dir.join(if release { "w-release" } else { "w-native" });
    let mut build = Command::new(wolf());
    build.current_dir(&dir).arg("build");
    if release {
        build.arg("--release");
    }
    let out = build
        .arg("main.lu")
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("wolf build runs");
    if lane_exit::environment_refusal(&out, "wolf build") {
        eprintln!(
            "SKIP: {case}: environment cannot build: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "wolf build {case}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&bin)
        .arg(n.to_string())
        .output()
        .expect("witness runs");
    assert!(
        run.status.success(),
        "{case} at {n} turns exited {:?}",
        run.status
    );
    let text = String::from_utf8_lossy(&run.stdout);
    let hwm = text
        .lines()
        .find_map(|l| l.strip_prefix("hwm="))
        .unwrap_or_else(|| panic!("{case} printed no hwm line: {text}"));
    Some(
        hwm.trim()
            .parse()
            .unwrap_or_else(|_| panic!("{case} hwm `{hwm}`")),
    )
}

const WITNESS: &str = r#"
struct St {
    vals: Map[str, str],
    n: int,
}

fn build(i: int) -> str {
    var parts = List[str]()
    var k = 0
    while k < 20 {
        (mut parts).push("piece {i} {k}")
        k += 1
    }
    "{parts[3]}-{i}"
}

fn hwm() -> str {
    let text = fs_read_text("/proc/self/status") else ""
    for line in text.lines() {
        if line.starts_with("VmHWM:") {
            return (line.strip_prefix("VmHWM:") else "").trim().strip_suffix("kB") else "?"
        }
    }
    "?"
}

STEP

fn main() -> int {
    let args = env_args()
    let n = if args.len > 0 { args[0].to_int() else 20000 } else { 20000 }
    var st = St { vals: Map[str, str](), n: 0 }
    var i = 0
    while i < n {
        step(mut st, i)
        i += 1
    }
    print("{st.vals["x"] else "?"} {st.n}")
    print("hwm={hwm().trim()}")
    0
}
"#;

const STEP_NO_REGION: &str = r#"
fn step(mut st: St, i: int) {
    let v = build(i)
    st.vals["x"] = v
    st.n += 1
}
"#;

const STEP_COPY_STR: &str = r#"
fn step(mut st: St, i: int) {
    let v = copy region scratch {
        build(i)
    }
    st.vals["x"] = v
    st.n += 1
}
"#;

const STEP_COPY_INT: &str = r#"
fn step(mut st: St, i: int) {
    let k = copy region scratch {
        build(i).len
    }
    st.n += k
}
"#;

/// Growth from 20,000 to 40,000 turns, in bytes per turn, on one tier.
fn growth_per_turn(case: &str, step: &str, release: bool) -> Option<f64> {
    let src = WITNESS.replace("STEP", step);
    let a = hwm_kb(case, &src, release, 20_000)?;
    let b = hwm_kb(case, &src, release, 40_000)?;
    eprintln!("{case}: VmHWM {a} kB at 20000 turns, {b} kB at 40000");
    Some((b.saturating_sub(a) as f64) * 1024.0 / 20_000.0)
}

/// The control: with no region, every turn's twenty strings and list
/// stay allocated for the life of the process — #612's ~1.3 KB per
/// turn. If this ever reads flat, the instrument has gone blind and
/// the bounded rows below prove nothing.
#[test]
fn the_control_grows_without_a_region() {
    for release in [false, true] {
        let Some(g) = growth_per_turn("no_region", STEP_NO_REGION, release) else {
            continue;
        };
        assert!(
            g > 1000.0,
            "no-region control grew {g:.0} B/turn (release={release}); the instrument cannot see growth"
        );
    }
}

/// `copy region` frees the scratch: what still grows is the kept value
/// itself (one ~20-byte `str` per turn, superseded in the map but never
/// freed by the process-root arena that holds the state) — bounded
/// here at 64 bytes a turn against the control's ~1,300.
#[test]
fn the_loop_frees_its_scratch_keeping_only_its_result() {
    for release in [false, true] {
        let Some(g) = growth_per_turn("copy_str", STEP_COPY_STR, release) else {
            continue;
        };
        assert!(
            g < 64.0,
            "copy region kept-str loop grew {g:.0} B/turn (release={release}), want < 64 ({ISSUE})"
        );
    }
}

/// Keeping only a scalar, the loop is flat: under 4 bytes a turn
/// (page-granular noise).
#[test]
fn a_loop_keeping_a_scalar_is_flat() {
    for release in [false, true] {
        let Some(g) = growth_per_turn("copy_int", STEP_COPY_INT, release) else {
            continue;
        };
        assert!(
            g < 4.0,
            "copy region scalar loop grew {g:.0} B/turn (release={release}), want flat ({ISSUE})"
        );
    }
}
