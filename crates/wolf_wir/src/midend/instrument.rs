//! Profile instrumentation (s45 target 1) — counters at the **WIR
//! level**, on the post-mid-end CFG.
//!
//! # Why WIR and not `-fprofile-generate`
//!
//! The consumers are wolf's own passes: the s42 inliner and the s43
//! clusterer read hotness, and only the block-placement half ever
//! reaches LLVM. Counters that lived in LLVM's instrumentation would be
//! invisible to both, and would not survive a backend swap — D1's
//! "LLVM is rented" posture is not decorative. Counting in WIR also
//! means the counts are positional over the SAME canonical block order
//! ([`crate::print::block_order`]) that the D8 content hash is taken
//! over, which is what makes a `.wprof` record structurally valid
//! against the body it names ([`crate::profile`]).
//!
//! # Where in the pipeline
//!
//! **After** the whole-program phase, never during it. The hash a
//! record is keyed by is the hash of the body the profiled binary
//! actually ran, i.e. the FINAL optimized body — the same hash the
//! frozen summary index publishes and the cluster cache key folds. So:
//! optimize, hash, then instrument. Instrumentation is the last thing
//! that touches the module before lowering, and an instrumented module
//! is never hashed again.
//!
//! # What is inserted
//!
//! One `__wolf_rt_prof_bump(i)` call at the head of every block, `i`
//! being that block's index in a flat, program-wide counter array
//! (function base + block position). Plus, in `main`:
//!
//! - `__wolf_rt_prof_init(index, len, total)` at the top of the entry
//!   block — `index` is a `data.addr` to the read-only index blob
//!   described below;
//! - `__wolf_rt_prof_dump()` before every `ret`, which is the clean
//!   exit. The runtime covers the other exits (trap, `os.exit`,
//!   error-returning `main`) from its own side, and the dump is
//!   idempotent, so no path double-writes and no path is missed.
//!
//! Instrumented code is ordinary code: it lowers through s41 like
//! anything else, and it changes performance, never behaviour.
//!
//! # The stamp
//!
//! An instrumented binary is marked by construction: it is the only
//! shape of wolf binary that references `__wolf_rt_prof_init`. The
//! driver refuses to put one in the release object cache, and the
//! symbol is what a test (or a suspicious human with `nm`) checks.
//!
//! # The index blob
//!
//! The runtime knows counter indices; only the compiler knows which
//! body each index belongs to. The bridge is one read-only blob of
//! module data, emitted here and parsed once at `prof_init`:
//!
//! ```text
//! wprof-index 1
//! path <output path>
//! total <n>
//! fn <content hash> <base> <blocks>
//! ```
//!
//! It is deliberately the same shape of line-oriented text as the
//! `.wprof` it helps produce — `wolf_rt` stays dependency-thin (D15),
//! and "parse four keywords" is the whole of its obligation.

use crate::ir::{Aux, Module, Param};
use crate::ops::Opcode;
use crate::types;

/// The index-blob format version. Purely internal (compiler and
/// runtime ship together in one binary and one archive), but versioned
/// anyway so a mismatched pair refuses instead of misreading.
pub const INDEX_VERSION: u32 = 1;

/// The runtime entry points instrumentation calls. Named here so the
/// driver's "is this an instrumented build" check and the runtime's
/// exports have one source of truth.
pub const RT_INIT: &str = "__wolf_rt_prof_init";
pub const RT_BUMP: &str = "__wolf_rt_prof_bump";
pub const RT_DUMP: &str = "__wolf_rt_prof_dump";

/// The default profile output path, when `--profile-gen` names no
/// directory and `WOLF_PROFILE_FILE` is unset.
pub const DEFAULT_PROFILE_FILE: &str = "default.wprof";

/// What one instrumentation run produced — the evidence surface.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstrumentStats {
    /// Functions given counters.
    pub funcs: usize,
    /// Counters allocated (one per block, program-wide).
    pub counters: usize,
    /// Bytes of read-only index blob emitted.
    pub index_bytes: usize,
    /// Whether an entry point was found to init/dump from. `false`
    /// means the module is library-shaped and nothing will ever be
    /// written — the driver says so rather than shipping a binary that
    /// silently produces no profile.
    pub has_entry: bool,
}

/// Insert counters into every block of every function in `m`, and the
/// init/dump calls into `main`. `out_path` is baked into the binary as
/// the default dump destination (the runtime's `WOLF_PROFILE_FILE`
/// overrides it).
///
/// Deterministic: functions in arena order, blocks in canonical
/// [`crate::print::block_order`], counter indices assigned in that
/// order. Two instrumented builds of one commit are identical.
pub fn instrument(m: &mut Module, out_path: &str) -> InstrumentStats {
    let mut stats = InstrumentStats::default();

    // ---- 1. plan: hash every body BEFORE touching it ------------------
    // The record key is the hash of the body that RAN in the profiled
    // binary's optimized form, which is this one — the instrumentation
    // itself is not part of what a later build will hash.
    let mut plan: Vec<(String, u32, u32)> = Vec::new(); // (hash, base, blocks)
    let mut base = 0u32;
    for (_, f) in m.funcs.iter() {
        let blocks = crate::print::block_order(f).len() as u32;
        plan.push((super::dedup::body_hash(m, f), base, blocks));
        base += blocks;
    }
    let total = base;
    stats.funcs = plan.len();
    stats.counters = total as usize;

    // ---- 2. the index blob -------------------------------------------
    let mut index = format!("wprof-index {INDEX_VERSION}\npath {out_path}\ntotal {total}\n");
    for (hash, base, blocks) in &plan {
        index.push_str(&format!("fn {hash} {base} {blocks}\n"));
    }
    stats.index_bytes = index.len();
    let index_data = m.intern_data(index.as_bytes());

    // ---- 3. signatures ------------------------------------------------
    let bump_sig = m.make_sig(vec![Param::val(types::I64)], vec![]);
    let init_sig = m.make_sig(
        vec![
            Param::val(types::PTR),
            Param::val(types::I64),
            Param::val(types::I64),
        ],
        vec![],
    );
    let dump_sig = m.make_sig(vec![], vec![]);
    m.add_decl(RT_BUMP, bump_sig);
    m.add_decl(RT_INIT, init_sig);
    m.add_decl(RT_DUMP, dump_sig);

    // ---- 4. rewrite every body ---------------------------------------
    let ids: Vec<_> = m.funcs.keys().collect();
    for (fi, fid) in ids.into_iter().enumerate() {
        let (_, fbase, _) = plan[fi];
        let is_entry = m.funcs[fid].name == "main";
        let order = crate::print::block_order(&m.funcs[fid]);
        let f = &mut m.funcs[fid];
        let bump = f.import_func(RT_BUMP, bump_sig);
        for (k, &b) in order.iter().enumerate() {
            let mark = f.blocks[b].insts.len();
            let idx = f
                .append_inst(
                    b,
                    Opcode::Iconst,
                    &[],
                    &[types::I64],
                    Aux::Int(i64::from(fbase + k as u32)),
                )
                .1[0];
            f.append_inst(b, Opcode::Call, &[idx], &[], Aux::Callee(bump));
            hoist_to_front(f, b, mark);
        }
        if !is_entry {
            continue;
        }
        stats.has_entry = true;
        // `main`'s entry block: init before anything, including its own
        // counter bump (a counter incremented before the array exists
        // is dropped by the runtime, but ordering it correctly is free).
        let entry = f.entry().expect("verified function has an entry");
        let init = f.import_func(RT_INIT, init_sig);
        let mark = f.blocks[entry].insts.len();
        let ptr = f
            .append_inst(
                entry,
                Opcode::DataAddr,
                &[],
                &[types::PTR],
                Aux::Data(index_data),
            )
            .1[0];
        let len = f
            .append_inst(
                entry,
                Opcode::Iconst,
                &[],
                &[types::I64],
                Aux::Int(index.len() as i64),
            )
            .1[0];
        let n = f
            .append_inst(
                entry,
                Opcode::Iconst,
                &[],
                &[types::I64],
                Aux::Int(i64::from(total)),
            )
            .1[0];
        f.append_inst(entry, Opcode::Call, &[ptr, len, n], &[], Aux::Callee(init));
        hoist_to_front(f, entry, mark);
        // Every `ret`: dump immediately before it. This is the clean
        // exit; the runtime owns the others.
        let dump = f.import_func(RT_DUMP, dump_sig);
        for &b in &order {
            let Some(pos) = f.blocks[b]
                .insts
                .iter()
                .position(|&i| f.insts[i].op == Opcode::Ret)
            else {
                continue;
            };
            let mark = f.blocks[b].insts.len();
            f.append_inst(b, Opcode::Call, &[], &[], Aux::Callee(dump));
            let moved: Vec<_> = f.blocks[b].insts.split_off(mark);
            f.blocks[b].insts.splice(pos..pos, moved);
        }
    }
    stats
}

/// Move the instructions appended past `mark` to the front of `b`,
/// preserving their order — the "insert at the head" primitive the
/// arena's append-only shape does not have natively.
fn hoist_to_front(f: &mut crate::ir::Function, b: crate::ir::Block, mark: usize) {
    let moved: Vec<_> = f.blocks[b].insts.split_off(mark);
    f.blocks[b].insts.splice(0..0, moved);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Module;

    /// A module with `main` calling a leaf, both multi-block.
    fn fixture() -> Module {
        let text = "\
fn @leaf(i64) -> i64 {
b0(%0: i64):
  %1 = iconst.i64 0
  %2 = icmp.sgt %0, %1
  br %2, b1, b2
b1:
  jmp b3(%0)
b2:
  jmp b3(%1)
b3(%3: i64):
  ret %3
}
fn @main() -> i64 {
b0:
  %0 = iconst.i64 7
  %1 = call @leaf(%0)
  ret %1
}
";
        crate::parse_module(text).expect("fixture parses")
    }

    #[test]
    fn every_block_gets_exactly_one_counter() {
        let mut m = fixture();
        let before: Vec<usize> = m
            .funcs
            .values()
            .map(|f| crate::print::block_order(f).len())
            .collect();
        let stats = instrument(&mut m, "default.wprof");
        assert_eq!(stats.funcs, 2);
        assert_eq!(stats.counters, before.iter().sum::<usize>());
        assert!(stats.has_entry);
        for (_, f) in m.funcs.iter() {
            for &b in &crate::print::block_order(f) {
                let bumps = f.blocks[b]
                    .insts
                    .iter()
                    .filter(|&&i| {
                        f.insts[i].op == Opcode::Call
                            && matches!(f.insts[i].aux, Aux::Callee(ef) if f.ext_funcs[ef].name == RT_BUMP)
                    })
                    .count();
                assert_eq!(bumps, 1, "one bump per block in @{}", f.name);
            }
        }
    }

    #[test]
    fn counter_indices_are_dense_and_program_wide() {
        let mut m = fixture();
        let stats = instrument(&mut m, "default.wprof");
        let mut seen: Vec<i64> = Vec::new();
        for (_, f) in m.funcs.iter() {
            for &b in &f.layout {
                for &i in &f.blocks[b].insts {
                    if f.insts[i].op == Opcode::Call
                        && let Aux::Callee(ef) = f.insts[i].aux
                        && f.ext_funcs[ef].name == RT_BUMP
                    {
                        let arg = f.vpool.get(f.insts[i].args)[0];
                        let crate::ir::ValueDef::Result(def, _) = f.values[arg].def else {
                            panic!("bump argument is not an iconst result");
                        };
                        let Aux::Int(v) = f.insts[def].aux else {
                            panic!("bump argument is not an iconst");
                        };
                        seen.push(v);
                    }
                }
            }
        }
        seen.sort_unstable();
        let want: Vec<i64> = (0..stats.counters as i64).collect();
        assert_eq!(seen, want, "dense 0..total, no gaps, no repeats");
    }

    #[test]
    fn the_bump_is_the_first_instruction_of_its_block() {
        let mut m = fixture();
        instrument(&mut m, "default.wprof");
        for (_, f) in m.funcs.iter() {
            if f.name == "main" {
                continue; // main's entry leads with the init sequence
            }
            for &b in &crate::print::block_order(f) {
                let first = f.blocks[b].insts[0];
                assert_eq!(f.insts[first].op, Opcode::Iconst);
                let second = f.blocks[b].insts[1];
                assert_eq!(f.insts[second].op, Opcode::Call);
            }
        }
    }

    #[test]
    fn main_inits_first_and_dumps_before_every_ret() {
        let mut m = fixture();
        instrument(&mut m, "profiles/x.wprof");
        let f = m
            .funcs
            .values()
            .find(|f| f.name == "main")
            .expect("main survives");
        let entry = f.entry().expect("entry");
        let calls: Vec<&str> = f.blocks[entry]
            .insts
            .iter()
            .filter(|&&i| f.insts[i].op == Opcode::Call)
            .map(|&i| match f.insts[i].aux {
                Aux::Callee(ef) => f.ext_funcs[ef].name.as_str(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            calls.first().copied(),
            Some(RT_INIT),
            "init runs before any counter: {calls:?}"
        );
        // Dump sits immediately before the `ret`, not after it.
        let insts = &f.blocks[entry].insts;
        let ret = insts
            .iter()
            .position(|&i| f.insts[i].op == Opcode::Ret)
            .expect("main returns");
        let before = f.insts[insts[ret - 1]];
        assert!(
            matches!(before.aux, Aux::Callee(ef) if f.ext_funcs[ef].name == RT_DUMP),
            "the dump is the last thing before the return"
        );
    }

    #[test]
    fn the_index_blob_names_every_body_by_content_hash() {
        let mut m = fixture();
        // Hashes taken before instrumentation are what the blob claims.
        let want: Vec<String> = m
            .funcs
            .values()
            .map(|f| super::super::dedup::body_hash(&m, f))
            .collect();
        let stats = instrument(&mut m, "default.wprof");
        let blob = m
            .data
            .iter()
            .map(|d| String::from_utf8_lossy(&d.bytes).into_owned())
            .find(|t| t.starts_with("wprof-index "))
            .expect("the index blob is module data");
        assert_eq!(blob.len(), stats.index_bytes);
        assert!(blob.contains("\npath default.wprof\n"), "{blob}");
        assert!(
            blob.contains(&format!("\ntotal {}\n", stats.counters)),
            "{blob}"
        );
        for h in &want {
            assert!(blob.contains(h), "index names body {h}:\n{blob}");
        }
    }

    #[test]
    fn an_instrumented_module_still_verifies() {
        let mut m = fixture();
        instrument(&mut m, "default.wprof");
        crate::verify::verify_module(&m).expect("instrumented module verifies");
    }

    #[test]
    fn instrumentation_is_deterministic() {
        let mut a = fixture();
        let mut b = fixture();
        instrument(&mut a, "default.wprof");
        instrument(&mut b, "default.wprof");
        assert_eq!(crate::print_module(&a), crate::print_module(&b));
    }

    #[test]
    fn a_library_shaped_module_reports_no_entry() {
        let m0 = "\
fn @f(i64) -> i64 {
b0(%0: i64):
  ret %0
}
";
        let mut m = crate::parse_module(m0).expect("parses");
        let stats = instrument(&mut m, "default.wprof");
        assert!(
            !stats.has_entry,
            "no `main` means nothing will ever dump, and the driver must say so"
        );
    }
}
