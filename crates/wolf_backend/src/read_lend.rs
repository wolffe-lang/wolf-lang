//! Copy elision for a large aggregate lent from an exclusive address.
//!
//! `Lowerer::read_mut_ref` rebuilds a snapshot with `load_flat`. The
//! native ABI already passes aggregates larger than sixteen bytes by
//! pointer. At the last backend boundary we can use the original
//! address instead, without changing WIR's value semantics: the rebuilt
//! value has exactly one use, a read argument of an internal call;
//! every field is still the original byte at that point; and the
//! source is an exclusive `mut` parameter, suspended until the call
//! returns ([mem.tier0.mode.read], [mem.tier0.excl.1]).
//!
//! A write or opaque call during argument evaluation keeps the snapshot.
//! So do a second use, a non-exclusive source, a consuming argument, a
//! C/unknown callee, and calls carrying any `mut` argument. This last
//! restriction is deliberately conservative: a Copy aggregate may be
//! read alongside a conflicting `mut` argument's pre-call value.
//! The analysis runs after optimization, independently on each emitted
//! function. No pointer view escapes into WIR or changes its SSA rules.

use std::collections::HashMap;
use wolf_wir::ir::{Aux, Block, Function, Inst, Mode, Module, Value, ValueDef};
use wolf_wir::ops::Opcode;
use wolf_wir::types::TypeData;

use crate::layout::{field_offsets, layout_of};

/// Aggregate SSA values that may use their source address for one read call.
pub fn borrowed_aggregates(m: &Module, f: &Function) -> HashMap<Value, Value> {
    let mut uses: HashMap<Value, usize> = HashMap::new();
    let mut positions = HashMap::new();
    for &block in &f.layout {
        for (i, &inst) in f.blocks[block].insts.iter().enumerate() {
            positions.insert(inst, (block, i));
            let data = f.insts[inst];
            for v in f.vpool.get(data.args) {
                *uses.entry(v).or_default() += 1;
            }
            let edges = match data.aux {
                Aux::Jump(a) => vec![a],
                Aux::Br(a, b) => vec![a, b],
                _ => vec![],
            };
            for edge in edges {
                for v in f.vpool.get(edge.args) {
                    *uses.entry(v).or_default() += 1;
                }
            }
        }
    }
    let mut out = HashMap::new();
    for &block in &f.layout {
        let insts = &f.blocks[block].insts;
        // Prefix counts make the no-intervening-effect check constant
        // time even for the 400-field witness (or several arguments).
        let mut barriers = vec![0usize];
        for &inst in insts {
            let harmless = matches!(
                f.insts[inst].op,
                Opcode::Iconst
                    | Opcode::Fconst
                    | Opcode::Bconst
                    | Opcode::PtrOff
                    | Opcode::Load
                    | Opcode::AggMake
                    | Opcode::AggGet
            );
            barriers.push(barriers.last().unwrap() + usize::from(!harmless));
        }
        for (i, &inst) in insts.iter().enumerate() {
            let data = f.insts[inst];
            if data.op != Opcode::Call {
                continue;
            }
            let Aux::Callee(ext) = data.aux else {
                continue;
            };
            let callee = &f.ext_funcs[ext];
            let sig = &m.sigs[callee.sig];
            if sig.params.iter().any(|p| p.mode == Mode::Mut)
                || !m.funcs.values().any(|g| g.name == callee.name && !g.export)
            {
                continue;
            }
            for (v, param) in f.vpool.get(data.args).into_iter().zip(&sig.params) {
                if param.mode != Mode::Val
                    || uses.get(&v) != Some(&1)
                    || !matches!(m.types.get(param.ty), TypeData::Agg(_))
                    || !layout_of(&m.types, param.ty).is_some_and(|l| l.size > 16)
                {
                    continue;
                }
                let Some(image) = image(m, f, v, block, &positions, &uses) else {
                    continue;
                };
                let ValueDef::Param(entry, idx) = f.values[image.root].def else {
                    continue;
                };
                if Some(&entry) != f.layout.first()
                    || m.sigs[f.sig].params[idx as usize].mode != Mode::Mut
                    || image.first >= i
                    || barriers[image.first] != barriers[i]
                {
                    continue;
                }
                out.extend(image.views);
            }
        }
    }
    out
}

struct Image {
    root: Value,
    offset: u64,
    first: usize,
    ptr: Value,
    views: Vec<(Value, Value)>,
}

/// Recognize only an exact, naturally laid-out reconstruction. Layout
/// equality is essential: `mut` spill layout and ABI layout must agree,
/// including nested fields and padding. Reordered or mixed loads stay
/// copies, as do loads from another block.
fn image(
    m: &Module,
    f: &Function,
    v: Value,
    block: Block,
    positions: &HashMap<Inst, (Block, usize)>,
    uses: &HashMap<Value, usize>,
) -> Option<Image> {
    let ValueDef::Result(inst, 0) = f.values[v].def else {
        return None;
    };
    let &(at, pos) = positions.get(&inst)?;
    if at != block {
        return None;
    }
    let data = f.insts[inst];
    let args = f.vpool.get(data.args);
    if data.op == Opcode::Load && data.aux == Aux::None {
        let (root, offset) = address(f, args[0])?;
        return Some(Image {
            root,
            offset,
            first: pos,
            ptr: args[0],
            views: vec![],
        });
    }
    if data.op != Opcode::AggMake {
        return None;
    }
    let TypeData::Agg(fields) = m.types.get(f.value_ty(v)) else {
        return None;
    };
    let offsets = field_offsets(&m.types, fields)?;
    let mut parts = args.into_iter().zip(offsets);
    let (first, off) = parts.next()?;
    if off != 0 {
        return None;
    }
    let mut result = image(m, f, first, block, positions, uses)?;
    for (field, off) in parts {
        let part = image(m, f, field, block, positions, uses)?;
        if part.root != result.root || part.offset != result.offset.checked_add(off as u64)? {
            return None;
        }
        result.first = result.first.min(part.first);
        result.views.extend(part.views);
    }
    // Multiple uses can need this node's snapshot after the call; its
    // parent can still borrow, but this node must retain its own slot.
    if uses.get(&v) == Some(&1) {
        result.views.push((v, result.ptr));
    }
    Some(result)
}

fn address(f: &Function, mut ptr: Value) -> Option<(Value, u64)> {
    let mut offset = 0u64;
    // Each step follows an SSA operand backwards, so this terminates.
    while let ValueDef::Result(inst, 0) = f.values[ptr].def {
        let data = f.insts[inst];
        if data.op != Opcode::PtrOff {
            break;
        }
        let Aux::Scale(scale) = data.aux else {
            return None;
        };
        let args = f.vpool.get(data.args);
        let ValueDef::Result(k, 0) = f.values[args[1]].def else {
            return None;
        };
        if f.insts[k].op != Opcode::Iconst {
            return None;
        }
        let Aux::Int(n) = f.insts[k].aux else {
            return None;
        };
        offset = offset.checked_add(u64::try_from(n).ok()?.checked_mul(scale)?)?;
        ptr = args[0];
    }
    Some((ptr, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(before: &str, after: &str) -> String {
        format!(
            "decl @reader({{i64, i64, i64}}) -> i64
             fn @reader({{i64, i64, i64}}) -> i64 {{
             b0(%0: {{i64, i64, i64}}):
               %1 = agg.get %0, 0
               ret %1
             }}
             fn @caller(mut ptr, mem.r0) -> i64 {{
             b0(%0: ptr, %1: mem.r0):
               %2 = iconst.i64 8
               %3 = ptr.off %0, %2, 1
               %4 = iconst.i64 16
               %5 = ptr.off %0, %4, 1
               %6 = load.i64 %0, %1
               %7 = load.i64 %3, %1
               %8 = load.i64 %5, %1
               %9 = agg.make %6, %7, %8
               {before}
               %10 = call @reader(%9)
               {after}
               ret %10
             }}"
        )
    }

    fn count(src: &str) -> usize {
        let m = wolf_wir::parse_module(src).expect("parse");
        wolf_wir::verify_module(&m).expect("verify");
        let f = m.funcs.values().find(|f| f.name == "caller").unwrap();
        borrowed_aggregates(&m, f).len()
    }

    #[test]
    fn lends_the_exclusive_address_until_the_read_call_returns() {
        assert_eq!(count(&fixture("", "%11 = store.i64 %10, %0, %1")), 1);
    }

    #[test]
    fn keeps_a_snapshot_used_again_after_the_call() {
        assert_eq!(count(&fixture("", "%11 = agg.get %9, 1")), 0);
    }

    #[test]
    fn keeps_a_snapshot_across_an_intervening_write() {
        assert_eq!(count(&fixture("%11 = store.i64 %2, %0, %1", "")), 0);
    }

    #[test]
    fn keeps_a_snapshot_without_exclusive_provenance() {
        let src = fixture("", "").replace("@caller(mut ptr", "@caller(ptr");
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn keeps_reordered_fields() {
        let src = fixture("", "").replace("agg.make %6, %7, %8", "agg.make %8, %7, %6");
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn keeps_small_register_arguments() {
        let src = fixture("", "")
            .replace("{i64, i64, i64}", "{i64, i64}")
            .replace("agg.make %6, %7, %8", "agg.make %6, %7");
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn lends_nested_aggregates_with_matching_layout() {
        let src = fixture("", "")
            .replace("{i64, i64, i64}", "{i64, {i64, i64}}")
            .replace(
                "%9 = agg.make %6, %7, %8",
                "%inner = agg.make %7, %8\n%9 = agg.make %6, %inner",
            );
        assert_eq!(count(&src), 2);
    }

    #[test]
    fn keeps_a_snapshot_across_an_opaque_call() {
        let src = format!("decl @opaque()\n{}", fixture("call @opaque()", ""));
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn keeps_a_snapshot_when_fields_span_a_write() {
        let src = fixture("", "")
            .replace(
                "%7 = load.i64 %3, %1",
                "%write = store.i64 %2, %0, %1\n%7 = load.i64 %3, %write",
            )
            .replace("%8 = load.i64 %5, %1", "%8 = load.i64 %5, %write");
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn counts_uses_on_branch_edges() {
        let src = fixture("", "jmp b1(%9)\nb1(%again: {i64, i64, i64}):");
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn does_not_lend_into_a_call_with_a_mut_argument() {
        let src = fixture("", "")
            .replace(
                "@reader({i64, i64, i64})",
                "@reader({i64, i64, i64}, mut ptr, mem.r0)",
            )
            .replace(
                "b0(%0: {i64, i64, i64}):",
                "b0(%0: {i64, i64, i64}, %ptr: ptr, %tok: mem.r0):",
            )
            .replace(
                "%10 = call @reader(%9)",
                "%10, %end = call @reader(%9, %0, %1)",
            );
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn does_not_change_take_or_external_calls() {
        let src = fixture("", "").replace("@reader({i64", "@reader(take {i64");
        assert_eq!(count(&src), 0);
        let src = fixture("", "").replace("call @reader", "call @external");
        let src = format!("decl @external({{i64, i64, i64}}) -> i64\n{src}");
        assert_eq!(count(&src), 0);
    }
}
