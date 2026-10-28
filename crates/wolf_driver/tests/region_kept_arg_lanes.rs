//! s227 (wolf-lang#660) — what a call can keep, it is taken to keep.
//!
//! `in r { f(mut outer, s) }` aims what `f` ALLOCATES at `r`; `s`'s
//! bytes stay in the region block that built them, and `f` may store
//! them in `outer` (`l.last = copy s` shares a `str`'s bytes, ruling
//! #35; a `push` or an index store lends them). Until s227 a `mut`
//! argument gained one site, the call's own in the ambient region, so
//! nothing pinned `outer` to the block: native and release printed the
//! freed chunk, the checked machine stale bytes, and lupin traps
//! `region-fault`. Every row below is red at trunk a027b643 (`exit(0)`
//! on checked, native and release) and E1010 after.
//!
//! The legal side is gated too, in one row: a string built inside the
//! `in` block, a scalar, a literal, bytes the callee can only copy, a
//! `mut` argument that can name no bytes, a builtin that keeps nothing
//! of its key, `copy region { }` then the call, and the two shapes the
//! first cut refused in the downstream census (wolf-book's `pargrep`,
//! lobo's reload) — kept bytes are shared, never placed.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every entry on the NATIVE lane only; this
//! gate asks checked, native, release and lupin.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

/// wolf-interp#289: only these published records retain the pre-mirror
/// outer-copy answer. An unlisted release or dev commit must mirror wolf.
fn copy_before_mirror(version: &str, commit: &str) -> bool {
    [("0.1.55", "e642f26"), ("0.1.56", "28547b4")]
        .iter()
        .any(|(v, c)| *v == version && commit.starts_with(c))
}

#[test]
fn the_copy_pin_names_only_the_measured_release_records() {
    assert!(copy_before_mirror("0.1.55", "e642f26"));
    assert!(copy_before_mirror("0.1.56", "28547b4"));
    assert!(!copy_before_mirror("0.1.57", "28547b4"));
    assert!(!copy_before_mirror("0.1.56", "unknown"));
    assert!(!copy_before_mirror("0.1.57", "fb054c8"));
    assert!(!copy_before_mirror("0.1.58", "6240efe"));
}

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    commit: String,
    codes: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
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
             (LUPIN={}) — the oracle leg of wolf-lang#660's gate did not run",
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

const ISSUE: &str = "wolf-lang#660";

const RULED_LUPIN: &str = "trap(region-fault)";

/// The paired lupins that run a region-built `str` kept in a `List`,
/// or as a `Map` key, to `exit(0)` instead of trapping
/// (wolffe-lang/wolf-interp#232: the dynamic machine misses a list
/// element and a map key reaching into a freed region). Pinned by
/// version so the pin dies with the fix: a lupin outside this list
/// must trap. Measured on the published 0.1.50 archive (the pairing
/// since wolf 0.2.27): all seven rows `exit(0)`, as 0.1.49 answered;
/// and on the published 0.1.51 archive (the 0.2.28 pairing, r33; #232
/// still open), the same; and on the published 0.1.52 archive (the
/// 0.2.29 pairing, r34; #232 still open), the same; and on the published
/// 0.1.53 archive (the 0.2.30 pairing, r35; #232 still open), the same;
/// and on the published 0.1.54 archive (the 0.2.31 pairing, r36; #232
/// still open), the same; and on the published 0.1.55 archive (the
/// 0.2.34 pairing, r39; #232 still open), the same.
// r40/r41: published 0.1.56/0.1.57 retain these seven measured answers (#232).
// r41 resume: published .58 re-measured these exact seven rows.
const LUPIN_MISSES_LIST_ELEMENTS: &[&str] = &[
    "0.1.50", "0.1.51", "0.1.52", "0.1.53", "0.1.54", "0.1.55", "0.1.56", "0.1.57", "0.1.58",
];

fn base_version(v: &str) -> &str {
    v.split('+').next().unwrap_or(v)
}

/// Checked, native and release refuse the program E1010.
fn refused_by_the_compiler(entry: &Path) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "fail(E1010)",
            "{flag} admitted an argument kept past its region: {} ({ISSUE})",
            entry.display()
        );
        assert!(
            obs.codes.iter().any(|c| c == "E1010"),
            "{flag} refused {} without E1010: {:?}",
            entry.display(),
            obs.codes
        );
    }
}

/// The compiler refuses; lupin traps `region-fault`.
fn refused_everywhere(entry: &Path) {
    refused_by_the_compiler(entry);
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

/// The compiler refuses; lupin traps, except the version pinned at
/// wolf-interp#232, which runs the stale read to `exit(0)`.
fn refused_and_lupin_pinned(entry: &Path) {
    refused_by_the_compiler(entry);
    if let Some(lupin) = lupin_says(entry) {
        if LUPIN_MISSES_LIST_ELEMENTS.contains(&base_version(&lupin.version)) {
            // The release answers `exit(0)`, exactly. A dev build of
            // the pinned version may already carry the fix and trap.
            let release = lupin.version == base_version(&lupin.version);
            assert!(
                lupin.verdict == "exit(0)" || (!release && lupin.verdict == RULED_LUPIN),
                "lupin {} on {}: {} (pinned at exit(0), wolf-interp#232)",
                lupin.version,
                entry.display(),
                lupin.verdict
            );
            eprintln!(
                "PINNED: lupin {} answers {} on {} (wolf-interp#232)",
                lupin.version,
                lupin.verdict,
                entry.display()
            );
        } else {
            assert_eq!(
                lupin.verdict,
                RULED_LUPIN,
                "lupin {} is past the wolf-interp#232 pin and must trap on {}",
                lupin.version,
                entry.display()
            );
        }
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

/// A fixture beside this test: the `List` shapes, kept out of
/// `corpus/` while lupin runs them (a corpus row is one truth for
/// every machine).
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/kept_arg")
        .join(name)
        .join("main.lu");
    assert!(p.exists(), "fixture missing: {}", p.display());
    p
}

/// The report itself: a field store through `copy s`.
#[test]
fn a_str_kept_in_a_mut_argument_s_field_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_field.lu"));
}

/// The callee's `Map` index store, which lends.
#[test]
fn a_str_kept_through_a_map_index_store_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_map.lu"));
}

/// A method's `mut self`.
#[test]
fn a_str_kept_in_a_mut_receiver_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_method.lu"));
}

/// Two `mut` arguments, one living in the region block.
#[test]
fn one_mut_argument_kept_in_another_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_two_mut.lu"));
}

/// A generic callee.
#[test]
fn a_generic_callee_keeping_its_argument_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_generic.lu"));
}

/// A callee in another module: the rule reads the signature alone.
#[test]
fn a_callee_in_another_module_keeping_its_argument_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_module/main.lu"));
}

/// The bytes are reached through a `read` aggregate.
#[test]
fn a_str_inside_a_read_struct_argument_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_read_struct.lu"));
}

/// A `take` argument moved into the `mut` one.
#[test]
fn a_take_argument_kept_in_a_mut_argument_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_take.lu"));
}

/// The receiver is the source.
#[test]
fn a_receiver_kept_in_a_mut_argument_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_recv.lu"));
}

/// A closure that captured the string. The checked machine refuses at
/// the memory check, before it would decline the closure.
#[test]
fn a_closure_argument_s_captures_kept_are_refused() {
    refused_everywhere(&corpus("memory/region_kept_arg_closure.lu"));
}

/// The result form with a `read` argument.
#[test]
fn a_result_keeping_a_read_argument_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_result_read.lu"));
}

/// A method's result drawn from its `read self`.
#[test]
fn a_result_keeping_its_read_receiver_is_refused() {
    refused_everywhere(&corpus("memory/region_kept_result_method.lu"));
}

/// No call at all: `copy d` shares the `str` inside `d` — until s255.
/// Admitted by `[mem.region.outer-write]`: the `copy` at the store into
/// the outer `o` (`in st { o = copy d }`) is the materializing copy, made
/// in `st`, so the row runs on checked, native and release and reads its
/// own bytes back; the sharing copy bound inside the block keeps s227's
/// refusal (`e1010_copy_bound_in_block_shares_str`). lupin 0.1.55 shares
/// the copy's bytes and traps `region-fault` — the is-lane's column,
/// pinned so a lupin that mirrors the clause moves it on purpose.
#[test]
fn a_copy_of_a_struct_at_a_store_into_an_outer_value_runs() {
    let entry = corpus("memory/region_copy_shares_str.lu");
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict, "exit(0)",
            "{flag} on the admitted row: {:?}",
            obs.codes
        );
        assert_eq!(
            obs.stdout, "28\nbuilt in pass, 0 uses\n",
            "{flag} stdout on the admitted row"
        );
    }
    if let Some(lupin) = lupin_says(&entry) {
        if copy_before_mirror(&lupin.version, &lupin.commit) {
            assert_eq!(
                lupin.verdict, "trap(region-fault)",
                "lupin {} {} retains the measured wolf-interp#289 lag",
                lupin.version, lupin.commit
            );
        } else {
            assert_eq!(lupin.verdict, "exit(0)", "unlisted lupin must mirror #289");
            assert_eq!(
                lupin.stdout, "28\nbuilt in pass, 0 uses\n",
                "mirrored copy's bytes"
            );
        }
    }
}

/// The copy a plain index store makes.
#[test]
fn an_index_store_s_copy_sharing_a_region_str_is_refused() {
    refused_everywhere(&corpus("memory/region_copy_index_store.lu"));
}

/// The builtin `(mut xs).push(s)` under `in st { }`.
#[test]
fn a_builtin_push_under_in_is_refused() {
    refused_and_lupin_pinned(&fixture("builtin_push"));
}

/// A callee's push.
#[test]
fn a_callee_s_push_is_refused() {
    refused_and_lupin_pinned(&fixture("callee_push"));
}

/// A generic callee's push.
#[test]
fn a_generic_callee_s_push_is_refused() {
    refused_and_lupin_pinned(&fixture("generic_push"));
}

/// `o = mk(s)` returning a `List[str]`.
#[test]
fn a_list_result_keeping_a_read_argument_is_refused() {
    refused_and_lupin_pinned(&fixture("result_list"));
}

/// `o = m.pairs()`: a builtin's result drawn from its receiver.
#[test]
fn a_builtin_result_drawn_from_its_receiver_is_refused() {
    refused_and_lupin_pinned(&fixture("map_pairs"));
}

/// A `Map` store keeps its KEY: `in st { m[s] = 1 }`. Red at trunk
/// a027b643 as a silent wrong answer — native and release answered
/// `m[k] else 0` with `0` for the key just stored, its bytes freed and
/// reused. lupin runs it (the same miss as a list element's,
/// wolf-interp#232).
#[test]
fn a_map_store_s_region_built_key_is_refused() {
    refused_and_lupin_pinned(&fixture("map_key_store"));
}

/// The same store with no `in` at all — the one shape of this family
/// that never needed the door.
#[test]
fn a_map_store_s_region_built_key_is_refused_without_in() {
    refused_and_lupin_pinned(&fixture("map_key_no_in"));
}

/// The sound neighbours, every one green at trunk a027b643: they must
/// stay green.
#[test]
fn the_sound_neighbours_still_run() {
    runs_everywhere(
        &corpus("memory/region_kept_arg_legal.lu"),
        "built in pass, 0 uses|21|static|built in st 0|21|7|7|21|2|carried out|a.log,b.log|conf-0\n",
    );
}
