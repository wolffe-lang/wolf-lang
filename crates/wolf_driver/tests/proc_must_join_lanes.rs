//! s233 (wolf-lang#678's helper half) — a function that may leave a proc
//! running with its caller's bytes is held to the frame rule at every
//! call.
//!
//! s229 refused a `spawn proc` argument whose bytes live in a region
//! local to the spawning frame, and passed a parameter's: a parameter
//! outlives the frame. The proc may outlive it too. `fn helper(s: str) {
//! let _w = spawn proc child(s, 0) }` called from `region pass {
//! helper(s) }` returned while `child` slept, the block freed `s`'s
//! bytes, and native and release printed the chunk the next region
//! reused (`child sees: ZZZZ…`); the checked machine printed the stale
//! bytes. What makes lobo's `budget.run_small` sound where `helper` is
//! not is that it waits for its proc on every path before it returns.
//!
//! The must-join check (`[conc.proc.arg]`, `wolf_mem::procjoin`): a
//! spawn whose arguments can name a parameter's bytes or the caller
//! region's must be met on every path out of the frame — `p.join()`, an
//! exit received from `p.monitor()`, or the handle handed to a function
//! that does so; a kill is not a join (wolf-lang#683). A function that
//! misses one is a sink for those parameters, and every call to it is
//! E1010 at the argument whose bytes live in a frame-local region (at
//! the call, for its caller region's). A closure that leaves a proc
//! running with a value that can name bytes, and a sink taken as a fn
//! value, are refused where they are made.
//!
//! Every refused row below runs to `exit(0)` before s233 on the
//! compiler's three lanes (17 of them printing the reused chunk on
//! native and release, linux x86-64, 549dea59). lupin runs the proc in
//! its own failure domain and no trap reaches the program, so its
//! verdict on a refused row is logged, not asserted (s229's spawn rows
//! do the same). The sound rows run on all four machines.
//!
//! Why a driver test beside the memory checker's snapshots (s171's
//! lesson, wave 45): `cargo xtask corpus` runs every entry on the
//! NATIVE lane only; this gate asks checked, native, release and lupin.

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
    /// Each diagnostic's code and the byte its span starts at.
    spans: Vec<(String, u64)>,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    let diags = rec["diagnostics"].as_array().cloned().unwrap_or_default();
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
        version: rec["impl_version"].as_str().unwrap_or("").to_string(),
        codes: diags
            .iter()
            .filter_map(|d| d["code"].as_str().map(str::to_string))
            .collect(),
        spans: diags
            .iter()
            .filter_map(|d| Some((d["code"].as_str()?.to_string(), d["span"][0].as_u64()?)))
            .collect(),
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
/// arranged here" (r10/#253): there an absent sibling is a failed
/// fetch, and a gate that answers broken plumbing with a green skip is
/// not a gate.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of wolf-lang#678's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the wolfgang lanes \
             are still compared against the EXPECTED answer here; only the oracle \
             leg is absent"
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

/// A fixture beside this test. The refused shapes cannot be corpus
/// rows: a corpus row is one truth for every machine, and lupin runs
/// them.
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/must_join")
        .join(name)
        .join("main.lu");
    assert!(p.exists(), "fixture missing: {}", p.display());
    p
}

const ISSUE: &str = "wolf-lang#678";

/// Checked, native and release refuse the program E1010; lupin's
/// verdict is logged (its proc reads inside its own failure domain).
fn refused(name: &str) {
    let entry = fixture(name);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "fail(E1010)",
            "{flag} admitted a proc that may outlive the bytes it reads: {} ({ISSUE})",
            entry.display()
        );
        assert!(
            obs.codes.iter().any(|c| c == "E1010")
                && obs.codes.iter().all(|c| c == "E1010" || c.starts_with('W')),
            "{flag} refused {} with an error besides E1010: {:?}",
            entry.display(),
            obs.codes
        );
    }
    if let Some(lupin) = lupin_says(&entry) {
        eprintln!(
            "OBSERVED: lupin {} answers {} printing {:?} on {}",
            lupin.version,
            lupin.verdict,
            lupin.stdout,
            entry.display()
        );
    }
}

/// Every lane, lupin included, runs the program to `exit(0)`; with
/// `want`, each prints exactly that.
fn runs(name: &str, want: Option<&str>) {
    let entry = fixture(name);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "exit(0)",
            "{flag} on the sound row {}: {:?}",
            entry.display(),
            obs.codes
        );
        if let Some(want) = want {
            assert_eq!(obs.stdout, want, "{flag} stdout on {}", entry.display());
        }
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin {} on the sound row {}",
            lupin.version,
            entry.display()
        );
        if let Some(want) = want {
            assert_eq!(
                lupin.stdout,
                want,
                "lupin {} stdout on {}",
                lupin.version,
                entry.display()
            );
        }
    }
}

/// The byte offset of `needle` in the fixture `name`, plus `skip`: where
/// a refusal must point.
fn offset_in(name: &str, needle: &str, skip: usize) -> u64 {
    let src = std::fs::read_to_string(fixture(name)).expect("fixture reads");
    let at = src
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is not in fixture {name}"));
    (at + skip) as u64
}

/// Checked, native and release refuse the program with `code` and
/// nothing else but warnings, and one `code` diagnostic starts at byte
/// `at`; lupin's verdict is logged.
fn refused_at(name: &str, code: &str, at: u64, issue: &str) {
    let entry = fixture(name);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            format!("fail({code})"),
            "{flag} on {} ({issue})",
            entry.display()
        );
        assert!(
            obs.codes.iter().all(|c| c == code || c.starts_with('W')),
            "{flag} refused {} with an error besides {code}: {:?}",
            entry.display(),
            obs.codes
        );
        assert!(
            obs.spans.iter().any(|(c, lo)| c == code && *lo == at),
            "{flag} refused {} but no {code} starts at byte {at}: {:?} ({issue})",
            entry.display(),
            obs.spans
        );
    }
    if let Some(lupin) = lupin_says(&entry) {
        eprintln!(
            "OBSERVED: lupin {} answers {} printing {:?} on {}",
            lupin.version,
            lupin.verdict,
            lupin.stdout,
            entry.display()
        );
    }
}

/// The must-join runtime's sound rows: checked, native and release run
/// the program to `exit(0)` printing exactly `want`. lupin's answer is
/// logged, not asserted, until the reference interpreter carries the
/// same rule (its released answer differs on these shapes, and the gate
/// finds it through the org checkout).
fn runs_pre_mirror(name: &str, want: &str, issue: &str) {
    let entry = fixture(name);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "exit(0)",
            "{flag} on {}: {:?} ({issue})",
            entry.display(),
            obs.codes
        );
        assert_eq!(
            obs.stdout,
            want,
            "{flag} stdout on {} ({issue})",
            entry.display()
        );
    }
    if let Some(lupin) = lupin_says(&entry) {
        eprintln!(
            "OBSERVED: lupin {} answers {} printing {:?} on {}",
            lupin.version,
            lupin.verdict,
            lupin.stdout,
            entry.display()
        );
    }
}

/// Every lane, lupin included, ends the program with `verdict` and
/// prints nothing.
fn ends_with(name: &str, verdict: &str) {
    let entry = fixture(name);
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(&entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(obs.verdict, verdict, "{flag} on {}", entry.display());
        assert_eq!(obs.stdout, "", "{flag} stdout on {}", entry.display());
    }
    if let Some(lupin) = lupin_says(&entry) {
        assert_eq!(
            lupin.verdict,
            verdict,
            "lupin {} on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// The eight `scribble` lines the reuse fixtures print after their join.
fn scribbles() -> String {
    (0..8).map(|k| format!("scribble {k} 28 690\n")).collect()
}

// ---- refused: the proc may outlive the bytes it reads ----

/// #678's helper half, as the issue states it.
#[test]
fn a_helper_that_spawns_with_its_parameter_and_does_not_join_is_refused() {
    refused("helper");
}

/// Through two calls: the outer function is a sink because it calls one.
#[test]
fn a_sink_two_calls_down_is_refused() {
    refused("two_deep");
}

/// The helper builds the bytes in its caller's region: E1010 at the call.
#[test]
fn a_helper_building_in_its_caller_s_region_is_refused() {
    refused("ambient");
}

/// A join on one branch only.
#[test]
fn a_join_on_one_branch_is_refused() {
    refused("one_branch");
}

/// A `return` before the join.
#[test]
fn an_early_return_before_the_join_is_refused() {
    refused("early_return");
}

/// An error edge (`?`) before the join.
#[test]
fn an_error_edge_before_the_join_is_refused() {
    refused("error_edge");
}

/// A `select` whose timeout arm returns without a join.
#[test]
fn a_timeout_arm_that_returns_unjoined_is_refused() {
    refused("select_timeout");
}

/// A kill is not a join (wolf-lang#683).
#[test]
fn a_kill_without_a_join_is_refused() {
    refused("kill_no_join");
}

/// The handle handed to a function that only kills it.
#[test]
fn a_handle_handed_to_a_function_that_does_not_join_is_refused() {
    refused("handle_to_nonjoiner");
}

/// The handle leaves the frame: the caller joins after the region closed.
#[test]
fn a_returned_handle_is_refused() {
    refused("returns_handle");
}

/// The handle is rebound before the join: the first proc is lost.
#[test]
fn a_rebound_handle_is_refused() {
    refused("rebound");
}

/// Spawned each iteration, only the last joined.
#[test]
fn only_the_last_of_a_loop_joined_is_refused() {
    refused("loop_last");
}

/// The proc is joined, but it leaves a grandchild running with the
/// bytes.
#[test]
fn a_joined_proc_that_leaves_a_grandchild_running_is_refused() {
    refused("grandchild");
}

/// A method spawning with its receiver's string field.
#[test]
fn a_method_on_a_region_built_struct_is_refused() {
    refused("method");
}

/// A generic helper.
#[test]
fn a_generic_helper_is_refused() {
    refused("generic");
}

/// The helper in another module.
#[test]
fn a_helper_in_another_module_is_refused() {
    refused("module");
}

/// A trait method, called through a generic bound: the call answers for
/// every impl.
#[test]
fn a_trait_method_through_a_bound_is_refused() {
    refused("trait_bound");
}

/// A struct parameter's string field.
#[test]
fn a_struct_parameter_s_field_is_refused() {
    refused("struct_param");
}

/// A `List[str]` parameter's element.
#[test]
fn a_list_parameter_s_element_is_refused() {
    refused("list_param");
}

/// A view of the parameter (`s.trim()`).
#[test]
fn a_view_of_the_parameter_is_refused() {
    refused("view");
}

/// The call sits under `in r { }` with the bytes built in `r`.
#[test]
fn a_call_under_in_is_refused() {
    refused("in_block");
}

/// A closure that spawns with its parameter: refused at the spawn.
#[test]
fn a_closure_that_leaves_a_proc_running_is_refused() {
    refused("closure");
}

/// The only join is inside a closure that is never called.
#[test]
fn a_join_inside_an_uncalled_closure_does_not_count() {
    refused("closure_join_uncalled");
}

/// A sink taken as a fn value.
#[test]
fn a_sink_taken_as_a_fn_value_is_refused() {
    refused("fn_value");
}

/// wolf-std's `mem.budget.with_cap` shape: a closure capturing region
/// bytes, run in a proc the timeout arm leaves running.
#[test]
fn a_closure_capturing_region_bytes_handed_to_a_timed_out_proc_is_refused() {
    refused("with_cap_closure");
}

// ---- sound: every path waits for the proc, or the bytes outlive it ----

/// The helper joins before it returns (s229's accepted spelling).
#[test]
fn a_helper_that_joins_still_runs() {
    runs(
        "sound_helper_joins",
        Some("child sees: built in pass, 7 uses\njoined 0\n28\n"),
    );
}

/// Both arms of an `if` join and return.
#[test]
fn a_join_on_every_path_still_runs() {
    runs(
        "sound_every_path_joins",
        Some("child sees: built in pass, 7 uses\n3\n28\n"),
    );
}

/// The join is deferred, and an early return runs it.
#[test]
fn a_deferred_join_still_runs() {
    runs(
        "sound_defer_join",
        Some("child sees: built in pass, 7 uses\n3\n28\n"),
    );
}

/// Spawned and joined in each iteration.
#[test]
fn a_join_in_each_iteration_still_runs() {
    runs(
        "sound_loop_joined",
        Some(
            "child sees: built in pass, 7 uses 0\nchild sees: built in pass, 7 uses 1\n\
             child sees: built in pass, 7 uses 2\n3\n28\n",
        ),
    );
}

/// Joined through a copy of the handle.
#[test]
fn a_join_through_a_copy_of_the_handle_still_runs() {
    runs(
        "sound_alias_join",
        Some("child sees: built in pass, 7 uses\n0\n28\n"),
    );
}

/// Scalars only: nothing to outlive.
#[test]
fn a_helper_spawning_with_scalars_still_runs() {
    runs("sound_ints_only", Some("28\nchild sees: 21\n"));
}

/// A literal from a region block: static bytes.
#[test]
fn a_helper_handed_a_literal_still_runs() {
    runs("sound_literal", Some("28\nchild sees: a literal\n"));
}

/// lobo's `acme.daemon_start` shape: never joined, and the bytes are
/// built in `main`'s own region, which outlives the proc.
#[test]
fn an_unjoined_helper_handed_main_s_bytes_still_runs() {
    runs(
        "sound_main_bytes",
        Some("28\nchild sees: built in main, 7 uses\n"),
    );
}

/// The handle handed to a function that kills and joins it (lobo's
/// `end_body`). Whether the child prints before the kill lands is the
/// scheduler's, so only the verdict is asserted.
#[test]
fn a_handle_handed_to_a_joining_function_still_runs() {
    runs("sound_handle_to_joiner", None);
}

/// lobo's `budget.run_small`: an exit from the monitor, or on the
/// timeout arm `cls = 4` and then `if cls == 4 { end_body(w) }` — read
/// path by path. Which arm a machine takes first is its scheduler's.
#[test]
fn lobo_s_monitor_and_end_body_shape_still_runs() {
    runs("sound_monitor_end_body", None);
}

/// The same with a `bool` flag.
#[test]
fn a_bool_flag_deciding_the_join_still_runs() {
    runs("sound_bool_flag", None);
}

/// lobo's chain: a request region, a serving function handed the path,
/// `run_small` building the head in its caller's region.
#[test]
fn lobo_s_serving_chain_still_runs() {
    runs("sound_lobo_chain", None);
}

// ---- the must-join runtime: paths no static rule over the frame can see ----
//
// A trap ending a path meets nothing, and a kill can land at any park:
// what keeps an owed proc from outliving its spawner's regions is the
// killed-proc sequence, which kills and waits out the procs the dying
// proc still owes before its regions free (`[conc.proc.kill]` step 2,
// wolf-lang#730). A join returns only once its proc has exited and
// forwards its own task's cancellation (`[conc.proc.join]`,
// wolf-lang#732). A cancellation that reaches a blocked `select` runs
// its timeout arm, or leaves as `?` would, and has to have a row to
// leave through (`[conc.select.ready]`, wolf-lang#1143). Every row here
// printed a `child` line, read freed bytes, trapped `assert` inside the
// native select or was accepted, on at least one of checked, native and
// release, before the rule. The process rows run under `taskset -c 0-3`
// and `taskset -c 0` too in the records.

/// A trap between the spawn and its join, inside a proc: the trap is
/// contained, and the proc it owes dies before `pass` frees.
#[test]
fn a_trap_between_spawn_and_join_in_a_proc_kills_the_owed_proc() {
    runs_pre_mirror(
        "trap_owed",
        &format!("outer 9 reclaimed true\n{}", scribbles()),
        "wolf-lang#730",
    );
}

/// The spawner killed while it waits in the join: the same harm with no
/// trap at all.
#[test]
fn a_killed_spawner_waits_out_its_owed_proc() {
    runs_pre_mirror(
        "kill_owed",
        &format!("outer joined 20 reclaimed true\n{}", scribbles()),
        "wolf-lang#730",
    );
}

/// A cancelled joiner: the join forwards the cancellation and returns
/// only once `child` has exited, so `child` reads `pass` while it lives.
#[test]
fn a_cancelled_joiner_waits_for_its_proc() {
    runs_pre_mirror(
        "cancel_join",
        &format!(
            "child wakes\nchild sees: built in pass, 7 uses\nhelper 0\nouter returns\n\
             outer joined 0 reclaimed true\n{}",
            scribbles()
        ),
        "wolf-lang#732",
    );
}

/// lobo's `run` shape: the cancellation runs the timeout arm, whose
/// cleanup kills and joins the proc before `pass` frees.
#[test]
fn a_cancelled_select_with_a_timeout_runs_the_timeout_arm() {
    runs_pre_mirror(
        "cancel_select_timeout",
        "cls 4\nouter reason normal(0)\n",
        "wolf-lang#1143",
    );
}

/// No timeout arm, in a function with a row: the cancellation leaves
/// the select as `?` would.
#[test]
fn a_cancelled_select_in_a_row_function_propagates() {
    runs_pre_mirror(
        "select_row",
        "wait_one raised cancelled\nworker got 9\nreason normal(7)\n",
        "wolf-lang#1143",
    );
}

/// A monitor select with no timeout arm carries the cancellation's `?`
/// edge, on which the frame returns with its proc running: `helper` is a
/// sink, and the call in `region pass` is E1010 at its argument.
#[test]
fn a_monitor_select_without_a_timeout_is_a_sink() {
    refused_at(
        "select_sink",
        "E1010",
        offset_in("select_sink", "helper(s) else 9", "helper(".len()),
        "wolf-lang#1143",
    );
}

/// A select that can block, with no timeout arm, in a function with no
/// row: the cancellation would have nowhere to go.
#[test]
fn a_blockable_select_without_a_timeout_or_a_row_is_refused() {
    refused_at(
        "select_no_row",
        "E0604",
        offset_in("select_no_row", "    select {", 4),
        "wolf-lang#1143",
    );
}

/// A killed proc's regions free before its reason is delivered, on the
/// checked machine too (wolf-lang#1141).
#[test]
fn a_killed_proc_is_reclaimed_before_its_reason() {
    runs_pre_mirror(
        "kill_reclaim",
        "joined 20 reclaimed true\n",
        "wolf-lang#1141",
    );
}

/// Control: the same trap in the root domain is process death.
#[test]
fn a_trap_between_spawn_and_join_in_main_ends_the_process() {
    ends_with("trap_in_root", "trap(bounds)");
}

/// Control: a cancellation at a receive with `?`.
#[test]
fn a_cancelled_receive_with_a_question_mark_propagates() {
    runs(
        "recv_question",
        Some("wait_one raised cancelled\nworker got 9\nreason normal(7)\n"),
    );
}

/// Control: a cancellation at a receive handled by `else`.
#[test]
fn a_cancelled_receive_with_else_is_handled_in_place() {
    runs(
        "recv_else",
        Some("recv raised cancelled\nafter recv 5\nworker got 5\nreason normal(7)\n"),
    );
}
