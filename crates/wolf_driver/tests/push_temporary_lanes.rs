//! wolf-lang#1177 (part 2) — a pushed value nobody else holds is stored as
//! it is.
//!
//! `[mem.region.edge.elem]`: a plain `push` copies its element into the
//! list, so the caller keeps an independent value (#385). The copy is only
//! needed when something else can still reach the value. A struct, tuple or
//! list literal, a `copy e` and a declared function's answer are held by
//! nothing else (`wolf_mem::forcopy::fresh_operand`), so a push of one may
//! store it as `push(take v)` stores a moved local: one allocation of the
//! value, not two. Before the change every machine copied them again, so
//! `push(make(i))` allocated each element twice and `push(copy v)` copied
//! twice.
//!
//! The witness builds the same 64 elements two ways in two regions and
//! prints whether the regions were charged alike; every compiling machine
//! (checked, native, release) must print `true` four times. lupin keeps its
//! own ledger, so it is held to the soundness row only
//! (`corpus/memory/push_fresh_only.lu`): no write through a pushed element
//! may reach the value it came from, on any machine.
//!
//! Why a driver test beside the corpus row (s171's lesson): `cargo xtask
//! corpus` runs every entry on the native lane only.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    codes: Vec<String>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        codes: rec["diagnostics"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| d["code"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
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
/// arranged here": there an absent sibling is a failed fetch.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of {ISSUE}'s soundness row did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN): the oracle leg is absent");
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

const ISSUE: &str = "wolf-lang#1177";

/// The charge witness: each pair of regions holds the same 64 elements,
/// made once by the push under test and once moved in by `take`.
const WITNESS: &str = r#"
struct S {
    a: List[int],
    b: List[int],
    c: List[int],
    d: List[int],
}

fn make(i: int) -> S {
    S { a: [i], b: [i], c: [i], d: [i] }
}

fn main() -> int {
    let n = 64
    let src = make(7)
    let moved = region()
    in moved {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            let v = make(i)
            (mut o).push(take v)
            i += 1
        }
    }
    let called = region()
    in called {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(make(i))
            i += 1
        }
    }
    let built = region()
    in built {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(S { a: [i], b: [i], c: [i], d: [i] })
            i += 1
        }
    }
    let lists_moved = region()
    in lists_moved {
        var o = List[List[int]]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            let v = [i, i]
            (mut o).push(take v)
            i += 1
        }
    }
    let lists_built = region()
    in lists_built {
        var o = List[List[int]]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push([i, i])
            i += 1
        }
    }
    let copies_moved = region()
    in copies_moved {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            let c = copy src
            (mut o).push(take c)
            i += 1
        }
    }
    let copies_pushed = region()
    in copies_pushed {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(copy src)
            i += 1
        }
    }
    let m = region_bytes(moved)
    let c = region_bytes(called)
    let b = region_bytes(built)
    let lm = region_bytes(lists_moved)
    let lb = region_bytes(lists_built)
    let cm = region_bytes(copies_moved)
    let cp = region_bytes(copies_pushed)
    let call_ok = c == m
    let struct_ok = b == m
    let list_ok = lb == lm
    let copy_ok = cp == cm
    print("call {call_ok} struct {struct_ok} list {list_ok} copy {copy_ok}")
    print("bytes moved={m} called={c} built={b} lists_moved={lm} lists_built={lb} copies_moved={cm} copies_pushed={cp}")
    0
}
"#;

const WANT: &str = "call true struct true list true copy true";

/// Every compiling machine charges a pushed fresh value what a moved one
/// charges: one allocation, never a second copy.
#[test]
fn a_pushed_fresh_value_costs_what_a_moved_one_costs() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("push_temporary_lanes");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, WITNESS).expect("write witness");
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "{flag} ({ISSUE}): {:?}", obs.codes);
        let first = obs.stdout.lines().next().unwrap_or("");
        assert_eq!(
            first, WANT,
            "{flag} ({ISSUE}): a pushed fresh value was copied again: {}",
            obs.stdout
        );
    }
}

/// The soundness row: an operand that may still reach a place is copied,
/// and no write through a pushed element reaches the value it came from,
/// on every machine and on lupin.
#[test]
fn a_pushed_value_that_may_reach_a_place_is_still_copied() {
    let entry =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/memory/push_fresh_only.lu");
    assert!(entry.exists(), "corpus row missing: {}", entry.display());
    let src = std::fs::read_to_string(&entry).expect("read the row");
    let want = src
        .lines()
        .find_map(|l| l.strip_prefix("//! check: run(exit=0, stdout=\""))
        .and_then(|l| l.strip_suffix("\")"))
        .expect("the row pins its stdout")
        .replace("\\n", "\n");
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "{flag} ({ISSUE}): {:?}", obs.codes);
        assert_eq!(obs.stdout, want, "{flag} ({ISSUE}) stdout");
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(
            lupin.verdict, "exit(0)",
            "lupin {} ({ISSUE})",
            lupin.version
        );
        assert_eq!(
            lupin.stdout, want,
            "lupin {} ({ISSUE}) stdout",
            lupin.version
        );
    }
}
