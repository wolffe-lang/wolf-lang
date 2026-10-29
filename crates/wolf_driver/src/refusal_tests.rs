//! The exit discipline and record shape for the coded lend refusal (#947).
use super::*;

#[test]
fn a_lend_refusal_keeps_exit_four_and_has_a_code_and_site() {
    const CHILD: &str = "WOLF_S271_REFUSAL_REPORT_CHILD";
    let src = "fn consume(view: List[byte]) {\n    let owned = view\n}\n";
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("lend.lu"));
    let mut sources = Sources::new();
    sources.add(file, "lend.lu", src.as_bytes());
    let lo = src.rfind("view").unwrap() as u32;
    let span = wolf_span::Span::new(file, lo, lo + 4);
    let reason = "a lent `bytes()` view in a value position (bind it with `let` to materialize)";
    if std::env::var_os(CHILD).is_some() {
        report_build_stop(
            "build",
            BuildStop::Refused {
                phase: "mem",
                reason: reason.to_string(),
                span: Some(span),
            },
            &sources,
        );
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "refusal_tests::a_lend_refusal_keeps_exit_four_and_has_a_code_and_site",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(4), "{err}");
    assert!(err.contains("error[E1016]"), "{err}");
    assert!(err.contains("lend.lu:2:17"), "{err}");
    assert!(err.contains("copy") && err.contains("bind"), "{err}");
    let mut extensions = Vec::new();
    record_refusal(&mut extensions, reason, Some(span), &sources);
    let record: serde_json::Value = extensions
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    assert_eq!(record["x-unsupported-code"], "E1016");
    assert_eq!(record["x-unsupported-file"], "lend.lu");
    assert_eq!(record["x-unsupported-line"], 2);
    assert_eq!(record["x-unsupported-col"], 17);
}
