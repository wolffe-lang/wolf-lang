//! `fmt-fuzz` — the corpus-seeded mutation loop over the formatter's
//! invariants. Stable Rust, no libFuzzer, no nightly: drive it with
//! `cargo xtask fmt-fuzz` (which is also the CI lane).
//!
//! The nightly's libFuzzer lane finds fmt idempotence breaks, but it
//! starts from bytes and has to rediscover wolf's grammar every run.
//! This loop starts from the *corpus* — real programs the formatter
//! already calls canonical — and mutates around comments, the layer
//! where every idempotence class so far has lived. Measured against the
//! same well, it finds in seconds what the byte-level lane finds in
//! fifteen minutes.
//!
//! # Invariants (per case)
//!
//! 1. `format_text` never panics (either pass).
//! 2. A fallback outcome returns the input byte-identical.
//! 3. Idempotence: `fmt(fmt(s)) == fmt(s)`, byte-equal.
//! 4. Comment multiset preservation across the format.
//! 5. Round-trip: output normalizes to the same tree as the input.
//! 6. Pass two never falls back after a pass one that did not — a
//!    formatter whose own output fails its self-check is broken even
//!    though invariant 3 would pass vacuously (the fallback returns the
//!    input, which *is* the previous output).
//!
//! Invariant 6 is stricter than the older libFuzzer target, which
//! accepts a second-pass fallback because the fallback returns its
//! input and invariant 3 then holds trivially.
//!
//! # Determinism
//!
//! SplitMix64 seeded from `--seed`; the same seed, budget and seed
//! corpus replay the same case stream. Findings are minimized (line
//! deletion, then byte deletion, keeping the failure class, under a
//! counted budget) and deduped by minimized content.
//!
//! # Flags
//!
//! - `--seconds=N` wall budget (`0`, or any `--cases=`, means no clock)
//! - `--cases=N` stop after N cases — the replayable form
//! - `--seed=N`, `--seeds=DIR` (repeatable; defaults to the corpus and
//!   the fmt regression fixtures)
//! - `--out=DIR` write each minimized finding as a `.lu.pending`
//! - `--expect=N` exit 0 only if exactly N findings survive (a ratchet
//!   for a pinned replay)
//! - `--allow-open` report convergence findings without failing; the
//!   corrupting classes still fail
//! - `--file=PATH` reproduce one case, showing both passes
//! - `--triage=DIR` one line per banked case: class + first line diff

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// --------------------------------------------------------------- rng ---

/// SplitMix64 — three lines, no dependency, good enough to mutate with.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish in `0..n` (`0` when `n == 0`).
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

// ------------------------------------------------------- invariants ---

/// The failure classes this loop can report. The string is the class
/// name used for dedup and for the minimizer's "same failure" test.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fail {
    Panic,
    FallbackNotIdentical,
    NotIdempotent,
    CommentDrift,
    TreeDrift,
    SecondPassFallback,
}

impl Fail {
    fn name(self) -> &'static str {
        match self {
            Fail::Panic => "panic",
            Fail::FallbackNotIdentical => "fallback-not-identical",
            Fail::NotIdempotent => "not-idempotent",
            Fail::CommentDrift => "comment-drift",
            Fail::TreeDrift => "tree-drift",
            Fail::SecondPassFallback => "second-pass-fallback",
        }
    }
}

fn norm(src: &[u8]) -> wolf_fmt::NTree {
    let mut sm = wolf_span::SourceMap::new();
    let f = sm.intern(Path::new("fuzz.lu"));
    let parse = wolf_parse::parse_file(f, src);
    wolf_fmt::normalize(&parse.root, src)
}

fn comments(src: &[u8]) -> Vec<Vec<u8>> {
    let mut sm = wolf_span::SourceMap::new();
    let f = sm.intern(Path::new("fuzz.lu"));
    wolf_fmt::comment_multiset(f, src)
}

/// Run every invariant over one input. `None` is a pass.
fn check_inner(src: &[u8]) -> Option<Fail> {
    let out = wolf_fmt::format_text(src);
    if out.fell_back {
        return (out.text != src).then_some(Fail::FallbackNotIdentical);
    }
    let again = wolf_fmt::format_text(&out.text);
    if again.fell_back {
        return Some(Fail::SecondPassFallback);
    }
    if again.text != out.text {
        return Some(Fail::NotIdempotent);
    }
    if comments(src) != comments(&out.text) {
        return Some(Fail::CommentDrift);
    }
    if norm(src) != norm(&out.text) {
        return Some(Fail::TreeDrift);
    }
    None
}

/// `check_inner` with panics caught — a panic is itself a finding.
fn check(src: &[u8]) -> Option<Fail> {
    match std::panic::catch_unwind(|| check_inner(src)) {
        Ok(v) => v,
        Err(_) => Some(Fail::Panic),
    }
}

// --------------------------------------------------------- mutation ---

/// Fragments spliced in by the token mutator. Comment shapes dominate
/// on purpose: every idempotence class found so far has been a
/// comment-layout class, and the ones that are not still need comments
/// nearby to surface.
const TOKENS: &[&str] = &[
    "// c\n",
    "//c",
    "  // t",
    "\n// own\n",
    "/// doc\n",
    "//! inner\n",
    "//",
    " //\n",
    "\n",
    "\n\n",
    "\n\n\n",
    "    ",
    "\t",
    " ",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "+",
    "-",
    "!",
    "*",
    "&",
    "?",
    ",",
    ";",
    ":",
    ".",
    "=",
    "==",
    "=>",
    "..",
    "^",
    "|",
    "fn f() {",
    "let x = ",
    "var y = ",
    "if c ",
    "else ",
    "match e ",
    "region r ",
    "in r ",
    "for i in 0..n ",
    "return ",
    "defer ",
    "move ",
    "freeze ",
    "\"s\"",
    "\"a {x} b\"",
    "\"{x:>8}\"",
    "0",
    "1",
    "xs",
];

/// One mutation step, applied in place.
fn mutate_once(buf: &mut Vec<u8>, rng: &mut Rng, pool: &[Vec<u8>]) {
    let len = buf.len();
    match rng.below(9) {
        // Splice a chunk of another seed in.
        0 => {
            let donor = rng.pick(pool);
            if donor.is_empty() {
                return;
            }
            let a = rng.below(donor.len());
            let b = (a + 1 + rng.below(96)).min(donor.len());
            let at = rng.below(len + 1);
            let piece = donor[a..b].to_vec();
            buf.splice(at..at, piece);
        }
        // Insert a token fragment.
        1 | 2 => {
            let t = rng.pick(TOKENS).as_bytes().to_vec();
            let at = rng.below(len + 1);
            buf.splice(at..at, t);
        }
        // Delete a range.
        3 => {
            if len == 0 {
                return;
            }
            let a = rng.below(len);
            let b = (a + 1 + rng.below(48)).min(len);
            buf.drain(a..b);
        }
        // Duplicate a range (the compounding-indent classes love this).
        4 => {
            if len == 0 {
                return;
            }
            let a = rng.below(len);
            let b = (a + 1 + rng.below(64)).min(len);
            let piece = buf[a..b].to_vec();
            let at = rng.below(len + 1);
            buf.splice(at..at, piece);
        }
        // Overwrite bytes with a token fragment.
        5 => {
            if len == 0 {
                return;
            }
            let t = rng.pick(TOKENS).as_bytes().to_vec();
            let a = rng.below(len);
            let b = (a + t.len()).min(len);
            buf.splice(a..b, t);
        }
        // Move a whole line somewhere else — cheap way to strand a
        // comment on a construct it did not belong to.
        6 => {
            let lines: Vec<usize> = (0..len).filter(|&i| buf[i] == b'\n').collect();
            if lines.len() < 2 {
                return;
            }
            let i = rng.below(lines.len() - 1);
            let (a, b) = (lines[i] + 1, lines[i + 1] + 1);
            let piece: Vec<u8> = buf.drain(a..b).collect();
            let at = rng.below(buf.len() + 1);
            buf.splice(at..at, piece);
        }
        // Flip one byte.
        7 => {
            if len == 0 {
                return;
            }
            let i = rng.below(len);
            buf[i] ^= 1 << rng.below(7);
        }
        // Truncate.
        _ => {
            if len == 0 {
                return;
            }
            buf.truncate(rng.below(len));
        }
    }
}

const MAX_CASE: usize = 24 * 1024;

fn mutate(pool: &[Vec<u8>], rng: &mut Rng) -> Vec<u8> {
    let mut buf = rng.pick(pool).clone();
    let steps = 1 + rng.below(6);
    for _ in 0..steps {
        mutate_once(&mut buf, rng, pool);
        if buf.len() > MAX_CASE {
            buf.truncate(MAX_CASE);
        }
    }
    buf
}

// ------------------------------------------------------- minimizing ---

/// Re-check attempts one minimization may spend. Bounded and counted,
/// not timed: a wall-clock bound would make the shrink — and therefore
/// the dedup that keys on it — depend on how busy the machine is, and
/// this loop's whole value is that a seed replays. A finding that
/// exhausts the budget is reported at whatever size it reached.
const MINIMIZE_BUDGET: usize = 30_000;

/// Shrink a finding while it keeps failing the *same* way: whole lines
/// first (fast, and keeps the result readable), then byte ranges.
fn minimize(src: &[u8], want: Fail) -> Vec<u8> {
    let mut cur = src.to_vec();
    let mut budget = MINIMIZE_BUDGET;
    let mut spend = |cand: &[u8]| -> bool {
        if budget == 0 {
            return false;
        }
        budget -= 1;
        check(cand) == Some(want)
    };
    // Line-granular passes until a pass changes nothing.
    loop {
        let mut shrunk = false;
        let mut i = 0;
        while i < cur.len() {
            let end = cur[i..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(cur.len(), |p| i + p + 1);
            let mut cand = cur.clone();
            cand.drain(i..end);
            if spend(&cand) {
                cur = cand;
                shrunk = true;
            } else {
                i = end;
            }
        }
        if !shrunk {
            break;
        }
    }
    // Byte-granular passes, shrinking chunk size geometrically.
    let mut chunk = 32usize;
    while chunk >= 1 {
        let mut i = 0;
        while i + chunk <= cur.len() {
            let mut cand = cur.clone();
            cand.drain(i..i + chunk);
            if spend(&cand) {
                cur = cand;
            } else {
                i += 1;
            }
        }
        chunk /= 2;
    }
    cur
}

// ------------------------------------------------------------ seeds ---

fn collect_lu(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            // NEVER seed from the bank of open classes. A mutation of a
            // program that already fails almost always still fails, so
            // banking ten cases here turned a 20-finding sweep into a
            // 1,900-finding one — all of them variants of what was
            // already known, drowning anything new. Point `--seeds=` at
            // the directory deliberately to hunt *around* an open
            // class; the default never does.
            if p.file_name().is_some_and(|n| n == "unfixed") {
                continue;
            }
            collect_lu(&p, out);
        } else {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            if name.ends_with(".lu") || name.ends_with(".lu.pending") {
                out.push(p);
            }
        }
    }
}

fn seed_pool(roots: &[PathBuf]) -> Vec<Vec<u8>> {
    let mut files = Vec::new();
    for r in roots {
        collect_lu(r, &mut files);
    }
    files.sort();
    let mut pool: Vec<Vec<u8>> = files
        .iter()
        .filter_map(|p| std::fs::read(p).ok())
        .filter(|b| b.len() <= MAX_CASE)
        .collect();
    // A couple of hand seeds so the loop still works with no corpus.
    pool.push(
        b"fn main() -> !int {\n    // c\n    let x = 1 + // t\n        2\n    0\n}\n".to_vec(),
    );
    pool.push(b"fn main() -> !int {\n    let y = -  // t\n        x\n    0\n}\n".to_vec());
    pool
}

// ------------------------------------------------------------- main ---

fn arg_val(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .find_map(|a| a.strip_prefix(key).map(|v| v.to_string()))
}

/// `--file=PATH`: check one input and show both passes. The reproducer
/// for a banked fixture — what the loop found, without the loop.
fn show_one(path: &str) -> ! {
    let src = std::fs::read(path).expect("read --file");
    let verdict = check(&src);
    let one = wolf_fmt::format_text(&src);
    let two = wolf_fmt::format_text(&one.text);
    println!("== {path}: {}", verdict.map_or("holds", |f| f.name()));
    println!("-- input --\n{}", String::from_utf8_lossy(&src));
    println!(
        "-- pass 1 (fell_back={}, partial={}) --\n{}",
        one.fell_back,
        one.partial,
        String::from_utf8_lossy(&one.text)
    );
    println!(
        "-- pass 2 (fell_back={}, partial={}) --\n{}",
        two.fell_back,
        two.partial,
        String::from_utf8_lossy(&two.text)
    );
    std::process::exit(i32::from(verdict.is_some()));
}

/// `--triage=DIR`: one line per banked case — the failure class and
/// the first line where pass one and pass two disagree. Findings
/// cluster hard (one root cause wears many shapes), and this is how the
/// clusters become visible without reading a hundred programs.
fn triage(dir: &str) -> ! {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("triage dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for p in &files {
        let src = std::fs::read(p).expect("read case");
        let Some(f) = check(&src) else {
            println!("{}\tHOLDS\t-", p.display());
            continue;
        };
        let sig = std::panic::catch_unwind(|| {
            let one = wolf_fmt::format_text(&src);
            let two = wolf_fmt::format_text(&one.text);
            let a: Vec<&[u8]> = one.text.split(|b| *b == b'\n').collect();
            let b: Vec<&[u8]> = two.text.split(|b| *b == b'\n').collect();
            for i in 0..a.len().max(b.len()) {
                let (x, y) = (
                    a.get(i).copied().unwrap_or(b""),
                    b.get(i).copied().unwrap_or(b""),
                );
                if x != y {
                    return format!(
                        "{:?} => {:?}",
                        String::from_utf8_lossy(x),
                        String::from_utf8_lossy(y)
                    );
                }
            }
            "<no line diff>".to_string()
        })
        .unwrap_or_else(|_| "<panicked>".to_string());
        println!("{}\t{}\t{sig}", p.display(), f.name());
    }
    std::panic::set_hook(default_hook);
    std::process::exit(0);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(p) = arg_val(&args, "--file=") {
        show_one(&p);
    }
    if let Some(d) = arg_val(&args, "--triage=") {
        triage(&d);
    }
    let seed: u64 = arg_val(&args, "--seed=")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x5EED_5EED_5EED_5EED);
    let max_cases: usize = arg_val(&args, "--cases=")
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    // `--seconds=0` (or a bare `--cases=`) means "no clock": a fixed
    // case count with a fixed seed replays exactly, which is what a CI
    // ratchet needs — a wall-clock budget would make the finding count
    // depend on how busy the machine is.
    let secs: u64 = arg_val(&args, "--seconds=")
        .and_then(|v| v.parse().ok())
        .unwrap_or(if max_cases == usize::MAX { 10 } else { 0 });
    let out_dir = arg_val(&args, "--out=").map(PathBuf::from);
    let roots: Vec<PathBuf> = {
        let v: Vec<PathBuf> = args
            .iter()
            .filter_map(|a| a.strip_prefix("--seeds=").map(PathBuf::from))
            .collect();
        if v.is_empty() {
            vec![
                PathBuf::from("corpus"),
                PathBuf::from("crates/wolf_fmt/tests/regressions"),
            ]
        } else {
            v
        }
    };

    let pool = seed_pool(&roots);
    eprintln!(
        "fmt-fuzz: {} seeds, seed=0x{seed:016X}, budget={secs}s{}",
        pool.len(),
        if max_cases == usize::MAX {
            String::new()
        } else {
            format!(", cases={max_cases}")
        }
    );

    // The loop *expects* panics; do not let each one print a backtrace
    // banner over the report.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut rng = Rng(seed);
    let deadline = (secs > 0).then(|| Instant::now() + Duration::from_secs(secs));
    let mut cases = 0usize;
    let mut findings: Vec<(Fail, Vec<u8>)> = Vec::new();
    let mut seen: BTreeSet<Vec<u8>> = BTreeSet::new();

    while cases < max_cases && deadline.is_none_or(|d| Instant::now() < d) {
        let src = mutate(&pool, &mut rng);
        cases += 1;
        if let Some(f) = check(&src) {
            let small = minimize(&src, f);
            if seen.insert(small.clone()) {
                findings.push((f, small));
            }
        }
    }

    std::panic::set_hook(default_hook);

    println!(
        "fmt-fuzz: {cases} cases, {} distinct findings",
        findings.len()
    );
    for (i, (f, src)) in findings.iter().enumerate() {
        println!("--- finding {} [{}] ---", i + 1, f.name());
        println!("{}", String::from_utf8_lossy(src));
        if let Some(dir) = &out_dir {
            let _ = std::fs::create_dir_all(dir);
            let name = format!(
                "fuzz_{}_{:02}.lu.pending",
                f.name().replace('-', "_"),
                i + 1
            );
            let _ = std::fs::write(dir.join(&name), src);
            println!("(written to {})", dir.join(&name).display());
        }
    }

    // Corruption is never tolerated, at any count: a lost comment, a
    // changed tree, a panic, or a fallback that did not return the
    // input is a bug in a different league from a layout that has not
    // converged.
    let corrupting = findings
        .iter()
        .filter(|(f, _)| {
            matches!(
                f,
                Fail::Panic | Fail::FallbackNotIdentical | Fail::CommentDrift | Fail::TreeDrift
            )
        })
        .count();
    if corrupting > 0 {
        eprintln!("fmt-fuzz: {corrupting} CORRUPTING finding(s) — these never pass");
        std::process::exit(1);
    }
    // The ratchet: with a fixed seed and a fixed case count this run is
    // a replay, so the layout-convergence classes still open have an
    // exact expected count. More is a regression; fewer means someone
    // drained part of the well and owes this number an update.
    if let Some(expect) = arg_val(&args, "--expect=").and_then(|v| v.parse::<usize>().ok()) {
        if findings.len() == expect {
            println!("fmt-fuzz: {expect} known-open finding(s), as banked");
            return;
        }
        eprintln!(
            "fmt-fuzz: expected {expect} known-open finding(s), got {} — \
             fix them and lower the number, or explain the new one",
            findings.len()
        );
        std::process::exit(1);
    }
    // `--allow-open`: the well is not dry, and a wall-clock CI lane
    // cannot pin an exact count on a machine of unknown speed. The lane
    // still holds the invariants that must never bend (above) and
    // reports what has not converged; the nightly's long sweep is where
    // a human triages the rest. Use `--expect=N` for a pinned replay.
    if !findings.is_empty() && !args.iter().any(|a| a == "--allow-open") {
        std::process::exit(1);
    }
    if !findings.is_empty() {
        println!(
            "fmt-fuzz: {} layout finding(s) still open — banked, not corrupting",
            findings.len()
        );
    }
}
