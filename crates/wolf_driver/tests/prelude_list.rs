//! `wolf prelude` from outside the process (s212; ruling #39 = B,
//! wolf-lang#586, wolf-lsp#32).
//!
//! The list is generated from the checker's own tables, and these tests
//! hold the three things a consumer relies on: every row of those
//! tables is in the output (a name the resolver accepts cannot be
//! missing from the list), the output parses as the documented schema,
//! and `spec/prelude.json` — the copy a vendoring consumer and `cargo
//! xtask prelude-diff` read at a tag — is byte-for-byte the output.

use std::path::Path;
use std::process::{Command, Output};

use wolf_sema::prelude;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wolf"))
        .args(args)
        .output()
        .expect("wolf runs")
}

fn stdout(args: &[&str]) -> String {
    let out = run(args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "`wolf {}` succeeds; stderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8")
}

fn json() -> serde_json::Value {
    serde_json::from_str(&stdout(&["prelude", "--json"])).expect("--json parses")
}

fn names_in(v: &serde_json::Value) -> Vec<String> {
    v["names"]
        .as_array()
        .expect("names is an array")
        .iter()
        .map(|e| e["name"].as_str().expect("a name").to_string())
        .collect()
}

/// The gate a planted omission reds: every row of the two ambient
/// tables — read here from the tables themselves, not from the
/// generator — is a `names` entry, in table order, with nothing extra.
#[test]
fn every_table_row_is_in_the_output() {
    let want: Vec<String> = prelude::PRELUDE
        .iter()
        .chain(prelude::BUILTIN_TYPES)
        .map(|a| a.name.to_string())
        .collect();
    let got = names_in(&json());
    let missing: Vec<&String> = want.iter().filter(|n| !got.contains(n)).collect();
    let extra: Vec<&String> = got.iter().filter(|n| !want.contains(n)).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "`wolf prelude --json` and the checker's tables part:\n  missing: {missing:?}\n  extra: {extra:?}"
    );
    assert_eq!(got, want, "same names, different order");
}

/// Every entry carries the documented fields with the documented types
/// (docs/prelude-json.md), and each row says what the table says.
#[test]
fn the_output_is_the_schema() {
    let v = json();
    assert_eq!(v["schema"], "wolf-prelude/0");
    let names = v["names"].as_array().expect("names");
    for (e, a) in names
        .iter()
        .zip(prelude::PRELUDE.iter().chain(prelude::BUILTIN_TYPES))
    {
        assert_eq!(e["name"], a.name);
        assert_eq!(e["kind"], a.kind.as_str(), "{}", a.name);
        match a.anchor {
            Some(x) => assert_eq!(e["anchor"], x, "{}", a.name),
            None => assert!(e["anchor"].is_null(), "{}", a.name),
        }
        assert_eq!(e["w0304"], prelude::shadow_hazard(a.name), "{}", a.name);
        assert_eq!(e.as_object().expect("an object").len(), 4, "{}", a.name);
    }
    let marks = v["marks"].as_array().expect("marks");
    let want = prelude::marks();
    assert_eq!(marks.len(), want.len());
    for (e, m) in marks.iter().zip(&want) {
        assert_eq!(e["name"], m.name.as_str());
        assert_eq!(e["anchor"], prelude::HOST_SIGS_ANCHOR);
        let by: Vec<&str> = e["by"]
            .as_array()
            .expect("by")
            .iter()
            .map(|b| b.as_str().expect("a builtin"))
            .collect();
        assert_eq!(by, m.by, "{}", m.name);
    }
}

/// `spec/prelude.json` is the output, byte for byte, so the copy read
/// at a tag is the list that tag's compiler printed.
#[test]
fn the_committed_copy_is_the_output() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/prelude.json");
    let committed = std::fs::read_to_string(&path).expect("spec/prelude.json reads");
    let printed = stdout(&["prelude", "--json"]);
    assert!(
        committed == printed,
        "spec/prelude.json is stale: regenerate it with \
         `cargo run -p wolf_driver -- prelude --json > spec/prelude.json`, and name the \
         change in the CHANGELOG (`cargo xtask prelude-diff <last tag> HEAD --markdown`)"
    );
}

/// The human table names every row and every mark; anything but
/// `--json` is a usage error.
#[test]
fn the_human_table_and_the_usage_error() {
    let h = stdout(&["prelude"]);
    for n in prelude::names() {
        assert!(
            h.lines()
                .any(|l| l.split_whitespace().next() == Some(n.name)),
            "{}",
            n.name
        );
    }
    for m in prelude::marks() {
        assert!(h.contains(&m.name), "{}", m.name);
    }
    let bad = run(&["prelude", "--yaml"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("usage: wolf prelude"));
}
