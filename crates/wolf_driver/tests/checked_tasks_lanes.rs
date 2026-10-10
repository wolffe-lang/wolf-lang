//! s226 (C1) — structured concurrency on the checked machine, held to
//! lupin: `[exec.checked.task]`, `[exec.checked.sched]`,
//! `[exec.checked.race]`.
//!
//! Until s226 `wolf conform-run --checked` refused `scope`, `spawn`,
//! `select`, `when`, closures and `par` by name ("structured
//! concurrency in checked execution (C1 deferred)"), so the concurrency
//! tier was checked by two machines: the compiled tiers and lupin. It
//! now runs them under a deterministic scheduler with a race detector,
//! and these gates hold it to the reference interpreter:
//!
//! - every run row of `corpus/conc/` reaches a verdict on the checked
//!   machine (none is `unsupported`), and the verdict and the output
//!   are lupin's;
//! - a data race is `trap(race)` at `[conc.mm.race.3]`, at lupin's
//!   span; a race-free program is accepted; a deadlock is
//!   `trap(deadlock)` by name; `par`'s results come back in order;
//! - the same seed is the same run, and the seed reaches the scheduler.
//!
//! Measured at trunk 76436101 (hasu, `~/lanes/s226/logs/table-trunk.tsv`
//! 8d451da1…): thirty corpus rows answered the C1 refusal on the
//! checked machine, five more "closures in checked execution", two
//! "this List method" (`par`) and two "freeze of a non-region value".
//!
//! Two witnesses part from lupin 0.1.49, each pinned by that version so
//! a newer lupin is held to the clause: the rendezvous back-edge
//! (`[conc.mm.hb.chan]`, lupin adds none and traps `race`) and a
//! self-acquisition reached through a closure (`[conc.deadlock.self]`,
//! lupin traps `exclusivity` first).
//!
//! The fixtures that race or deadlock are not corpus rows: the compiled
//! tiers run a race as whatever the hardware does and wait on a
//! deadlock (`[conc.deadlock.trap]` permits it).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The lupin release whose two partings are measured and pinned.
const PINNED_LUPIN: &str = "0.1.49";

#[derive(Debug, Clone)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    unsupported: String,
    clause: String,
    span: String,
    seeded: bool,
    stderr: String,
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
            .or(rec["x-unsupported"].as_str())
            .unwrap_or("")
            .to_string(),
        clause: s("x-trap-clause"),
        span: rec["x-trap-span"].to_string(),
        seeded: rec["seeded"].as_bool().unwrap_or(false),
        stderr: String::from_utf8_lossy(stderr).into_owned(),
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/checked_tasks")
        .join(name)
        .join("main.lu")
}

fn corpus(rel: &str) -> PathBuf {
    root().join("corpus").join(rel)
}

/// `wolf conform-run <entry> --checked --json [--seed=N]`: the checked
/// machine's own answer. (`wolf run --checked` is the native build.)
fn checked(entry: &Path, seed: Option<u64>) -> Obs {
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--checked")
        .arg("--json");
    if let Some(seed) = seed {
        cmd.arg(format!("--seed={seed}"));
    }
    let out = cmd.output().expect("wolf runs");
    assert!(
        out.status.success(),
        "conform-run --checked failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    parse_obs(&out.stdout, &out.stderr, "the checked observation")
}

fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

/// A compiled lane; `None` is the s59 environment skip, named on
/// stderr.
fn compiled(entry: &Path, release: bool) -> Option<Obs> {
    ensure_rt_staticlib();
    let mut cmd = Command::new(wolf());
    cmd.arg("conform-run")
        .arg(entry)
        .arg("--native")
        .arg("--json");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf runs");
    let what = if release { "--release" } else { "--native" };
    if lane_exit::environment_refusal(&out, &format!("wolf {what}")) {
        eprintln!(
            "SKIP: environment cannot run the {what} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {what} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let obs = parse_obs(&out.stdout, &out.stderr, "the compiled observation");
    if obs.verdict == "unsupported" {
        eprintln!(
            "SKIP: the {what} lane declines {} on this host: {}",
            entry.display(),
            obs.stderr.trim()
        );
        return None;
    }
    Some(obs)
}

/// The sibling lupin, found exactly as `pairing.rs` finds it.
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

fn lupin(entry: &Path, seed: Option<u64>) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (set LUPIN, or build ../wolf-interp)"
        );
        eprintln!("SKIP: no sibling lupin — the lupin half of this gate did not run");
        return None;
    };
    let mut cmd = Command::new(lupin);
    cmd.arg("conform-run").arg(entry).arg("--json");
    if let Some(seed) = seed {
        cmd.arg(format!("--seed={seed}"));
    }
    let out = cmd.output().expect("lupin runs");
    Some(parse_obs(&out.stdout, &out.stderr, "lupin's observation"))
}

/// The checked machine and lupin answer one verdict, clause, span and
/// output on `entry`.
fn agrees_with_lupin(entry: &Path, want_verdict: &str, want_clause: &str) -> Obs {
    let c = checked(entry, None);
    assert_eq!(
        c.verdict,
        want_verdict,
        "checked on {}: {} {}",
        entry.display(),
        c.unsupported,
        c.stderr
    );
    assert_eq!(
        c.clause,
        want_clause,
        "checked clause on {}",
        entry.display()
    );
    if let Some(l) = lupin(entry, None) {
        assert_eq!(
            (
                l.verdict.as_str(),
                l.clause.as_str(),
                l.span.as_str(),
                l.stdout.as_str()
            ),
            (
                c.verdict.as_str(),
                c.clause.as_str(),
                c.span.as_str(),
                c.stdout.as_str()
            ),
            "lupin {} and the checked machine on {}",
            l.version,
            entry.display()
        );
    }
    c
}

/// `[exec.checked.race]` on the corpus's own racy row: four tasks write
/// one word with a plain `p[0] = p[0] + 1`. `trap(race)` at
/// `[conc.mm.race.3]`, at the span lupin reports, with both tasks named
/// on stderr. Red at trunk: `unsupported`, the C1 refusal.
#[test]
fn the_plain_counter_is_a_race() {
    let c = agrees_with_lupin(
        &corpus("conc/atomic_race_plain.lu"),
        "trap(race)",
        "conc.mm.race.3",
    );
    assert!(
        c.stderr.contains("data race on allocation") && c.stderr.contains("(task 1)"),
        "the race names its memory and both tasks on stderr: {}",
        c.stderr
    );
    assert_eq!(c.stdout, "", "nothing is printed after the race");
}

/// A channel-less shared write: two tasks store to one word and nothing
/// orders the stores. Reported at the second store's own span.
#[test]
fn a_channel_less_shared_write_is_a_race() {
    agrees_with_lupin(
        &fixture("race_shared_write"),
        "trap(race)",
        "conc.mm.race.3",
    );
}

/// `[conc.mm.race.1]`: two atomic accesses never race; an atomic and an
/// unordered plain one do.
#[test]
fn an_atomic_and_a_plain_access_race() {
    agrees_with_lupin(
        &fixture("race_atomic_plain"),
        "trap(race)",
        "conc.mm.race.3",
    );
}

/// The same memory, the same two writers, an order between them: a
/// channel hand-off (`[conc.mm.hb.chan]`) and a `when` critical section
/// (`[conc.mm.hb.mutex]`). Accepted, with the count, on four machines.
#[test]
fn a_race_free_program_is_accepted() {
    for (name, out) in [("race_free_channel", "2\n"), ("race_free_mutex", "4 4\n")] {
        let entry = fixture(name);
        let c = agrees_with_lupin(&entry, "exit(0)", "");
        assert_eq!(c.stdout, out, "{name} on the checked machine");
        for release in [false, true] {
            if let Some(obs) = compiled(&entry, release) {
                assert_eq!(
                    (obs.verdict.as_str(), obs.stdout.as_str()),
                    ("exit(0)", out),
                    "{name}, release={release}"
                );
            }
        }
    }
    // The exact counter: 500,000 atomic operations across eight tasks
    // and two scopes, inside the step budget.
    let c = agrees_with_lupin(&corpus("conc/atomic_counter.lu"), "exit(0)", "");
    assert_eq!(c.stdout, "400000 100000\n");
}

/// `[conc.mm.hb.chan]`: "for unbuffered channels the receive also
/// happens-before the send *returns*". The receiver writes before it
/// receives, the sender reads after its send returned; only that
/// sentence orders the two. The checked machine and the compiled tiers
/// run it; lupin 0.1.49 has no such edge and traps `race` (measured,
/// pinned by version; a newer lupin is held to the clause).
#[test]
fn the_rendezvous_receive_orders_the_senders_next_access() {
    let entry = fixture("race_free_rendezvous");
    let c = checked(&entry, None);
    assert_eq!(
        (c.verdict.as_str(), c.stdout.as_str()),
        ("exit(0)", "7\n"),
        "checked: {}",
        c.stderr
    );
    for release in [false, true] {
        if let Some(obs) = compiled(&entry, release) {
            assert_eq!(
                (obs.verdict.as_str(), obs.stdout.as_str()),
                ("exit(0)", "7\n")
            );
        }
    }
    let Some(l) = lupin(&entry, None) else { return };
    if l.version == PINNED_LUPIN {
        assert_eq!(
            (l.verdict.as_str(), l.clause.as_str()),
            ("trap(race)", "conc.mm.race.3"),
            "lupin {PINNED_LUPIN} has no rendezvous back-edge (measured at s226); \
             if this moved, drop the pin"
        );
    } else {
        assert_eq!(
            (l.verdict.as_str(), l.stdout.as_str()),
            ("exit(0)", "7\n"),
            "lupin {} must order a rendezvous receive before the send's return \
             ([conc.mm.hb.chan])",
            l.version
        );
    }
}

/// `[conc.deadlock.def]`, `[conc.deadlock.trap]`: two tasks each wait
/// for the other's message. `trap(deadlock)` at the clause and span
/// lupin reports, the blocked-task roster on stderr.
#[test]
fn a_deadlock_is_reported_by_name() {
    let c = agrees_with_lupin(
        &fixture("deadlock_two_tasks"),
        "trap(deadlock)",
        "conc.deadlock.trap",
    );
    assert!(
        c.stderr.contains("deadlock: every live task is blocked")
            && c.stderr.contains("`main` (task 0)")
            && c.stderr.contains("(task 1)")
            && c.stderr.contains("(task 2)"),
        "the roster names every blocked task: {}",
        c.stderr
    );
    assert_eq!(c.stdout, "", "`unreachable` is not printed");
}

/// `[conc.deadlock.self]`: an acquisition of a mutex the task already
/// holds, reached through a call, is `trap(deadlock)` at once. lupin
/// 0.1.49 answers `trap(exclusivity)` on the closure spelling (measured,
/// pinned by version).
#[test]
fn a_self_acquisition_through_a_call_is_a_deadlock() {
    let entry = fixture("deadlock_self_when");
    let c = checked(&entry, None);
    assert_eq!(
        (c.verdict.as_str(), c.clause.as_str()),
        ("trap(deadlock)", "conc.deadlock.self"),
        "checked: {}",
        c.stderr
    );
    let Some(l) = lupin(&entry, None) else { return };
    if l.version == PINNED_LUPIN {
        assert_eq!(
            l.verdict, "trap(exclusivity)",
            "lupin {PINNED_LUPIN} on a `when` reached through a closure that captured \
             the held mutex (measured at s226); if this moved, drop the pin"
        );
    } else {
        assert_eq!(
            (l.verdict.as_str(), l.clause.as_str()),
            ("trap(deadlock)", "conc.deadlock.self"),
            "lupin {}",
            l.version
        );
    }
}

/// Ruling #30 (`[exec.checked.task]`): each `par` worker builds its
/// rows — a `str` and a `List` apiece — in its own region, and after
/// the join every result is in the `par`'s region, in input order.
#[test]
fn par_results_move_back_in_order() {
    const OUT: &str = "1000 0 332833500 row-999 42 0\n";
    let entry = fixture("par_regions");
    let c = agrees_with_lupin(&entry, "exit(0)", "");
    assert_eq!(c.stdout, OUT);
    for release in [false, true] {
        if let Some(obs) = compiled(&entry, release) {
            assert_eq!(
                (obs.verdict.as_str(), obs.stdout.as_str()),
                ("exit(0)", OUT)
            );
        }
    }
    for rel in ["conc/par_order.lu", "conc/par_fail_reraises.lu"] {
        agrees_with_lupin(&corpus(rel), "exit(0)", "");
    }
}

/// `[exec.checked.sched]`, `[sched.stable]`: equal seeds are equal runs
/// — the same verdict and the same bytes — and the seed REACHES the
/// scheduler: with no seed three unordered tasks print in spawn order,
/// and some seed prints another order.
#[test]
fn the_same_seed_is_the_same_run() {
    let entry = fixture("seed_order");
    let unseeded = checked(&entry, None);
    assert_eq!(unseeded.stdout, "abc\n", "no seed: spawn order");
    assert!(!unseeded.seeded, "no seed, no `seeded`");
    assert_eq!(
        checked(&entry, Some(0)).stdout,
        "abc\n",
        "seed 0 is no seed"
    );
    let packed = (1u64 << 62) | 5;
    let mut lines = std::collections::BTreeSet::new();
    for seed in [1, 2, 3, 7, 42, 1009, 65_537, packed] {
        let a = checked(&entry, Some(seed));
        let b = checked(&entry, Some(seed));
        assert_eq!(
            (a.verdict.as_str(), a.stdout.as_str()),
            (b.verdict.as_str(), b.stdout.as_str()),
            "seed {seed} twice"
        );
        assert!(
            a.seeded,
            "a seeded checked record says so ([proto.seed.flag])"
        );
        assert_eq!(a.verdict, "exit(0)");
        let mut letters: Vec<char> = a.stdout.trim().chars().collect();
        letters.sort_unstable();
        assert_eq!(letters, ['a', 'b', 'c'], "seed {seed}: a permutation");
        lines.insert(a.stdout);
    }
    assert!(
        lines.len() > 1,
        "the seed must reach the scheduler: eight seeds gave one order, {lines:?}"
    );
    // A packed seed is its digits: 5 = 2 + 1·3 picks the third of three
    // ready tasks, then the second of the two left.
    assert_eq!(checked(&entry, Some(packed)).stdout, "cba\n");
}

/// Is this corpus file a run row, by its own header?
fn run_row(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .is_ok_and(|src| src.lines().take(3).any(|l| l.starts_with("//! check: run")))
}

fn conc_run_rows() -> Vec<PathBuf> {
    let mut rows: Vec<PathBuf> = std::fs::read_dir(corpus("conc"))
        .expect("corpus/conc")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "lu") && run_row(p))
        .collect();
    // The rows outside the tier that the C1 refusal also held dark.
    for rel in [
        "procs.lu",
        "net/spawn_accept.lu",
        "os/signal_supervisor.lu",
        "test/conc_schedules_test.lu",
        "typecheck/unit_context_discard.lu",
    ] {
        rows.push(corpus(rel));
    }
    rows.sort();
    rows
}

/// The per-row agreement: every concurrency run row reaches a verdict
/// on the checked machine — none is `unsupported`, C1 is retired — and
/// the verdict and the output are lupin's. One row is compared by
/// class, as its directive says (`run(exit=nonzero)`); one lupin
/// declines.
#[test]
fn every_conc_run_row_agrees_with_lupin() {
    let rows = conc_run_rows();
    assert!(rows.len() >= 35, "the tier has its rows: {}", rows.len());
    let mut compared = 0usize;
    for row in &rows {
        let name = row.strip_prefix(root().join("corpus")).unwrap_or(row);
        let name = name.to_string_lossy().replace('\\', "/");
        let c = checked(row, None);
        assert_ne!(
            c.verdict, "unsupported",
            "{name}: the checked machine declines — {}",
            c.unsupported
        );
        let Some(l) = lupin(row, None) else { continue };
        match name.as_str() {
            // `[conc.proc.root]`: nonzero, implementation-specified —
            // 121 here (the native runtime's number), 1 on lupin.
            "conc/proc_link_root_death.lu" => {
                assert_eq!(c.verdict, "exit(121)", "{name}");
                assert!(
                    l.verdict.starts_with("exit(") && l.verdict != "exit(0)",
                    "{name}: lupin {} answers the class: {}",
                    l.version,
                    l.verdict
                );
            }
            // lupin has no in-machine signal queue for a task to raise
            // into and declines the row by name.
            "os/signal_supervisor.lu" if l.verdict == "unsupported" => {
                assert_eq!(
                    (c.verdict.as_str(), c.stdout.as_str()),
                    ("exit(0)", "upgrade\n")
                );
            }
            _ => assert_eq!(
                (c.verdict.as_str(), c.stdout.as_str()),
                (l.verdict.as_str(), l.stdout.as_str()),
                "{name}: the checked machine against lupin {}",
                l.version
            ),
        }
        compared += 1;
    }
    eprintln!(
        "REPORT: {} rows on the checked machine, {compared} held to lupin",
        rows.len()
    );
}

/// X12's seed stability on the corpus rows (`[sched.stable]`): under
/// each seed the verdict and the output are what the unseeded run
/// gives, twice. (These rows' outputs are schedule-independent by their
/// own `check:` headers; the row whose line a seed does move is
/// `the_same_seed_is_the_same_run`'s fixture.)
#[test]
fn every_conc_run_row_is_seed_stable() {
    for row in conc_run_rows() {
        // Real sockets and real sleeps: one run is enough for them.
        let name = row.to_string_lossy().replace('\\', "/");
        if name.ends_with("net/spawn_accept.lu") {
            continue;
        }
        let base = checked(&row, None);
        for seed in [1, 7, 42] {
            for round in 0..2 {
                let obs = checked(&row, Some(seed));
                assert_eq!(
                    (obs.verdict.as_str(), obs.stdout.as_str()),
                    (base.verdict.as_str(), base.stdout.as_str()),
                    "{name} under seed {seed}, round {round}"
                );
            }
        }
    }
}
