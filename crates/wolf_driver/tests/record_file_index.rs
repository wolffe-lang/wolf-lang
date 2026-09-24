//! s181 (wolf-lang#437): the observation record names the FILE a
//! diagnostic's span lies in, and the checked lane, the native lane and
//! lupin agree on the span and on the file.
//!
//! `[proto.record.diag]` carries `{code, span, severity}` with byte
//! offsets. In a package of several files those offsets alone do not
//! say which file they index, so before s181 a span from a sibling
//! module read as a range in the entry file: the witness's duplicate
//! `area` is at `[245, 249]` of `geometry/shapes.lu`, and bytes
//! `[245, 249]` of `main.lu` are `s.lu`, inside a doc comment.
//!
//! The rule the clause makes, and this file pins: a top-level `files`
//! array (package-relative, the entry at index 0) and an integer
//! `file` on EVERY diagnostic, present exactly when some diagnostic
//! lies outside the entry file; both absent otherwise, so a single-file
//! record is byte-identical to one written before the clause.
//!
//! The lupin leg needs lupin 0.1.39: 0.1.38 refuses both keys
//! (wolf-interp `ba357aa`, `src/schema.rs:58-59` and `:258`), so
//! against 0.1.38 this file is red by construction.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn corpus(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel)
}

/// The witness package: `main.lu` uses `geometry`, whose one file
/// defines `area` twice.
fn witness() -> PathBuf {
    corpus("resolve/sibling_diag/main.lu")
}

/// One wolfgang record. `None` means the host cannot run the native
/// lane (the s59 skip pattern), never a silent pass.
fn record_in(cwd: Option<&Path>, entry: &Path, flag: &str) -> Option<serde_json::Value> {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run").arg(entry).arg(flag).arg("--json");
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd.output().expect("wolf runs");
    if out.status.code() == Some(2) && flag == "--native" && out.stdout.is_empty() {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "conform-run {flag} on {} printed no record ({e}): {}",
            entry.display(),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    Some(rec)
}

fn record(entry: &Path, flag: &str) -> Option<serde_json::Value> {
    record_in(None, entry, flag)
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

/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying "one was
/// arranged here" (r10/#253): an absent sibling there is a failed
/// fetch, and a green skip would hide it.
fn lupin_or_skip() -> Option<PathBuf> {
    let found = sibling_lupin();
    if found.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #437's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still held to the EXPECTED record here; only the oracle leg is absent"
        );
    }
    found
}

/// The file a diagnostic names, resolved per the clause: a
/// diagnostic without `file` names the entry, which is index 0.
fn named_file(rec: &serde_json::Value, diag: &serde_json::Value) -> String {
    match diag.get("file") {
        None => "<entry>".to_string(),
        Some(i) => {
            let i = i.as_u64().expect("`file` is an integer") as usize;
            if i == 0 {
                return "<entry>".to_string();
            }
            rec["files"][i]
                .as_str()
                .unwrap_or_else(|| panic!("`file` {i} indexes past `files` in {rec}"))
                .to_string()
        }
    }
}

/// `(code, span, file)` for every diagnostic, in record order.
fn located(rec: &serde_json::Value) -> Vec<(String, String, String)> {
    rec["diagnostics"]
        .as_array()
        .expect("diagnostics is an array")
        .iter()
        .map(|d| {
            (
                d["code"].as_str().unwrap_or("").to_string(),
                d["span"].to_string(),
                named_file(rec, d),
            )
        })
        .collect()
}

/// What every machine must say about the witness, asserted against the
/// EXPECTED record and not only lane against lane: cross-lane equality
/// alone passes when the lanes are wrong together.
fn assert_witness_record(rec: &serde_json::Value, who: &str) {
    assert_eq!(rec["verdict"], "fail(E0302)", "{who}: verdict in {rec}");
    assert_eq!(
        rec["files"],
        serde_json::json!(["main.lu", "geometry/shapes.lu"]),
        "{who}: the record must name the files its spans lie in (#437) — {rec}"
    );
    assert_eq!(
        rec["diagnostics"],
        serde_json::json!([
            { "code": "E0302", "span": [245, 249], "severity": "error", "file": 1 }
        ]),
        "{who}: the diagnostic must carry its file index (#437) — {rec}"
    );
}

/// The span reads as the offending name in the file the record names,
/// and as something else in the entry — which is the whole defect.
#[test]
fn the_witness_span_is_in_the_sibling_and_not_in_the_entry() {
    let pkg = witness();
    let pkg = pkg.parent().unwrap();
    let sibling = std::fs::read(pkg.join("geometry/shapes.lu")).unwrap();
    let entry = std::fs::read(pkg.join("main.lu")).unwrap();
    assert_eq!(&sibling[245..249], b"area");
    assert_ne!(
        &entry[245..249],
        b"area",
        "the witness no longer discriminates: the entry has `area` at the same bytes"
    );
}

#[test]
fn both_wolfgang_lanes_name_the_sibling_file() {
    let checked = record(&witness(), "--checked").expect("the checked lane always runs");
    assert_witness_record(&checked, "checked");
    if let Some(native) = record(&witness(), "--native") {
        assert_witness_record(&native, "native");
        assert_eq!(
            located(&checked),
            located(&native),
            "the wolfgang lanes disagree on span or file"
        );
    }
}

/// Paths are package-relative, so the record does not depend on where
/// the run was started — the same record from the repo root, from the
/// package directory, and by absolute path.
#[test]
fn files_are_package_relative_wherever_the_run_starts() {
    let abs = std::fs::canonicalize(witness()).unwrap();
    let pkg = abs.parent().unwrap().to_path_buf();
    let from_root = record(&abs, "--checked").unwrap();
    let from_pkg = record_in(Some(&pkg), Path::new("main.lu"), "--checked").unwrap();
    let from_parent = record_in(
        Some(pkg.parent().unwrap()),
        Path::new("sibling_diag/main.lu"),
        "--checked",
    )
    .unwrap();
    for (rec, how) in [
        (&from_root, "absolute path"),
        (&from_pkg, "cwd = package root"),
        (&from_parent, "cwd = package root's parent"),
    ] {
        assert_eq!(rec["files"], from_root["files"], "{how}: {rec}");
        assert_eq!(rec["diagnostics"], from_root["diagnostics"], "{how}: {rec}");
    }
    assert_witness_record(&from_pkg, "checked, cwd = package root");
}

/// lupin agrees on the code, the span AND the file — and its schema
/// validator accepts wolfgang's record, keys and all.
#[test]
fn lupin_agrees_on_span_and_file() {
    let Some(lupin) = lupin_or_skip() else {
        return;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(witness())
        .arg("--json")
        .output()
        .expect("lupin runs");
    let theirs: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("lupin's record parses: {e}"));
    let ours = record(&witness(), "--checked").unwrap();
    assert_eq!(
        located(&theirs),
        located(&ours),
        "lupin ({}) and wolfgang disagree on code, span or file — lupin's record: {theirs}",
        lupin.display()
    );
    assert_witness_record(&theirs, "lupin");

    // The counterparty's strict validator is the other half of
    // "additive": it must accept a record carrying both keys.
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s181-witness-record.json");
    std::fs::write(&path, ours.to_string()).unwrap();
    let v = Command::new(&lupin)
        .args(["protocol", "validate"])
        .arg(&path)
        .output()
        .expect("lupin protocol validate runs");
    assert!(
        v.status.success(),
        "lupin's validator refuses wolfgang's record: {}{}",
        String::from_utf8_lossy(&v.stdout),
        String::from_utf8_lossy(&v.stderr)
    );
}

// --------------------------------------------- what must NOT move ----

/// A single-file package never carries either key.
#[test]
fn a_single_file_record_carries_neither_key() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s181_single_file");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.lu");
    std::fs::write(&entry, "fn main() -> !int {\n    let s: str = 1\n    0\n}\n").unwrap();
    let rec = record(&entry, "--checked").unwrap();
    assert!(
        rec["verdict"].as_str().unwrap_or("").starts_with("fail("),
        "the control must carry a diagnostic to mean anything: {rec}"
    );
    assert!(rec.get("files").is_none(), "{rec}");
    for d in rec["diagnostics"].as_array().unwrap() {
        assert!(d.get("file").is_none(), "{rec}");
    }
}

/// A multi-file package whose diagnostics all lie in the entry carries
/// neither key either: the rule is about where the SPANS are, not about
/// how the package was staged, so two machines that stage it
/// differently still emit the same key set. `corpus/resolve/private`
/// is `fail(E0304)` at the call in `main.lu`.
#[test]
fn a_multi_file_package_with_every_span_in_the_entry_carries_neither_key() {
    let rec = record(&corpus("resolve/private/main.lu"), "--checked").unwrap();
    assert_eq!(rec["verdict"], "fail(E0304)", "{rec}");
    assert!(rec.get("files").is_none(), "{rec}");
    for d in rec["diagnostics"].as_array().unwrap() {
        assert!(d.get("file").is_none(), "{rec}");
    }
}

/// When the keys are present, EVERY diagnostic carries `file`, entry
/// ones as `0`, and `files` lists each named file once, in order of
/// first appearance. `corpus/resolve/cycle` has diagnostics in two
/// sibling modules.
#[test]
fn every_diagnostic_carries_file_once_the_keys_are_present() {
    let rec = record(&corpus("resolve/cycle/main.lu"), "--checked").unwrap();
    let files = rec["files"].as_array().expect("cycle's spans leave the entry");
    assert_eq!(files[0], "main.lu", "{rec}");
    let mut seen = vec![0u64];
    for d in rec["diagnostics"].as_array().unwrap() {
        let i = d["file"].as_u64().unwrap_or_else(|| panic!("no `file` on {d} in {rec}"));
        if !seen.contains(&i) {
            assert_eq!(i, seen.len() as u64, "indices appear in order: {rec}");
            seen.push(i);
        }
    }
    assert_eq!(seen.len(), files.len(), "every listed file is named: {rec}");
}
