//! s220 (wolf-lang#551, #538, #553) — every machine prints and computes
//! wolf's unsigned and wrapping integers as the values their types hold
//! (`[type.interp.value]`, `[type.wrap.div]`).
//!
//! Two gates, each on the checked, native and release machines and on
//! lupin:
//!
//! 1. **The witness rows**: the eighteen `corpus/{typecheck,faults}/
//!    int_truth_*.lu` files, each held to its own `check:` header. A row
//!    per shape — unsigned and signed printing, `u64` arithmetic,
//!    complement and casts above `i64::MAX`, wrapping printing, ordering,
//!    division and negation at every width 8/16/32/64 and both signs, and
//!    the traps at the edges.
//! 2. **The sweep**: seeded random values of all eighteen integer types
//!    (`i8`…`i64`, `int`, `u8`…`u64`, `uint`, `wrapping[i8]`…
//!    `wrapping[u64]`), biased to the edges (`min`, `max`, `0`, `±1`,
//!    `2^63`), printed bare, in hex and built into a `str`, ordered
//!    pairwise, and combined by `+ - * / %` (and `-x` at a wrapping type)
//!    wherever the plain types stay in range — every answer compared
//!    with this file's own `i128` reference, so four machines agreeing
//!    on a wrong value is still red. `WOLF_INT_SWEEP_SEED` and
//!    `WOLF_INT_SWEEP_VALUES` widen it on demand; the counts go to
//!    stderr.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every entry on the NATIVE lane only.
//!
//! Measured at 0.2.25 (kasumi, lupin 0.1.49): 13 of the first 17 rows
//! parted from lupin on the checked machine and 12 on native and
//! release, and lupin answered all 17 as this file expects. The sweep
//! then found lupin's own defect: 0.1.48 and 0.1.49 multiply in `i128`
//! and trap a `wrapping[u64]` product past 2^127, so the eighteenth row
//! (`int_truth_wrap_mul_wide`) and any sweep program with such a product
//! pin those two releases to `trap(overflow)` by version. lupin 0.1.48
//! (the 0.2.25 pairing) also has no `!` on integers (s213's #575 mirror
//! shipped in 0.1.49): the two rows that use it pin 0.1.48 to
//! `unsupported`.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const ISSUE: &str = "wolf-lang#551/#538/#553";

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    /// The build's commit: a release names its tag's (`f516a5f` for lupin
    /// 0.1.49), a branch build its own.
    commit: String,
}

fn parse(bytes: &[u8]) -> Option<Obs> {
    let rec: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
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
/// no record fails too (#550: never read a failure as a skip).
fn wolfgang(dir: &Path, file: &str, flag: &str) -> Lane {
    let out = Command::new(wolf())
        .current_dir(dir)
        .args(["conform-run", file, flag, "--json"])
        .output()
        .expect("wolf runs");
    if let Some(obs) = parse(&out.stdout) {
        return Lane::Ran(obs);
    }
    if flag != "--checked" && lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        return Lane::Skip(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    panic!(
        "conform-run {flag} {file} in {} left no record (exit {:?}): {}",
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

fn require_lupin() -> Option<PathBuf> {
    let l = sibling_lupin();
    if l.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of {ISSUE}'s gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang machines \
             are still held to the reference answers; only the oracle leg is absent"
        );
    }
    l
}

fn lupin(lupin: &Path, dir: &Path, file: &str) -> Obs {
    let out = Command::new(lupin)
        .current_dir(dir)
        .args(["conform-run", file, "--json"])
        .output()
        .expect("lupin runs");
    parse(&out.stdout).unwrap_or_else(|| {
        panic!(
            "lupin left no record for {file} in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// What a program must answer.
#[derive(Clone, Debug, PartialEq)]
enum Want {
    /// `exit(0)` with exactly this stdout.
    Out(String),
    /// `trap(<kind>)`.
    Trap(String),
}

fn matches(want: &Want, obs: &Obs) -> bool {
    match want {
        Want::Out(s) => obs.verdict == "exit(0)" && &obs.stdout == s,
        Want::Trap(k) => obs.verdict == format!("trap({k})"),
    }
}

/// One program and what every machine must answer.
#[derive(Debug)]
struct Job {
    dir: PathBuf,
    file: String,
    want: Want,
    /// lupin RELEASES measured short of the answer — by version and the
    /// release's commit, EXACTLY — with the verdict each gave (s180's
    /// pin: a release that has not mirrored is asserted as measured; any
    /// other build, a branch build at the same version included, owes
    /// the answer).
    lupin_pins: Vec<Pin>,
}

/// `(impl_version, commit, verdict)` of a measured lupin release.
type Pin = (&'static str, &'static str, &'static str);

/// lupin 0.1.48 has no `!` on integers (#575's mirror shipped in 0.1.49).
const PIN_INT_NOT: Pin = ("0.1.48", "531bf05", "unsupported");
/// lupin 0.1.48 and 0.1.49 multiply in `i128` and trap a wrapping
/// product past 2^127 (found by s220's sweep; wolf-interp's s220 branch
/// fixes it for the next lupin).
const PINS_WIDE_MUL: [Pin; 2] = [
    ("0.1.48", "531bf05", "trap(overflow)"),
    ("0.1.49", "f516a5f", "trap(overflow)"),
];

/// Run `jobs` on four machines with a worker pool; every disagreement is
/// collected (a gate that stops at its first red row hides the shape of
/// the rest).
fn run_all(jobs: &[Job]) -> (Vec<String>, Vec<String>) {
    let lupin_bin = require_lupin();
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
                    let Some(Job {
                        dir,
                        file,
                        want,
                        lupin_pins,
                    }) = jobs.get(i)
                    else {
                        break;
                    };
                    let mut bad = Vec::new();
                    for flag in ["--checked", "--native", "--release"] {
                        match wolfgang(dir, file, flag) {
                            Lane::Skip(why) => skips.lock().unwrap().push(format!("{flag}: {why}")),
                            Lane::Ran(obs) => {
                                if !matches(want, &obs) {
                                    bad.push(format!("{flag} {} {:?}", obs.verdict, obs.stdout));
                                }
                            }
                        }
                    }
                    if let Some(l) = &lupin_bin {
                        let obs = lupin(l, dir, file);
                        let pin = lupin_pins
                            .iter()
                            .find(|(v, c, _)| *v == obs.version && *c == obs.commit);
                        let ok = match pin {
                            Some((_, _, verdict)) => obs.verdict == *verdict,
                            None => matches(want, &obs),
                        };
                        if !ok {
                            bad.push(format!(
                                "lupin {} {} {:?}",
                                obs.version, obs.verdict, obs.stdout
                            ));
                        }
                    }
                    if !bad.is_empty() {
                        failures
                            .lock()
                            .unwrap()
                            .push(format!("{file}: want {want:?}; got {}", bad.join("; ")));
                    }
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    failures.sort();
    (failures, skips.into_inner().unwrap())
}

fn report_skips(skips: &[String]) {
    if let Some(first) = skips.first() {
        eprintln!(
            "SKIP: {} program-lanes could not run on this host (the s59 pattern); first: {first}",
            skips.len()
        );
    }
}

// ------------------------------------------------------- the rows ----

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

/// A row's `check:` header as a [`Want`]: `run(exit=0, stdout="…")`
/// (JSON string escapes) or `run(exit=trap(kind))`.
fn want_of(text: &str, path: &Path) -> Want {
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("//! check: "))
        .unwrap_or_else(|| panic!("{} has no check: header", path.display()));
    if let Some(rest) = line.strip_prefix("run(exit=trap(") {
        let kind = rest.split(')').next().expect("a trap kind");
        return Want::Trap(kind.to_string());
    }
    let lit = line
        .strip_prefix("run(exit=0, stdout=")
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{}: unread check header {line:?}", path.display()));
    Want::Out(serde_json::from_str(lit).expect("the stdout literal is a JSON string"))
}

/// Every `int_truth_*` row on four machines, held to its own header.
#[test]
fn the_int_truth_rows_answer_on_four_machines() {
    let mut jobs = Vec::new();
    for sub in ["typecheck", "faults"] {
        let dir = corpus_root().join(sub);
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .expect("corpus dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("int_truth_") && n.ends_with(".lu"))
            .collect();
        names.sort();
        for n in names {
            let text = std::fs::read_to_string(dir.join(&n)).expect("row reads");
            let want = want_of(&text, &dir.join(&n));
            let code: Vec<&str> = text.lines().filter(|l| !l.starts_with("//")).collect();
            let mut lupin_pins = Vec::new();
            if code
                .iter()
                .any(|l| l.contains("{!") || l.contains("= !") || l.contains("& !"))
            {
                lupin_pins.push(PIN_INT_NOT);
            }
            if n == "int_truth_wrap_mul_wide.lu" {
                lupin_pins.extend(PINS_WIDE_MUL);
            }
            jobs.push(Job {
                dir: dir.clone(),
                file: n,
                want,
                lupin_pins,
            });
        }
    }
    // A row that vanishes, or a directory the scan cannot see, is red.
    assert_eq!(jobs.len(), 18, "the eighteen int_truth rows: {jobs:?}");
    assert_eq!(
        jobs.iter()
            .filter(|j| matches!(j.want, Want::Trap(_)))
            .count(),
        7,
        "seven trap rows"
    );
    assert_eq!(
        jobs.iter().filter(|j| j.lupin_pins.contains(&PIN_INT_NOT)).count(),
        2,
        "two rows use `!`"
    );
    let (failures, skips) = run_all(&jobs);
    report_skips(&skips);
    eprintln!("int_truth rows: {} rows x 4 machines", jobs.len());
    assert!(
        failures.is_empty(),
        "{ISSUE}: {} of {} rows disagree:\n{}",
        failures.len(),
        jobs.len(),
        failures.join("\n")
    );
}

// ------------------------------------------------------ the sweep ----

/// splitmix64: a seeded, dependency-free generator (the sweep must be
/// reproducible from its seed alone).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[derive(Clone, Copy, Debug)]
struct IntTy {
    /// The spelling of the inner prim (`i8`, `uint`, …).
    prim: &'static str,
    bits: u32,
    signed: bool,
    wrapping: bool,
}

impl IntTy {
    fn name(&self) -> String {
        if self.wrapping {
            format!("wrapping[{}]", self.prim)
        } else {
            self.prim.to_string()
        }
    }
    fn range(&self) -> (i128, i128) {
        if self.signed {
            (-(1i128 << (self.bits - 1)), (1i128 << (self.bits - 1)) - 1)
        } else {
            (0, (1i128 << self.bits) - 1)
        }
    }
    /// `v` reduced to this type's width and signedness (the wrapping
    /// family's meaning).
    fn wrap(&self, v: i128) -> i128 {
        let m = v.rem_euclid(1i128 << self.bits);
        if self.signed && m >= 1i128 << (self.bits - 1) {
            m - (1i128 << self.bits)
        } else {
            m
        }
    }
    fn fits(&self, v: i128) -> bool {
        let (lo, hi) = self.range();
        (lo..=hi).contains(&v)
    }
}

const PRIMS: [(&str, u32, bool); 10] = [
    ("i8", 8, true),
    ("i16", 16, true),
    ("i32", 32, true),
    ("i64", 64, true),
    ("int", 64, true),
    ("u8", 8, false),
    ("u16", 16, false),
    ("u32", 32, false),
    ("u64", 64, false),
    ("uint", 64, false),
];

/// The eighteen integer types: ten plain, and `wrapping[T]` over the
/// eight sized prims.
fn types() -> Vec<IntTy> {
    let mut out: Vec<IntTy> = PRIMS
        .iter()
        .map(|&(prim, bits, signed)| IntTy {
            prim,
            bits,
            signed,
            wrapping: false,
        })
        .collect();
    out.extend(
        PRIMS
            .iter()
            .filter(|(p, ..)| !matches!(*p, "int" | "uint"))
            .map(|&(prim, bits, signed)| IntTy {
                prim,
                bits,
                signed,
                wrapping: true,
            }),
    );
    out
}

/// `n` values of `t`: the edges first, then seeded draws — a third over
/// the whole width, a third small, a third near the top bit.
fn values(t: &IntTy, rng: &mut Rng, n: usize) -> Vec<i128> {
    let (lo, hi) = t.range();
    let half = 1i128 << (t.bits - 1);
    let mut out = vec![lo, hi, 0, 1, half - 1];
    if t.signed {
        out.extend([-1, lo + 1]);
    } else {
        out.extend([half, hi - 1]);
    }
    while out.len() < n {
        let r = rng.next();
        let v = match r % 3 {
            0 => t.wrap(i128::from(rng.next())),
            1 => t.wrap(i128::from(rng.next() % 2001) - 1000),
            _ => t.wrap(half + i128::from(rng.next() % 2001) - 1000),
        };
        out.push(v);
    }
    out
}

/// The hex spelling the spec gives an integer hole: sign-magnitude.
fn hex(v: i128) -> String {
    if v < 0 {
        format!("-{:x}", -v)
    } else {
        format!("{v:x}")
    }
}

/// One binding: plain types take the literal (a signed one may be
/// negative); a signed wrapping value is spelled through `int`, which
/// holds every value of every signed width.
fn bind(t: &IntTy, i: usize, v: i128) -> String {
    if t.wrapping && t.signed {
        format!("    let s{i}: int = {v}\n    let v{i} = s{i} as {}\n", t.name())
    } else {
        format!("    let v{i}: {} = {v}\n", t.name())
    }
}

/// A program for `t` over `vals`, and the stdout the reference says it
/// prints. Returns the number of holes it checks too.
fn program(t: &IntTy, vals: &[i128]) -> (String, String, usize) {
    let mut src = String::from("fn main() {\n");
    for (i, v) in vals.iter().enumerate() {
        src.push_str(&bind(t, i, *v));
    }
    let mut lines: Vec<(String, String)> = Vec::new();
    let all: Vec<usize> = (0..vals.len()).collect();
    let row = |f: &dyn Fn(usize) -> String| all.iter().map(|&i| f(i)).collect::<Vec<_>>().join(" ");
    lines.push((row(&|i| format!("{{v{i}}}")), row(&|i| vals[i].to_string())));
    lines.push((row(&|i| format!("{{v{i}:x}}")), row(&|i| hex(vals[i]))));
    let mut holes = 2 * vals.len();
    // Pairs: each value with its successor (wrapping round).
    let n = vals.len();
    let mut ord = (Vec::new(), Vec::new());
    let mut ops = (Vec::new(), Vec::new());
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b) = (vals[i], vals[j]);
        ord.0.push(format!("{{v{i} < v{j}}} {{v{i} >= v{j}}}"));
        ord.1.push(format!("{} {}", a < b, a >= b));
        holes += 2;
        let cands: [(&str, Option<i128>); 5] = [
            ("+", Some(a + b)),
            ("-", Some(a - b)),
            // Two 64-bit values can overflow `i128` only past every
            // range; the wrapping reduction reads the low bits, which
            // `wrapping_mul` keeps.
            ("*", Some(a.checked_mul(b).unwrap_or_else(|| a.wrapping_mul(b)))),
            ("/", (b != 0).then(|| a / b)),
            ("%", (b != 0).then(|| a % b)),
        ];
        for (op, r) in cands {
            let Some(r) = r else { continue };
            let r = if t.wrapping {
                t.wrap(r)
            } else if t.fits(r) {
                r
            } else {
                continue;
            };
            ops.0.push(format!("{{v{i} {op} v{j}}}"));
            ops.1.push(r.to_string());
            holes += 1;
        }
        if t.wrapping {
            ops.0.push(format!("{{-v{i}}}"));
            ops.1.push(t.wrap(-a).to_string());
            holes += 1;
        }
    }
    lines.push((ord.0.join(" "), ord.1.join(" ")));
    lines.push((ops.0.join(" "), ops.1.join(" ")));
    let mut want = String::new();
    for (code, out) in &lines {
        src.push_str(&format!("    print(\"{code}\")\n"));
        want.push_str(out);
        want.push('\n');
    }
    // The same values built into a `str` (`[type.interp.value]`: the
    // same bytes printed or materialized).
    src.push_str(&format!("    let s = \"{}\"\n    print(s)\n}}\n", lines[0].0));
    want.push_str(&lines[0].1);
    want.push('\n');
    holes += vals.len();
    (src, want, holes)
}

/// Values per sweep program (see the 4096-byte note in the sweep).
const CHUNK: usize = 12;

fn env_num(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Seeded random values of all eighteen integer types, printed and
/// combined on four machines, each answer held to the `i128` reference.
#[test]
fn random_integers_print_and_compute_alike_on_four_machines() {
    let seed = env_num("WOLF_INT_SWEEP_SEED", 0x5220);
    let per = env_num("WOLF_INT_SWEEP_VALUES", 16).max(9) as usize;
    let mut rng = Rng(seed);
    let root = std::env::temp_dir().join(format!("wolf-int-truth-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut jobs = Vec::new();
    let mut holes = 0usize;
    let mut nvals = 0usize;
    for t in types() {
        let all = values(&t, &mut rng, per);
        nvals += all.len();
        // A record carries at most 4096 bytes of stdout inline, so the
        // values go out in programs of twelve, each well under it.
        for (k, vals) in all.chunks(CHUNK).enumerate() {
            let (src, want, h) = program(&t, vals);
            assert!(
                want.len() < 4000,
                "a sweep program's stdout must fit the record's inline 4096 bytes ({} for {})",
                want.len(),
                t.name()
            );
            holes += h;
            let name = format!("{}{k}", t.name().replace(['[', ']'], "_"));
            let dir = root.join(&name);
            std::fs::create_dir_all(&dir).expect("sweep dir");
            std::fs::write(dir.join("prog.lu"), &src).expect("sweep program");
            // A wrapping product past 2^127 is lupin <= 0.1.49's trap.
            let wide = t.wrapping
                && (0..vals.len())
                    .any(|i| vals[i].checked_mul(vals[(i + 1) % vals.len()]).is_none());
            jobs.push(Job {
                dir,
                file: "prog.lu".to_string(),
                want: Want::Out(want),
                lupin_pins: if wide { PINS_WIDE_MUL.to_vec() } else { Vec::new() },
            });
        }
    }
    assert!(jobs.len() >= 18, "eighteen integer types");
    let (failures, skips) = run_all(&jobs);
    report_skips(&skips);
    eprintln!(
        "int sweep: seed {seed:#x}, 18 types, {} programs, {nvals} values, {holes} checked holes per machine",
        jobs.len()
    );
    if failures.is_empty() {
        let _ = std::fs::remove_dir_all(&root);
    }
    assert!(
        failures.is_empty(),
        "{ISSUE}: the sweep (seed {seed:#x}, programs kept under {}) parts on {} of {} programs:\n{}",
        root.display(),
        failures.len(),
        jobs.len(),
        failures.join("\n")
    );
}

/// The reference itself, pinned on the cases the issues name, so a
/// change to it cannot quietly agree with a broken machine.
#[test]
fn the_reference_answers_the_issues_witnesses() {
    let w_i8 = IntTy {
        prim: "i8",
        bits: 8,
        signed: true,
        wrapping: true,
    };
    let w_u64 = IntTy {
        prim: "u64",
        bits: 64,
        signed: false,
        wrapping: true,
    };
    assert_eq!(w_i8.wrap(200), -56, "#553");
    assert_eq!(w_i8.wrap(-128 / -1), -128, "MIN / -1 wraps");
    assert_eq!(w_u64.wrap(-1), i128::from(u64::MAX), "#538");
    assert_eq!(w_u64.wrap(i128::from(u64::MAX) / 10), 1_844_674_407_370_955_161);
    assert_eq!(hex(-56), "-38");
    assert_eq!(hex(i128::from(u64::MAX)), "ffffffffffffffff");
    let t = types();
    assert_eq!(t.len(), 18);
    let mut rng = Rng(1);
    for ty in &t {
        let v = values(ty, &mut rng, 16);
        assert_eq!(v.len(), 16);
        assert!(v.iter().all(|x| ty.fits(*x)), "{ty:?} {v:?}");
    }
}
