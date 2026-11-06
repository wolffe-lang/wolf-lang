//! Minimal owned decoder for frame instructions, not a general disassembler.
//! Sources: Intel SDM vol. 2 (325383-086US), ADD/MOV/POP/PUSH/LEAVE/RET;
//! Arm ARM DDI0487B.a C6.2 ADD/SUB/LDP/STP/RET, and Arm A64 2026-03
//! HINT/PACIA/PACIB/AUTIA/AUTIB instruction descriptions.
//!
//! Decode only at a producer-supplied instruction boundary. In particular, do
//! not scan for 0xc3: that byte can occur inside an x86 immediate. Return-marker
//! ranges come from immovable CLIF sequence points. Unknown forms fail closed.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub fn sp(self) -> u16 {
        match self {
            Self::X64 => 7,
            Self::Arm64 => 31,
        }
    }
    pub fn fp(self) -> u16 {
        match self {
            Self::X64 => 6,
            Self::Arm64 => 29,
        }
    }
    pub fn ra(self) -> u16 {
        match self {
            Self::X64 => 16,
            Self::Arm64 => 30,
        }
    }
    pub fn entry_sp_delta(self) -> i64 {
        match self {
            Self::X64 => 8,
            Self::Arm64 => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    Sp,
    Fp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpChange {
    None,
    Add(i64),
    FromFp(i64),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    pub reg: u16,
    pub base: Base,
    pub offset: i64,
    pub width: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub end: usize,
    pub sp: SpChange,
    pub fp_from_sp: Option<i64>,
    pub loads: Vec<Access>,
    pub stores: Vec<Access>,
    pub signed_ra: Option<bool>,
    pub ret: bool,
}

impl Step {
    fn plain(end: usize) -> Self {
        Self {
            end,
            sp: SpChange::None,
            fp_from_sp: None,
            loads: Vec::new(),
            stores: Vec::new(),
            signed_ra: None,
            ret: false,
        }
    }
}

/// Decode from a known boundary through the first actual return. The caller
/// restricts `bytes` to the producer's basic block, never the next function.
pub fn teardown(arch: Arch, bytes: &[u8]) -> Result<Vec<Step>, usize> {
    let mut steps = Vec::new();
    let mut at = 0;
    let mut consts = Consts::default();
    while at < bytes.len() {
        let step = next(arch, bytes, at, &mut consts)?;
        at = step.end;
        let ret = step.ret;
        steps.push(step);
        if ret {
            return Ok(steps);
        }
    }
    Err(at)
}

/// Decode every instruction of a producer-bounded range that ends exactly at
/// an instruction boundary, such as a prologue. A return inside it fails.
pub fn instructions(arch: Arch, bytes: &[u8]) -> Result<Vec<Step>, usize> {
    let mut steps = Vec::new();
    let mut at = 0;
    let mut consts = Consts::default();
    while at < bytes.len() {
        let step = next(arch, bytes, at, &mut consts)?;
        if step.ret || step.end > bytes.len() {
            return Err(at);
        }
        at = step.end;
        steps.push(step);
    }
    Ok(steps)
}

/// The values MOV wide immediates gave arm64 scratch registers within one
/// decoded range. Nothing is assumed about a register's value on entry, and
/// any other write to it forgets the value.
#[derive(Default)]
struct Consts([Option<u64>; 31]);

impl Consts {
    fn get(&self, reg: u16) -> Option<u64> {
        self.0.get(usize::from(reg)).copied().flatten()
    }
    fn set(&mut self, reg: u16, value: Option<u64>) {
        if let Some(slot) = self.0.get_mut(usize::from(reg)) {
            *slot = value;
        }
    }
}

/// The instruction at `at`, with `end` as an offset into `bytes`.
fn next(arch: Arch, bytes: &[u8], at: usize, consts: &mut Consts) -> Result<Step, usize> {
    let mut step = match arch {
        Arch::X64 => x64(&bytes[at..]),
        Arch::Arm64 => arm64(&bytes[at..], consts),
    }
    .ok_or(at)?;
    step.end += at;
    Ok(step)
}

fn gp(x: u8) -> u16 {
    [0, 2, 1, 3, 7, 6, 4, 5, 8, 9, 10, 11, 12, 13, 14, 15][x as usize]
}
fn volatile_x64(x: u8) -> bool {
    matches!(x, 0..=2 | 6..=11)
}
fn i32_at(b: &[u8], i: usize) -> Option<i64> {
    Some(i32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as i64)
}

fn x64(b: &[u8]) -> Option<Step> {
    let mut at = 0;
    let rex = if matches!(b.first()?, 0x40..=0x4f) {
        at = 1;
        b[0]
    } else {
        0
    };
    let op = *b.get(at)?;
    at += 1;
    let mut s = Step::plain(at);
    match op {
        0xc3 if rex == 0 => s.ret = true,
        0x90 if rex == 0 => {}
        0xc9 if rex == 0 => {
            s.sp = SpChange::FromFp(8);
            s.loads.push(Access {
                reg: 6,
                base: Base::Fp,
                offset: 0,
                width: 8,
            });
        }
        0x50..=0x5f if rex & 0x6 == 0 => {
            let reg = (op & 7) | ((rex & 1) << 3);
            if reg == 4 {
                return None;
            }
            let pop = op >= 0x58;
            s.sp = SpChange::Add(if pop { 8 } else { -8 });
            let access = Access {
                reg: gp(reg),
                base: Base::Sp,
                offset: if pop { 0 } else { -8 },
                width: 8,
            };
            if pop {
                s.loads.push(access);
            } else {
                s.stores.push(access);
            }
        }
        0x81 | 0x83 if rex == 0x48 => {
            let mode = *b.get(at)?;
            at += 1;
            if !matches!(mode, 0xc4 | 0xec) {
                return None;
            }
            let value = if op == 0x83 {
                let n = *b.get(at)? as i8 as i64;
                at += 1;
                n
            } else {
                let n = i32_at(b, at)?;
                at += 4;
                n
            };
            s.sp = SpChange::Add(if mode == 0xc4 { value } else { -value });
        }
        0xb8..=0xbf => {
            let dest = (op & 7) | ((rex & 1) << 3);
            if rex & 6 != 0 || !volatile_x64(dest) {
                return None;
            }
            at += if rex & 8 != 0 { 8 } else { 4 };
            b.get(..at)?;
        }
        0xc7 => {
            let mode = *b.get(at)?;
            at += 1;
            if mode & 0xf8 != 0xc0 || rex & 6 != 0 {
                return None;
            }
            if !volatile_x64((mode & 7) | ((rex & 1) << 3)) {
                return None;
            }
            at += 4;
            b.get(..at)?;
        }
        0x89 | 0x8b | 0x31 | 0x33 => {
            let mode = *b.get(at)?;
            at += 1;
            let reg = ((mode >> 3) & 7) | ((rex & 4) << 1);
            let rm = (mode & 7) | ((rex & 1) << 3);
            if mode >> 6 == 3 {
                let (dest, src) = if matches!(op, 0x89 | 0x31) {
                    (rm, reg)
                } else {
                    (reg, rm)
                };
                if matches!(op, 0x89 | 0x8b) && rex & 8 != 0 && (dest, src) == (4, 5) {
                    s.sp = SpChange::FromFp(0);
                } else if matches!(op, 0x89 | 0x8b) && rex & 8 != 0 && (dest, src) == (5, 4) {
                    s.fp_from_sp = Some(0);
                } else if !volatile_x64(dest)
                    && (dest != src || rex & 8 == 0 || matches!(op, 0x31 | 0x33))
                {
                    return None;
                }
            } else {
                if !matches!(op, 0x89 | 0x8b) || rex & 8 == 0 || rex & 2 != 0 {
                    return None;
                }
                let base = match rm {
                    4 => {
                        if *b.get(at)? != 0x24 {
                            return None;
                        }
                        at += 1;
                        Base::Sp
                    }
                    5 if mode >> 6 != 0 => Base::Fp,
                    _ => return None,
                };
                let offset = match mode >> 6 {
                    0 => 0,
                    1 => {
                        let n = *b.get(at)? as i8 as i64;
                        at += 1;
                        n
                    }
                    2 => {
                        let n = i32_at(b, at)?;
                        at += 4;
                        n
                    }
                    _ => return None,
                };
                if reg == 4 {
                    return None;
                }
                let access = Access {
                    reg: gp(reg),
                    base,
                    offset,
                    width: 8,
                };
                if op == 0x8b {
                    s.loads.push(access);
                } else {
                    s.stores.push(access);
                }
            }
        }
        _ => return None,
    }
    s.end = at;
    Some(s)
}

fn arm64(b: &[u8], consts: &mut Consts) -> Option<Step> {
    let w = u32::from_le_bytes(b.get(..4)?.try_into().ok()?);
    let mut s = Step::plain(4);
    match w {
        0xd65f03c0 => {
            s.ret = true;
            return Some(s);
        } // RET X30
        0xd503201f => return Some(s), // NOP
        0xd503233f | 0xd503237f => {
            s.signed_ra = Some(true);
            return Some(s);
        }
        0xd50323bf | 0xd50323ff => {
            s.signed_ra = Some(false);
            return Some(s);
        }
        _ => {}
    }
    let rn = ((w >> 5) & 31) as u16;
    let rt = (w & 31) as u16;
    // ADD/SUB immediate, without flags. The shift field is 0 or 12.
    if w & 0x3f800000 == 0x11000000 {
        let shift = (w >> 22) & 3;
        if shift > 1 {
            return None;
        }
        let mut imm = (((w >> 10) & 0xfff) as i64) << (shift * 12);
        if w & 0x40000000 != 0 {
            imm = -imm;
        }
        if rt == 31 {
            if w >> 31 == 0 {
                return None;
            }
            s.sp = match rn {
                31 => SpChange::Add(imm),
                29 => SpChange::FromFp(imm),
                _ => return None,
            };
        } else if rt == 29 && rn == 31 && w >> 31 != 0 {
            s.fp_from_sp = Some(imm);
        } else if rt >= 19 {
            return None;
        } else {
            consts.set(rt, None);
        }
        return Some(s);
    }
    // ADD/SUB (extended register), 64-bit, no flags, UXTX with no shift, SP to
    // SP: a frame too large for an immediate, released or allocated through a
    // scratch register this range itself set. Its value must be known.
    if w & 0xbfe0fc00 == 0x8b206000 {
        let rm = ((w >> 16) & 31) as u16;
        if rt != 31 || rn != 31 {
            return None;
        }
        let value = i64::try_from(consts.get(rm)?).ok()?;
        s.sp = SpChange::Add(if w & 0x40000000 != 0 { -value } else { value });
        return Some(s);
    }
    // ORR (shifted register) MOV alias: a return-register copy.
    if w & 0x7fe0ffe0 == 0x2a0003e0 {
        if rt >= 19 && rt != 31 {
            return None;
        }
        consts.set(rt, None);
        return Some(s);
    }
    // MOVN/MOVZ/MOVK immediate materialization into a volatile register. The
    // value is kept: an SP adjustment above may read it.
    if w & 0x1f800000 == 0x12800000 {
        let opc = (w >> 29) & 3;
        let wide = w >> 31 != 0;
        let shift = ((w >> 21) & 3) * 16;
        if opc == 1 || (rt >= 19 && rt != 31) || (!wide && shift > 16) {
            return None;
        }
        let imm = u64::from((w >> 5) & 0xffff) << shift;
        let mask = if wide { u64::MAX } else { 0xffff_ffff };
        let value = match opc {
            0 => Some(!imm & mask),
            2 => Some(imm),
            _ => consts
                .get(rt)
                .map(|old| ((old & !(0xffff << shift)) | imm) & mask),
        };
        consts.set(rt, value);
        return Some(s);
    }
    let base = match rn {
        31 => Base::Sp,
        29 => Base::Fp,
        _ => return None,
    };
    if w & 0x3a000000 == 0x28000000 {
        let vector = w & (1 << 26) != 0;
        let opc = w >> 30;
        let width = match (vector, opc) {
            (false, 0) => 4,
            (false, 2) => 8,
            (true, 0) => 4,
            (true, 1) => 8,
            (true, 2) => 16,
            _ => return None,
        };
        let mode = (w >> 23) & 3;
        if mode == 0 || (mode != 2 && base != Base::Sp) {
            return None;
        }
        let imm = (((w >> 15) & 0x7f) as i64 - if w & (1 << 21) != 0 { 128 } else { 0 }) * width;
        let second = ((w >> 10) & 31) as u16;
        let load = w & (1 << 22) != 0;
        if load && rt == second {
            return None;
        }
        if mode != 2 {
            s.sp = SpChange::Add(imm);
        }
        let offset = if mode == 1 { 0 } else { imm };
        for (reg, off) in [(rt, offset), (second, offset + width)] {
            if !vector && reg == 31 {
                continue;
            }
            let access = Access {
                reg: reg + if vector { 64 } else { 0 },
                base,
                offset: off,
                width: width as u8,
            };
            if load {
                consts.set(access.reg, None);
                s.loads.push(access);
            } else {
                s.stores.push(access);
            }
        }
        return Some(s);
    }
    // 64-bit GPR or D-register stack loads/stores, unsigned scaled offset.
    let top = w & 0xffc00000;
    if matches!(top, 0xf9400000 | 0xf9000000 | 0xfd400000 | 0xfd000000) {
        let access = Access {
            reg: rt + if w & (1 << 26) != 0 { 64 } else { 0 },
            base,
            offset: ((w >> 10) & 0xfff) as i64 * 8,
            width: 8,
        };
        if w & (1 << 22) != 0 {
            consts.set(access.reg, None);
            s.loads.push(access);
        } else {
            s.stores.push(access);
        }
        return Some(s);
    }
    // Same widths, signed unscaled or pre/post-indexed offsets.
    let top = w & 0xffe00000;
    if matches!(top, 0xf8400000 | 0xf8000000 | 0xfc400000 | 0xfc000000) {
        let mode = (w >> 10) & 3;
        if mode == 2 || (mode != 0 && base != Base::Sp) {
            return None;
        }
        let imm = ((w >> 12) & 0x1ff) as i64 - if w & (1 << 20) != 0 { 512 } else { 0 };
        if mode != 0 {
            s.sp = SpChange::Add(imm);
        }
        let access = Access {
            reg: rt + if w & (1 << 26) != 0 { 64 } else { 0 },
            base,
            offset: if mode == 1 { 0 } else { imm },
            width: 8,
        };
        if w & (1 << 22) != 0 {
            consts.set(access.reg, None);
            s.loads.push(access);
        } else {
            s.stores.push(access);
        }
        return Some(s);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_whole_prologue_decodes_to_its_frame_steps() {
        // push rbp; mov rbp,rsp; sub rsp,0x40; mov [rsp+0x20],rbx; mov [rsp+0x28],r12
        let bytes = [
            0x55, 0x48, 0x89, 0xe5, 0x48, 0x83, 0xec, 0x40, 0x48, 0x89, 0x5c, 0x24, 0x20, 0x4c,
            0x89, 0x64, 0x24, 0x28,
        ];
        let steps = instructions(Arch::X64, &bytes).unwrap();
        assert_eq!(
            steps.iter().map(|s| s.end).collect::<Vec<_>>(),
            [1, 4, 8, 13, 18]
        );
        assert_eq!(steps[0].sp, SpChange::Add(-8));
        assert_eq!(steps[1].fp_from_sp, Some(0));
        assert_eq!(steps[2].sp, SpChange::Add(-0x40));
        assert_eq!(
            (steps[4].stores[0].reg, steps[4].stores[0].offset),
            (12, 0x28)
        );
        // A truncated last instruction or a return inside the range fails.
        assert_eq!(instructions(Arch::X64, &bytes[..17]), Err(13));
        assert_eq!(instructions(Arch::X64, &[0x55, 0xc3]), Err(1));
    }
    #[test]
    fn x64_immediate_ret_byte_is_not_a_return() {
        let steps = teardown(
            Arch::X64,
            &[0xb8, 0xc3, 0, 0, 0, 0x48, 0x83, 0xc4, 0x20, 0x5d, 0xc3],
        )
        .unwrap();
        assert_eq!(
            steps.iter().map(|s| s.end).collect::<Vec<_>>(),
            [5, 9, 10, 11]
        );
        assert_eq!(steps[1].sp, SpChange::Add(32));
        assert_eq!(steps[2].loads[0].reg, 6);
        assert!(!steps[0].ret);
    }
    #[test]
    fn x64_stack_and_extended_register_restores() {
        let steps = teardown(
            Arch::X64,
            &[0x4c, 0x8b, 0x64, 0x24, 0x10, 0x41, 0x5d, 0xc9, 0xc3],
        )
        .unwrap();
        assert_eq!(
            steps[0].loads[0],
            Access {
                reg: 12,
                base: Base::Sp,
                offset: 16,
                width: 8
            }
        );
        assert_eq!(steps[1].loads[0].reg, 13);
        assert_eq!(steps[2].sp, SpChange::FromFp(8));
    }
    #[test]
    fn arm64_restore_pair_and_authenticate() {
        let bytes: Vec<_> = [0x910083ffu32, 0xa8c17bfd, 0xd50323bf, 0xd65f03c0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let steps = teardown(Arch::Arm64, &bytes).unwrap();
        assert_eq!(steps[0].sp, SpChange::Add(32));
        assert_eq!(steps[1].sp, SpChange::Add(16));
        assert_eq!(
            steps[1].loads.iter().map(|a| a.reg).collect::<Vec<_>>(),
            [29, 30]
        );
        assert_eq!(steps[2].signed_ra, Some(false));
    }
    /// A frame larger than an ADD immediate reaches: the return sequence
    /// rebuilds the size in a scratch register and releases it from SP, then
    /// pops the saved pairs (an arm64 `main` of 24,896 frame bytes).
    #[test]
    fn arm64_large_frame_release_through_a_scratch_register() {
        let bytes: Vec<_> = [
            0xd28c2810u32, // mov x16, #0x6140
            0x8b3063ff,    // add sp, sp, x16
            0xa8c153f3,    // ldp x19, x20, [sp], #16
            0xa8c15bf5,
            0xa8c163f7,
            0xa8c16bf9,
            0xa8c173fb,
            0xa8c17bfd, // ldp x29, x30, [sp], #16
            0xd65f03c0, // ret
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
        let steps = teardown(Arch::Arm64, &bytes).unwrap();
        assert_eq!(steps.len(), 9);
        assert_eq!(steps[0].sp, SpChange::None);
        assert_eq!(steps[1].sp, SpChange::Add(0x6140));
        assert!(steps[2..8].iter().all(|s| s.sp == SpChange::Add(16)));
        assert_eq!(
            steps[2].loads.iter().map(|a| a.reg).collect::<Vec<_>>(),
            [19, 20]
        );
        assert!(steps[8].ret);
    }
    #[test]
    fn arm64_scratch_values_come_only_from_the_range_itself() {
        let words = |w: &[u32]| -> Vec<u8> { w.iter().flat_map(|w| w.to_le_bytes()).collect() };
        // add sp, sp, x16 with x16 never set here: no claim.
        assert_eq!(
            teardown(Arch::Arm64, &words(&[0x8b3063ff, 0xd65f03c0])),
            Err(0)
        );
        // mov x16, #0x6140; ldr x16, [sp, #8]; add sp, sp, x16: the load forgets it.
        assert_eq!(
            teardown(
                Arch::Arm64,
                &words(&[0xd28c2810, 0xf94007f0, 0x8b3063ff, 0xd65f03c0])
            ),
            Err(8)
        );
        // mov x16, #0x2345; movk x16, #1, lsl #16; sub sp, sp, x16 (a prologue).
        let steps =
            instructions(Arch::Arm64, &words(&[0xd28468b0, 0xf2a00030, 0xcb3063ff])).unwrap();
        assert_eq!(steps[2].sp, SpChange::Add(-0x12345));
        // Only SP to SP: add x0, sp, x16 is not a frame adjustment.
        assert!(instructions(Arch::Arm64, &words(&[0xd28c2810, 0x8b3063e0])).is_err());
    }
    #[test]
    fn unknown_or_truncated_forms_never_claim_coverage() {
        for bytes in [
            &[0x48, 0x83, 0xc4][..],
            &[0x48, 0x89, 0xc4, 0xc3],
            &[0x67, 0x5d, 0xc3],
        ] {
            assert!(teardown(Arch::X64, bytes).is_err());
        }
        assert!(teardown(Arch::Arm64, &[0, 0, 0, 0]).is_err());
        assert!(teardown(Arch::Arm64, &[0xc0, 3, 0x5f]).is_err());
    }
}
