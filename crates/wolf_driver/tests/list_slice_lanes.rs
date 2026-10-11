//! s228 (wolf-lang#663) — a `List` slice is one copy, and it is the
//! same list on every machine.
//!
//! `xs[a..b]` used to lower to a loop of `__wolf_rt_list_push` calls,
//! one per element, into a list that grew by doubling: 1.9 ns a byte
//! (7.8 µs for a 4 KiB `List[byte]` slice, where a decode of the same
//! bytes took 0.13 µs), and every abandoned buffer stayed charged to
//! the region. It is now one runtime call — a bounds check the caller
//! already made, one allocation at exact capacity, one copy
//! (`wolf_rt::list::__wolf_rt_list_slice`) — for every element type,
//! since every element type a slice is lowered for is flat.
//!
//! What a program observes must not move, and one thing must start to
//! agree:
//!
//! - five element types (`byte`, `int`, `str`, a struct, a tuple),
//!   the open and end-relative forms, the empty slice, and a slice's
//!   independence from its source in both directions — the same lines
//!   on native, release, checked and lupin, at trunk and here;
//! - one 4,096-byte slice inside `region r(cap: 6000)`. The push loop
//!   charged the region 8,240 bytes (the doubling buffers), so the
//!   native tiers answered `trap(alloc-contract)` where the checked
//!   machine (4,096 charged) and lupin ran it. One copy charges 4,144,
//!   and all four machines print `budget 4375`. This is the row that
//!   is red at trunk.

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
    unsupported: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!(
            "{what} record parses: {e}; stderr: {}",
            String::from_utf8_lossy(stderr)
        )
    });
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        unsupported: rec["x-unsupported-construct"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned()),
    }
}

/// One wolfgang machine. `None`: the host cannot run the lane (the s59
/// skip pattern, printed), never a silent pass.
fn lane(entry: &Path, flags: &[&str]) -> Option<Obs> {
    let what = format!("wolf {}", flags.join(" "));
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .args(flags)
        .arg("--json")
        // The program's own directory: lupin serves the fs tier inside
        // its working directory only (`[os.fs.path.domain]`), so every
        // machine runs where the program's data file is.
        .current_dir(entry.parent().expect("an entry has a directory"))
        .output()
        .expect("wolf runs");
    if flags != ["--checked"] && lane_exit::environment_refusal(&out, &what) {
        eprintln!(
            "SKIP: environment cannot run the {} lane: {}",
            flags.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    Some(parse_obs(&out.stdout, &out.stderr, &what))
}

/// The sibling lupin, found as `pairing.rs` finds it.
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

fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of the slice lanes did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the compiled machines are \
             still held to the expected answer; only the oracle leg is absent"
        );
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(entry.parent().expect("an entry has a directory"))
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, &out.stderr, "lupin"))
}

/// Write `src` as `main.lu` in a fresh directory of its own.
fn program(name: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("s228-slice-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p = dir.join("main.lu");
    std::fs::write(&p, src).expect("write program");
    p
}

fn every_machine(name: &str, src: &str, want: &str) {
    let entry = program(name, src);
    for flags in [&["--native"][..], &["--native", "--release"][..]] {
        let Some(obs) = lane(&entry, flags) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "{name}: wolf {} ({})",
            flags.join(" "),
            obs.version
        );
    }
    let obs = lane(&entry, &["--checked"]).expect("the checked machine always answers");
    assert_eq!(
        (obs.verdict.as_str(), obs.stdout.as_str()),
        ("exit(0)", want),
        "{name}: wolf --checked ({})",
        obs.version
    );
    if let Some(obs) = lupin_says(&entry) {
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "{name}: lupin {} ({})",
            obs.version,
            obs.unsupported.trim()
        );
    }
}

const SLICES: &str = r#"struct Pt {
    x: int,
    y: int,
}

fn main() -> !int {
    var bs = List[byte]()
    var ns = List[int]()
    var ss = List[str]()
    var ps = List[Pt]()
    var ts = List[(int, str)]()
    var i = 0
    while i < 300 {
        (mut bs).push((i % 251) as byte)
        (mut ns).push(i * 3)
        (mut ss).push("s{i}")
        (mut ps).push(Pt { x: i, y: 0 - i })
        (mut ts).push((i, "t{i}"))
        i = i + 1
    }
    let b = bs[10..266]
    let n = ns[..^1]
    let s = ss[^3..]
    let p = ps[100..103]
    let t = ts[7..9]
    let e = ns[5..5]
    print("{b.len} {b[0] as int} {b[255] as int} | {n.len} {n[298]} | {s.len} {s[0]} {s[2]} | {p.len} {p[2].x} {p[2].y} | {t.len} {t[1].0} {t[1].1} | {e.len}")
    // A slice is a fresh list: writing either side leaves the other.
    var m = ns[0..4]
    (mut m).push(99)
    m[0] = 7
    (mut ns).push(1)
    print("{m.len} {m[0]} {m[4]} | {ns.len} {ns[0]}")
    // One copy at exact capacity: 4,096 bytes fit a 6,000-byte budget.
    var big = List[byte]()
    i = 0
    while i < 8192 {
        (mut big).push((i % 251) as byte)
        i = i + 1
    }
    var tot = 0
    region r(cap: 6000) {
        let x = big[100..4196]
        tot = x.len + (x[0] as int) + (x[4095] as int)
    }
    print("budget {tot}")
    0
}
"#;

#[test]
fn a_list_slice_is_one_copy_of_the_same_elements() {
    every_machine(
        "slices",
        SLICES,
        "256 10 14 | 299 894 | 3 s297 s299 | 3 102 -102 | 2 8 t8 | 0\n\
         5 7 99 | 301 0\n\
         budget 4375\n",
    );
}

