//! s259 (#889, #890): refusals keep their source identity; a typo in a
//! container's type arguments is E0301 on every machine, not a refusal.
//! s266 (#932): an `else` fallback that swallowed a range says so.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("wolf-s259-{}-{unique}-{name}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn corpus(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel)
}

fn record(entry: &Path, lane: &str, std: Option<&Path>) -> Option<serde_json::Value> {
    let mut cmd = Command::new(wolf());
    cmd.args(["conform-run", "--json", lane]).arg(entry);
    if let Some(std) = std {
        cmd.arg("--std-root").arg(std);
    }
    let out = cmd.output().expect("wolf runs");
    if lane != "--checked" && lane_exit::environment_refusal(&out, lane) {
        eprintln!(
            "SKIP: {lane} is unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    assert!(
        out.status.success(),
        "{lane}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(serde_json::from_slice(&out.stdout).expect("one JSON record"))
}

fn build(entry: &Path, out: &Path, std: Option<&Path>) -> Output {
    let mut cmd = Command::new(wolf());
    cmd.arg("build").arg(entry).arg("-o").arg(out);
    if let Some(std) = std {
        cmd.arg("--std-root").arg(std);
    }
    cmd.output().expect("wolf build runs")
}

#[test]
fn a_refusal_in_the_second_file_names_its_file_line_column_and_source() {
    let dir = Scratch::new("refusal");
    let entry = dir.path().join("main.lu");
    let second = dir.path().join("second.lu");
    let second_display = second.to_string_lossy().replace('\\', "/");
    let src = include_str!("fixtures/refusal_site/second.lu");
    std::fs::write(&entry, include_str!("fixtures/refusal_site/main.lu")).unwrap();
    std::fs::write(&second, src).unwrap();
    let out = build(&entry, &dir.path().join("out"), None);
    assert_eq!(
        out.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with(
            "wolf build: cannot compile this yet — methods on builtin types (the std surface)"
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("{second_display}:3:5")),
        "{stderr}"
    );
    assert!(stderr.contains("3 |     value.not_compiled()"), "{stderr}");
    assert!(stderr.contains("^^^^^^^^"), "{stderr}");
    assert!(!stderr.contains(" @"), "{stderr}");
    assert!(!stderr.contains("error["), "{stderr}");
    let lo = src.find("value.not_compiled").unwrap();
    for lane in ["--checked", "--native", "--release"] {
        let Some(rec) = record(&entry, lane, None) else {
            continue;
        };
        assert_eq!(rec["verdict"], "unsupported", "{lane}: {rec}");
        assert_eq!(rec["phase_reached"], "resolve", "{lane}: {rec}");
        assert_eq!(rec["diagnostics"], serde_json::json!([]), "{rec}");
        assert_eq!(
            rec["x-unsupported-construct"],
            "methods on builtin types (the std surface)"
        );
        assert_eq!(
            rec["x-unsupported-span"],
            serde_json::json!([lo, lo + "value.not_compiled()".len()])
        );
        assert_eq!(rec["x-unsupported-file"], second_display);
        assert_eq!(rec["x-unsupported-line"], 3);
        assert_eq!(rec["x-unsupported-col"], 5);
    }
}

#[test]
fn an_unresolved_container_argument_is_e0301_on_every_lane() {
    let dir = Scratch::new("unresolved");
    let mk = dir.path().join("main.lu");
    let std = corpus("rows/ctor_arg_qualified/std");
    std::fs::write(
        &mk,
        "use std.process\nfn main() -> int {\n    let ready = process.Child { slot: 0 }\n    var in_flight = Map[str, Child]()\n    0\n}\n",
    )
    .unwrap();
    for entry in [corpus("rows/negative/ctor_arg_unresolved.lu"), mk] {
        let src = std::fs::read_to_string(&entry).unwrap();
        let name = if src.contains("Child") {
            "Child"
        } else {
            "Nope"
        };
        let lo = src.find(&format!(", {name}")).unwrap() + 2;
        for lane in ["--checked", "--native", "--release"] {
            let Some(rec) = record(&entry, lane, Some(&std)) else {
                continue;
            };
            assert_eq!(rec["verdict"], "fail(E0301)", "{lane}: {rec}");
            let ds = rec["diagnostics"].as_array().unwrap();
            assert_eq!(ds.len(), 1, "{lane}: {rec}");
            assert_eq!(ds[0]["span"], serde_json::json!([lo, lo + name.len()]));
            assert!(rec.get("x-unsupported-construct").is_none(), "{rec}");
        }
        let out = build(&entry, &dir.path().join("out"), Some(&std));
        assert_eq!(out.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("error[E0301]: nothing named `{name}` is in scope")),
            "{stderr}"
        );
        assert!(
            stderr.contains("each argument must name a type"),
            "{stderr}"
        );
        assert!(!stderr.contains("not a bug in your program"), "{stderr}");
    }
}

/// s266 (wolf-lang#932, R-s266-1 A): an open-ended range that IS an
/// `else` fallback (`rest[i else 0 + 2..]`, `[gram.amb.else]`) is
/// refused under a name that says so, with the parenthesising note, on
/// the build and in every lane's record; the parenthesised and bound
/// spellings beside it run (`xtask corpus` holds their output).
#[test]
fn an_else_fallback_range_names_the_fallback_and_says_to_parenthesise() {
    const CONSTRUCT: &str = "an open-ended or end-relative range as an `else` fallback (slicing)";
    const NOTE: &str =
        "the `else` fallback extends to the end of the expression; parenthesise `(… else 0)`";
    let entry = corpus("rows/else_fallback_range_refused.lu");
    let src = std::fs::read_to_string(&entry).unwrap();
    let dir = Scratch::new("else-fallback");
    let out = build(&entry, &dir.path().join("out"), None);
    assert_eq!(
        out.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with(&format!(
            "wolf build: cannot compile this yet — {CONSTRUCT}"
        )),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("= note: {NOTE}")), "{stderr}");
    assert!(stderr.contains("0 + 2.."), "{stderr}");
    // The header prose spells the fallback too; the code is the last.
    let lo = src.rfind("0 + 2..").unwrap();
    for lane in ["--checked", "--native", "--release"] {
        let Some(rec) = record(&entry, lane, None) else {
            continue;
        };
        assert_eq!(rec["verdict"], "unsupported", "{lane}: {rec}");
        assert_eq!(rec["phase_reached"], "resolve", "{lane}: {rec}");
        assert_eq!(rec["x-unsupported-construct"], CONSTRUCT, "{lane}: {rec}");
        assert_eq!(rec["x-unsupported-note"], NOTE, "{lane}: {rec}");
        assert_eq!(
            rec["x-unsupported-span"],
            serde_json::json!([lo, lo + "0 + 2..".len()]),
            "{lane}: {rec}"
        );
    }
    // The ledger's own range refusal keeps its name and carries no note
    // outside a fallback, and inside one the author parenthesised: the
    // range there is what the author wrote.
    for (name, body) in [
        ("plain", "    let r = (2..)\n    print(\"{r.start}\")\n"),
        (
            "paren",
            "    let rest = \"a\"\n    let v = rest[rest.find(\"a\") else (2..)]\n    print(v)\n",
        ),
    ] {
        let file = dir.path().join(format!("{name}.lu"));
        std::fs::write(&file, format!("fn main() -> int {{\n{body}    0\n}}\n")).unwrap();
        let out = build(&file, &dir.path().join(name), None);
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(4), "{name}: {stderr}");
        assert!(
            stderr.starts_with(
                "wolf build: cannot compile this yet — open-ended or end-relative ranges (slicing)"
            ),
            "{name}: {stderr}"
        );
        assert!(!stderr.contains("= note:"), "{name}: {stderr}");
        let Some(rec) = record(&file, "--checked", None) else {
            continue;
        };
        assert_eq!(
            rec["x-unsupported-construct"], "open-ended or end-relative ranges (slicing)",
            "{name}: {rec}"
        );
        assert!(rec.get("x-unsupported-note").is_none(), "{name}: {rec}");
    }
}

#[test]
fn the_qualified_container_argument_runs_on_every_lane() {
    let entry = corpus("rows/ctor_arg_qualified/main.lu");
    let std = entry.parent().unwrap().join("std");
    for lane in ["--checked", "--native", "--release"] {
        let Some(rec) = record(&entry, lane, Some(&std)) else {
            continue;
        };
        assert_eq!(rec["verdict"], "exit(0)", "{lane}: {rec}");
    }
}

#[test]
fn lupin_agrees_on_the_missing_and_qualified_arguments() {
    let mut lupin = std::env::var_os("LUPIN")
        .map(PathBuf::from)
        .filter(|p| p.is_file());
    if lupin.is_none() {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                lupin = Some(candidate);
                break;
            }
        }
    }
    let Some(lupin) = lupin else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "the required lupin sibling is missing"
        );
        eprintln!("SKIP: no lupin sibling (set LUPIN); only the oracle leg is absent");
        return;
    };
    for (rel, verdict) in [
        ("rows/negative/ctor_arg_unresolved.lu", "fail(E0301)"),
        ("rows/ctor_arg_qualified/main.lu", "exit(0)"),
    ] {
        let entry = corpus(rel);
        let std = corpus("rows/ctor_arg_qualified/std");
        let out = Command::new(&lupin)
            .args(["conform-run", "--json"])
            .arg(&entry)
            .arg("--std-root")
            .arg(&std)
            .output()
            .expect("lupin runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rec: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("lupin JSON record");
        assert_eq!(rec["verdict"], verdict, "{rec}");
    }
}
