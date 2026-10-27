//! Recognize pure integer equality chains without changing WIR or its CFG.
//! Interior blocks remain available to other predecessors. We bypass only
//! parameterless blocks whose comparisons have no escaping uses. Integer
//! literals are rematerialized at entry; argument-carrying edges, effects,
//! and trapping tests retain their chain.

use std::collections::{HashMap, HashSet};
use wolf_wir::ir::{Aux, Block, Function, Inst, Module, Value, ValueDef};
use wolf_wir::ops::{IntCc, Opcode};
use wolf_wir::types::TypeData;

pub(crate) struct DenseMatch {
    pub value: Value,
    pub min: i64,
    pub targets: Vec<Block>,
    pub default: Block,
    // Rematerialize in the function entry, not the chain head: an
    // interior block may also be entered from a different predecessor.
    pub constants: Vec<(Value, i64)>,
}

fn constant(f: &Function, v: Value) -> Option<i64> {
    let ValueDef::Result(i, 0) = f.values[v].def else {
        return None;
    };
    match (f.insts[i].op, f.insts[i].aux) {
        (Opcode::Iconst, Aux::Int(n)) => Some(n),
        _ => None,
    }
}

fn case(f: &Function, i: Inst) -> Option<(Value, i64, Block, Block)> {
    let d = f.insts[i];
    if d.op != Opcode::Br {
        return None;
    }
    let Aux::Br(t, e) = d.aux else { return None };
    // Forwarding arguments would need per-case trampolines and remapping
    // of definitions in bypassed blocks. Do not guess at that remapping.
    if !f.vpool.get(t.args).is_empty() || !f.vpool.get(e.args).is_empty() {
        return None;
    }
    let cond = *f.vpool.get(d.args).first()?;
    let ValueDef::Result(cmp, 0) = f.values[cond].def else {
        return None;
    };
    let cmp = f.insts[cmp];
    if cmp.op != Opcode::Icmp || cmp.aux != Aux::IntCc(IntCc::Eq) {
        return None;
    }
    let args = f.vpool.get(cmp.args);
    if let Some(n) = constant(f, args[1]) {
        Some((args[0], n, t.block, e.block))
    } else {
        constant(f, args[0]).map(|n| (args[1], n, t.block, e.block))
    }
}

pub(crate) fn find(m: &Module, f: &Function) -> HashMap<Inst, DenseMatch> {
    // Count uses by their containing block, including block-call operands.
    let mut uses: HashMap<Value, HashSet<Block>> = HashMap::new();
    for &b in &f.layout {
        for &i in &f.blocks[b].insts {
            let d = f.insts[i];
            let mut args = f.vpool.get(d.args);
            match d.aux {
                Aux::Jump(e) => args.extend(f.vpool.get(e.args)),
                Aux::Br(t, e) => {
                    args.extend(f.vpool.get(t.args));
                    args.extend(f.vpool.get(e.args));
                }
                _ => {}
            }
            for v in args {
                uses.entry(v).or_default().insert(b);
            }
        }
    }
    let pure = |b: Block| {
        f.block_params(b).is_empty()
            && f.blocks[b].insts.iter().all(|&i| {
                matches!(f.insts[i].op, Opcode::Iconst | Opcode::Icmp | Opcode::Br)
                    && (f.insts[i].op == Opcode::Iconst
                        || f.vpool.get(f.insts[i].results).iter().all(|v| {
                            uses.get(v)
                                .is_none_or(|bs| bs.iter().all(|&use_b| use_b == b))
                        }))
            })
    };
    let mut candidates = Vec::new();
    for &b in &f.layout {
        let Some(&head) = f.blocks[b].insts.last() else {
            continue;
        };
        let Some((value, _, _, _)) = case(f, head) else {
            continue;
        };
        let bits = match m.types.get(f.value_ty(value)) {
            TypeData::I8 => 8,
            TypeData::I16 => 16,
            TypeData::I32 => 32,
            TypeData::I64 => 64,
            _ => continue,
        };
        // Equality is on bits; canonicalize narrow constants to the same
        // signed i64 representation used by the emitter's sign extension.
        let canonical = |n: i64| (n << (64 - bits)) >> (64 - bits);
        let mut cases = std::collections::BTreeMap::new();
        let mut chain = Vec::new();
        let mut constants = Vec::new();
        let mut seen = HashSet::new();
        let mut current = head;
        let default = loop {
            if !seen.insert(current) {
                break None;
            }
            let Some((v, n, t, e)) = case(f, current) else {
                break None;
            };
            if v != value {
                break None;
            }
            // Duplicate literals keep their first arm, exactly as the chain.
            cases.entry(canonical(n)).or_insert(t);
            chain.push(current);
            let next = f.blocks[e].insts.last().copied();
            if pure(e) && next.and_then(|i| case(f, i)).is_some_and(|c| c.0 == value) {
                for &i in &f.blocks[e].insts {
                    if let (Opcode::Iconst, Aux::Int(n)) = (f.insts[i].op, f.insts[i].aux) {
                        constants.push((f.vpool.get(f.insts[i].results)[0], n));
                    }
                }
                current = next.unwrap();
            } else {
                break Some(e);
            }
        };
        let Some(default) = default else { continue };
        if cases.len() < 8 {
            continue;
        }
        let min = *cases.first_key_value().unwrap().0;
        let max = *cases.last_key_value().unwrap().0;
        let span = i128::from(max) - i128::from(min) + 1;
        if span > (cases.len() as i128) * 4 {
            continue;
        }
        let mut targets = vec![default; span as usize];
        for (n, target) in cases {
            targets[(i128::from(n) - i128::from(min)) as usize] = target;
        }
        candidates.push((
            head,
            chain,
            DenseMatch {
                value,
                min,
                targets,
                default,
                constants,
            },
        ));
    }
    // Emit the longest chain first, not a table for every suffix. Original
    // interior branches remain intact, including any other incoming edges.
    candidates.sort_by_key(|(_, chain, _)| std::cmp::Reverse(chain.len()));
    let mut covered = HashSet::new();
    let mut out = HashMap::new();
    for (head, chain, table) in candidates {
        if covered.contains(&head) {
            continue;
        }
        covered.extend(chain);
        out.insert(head, table);
    }
    out
}
