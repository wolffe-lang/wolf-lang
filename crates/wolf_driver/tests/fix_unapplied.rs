//! #616: a maybe suggestion is visible, and an invalid file never succeeds.
use std::process::Command;

#[test]
fn maybe_fixes_are_reported_and_invalid_files_fail_even_with_apply() {
    let src = "fn keep(xs: List[int]) -> List[int] {\n    xs\n}\nfn main() {\n    var mine = List[int]()\n    (mut mine).push(1)\n    var theirs = keep(mine)\n    (mut theirs).push(2)\n    print(\"mine has {mine.len}, theirs has {theirs.len}\")\n}\n";
    let dir = std::env::temp_dir().join(format!("wolf-fix-maybe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.lu");
    for apply in [false, true] {
        std::fs::write(&file, src).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_wolf"));
        cmd.arg("fix").arg(&file).env_remove("WOLF_STD");
        if apply {
            cmd.arg("--apply");
        }
        let out = cmd.output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(1), "apply={apply}: {text}");
        assert!(text.contains("unapplied"), "{text}");
        assert!(text.contains("maybe"), "{text}");
        assert!(text.contains("E1002"), "{text}");
        assert!(text.contains("main.lu:2:5"), "a full site: {text}");
        assert!(!text.contains("nothing to fix"), "{text}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), src);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn apply_rechecks_for_errors_uncovered_by_the_edits() {
    let dir = std::env::temp_dir().join(format!("wolf-fix-recheck-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.lu");
    std::fs::write(
        &file,
        "fn main() {\n    let s = \"abc\"\n    let b = s[-1]\n    missing_name()\n}\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .args(["fix", "--apply"])
        .arg(&file)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(std::fs::read_to_string(&file).unwrap().contains("s[^1]"));
    assert!(
        err.contains("E0301"),
        "the post-edit error must be reported: {err}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_without_a_suggestion_is_not_nothing_to_fix() {
    let dir = std::env::temp_dir().join(format!("wolf-fix-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.lu");
    std::fs::write(&file, "fn main() {\n    missing_name()\n}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("fix")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("E0301"));
    std::fs::remove_dir_all(dir).unwrap();
}
