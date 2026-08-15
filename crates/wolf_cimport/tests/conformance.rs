//! The importer **conformance suite** (s46 Target 1).
//!
//! A directory of (headers, expected artifact dump) pairs, run against
//! whichever importer implementations exist. This suite is the reason
//! the libclang bootstrap can be burned: c15's embedded C frontend must
//! go green here before it may replace a worker, and "green" means the
//! *dumps match* — not that the internals look right.
//!
//! Each case is `conformance/<name>.h`, optionally with companion
//! headers `conformance/<name>.<n>.h` that it may `#include`, and an
//! expected `conformance/<name>.dump`.
//!
//! Regenerate expectations with `WOLF_CIMPORT_BLESS=1 cargo test -p
//! wolf_cimport --test conformance`, and **read the diff** — a blessed
//! change to a refusal is a change to what the compiler promises.
//!
//! To run the suite against a different importer, point
//! `WOLF_CIMPORT_CONFORMANCE_WORKER` at its executable. It then speaks
//! the wire protocol exactly as the compiler would, so a worker that
//! passes here passes for real.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wolf_cimport::cache::ImportRequest;
use wolf_cimport::protocol::{Request, Response};
use wolf_cimport::refworker::{MemHeaders, import};

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

/// A case's headers, keyed by the name they are `#include`d as.
fn case_headers(dir: &Path, name: &str) -> MemHeaders {
    let mut map = BTreeMap::new();
    for entry in std::fs::read_dir(dir).expect("conformance dir") {
        let p = entry.expect("dir entry").path();
        let Some(fname) = p.file_name().and_then(|f| f.to_str()) else {
            continue;
        };
        if !fname.ends_with(".h") {
            continue;
        }
        // `<name>.h` and its companions `<name>.<something>.h`.
        if fname == format!("{name}.h") || fname.starts_with(&format!("{name}.")) {
            let text = std::fs::read_to_string(&p).expect("header");
            map.insert(fname.to_string(), text);
        }
    }
    MemHeaders(map)
}

/// Every case name, sorted. A case is a `<name>.h` with no extra dots
/// in its stem (companions have them).
fn cases(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("conformance dir") {
        let p = entry.expect("dir entry").path();
        let Some(fname) = p.file_name().and_then(|f| f.to_str()) else {
            continue;
        };
        let Some(stem) = fname.strip_suffix(".h") else {
            continue;
        };
        if stem.contains('.') {
            continue; // a companion header
        }
        out.push(stem.to_string());
    }
    out.sort();
    out
}

/// The target every case imports for, unless its name says otherwise.
/// `<name>__<triple-ish>` runs on that target instead — the mechanism
/// that keeps per-target width bugs out.
fn target_for(name: &str) -> String {
    match name.rsplit_once("__") {
        Some((_, "win64")) => "x86_64-pc-windows-msvc".to_string(),
        Some((_, "arm64")) => "aarch64-unknown-linux-gnu".to_string(),
        _ => "x86_64-unknown-linux-gnu".to_string(),
    }
}

/// Import a case with the in-process reference worker.
fn import_reference(dir: &Path, name: &str) -> String {
    let req = ImportRequest {
        headers: vec![format!("{name}.h")],
        target: target_for(name),
        ..Default::default()
    };
    let a = import(&req, &case_headers(dir, name))
        .unwrap_or_else(|e| panic!("case `{name}` did not import: {e}"));
    wolf_cimport::dump(&a)
}

#[test]
fn the_suite_is_big_enough_to_mean_something() {
    let dir = conformance_dir();
    let cases = cases(&dir);
    assert!(
        cases.len() >= 15,
        "the conformance suite has {} cases; the s46 contract asks for at least \
         15 covering bitfields, tag collisions, internal linkage and demotions. \
         It grows every time a demotion or mapping bug is fixed.",
        cases.len()
    );
}

/// The suite proper: every case's dump matches its expectation.
#[test]
fn every_case_matches_its_expected_dump() {
    let dir = conformance_dir();
    let bless = std::env::var("WOLF_CIMPORT_BLESS").is_ok();
    let mut failures = Vec::new();

    for name in cases(&dir) {
        let got = import_reference(&dir, &name);
        let expected_path = dir.join(format!("{name}.dump"));
        if bless {
            std::fs::write(&expected_path, &got).expect("writes the expectation");
            continue;
        }
        let want = match std::fs::read_to_string(&expected_path) {
            Ok(w) => w,
            Err(_) => {
                failures.push(format!(
                    "case `{name}` has no expected dump. Run with \
                     WOLF_CIMPORT_BLESS=1 and review the result."
                ));
                continue;
            }
        };
        if got != want {
            failures.push(format!(
                "case `{name}` diverged.\n--- expected ---\n{want}\n--- got ---\n{got}"
            ));
        }
    }

    assert!(
        !bless,
        "the expectations were regenerated; re-run without WOLF_CIMPORT_BLESS \
         and review the diff before committing"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The artifact is byte-deterministic: importing the same headers twice
/// produces identical bytes. Without this the cache is a liability and
/// the suite compares noise.
#[test]
fn imports_are_byte_reproducible() {
    let dir = conformance_dir();
    for name in cases(&dir) {
        let req = ImportRequest {
            headers: vec![format!("{name}.h")],
            target: target_for(&name),
            ..Default::default()
        };
        let h = case_headers(&dir, &name);
        let a = import(&req, &h).expect("imports");
        let b = import(&req, &h).expect("imports");
        assert_eq!(
            wolf_cimport::encode(&a),
            wolf_cimport::encode(&b),
            "case `{name}` is not byte-reproducible"
        );
    }
}

/// Every case survives serialization. A dump that matches while the
/// binary form loses a field would hide a real bug behind a green test.
#[test]
fn every_case_survives_the_binary_round_trip() {
    let dir = conformance_dir();
    for name in cases(&dir) {
        let req = ImportRequest {
            headers: vec![format!("{name}.h")],
            target: target_for(&name),
            ..Default::default()
        };
        let a = import(&req, &case_headers(&dir, &name)).expect("imports");
        let back = wolf_cimport::decode(&wolf_cimport::encode(&a)).expect("decodes");
        assert_eq!(a, back, "case `{name}` did not survive the round trip");
    }
}

/// The suite must actually exercise refusals — a conformance suite of
/// only the happy path would let the honesty guarantee rot silently.
#[test]
fn the_suite_exercises_the_demotion_ladder() {
    let dir = conformance_dir();
    let mut seen_tags = std::collections::BTreeSet::new();
    let mut seen_levels = std::collections::BTreeSet::new();
    for name in cases(&dir) {
        let req = ImportRequest {
            headers: vec![format!("{name}.h")],
            target: target_for(&name),
            ..Default::default()
        };
        let a = import(&req, &case_headers(&dir, &name)).expect("imports");
        for (_, demotion, r) in a.refusals() {
            seen_tags.insert(r.tag());
            seen_levels.insert(demotion);
        }
    }
    assert!(
        seen_tags.len() >= 8,
        "the suite exercises only {} refusal kinds: {seen_tags:?}",
        seen_tags.len()
    );
    for level in [
        wolf_cimport::Demotion::Opaque,
        wolf_cimport::Demotion::ExternOnly,
        wolf_cimport::Demotion::ErrorOnUse,
    ] {
        assert!(
            seen_levels.contains(&level),
            "no case demotes to `{}` — the whole ladder must be covered",
            level.tag()
        );
    }
}

/// Run the suite against an out-of-process worker when one is pinned.
/// This is the mechanism the contract turns on: a new importer (the
/// libclang worker, or c15's frontend) proves itself against the
/// interface, over the real wire, before it may replace anything.
#[test]
fn an_external_worker_agrees_with_the_reference() {
    let Ok(program) = std::env::var("WOLF_CIMPORT_CONFORMANCE_WORKER") else {
        // No worker pinned: nothing to compare against, and that is a
        // normal CI run rather than a skipped obligation.
        return;
    };
    let dir = conformance_dir();
    let worker = wolf_cimport::Worker {
        program: PathBuf::from(&program),
        found_via: "WOLF_CIMPORT_CONFORMANCE_WORKER",
    };
    for name in cases(&dir) {
        let req = ImportRequest {
            headers: vec![format!("{name}.h")],
            include_paths: vec![dir.display().to_string()],
            target: target_for(&name),
            ..Default::default()
        };
        let resp = worker
            .ask(&Request::Import(req))
            .unwrap_or_else(|e| panic!("case `{name}`: {e}"));
        let Response::Artifact(bytes) = resp else {
            panic!("case `{name}`: worker did not answer with an artifact: {resp:?}");
        };
        let a = wolf_cimport::decode(&bytes).expect("decodes");
        // Compare the *dumps*, minus the lines that legitimately differ
        // between implementations: which importer answered, and where
        // the headers happened to live on this machine.
        let got = normalize(&wolf_cimport::dump(&a));
        let want = normalize(&import_reference(&dir, &name));
        assert_eq!(got, want, "case `{name}` diverged from the reference");
    }
}

/// Strip the lines that are about *this machine* rather than about C.
fn normalize(dump: &str) -> String {
    dump.lines()
        .filter(|l| !l.starts_with("importer:") && !l.starts_with("file "))
        .map(|l| match l.split_once(" @") {
            Some((head, _)) => head.to_string(),
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
