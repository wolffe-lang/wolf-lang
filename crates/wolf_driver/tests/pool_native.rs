//! s173 — the pool's runtime shape, the handle, and the unsafe tier.
//!
//! # The prediction, written before the first change
//!
//! This header is committed BEFORE any source edit, so the table can
//! be read against what was measured rather than rewritten after it.
//! Nothing in this file measures anything yet; the gates land in the
//! commits that follow.
//!
//! ## The baseline, re-derived rather than copied
//!
//! The contract's inputs line gives s168's `lane-coverage` "after"
//! column as `634/364/405/404/419/350`. Measured here at trunk
//! `2f8deb7f` on kasumi (linux x86-64), `cargo xtask lane-coverage`:
//!
//! ```text
//! 637 non-member corpus entries
//!   checked  executes 364 at run
//!   native   executes 408 at run
//!   release  executes 407 at run
//!   UNION 422 of 637 — all three 350
//! ```
//!
//! So `637/364/408/407/422/350`. Three of the six numbers moved after
//! s168's report: s170 landed the conc surface between them. The
//! drift is the orchestrator's, not a defect, and it is recorded
//! rather than absorbed.
//!
//! ## What is predicted
//!
//! | # | claim | falsified by |
//! |---|---|---|
//! | P1 | `Pool[T]` is one runtime header pointer and `handle T` one packed `i64` (index in the low half, generation in the high half), so both lower with no new backend shape | a backend that needs a third WIR type for either |
//! | P2 | the three `phase: mem` Pool witnesses — `corpus/memory/handle_stale.lu`, `corpus/memory/region_multiopen_swap.lu`, `corpus/regions.lu` — all advance to `phase: run` and are executed by native and release | any one of the three still refused by the native lane at the end |
//! | P3 | four of the five unsafe-tier witnesses advance `mem → run`: `corpus/memory/unsafe_noalias.lu`, `corpus/memory/unsafe_creation_not_use.lu`, `corpus/lints/assume_reassigned.lu`, `corpus/lints/safety_comment_missing.lu`. `corpus/memory/unsafe_ub_uaf.lu` does NOT — its header asks for `trap(ub)`, which is the CHECKED build's answer to a use-after-free, and a native build's answer to the same program is undefined by `[mem.unsafe.raw.1]` | any of the four still refused; or `unsafe_ub_uaf.lu` advancing |
//! | P4 | two new three-lane corpus entries land: the `pool[h].next = k` place write (wolf-lang#31's LRU centrepiece) and the Pool accessor set | fewer or more than two |
//! | P5 | `lane-coverage` after: **639 / 366 / 417 / 416 / 424 / 359** — entries +2, checked +2, native +9 (3 Pool movers, 4 unsafe movers, 2 new), release +9, union +2 (the seven movers are already in the union through checked), all-three +9 | any cell off by more than 0 |
//! | P6 | `wolf_rt`'s quarantine allocator stops being `unimplemented!`, and the test that proves it is seen RED at trunk before the fix — `alloc` returns an address with a nonzero tag, `check` accepts it, `free` then makes `check` answer `UseAfterFree` | the test passing at trunk |
//! | P7 | the `Shared[T]` / `Weak[T]` half of `TyKind`'s shared tier stays refused by name. It is the rc row on wolf-lang#268 and it is NOT this lane's: a pool slot's liveness is a generation compare, a cell's is a refcount, and the second needs a drop protocol the native pipe does not have | a shared-cell constructor lowering here |
//! | P8 | no file under `spec/` is touched. Every clause the work wants is written in the report as a proposal with its witness | one byte of `spec/` in this branch's diff |
//!
//! P5 is the falsifiable one; P2, P3 and P6 say which artifacts carry
//! it. P7 is a prediction about scope, and saying it here is what
//! makes "not done" a result rather than an omission.
//!
//! ## Which wolf-lang#268 rows this expects to retire
//!
//! | rows | the compiler's own words, as #268 quotes them |
//! |---|---|
//! | ×6 | `Pool/shared constructor lowering (runtime shapes, c06)` — the Pool half only |
//! | ×3 | `indexing outside str/List (Pool/Map runtime shapes, c06/std)` — the Pool and raw-pointer halves |
//! | ×3 | `index writes outside List (raw-pointer writes c10; Pool/Map c06/std)` |
//! | ×3 | `raw-pointer casts (unsafe-tier WIR ops, deferred from s26 — see closeout)` |
//! | ×2 | `assume noalias (unsafe-tier WIR ops, deferred from s26 — see closeout)` |
//!
//! #268's table is quoted at wolf 0.2.7. s157 has since struck the
//! sprint ids out of every one of those strings, which is why a grep
//! of `wolf_wir/src` for `c06|s26|s54` finds none of them today. The
//! strings survive; the ids do not, and `cargo xtask print-gate`
//! keeps it that way.

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
}

/// Run one entry on one lane. `None` means the host cannot drive the
/// native lane (the s59 skip pattern) — a loud skip, never a silent
/// pass.
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag != "--checked" {
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
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

fn case(name: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, src).expect("write fixture");
    entry
}

/// Every lane runs it, every lane exits 0, and every lane prints
/// `want`. Equality alone is not the assertion — `want` is written
/// out here so two lanes cannot be wrong together and still pass,
/// which is the s171 lesson.
fn every_lane_says(entry: &Path, want: &str) {
    let checked = lane(entry, "--checked").expect("the checked lane always runs");
    assert_eq!(checked.verdict, "exit(0)", "checked verdict");
    assert_eq!(checked.stdout, want, "the CHECKED lane's answer");
    for flag in ["--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            continue;
        };
        assert_eq!(obs.verdict, "exit(0)", "{flag} verdict");
        assert_eq!(obs.stdout, want, "the {flag} lane's answer");
    }
}

/// Every lane traps, with the same trap identity.
fn every_lane_traps(entry: &Path, kind: &str) {
    for flag in ["--checked", "--native", "--release"] {
        let Some(obs) = lane(entry, flag) else {
            continue;
        };
        assert_eq!(
            obs.verdict,
            format!("trap({kind})"),
            "{flag} did not trap {kind} on {}",
            entry.display()
        );
    }
}

/// The two-phase shape (`[mem.shared.handle.1]`): a handle exists
/// before the node it names does, which is how a cycle gets built
/// without a null.
#[test]
fn reserve_names_a_slot_before_init_fills_it() {
    let entry = case(
        "s173_two_phase",
        r#"
struct Node { value: int, next: handle Node }

fn main() -> !int {
    region r: pool(Node) {
        var p = Pool[Node]()
        let a = (mut p).reserve()
        let b = (mut p).reserve()
        (mut p).init(a, Node { value: 1, next: b })
        (mut p).init(b, Node { value: 2, next: a })
        print("{p[a].value} {p[b].value} {p[p[a].next].value}")
        0
    }
}
"#,
    );
    every_lane_says(&entry, "1 2 2\n");
}

/// wolf-lang#31: the place write through a handle. The sibling slot
/// and the sibling FIELD must both be untouched — a read-modify-write
/// of the whole node would be indistinguishable on one field and
/// wrong on two, which is why four numbers are printed.
#[test]
fn a_field_of_a_pool_slot_is_a_place() {
    let entry = case(
        "s173_place_write",
        r#"
struct Node { value: int, next: handle Node }

fn main() -> !int {
    region r: pool(Node) {
        var p = Pool[Node]()
        let a = (mut p).reserve()
        let b = (mut p).reserve()
        (mut p).init(a, Node { value: 1, next: a })
        (mut p).init(b, Node { value: 2, next: b })
        p[a].next = b
        p[b].value = 20
        print("{p[a].value} {p[p[a].next].value} {p[b].value} {p[p[b].next].value}")
        0
    }
}
"#,
    );
    every_lane_says(&entry, "1 20 20 20\n");
}

/// `[mem.shared.handle.2]`: a stale handle is a DEFINED trap on every
/// lane — release included. This is the one that separates a handle
/// from a pointer, and the release tier is where a weaker design
/// would quietly stop checking.
#[test]
fn a_stale_handle_traps_on_every_lane_including_release() {
    let entry = case(
        "s173_stale_read",
        r#"
fn main() -> !int {
    region r: pool(int) {
        var p = Pool[int]()
        let h = (mut p).reserve()
        (mut p).init(h, 7)
        (mut p).remove(h)
        let v = p[h]
        v
    }
}
"#,
    );
    every_lane_traps(&entry, "stale-handle");
}

/// The write half of the same rule: a stale handle traps at an
/// `init`, at a `remove`, and at a place write, not only at a read.
#[test]
fn every_seam_through_a_stale_handle_traps() {
    for (name, body) in [
        ("init", "(mut p).init(h, 9)"),
        ("remove", "(mut p).remove(h)"),
        ("write", "p[h] = 9"),
    ] {
        let entry = case(
            &format!("s173_stale_{name}"),
            &format!(
                r#"
fn main() -> !int {{
    region r: pool(int) {{
        var p = Pool[int]()
        let h = (mut p).reserve()
        (mut p).init(h, 7)
        (mut p).remove(h)
        {body}
        0
    }}
}}
"#
            ),
        );
        every_lane_traps(&entry, "stale-handle");
    }
}

/// `alive` is the ONE seam a stale handle reaches without faulting.
/// If it trapped too there would be no way to ask the question, and
/// the type would be unusable.
#[test]
fn the_liveness_probe_does_not_trap_on_a_stale_handle() {
    let entry = case(
        "s173_alive_probe",
        r#"
fn main() -> !int {
    region r: pool(int) {
        var p = Pool[int]()
        let h = (mut p).reserve()
        (mut p).init(h, 7)
        let before = p.alive(h)
        (mut p).remove(h)
        let after = p.alive(h)
        print("{before} {after} {p.len()}")
        0
    }
}
"#,
    );
    every_lane_says(&entry, "true false 0\n");
}

/// A reused slot answers the NEW handle and not the old one. Without
/// the generation this is the classic slot-map bug: the index is
/// valid, the tenant is not.
#[test]
fn a_reused_slot_does_not_answer_the_handle_of_its_previous_tenant() {
    let entry = case(
        "s173_reuse",
        r#"
fn main() -> !int {
    region r: pool(int) {
        var p = Pool[int]()
        let first = (mut p).reserve()
        (mut p).init(first, 1)
        (mut p).remove(first)
        let second = (mut p).reserve()
        (mut p).init(second, 2)
        print("{p.alive(first)} {p.alive(second)} {p[second]}")
        0
    }
}
"#,
    );
    every_lane_says(&entry, "false true 2\n");
}

/// The pool grows, and growth moves the payload buffer. Every handle
/// issued before the growth must still resolve — a handle is an index
/// and a generation, so it survives a reallocation that would have
/// invalidated a pointer. This is the test that would fail if the
/// slot address were cached across a `reserve`.
#[test]
fn growth_keeps_every_earlier_handle_valid() {
    let entry = case(
        "s173_growth",
        r#"
fn main() -> !int {
    region r: pool(int) {
        var p = Pool[int]()
        var hs = List[handle int]()
        for i in 0..64 {
            let h = (mut p).reserve()
            (mut p).init(h, i)
            (mut hs).push(h)
        }
        var sum = 0
        for i in 0..64 { sum += p[hs[i]] }
        print("{sum} {p.len()}")
        0
    }
}
"#,
    );
    // 0 + 1 + ... + 63 = 2016.
    every_lane_says(&entry, "2016 64\n");
}

/// The hazard the lowering's design turns on, driven rather than
/// asserted in prose.
///
/// The first draft of this lane minted the slot address ONCE at the
/// place step, on the argument that `[mem.model.place]` collapses the
/// pool to one opaque place, so a second path to it inside the same
/// surface would be E1002 before it could run. **That argument is
/// false**, and this test is what falsified it: the sibling shape on
/// a `List` — `xs[0].n = grow(mut xs)` — is accepted by every lane
/// today and answers correctly only because `elem_addr` re-mints the
/// element address at the access. The exclusivity rule s168 landed
/// covers a nested call under a `mut` ARGUMENT, not the right-hand
/// side of an assignment.
///
/// So the pool keeps a recipe too, and the liveness check rides the
/// re-mint. Both halves are exercised here: growth moving the buffer
/// under a pending place, and a `remove` staling the slot under one.
#[test]
fn a_place_write_survives_the_pool_moving_under_it() {
    let entry = case(
        "s173_place_regrow",
        r#"
struct Node { value: int, next: handle Node }

fn grow(mut p: Pool[Node]) -> int {
    for i in 0..64 {
        let h = (mut p).reserve()
        (mut p).init(h, Node { value: i, next: h })
    }
    7
}

fn main() -> !int {
    region r: pool(Node) {
        var p = Pool[Node]()
        let a = (mut p).reserve()
        (mut p).init(a, Node { value: 1, next: a })
        // The right-hand side reserves 64 slots, which moves the
        // payload buffer. An address minted before it would be
        // dangling by the time the store lands.
        p[a].value = grow(mut p)
        print("{p[a].value} {p.len()}")
        0
    }
}
"#,
    );
    every_lane_says(&entry, "7 65\n");
}

/// The other half: the slot goes STALE under a pending place write.
/// The checked machine validates the generation at the access, so the
/// native lane has to fault in the same place — which is only
/// possible if the address is re-minted there.
#[test]
fn a_place_write_through_a_slot_removed_under_it_traps() {
    let entry = case(
        "s173_place_restale",
        r#"
fn kill(mut p: Pool[int], h: handle int) -> int {
    (mut p).remove(h)
    9
}

fn main() -> !int {
    region r: pool(int) {
        var p = Pool[int]()
        let a = (mut p).reserve()
        (mut p).init(a, 1)
        p[a] = kill(mut p, a)
        0
    }
}
"#,
    );
    every_lane_traps(&entry, "stale-handle");
}
