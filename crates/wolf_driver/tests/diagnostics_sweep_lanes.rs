//! s289: the diagnostics sweep's first change, each row on the checked
//! machine, native and release, and on the pinned lupin as the fourth
//! machine (s285's and s287's shape).
//!
//! - wolf-lang#1085: a capitalized name that nothing declares is read as an
//!   error tag. Where no error row takes it, every compiled tier declined
//!   the program as "an error-row tag outside `!T` context", which blamed
//!   the compiler for the program's mistake. `[gram.expr.tagident]` and
//!   `[gram.expr.variant]` already say it is E0301; a tag the function
//!   does spell, in a function that cannot fail, is E0604.
//! - wolf-lang#1083: a module named like one of the prelude's provisional
//!   stand-ins (`worker`) and not imported resolved to the stand-in, and
//!   the checker declined it as "a std/prelude stub without a signature".
//!   It misses as every unimported module does, with the import named.
//!
//! Every rule here is a front-end refusal, so one fix covers the three
//! compiled tiers. lupin runs the #1085 rows (it reads the name as a tag
//! value), so its mirror is a wolf-interp change; until the pin moves,
//! lupin is held to the code and spans whenever its record carries the
//! code, and otherwise to the answer the pinned release gives today, which
//! each test names.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn entry(row: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row)
}

fn record_of(out: &Output, row: &str, machine: &str) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{machine} {row}: {}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{machine} {row}: {e}: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// The compiled tiers' records and rendered stderr for `row`; a tier the
/// host cannot run is a loud SKIP, refused when `WOLF_TEST_NATIVE_REQUIRE`
/// is set.
fn compiled(row: &str) -> Vec<(&'static str, serde_json::Value, String)> {
    let mut runs = Vec::new();
    for machine in ["--checked", "--native", "--release"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["conform-run", "--json", machine])
            .arg(entry(row))
            .output()
            .expect("run wolf");
        if machine != "--checked" && lane_exit::environment_refusal(&out, machine) {
            assert!(
                std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
                "required {machine} unavailable: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            eprintln!(
                "SKIP: {machine} {row}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            continue;
        }
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        runs.push((machine, record_of(&out, row, machine), stderr));
    }
    runs
}

fn lupin_binary() -> Option<PathBuf> {
    let lupin = std::env::var_os("LUPIN").map(PathBuf::from).or_else(|| {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    });
    if lupin.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "required lupin missing"
        );
    }
    lupin
}

fn lupin(row: &str) -> Option<serde_json::Value> {
    let Some(lupin) = lupin_binary() else {
        eprintln!("SKIP: lupin {row}: set LUPIN to run the fourth machine");
        return None;
    };
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(entry(row))
        .output()
        .expect("run lupin");
    Some(record_of(&out, row, "lupin"))
}

/// The spans of every error diagnostic in `record`, in order, with its code.
fn errors(record: &serde_json::Value) -> Vec<(String, (u64, u64))> {
    let mut found: Vec<(String, (u64, u64))> = record["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["severity"] != "warning")
        .map(|d| {
            (
                d["code"].as_str().unwrap_or("?").to_string(),
                (
                    d["span"][0].as_u64().unwrap_or(u64::MAX),
                    d["span"][1].as_u64().unwrap_or(u64::MAX),
                ),
            )
        })
        .collect();
    found.sort_by_key(|(_, s)| *s);
    found
}

/// What a row's rendering must say, and what it must not.
struct Says<'a> {
    has: &'a [&'a str],
    lacks: &'a [&'a str],
}

/// Every compiled tier refuses `row` by `code` at exactly `span` and at
/// nothing else, its rendering says each of `says.has` and none of
/// `says.lacks`, and no tier calls it unsupported. lupin carries `code` at
/// the same span, or (the pinned release) answers `lupin_else`.
fn refused(row: &str, code: &str, span: (u64, u64), says: Says<'_>, lupin_else: &str) {
    let want = vec![(code.to_string(), span)];
    for (machine, record, stderr) in compiled(row) {
        assert_eq!(
            record["verdict"],
            format!("fail({code})"),
            "{machine} {row}: {record}\n{stderr}"
        );
        assert_eq!(errors(&record), want, "{machine} {row}: spans: {record}");
        for text in says.has {
            assert!(
                stderr.contains(text),
                "{machine} {row}: the rendering lacks {text:?}:\n{stderr}"
            );
        }
        for text in says.lacks {
            assert!(
                !stderr.contains(text),
                "{machine} {row}: the rendering says {text:?}:\n{stderr}"
            );
        }
    }
    let Some(record) = lupin(row) else {
        return;
    };
    if record["verdict"] == format!("fail({code})") {
        assert_eq!(errors(&record), want, "lupin {row}: spans: {record}");
    } else {
        assert_eq!(record["verdict"], lupin_else, "lupin {row}: {record}");
    }
}

/// The words the compiler used to blame itself with, which no row of this
/// file may render.
const DECLINES: &[&str] = &[
    "an error-row tag outside `!T` context",
    "a std/prelude stub without a signature",
    "cannot compile this yet",
];

// ------------------------- wolf-lang#1085, an undeclared capitalized name --

/// The tail of a function that returns `int`: E0301 at `OPEN`, with the
/// tag reading explained and both fixes, `-> !int` among them, since a
/// tail is where a raise would go. lupin 0.1.60 exits 1 (`error: OPEN`).
#[test]
fn an_undeclared_name_as_a_tail_is_e0301() {
    refused(
        "typecheck/tag_undeclared_tail.lu",
        "E0301",
        (712, 716),
        Says {
            has: &[
                "error[E0301]: nothing named `OPEN` is in scope",
                "is read as an error tag",
                "`answer` cannot fail",
                "`-> !int`",
            ],
            lacks: DECLINES,
        },
        "exit(1)",
    );
}

/// Assigned to a field: no raise position, so no `!T` advice at all.
#[test]
fn an_undeclared_name_assigned_is_e0301_without_the_fallible_advice() {
    refused(
        "typecheck/tag_undeclared_assign.lu",
        "E0301",
        (568, 573),
        Says {
            has: &[
                "error[E0301]: nothing named `INPUT` is in scope",
                "is read as an error tag",
            ],
            lacks: &[DECLINES[0], DECLINES[1], DECLINES[2], "`-> !"],
        },
        "exit(1)",
    );
}

/// The operand of `return` in a function that cannot fail: as the tail.
#[test]
fn an_undeclared_name_returned_is_e0301() {
    refused(
        "typecheck/tag_undeclared_return.lu",
        "E0301",
        (543, 549),
        Says {
            has: &[
                "error[E0301]: nothing named `CANCEL` is in scope",
                "`answer` cannot fail",
                "`-> !int`",
            ],
            lacks: DECLINES,
        },
        "exit(1)",
    );
}

/// An unannotated `let`: E0301, and no `!T` advice.
#[test]
fn an_undeclared_name_in_a_let_is_e0301() {
    refused(
        "typecheck/tag_undeclared_let.lu",
        "E0301",
        (483, 487),
        Says {
            has: &["error[E0301]: nothing named `OPEN` is in scope"],
            lacks: &[DECLINES[0], DECLINES[1], DECLINES[2], "`-> !"],
        },
        "exit(1)",
    );
}

/// The call form, `Bad(3)`: E0301 at the name, not the call.
#[test]
fn an_undeclared_name_called_with_a_payload_is_e0301() {
    refused(
        "typecheck/tag_undeclared_payload.lu",
        "E0301",
        (515, 518),
        Says {
            has: &[
                "error[E0301]: nothing named `Bad` is in scope",
                "`answer` cannot fail",
            ],
            lacks: DECLINES,
        },
        "exit(1)",
    );
}

/// Inside a function that can fail, a `let` is still no raise position:
/// E0301, and the note says how a tag is raised there.
#[test]
fn an_undeclared_name_in_a_fallible_functions_let_is_e0301() {
    refused(
        "typecheck/tag_undeclared_fallible_let.lu",
        "E0301",
        (495, 498),
        Says {
            has: &[
                "error[E0301]: nothing named `Bad` is in scope",
                "`return Bad`",
            ],
            lacks: &[DECLINES[0], DECLINES[1], DECLINES[2], "cannot fail"],
        },
        "exit(1)",
    );
}

/// The file imports a module that defines `OPEN`: the qualified spelling
/// is the fix, offered as the edit.
#[test]
fn a_bare_name_an_imported_module_defines_offers_the_qualified_spelling() {
    refused(
        "typecheck/tag_undeclared_module_item/main.lu",
        "E0301",
        (578, 582),
        Says {
            has: &[
                "error[E0301]: nothing named `OPEN` is in scope",
                "help: write `dialog.OPEN`",
            ],
            lacks: DECLINES,
        },
        "exit(0)",
    );
}

/// A tag the function spells, returned where it cannot fail: E0604, with
/// `?`'s fix. lupin 0.1.60 declines it as unresolved.
#[test]
fn the_functions_own_tag_where_it_cannot_fail_is_e0604() {
    refused(
        "typecheck/tag_known_cannot_fail.lu",
        "E0604",
        (672, 675),
        Says {
            has: &[
                "error[E0604]: `bad` is an error tag, and `answer` cannot fail",
                "! {bad}",
            ],
            lacks: DECLINES,
        },
        "unsupported",
    );
}

/// `wolf test` over a module that forgot a constant: the file is
/// rejected with the E0301, never "unsupported".
#[test]
fn wolf_test_rejects_a_module_that_forgot_a_constant() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/diagnostics_sweep/forgot_a_constant");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s289_forgot_a_constant");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("dialog")).expect("mkdir");
    for f in ["dialog_test.lu", "dialog/dialog.lu"] {
        std::fs::copy(src.join(f), dir.join(f)).expect("copy fixture");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("test")
        .arg("dialog_test.lu")
        .current_dir(&dir)
        .output()
        .expect("run wolf test");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("1 rejected"), "{text}");
    assert!(text.contains("0 unsupported"), "{text}");
    assert!(
        text.contains("nothing named `OPEN` is in scope"),
        "the E0301 is shown:\n{text}"
    );
    for decline in DECLINES {
        assert!(!text.contains(decline), "{text}");
    }
}

// --------------------------- wolf-lang#1083, a module named like a stub --

/// #1083's witness: `worker/` is a module of the package, unimported.
/// E0301 at resolve, the import named and offered. lupin 0.1.60 declines
/// the call as unresolved.
#[test]
fn an_unimported_module_named_like_a_stand_in_is_e0301_with_its_use() {
    refused(
        "resolve/stand_in_unimported/main.lu",
        "E0301",
        (807, 813),
        Says {
            has: &[
                "error[E0301]: nothing named `worker` is in scope",
                "`worker` is a module of this package, and this file does not import it",
                "use worker",
            ],
            lacks: DECLINES,
        },
        "unsupported",
    );
    for (machine, record, _) in compiled("resolve/stand_in_unimported/main.lu") {
        assert_eq!(record["phase_reached"], "resolve", "{machine}: {record}");
    }
}

/// The same program with the module named `helper`: already E0301, now
/// with the same note and edit.
#[test]
fn an_unimported_module_is_e0301_with_its_use() {
    refused(
        "resolve/module_unimported/main.lu",
        "E0301",
        (537, 543),
        Says {
            has: &[
                "error[E0301]: nothing named `helper` is in scope",
                "`helper` is a module of this package, and this file does not import it",
                "use helper",
            ],
            lacks: DECLINES,
        },
        "unsupported",
    );
}

/// A stand-in nothing defines, called: E0301 with the stand-in named as
/// such. lupin 0.1.60 declines it ("no pinned semantics").
#[test]
fn an_undefined_stand_in_called_is_e0301() {
    refused(
        "typecheck/stand_in_undefined_call.lu",
        "E0301",
        (643, 649),
        Says {
            has: &["error[E0301]: nothing defines `worker`", "provisional"],
            lacks: DECLINES,
        },
        "unsupported",
    );
}

/// The same as a receiver, with no module of the name anywhere.
#[test]
fn an_undefined_stand_in_as_a_receiver_is_e0301() {
    refused(
        "typecheck/stand_in_undefined_receiver.lu",
        "E0301",
        (449, 455),
        Says {
            has: &["error[E0301]: nothing defines `worker`"],
            lacks: DECLINES,
        },
        "unsupported",
    );
}

// --------------------------------------------------------------- controls --

/// A bare variant value keeps its E0301 and its `Ordering.` edit.
#[test]
fn a_bare_variant_keeps_its_e0301() {
    for (machine, record, stderr) in compiled("typecheck/variant_bare_value.lu") {
        assert_eq!(record["verdict"], "fail(E0301)", "{machine}: {record}");
        assert!(
            stderr.contains("write `Ordering.Less`"),
            "{machine}:\n{stderr}"
        );
    }
}

/// `[type.unit.discard]`'s recorded decline keeps its words: the tag is
/// the fallible function's own, in a unit position.
#[test]
fn the_unit_discard_decline_keeps_its_words() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unit_tail_if/bare_tag.lu");
    let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .args(["conform-run", "--json", "--checked"])
        .arg(&fixture)
        .output()
        .expect("run wolf");
    let record = record_of(&out, "unit_tail_if/bare_tag.lu", "--checked");
    assert_eq!(record["verdict"], "unsupported", "{record}");
    assert_eq!(
        record["x-unsupported-construct"], "an error-row tag outside `!T` context",
        "{record}"
    );
}
