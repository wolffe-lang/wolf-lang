//! The compile-failure footer names the code the reader was actually
//! shown (#249). Before this test the line said `wolf --explain E0201`
//! for every failure, so a build reporting `E0301` sent the reader to
//! the parser's entry — worse than silence for the one reader the line
//! exists to help, who has no way to tell they were misdirected.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// A fresh directory under the target tmpdir holding the named files.
fn fixture(case: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("explain_footer")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, src) in files {
        std::fs::write(dir.join(name), src).unwrap();
    }
    dir
}

/// `wolf build <entry>`'s stderr, with `WOLF_STD` scrubbed so the std
/// stub tables answer (fixtures never borrow a sibling checkout).
fn build_stderr(dir: &Path, entry: &str) -> String {
    let out = Command::new(wolf())
        .arg("build")
        .arg(dir.join(entry))
        .arg("-o")
        .arg(dir.join("out.bin"))
        .env_remove("WOLF_STD")
        .output()
        .expect("wolf runs");
    assert!(!out.status.success(), "the fixture must fail to compile");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The regression itself: two `main`s in one directory (D32) report
/// E0302, and the footer must say E0302.
#[test]
fn footer_names_the_reported_code_not_e0201() {
    let dir = fixture(
        "two_mains",
        &[
            (
                "hello.lu",
                "fn main() {\n    print(\"hello, wolf\\n\")\n}\n",
            ),
            ("try2.lu", "fn main() {\n    print(\"second\\n\")\n}\n"),
        ],
    );
    let err = build_stderr(&dir, "hello.lu");
    assert!(
        err.contains("error[E0302]"),
        "fixture reports E0302:\n{err}"
    );
    assert!(
        err.contains("`wolf --explain E0302`"),
        "the footer names the code actually reported:\n{err}"
    );
    assert!(
        !err.contains("--explain E0201"),
        "the hardcoded E0201 is gone:\n{err}"
    );
}

/// The other code the stranger walk hit: `use std.list` with no std
/// root is E0301, and the footer must say E0301.
#[test]
fn footer_names_e0301_for_a_missing_std_item() {
    let dir = fixture(
        "missing_std_item",
        &[(
            "main.lu",
            "use std.list\n\nfn main() {\n    print(\"hi\\n\")\n}\n",
        )],
    );
    let err = build_stderr(&dir, "main.lu");
    assert!(
        err.contains("error[E0301]"),
        "fixture reports E0301:\n{err}"
    );
    assert!(
        err.contains("`wolf --explain E0301`"),
        "the footer names the code actually reported:\n{err}"
    );
}

/// E0201 is still named when E0201 is what happened — the fix is "name
/// the first code", not "never say E0201".
#[test]
fn footer_still_names_e0201_for_a_parse_error() {
    let dir = fixture(
        "parse_error",
        &[("main.lu", "fn main() {\n    let x = 1 +\n}\n")],
    );
    let err = build_stderr(&dir, "main.lu");
    assert!(
        err.contains("error[E0201]"),
        "fixture reports E0201:\n{err}"
    );
    assert!(
        err.contains("`wolf --explain E0201`"),
        "the footer names the code actually reported:\n{err}"
    );
}

/// Every code the footer can name must be in the registry — the whole
/// point is that following the advice lands on a real explanation.
#[test]
fn the_named_code_explains() {
    let dir = fixture(
        "explainable",
        &[
            ("hello.lu", "fn main() {\n    print(\"a\\n\")\n}\n"),
            ("try2.lu", "fn main() {\n    print(\"b\\n\")\n}\n"),
        ],
    );
    let err = build_stderr(&dir, "hello.lu");
    let code = err
        .split("`wolf --explain ")
        .nth(1)
        .and_then(|s| s.split('`').next())
        .expect("the footer names a code")
        .to_string();
    let out = Command::new(wolf())
        .arg("--explain")
        .arg(&code)
        .output()
        .expect("wolf runs");
    assert!(
        out.status.success(),
        "`wolf --explain {code}` is a real catalog entry"
    );
}
