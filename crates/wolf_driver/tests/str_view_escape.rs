//! s171 (wolf-lang#392) — a `[mem.str.view]` product carries its
//! receiver's sites, so it cannot outlive the region whose bytes it
//! names.
//!
//! `[mem.region.escape]` said "a slice and every `[mem.str.view]`
//! product allocate nothing"; `[mem.str.view]` said every yielded
//! `str` "is a subslice of the receiver's own storage". Read together
//! as "allocates nothing, so no site", a view of a region-built `str`
//! escaped clean while the `str` ITSELF leaving was E1010 — the same
//! bytes, one word narrower, and only one of those answers can be
//! right.
//!
//! The refusal lives in the shared mem tier, so both wolf lanes see
//! it; these tests assert the refusal AND the three shapes that must
//! stay legal, because a rule that refuses everything is not the rule.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    codes: Vec<String>,
}

fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag == "--native" {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn fixture(case: &str, body: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, body).expect("write fixture");
    entry
}

fn refused(case: &str, body: &str) {
    let entry = fixture(case, body);
    for flag in ["--checked", "--native"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict, "fail(E1010)",
            "{flag} admitted a view escaping its region ({case}, #392)"
        );
        assert!(
            obs.codes.iter().any(|c| c == "E1010"),
            "{flag} refused {case} without E1010: {:?}",
            obs.codes
        );
    }
}

fn allowed(case: &str, body: &str) {
    let entry = fixture(case, body);
    for flag in ["--checked", "--native"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict, "exit(0)",
            "{flag} refused a LEGAL view shape ({case}): {:?}",
            obs.codes
        );
    }
}

/// The issue's own shape: `trim` of a region-built `str`, returned.
#[test]
fn a_view_of_a_region_built_str_may_not_leave_the_region() {
    refused(
        "s171_view_return",
        r#"
fn build() -> str {
    region scratch {
        let s = "  re" + "gions  "
        let t = s.trim()
        t
    }
}

fn main() -> !int {
    print("{build()}")
    0
}
"#,
    );
}

/// The whole view family, not just `trim` — and a `split` piece, whose
/// LIST is an allocation but whose strings are subslices of the
/// receiver.
#[test]
fn every_view_product_of_a_region_built_str_is_refused() {
    for (case, expr) in [
        ("trim_start", r#"s.trim_start()"#),
        ("trim_end", r#"s.trim_end()"#),
        ("strip_prefix", r#"s.strip_prefix("  ") else s"#),
    ] {
        refused(
            &format!("s171_view_{case}"),
            &format!(
                r#"
fn build() -> str {{
    region scratch {{
        let s = "  re" + "gions  "
        let t = {expr}
        t
    }}
}}

fn main() -> !int {{
    print("{{build()}}")
    0
}}
"#
            ),
        );
    }
}

/// A view of a LITERAL is no site at all: static bytes, nothing to
/// outlive. If this ever refuses, the rule has become "a view is
/// radioactive".
#[test]
fn a_view_of_a_literal_escapes_freely() {
    allowed(
        "s171_view_literal",
        r#"
fn build() -> str {
    region scratch {
        let t = "  hello  ".trim()
        t
    }
}

fn main() -> !int {
    print("{build()}")
    0
}
"#,
    );
}

/// A view used INSIDE the region that owns its bytes is exactly what
/// `[mem.str.view]` exists for, and stays free.
#[test]
fn a_view_used_inside_its_own_region_stays_legal() {
    allowed(
        "s171_view_inside",
        r#"
fn main() -> !int {
    region scratch {
        let s = "  re" + "gions  "
        print(s.trim())
        var n = 0
        for w in s.words() { n = n + 1 }
        print("{n}")
    }
    0
}
"#,
    );
}

/// Only the RECEIVER's sites flow. `s.strip_prefix(p)` is a subslice
/// of `s` and never of `p`, so a region-built NEEDLE must not pin the
/// result: the view here is of a literal and escapes, though the
/// needle was built in the region.
#[test]
fn an_argument_s_sites_do_not_attach_to_the_view() {
    allowed(
        "s171_view_needle",
        r#"
fn build() -> str {
    region scratch {
        let needle = "he" + "llo"
        let t = "hello world".strip_prefix(needle) else "?"
        t
    }
}

fn main() -> !int {
    print("{build()}")
    0
}
"#,
    );
}
