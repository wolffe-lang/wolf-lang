//! Producer-bounded debug CFI, including decoded return sequences.
//! Sequence-point tags provide real instruction boundaries. A failed decode
//! removes its entire return block suffix from CFI coverage, never guesses.
use std::collections::BTreeMap;

use cranelift_codegen::cursor::{Cursor, FuncCursor};
use cranelift_codegen::ir::{DebugTag, InstBuilder};
use cranelift_codegen::isa::{TargetIsa, unwind::UnwindInst};
use cranelift_codegen::{CompiledCode, Context};
use wolf_backend::dwarf::{CallFrameInstruction as Cfi, FrameDescriptionEntry, Register};
use wolf_backend::frame_decode::{self, Arch, Base, SpChange};
use wolf_backend::{BackendError, DebugSink};
use wolf_wir::ir::FuncId;

const RETURN_TAG: u32 = 0x5328_3000;

/// An immovable, zero-code marker before each CLIF return. Unlike searching
/// raw bytes for RET, this cannot mistake an immediate byte for an opcode.
pub(super) fn mark_returns(ctx: &mut Context) -> usize {
    let returns: Vec<_> = ctx
        .func
        .layout
        .blocks()
        .flat_map(|block| ctx.func.layout.block_insts(block))
        .filter(|&inst| ctx.func.dfg.insts[inst].opcode() == cranelift_codegen::ir::Opcode::Return)
        .collect();
    for &inst in &returns {
        let marker = {
            let mut cursor = FuncCursor::new(&mut ctx.func);
            cursor.goto_inst(inst);
            cursor.ins().sequence_point()
        };
        ctx.func
            .debug_tags
            .set(marker, [DebugTag::User(RETURN_TAG)]);
    }
    returns.len()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Rule {
    Offset(i64),
    Value(i64),
    Same,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    cfa: (u16, i64),
    regs: BTreeMap<u16, Rule>,
    signed_ra: bool,
}
impl State {
    fn entry(arch: Arch) -> Self {
        let ra = if arch == Arch::X64 {
            Rule::Offset(-8)
        } else {
            Rule::Same
        };
        Self {
            cfa: (arch.sp(), arch.entry_sp_delta()),
            regs: BTreeMap::from([(arch.ra(), ra)]),
            signed_ra: false,
        }
    }

    fn write(
        &self,
        previous: Option<&Self>,
        fde: &mut FrameDescriptionEntry,
        offset: u32,
    ) -> Result<(), BackendError> {
        let narrow = |n: i64| {
            i32::try_from(n).map_err(|_| {
                BackendError::Unsupported(
                    "debug frame offset beyond signed 32-bit CFI writer range".into(),
                )
            })
        };
        if previous.is_none_or(|old| old.cfa != self.cfa) {
            fde.add_instruction(offset, Cfi::Cfa(Register(self.cfa.0), narrow(self.cfa.1)?));
        }
        for (&reg, rule) in &self.regs {
            if previous.is_some_and(|old| old.regs.get(&reg) == Some(rule)) {
                continue;
            }
            let rule = match rule {
                Rule::Offset(n) => Cfi::Offset(Register(reg), narrow(*n)?),
                Rule::Value(n) => Cfi::ValOffset(Register(reg), narrow(*n)?),
                Rule::Same => Cfi::SameValue(Register(reg)),
            };
            fde.add_instruction(offset, rule);
        }
        if previous.is_some_and(|old| old.signed_ra) != self.signed_ra {
            fde.add_instruction(offset, Cfi::NegateRaState);
        }
        Ok(())
    }
}

/// The prologue's rule rows, and how far SP sits below the CFA at its end:
/// the frame-register push and every stack allocation are unwind facts.
fn prologue(
    compiled: &CompiledCode,
    isa: &dyn TargetIsa,
    arch: Arch,
) -> Result<(Vec<(u32, State)>, i64), BackendError> {
    let mut state = State::entry(arch);
    let mut rows = vec![(0, state.clone())];
    let mut frame_up = 0i64;
    let mut clobber_down = 0i64;
    let mut sp_below_cfa = arch.entry_sp_delta();
    for (offset, inst) in &compiled.buffer.unwind_info {
        match inst {
            UnwindInst::PushFrameRegs {
                offset_upward_to_caller_sp,
            } => {
                let delta = i64::from(*offset_upward_to_caller_sp);
                sp_below_cfa = delta;
                state.cfa = (arch.sp(), delta);
                state.regs.insert(arch.fp(), Rule::Offset(-delta));
                if arch == Arch::Arm64 {
                    state.regs.insert(arch.ra(), Rule::Offset(-delta + 8));
                }
            }
            UnwindInst::DefineNewFrame {
                offset_upward_to_caller_sp,
                offset_downward_to_clobbers,
            } => {
                frame_up = i64::from(*offset_upward_to_caller_sp);
                clobber_down = i64::from(*offset_downward_to_clobbers);
                state.cfa = (arch.fp(), frame_up);
            }
            UnwindInst::StackAlloc { size } => {
                sp_below_cfa += i64::from(*size);
                if state.cfa.0 == arch.sp() {
                    state.cfa.1 += i64::from(*size);
                }
            }
            UnwindInst::SaveReg {
                clobber_offset,
                reg,
            }
            | UnwindInst::RegStackOffset {
                clobber_offset,
                reg,
            } => {
                let reg = isa
                    .map_regalloc_reg_to_dwarf((*reg).into())
                    .map_err(|e| BackendError::Internal(format!("debug CFI register: {e:?}")))?;
                let offset = i64::from(*clobber_offset) - frame_up - clobber_down;
                state.regs.insert(
                    reg,
                    if matches!(inst, UnwindInst::SaveReg { .. }) {
                        Rule::Offset(offset)
                    } else {
                        Rule::Value(offset)
                    },
                );
            }
            UnwindInst::Aarch64SetPointerAuth { return_addresses } => {
                state.signed_ra = *return_addresses
            }
        }
        rows.push((*offset, state.clone()));
    }
    Ok((rows, sp_below_cfa))
}

/// Derive SP-relative CFA distances backwards from the return's ABI state,
/// then replay the proven instructions forward. MOV SP,FP deliberately makes
/// the earlier SP unknown; FP-relative rules remain usable before that point.
/// Only then is the prologue consulted: a fixed frame leaves SP `body_sp`
/// below the CFA from the prologue's end to each return sequence, so restores
/// addressed from SP before MOV SP,FP are still checked against the save facts
/// (a wrong distance fails that check, never a rule). The two derivations
/// cannot overlap: before the last SP-from-FP step only the prologue's exists,
/// and after it the forward walk restarts from the return-derived value.
fn epilogue(
    arch: Arch,
    body: &State,
    body_sp: i64,
    steps: &[frame_decode::Step],
) -> Result<Vec<(usize, State)>, &'static str> {
    let mut before = vec![None; steps.len()];
    let mut after = vec![None; steps.len()];
    let mut delta = Some(arch.entry_sp_delta());
    for (i, step) in steps.iter().enumerate().rev() {
        after[i] = delta;
        delta = match step.sp {
            SpChange::None => delta,
            SpChange::Add(n) => delta.map(|d| d + n),
            SpChange::FromFp(_) => None,
        };
        before[i] = delta;
    }
    if before.iter().any(Option::is_none) {
        let mut sp = Some(body_sp);
        for (i, step) in steps.iter().enumerate() {
            before[i] = before[i].or(sp);
            sp = match step.sp {
                SpChange::None => sp,
                SpChange::Add(n) => sp.map(|d| d - n),
                // SP now comes from FP; only the return-derived value knows it.
                SpChange::FromFp(_) => after[i],
            };
            after[i] = after[i].or(sp);
        }
    }
    let mut state = body.clone();
    let mut rows = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        if let SpChange::FromFp(offset) = step.sp
            && (state.cfa.0 != arch.fp() || after[i] != Some(state.cfa.1 - offset))
        {
            return Err("SP-from-FP teardown disagrees with the producer's CFA fact");
        }
        let mut restored_fp = false;
        for access in &step.loads {
            let Some(rule) = body.regs.get(&access.reg) else {
                if access.reg == arch.fp() || access.reg == arch.ra() {
                    return Err("restore without a saved-register fact");
                }
                continue;
            };
            let Rule::Offset(saved) = rule else {
                return Err("restore without a memory save rule");
            };
            if access.width < 8 {
                return Err("partial-width callee register restore");
            }
            let address = match access.base {
                Base::Sp => {
                    access.offset - before[i].ok_or("unknown SP before register restore")?
                }
                Base::Fp if state.cfa.0 == arch.fp() => access.offset - state.cfa.1,
                Base::Fp => return Err("frame-relative restore after FP was replaced"),
            };
            if address != *saved {
                return Err("restore address differs from the producer's save fact");
            }
            state.regs.insert(access.reg, Rule::Same);
            restored_fp |= access.reg == arch.fp();
        }
        if step.stores.iter().any(|a| body.regs.contains_key(&a.reg)) {
            return Err("callee-register save inside return sequence");
        }
        if restored_fp || (state.cfa.0 == arch.sp() && step.sp != SpChange::None) {
            state.cfa = (
                arch.sp(),
                after[i].ok_or("unknown SP after frame teardown")?,
            );
        }
        if let Some(offset) = step.fp_from_sp {
            state.cfa = (
                arch.fp(),
                after[i].ok_or("unknown SP at frame establishment")? - offset,
            );
        }
        if let Some(signed) = step.signed_ra {
            state.signed_ra = signed;
        }
        if step.ret {
            if state.cfa != (arch.sp(), arch.entry_sp_delta()) {
                return Err("return did not restore the entry CFA");
            }
            for (&reg, rule) in &body.regs {
                if reg == arch.sp() || (arch == Arch::X64 && reg == arch.ra()) {
                    continue;
                }
                if matches!(rule, Rule::Offset(_)) && state.regs.get(&reg) != Some(&Rule::Same) {
                    return Err("return did not restore every saved register");
                }
            }
        }
        rows.push((step.end, state.clone()));
    }
    Ok(rows)
}

pub(super) fn emit(
    compiled: &CompiledCode,
    isa: &dyn TargetIsa,
    id: FuncId,
    name: &str,
    return_count: usize,
    debug: &mut dyn DebugSink,
) -> Result<(), BackendError> {
    let arch = match isa.triple().architecture {
        target_lexicon::Architecture::X86_64 => Arch::X64,
        target_lexicon::Architecture::Aarch64(_) => Arch::Arm64,
        _ => {
            return Err(BackendError::Unsupported(
                "debug frame decoder for this architecture".into(),
            ));
        }
    };
    let size = compiled.buffer.total_size();
    let cie = isa
        .create_systemv_cie()
        .ok_or_else(|| BackendError::Unsupported("debug CFI for this target".into()))?;
    let (mut events, body_sp) = prologue(compiled, isa, arch)?;
    let body = events.last().expect("entry row").1.clone();
    let mut markers: Vec<u32> = compiled
        .buffer
        .debug_tags()
        .filter(|list| list.tags.contains(&DebugTag::User(RETURN_TAG)))
        .map(|list| list.offset)
        .collect();
    markers.sort_unstable();
    markers.dedup();
    if return_count != 0 && markers.is_empty() {
        eprintln!("CFI_GAP {name} 0..{size}: missing producer return markers");
        return Ok(());
    }
    let mut blocks = compiled.bb_starts.clone();
    blocks.sort_unstable();
    blocks.dedup();
    let mut gaps = Vec::new();
    for start in markers {
        let end = blocks
            .iter()
            .copied()
            .find(|&offset| offset > start)
            .unwrap_or(size);
        let decoded = compiled
            .buffer
            .data()
            .get(start as usize..end as usize)
            .ok_or("return marker outside code")
            .and_then(|bytes| {
                frame_decode::teardown(arch, bytes)
                    .map_err(|_| "unknown return-sequence instruction")
            });
        let decoded = decoded.and_then(|steps| epilogue(arch, &body, body_sp, &steps));
        match decoded {
            Ok(rows) => {
                let ret_end = rows
                    .last()
                    .map(|(off, _)| start + *off as u32)
                    .unwrap_or(start);
                for (off, row) in rows {
                    events.push((start + off as u32, row));
                }
                if ret_end < size {
                    events.push((ret_end, body.clone()));
                }
            }
            Err(reason) => {
                eprintln!("CFI_GAP {name} {start}..{end}: {reason}");
                gaps.push((start, end));
                events.push((end, body.clone()));
            }
        }
    }
    events.sort_by_key(|(offset, _)| *offset);
    let mut ranges = Vec::new();
    let mut at = 0;
    for (start, end) in gaps {
        if start > at {
            ranges.push((at, start));
        }
        at = at.max(end);
    }
    if at < size {
        ranges.push((at, size));
    }
    for (start, end) in ranges {
        let mut fde = FrameDescriptionEntry::new(
            wolf_backend::dwarf::function_address_at(id, start),
            end - start,
        );
        let initial = events
            .iter()
            .rev()
            .find(|(offset, _)| *offset <= start)
            .expect("entry state")
            .1
            .clone();
        initial.write(None, &mut fde, 0)?;
        let mut previous = initial;
        for (offset, state) in &events {
            if *offset <= start || *offset >= end {
                continue;
            }
            state.write(Some(&previous), &mut fde, *offset - start)?;
            previous = state.clone();
        }
        debug.frame_info(cie.clone(), fde);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_rules_change_after_the_instruction_not_before() {
        let body = State {
            cfa: (6, 16),
            regs: BTreeMap::from([(6, Rule::Offset(-16)), (16, Rule::Offset(-8))]),
            signed_ra: false,
        };
        let steps = frame_decode::teardown(Arch::X64, &[0x48, 0x83, 0xc4, 32, 0x5d, 0xc3]).unwrap();
        let rows = epilogue(Arch::X64, &body, 16, &steps).unwrap();
        assert_eq!(rows[0].1.cfa, (6, 16));
        assert_eq!(rows[1].1.cfa, (7, 8));
        assert_eq!(rows[1].1.regs[&6], Rule::Same);
    }
    #[test]
    fn sp_from_fp_must_agree_with_the_return_derived_distance() {
        let body = State {
            cfa: (6, 32),
            regs: BTreeMap::from([(6, Rule::Offset(-16)), (16, Rule::Offset(-8))]),
            signed_ra: false,
        };
        let steps = frame_decode::teardown(Arch::X64, &[0x48, 0x89, 0xec, 0x5d, 0xc3]).unwrap();
        assert!(epilogue(Arch::X64, &body, 16, &steps).is_err());
    }

    // The pressure chain's x86-64 return sequence: SP-relative callee-saved
    // restores, the fixed frame's release, MOV RSP,RBP, POP RBP and RET.
    const PRESSURE_RETURN: &[u8] = &[
        0x48, 0x8b, 0x5c, 0x24, 0x20, // mov rbx, [rsp+0x20]
        0x4c, 0x8b, 0x64, 0x24, 0x28, // mov r12, [rsp+0x28]
        0x4c, 0x8b, 0x6c, 0x24, 0x30, // mov r13, [rsp+0x30]
        0x48, 0x83, 0xc4, 0x40, // add rsp, 0x40
        0x48, 0x89, 0xec, // mov rsp, rbp
        0x5d, // pop rbp
        0xc3, // ret
    ];

    /// Its body rule: push rbp, mov rbp,rsp, sub rsp,0x40 put SP 80 below the
    /// CFA, so the saves at [rsp+0x20..0x30] are CFA-48, -40 and -32.
    fn pressure_body() -> State {
        State {
            cfa: (6, 16),
            regs: BTreeMap::from([
                (3, Rule::Offset(-48)),
                (6, Rule::Offset(-16)),
                (12, Rule::Offset(-40)),
                (13, Rule::Offset(-32)),
                (16, Rule::Offset(-8)),
            ]),
            signed_ra: false,
        }
    }

    #[test]
    fn sp_relative_restores_before_mov_rsp_rbp_use_the_fixed_frame() {
        let steps = frame_decode::teardown(Arch::X64, PRESSURE_RETURN).unwrap();
        let rows = epilogue(Arch::X64, &pressure_body(), 80, &steps).unwrap();
        assert_eq!(rows.len(), 7);
        // Each restore takes effect after its own instruction.
        assert_eq!(rows[0].1.regs[&3], Rule::Same);
        assert_eq!(rows[0].1.regs[&12], Rule::Offset(-40));
        assert_eq!(rows[2].1.regs[&13], Rule::Same);
        // The CFA stays frame-based through MOV RSP,RBP, then POP RBP
        // returns it to the entry rule with RBP restored.
        assert_eq!(rows[4].1.cfa, (6, 16));
        assert_eq!(rows[5].1.cfa, (7, 8));
        assert_eq!(rows[5].1.regs[&6], Rule::Same);
    }

    #[test]
    fn a_wrong_fixed_frame_leaves_the_return_uncovered() {
        let steps = frame_decode::teardown(Arch::X64, PRESSURE_RETURN).unwrap();
        for wrong in [72, 88] {
            assert_eq!(
                epilogue(Arch::X64, &pressure_body(), wrong, &steps),
                Err("restore address differs from the producer's save fact")
            );
        }
    }

    #[test]
    fn complete_return_derived_distances_never_consult_the_prologue() {
        let body = State {
            cfa: (6, 16),
            regs: BTreeMap::from([(6, Rule::Offset(-16)), (16, Rule::Offset(-8))]),
            signed_ra: false,
        };
        let steps = frame_decode::teardown(Arch::X64, &[0x48, 0x83, 0xc4, 32, 0x5d, 0xc3]).unwrap();
        assert!(epilogue(Arch::X64, &body, 16, &steps).is_ok());
        assert_eq!(
            epilogue(Arch::X64, &body, 16, &steps),
            epilogue(Arch::X64, &body, 999, &steps)
        );
    }

    #[test]
    fn mismatched_save_locations_leave_the_range_uncovered() {
        let body = State {
            cfa: (6, 16),
            regs: BTreeMap::from([(6, Rule::Offset(-24)), (16, Rule::Offset(-8))]),
            signed_ra: false,
        };
        let steps = frame_decode::teardown(Arch::X64, &[0x5d, 0xc3]).unwrap();
        assert!(epilogue(Arch::X64, &body, 16, &steps).is_err());
    }
}
