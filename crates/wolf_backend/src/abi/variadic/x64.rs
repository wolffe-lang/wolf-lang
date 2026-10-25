//! Owned SysV trampoline encodings, derived from the psABI contract.
//! No general assembler: only RSP-relative moves, LEA, a rel32 branch,
//! and frame adjustment. The object emitter supplies the branch relocation.

use super::{Frame, Location, Start};

#[derive(Clone, Debug)]
pub struct Code {
    pub bytes: Vec<u8>,
    /// Offset of the rel32 displacement; the ELF relocation addend is -4.
    pub branch_offset: u32,
}

const GP: [u8; 6] = [7, 6, 2, 1, 8, 9];

fn disp(code: &mut Vec<u8>, n: u32) {
    code.extend(n.to_le_bytes());
}

// mov r64, [rsp+disp32] / mov [rsp+disp32], r64 / lea r64, [rsp+disp32].
fn rsp_op(code: &mut Vec<u8>, opcode: u8, reg: u8, offset: u32) {
    code.extend([
        0x48 | ((reg >> 3) << 2),
        opcode,
        0x84 | ((reg & 7) << 3),
        0x24,
    ]);
    disp(code, offset);
}

/// Preserve all incoming argument registers and the original overflow area.
/// Only RAX is scratched before the core call (besides the hidden context's
/// previously unused GP slot); neither return bank is touched after the call.
/// Incoming AL==0 skips every SSE instruction, including on the freestanding
/// integer-only target. Test AL before the first scratch use of RAX.
pub fn incoming(frame: &Frame, start: Start) -> Code {
    // The planner's final eight bytes are padding above the save area.
    // Use them for the caller's RBP without changing any RSP-relative offset.
    assert_eq!(
        frame.frame_bytes,
        frame.save_offset + super::SAVE_AREA_SIZE + 8
    );
    let stack_bytes = frame.frame_bytes - 8;
    let mut bytes = vec![0x55, 0x48, 0x89, 0xe5]; // push rbp; mov rbp, rsp
    bytes.extend([0x48, 0x81, 0xec]); // sub rsp, imm32
    disp(&mut bytes, stack_bytes);
    for (i, reg) in GP.iter().enumerate() {
        rsp_op(&mut bytes, 0x89, *reg, frame.save_offset + i as u32 * 8);
    }
    bytes.extend([0x84, 0xc0, 0x74, 0]); // test al, al; je past all SSE saves
    let skip_sse = bytes.len() - 1;
    let sse_start = bytes.len();
    for i in 0..8u8 {
        // movdqu [rsp+disp32], xmmN (save the complete 16-byte slot).
        bytes.extend([0xf3, 0x0f, 0x7f, 0x84 | (i << 3), 0x24]);
        disp(&mut bytes, frame.save_offset + 48 + u32::from(i) * 16);
    }
    let sse_bytes = bytes.len() - sse_start;
    assert!(
        sse_bytes <= i8::MAX as usize,
        "SSE save block fits a short branch"
    );
    bytes[skip_sse] = sse_bytes as u8;
    for off in (0..frame.named_stack_bytes).step_by(8) {
        rsp_op(&mut bytes, 0x8b, 0, frame.frame_bytes + 8 + off);
        rsp_op(&mut bytes, 0x89, 0, off);
    }
    for (off, value) in [(0, start.gp_offset), (4, start.fp_offset)] {
        bytes.extend([0xc7, 0x84, 0x24]); // mov dword [rsp+disp32], imm32
        disp(&mut bytes, frame.list_offset + off);
        disp(&mut bytes, value);
    }
    rsp_op(&mut bytes, 0x8d, 0, frame.overflow_delta);
    rsp_op(&mut bytes, 0x89, 0, frame.list_offset + 8);
    rsp_op(&mut bytes, 0x8d, 0, frame.save_offset);
    rsp_op(&mut bytes, 0x89, 0, frame.list_offset + 16);
    match &frame.context {
        Location::Registers(pieces) => {
            let index = pieces[0].save_offset / 8;
            rsp_op(&mut bytes, 0x8d, GP[index as usize], frame.list_offset);
        }
        Location::Stack { offset, .. } => {
            rsp_op(&mut bytes, 0x8d, 0, frame.list_offset);
            rsp_op(&mut bytes, 0x89, 0, *offset);
        }
        Location::Erased => unreachable!("the hidden context is not a token"),
    }
    bytes.push(0xe8); // call rel32
    let branch_offset = bytes.len() as u32;
    disp(&mut bytes, 0);
    bytes.extend([0x48, 0x81, 0xc4]); // add rsp, imm32
    disp(&mut bytes, stack_bytes);
    bytes.extend([0x5d, 0xc3]); // pop rbp; ret
    Code {
        bytes,
        branch_offset,
    }
}

/// A tail jump does not push another return address or move stack arguments.
pub fn outgoing(vector_registers: u8) -> Code {
    assert!(vector_registers <= 8);
    let mut bytes = vec![0xb8]; // mov eax, imm32
    disp(&mut bytes, 0); // planted: omit the vector-register count
    bytes.push(0xe9); // jmp rel32
    let branch_offset = bytes.len() as u32;
    disp(&mut bytes, 0);
    Code {
        bytes,
        branch_offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wolf_wir::{
        ir::{Module, Param},
        types,
    };

    #[test]
    fn tail_shims_set_the_entire_eax_and_preserve_the_stack() {
        for count in 0..=8 {
            let c = outgoing(count);
            assert_eq!(c.bytes, [0xb8, count, 0, 0, 0, 0xe9, 0, 0, 0, 0]);
            assert_eq!(c.branch_offset, 6);
        }
    }

    #[test]
    fn save_area_is_complete_before_scratch_or_named_stack_copy() {
        let mut m = Module::new();
        for fixed in [1, 6, 9] {
            let sig = m.make_sig(vec![Param::val(types::I64); fixed], vec![]);
            let frame = super::super::frame(&m.types, &m.sigs[sig]).unwrap();
            let plan = super::super::plan(&m.types, &m.sigs[sig], fixed).unwrap();
            let c = incoming(&frame, plan.start);
            assert_eq!(
                frame.frame_bytes,
                frame.save_offset + super::super::SAVE_AREA_SIZE + 8
            );
            assert_eq!(&c.bytes[..7], &[0x55, 0x48, 0x89, 0xe5, 0x48, 0x81, 0xec]);
            assert_eq!(&c.bytes[7..11], &(frame.frame_bytes - 8).to_le_bytes());
            for i in 0..6 {
                let off = 11 + i * 8;
                assert_eq!(c.bytes[off + 1], 0x89);
                assert_eq!(
                    &c.bytes[off + 4..off + 8],
                    &(frame.save_offset + i as u32 * 8).to_le_bytes()
                );
            }
            let guard = 11 + 6 * 8;
            assert_eq!(&c.bytes[guard..guard + 4], &[0x84, 0xc0, 0x74, 8 * 9]);
            for i in 0..8 {
                let off = guard + 4 + i * 9;
                assert_eq!(&c.bytes[off..off + 3], &[0xf3, 0x0f, 0x7f]);
                assert_eq!(
                    &c.bytes[off + 5..off + 9],
                    &(frame.save_offset + 48 + i as u32 * 16).to_le_bytes()
                );
            }
            assert_eq!(c.bytes[c.branch_offset as usize - 1], 0xe8);
            let epilogue = c.bytes.len() - 9;
            assert_eq!(&c.bytes[epilogue..epilogue + 3], &[0x48, 0x81, 0xc4]);
            assert_eq!(
                &c.bytes[epilogue + 3..epilogue + 7],
                &(frame.frame_bytes - 8).to_le_bytes()
            );
            assert_eq!(&c.bytes[epilogue + 7..], &[0x5d, 0xc3]);
        }
    }
}
