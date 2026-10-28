//! #947: an exit-4 lend refusal has a code a reader can explain.
use std::process::Command;

#[test]
fn the_lend_refusal_has_an_explanation_with_both_workarounds() {
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .args(["--explain", "E1016"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("copy"), "{text}");
    assert!(text.contains("bind"), "{text}");
    assert!(text.contains("refusal"), "{text}");
}
