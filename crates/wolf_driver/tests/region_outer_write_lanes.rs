//! s255 (wolf-lang#839, #856, #821, #844; `[mem.region.outer-write]`,
//! `[type.list.reserve]`, `[type.list.reset]`) — the loop's region:
//! every row on every machine, lupin's column beside it, the three
//! refusals' reports, and the resident set of yew's turn at 100,000 and
//! 200,000 turns.
//!
//! `cargo xtask corpus` runs a `phase: run` row natively and a refusal
//! row on the default lane; this gate runs each of s255's rows on the
//! checked machine, native and release, and asserts one answer. lupin
//! 0.1.55 and 0.1.56 are pinned by their published records (#289): each shares
//! `copy`'s bytes, so it traps `region-fault` where a copied `str` is
//! read back after its block; it tracks the list itself and never pinned
//! a `mut` argument, so the scalar-push, index and clean-call rows run;
//! it resolves neither `reserve` nor `reset`. Every unlisted release
//! must follow the run rows' compiler answer; negative controls keep
//! their explicit static/dynamic comparison.
//!
//! The resident-set rows (`[mem.region.outer-write]`'s point): the
//! fixture `fixtures/outer_write/main.lu` is `region_outer_write_call.lu`
//! taking its turn count from the command line; a turn's cost is what
//! the extra N turns add to the peak resident set, divided by N — the
//! OS's own high-water mark of exactly this child (`wait4(2)`'s
//! `ru_maxrss`), never a ledger. The control without the block keeps
//! every turn's string (yew's measured shape, #839: hundreds of bytes
//! a key); the block keeps nothing, so the row's bound is 4 B a turn on
//! both tiers.
//!
//! s270b (wolf-lang#948) adds the stores the clause refuses that s255's
//! push left open — a PLAIN `push` of a value whose copy reaches the heap
//! (a `List`, a struct holding one) into an outer list, in the block or
//! through a callee's `mut` parameter: wolf-lang#385's copy is made in the
//! ambient region, so the list kept the block's header — and the repaired
//! heap at the observed threshold's two sides, the issue's control and
//! its own size. lupin 0.1.58 runs the repaired heap's block form to the
//! compilers' answer and traps `region-fault` on the one copy it does not
//! mirror yet, `push(copy …)` through a `mut` parameter (pinned below by
//! its record; an unlisted release must mirror).

#![cfg(unix)]

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// wolf-interp#289: only these published records retain the pre-mirror
/// outer-copy answer. An unlisted release or dev commit must mirror wolf.
fn copy_before_mirror(version: &str, commit: &str) -> bool {
    [("0.1.55", "e642f26"), ("0.1.56", "28547b4")]
        .iter()
        .any(|(v, c)| *v == version && commit.starts_with(c))
}

/// s270b: the published lupin records whose `push(copy …)` through a `mut`
/// parameter lands in the caller's block (measured: 0.1.58 traps
/// `region-fault` on `region_outer_write_push_copy_call.lu`). An unlisted
/// release or dev commit must mirror the compilers.
fn param_copy_before_mirror(version: &str, commit: &str) -> bool {
    [("0.1.58", "6240efe")]
        .iter()
        .any(|(v, c)| *v == version && commit.starts_with(c))
}

#[test]
fn the_copy_pin_names_only_the_measured_release_records() {
    assert!(copy_before_mirror("0.1.55", "e642f26"));
    assert!(copy_before_mirror("0.1.56", "28547b4"));
    assert!(!copy_before_mirror("0.1.57", "28547b4"));
    assert!(!copy_before_mirror("0.1.56", "unknown"));
    assert!(!copy_before_mirror("0.1.57", "fb054c8"));
    assert!(!copy_before_mirror("0.1.58", "6240efe"));
    assert!(param_copy_before_mirror("0.1.58", "6240efe"));
    assert!(!param_copy_before_mirror("0.1.58", "unknown"));
    assert!(!param_copy_before_mirror("0.1.59", "6240efe"));
    assert!(!param_copy_before_mirror("0.1.57", "fb054c8"));
}

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn report(line: &str) {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    diags: Vec<serde_json::Value>,
    version: String,
    commit: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        commit: rec["commit"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        diags: rec["diagnostics"].as_array().cloned().unwrap_or_default(),
    }
}

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
             (LUPIN={}) — the lupin column of s255's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the three wolfgang \
             lanes are still asserted; only lupin's column is absent"
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

fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.is_file(), "corpus row missing: {}", p.display());
    p
}

/// What lupin 0.1.55 answers, when it is not the three lanes'.
enum Lupin {
    Same,
    Verdict(&'static str),
    /// s270b: what a [`param_copy_before_mirror`] record answers for a
    /// `push(copy …)` through a `mut` parameter; an unlisted lupin mirrors.
    ParamCopy(&'static str),
}

/// A run row: exit(0) and `want` on the checked machine, native and
/// release; lupin's column as measured.
fn run_row(rel: &str, want: &str, lupin: Lupin, why: &str) {
    let entry = corpus(rel);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "{rel} on {flag}: {why}");
        assert_eq!(obs.stdout, want, "{rel} on {flag}: the answer");
    }
    if let Some(l) = lupin_says(&entry) {
        match lupin {
            Lupin::Same => {
                assert_eq!(l.verdict, "exit(0)", "{rel}: lupin's verdict");
                assert_eq!(l.stdout, want, "{rel}: lupin's answer");
            }
            Lupin::Verdict(v) if copy_before_mirror(&l.version, &l.commit) => {
                assert_eq!(l.verdict, v, "{rel}: measured pre-mirror lupin");
            }
            Lupin::ParamCopy(v) if param_copy_before_mirror(&l.version, &l.commit) => {
                assert_eq!(l.verdict, v, "{rel}: measured pre-mirror lupin (s270b)");
            }
            Lupin::Verdict(_) | Lupin::ParamCopy(_) => {
                assert_eq!(
                    l.verdict, "exit(0)",
                    "{rel}: unlisted lupin must mirror #289"
                );
                assert_eq!(l.stdout, want, "{rel}: unlisted lupin's mirrored output");
            }
        }
    }
}

/// A refusal row: `code` on every compiling machine (the observation
/// record carries the code and the span), and every `needle` in the
/// RENDERED report `wolf build` prints (the message, the labels and the
/// notes — what a reader sees); lupin's column as measured.
fn fail_row(rel: &str, code: &str, needles: &[&str], lupin: Lupin, why: &str) {
    let entry = corpus(rel);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict,
            format!("fail({code})"),
            "{rel} on {flag}: {why}"
        );
        assert!(
            obs.diags.iter().any(|d| d["code"].as_str() == Some(code)),
            "{rel} on {flag}: a {code} in the record"
        );
    }
    let dir = scratch(&format!("report-{}", rel.replace('/', "-")));
    let out = Command::new(wolf())
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(dir.join("refused"))
        .arg("--no-cache")
        .output()
        .expect("wolf build runs");
    assert!(
        !out.status.success(),
        "{rel}: `wolf build` should refuse it"
    );
    let text = String::from_utf8_lossy(&out.stderr);
    for needle in needles {
        assert!(
            text.contains(needle),
            "{rel}: the report should say {needle:?}; got:\n{text}"
        );
    }
    if let Some(l) = lupin_says(&entry) {
        match lupin {
            Lupin::Same => assert_eq!(l.verdict, format!("fail({code})"), "{rel}: lupin"),
            Lupin::Verdict(v) | Lupin::ParamCopy(v) => {
                assert_eq!(l.verdict, v, "{rel}: lupin's verdict (its own)")
            }
        }
    }
}

#[test]
fn a_scalar_store_and_push_into_an_outer_value_keep_nothing_of_the_block() {
    run_row(
        "memory/region_outer_write_scalar.lu",
        "1000 1000 7 4\n",
        Lupin::Same,
        "a scalar field store, an int index store, a byte push and an int push into outer \
         values inside a region block are admitted; the growth is the list's region's",
    );
}

#[test]
fn a_copied_str_pushed_into_an_outer_list_is_materialized_there() {
    run_row(
        "memory/region_outer_write_str_push.lu",
        "100 turn 0 turn 99 42 true\n",
        Lupin::Verdict("trap(region-fault)"),
        "push(copy s) into an outer List[str] re-makes the bytes in the list's region",
    );
}

#[test]
fn a_copied_struct_holding_a_list_and_a_str_is_deep_copied_into_the_outer_region() {
    run_row(
        "memory/region_outer_write_struct.lu",
        "50 r0 r49 3 49 98 147\n",
        Lupin::Verdict("trap(region-fault)"),
        "push(copy row) deep-copies the name's bytes and the list's buffer into the outer region",
    );
}

#[test]
fn a_field_copied_through_a_mut_parameter_lands_in_the_entry_ambient() {
    run_row(
        "memory/region_outer_write_field.lu",
        "t9 1 9 x1 2 7\n",
        Lupin::Verdict("trap(region-fault)"),
        "s.title = copy t and s.last = copy xs inside the callee's own region block; a local's \
         field written from a block in main",
    );
}

#[test]
fn a_map_store_copies_its_key_and_value_into_the_map_region() {
    run_row(
        "memory/region_outer_write_map.lu",
        "40 v7 v39 true\n",
        Lupin::Verdict("trap(region-fault)"),
        "m[copy k] = copy v into an outer map materializes both",
    );
}

#[test]
fn an_index_store_with_copy_materializes_the_element() {
    run_row(
        "memory/region_outer_write_index.lu",
        "4 n28 n29 n30 n31\n",
        Lupin::Same,
        "names[i % 4] = copy t into an outer list",
    );
}

#[test]
fn a_clean_callee_runs_inside_the_turn_region() {
    run_row(
        "memory/region_outer_write_call.lu",
        "2000 20 7 2\n",
        Lupin::Same,
        "yew's turn: render(mut s, i) inside region turn { } — the parameter is ambient-clean",
    );
}

#[test]
fn reset_then_pushes_rebuild_a_long_lived_list_inside_the_turn() {
    run_row(
        "memory/region_outer_write_reset.lu",
        "300 3 299 0 6\n",
        Lupin::Verdict("unsupported"),
        "#821's rebuild: reset stores nothing, the pushes land in the list's region",
    );
}

#[test]
fn the_ledger_charges_the_copies_to_the_outer_region() {
    run_row(
        "memory/region_outer_write_charged.lu",
        "outer true | turn true | lines 200 | reclaimed true\n",
        Lupin::Same,
        "region_bytes(outer) holds the copied lines, region_bytes(turn) the scratch; the root \
         gets nothing",
    );
}

#[test]
fn list_reserve_grows_the_capacity_and_nothing_else() {
    run_row(
        "memory/list_reserve.lu",
        "0 1000 0 999 3 7 8 9 true\n",
        Lupin::Verdict("unsupported"),
        "reserve(n) leaves the length and elements alone; a smaller n is a no-op",
    );
}

#[test]
fn list_reset_empties_and_keeps_the_buffer() {
    run_row(
        "memory/list_reset.lu",
        "5 0 3 10 11 12 2 b2 true\n",
        Lupin::Verdict("unsupported"),
        "reset() takes the length to 0, the next pushes reuse the storage, the ledger does not move",
    );
}

#[test]
fn a_plain_str_push_into_an_outer_list_is_refused_naming_copy() {
    fail_row(
        "memory/region_outer_write_refused.lu",
        "E1010",
        &["push(copy t)", "mem.region.outer-write"],
        Lupin::Verdict("exit(0)"),
        "the plain push shares the block's bytes; the report offers copy at the store",
    );
}

#[test]
fn a_plain_field_store_of_a_block_value_is_refused_naming_copy() {
    fail_row(
        "memory/region_outer_write_refused_field.lu",
        "E1004",
        &["mem.region.outer-write", "written `copy"],
        Lupin::Verdict("trap(region-fault)"),
        "the cross-region field store, with the clause's repair named",
    );
}

#[test]
fn an_unclean_callee_is_refused_naming_its_store() {
    fail_row(
        "memory/region_outer_write_call_refused.lu",
        "E1010",
        &[
            "`stash` keeps a value built in its caller's region in its `mut` parameter `s`",
            "ambient-clean",
        ],
        Lupin::Verdict("trap(region-fault)"),
        "stash stores a str it builds into a flat field of its mut parameter",
    );
}

#[test]
fn the_rewritten_stand_in_still_refuses_and_names_the_callee() {
    fail_row(
        "memory/region_scalar_list_mut.lu",
        "E1010",
        &["`grow` keeps a value built in its caller's region"],
        Lupin::Verdict("exit(0)"),
        "grow pushes a plain str it builds into h.names: not ambient-clean",
    );
}

// ------------------------------------- s270b: the push's copy (#948) --

#[test]
fn a_callee_pushing_a_slot_it_builds_into_the_heap_is_refused_naming_its_store() {
    fail_row(
        "memory/region_outer_write_push_heap_call.lu",
        "E1010",
        &[
            "`alloc` keeps a value built in its caller's region in its `mut` parameter `h`",
            "a container store of a scalar or of a `copy`",
        ],
        Lupin::Verdict("trap(region-fault)"),
        "yew's heap at its smallest: the plain push copies the slot in the block, so alloc's \
         parameter is not ambient-clean",
    );
}

#[test]
fn a_plain_push_of_a_block_built_slot_into_an_outer_list_is_refused_naming_copy() {
    fail_row(
        "memory/region_outer_write_push_heap.lu",
        "E1010",
        &[
            "`slots` still holds a value allocated in region `scratch`",
            "written `copy e` at the store",
        ],
        Lupin::Verdict("trap(region-fault)"),
        "the same store in the block: the slot's List header is the block's",
    );
}

#[test]
fn a_plain_push_copies_an_outer_list_into_the_block_and_is_refused() {
    fail_row(
        "memory/region_outer_write_push_plain_copy.lu",
        "E1010",
        &[
            "`outs` still holds a value allocated in region `scratch`",
            "mem.region.outer-write",
        ],
        Lupin::Verdict("exit(0)"),
        "x is built outside the block, but the push's copy of it is made inside: native and \
         release read the next region's list through the freed header",
    );
}

#[test]
fn the_repaired_heap_answers_just_under_the_threshold() {
    run_row(
        "memory/region_outer_write_push_copy_under.lu",
        "4 1024 19 16 4 0 2000\n",
        Lupin::Same,
        "push(copy empty()) into the outer heap, 16 list elements, read after a reused chunk \
         and a collection",
    );
}

#[test]
fn the_repaired_heap_answers_just_over_the_threshold() {
    run_row(
        "memory/region_outer_write_push_copy_over.lu",
        "4 1024 19 17 4 0 2000\n",
        Lupin::Same,
        "17 list elements: the third doubling, where yew's unrepaired witness first trapped",
    );
}

#[test]
fn the_repaired_heap_answers_the_issues_one_kib_control() {
    run_row(
        "memory/region_outer_write_push_copy_control.lu",
        "4 1024 19 2 4 0 2000\n",
        Lupin::Same,
        "the issue's 1,024-byte, 2-element control",
    );
}

#[test]
fn the_repaired_allocator_called_in_the_block_answers_at_the_issues_size() {
    run_row(
        "memory/region_outer_write_push_copy_call.lu",
        "4 100000 101 64 4 0 2000\n",
        Lupin::ParamCopy("trap(region-fault)"),
        "push(copy empty()) through alloc's mut parameter is ambient-clean; 100,000 bytes and \
         64 list elements",
    );
}

// ------------------------------------------------- the resident set --

fn scratch(test: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("region_outer_write_lanes")
        .join(test);
    if d.exists() {
        std::fs::remove_dir_all(&d).expect("old scratch removed");
    }
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// The witness built for one tier; a compile error fails the gate.
fn build(dir: &Path, release: bool) -> Option<PathBuf> {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/outer_write/main.lu");
    let tier = if release { "release" } else { "native" };
    let exe = dir.join(format!("outer-write-{tier}"));
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    if lane_exit::environment_refusal(&out, "wolf build") {
        eprintln!(
            "SKIP: the {tier} tier cannot build here: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "wolf build ({tier}) refused the witness: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(exe)
}

/// One run: the program's stdout and the child's peak resident set in
/// bytes, from `wait4(2)` on exactly this child.
fn peak(exe: &Path, dir: &Path, mode: &str, size: &str) -> (String, u64) {
    let mut child = Command::new(exe)
        .args([mode, size])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the witness");
    let mut stdout = String::new();
    {
        use std::io::Read as _;
        let mut out = child.stdout.take().expect("piped stdout");
        out.read_to_string(&mut stdout).expect("read stdout");
    }
    let (status, bytes) = reap_with_peak(child);
    assert!(
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "the witness {} {mode} {size} did not exit 0 (status {status})",
        exe.display()
    );
    (stdout, bytes)
}

/// Reap `child` with `wait4(2)`: its raw status and its peak resident set
/// in bytes (`ru_maxrss`: kibibytes on linux, bytes on macOS).
fn reap_with_peak(child: std::process::Child) -> (libc::c_int, u64) {
    let pid = child.id() as libc::pid_t;
    let mut status: libc::c_int = 0;
    // SAFETY: a zeroed `rusage` is a valid out-parameter; `wait4` fills it
    // for the child this test spawned and has not waited for.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: plain FFI on a pid this process spawned.
    let reaped = unsafe { libc::wait4(pid, &mut status, 0, &mut ru) };
    assert_eq!(reaped, pid, "wait4 reaps exactly the child");
    let maxrss = ru.ru_maxrss as u64;
    let bytes = if cfg!(target_os = "macos") {
        maxrss
    } else {
        maxrss * 1024
    };
    (status, bytes)
}

/// The per-turn cost on one tier: the block's and the control's, in
/// bytes a turn over the second 100,000 turns.
fn per_turn(release: bool) {
    let tier = if release { "release" } else { "native" };
    let dir = scratch(&format!("peak-{tier}"));
    let Some(exe) = build(&dir, release) else {
        return;
    };
    let mut rows: Vec<(&str, i64)> = Vec::new();
    for mode in ["region", "none"] {
        let (o1, p1) = peak(&exe, &dir, mode, "x");
        let (o2, p2) = peak(&exe, &dir, mode, "xx");
        assert_eq!(o1.trim(), "100000 23 23", "{tier} {mode} x: the answer");
        assert_eq!(o2.trim(), "200000 26 26", "{tier} {mode} xx: the answer");
        let cost = (p2 as i64 - p1 as i64) / 100_000;
        report(&format!(
            "REPORT outer-write {tier} {mode}: 100000 turns {} KB, 200000 turns {} KB, {cost} B a turn",
            p1 / 1024,
            p2 / 1024
        ));
        rows.push((mode, cost));
    }
    let region = rows
        .iter()
        .find(|(m, _)| *m == "region")
        .map(|(_, c)| *c)
        .unwrap();
    let none = rows
        .iter()
        .find(|(m, _)| *m == "none")
        .map(|(_, c)| *c)
        .unwrap();
    // The block keeps nothing of the turn: at most the measurement's
    // noise (4 B a turn, the bound cwd_region_lanes uses). The control
    // keeps the turn's string (a 23-byte interpolation and the
    // arena's rounding): far above the bound, so the row is reading
    // the block's work and not the allocator's.
    assert!(
        region <= 4,
        "{tier}: the per-turn region keeps {region} B a turn (bound 4): the block's work is kept"
    );
    assert!(
        none > 4,
        "{tier}: the control without a region keeps {none} B a turn; the row cannot tell the \
         block from no block"
    );
}

#[test]
fn the_turn_region_runs_flat_on_the_native_tier() {
    per_turn(false);
}

#[test]
fn the_turn_region_runs_flat_on_the_release_tier() {
    per_turn(true);
}
