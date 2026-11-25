//! wolf-lang#1221 — the plain stores that keep their argument store as it is
//! every operand nothing else holds.
//!
//! `[mem.region.edge.elem]`: `push` and a pool's `init` copy a plain operand
//! in, so the caller keeps an independent value (#385, #1183). The copy is
//! needed only when something else can still reach the value. #1189 let
//! `push` keep a literal, a `copy` and a declared function's answer to
//! primitive arguments (`wolf_mem::forcopy::fresh_operand`). This row holds
//! the rest of that predicate's answer, and `init` reading it too:
//! - `init` keeps every operand `push` keeps;
//! - an element `get`, `first` or `last` answers is a copy of its own
//!   (#1200), and one `pop` answers has left the list;
//! - an `if`, `match`, block or `else` is fresh when every value it can
//!   produce is;
//! - an enum constructor moves its payloads in, as a struct literal does.
//!
//! The witness builds the same 64 values two ways in two regions and prints
//! whether the regions were charged alike; every compiling machine (checked,
//! native, release) must print `true` four times. lupin keeps its own ledger,
//! so it is held to the soundness rows only: `corpus/memory/store_fresh_only.lu`
//! on every machine and lupin, and `corpus/memory/init_fresh_only.lu` on the
//! compiling machines (lupin 0.1.62 does not index a pool by its handle).
//!
//! Why a driver test beside the corpus rows (s171's lesson): `cargo xtask
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

const ISSUE: &str = "wolf-lang#1221";

/// The charge witness: each pair of regions holds the same 64 values, made
/// once by the store under test and once moved in by `take`.
const WITNESS: &str = r#"
struct S {
    a: List[int],
    b: List[int],
}

enum E {
    Full(List[int]),
    Empty,
}

fn make(i: int) -> S {
    S { a: [i], b: [i] }
}

fn main() -> int {
    let n = 64
    let c = true
    let src = make(7)
    var ys = List[S]()
    (mut ys).push(make(1))

    // M1: a pool's init of a fresh operand, against init(h, take v)
    let init_moved = region()
    in init_moved {
        var p = Pool[S]()
        var i = 0
        while i < n {
            let h = (mut p).reserve()
            let v = make(i)
            (mut p).init(h, take v)
            i += 1
        }
    }
    let init_called = region()
    in init_called {
        var p = Pool[S]()
        var i = 0
        while i < n {
            let h = (mut p).reserve()
            (mut p).init(h, make(i))
            i += 1
        }
    }
    let init_built = region()
    in init_built {
        var p = Pool[S]()
        var i = 0
        while i < n {
            let h = (mut p).reserve()
            (mut p).init(h, S { a: [i], b: [i] })
            i += 1
        }
    }
    let init_copy_moved = region()
    in init_copy_moved {
        var p = Pool[S]()
        var i = 0
        while i < n {
            let h = (mut p).reserve()
            let v = copy src
            (mut p).init(h, take v)
            i += 1
        }
    }
    let init_copied = region()
    in init_copied {
        var p = Pool[S]()
        var i = 0
        while i < n {
            let h = (mut p).reserve()
            (mut p).init(h, copy src)
            i += 1
        }
    }

    // M2: a builtin answer of its own, against push(take answer)
    let first_moved = region()
    in first_moved {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            let v = ys.first() else make(0)
            (mut o).push(take v)
            i += 1
        }
    }
    let first_pushed = region()
    in first_pushed {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(ys.first() else make(0))
            i += 1
        }
    }
    let get_pushed = region()
    in get_pushed {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(ys.get(0) else make(0))
            i += 1
        }
    }
    let last_pushed = region()
    in last_pushed {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(ys.last() else make(0))
            i += 1
        }
    }
    let pop_moved = region()
    in pop_moved {
        var q = List[S]()
        var k = 0
        while k < n {
            (mut q).push(make(k))
            k += 1
        }
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            let v = (mut q).pop() else make(0)
            (mut o).push(take v)
            i += 1
        }
    }
    let pop_pushed = region()
    in pop_pushed {
        var q = List[S]()
        var k = 0
        while k < n {
            (mut q).push(make(k))
            k += 1
        }
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push((mut q).pop() else make(0))
            i += 1
        }
    }

    // M3: if, match and block over fresh arms, against push(take v)
    let made_moved = region()
    in made_moved {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            let v = make(i)
            (mut o).push(take v)
            i += 1
        }
    }
    let if_pushed = region()
    in if_pushed {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(if c { make(i) } else { S { a: [i], b: [i] } })
            i += 1
        }
    }
    let match_pushed = region()
    in match_pushed {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(match i % 2 {
                0 => make(i),
                _ => S { a: [i], b: [i] },
            })
            i += 1
        }
    }
    let block_pushed = region()
    in block_pushed {
        var o = List[S]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push({
                let k = i + 1
                make(k - 1)
            })
            i += 1
        }
    }

    // N6: an enum constructor, against push(take v)
    let ctor_moved = region()
    in ctor_moved {
        var o = List[E]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            let v = E.Full([i, i])
            (mut o).push(take v)
            i += 1
        }
    }
    let ctor_pushed = region()
    in ctor_pushed {
        var o = List[E]()
        (mut o).reserve(n)
        var i = 0
        while i < n {
            (mut o).push(E.Full([i, i]))
            i += 1
        }
    }

    let im = region_bytes(init_moved)
    let ic = region_bytes(init_called)
    let ib = region_bytes(init_built)
    let icm = region_bytes(init_copy_moved)
    let icp = region_bytes(init_copied)
    let fm = region_bytes(first_moved)
    let fp = region_bytes(first_pushed)
    let gp = region_bytes(get_pushed)
    let lp = region_bytes(last_pushed)
    let pm = region_bytes(pop_moved)
    let pp = region_bytes(pop_pushed)
    let mm = region_bytes(made_moved)
    let ifp = region_bytes(if_pushed)
    let mp = region_bytes(match_pushed)
    let bp = region_bytes(block_pushed)
    let cm = region_bytes(ctor_moved)
    let cp = region_bytes(ctor_pushed)
    let init_ok = ic == im && ib == im && icp == icm
    let answer_ok = fp == fm && gp == fm && lp == fm && pp == pm
    let arms_ok = ifp == mm && mp == mm && bp == mm
    let ctor_ok = cp == cm
    print("init {init_ok} answer {answer_ok} arms {arms_ok} ctor {ctor_ok}")
    print("bytes init_moved={im} init_called={ic} init_built={ib} init_copy_moved={icm} init_copied={icp} first_moved={fm} first={fp} get={gp} last={lp} pop_moved={pm} pop={pp} made_moved={mm} if={ifp} match={mp} block={bp} ctor_moved={cm} ctor={cp}")
    0
}
"#;

const WANT: &str = "init true answer true arms true ctor true";

/// Every compiling machine charges a stored fresh value what a moved one
/// charges: one allocation, never a second copy.
#[test]
fn a_stored_fresh_value_costs_what_a_moved_one_costs() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fresh_store_lanes");
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
            "{flag} ({ISSUE}): a stored fresh value was copied again: {}",
            obs.stdout
        );
    }
}

/// A corpus row's pinned stdout.
fn pinned(entry: &Path) -> String {
    assert!(entry.exists(), "corpus row missing: {}", entry.display());
    let src = std::fs::read_to_string(entry).expect("read the row");
    src.lines()
        .find_map(|l| l.strip_prefix("//! check: run(exit=0, stdout=\""))
        .and_then(|l| l.strip_suffix("\")"))
        .expect("the row pins its stdout")
        .replace("\\n", "\n")
}

/// The row on every compiling machine; `true` runs lupin too.
fn holds(entry: &Path, with_lupin: bool) {
    let want = pinned(entry);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "{flag} ({ISSUE}): {:?}", obs.codes);
        assert_eq!(obs.stdout, want, "{flag} ({ISSUE}) stdout");
    }
    if with_lupin && let Some(lupin) = lupin_says(entry) {
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

/// The soundness row for `push`: no write through a pushed element reaches
/// the value it came from, whether the push kept the operand or copied it,
/// on every machine and on lupin.
#[test]
fn a_pushed_value_that_may_reach_a_place_is_still_copied_and_a_fresh_one_is_its_own() {
    holds(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/memory/store_fresh_only.lu"),
        true,
    );
}

/// The soundness row for a pool's `init`, on the compiling machines.
#[test]
fn an_init_value_that_may_reach_a_place_is_still_copied_and_a_fresh_one_is_its_own() {
    holds(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/memory/init_fresh_only.lu"),
        false,
    );
}
