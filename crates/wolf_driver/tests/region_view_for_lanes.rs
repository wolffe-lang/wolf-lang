//! s207 (wolf-lang#540) — a piece bound by a `for` over
//! `words()`/`lines()`/`split()` carries its receiver's sites, so it
//! cannot outlive the region whose bytes it names.
//!
//! `[mem.region.escape]`'s s171 extension says a `[mem.str.view]`
//! product carries its receiver's sites and names "the pieces of
//! `split`/`words`/`lines`". A piece reached by index (`ps[0]`) was
//! refused; a piece bound by a `for` was not, because the mem tier gave
//! the loop binding no sites at all — so `last = w` returned out of the
//! region printed from freed bytes on checked, native and release, while
//! lupin at is68 (wolf-interp#126) traps `region-fault`. The loop binding
//! now carries the iterable's sites, less the LIST a view call
//! allocates (it holds no piece's bytes).
//!
//! The gate also runs is68's 19 view-escape shapes, every one already
//! E1010, and a legal row whose every line must keep running.
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
             (LUPIN={}) — the oracle leg of wolf-lang#540's gate did not run",
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

const ISSUE: &str = "wolf-lang#540";

/// The corpus row itself is the program: one truth per file.
fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.exists(), "corpus row missing: {}", p.display());
    p
}

/// lupin releases that predate wolf-interp#126 (is68): they run every
/// view escape to `exit(0)`. is68's own build reported the same
/// version and trapped, so a pinned version may answer either. Emptied
/// at the 0.1.45 pairing (r27): 0.1.45 carries is68 and traps every one.
const PRE_VIEW_TRAP_LUPIN: &[&str] = &[];

/// lupin releases that run a `return` out of a region block
/// (wolf-interp#178): is68 included. 0.1.45 (the 0.2.22 pairing, r27)
/// still runs it, measured; kept with its issue.
const PRE_RETURN_TRAP_LUPIN: &[&str] = &["0.1.44", "0.1.45"];

const RULED_LUPIN: &str = "trap(region-fault)";

/// Checked, native and release refuse the program E1010; lupin traps
/// `region-fault`, or — for a version in `pre` — runs it to `exit(0)`.
fn refused_everywhere(entry: &Path, pre: &[&str]) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "fail(E1010)",
            "{flag} admitted a for-bound piece escaping its region: {} ({ISSUE})",
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
        if lupin.verdict == RULED_LUPIN {
            return;
        }
        assert!(
            pre.contains(&lupin.version.as_str()),
            "lupin {}'s verdict on {}: {}, want {RULED_LUPIN}",
            lupin.version,
            entry.display(),
            lupin.verdict
        );
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin {} (pre-mirror) verdict on {}",
            lupin.version,
            entry.display()
        );
    }
}

fn fixture(case: &str, body: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("region_view_for_lanes")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, body).expect("write fixture");
    entry
}

/// The issue's program. Red at trunk dfcc2f13: `exit(0)`, `[here]` on
/// all three tiers.
#[test]
fn a_words_piece_returned_out_of_its_region_is_refused() {
    refused_everywhere(
        &corpus("memory/region_str_view_for_words.lu"),
        PRE_VIEW_TRAP_LUPIN,
    );
}

/// Held by a binding declared outside the region. Red at trunk
/// dfcc2f13.
#[test]
fn a_words_piece_held_outside_its_region_is_refused() {
    refused_everywhere(
        &corpus("memory/region_str_view_for_held.lu"),
        PRE_VIEW_TRAP_LUPIN,
    );
}

/// `lines()` and `split()` (with a `break`). Red at trunk dfcc2f13.
#[test]
fn lines_and_split_pieces_returned_out_of_their_region_are_refused() {
    for row in [
        "memory/region_str_view_for_lines.lu",
        "memory/region_str_view_for_split.lu",
    ] {
        refused_everywhere(&corpus(row), PRE_VIEW_TRAP_LUPIN);
    }
}

/// A `for` over the pieces held in a local; a piece of a piece and its
/// `trim`; the piece as a `region` block's value. Red at trunk
/// dfcc2f13.
#[test]
fn a_piece_through_a_place_a_chain_or_a_block_value_is_refused() {
    for row in [
        "memory/region_str_view_for_place.lu",
        "memory/region_str_view_for_nested.lu",
        "memory/region_str_view_for_value.lu",
    ] {
        refused_everywhere(&corpus(row), PRE_VIEW_TRAP_LUPIN);
    }
}

/// `return w` from inside the loop. Red at trunk dfcc2f13; lupin runs
/// it at 0.1.44 and at is68 (wolf-interp#178).
#[test]
fn a_piece_returned_from_inside_the_loop_is_refused() {
    refused_everywhere(
        &corpus("memory/region_str_view_for_return.lu"),
        PRE_RETURN_TRAP_LUPIN,
    );
}

/// Not a view: the elements of a list a call built in the region. Red
/// at trunk dfcc2f13. lupin 0.1.44 already traps it, so nothing is
/// pinned.
#[test]
fn an_element_of_a_region_built_list_held_outside_is_refused() {
    refused_everywhere(&corpus("memory/region_str_list_for_held.lu"), &[]);
}

/// `copy` of a `str` shares its bytes (`[mem.tier0.move.3]`), so the
/// copy of a region-built `str`, or of a piece of one, still names the
/// region. Red at trunk dfcc2f13: `exit(0)` on all three tiers. lupin
/// 0.1.44 traps the first and runs the second (the piece).
#[test]
fn a_copied_str_still_names_its_region() {
    refused_everywhere(&corpus("memory/region_str_copy_return.lu"), &[]);
    refused_everywhere(
        &corpus("memory/region_str_view_for_copy.lu"),
        PRE_VIEW_TRAP_LUPIN,
    );
}

/// The legal side: pieces of a parameter, a literal, a literal local
/// and a string built outside the region; pieces used inside; an `int`
/// drawn from a piece; a `List[int]` element. Every lane runs it, lupin
/// included. Green at trunk dfcc2f13, and it must stay green.
#[test]
fn pieces_whose_bytes_outlive_the_region_still_run() {
    let entry = corpus("memory/region_str_view_for_inside.lu");
    let want = "regions\nhere\n2\n[here] [here] [gions] [here] [11] [7]\n";
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict, "exit(0)",
            "{flag} on the legal row: {:?}",
            obs.codes
        );
        assert_eq!(obs.stdout, want, "{flag} stdout on the legal row");
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(
            lupin.verdict, "exit(0)",
            "lupin {} on the legal row",
            lupin.version
        );
        assert_eq!(
            lupin.stdout, want,
            "lupin {} stdout on the legal row",
            lupin.version
        );
    }
}

/// is68's 19 view-escape shapes (wolf-interp `tests/rulings_is68/
/// v126_*`, the rows that are not `for` pieces or controls), verbatim.
/// Every one was already E1010 on the three tiers at trunk dfcc2f13;
/// lupin traps each from is68 on.
const IS68_SHAPES: &[(&str, &str)] = &[
    (
        "v126_block_value",
        "fn main() -> !int {\n    let t = region scratch {\n        let s = \"  re\" + \"gions  \"\n        s.trim()\n    }\n    print(\"[{t}]\")\n    0\n}\n",
    ),
    (
        "v126_copy_in_region",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = copy s.trim()\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_field_view",
        "struct Doc { title: str }\n\nfn build() -> str {\n    region scratch {\n        let d = Doc { title: \"  re\" + \"gions  \" }\n        d.title.trim()\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_get_inclusive",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        s.get(2..=5) else \"?\"\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_get_open",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s.get(2..) else \"?\"\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_get_range",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s.get(2..6) else \"?\"\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_held_outside",
        "fn main() -> !int {\n    var keep = \"\"\n    region scratch {\n        let s = \"  ab\" + \"cd  \"\n        keep = s.trim()\n    }\n    print(\"[{keep}]\")\n    0\n}\n",
    ),
    (
        "v126_issue",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions  \"\n        let t = s.trim()\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"{build()}\")\n    0\n}\n",
    ),
    (
        "v126_proc_send",
        "fn worker(n: int, out: channel[str]) -> !int {\n    let s = \"  ab\".repeat(n)\n    out.send(s.trim())?\n    0\n}\n\nfn main() -> !int {\n    let out = channel[str](4)\n    let c = spawn proc worker(2, out)\n    let m = c.monitor()\n    let v = out.recv() else \"none\"\n    print(\"[{v}]\")\n    0\n}\n",
    ),
    (
        "v126_slice",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s[2..6]\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_split_index",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let ps = s.split(\",\")\n        ps[0]\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_strip_prefix",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s.strip_prefix(\"  \") else \"?\"\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_strip_prefix_tail",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        s.strip_prefix(\"  \") else \"?\"\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_strip_suffix",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s.strip_suffix(\"  \") else \"?\"\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_trim",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s.trim()\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_trim_end",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s.trim_end()\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_trim_start",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s.trim_start()\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_trim_tail",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        s.trim()\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
    (
        "v126_view_of_view",
        "fn build() -> str {\n    region scratch {\n        let s = \"  re\" + \"gions,x  \"\n        let t = s.trim().trim_end()\n        t\n    }\n}\n\nfn main() -> !int {\n    print(\"[{build()}]\")\n    0\n}\n",
    ),
];

#[test]
fn is68s_nineteen_view_escapes_stay_refused() {
    assert_eq!(IS68_SHAPES.len(), 19, "is68 measured nineteen shapes");
    for (case, body) in IS68_SHAPES {
        refused_everywhere(&fixture(case, body), PRE_VIEW_TRAP_LUPIN);
    }
}
