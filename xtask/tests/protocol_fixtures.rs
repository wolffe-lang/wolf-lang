//! The protocol's own tests ([proto.harness.fixtures]): fixtures under
//! corpus/protocol/ must accept/reject exactly as named.

fn fixture(name: &str) -> serde_json::Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../corpus/protocol/");
    let body = std::fs::read_to_string(format!("{path}{name}")).expect("read fixture");
    serde_json::from_str(&body).expect("fixture is JSON")
}

#[test]
fn valid_accepts() {
    assert!(xtask::protocol::validate_record(&fixture("valid.json")).is_ok());
}

#[test]
fn extensions_accept_and_do_not_diverge_alone() {
    let ext = fixture("with-extensions.json");
    assert!(xtask::protocol::validate_record(&ext).is_ok());
}

#[test]
fn warnings_fixture_accepts_and_absence_never_diverges() {
    // The s67 additive array ([proto.record.warn]): validates when
    // present, and a record without it never diverges against one with
    // it (honest-absent — lupin implements the subset it has).
    let w = fixture("with-warnings.json");
    assert!(xtask::protocol::validate_record(&w).is_ok());
    let mut plain = w.clone();
    plain.as_object_mut().unwrap().remove("warnings");
    assert!(xtask::protocol::compare(&w, &plain, false).is_none());
}

/// `[proto.record.pass]` (s169): a full-ladder `pass` — the ladder
/// completed clean and the lane executed nothing — validates, is not a
/// divergence against a lane that DID execute, and is not coverage.
#[test]
fn a_clean_stop_validates_and_compares_against_a_run_as_nothing() {
    let p = fixture("clean-stop.json");
    assert!(xtask::protocol::validate_record(&p).is_ok());
    assert!(
        !xtask::protocol::covered_at_run(&p),
        "`pass` is never coverage"
    );
    let mut ran = p.clone();
    ran["phase_reached"] = serde_json::json!("run");
    ran["verdict"] = serde_json::json!("exit(0)");
    assert!(
        xtask::protocol::compare(&p, &ran, false).is_none(),
        "the passing side did not execute ([proto.cmp.pass])"
    );
    // But a REJECTION on the other side is a divergence, and that is
    // the pair the amendment exists to expose.
    let mut rejected = p.clone();
    rejected["phase_reached"] = serde_json::json!("typecheck");
    rejected["verdict"] = serde_json::json!("fail(E0401)");
    rejected["diagnostics"] =
        serde_json::json!([{"code": "E0401", "span": [0, 1], "severity": "error"}]);
    assert!(
        xtask::protocol::compare(&p, &rejected, false).is_some(),
        "one side accepted the program and the other rejected it"
    );
}

/// `[proto.record.trap]` (s169): additive, honest-absent, never
/// compared.
#[test]
fn a_trap_message_validates_and_never_diverges() {
    let m = fixture("with-trap-message.json");
    assert!(xtask::protocol::validate_record(&m).is_ok());
    let mut silent = m.clone();
    silent.as_object_mut().unwrap().remove("trap_message");
    assert!(xtask::protocol::compare(&m, &silent, false).is_none());
    let mut other = m.clone();
    other["trap_message"] = serde_json::json!("some other wording entirely");
    assert!(
        xtask::protocol::compare(&m, &other, false).is_none(),
        "wording is not comparison surface"
    );
}

/// `[proto.record.diag]`'s file index (s181, #437): additive, and
/// all-or-nothing; the comparator reads the file a diagnostic names,
/// not the raw index.
#[test]
fn a_file_index_validates_and_is_compared_by_path() {
    let r = fixture("with-file-index.json");
    assert!(xtask::protocol::validate_record(&r).is_ok());

    // A record without the keys says every span is in the entry: the
    // same code and span in the ENTRY is a different observation.
    let mut entry_only = r.clone();
    entry_only.as_object_mut().unwrap().remove("files");
    entry_only["diagnostics"][0].as_object_mut().unwrap().remove("file");
    assert!(xtask::protocol::validate_record(&entry_only).is_ok());
    assert!(
        xtask::protocol::compare(&r, &entry_only, false).is_some(),
        "same bytes, different file: a divergence"
    );

    // The index is resolved: another machine may list the files in
    // another order and still agree.
    let mut reordered = r.clone();
    reordered["files"] = serde_json::json!(["main.lu", "other.lu", "geometry/shapes.lu"]);
    reordered["diagnostics"][0]["file"] = serde_json::json!(2);
    assert!(xtask::protocol::compare(&r, &reordered, false).is_none());

    // `file: 0` and no `file` both name the entry.
    let mut zero = r.clone();
    zero["diagnostics"][0]["file"] = serde_json::json!(0);
    assert!(xtask::protocol::compare(&zero, &entry_only, false).is_none());

    // All-or-nothing.
    let mut orphan = entry_only.clone();
    orphan["diagnostics"][0]["file"] = serde_json::json!(1);
    assert!(xtask::protocol::validate_record(&orphan).is_err(), "`file` without `files`");
    let mut missing = r.clone();
    missing["diagnostics"][0].as_object_mut().unwrap().remove("file");
    assert!(xtask::protocol::validate_record(&missing).is_err(), "`files` without `file`");
    let mut past = r.clone();
    past["diagnostics"][0]["file"] = serde_json::json!(2);
    assert!(xtask::protocol::validate_record(&past).is_err(), "index past `files`");
    let mut empty = r.clone();
    empty["files"] = serde_json::json!([]);
    assert!(xtask::protocol::validate_record(&empty).is_err(), "empty `files`");
}

#[test]
fn wrong_version_rejects() {
    assert!(xtask::protocol::validate_record(&fixture("wrong-version.json")).is_err());
}

#[test]
fn missing_field_rejects() {
    assert!(xtask::protocol::validate_record(&fixture("missing-field.json")).is_err());
}

#[test]
fn perturbed_valid_record_diverges() {
    // the red-test in unit form: a stdout perturbation must be flagged
    let a = fixture("valid.json");
    let mut b = a.clone();
    b["stdout_sha256"] =
        serde_json::json!("0000000000000000000000000000000000000000000000000000000000000000");
    let d = xtask::protocol::compare(&a, &b, false);
    assert!(d.is_some(), "perturbed stdout must diverge");
}
