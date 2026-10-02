//! s190 (wolf-lang#486) — the struct field shorthand is the longhand.
//! `[gram.expr]`: `Point { x }` binds the field from the identifier, so
//! `W { xs }` is `W { xs: xs }`, and `[mem.tier0.move.1]` moves a
//! non-`Copy` value on initialization. The shorthand therefore moves
//! `xs`, and a later use is E1001, on every machine.
//!
//! Before s190 the shorthand carried a bare identifier token and no
//! value node, so every pass that reads a field's value read nothing:
//! the move checker saw no move (wolf 0.2.19 printed `2 2` on native
//! and release, two names on one buffer), the checked machine dropped
//! the field (`unsupported` at the first field read), and a shorthand
//! inside a closure or a task was never seen as a capture
//! (`unsupported` at lowering, no E1002 for a borrow it takes).
//!
//! Every case runs the corpus row AND its longhand twin (the row's text
//! with each shorthand spelled out) on the checked, native and release
//! lanes and on lupin, and asserts: the twins agree on every lane
//! (verdict, diagnostic codes, stdout), and the wolfgang lanes give the
//! ruled answer. Equality alone would pass when both spellings are
//! wrong together (s171's lesson, wave 45); the ruled answer alone
//! would not say the spellings agree. lupin 0.1.42 copies through the
//! shorthand (wolffe-lang/wolf-interp#159): its measured answers are
//! pinned by version below as pre-mirror (s180's design), so a newer
//! lupin that still differs from its own longhand reds by name. The
//! pins were dropped at the 0.1.43 pairing (r25): 0.1.43 carries is62's
//! #159 mirror and gives every row its longhand's answer.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug, PartialEq, Eq)]
struct Obs {
    verdict: String,
    codes: Vec<String>,
    stdout: String,
    version: String,
}

impl Obs {
    /// Everything but the implementation's own version.
    fn answer(&self) -> (&str, &[String], &str) {
        (&self.verdict, &self.codes, &self.stdout)
    }
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    let codes = rec["diagnostics"]
        .as_array()
        .map(|ds| {
            ds.iter()
                .filter_map(|d| d["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        codes,
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
    }
}

/// One wolfgang lane. `None` means the host cannot run the native lane
/// (the s59 skip pattern), never a silent pass.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, "the observation"))
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

/// lupin's observation, or `None` when this box has no sibling.
/// `WOLF_PAIRING_REQUIRE_SIBLING` is the linux CI job saying "one was
/// arranged here" (r10/#253): there an absent sibling is a failed
/// fetch, not a skip.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of #486's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the ruled answer and their longhand twins; \
             only the oracle leg is absent"
        );
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

/// The corpus row itself is the program: one truth per file.
fn corpus(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(name);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// The row's longhand twin: each `(shorthand, longhand)` spelling
/// replaced in the program text (the `//!` header is prose and is left
/// alone), each exactly once, written in a directory of its own (a
/// directory is a module).
fn longhand_twin(row: &Path, spellings: &[(&str, &str)]) -> PathBuf {
    let src = std::fs::read_to_string(row).expect("row reads");
    let (header, mut code): (Vec<&str>, Vec<String>) = {
        let n = src.lines().take_while(|l| l.starts_with("//!")).count();
        let lines: Vec<&str> = src.lines().collect();
        (
            lines[..n].to_vec(),
            lines[n..].iter().map(|l| l.to_string()).collect(),
        )
    };
    for (short, long) in spellings {
        let hits: usize = code.iter().map(|l| l.matches(short).count()).sum();
        assert_eq!(
            hits,
            1,
            "`{short}` appears exactly once in the program text of {}",
            row.display()
        );
        for l in &mut code {
            *l = l.replace(short, long);
        }
    }
    let mut text = header.join("\n");
    text.push('\n');
    text.push_str(&code.join("\n"));
    text.push('\n');
    let stem = row.file_stem().unwrap().to_string_lossy().to_string();
    let dir = std::env::temp_dir().join(format!("wolf-s190-{}-{stem}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("twin dir");
    let twin = dir.join("main.lu");
    std::fs::write(&twin, text).expect("twin writes");
    twin
}

/// What the wolfgang lanes must answer.
enum Ruled {
    /// `exit(0)` with this stdout.
    Runs(&'static str),
    /// A refusal with exactly these diagnostic codes (warnings included,
    /// in record order).
    Refused(&'static str, &'static [&'static str]),
}

/// The row and its longhand twin agree on every lane; the wolfgang
/// lanes give the ruled answer; lupin agrees with its own longhand, or
/// — for a version named in `lupin_pre_mirror` — gives the measured
/// `(verdict, stdout)` of wolf-interp#159. Every lane is observed before
/// the case fails, and the failure lists each lane's miss with what it
/// answered, so one red names every machine that is wrong.
fn shorthand_is_longhand(
    name: &str,
    spellings: &[(&str, &str)],
    ruled: Ruled,
    lupin_pre_mirror: &[(&str, (&str, &str))],
) {
    let row = corpus(name);
    let twin = longhand_twin(&row, spellings);
    let mut misses: Vec<String> = Vec::new();
    for flag in ["--checked", "--native", "--release"] {
        let (Some(short), Some(long)) = (lane(&row, flag), lane(&twin, flag)) else {
            continue;
        };
        let ruled_ok = match &ruled {
            Ruled::Runs(want) => short.verdict == "exit(0)" && short.stdout == *want,
            Ruled::Refused(verdict, codes) => short.verdict == *verdict && short.codes == *codes,
        };
        if !ruled_ok {
            misses.push(format!(
                "the {flag} lane misses the ruled answer (wolf-lang#486): \
                 answered {short:?}"
            ));
        }
        if short.answer() != long.answer() {
            misses.push(format!(
                "the {flag} lane answers the shorthand and its longhand twin \
                 differently: shorthand {short:?}, longhand {long:?}"
            ));
        }
    }
    if let (Some(short), Some(long)) = (lupin_says(&row), lupin_says(&twin)) {
        match lupin_pre_mirror.iter().find(|(v, _)| *v == short.version) {
            Some((_, (verdict, stdout))) => {
                if (short.verdict.as_str(), short.stdout.as_str()) != (*verdict, *stdout) {
                    misses.push(format!(
                        "lupin {} misses its pinned pre-mirror answer \
                         (wolf-interp#159): answered {short:?}",
                        short.version
                    ));
                }
            }
            None => {
                if short.answer() != long.answer() {
                    misses.push(format!(
                        "lupin {} answers the shorthand and its longhand twin \
                         differently (wolf-interp#159 — a new lupin without the \
                         mirror pins its answer by version): shorthand {short:?}, \
                         longhand {long:?}",
                        short.version
                    ));
                }
            }
        }
    }
    assert!(misses.is_empty(), "{name}:\n  {}", misses.join("\n  "));
}

const E1001: Ruled = Ruled::Refused("fail(E1001)", &["E1001"]);

/// The issue's shape. Red at trunk: `2 2` on native and release,
/// `unsupported` on checked.
#[test]
fn the_shorthand_moves_its_local() {
    shorthand_is_longhand(
        "field_shorthand_moves.lu",
        &[("W { xs }", "W { xs: xs }")],
        E1001,
        &[],
    );
}

/// `Copy` fields (`int`, `str`) copy and run. Red at trunk on checked
/// (`unsupported`: the fields were dropped).
#[test]
fn a_copy_field_through_the_shorthand_stays_a_copy() {
    shorthand_is_longhand(
        "field_shorthand_copy.lu",
        &[("P { n, s }", "P { n: n, s: s }")],
        Ruled::Runs("3 hi 3 hi\n"),
        &[],
    );
}

/// Shorthand and longhand in one literal. Red at trunk: `2 2 2 1` on
/// native and release.
#[test]
fn a_mixed_literal_moves_through_both_spellings() {
    shorthand_is_longhand(
        "field_shorthand_mixed.lu",
        &[("M { xs, n, ys: ys }", "M { xs: xs, n: n, ys: ys }")],
        E1001,
        &[],
    );
}

/// The mixed literal with no later use runs. Red at trunk on checked.
#[test]
fn a_mixed_literal_without_a_later_use_runs() {
    shorthand_is_longhand(
        "field_shorthand_mixed_runs.lu",
        &[("M { xs, n, ys: ys }", "M { xs: xs, n: n, ys: ys }")],
        Ruled::Runs("2 2 3 2\n"),
        &[],
    );
}

/// A shorthand inside a nested literal. Red at trunk: `2 2 5`.
#[test]
fn a_nested_literal_moves_through_the_inner_shorthand() {
    shorthand_is_longhand(
        "field_shorthand_nested.lu",
        &[("O { w: W { xs }, n }", "O { w: W { xs: xs }, n: n }")],
        E1001,
        &[],
    );
}

/// A shorthand whose local is a struct built by a shorthand. Red at
/// trunk: `2 2 5`.
#[test]
fn a_struct_local_moves_through_the_outer_shorthand() {
    shorthand_is_longhand(
        "field_shorthand_nested_bound.lu",
        &[
            ("W { xs }", "W { xs: xs }"),
            ("O { w, n }", "O { w: w, n: n }"),
        ],
        E1001,
        &[],
    );
}

/// The nested literal with no later use runs. Red at trunk on checked.
#[test]
fn a_nested_literal_without_a_later_use_runs() {
    shorthand_is_longhand(
        "field_shorthand_nested_runs.lu",
        &[("O { w: W { xs }, n }", "O { w: W { xs: xs }, n: n }")],
        Ruled::Runs("3 5\n"),
        &[],
    );
}

/// `return W { xs }` moves a `mut` parameter out (s184). Red at trunk:
/// W1002 and `2 2` on native and release.
#[test]
fn a_returned_shorthand_moves_a_mut_parameter_out() {
    shorthand_is_longhand(
        "field_shorthand_return_mut.lu",
        &[("W { xs }", "W { xs: xs }")],
        E1001,
        &[],
    );
}

/// `return W { xs }` of a `take` parameter runs. Red at trunk on
/// checked.
#[test]
fn a_returned_shorthand_of_a_take_parameter_runs() {
    shorthand_is_longhand(
        "field_shorthand_return_take.lu",
        &[("W { xs }", "W { xs: xs }")],
        Ruled::Runs("3\n"),
        &[],
    );
}

/// Inside a closure. Red at trunk: `unsupported` ("this field-init
/// shorthand") on native and release.
#[test]
fn a_shorthand_inside_a_closure_moves_the_capture() {
    shorthand_is_longhand(
        "field_shorthand_closure.lu",
        &[("W { xs }", "W { xs: xs }")],
        Ruled::Refused("fail(E1001)", &["E1001", "E1001"]),
        &[],
    );
}

/// Inside a task. Red at trunk: `unsupported` on native and release.
#[test]
fn a_shorthand_inside_a_task_moves_the_capture() {
    shorthand_is_longhand(
        "field_shorthand_task.lu",
        &[("W { xs }", "W { xs: xs }")],
        Ruled::Refused("fail(E1001)", &["E1001", "E1001"]),
        &[],
    );
}

/// A closure capturing through the shorthand borrows like the longhand
/// (E1002 with W1102). Red at trunk: no capture seen, `unsupported`.
/// lupin 0.1.42 runs the shorthand (`1 2`) and traps the longhand
/// (`exclusivity`).
#[test]
fn a_shorthand_capture_is_a_borrow() {
    shorthand_is_longhand(
        "field_shorthand_closure_borrow.lu",
        &[("P { n }", "P { n: n }")],
        Ruled::Refused("fail(E1002)", &["E1002", "W1102"]),
        &[],
    );
}
