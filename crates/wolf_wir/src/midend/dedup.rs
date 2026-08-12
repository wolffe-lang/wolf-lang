//! WIR-level content-hash deduplication (s43 target 3, D8 — the
//! volume killer).
//!
//! Two bodies that differ in nothing but their local names are ONE
//! body. Wolf kills the duplicate here, before the cache/backend
//! boundary — not in the linker, not in LLVM's `mergefunc` at the end
//! of a pipeline that already paid to optimize both copies. This is
//! the structural immunity to rustc's IR-volume disease: the backend
//! never sees the duplicate at all.
//!
//! # Normalization (the I11 designer-resolved rule)
//!
//! The hash input is the CANONICAL PRINT of the body with the
//! function's own name and linkage erased. The printer already
//! normalizes exactly what I11 licenses and nothing more:
//! - blocks in reverse postorder from the entry (unreachable blocks
//!   are gone by pass-boundary compaction),
//! - values numbered positionally in definition order along that
//!   layout — the "up to local names only" clause,
//! - facts rendered and SORTED (side tables sorted),
//! - source spans and debug-variable names excluded (non-canonical by
//!   the printer's contract).
//!
//! Everything else hashes: opcodes, operand order, types (region ids
//! included — region identity lives in the type, so two bodies over
//! different regions are different bodies), signatures, callee names,
//! data references, error tags. Two bodies differing in any
//! instruction, type, or fact hash apart — no cleverness beyond
//! renaming, exactly as ruled.
//!
//! The hash is taken POST-mid-end (D8): instantiations converge only
//! after s42 has folded their type-dependent differences away.
//!
//! # What is NOT merged
//!
//! - **Exported** functions (D19 C membrane): the linker symbol must
//!   exist at its own name.
//! - **`main`**: the entry shim imports it by name.
//! - **Address-taken** functions (`func.addr`, i.e. every task entry):
//!   merging them would make two distinct entries compare equal as
//!   pointers. Conservative and cheap — task entries are few.
//!
//! Survivors keep their name; the merged duplicates' call sites are
//! retargeted to the representative (direct calls, no PLT-style
//! indirection). Release-tier debug info is `DwarfFidelity::None`
//! (s41), so the contract's "alias → DW entries" obligation has no
//! surface to land on yet — recorded for c09's closeout and s58.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::entity::{EntityRef, PrimaryMap};
use crate::ir::{Aux, ExtFunc, ExtFuncData, Function, Module};
use crate::ops::Opcode;

/// The D8 content hash of one body: normalized, name-erased.
pub fn body_hash(m: &Module, f: &Function) -> String {
    crate::hash::sha256_hex(normal_form(m, f).as_bytes())
}

/// The exact bytes [`body_hash`] hashes — the debugging surface (and
/// what a hash-collision report would have to show).
pub fn normal_form(m: &Module, f: &Function) -> String {
    // The printer renders the header from the function's own name and
    // export flag; erase both, keep the signature (a body is only the
    // same body at the same signature).
    let mut anon = f.clone();
    anon.name = "$".to_string();
    anon.export = false;
    crate::print::print_function(m, &anon)
}

/// What one dedup run did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DedupStats {
    /// Bodies considered (every function in the module).
    pub bodies_seen: usize,
    /// Distinct content-hash classes among them.
    pub bodies_unique: usize,
    /// Bodies merged away into a representative.
    pub bodies_merged: usize,
    /// Call sites (and `func.addr` sites) retargeted.
    pub sites_retargeted: usize,
}

impl DedupStats {
    /// Instantiations-to-unique-bodies ratio — the tracked health
    /// metric (D8). `None` when nothing was compiled.
    pub fn ratio(&self) -> Option<f64> {
        if self.bodies_unique == 0 {
            return None;
        }
        Some(self.bodies_seen as f64 / self.bodies_unique as f64)
    }
}

/// Merge content-identical bodies; retarget every reference by name.
/// Deterministic: classes are keyed by content hash, and the
/// representative of a class is its lexicographically smallest
/// mergeable name (never a wall-clock or arena-order choice).
pub fn dedup(m: &mut Module) -> DedupStats {
    let mut stats = DedupStats::default();
    // Address-taken names: a `func.addr` anywhere pins the identity.
    let mut pinned: BTreeSet<String> = BTreeSet::new();
    for (_, f) in m.funcs.iter() {
        for &b in &f.layout {
            for &inst in &f.blocks[b].insts {
                if f.insts[inst].op == Opcode::FuncAddr
                    && let Aux::Callee(ef) = f.insts[inst].aux
                {
                    pinned.insert(f.ext_funcs[ef].name.clone());
                }
            }
        }
    }
    // Classes: content hash → mergeable member names (sorted).
    let mut classes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut all_hashes: BTreeSet<String> = BTreeSet::new();
    for (_, f) in m.funcs.iter() {
        stats.bodies_seen += 1;
        let h = body_hash(m, f);
        all_hashes.insert(h.clone());
        if f.export || f.name == "main" || pinned.contains(&f.name) {
            continue;
        }
        classes.entry(h).or_default().push(f.name.clone());
    }
    stats.bodies_unique = all_hashes.len();
    // rename: merged name → representative name.
    let mut rename: BTreeMap<String, String> = BTreeMap::new();
    for members in classes.values_mut() {
        if members.len() < 2 {
            continue;
        }
        members.sort();
        let rep = members[0].clone();
        for dup in &members[1..] {
            rename.insert(dup.clone(), rep.clone());
        }
    }
    if rename.is_empty() {
        return stats;
    }
    stats.bodies_merged = rename.len();
    for id in m.funcs.keys().collect::<Vec<_>>() {
        stats.sites_retargeted += retarget(&mut m.funcs[id], &rename);
    }
    // Drop the merged bodies. FuncIds are positional and nothing keys
    // on them across a Module (callees resolve by NAME), so the table
    // rebuilds in place — the same discipline as dead-function
    // elimination.
    let old = std::mem::replace(&mut m.funcs, PrimaryMap::new());
    for (_, f) in old.iter() {
        if !rename.contains_key(&f.name) {
            m.funcs.push(f.clone());
        }
    }
    stats
}

/// Rewrite `f`'s callee table through `rename`, deduplicating the
/// resulting imports (two imports of one name would print one `decl`
/// but declare the symbol twice to a backend). Returns the number of
/// instructions whose callee moved.
fn retarget(f: &mut Function, rename: &BTreeMap<String, String>) -> usize {
    if f.ext_funcs.is_empty() {
        return 0;
    }
    let mut fresh: PrimaryMap<ExtFunc, ExtFuncData> = PrimaryMap::new();
    let mut seen: HashMap<(String, crate::ir::SigId), ExtFunc> = HashMap::new();
    let mut map: Vec<ExtFunc> = Vec::with_capacity(f.ext_funcs.len());
    let mut moved: Vec<bool> = Vec::with_capacity(f.ext_funcs.len());
    for ef in f.ext_funcs.values() {
        let new_name = rename.get(&ef.name).cloned();
        moved.push(new_name.is_some());
        let name = new_name.unwrap_or_else(|| ef.name.clone());
        let key = (name.clone(), ef.sig);
        let id = *seen
            .entry(key)
            .or_insert_with(|| fresh.push(ExtFuncData { name, sig: ef.sig }));
        map.push(id);
    }
    f.ext_funcs = fresh;
    let mut sites = 0usize;
    for inst in f.insts.keys().collect::<Vec<_>>() {
        if let Aux::Callee(ef) = f.insts[inst].aux {
            let i = ef.index();
            if moved[i] {
                sites += 1;
            }
            f.insts[inst].aux = Aux::Callee(map[i]);
        }
    }
    sites
}
