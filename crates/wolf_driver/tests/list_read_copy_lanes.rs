//! wolf-lang#1184 — `get`, `first` and `last` answer a value of their own,
//! on every machine.
//!
//! `[mem.list.pop]`: `xs.get(i)` answers the element at `i`, and
//! `xs.first()` and `xs.last()` are `get(0)` and `get(xs.len - 1)`. Like
//! every value (`[mem.model.value]`), the answer is independent of the
//! list. Natively and on the checked machine the answer was the element
//! itself: its list's own header word, or the same list's handle. So a
//! write through the answer changed the list, and a later write to the list
//! changed the answer. Now an element that reaches heap storage is copied
//! out, as `push` copies one in.
//!
//! Two gates:
//! - the corpus row's header, on the checked machine, native, release and
//!   lupin;
//! - the copy's cost: in a region, a `get`, `first` or `last` of a
//!   `List[List[int]]` element charges what `copy` of that element charges,
//!   and a `get` of an `int` element charges nothing. Each machine's numbers
//!   are printed. lupin keeps its own ledger, so it answers to the row
//!   only.
//!
//! Why a driver test beside the corpus row: `cargo xtask corpus` runs every
//! entry on the native lane only.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The corpus rows.
const ROWS: [&str; 1] = ["list_read_answers_own_value"];

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

struct Obs {
    verdict: String,
    stdout: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    }
}

/// One machine's observation of `entry`; `None` where the host cannot run
/// a compiled tier (a loud skip, refused under `WOLF_TEST_NATIVE_REQUIRE`).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required tier {flag} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
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

/// The sibling lupin, found as the other lane gates find it: `LUPIN`
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

fn lupin(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN); the machines still answer the header"
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

/// The `stdout="…"` of a row's `run(exit=0, …)` header, unescaped.
fn expected(src: &str) -> String {
    let line = src
        .lines()
        .find(|l| l.starts_with("//! check: run(exit=0, stdout=\""))
        .expect("a run(exit=0, stdout=…) header");
    let body = line
        .trim_start_matches("//! check: run(exit=0, stdout=\"")
        .trim_end_matches("\")");
    body.replace("\\n", "\n").replace("\\\"", "\"")
}

fn corpus(row: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(format!("{row}.lu"))
}

#[test]
fn every_row_answers_its_header_on_every_machine() {
    let mut wrong = Vec::new();
    for row in ROWS {
        let entry = corpus(row);
        let src = std::fs::read_to_string(&entry).expect("the row exists");
        let want = expected(&src);
        for m in MACHINES {
            let Some(o) = lane(&entry, m) else {
                continue;
            };
            if o.verdict != "exit(0)" || o.stdout != want {
                wrong.push(format!(
                    "{row} {m}: {} {:?}, the header {:?}",
                    o.verdict, o.stdout, want
                ));
            }
        }
        if let Some(o) = lupin(&entry)
            && (o.verdict != "exit(0)" || o.stdout != want)
        {
            wrong.push(format!(
                "{row} lupin: {} {:?}, the header {:?}",
                o.verdict, o.stdout, want
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The cost witness. Each read runs in a region of its own, and the region's
/// ledger is read after it: `get`, `first` and `last` of a 4-int list
/// element, `copy` of the same element, and `get` of an `int`.
const WITNESS: &str = r#"
fn main() -> int {
    let rows = [[1, 2, 3, 4], [5, 6, 7, 8]]
    let ints = [1, 2, 3, 4]
    var seen = 0
    let got = region()
    in got {
        let a = rows.get(1) else [0]
        seen += a.len
    }
    let firsts = region()
    in firsts {
        let a = rows.first() else [0]
        seen += a.len
    }
    let lasts = region()
    in lasts {
        let a = rows.last() else [0]
        seen += a.len
    }
    let copied = region()
    in copied {
        let a = copy rows[1]
        seen += a.len
    }
    let scalar = region()
    in scalar {
        let n = ints.get(1) else 0
        seen += n
    }
    let g = region_bytes(got)
    let f = region_bytes(firsts)
    let l = region_bytes(lasts)
    let c = region_bytes(copied)
    let s = region_bytes(scalar)
    print("get={g} first={f} last={l} copy={c} scalar={s} seen={seen}")
    0
}
"#;

fn field(line: &str, key: &str) -> i64 {
    line.split(' ')
        .find_map(|w| w.strip_prefix(&format!("{key}=")))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("`{key}=` in {line:?}"))
}

/// A heap element read by `get`, `first` or `last` charges its copy, which
/// is what `copy` of it charges, and an `int` read charges nothing.
#[test]
fn a_read_of_a_heap_element_charges_its_copy() {
    let dir = std::env::temp_dir().join(format!("wolf-list-read-cost-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, WITNESS).expect("write witness");
    let mut wrong = Vec::new();
    for m in MACHINES {
        let Some(o) = lane(&entry, m) else {
            continue;
        };
        let line = o.stdout.trim().to_string();
        eprintln!("COST {m}: {line}");
        if o.verdict != "exit(0)" {
            wrong.push(format!("{m}: {} {line:?}", o.verdict));
            continue;
        }
        let (g, f, l, c, s) = (
            field(&line, "get"),
            field(&line, "first"),
            field(&line, "last"),
            field(&line, "copy"),
            field(&line, "scalar"),
        );
        if c <= 0 || g != c || f != c || l != c || s != 0 || field(&line, "seen") != 18 {
            wrong.push(format!(
                "{m}: {line:?}: every read of the list element must charge what its copy \
                 charges, and the int read nothing"
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
