//! s229 (wolf-lang#665, and #678's direct half) — what a call can send,
//! it is taken to send; what a proc is handed outlives the spawning frame.
//!
//! A payload leaves the sending frame, so it must outlive it
//! (`[conc.chan.payload]`), and the builtin `send` demands that. A
//! callee handed the channel sends too and was refused nowhere: in the
//! callee the payload is a parameter, or a value built in its caller's
//! current region. So `region pass { let s = "built {n}"; fwd(ch, s) }`
//! — `fwd` one line around `ch.send(s)` — was accepted while the
//! direct send was E1010: native and release handed the receiver the
//! chunk the next region reused, the checked machine stale bytes.
//! Every refused row below is red before s229 (`exit(0)` on the
//! compiler's lanes) and E1010 after.
//!
//! lupin keeps a payload's bytes in the sender's region and traps
//! `region-fault` when a `for`, a `select` arm, a `?`-bound receive or
//! an aggregate payload reads them after that region died; those rows
//! are corpus rows, one truth for every machine. It misses the read
//! inside `"{ch.recv() else "?"}"` (wolffe-lang/wolf-interp#232), so
//! those shapes are fixtures beside this test, pinned to the lupin
//! that runs them.
//!
//! wolf-lang#678 is the same frame rule at `spawn proc` with no
//! channel at all: an argument built in a region block is read by the
//! proc after the block closed. Its rows are at the end.
//!
//! The legal side is gated in two rows (one needs tasks, which the
//! checked machine declines), and so is the rule's stated cost: three
//! sound programs it refuses because it reads the signature and
//! never the callee's body.
//!
//! Why a driver test beside the corpus rows (s171's lesson, wave 45):
//! `cargo xtask corpus` runs every entry on the NATIVE lane only; this
//! gate asks checked, native, release and lupin.

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
/// arranged here" (r10/#253): there an absent sibling is a failed
/// fetch, and a gate that answers broken plumbing with a green skip is
/// not a gate.
fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of wolf-lang#665's gate did not run",
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

/// The corpus row itself is the program: one truth per file.
fn corpus(rel: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel);
    assert!(p.exists(), "corpus row missing: {}", p.display());
    p
}

/// A fixture beside this test: the shapes lupin runs, kept out of
/// `corpus/` (a corpus row is one truth for every machine).
fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sent_arg")
        .join(name)
        .join("main.lu");
    assert!(p.exists(), "fixture missing: {}", p.display());
    p
}

const ISSUE: &str = "wolf-lang#665";

const RULED_LUPIN: &str = "trap(region-fault)";

/// The paired lupins that run a channel payload whose bytes live in a
/// freed region to `exit(0)` when the receive is read inside an
/// interpolation (wolffe-lang/wolf-interp#232: the dynamic machine
/// misses a channel payload reaching into a freed region there, as it
/// misses a list element and a map key). Pinned by version so the pin
/// dies with the fix: a lupin outside this list must trap. Measured on
/// the published 0.1.50 archive: every pinned row `exit(0)`; and on the
/// published 0.1.51 archive (the 0.2.28 pairing, r33; #232 still open),
/// the same; and on the published 0.1.52 archive (the 0.2.29 pairing,
/// r34; #232 still open), the same; and on the published 0.1.53 archive
/// (the 0.2.30 pairing, r35; #232 still open), the same; and on the
/// published 0.1.54 archive (the 0.2.31 pairing, r36; #232 still open),
/// the same; and on the published 0.1.55 archive (the 0.2.34 pairing,
/// r39; #232 still open), the same.
// r40/r41: published .56 and .57 retain these separately measured answers.
// r41 resume: published .58 re-measured these exact 21 rows.
const LUPIN_MISSES_CHANNEL_PAYLOADS: &[&str] = &[
    "0.1.50", "0.1.51", "0.1.52", "0.1.53", "0.1.54", "0.1.55", "0.1.56", "0.1.57", "0.1.58",
];

fn base_version(v: &str) -> &str {
    v.split('+').next().unwrap_or(v)
}

/// Checked, native and release refuse the program E1010.
fn refused_by_the_compiler(entry: &Path) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        assert_eq!(
            obs.verdict,
            "fail(E1010)",
            "{flag} admitted a payload a callee may send past its region: {} ({ISSUE})",
            entry.display()
        );
        assert!(
            obs.codes.iter().any(|c| c == "E1010"),
            "{flag} refused {} without E1010: {:?}",
            entry.display(),
            obs.codes
        );
    }
}

/// The compiler refuses; lupin traps `region-fault`.
fn refused_everywhere(entry: &Path) {
    refused_by_the_compiler(entry);
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            RULED_LUPIN,
            "lupin {}'s verdict on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// The compiler refuses; lupin traps, except the version pinned at
/// wolf-interp#232, which runs the stale read to `exit(0)`.
fn refused_and_lupin_pinned(entry: &Path) {
    refused_by_the_compiler(entry);
    if let Some(lupin) = lupin_says(entry) {
        if LUPIN_MISSES_CHANNEL_PAYLOADS.contains(&base_version(&lupin.version)) {
            // The release answers `exit(0)`, exactly. A dev build of
            // the pinned version may already carry the fix and trap.
            let release = lupin.version == base_version(&lupin.version);
            assert!(
                lupin.verdict == "exit(0)" || (!release && lupin.verdict == RULED_LUPIN),
                "lupin {} on {}: {} (pinned at exit(0), wolf-interp#232)",
                lupin.version,
                entry.display(),
                lupin.verdict
            );
            eprintln!(
                "PINNED: lupin {} answers {} on {} (wolf-interp#232)",
                lupin.version,
                lupin.verdict,
                entry.display()
            );
        } else {
            assert_eq!(
                lupin.verdict,
                RULED_LUPIN,
                "lupin {} is past the wolf-interp#232 pin and must trap on {}",
                lupin.version,
                entry.display()
            );
        }
    }
}

/// The rule's stated cost: the compiler refuses a program that would
/// have run correctly, and lupin — which runs it — says so.
fn refused_though_sound(entry: &Path, want: &str) {
    refused_by_the_compiler(entry);
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin {} on the sound program {}",
            lupin.version,
            entry.display()
        );
        assert_eq!(
            lupin.stdout,
            want,
            "lupin {} stdout on {}",
            lupin.version,
            entry.display()
        );
    }
}

/// The lanes named run the program to `exit(0)` printing `want`, and
/// so does lupin. With `tasks`, the checked machine may instead
/// decline (`scope` and `spawn proc`, C1).
fn runs(entry: &Path, want: &str, tasks: bool) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            assert_ne!(flag, "--checked", "the checked lane always runs");
            continue;
        };
        if tasks && flag == "--checked" && obs.verdict == "unsupported" {
            eprintln!(
                "DECLINED: --checked answers unsupported on {} (C1)",
                entry.display()
            );
            continue;
        }
        assert_eq!(
            obs.verdict,
            "exit(0)",
            "{flag} on the legal row {}: {:?}",
            entry.display(),
            obs.codes
        );
        assert_eq!(obs.stdout, want, "{flag} stdout on {}", entry.display());
    }
    if let Some(lupin) = lupin_says(entry) {
        assert_eq!(
            lupin.verdict,
            "exit(0)",
            "lupin {} on the legal row {}",
            lupin.version,
            entry.display()
        );
        assert_eq!(
            lupin.stdout,
            want,
            "lupin {} stdout on {}",
            lupin.version,
            entry.display()
        );
    }
}

// ---- corpus rows: the compiler refuses, lupin traps ----

/// A struct payload reaching a region-built `str`.
#[test]
fn a_struct_payload_sent_by_a_callee_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_struct.lu"));
}

/// An enum payload.
#[test]
fn an_enum_payload_sent_by_a_callee_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_enum.lu"));
}

/// A tuple payload.
#[test]
fn a_tuple_payload_sent_by_a_callee_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_tuple.lu"));
}

/// The `str` two structs deep.
#[test]
fn a_nested_struct_payload_sent_by_a_callee_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_nested.lu"));
}

/// The report's shape, received by a `for`.
#[test]
fn a_str_sent_by_a_callee_and_received_by_a_for_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_for.lu"));
}

/// Received by a `select` arm.
#[test]
fn a_str_sent_by_a_callee_and_received_by_a_select_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_select.lu"));
}

/// Received through `?` in a callee and returned.
#[test]
fn a_str_sent_by_a_callee_and_received_through_a_call_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_bound.lu"));
}

/// A `scope` task hands the helper a string built in a region block.
#[test]
fn a_scope_task_s_helper_wrapped_send_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_scope.lu"));
}

/// A proc entry hands a helper a string built in the proc's region.
#[test]
fn a_proc_s_helper_wrapped_send_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_proc.lu"));
}

/// The helper builds in the proc's region and sends: no argument
/// carries the bytes.
#[test]
fn a_callee_building_in_a_proc_s_region_and_sending_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_proc_site.lu"));
}

/// A `spawn proc` argument living in a region block of the spawner,
/// beside the channel the proc sends it on.
#[test]
fn a_spawn_argument_beside_a_channel_is_refused() {
    refused_everywhere(&corpus("conc/chan_sent_arg_spawn.lu"));
}

// ---- fixtures: the compiler refuses, lupin 0.1.50 runs (wolf-interp#232) ----

/// wolf-lang#665's witness, byte for byte.
#[test]
fn the_report_s_helper_wrapped_send_is_refused() {
    refused_and_lupin_pinned(&fixture("helper"));
}

/// A generic callee.
#[test]
fn a_generic_callee_s_send_is_refused() {
    refused_and_lupin_pinned(&fixture("generic"));
}

/// A callee in another module: the rule reads the signature alone.
#[test]
fn a_callee_in_another_module_sending_is_refused() {
    refused_and_lupin_pinned(&fixture("module"));
}

/// A method on a struct holding the channel.
#[test]
fn a_method_on_a_struct_holding_the_channel_is_refused() {
    refused_and_lupin_pinned(&fixture("method"));
}

/// Through two calls.
#[test]
fn a_send_two_calls_down_is_refused() {
    refused_and_lupin_pinned(&fixture("two_calls"));
}

/// The bytes are reached through a `List[str]` argument.
#[test]
fn a_str_inside_a_list_argument_is_refused() {
    refused_and_lupin_pinned(&fixture("list"));
}

/// The bytes are reached through a struct argument's field.
#[test]
fn a_str_inside_a_struct_argument_is_refused() {
    refused_and_lupin_pinned(&fixture("struct_field"));
}

/// A `take` argument.
#[test]
fn a_take_argument_sent_by_its_callee_is_refused() {
    refused_and_lupin_pinned(&fixture("take"));
}

/// A `mut` argument.
#[test]
fn a_mut_argument_sent_by_its_callee_is_refused() {
    refused_and_lupin_pinned(&fixture("mut"));
}

/// The call sits under `in st { }`.
#[test]
fn a_helper_wrapped_send_under_in_is_refused() {
    refused_and_lupin_pinned(&fixture("in_block"));
}

/// A view of the region-built string.
#[test]
fn a_view_sent_by_a_callee_is_refused() {
    refused_and_lupin_pinned(&fixture("view"));
}

/// No argument carries the bytes: the callee builds in its caller's
/// current region, a region block, and sends.
#[test]
fn a_callee_building_in_a_region_block_and_sending_is_refused() {
    refused_and_lupin_pinned(&fixture("call_site"));
}

/// A trait method through a bound, the channel inside the receiver.
#[test]
fn a_trait_method_s_send_is_refused() {
    refused_and_lupin_pinned(&fixture("trait_method"));
}

/// A `dyn` value hides the channel.
#[test]
fn a_dyn_value_hiding_the_channel_is_refused() {
    refused_and_lupin_pinned(&fixture("dyn"));
}

/// Inside a generic body: a bounded type parameter may hold the
/// channel.
#[test]
fn a_bounded_type_parameter_that_may_hold_the_channel_is_refused() {
    refused_and_lupin_pinned(&fixture("generic_bound"));
}

/// The channel is reached through a list.
#[test]
fn a_channel_inside_a_list_argument_is_refused() {
    refused_and_lupin_pinned(&fixture("chan_in_list"));
}

/// A closure that captured the channel, called in the region block.
#[test]
fn a_closure_that_captured_the_channel_called_is_refused() {
    refused_and_lupin_pinned(&fixture("closure_called"));
}

/// The same closure under a second name.
#[test]
fn a_rebound_closure_that_captured_the_channel_is_refused() {
    refused_and_lupin_pinned(&fixture("closure_rebound"));
}

/// The closure handed to a callee, which calls it with bytes from a
/// region block of its own.
#[test]
fn a_closure_that_captured_the_channel_handed_to_a_callee_is_refused() {
    refused_and_lupin_pinned(&fixture("closure_handed"));
}

/// The closure builds its payload wherever it is called.
#[test]
fn a_closure_building_its_payload_in_a_callee_s_region_is_refused() {
    refused_and_lupin_pinned(&fixture("closure_builds"));
}

/// The closure returned from the frame that made it.
#[test]
fn a_closure_that_captured_the_channel_returned_is_refused() {
    refused_and_lupin_pinned(&fixture("closure_returned"));
}

// ---- the rule's stated cost: sound, and refused ----

/// The callee only reads the argument and sends a literal.
#[test]
fn a_callee_that_only_reads_beside_a_channel_is_refused_though_sound() {
    refused_though_sound(&fixture("sound_only_reads"), "21\n28\nnoted\n");
}

/// The channel is made, used and drained inside one region block.
#[test]
fn a_channel_used_inside_one_region_block_is_refused_though_sound() {
    refused_though_sound(&fixture("sound_same_region"), "built in pass, 0 uses\n");
}

/// A proc entry hands its parameter to a helper.
#[test]
fn a_proc_s_parameter_through_a_helper_is_refused_though_sound() {
    refused_though_sound(&fixture("sound_proc_param"), "built 7 here\n");
}

// ---- wolf-lang#678, the spawn twin: no channel at all ----

/// The compiler refuses. lupin's proc reads the freed bytes inside its
/// own failure domain and no trap reaches the program, so its verdict
/// is logged and not asserted.
fn spawn_refused(entry: &Path) {
    refused_by_the_compiler(entry);
    if let Some(lupin) = lupin_says(entry) {
        eprintln!(
            "OBSERVED: lupin {} answers {} printing {:?} on {}",
            lupin.version,
            lupin.verdict,
            lupin.stdout,
            entry.display()
        );
    }
}

/// #678's witness: a `spawn proc` argument built in a region block.
#[test]
fn a_spawn_argument_built_in_a_region_block_is_refused() {
    spawn_refused(&fixture("spawn_region_arg"));
}

/// The bytes are reached through a struct argument.
#[test]
fn a_spawn_argument_s_struct_field_built_in_a_region_block_is_refused() {
    spawn_refused(&fixture("spawn_struct_arg"));
}

/// The proc is joined inside the block that built its argument.
#[test]
fn a_spawn_joined_inside_its_region_block_is_refused_though_sound() {
    refused_though_sound(
        &fixture("sound_spawn_joined"),
        "child sees: built in pass, 7 uses\n0\n",
    );
}

/// A function handed the bytes as a parameter spawns and joins.
#[test]
fn a_spawn_with_a_parameter_joined_before_the_return_still_runs() {
    runs(
        &fixture("spawn_helper_joins"),
        "child sees: built in pass, 7 uses\n0\n",
        true,
    );
}

/// A literal and a scalar own no region storage.
#[test]
fn a_spawn_with_a_literal_and_a_scalar_still_runs() {
    runs(
        &fixture("spawn_scalar_literal"),
        "child sees: a literal 21\n",
        true,
    );
}

// ---- the sound neighbours ----

/// Every one green before s229 on all four machines.
#[test]
fn the_sound_neighbours_still_run() {
    runs(
        &corpus("conc/chan_sent_arg_legal.lu"),
        "28\nbuilt in main, 0 uses\nbuilt by emit, 0 uses\nstatic\nbuilt in pass, 0 uses\na 0 doc 2\n21\n",
        false,
    );
}

/// The ones that need tasks: native, release and lupin (the checked
/// machine declines `scope` and `spawn proc`).
#[test]
fn the_sound_neighbours_with_tasks_still_run() {
    runs(
        &corpus("conc/chan_sent_arg_legal_tasks.lu"),
        "built in main, 7 uses\nbuilt in launch, 7 uses\n7\n",
        true,
    );
}
