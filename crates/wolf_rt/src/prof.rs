//! Profile counters and the `.wprof` dump (s45 target 1, the runtime
//! half).
//!
//! Three shims, present only in `--profile-gen` builds:
//!
//! - [`__wolf_rt_prof_init`] — once, at the top of `main`: parse the
//!   compiler-emitted index blob and allocate the counter array.
//! - [`__wolf_rt_prof_bump`] — at the head of every block: one relaxed
//!   atomic increment. **No allocation, no lock, no syscall** (D15
//!   discipline): the array exists before the first bump, and a bump
//!   that somehow arrives before init or past the end is dropped
//!   rather than growing anything.
//! - [`__wolf_rt_prof_dump`] — write the profile. Idempotent, so the
//!   several exits below can each call it and only the first writes.
//!
//! # Every exit dumps
//!
//! The compiler puts a dump before every `ret` in `main` — the clean
//! exit. It cannot put one on the paths that never return, so those
//! call in from this side: [`crate::native::__wolf_rt_trap`] (the
//! fault path the sprint names), [`crate::native::__wolf_rt_main_err`]
//! (an error-returning `main`), and [`crate::os::__wolf_rt_os_exit`].
//! That is deliberately explicit rather than an `atexit` registration:
//! `atexit` is a libc surface, wolf is platform-agnostic with Windows
//! in tier 1, and four named call sites are easier to audit than a
//! handler whose ordering depends on the C runtime.
//!
//! A profile from a faulting run is still a profile — the counters up
//! to the fault are exactly the execution that happened, and throwing
//! them away would make the tool useless on the program you most want
//! to look at.
//!
//! # The output path
//!
//! `WOLF_PROFILE_FILE` wins; otherwise the path the compiler baked
//! into the index blob (from `--profile-gen[=<dir>]`, defaulting to
//! `default.wprof` in the working directory). A dump that cannot be
//! written says so on stderr and does nothing else: a profiling run is
//! not allowed to change the program's exit status.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// The index-blob format this runtime parses. Must match
/// `wolf_wir::midend::instrument::INDEX_VERSION`; a mismatch refuses
/// loudly rather than misreading (compiler and runtime ship together,
/// so a mismatch means someone mixed builds).
const INDEX_VERSION: u32 = 1;

/// The `.wprof` container version this runtime writes. Must match
/// `wolf_wir::profile::WPROF_VERSION`.
const WPROF_VERSION: u32 = 1;

/// One function's slice of the flat counter array.
#[derive(Debug)]
struct Entry {
    /// The D8 content hash of the body — the record key.
    hash: String,
    base: u32,
    blocks: u32,
}

struct State {
    counters: Vec<AtomicU64>,
    entries: Vec<Entry>,
    path: String,
}

static STATE: OnceLock<State> = OnceLock::new();
static DUMPED: AtomicBool = AtomicBool::new(false);

/// Install the counter array from the compiler's index blob.
///
/// Idempotent: a second call is ignored (the first `main` wins), which
/// is what makes this safe against a re-entrant entry point.
///
/// # Safety
///
/// `index` must point at `len` readable bytes of UTF-8 — the blob the
/// compiler emitted as module data, which is read-only for the life of
/// the process.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_prof_init(index: i64, len: i64, total: i64) {
    if STATE.get().is_some() {
        return;
    }
    if index == 0 || len <= 0 || total < 0 {
        eprintln!("wolf-prof: refusing a malformed profile index; no profile will be written");
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(index as *const u8, len as usize) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        eprintln!("wolf-prof: the profile index is not UTF-8; no profile will be written");
        return;
    };
    match parse_index(text, total as u64) {
        Ok((entries, path)) => {
            let mut counters = Vec::new();
            counters.resize_with(total as usize, || AtomicU64::new(0));
            let _ = STATE.set(State {
                counters,
                entries,
                path,
            });
        }
        Err(why) => {
            eprintln!("wolf-prof: {why}; no profile will be written");
        }
    }
}

/// Count one block execution. The hot path: one relaxed add.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_prof_bump(idx: i64) {
    let Some(st) = STATE.get() else { return };
    if idx < 0 {
        return;
    }
    if let Some(c) = st.counters.get(idx as usize) {
        c.fetch_add(1, Ordering::Relaxed);
    }
}

/// Write the `.wprof`. The first call writes; later ones are no-ops.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_prof_dump() {
    let Some(st) = STATE.get() else { return };
    if DUMPED.swap(true, Ordering::SeqCst) {
        return;
    }
    let path = std::env::var("WOLF_PROFILE_FILE").unwrap_or_else(|_| st.path.clone());
    if let Err(e) = std::fs::write(&path, render(st)) {
        // A failed dump is reported and survived. A profiling run must
        // not change the program's exit status.
        eprintln!("wolf-prof: could not write `{path}`: {e}");
    }
}

/// Dump from an exit path that never returns. Separate from the shim
/// so the never-returning shims do not have to reason about the C ABI
/// name; behaviour is identical.
pub(crate) fn dump_on_exit() {
    __wolf_rt_prof_dump();
}

/// The `.wprof` v1 serialization — the same canonical, deterministic
/// text `wolf_wir::profile::Profile::render` produces, so a runtime
/// dump and a `wolf profile merge` output are the same kind of file.
/// Records ascend by hash.
fn render(st: &State) -> String {
    let mut recs: Vec<(&str, Vec<u64>)> = st
        .entries
        .iter()
        .map(|e| {
            let lo = e.base as usize;
            let hi = lo + e.blocks as usize;
            let counts: Vec<u64> = st.counters[lo..hi.min(st.counters.len())]
                .iter()
                .map(|c| c.load(Ordering::Relaxed))
                .collect();
            (e.hash.as_str(), counts)
        })
        .collect();
    recs.sort_by(|a, b| a.0.cmp(b.0));
    let samples: u64 = recs
        .iter()
        .flat_map(|(_, c)| c.iter())
        .fold(0u64, |a, &b| a.saturating_add(b));
    let mut out = format!("wprof {WPROF_VERSION}\nproducer instr\nruns 1\nsamples {samples}\n");
    for (hash, counts) in &recs {
        out.push_str("fn ");
        out.push_str(hash);
        out.push(' ');
        out.push_str(&counts.len().to_string());
        for c in counts {
            out.push(' ');
            out.push_str(&c.to_string());
        }
        out.push('\n');
    }
    out
}

/// Parse the compiler's index blob. Refuses anything it does not
/// fully understand — the same posture the `.wprof` reader takes.
fn parse_index(text: &str, total: u64) -> Result<(Vec<Entry>, String), String> {
    let mut lines = text.lines();
    let head = lines.next().unwrap_or("");
    let Some(v) = head.strip_prefix("wprof-index ") else {
        return Err("the profile index has no version header".to_string());
    };
    if v.trim().parse::<u32>() != Ok(INDEX_VERSION) {
        return Err(format!(
            "profile index v{}, but this runtime reads v{INDEX_VERSION} (compiler and runtime \
             archive are from different builds)",
            v.trim()
        ));
    }
    let mut path = String::from("default.wprof");
    let mut entries: Vec<Entry> = Vec::new();
    let mut declared: Option<u64> = None;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(p) = line.strip_prefix("path ") {
            path = p.to_string();
            continue;
        }
        if let Some(t) = line.strip_prefix("total ") {
            declared = t.trim().parse::<u64>().ok();
            continue;
        }
        let Some(rest) = line.strip_prefix("fn ") else {
            return Err(format!("unknown profile-index directive `{line}`"));
        };
        let mut it = rest.split_whitespace();
        let (Some(hash), Some(base), Some(blocks)) = (it.next(), it.next(), it.next()) else {
            return Err("malformed profile-index record".to_string());
        };
        let (Ok(base), Ok(blocks)) = (base.parse::<u32>(), blocks.parse::<u32>()) else {
            return Err("profile-index record with a non-numeric range".to_string());
        };
        if u64::from(base) + u64::from(blocks) > total {
            return Err("profile-index record runs past the counter array".to_string());
        }
        entries.push(Entry {
            hash: hash.to_string(),
            base,
            blocks,
        });
    }
    if declared != Some(total) {
        return Err("the profile index disagrees with the counter count".to_string());
    }
    Ok((entries, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(b: u8) -> String {
        std::iter::repeat_n(format!("{b:02x}"), 32).collect()
    }

    fn state(counts: &[u64], entries: Vec<Entry>) -> State {
        State {
            counters: counts.iter().map(|&c| AtomicU64::new(c)).collect(),
            entries,
            path: "default.wprof".to_string(),
        }
    }

    #[test]
    fn the_index_round_trips_its_ranges() {
        let text = format!(
            "wprof-index 1\npath out/p.wprof\ntotal 5\nfn {} 0 3\nfn {} 3 2\n",
            hash(0xaa),
            hash(0x0b)
        );
        let (entries, path) = parse_index(&text, 5).expect("parses");
        assert_eq!(path, "out/p.wprof");
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].base, entries[0].blocks), (0, 3));
        assert_eq!((entries[1].base, entries[1].blocks), (3, 2));
    }

    #[test]
    fn a_version_mismatch_refuses() {
        let e = parse_index("wprof-index 2\ntotal 0\n", 0).expect_err("refuses");
        assert!(e.contains("different builds"), "{e}");
    }

    #[test]
    fn a_missing_header_refuses() {
        assert!(parse_index("total 0\n", 0).is_err());
    }

    #[test]
    fn an_unknown_directive_refuses() {
        let text = format!("wprof-index 1\ntotal 1\nedges 2\nfn {} 0 1\n", hash(1));
        assert!(parse_index(&text, 1).is_err());
    }

    #[test]
    fn a_record_past_the_array_refuses() {
        let text = format!("wprof-index 1\ntotal 2\nfn {} 1 4\n", hash(1));
        assert!(parse_index(&text, 2).is_err());
    }

    #[test]
    fn a_total_that_disagrees_refuses() {
        let text = format!("wprof-index 1\ntotal 9\nfn {} 0 1\n", hash(1));
        assert!(parse_index(&text, 1).is_err());
    }

    #[test]
    fn render_emits_canonical_wprof_v1() {
        let st = state(
            &[10, 90, 5, 7],
            vec![
                Entry {
                    hash: hash(0xaa),
                    base: 0,
                    blocks: 3,
                },
                Entry {
                    hash: hash(0x0b),
                    base: 3,
                    blocks: 1,
                },
            ],
        );
        let text = render(&st);
        assert_eq!(
            text,
            format!(
                "wprof 1\nproducer instr\nruns 1\nsamples 112\nfn {} 1 7\nfn {} 3 10 90 5\n",
                hash(0x0b),
                hash(0xaa)
            ),
            "records ascend by hash and samples is the sum"
        );
    }

    #[test]
    fn an_all_zero_run_still_renders_a_valid_profile() {
        let st = state(
            &[0, 0],
            vec![Entry {
                hash: hash(3),
                base: 0,
                blocks: 2,
            }],
        );
        let text = render(&st);
        assert!(text.starts_with("wprof 1\nproducer instr\n"));
        assert!(text.contains("samples 0"));
    }

    #[test]
    fn bumping_before_init_is_a_no_op_not_a_crash() {
        // STATE is process-global and this test must not install one;
        // the guard being exercised is the `STATE.get()` early return.
        __wolf_rt_prof_bump(0);
        __wolf_rt_prof_bump(-1);
        __wolf_rt_prof_bump(i64::MAX);
    }

    #[test]
    fn dumping_before_init_writes_nothing() {
        __wolf_rt_prof_dump();
        assert!(
            !std::path::Path::new("default.wprof").exists()
                || std::env::var("WOLF_PROFILE_FILE").is_ok(),
            "an uninitialized runtime never writes a profile"
        );
    }
}
