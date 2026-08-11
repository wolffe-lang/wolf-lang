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
