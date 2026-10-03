//! kw03 (wolf-lang#533, ruling K12) — `[type.numlit.cast.narrow]` on
//! four machines: an integer cast keeps the value, and traps `overflow`
//! when the target cannot hold it; truncation is `v as wrapping[T] as T`.
//!
//! The matrix: every ordered pair of the ten integer types (`i8`–`i64`,
//! `u8`–`u64`, `int`, `uint`), at each of the target's boundary values
//! the source can hold — `min`, `max`, `min - 1`, `max + 1`, `0`, `-1`.
//! That is 245 cast rows, 77 of them out of range; each of the 77 is
//! also written as the truncation spelling, which must keep the low bits
//! (read by the target's signedness). Every row is its own program, run
//! by `conform-run` on the checked, native and release machines and by
//! lupin.
//!
//! Measured at trunk `a2cb316a` before kw03 (kasumi, lupin 0.1.44):
//! native and release refused 144 cast rows ("narrowing numeric casts
//! (range-check semantics)") and answered 23 sign-changing rows with the
//! bits reinterpreted (`-1 as u64` printed `-1`); the checked machine
//! trapped 31 truncation rows (it range-checked a signed wrapping value's
//! stored mask, not its value); lupin answered all 322.
//!
//! One named carve-out, wolf-lang#551: the checked machine's `u64`
//! holds `0..=i64::MAX`, so a row whose source or kept value lies above
//! `i64::MAX` is `unsupported` there (the literal, or the cast that would
//! have to hold it, refused by name). Native, release and lupin answer
//! those rows; a kept value above `i64::MAX` is compared (`r == LIT`)
//! instead of printed, because printing a plain `u64` that large is
//! #551's other half, not this clause.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const ISSUE: &str = "wolf-lang#533";

/// The ten integer types: name, width, signed.
const TYPES: [(&str, u32, bool); 10] = [
    ("i8", 8, true),
    ("i16", 16, true),
    ("i32", 32, true),
    ("i64", 64, true),
    ("u8", 8, false),
    ("u16", 16, false),
    ("u32", 32, false),
    ("u64", 64, false),
    ("int", 64, true),
    ("uint", 64, false),
];

fn range(bits: u32, signed: bool) -> (i128, i128) {
    if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    }
}

/// A literal the source type holds, spelled the way wolf takes it
/// (no unary minus on a literal at `i64::MIN`).
fn literal(v: i128) -> String {
    if v >= 0 {
        v.to_string()
    } else if v == i128::from(i64::MIN) {
        "0 - 9223372036854775807 - 1".to_string()
    } else {
        format!("0 - {}", -v)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Want {
    /// `exit(0)` with this stdout.
    Keep(String),
    Trap,
}

#[derive(Clone, Debug)]
struct Row {
    name: String,
    src: &'static str,
    dst: &'static str,
    value: i128,
    /// The value the program must keep (`None` for a trap row).
    kept: Option<i128>,
    want: Want,
    program: String,
}

impl Row {
    /// wolf-lang#551: the checked machine cannot hold this row's source
    /// literal or its kept value.
    fn needs_u64_upper_half(&self) -> bool {
        let top = i128::from(i64::MAX);
        self.value > top || self.kept.is_some_and(|k| k > top)
    }
}

fn row(
    name: String,
    src: &'static str,
    dst: &'static str,
    v: i128,
    kept: Option<i128>,
    wrap: bool,
) -> Row {
    let cast = if wrap {
        format!("v as wrapping[{dst}] as {dst}")
    } else {
        format!("v as {dst}")
    };
    let (show, want) = match kept {
        None => ("{r}".to_string(), Want::Trap),
        Some(k) if k > i128::from(i64::MAX) => (format!("{{r == {k}}}"), Want::Keep("true".into())),
        Some(k) => ("{r}".to_string(), Want::Keep(k.to_string())),
    };
    let program = format!(
        "fn main() -> int {{\n    let v: {src} = {}\n    let r = {cast}\n    print(\"{show}\")\n    0\n}}\n",
        literal(v)
    );
    Row {
        name,
        src,
        dst,
        value: v,
        kept,
        want,
        program,
    }
}

fn rows() -> Vec<Row> {
    let mut out = Vec::new();
    for &(s, sb, ss) in &TYPES {
        for &(d, db, ds) in &TYPES {
            if s == d {
                continue;
            }
            let (lo, hi) = range(db, ds);
            let (slo, shi) = range(sb, ss);
            let mut vals = vec![lo, hi, lo - 1, hi + 1, 0, -1];
            vals.sort_unstable();
            vals.dedup();
            for v in vals.into_iter().filter(|v| (slo..=shi).contains(v)) {
                let fits = (lo..=hi).contains(&v);
                out.push(row(
                    format!("cast_{s}_to_{d}_{v}"),
                    s,
                    d,
                    v,
                    fits.then_some(v),
                    false,
                ));
                if !fits {
                    let mut t = v & ((1i128 << db) - 1);
                    if ds && t >= (1i128 << (db - 1)) {
                        t -= 1i128 << db;
                    }
                    out.push(row(format!("wrap_{s}_to_{d}_{v}"), s, d, v, Some(t), true));
                }
            }
        }
    }
    out
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
}

fn parse(bytes: &[u8]) -> Option<Obs> {
    let rec: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"]
            .as_str()
            .unwrap_or("")
            .trim_end()
            .to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
    })
}

enum Lane {
    Ran(Obs),
    /// The host cannot run this tier (the s59 skip), named on stderr.
    Skip(String),
}

/// One wolfgang machine. `conform-run` answers a compile error or an
/// `unsupported` with exit 0 and a record, so an exit 2 with no record
/// is the only skip; an ICE fails (#471), and anything else that leaves
/// no record fails too (#550's lesson: never read a failure as a skip).
fn wolfgang(dir: &Path, flag: &str) -> Lane {
    let out = Command::new(wolf())
        .current_dir(dir)
        .args(["conform-run", "prog.lu", flag, "--json"])
        .output()
        .expect("wolf runs");
    if let Some(obs) = parse(&out.stdout) {
        return Lane::Ran(obs);
    }
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        return Lane::Skip(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    panic!(
        "conform-run {flag} in {} left no record (exit {:?}): {}",
        dir.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The sibling lupin, found as `pairing.rs` finds it: `LUPIN` first,
/// then a `wolf-interp` checkout beside an ancestor.
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

fn lupin(lupin: &Path, dir: &Path) -> Obs {
    let out = Command::new(lupin)
        .current_dir(dir)
        .args(["conform-run", "prog.lu", "--json"])
        .output()
        .expect("lupin runs");
    parse(&out.stdout).unwrap_or_else(|| {
        panic!(
            "lupin left no record in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn matches(want: &Want, obs: &Obs) -> bool {
    match want {
        Want::Trap => obs.verdict == "trap(overflow)",
        Want::Keep(s) => obs.verdict == "exit(0)" && &obs.stdout == s,
    }
}

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("wolf-narrow-cast-lanes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// The whole matrix on four machines. Every disagreement is collected
/// and reported together (a matrix that stops at its first red row hides
/// the shape of the rest).
#[test]
fn every_integer_pair_keeps_or_traps_on_four_machines() {
    let rows = rows();
    assert_eq!(rows.len(), 322, "245 cast rows + 77 truncation rows");
    assert_eq!(rows.iter().filter(|r| r.want == Want::Trap).count(), 77);

    let root = scratch();
    let lupin_bin = sibling_lupin();
    if lupin_bin.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of {ISSUE}'s gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang machines \
             are still held to the clause's answer; only the oracle leg is absent"
        );
    }

    let failures = Mutex::new(Vec::<String>::new());
    let skips = Mutex::new(Vec::<String>::new());
    let next = Mutex::new(0usize);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(16));
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = {
                        let mut n = next.lock().unwrap();
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let Some(r) = rows.get(i) else { break };
                    let dir = root.join(&r.name);
                    std::fs::create_dir_all(&dir).expect("row dir");
                    std::fs::write(dir.join("prog.lu"), &r.program).expect("row program");
                    let mut bad = Vec::new();
                    for flag in ["--checked", "--native", "--release"] {
                        match wolfgang(&dir, flag) {
                            Lane::Skip(why) => skips.lock().unwrap().push(format!("{flag}: {why}")),
                            Lane::Ran(obs) => {
                                let ok = if flag == "--checked" && r.needs_u64_upper_half() {
                                    obs.verdict == "unsupported"
                                } else {
                                    matches(&r.want, &obs)
                                };
                                if !ok {
                                    bad.push(format!("{flag} {} {:?}", obs.verdict, obs.stdout));
                                }
                            }
                        }
                    }
                    if let Some(l) = &lupin_bin {
                        let obs = lupin(l, &dir);
                        if !matches(&r.want, &obs) {
                            bad.push(format!(
                                "lupin {} {} {:?}",
                                obs.version, obs.verdict, obs.stdout
                            ));
                        }
                    }
                    if !bad.is_empty() {
                        failures.lock().unwrap().push(format!(
                            "{} ({} as {}, value {}): want {:?}; got {}",
                            r.name,
                            r.src,
                            r.dst,
                            r.value,
                            r.want,
                            bad.join("; ")
                        ));
                    }
                }
            });
        }
    });

    let skips = skips.into_inner().unwrap();
    if let Some(first) = skips.first() {
        eprintln!(
            "SKIP: {} row-lanes could not run on this host (the s59 pattern); first: {first}",
            skips.len()
        );
    }
    let mut failures = failures.into_inner().unwrap();
    failures.sort();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        failures.is_empty(),
        "{ISSUE} / [type.numlit.cast.narrow]: {} of {} rows disagree:\n{}",
        failures.len(),
        rows.len(),
        failures.join("\n")
    );
}

/// The carve-out stays exactly #551's rows: the checked machine is
/// excused on these 20 and no others.
#[test]
fn the_checked_carve_out_is_only_the_u64_upper_half() {
    let rows = rows();
    let carved: Vec<&str> = rows
        .iter()
        .filter(|r| r.needs_u64_upper_half())
        .map(|r| r.name.as_str())
        .collect();
    assert!(
        carved
            .iter()
            .all(|n| n.contains("u64") || n.contains("uint")),
        "{carved:?}"
    );
    assert_eq!(carved.len(), 20, "{carved:?}");
}
