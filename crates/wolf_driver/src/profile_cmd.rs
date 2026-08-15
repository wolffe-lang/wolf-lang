//! `wolf profile show|merge` (s45 target 2) — the debugging surface
//! for when PGO "does nothing".
//!
//! Both subcommands are deliberately tiny and deliberately pure: they
//! read `.wprof` files, they write `.wprof` files or a report, and they
//! compile nothing. A profile is plain data (D33: instrument→run→use is
//! driver flags and a data file, nothing executable), and the tool that
//! inspects it should not need a compiler.
//!
//! - `wolf profile show <f.wprof>` — what is in the file: version,
//!   producer, runs merged, total samples, record count, and the
//!   hottest bodies with the share of the profile each carries. This
//!   answers "is the profile empty / did the training run do anything /
//!   is the hot thing the thing I think it is".
//! - `wolf profile merge <out.wprof> <in.wprof>…` — sum compatible
//!   records across runs and shards. Refuses to merge two files that
//!   disagree about a body's block count, because the content hash
//!   fixes that structure and a disagreement means one file is corrupt.
//!
//! **Staleness against a particular build** is reported by the build,
//! not here: `wolf build --release --profile=<f>` prints one summary
//! line naming how many records no longer match, and
//! `--codegen-report` prints the full coverage beside the summary
//! index. That is where the body hashes exist, and duplicating a
//! compile inside `profile show` to re-derive them would be a second
//! answer to a question that already has one.

use std::path::{Path, PathBuf};

use wolf_wir::profile::Profile;

fn usage() -> ! {
    eprintln!(
        "usage: wolf profile show <file.wprof>\n       \
         wolf profile merge <out.wprof> <in.wprof> [in.wprof…]"
    );
    std::process::exit(2)
}

fn fail(msg: String) -> ! {
    eprintln!("wolf profile: {msg}");
    std::process::exit(2)
}

/// How many records `show` lists before it stops.
const TOP_N: usize = 10;

pub fn profile(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("show") => show(&args[1..]),
        Some("merge") => merge(&args[1..]),
        _ => usage(),
    }
}

fn read(path: &Path) -> Profile {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
    Profile::parse(&text).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())))
}

fn show(args: &[String]) {
    let [file] = args else { usage() };
    let path = PathBuf::from(file);
    let p = read(&path);
    let samples = p.samples();
    println!("profile {}", path.display());
    println!(
        "  format wprof {} ({}), {} run(s) merged",
        wolf_wir::profile::WPROF_VERSION,
        wolf_wir::profile::PRODUCER_INSTR,
        p.runs
    );
    println!("  {} record(s), {samples} sample(s)", p.funcs.len());
    if p.funcs.is_empty() {
        println!("  (empty: the instrumented run wrote no records at all)");
        return;
    }
    if samples == 0 {
        println!(
            "  (every count is zero: the instrumented binary exited before running any \
             counted block)"
        );
        return;
    }
    // Hottest by PEAK block count, not entry count: a function called
    // once around a long loop is hot, and its entry count says 1.
    let mut rows: Vec<(&String, u64, u64, usize)> = p
        .funcs
        .iter()
        .map(|(h, r)| (h, r.peak(), r.entry(), r.blocks.len()))
        .collect();
    // Descending peak; ties by hash, so the listing is deterministic.
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let shown = rows.len().min(TOP_N);
    println!("  hottest {shown} of {}:", rows.len());
    println!(
        "    {:>16}  {:>14}  {:>14}  {:>6}  body",
        "share", "peak", "entries", "blocks"
    );
    for (hash, peak, entry, blocks) in rows.iter().take(TOP_N) {
        // Integer permille: no floats in a tool whose job is to be
        // believed about small numbers.
        let permille = (u128::from(*peak) * 1000) / u128::from(samples.max(1));
        println!(
            "    {:>13}.{}%  {peak:>14}  {entry:>14}  {blocks:>6}  {}",
            permille / 10,
            permille % 10,
            &hash[..16]
        );
    }
}

fn merge(args: &[String]) {
    let Some((out, ins)) = args.split_first() else {
        usage()
    };
    if ins.is_empty() {
        usage()
    }
    let mut acc = Profile::default();
    for f in ins {
        let p = read(Path::new(f));
        acc.merge(&p)
            .unwrap_or_else(|e| fail(format!("merging {f}: {e}")));
    }
    let out = PathBuf::from(out);
    std::fs::write(&out, acc.render()).unwrap_or_else(|e| fail(format!("{}: {e}", out.display())));
    eprintln!(
        "wolf profile merge: {} file(s) -> {} ({} record(s), {} sample(s), {} run(s))",
        ins.len(),
        out.display(),
        acc.funcs.len(),
        acc.samples(),
        acc.runs
    );
}
