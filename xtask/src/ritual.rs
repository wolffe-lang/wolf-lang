//! `cargo xtask bench ritual` — the M2 measurement ritual, codified.
//!
//! Nine hand-runs (2026-08-20 → -22, #89) established the protocol this
//! command automates: a measurement is trustworthy only when the machine
//! is provably quiet, the conditions are recorded before the number
//! exists, the gate renders from the raw records, and the verdict lands
//! in a ledger that computes the s44 clock instead of remembering it.
//! Every step refuses by name; a ritual that cannot be performed
//! honestly is not performed (exit != 0 with the reason). A REFUTED
//! gate, by contrast, is a result: the ritual completed, exit 0, and
//! the verdict is data.
//!
//! Order of operations, each gated on the last:
//!   quiet check -> conditions file -> the t1 run -> the gate ->
//!   tick accounting -> bench-data append (named skip if it cannot).
//!
//! `--dry-run` exercises the mechanics (quiet check, conditions, ledger
//! arithmetic) with a synthetic verdict and NO bench run; it never
//! touches the real ledger — the would-be line goes to the out-dir as
//! `ledger-preview.jsonl`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::bench_t1;

const LEDGER: &str = "bench/ritual-ledger.jsonl";
/// s44 says three consecutive NIGHTLY runs. Two holds taken minutes
/// apart are one thermal state wearing two timestamps, so a hold only
/// ADVANCES the count when at least this many seconds separate it from
/// the previously counted hold; a closer one is recorded with an
/// unchanged count and a note. Half a day is the honest floor for
/// "a different night".
const NIGHT_SECONDS: i64 = 12 * 60 * 60;
/// How long the quiet check will wait for load to settle before
/// refusing (the hand ritual's own habit, bounded).
const SETTLE_SECONDS: u64 = 300;

pub fn ritual(args: &[String]) -> ExitCode {
    let mut out_dir = PathBuf::from("bench-results/ritual");
    let mut dry_run = false;
    for a in args {
        if let Some(v) = a.strip_prefix("--out-dir=") {
            out_dir = PathBuf::from(v);
        } else if a == "--dry-run" {
            dry_run = true;
        } else {
            eprintln!("bench ritual: unknown argument `{a}`");
            return ExitCode::from(2);
        }
    }
    if !Path::new("bench/bench-exceptions.md").exists() {
        eprintln!("bench ritual: run from the repository root (bench/ not found)");
        return ExitCode::from(2);
    }

    // A ritual measures a commit, not a working tree. A DRY run
    // measures nothing, so a dirty tree only marks the conditions.
    let dirty = run_capture("git", &["status", "--porcelain"]);
    let tree_dirty = match dirty {
        Some(s) => !s.trim().is_empty(),
        None => {
            eprintln!("bench ritual: REFUSED — cannot read git status");
            return ExitCode::FAILURE;
        }
    };
    if tree_dirty && !dry_run {
        eprintln!("bench ritual: REFUSED — the tree is dirty; a ritual measures a commit");
        return ExitCode::FAILURE;
    }
    let commit = run_capture("git", &["rev-parse", "--short", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    // ---------------------------------------------------- quiet check --
    let mut waited = 0u64;
    let load = loop {
        let load = load1();
        if load < 1.0 {
            break load;
        }
        if waited >= SETTLE_SECONDS {
            eprintln!(
                "bench ritual: REFUSED — load1 {load:.2} after waiting {SETTLE_SECONDS}s \
                 (the ritual needs a quiet machine, not a patient one)"
            );
            return ExitCode::FAILURE;
        }
        eprintln!("bench ritual: load1 {load:.2}, settling ({waited}/{SETTLE_SECONDS}s)");
        std::thread::sleep(std::time::Duration::from_secs(15));
        waited += 15;
    };
    let intruders = compiler_processes_beyond_self();
    if !intruders.is_empty() {
        eprintln!(
            "bench ritual: REFUSED — compiler processes are running beyond this ritual: {}",
            intruders.join(", ")
        );
        return ExitCode::FAILURE;
    }

    // ------------------------------------------------ conditions file --
    let ts = utc_now_iso();
    std::fs::create_dir_all(&out_dir).expect("mkdir ritual out-dir");
    // Filenames must be filesystem-agnostic: upload-artifact rejects
    // the ISO stamp's colons (NTFS), which killed the first validation
    // run's upload. The LEDGER keeps ISO; the filename drops separators.
    let fname_ts: String = ts.chars().filter(|c| *c != ':' && *c != '-').collect();
    let stem = format!("{fname_ts}-{commit}");
    let profdata = which("llvm-profdata");
    let conditions = format!(
        "M2 ritual conditions — {ts}, commit {commit}{dirty_mark}\n\
         load: {load:.2} at launch (quiet check: settled, zero compiler processes beyond self)\n\
         governor: {gov}; smt: {smt}; aslr: {aslr}\n\
         llvm-profdata: {prof}\n\
         performed by: cargo xtask bench ritual (the codified protocol; conditions generated, not typed)\n",
        dirty_mark = if tree_dirty {
            " (tree DIRTY — dry run only)"
        } else {
            ""
        },
        gov = read_trim("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"),
        smt = read_trim("/sys/devices/system/cpu/smt/control"),
        aslr = read_trim("/proc/sys/kernel/randomize_va_space"),
        prof = if profdata {
            "present"
        } else {
            "ABSENT — the PGO scrutiny lane will skip, named"
        },
    );
    let cond_path = out_dir.join(format!("{stem}-conditions.txt"));
    std::fs::write(&cond_path, &conditions).expect("write conditions");
    eprint!("{conditions}");
    eprintln!("bench ritual: conditions -> {}", cond_path.display());

    if dry_run {
        // Mechanics only: ledger arithmetic over a synthetic verdict,
        // previewed beside the results, the real ledger untouched.
        let entry = ledger_entry(
            &ts,
            &commit,
            None,
            "DRY-RUN",
            "mechanics test, no bench run",
        );
        let preview = out_dir.join("ledger-preview.jsonl");
        std::fs::write(&preview, format!("{entry}\n")).expect("write ledger preview");
        eprintln!("bench ritual: DRY RUN — would append: {entry}");
        eprintln!(
            "bench ritual: preview -> {} (real ledger untouched)",
            preview.display()
        );
        return ExitCode::SUCCESS;
    }

    // ------------------------------------------------------- the run --
    let jsonl = out_dir.join(format!("{stem}-t1.jsonl"));
    eprintln!(
        "bench ritual: measuring (t1, 10 runs/kernel) -> {}",
        jsonl.display()
    );
    let Some(records) = bench_t1::run(10, None, &commit) else {
        eprintln!("bench ritual: FAILED — the t1 suite did not complete");
        return ExitCode::FAILURE;
    };
    let mut body = String::new();
    for r in &records {
        body.push_str(&r.to_string());
        body.push('\n');
    }
    std::fs::write(&jsonl, body).expect("write t1 jsonl");

    // ------------------------------------------------------ the gate --
    let path_str = jsonl.display().to_string();
    let (scored, outcome) = match bench_t1::gate_eval(&path_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("bench ritual: FAILED — {e}");
            return ExitCode::FAILURE;
        }
    };
    let _ = bench_t1::gate_render(&path_str, &scored, &outcome);
    let verdict = if outcome.primary_holds {
        "HOLDS"
    } else {
        "DOES-NOT-HOLD"
    };
    let gate_path = out_dir.join(format!("{stem}-gate.txt"));
    std::fs::write(
        &gate_path,
        format!(
            "verdict: {verdict}\ngeomean: {}\nexceptions: {}\n",
            outcome
                .geomean
                .map(|g| format!("{g:.3}"))
                .unwrap_or_else(|| "unavailable".into()),
            outcome.exceptions_declared
        ),
    )
    .expect("write gate summary");

    // ----------------------------------------------- tick accounting --
    let entry = ledger_entry(&ts, &commit, outcome.geomean, verdict, "");
    append_line(LEDGER, &entry);
    eprintln!("bench ritual: ledger += {entry}");
    let consecutive = entry_consecutive(&entry);
    if consecutive >= 3 {
        eprintln!("==================================================================");
        eprintln!("  M2 DECLARATION THRESHOLD MET — three consecutive holds (s44).");
        eprintln!("  The declaration itself is the human's announcement; this tool");
        eprintln!("  states the fact and the ledger holds the evidence.");
        eprintln!("==================================================================");
    }

    // --------------------------------------------- bench-data append --
    bench_data_append(&ts, &commit, &[&cond_path, &jsonl, &gate_path]);

    ExitCode::SUCCESS
}

/// One ledger line, with the consecutive count computed from the
/// ledger's own tail under the header's rule.
fn ledger_entry(ts: &str, commit: &str, geomean: Option<f64>, verdict: &str, note: &str) -> String {
    let (consecutive, sep_note) = next_consecutive(ts, verdict);
    let note = if note.is_empty() {
        sep_note
    } else if sep_note.is_empty() {
        note.to_string()
    } else {
        format!("{note}; {sep_note}")
    };
    let g = geomean
        .map(|g| format!("{g:.3}"))
        .unwrap_or_else(|| "null".into());
    format!(
        "{{\"ts\":\"{ts}\",\"commit\":\"{commit}\",\"geomean\":{g},\"verdict\":\"{verdict}\",\"consecutive\":{consecutive}{}}}",
        if note.is_empty() {
            String::new()
        } else {
            format!(",\"note\":\"{note}\"")
        }
    )
}

fn entry_consecutive(entry: &str) -> u64 {
    serde_json::from_str::<serde_json::Value>(entry)
        .ok()
        .and_then(|v| v["consecutive"].as_u64())
        .unwrap_or(0)
}

/// The tick rule, as the ledger header states it: a non-hold resets to
/// zero; a hold advances the count only when >= 12h separate it from
/// the last COUNTED hold (s44 says nightly; two samples of one thermal
/// state are one tick); a closer hold keeps the count with a note.
fn next_consecutive(ts: &str, verdict: &str) -> (u64, String) {
    if verdict != "HOLDS" {
        return (0, String::new());
    }
    let tail = last_entry();
    let Some(prev) = tail else {
        return (1, String::new());
    };
    let prev_verdict = prev["verdict"].as_str().unwrap_or("");
    let prev_count = prev["consecutive"].as_u64().unwrap_or(0);
    if prev_verdict != "HOLDS" || prev_count == 0 {
        return (1, String::new());
    }
    let gap = match (
        iso_to_epoch(prev["ts"].as_str().unwrap_or("")),
        iso_to_epoch(ts),
    ) {
        (Some(a), Some(b)) => b - a,
        _ => 0,
    };
    if gap >= NIGHT_SECONDS {
        (prev_count + 1, String::new())
    } else {
        (
            prev_count,
            format!(
                "same-night re-render ({}m after the counted hold); count unchanged",
                gap / 60
            ),
        )
    }
}

fn last_entry() -> Option<serde_json::Value> {
    let body = std::fs::read_to_string(LEDGER).ok()?;
    body.lines()
        .rfind(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .and_then(|l| serde_json::from_str(l).ok())
}

fn append_line(path: &str, line: &str) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .expect("open ritual ledger");
    writeln!(f, "{line}").expect("append ritual ledger");
}

/// Append the ritual's artifacts to the bench-data branch the way the
/// nightly's bench-full job does. Any failure is a NAMED skip — the
/// measurement is already on disk and in the ledger; the branch is the
/// archive, not the record of truth.
fn bench_data_append(ts: &str, commit: &str, files: &[&PathBuf]) {
    let day = &ts[..10];
    let wt = std::env::temp_dir().join(format!("ritual-bd-{commit}"));
    let wt_s = wt.display().to_string();
    let steps: &[(&str, Vec<&str>)] = &[
        ("git", vec!["fetch", "origin", "bench-data"]),
        ("git", vec!["worktree", "add", &wt_s, "origin/bench-data"]),
    ];
    for (cmd, args) in steps {
        if !run_ok(cmd, args) {
            eprintln!(
                "bench ritual: bench-data append SKIPPED — `{cmd} {}` failed \
                 (no branch, no credentials, or offline); the ledger and {day} artifacts remain local",
                args.join(" ")
            );
            let _ = run_ok("git", &["worktree", "remove", "--force", &wt_s]);
            return;
        }
    }
    let dest = wt.join("data").join(day).join("ritual");
    std::fs::create_dir_all(&dest).expect("mkdir bench-data dest");
    for f in files {
        let name = f.file_name().expect("artifact name");
        std::fs::copy(f, dest.join(name)).expect("copy artifact to bench-data");
    }
    let ledger_dest = dest.join("ritual-ledger.jsonl");
    let _ = std::fs::copy(LEDGER, ledger_dest);
    let msg = format!("ritual {ts} ({commit})");
    let pushed = run_ok("git", &["-C", &wt_s, "add", "data"])
        && run_ok("git", &["-C", &wt_s, "commit", "-m", &msg])
        && run_ok("git", &["-C", &wt_s, "push", "origin", "HEAD:bench-data"]);
    if pushed {
        eprintln!("bench ritual: bench-data += data/{day}/ritual/");
    } else {
        eprintln!(
            "bench ritual: bench-data append SKIPPED at commit/push (credentials?); \
             artifacts remain local under {}",
            files[0].parent().unwrap_or(Path::new(".")).display()
        );
    }
    let _ = run_ok("git", &["worktree", "remove", "--force", &wt_s]);
}

// ------------------------------------------------------------ probes --

fn load1() -> f64 {
    read_trim("/proc/loadavg")
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(f64::MAX)
}

/// Compiler processes not in this ritual's own ancestry. Matching on
/// comm (not args) keeps daemons whose ARGUMENTS name compilers — the
/// earlyoom --prefer regex — out of the count by construction.
fn compiler_processes_beyond_self() -> Vec<String> {
    let mut ancestry = std::collections::HashSet::new();
    let mut pid = std::process::id();
    for _ in 0..32 {
        ancestry.insert(pid);
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        // field 4 is ppid; comm (field 2) may contain spaces, so parse
        // from after the closing paren.
        let Some(rest) = stat.rsplit_once(") ").map(|(_, r)| r) else {
            break;
        };
        let Some(ppid) = rest
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u32>().ok())
        else {
            break;
        };
        if ppid <= 1 {
            break;
        }
        pid = ppid;
    }
    let Some(out) = run_capture("ps", &["-eo", "pid=,comm="]) else {
        return vec!["(ps unavailable — cannot prove quiet)".into()];
    };
    let mut hits = Vec::new();
    for line in out.lines() {
        let mut it = line.split_whitespace();
        let (Some(pid_s), Some(comm)) = (it.next(), it.next()) else {
            continue;
        };
        let Ok(p) = pid_s.parse::<u32>() else {
            continue;
        };
        if ancestry.contains(&p) {
            continue;
        }
        if matches!(
            comm,
            "cargo" | "rustc" | "xtask" | "clang" | "cc1" | "ld" | "lld"
        ) {
            hits.push(format!("{comm}({p})"));
        }
    }
    hits
}

fn which(bin: &str) -> bool {
    std::process::Command::new("which")
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn read_trim(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "n/a".into())
}

fn run_capture(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_ok(cmd: &str, args: &[&str]) -> bool {
    std::process::Command::new(cmd)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ------------------------------------------------------------- time --

fn utc_now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before epoch")
        .as_secs() as i64;
    epoch_to_iso(secs)
}

/// Minimal UTC ISO-8601 (YYYY-MM-DDTHH:MMZ, the conditions files'
/// spelling) — both directions, no dependency (D15 temperament even
/// where D15 does not bind).
fn epoch_to_iso(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60
    )
}

fn iso_to_epoch(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 16 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hh, mm) = (num(11..13)?, num(14..16)?);
    Some(days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60)
}

// Howard Hinnant's civil-days algorithms, integer-only.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_round_trips() {
        for s in [
            "2026-08-20T22:45Z",
            "2026-08-22T04:11Z",
            "2000-03-01T00:00Z",
        ] {
            let e = iso_to_epoch(s).expect("parse");
            assert_eq!(epoch_to_iso(e), s);
        }
    }

    #[test]
    fn a_night_is_twelve_hours() {
        let a = iso_to_epoch("2026-08-22T03:53Z").unwrap();
        let b = iso_to_epoch("2026-08-22T04:11Z").unwrap();
        assert!((b - a) < NIGHT_SECONDS, "18 minutes is not a night");
        let c = iso_to_epoch("2026-08-23T03:53Z").unwrap();
        assert!((c - a) >= NIGHT_SECONDS, "a day later is");
    }
}
