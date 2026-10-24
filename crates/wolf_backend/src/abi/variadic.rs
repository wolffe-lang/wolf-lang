//! SysV AMD64 variable-argument bookkeeping, shared by the emitters.
//!
//! Written from psABI 1.0 §3.2.3 and §3.5.7, figures 3.33–3.34.
//! These are plans, not a second calling convention: classification
//! comes from the ordinary C plan, including aggregate rollback.

use super::{CTarget, Conv, ParamPass, RegClass, RetPass, Unit, plan_sig};
use crate::layout;
use wolf_wir::ir::SigData;
use wolf_wir::types::TypeInterner;

pub const GP_LIMIT: u32 = 48;
pub const FP_BASE: u32 = 48;
pub const FP_LIMIT: u32 = 176;
pub const SAVE_AREA_SIZE: u32 = 176;
pub const SAVE_AREA_ALIGN: u32 = 16;
pub const VA_LIST_SIZE: u32 = 24;
pub const VA_LIST_ALIGN: u32 = 8;

/// One piece of an argument held in an incoming register. `save_offset`
/// addresses its corresponding slot in a variadic callee's save area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterPiece {
    pub value_offset: u32,
    pub save_offset: u32,
    pub bytes: u8,
    pub class: RegClass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// Tokens erase and consume neither registers nor stack bytes.
    Erased,
    Registers(Vec<RegisterPiece>),
    /// Relative to the first incoming stack argument, not the return
    /// address. At callee entry that address is `%rsp + 8`.
    Stack {
        offset: u32,
        size: u32,
        align: u32,
    },
}

/// Initial `va_start` cursor. The two pointer fields are supplied by the
/// invocation's prologue; `overflow_offset` advances past NAMED stack
/// arguments only, never past the entire call's stack arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start {
    pub gp_offset: u32,
    pub fp_offset: u32,
    pub overflow_offset: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub params: Vec<Location>,
    /// `%al`, counting named AND unnamed SSE argument registers.
    pub vector_registers: u8,
    /// Actual argument bytes, before the caller's 16-byte frame padding.
    pub stack_bytes: u32,
    pub aligned_stack_bytes: u32,
    pub start: Start,
}

fn align_up(n: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (n + align - 1) & !(align - 1)
}

/// Plan a complete call after default promotions. `fixed` counts named
/// non-token parameters. Passing an unpromoted tail is a lowerer error:
/// signedness and C integer promotions belong above WIR, which erases
/// the distinction between i8 and u8.
pub fn plan(types: &TypeInterner, sig: &SigData, fixed: usize) -> Result<Call, String> {
    let abi = plan_sig(types, sig, Conv::C, CTarget::SysvX64);
    let count = sig.params.iter().filter(|p| !types.is_token(p.ty)).count();
    if fixed > count {
        return Err(format!(
            "variadic fixed prefix {fixed} exceeds {count} arguments"
        ));
    }
    let mut gp = abi
        .rets
        .iter()
        .filter(|r| matches!(r, RetPass::Sret { .. }))
        .count() as u32;
    let mut fp = 0u32;
    let mut stack = 0u32;
    let mut params = Vec::with_capacity(sig.params.len());
    let mut seen = 0;
    let mut start = Start {
        gp_offset: gp * 8,
        fp_offset: FP_BASE,
        overflow_offset: 0,
    };
    for (param, pass) in sig.params.iter().zip(&abi.params) {
        if matches!(pass, ParamPass::Token) {
            params.push(Location::Erased);
            continue;
        }
        let layout = layout::layout_of(types, param.ty)
            .ok_or_else(|| "a variadic argument has no concrete layout".to_string())?;
        let units = match pass {
            ParamPass::Split(units) => Some(units.clone()),
            ParamPass::Direct(ty) => {
                let class = super::scalar_class(types, *ty).ok_or_else(|| {
                    "a direct variadic argument has no register class".to_string()
                })?;
                Some(vec![Unit {
                    offset: 0,
                    class,
                    bytes: layout.size as u8,
                }])
            }
            ParamPass::Memory => None,
            ParamPass::Token => unreachable!("erased above"),
        };
        let needed_gp = units.as_ref().map_or(0, |us| {
            us.iter().filter(|u| u.class == RegClass::Int).count() as u32
        });
        let needed_fp = units.as_ref().map_or(0, |us| {
            us.iter().filter(|u| u.class == RegClass::Sse).count() as u32
        });
        if let Some(units) = units.filter(|_| gp + needed_gp <= 6 && fp + needed_fp <= 8) {
            let mut pieces = Vec::with_capacity(units.len());
            for unit in units {
                let save_offset = match unit.class {
                    RegClass::Int => {
                        let offset = gp * 8;
                        gp += 1;
                        offset
                    }
                    RegClass::Sse => {
                        let offset = FP_BASE + fp * 16;
                        fp += 1;
                        offset
                    }
                };
                pieces.push(RegisterPiece {
                    value_offset: unit.offset,
                    save_offset,
                    bytes: unit.bytes,
                    class: unit.class,
                });
            }
            params.push(Location::Registers(pieces));
        } else {
            let align = layout.align.max(8);
            stack = align_up(stack, align);
            params.push(Location::Stack {
                offset: stack,
                size: layout.size,
                align,
            });
            stack += align_up(layout.size, 8);
        }
        seen += 1;
        if seen == fixed {
            start = Start {
                gp_offset: gp * 8,
                fp_offset: FP_BASE + fp * 16,
                overflow_offset: stack,
            };
        }
    }
    Ok(Call {
        params,
        vector_registers: fp as u8,
        stack_bytes: stack,
        aligned_stack_bytes: align_up(stack, 16),
        start,
    })
}

/// Private native trampoline frame. At entry the return address is at
/// old RSP and named stack arguments at old RSP+8. After subtracting
/// `frame_bytes`, RSP is 16-byte aligned for the fixed-core call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub frame_bytes: u32,
    pub named_stack_bytes: u32,
    pub list_offset: u32,
    pub save_offset: u32,
    pub overflow_delta: u32,
    pub context: Location,
}

pub fn frame(types: &TypeInterner, fixed_sig: &SigData) -> Result<Frame, String> {
    let fixed = fixed_sig
        .params
        .iter()
        .filter(|p| !types.is_token(p.ty))
        .count();
    let call = plan(types, fixed_sig, fixed)?;
    let gp = call.start.gp_offset / 8;
    let (context, outgoing_bytes) = if gp < 6 {
        (
            Location::Registers(vec![RegisterPiece {
                value_offset: 0,
                save_offset: gp * 8,
                bytes: 8,
                class: RegClass::Int,
            }]),
            call.stack_bytes,
        )
    } else {
        (
            Location::Stack {
                offset: call.stack_bytes,
                size: 8,
                align: 8,
            },
            call.stack_bytes + 8,
        )
    };
    let list_offset = align_up(outgoing_bytes, VA_LIST_ALIGN);
    let save_offset = align_up(list_offset + VA_LIST_SIZE, SAVE_AREA_ALIGN);
    let frame_bytes = align_up(save_offset + SAVE_AREA_SIZE + 8, 16) - 8;
    Ok(Frame {
        frame_bytes,
        named_stack_bytes: call.stack_bytes,
        list_offset,
        save_offset,
        overflow_delta: frame_bytes + 8 + call.stack_bytes,
        context,
    })
}

/// §3.5.7 step 3, with subtraction kept in-range for arbitrary shapes.
/// An aggregate must fit BOTH banks or it comes wholly from overflow.
pub fn fits_registers(gp_offset: u32, fp_offset: u32, gp_slots: u32, fp_slots: u32) -> bool {
    gp_offset <= GP_LIMIT
        && (FP_BASE..=FP_LIMIT).contains(&fp_offset)
        && gp_offset.is_multiple_of(8)
        && (fp_offset - FP_BASE).is_multiple_of(16)
        && gp_slots <= (GP_LIMIT - gp_offset) / 8
        && fp_slots <= (FP_LIMIT - fp_offset) / 16
}

#[cfg(test)]
mod tests {
    use super::*;
    use wolf_wir::ir::{Module, Param};
    use wolf_wir::types::{self, TypeData};

    #[test]
    fn trampoline_frame_keeps_context_save_area_and_return_address_disjoint() {
        for fixed in 1..20 {
            let mut m = Module::new();
            let sig = m.make_sig(vec![Param::val(types::I64); fixed], vec![types::I64]);
            let f = frame(&m.types, &m.sigs[sig]).unwrap();
            assert_eq!(f.frame_bytes % 16, 8);
            assert_eq!(f.save_offset % 16, 0);
            assert!(f.list_offset >= f.named_stack_bytes);
            assert!(f.save_offset >= f.list_offset + VA_LIST_SIZE);
            assert!(f.save_offset + SAVE_AREA_SIZE <= f.frame_bytes);
            assert_eq!(f.overflow_delta, f.frame_bytes + 8 + f.named_stack_bytes);
            if fixed >= 6 {
                assert!(
                    matches!(f.context, Location::Stack { offset, .. } if offset == f.named_stack_bytes)
                );
                assert!(f.list_offset >= f.named_stack_bytes + 8);
            }
        }
    }

    #[test]
    fn offsets_match_the_psabi_record_and_full_save_area() {
        assert_eq!((VA_LIST_SIZE, VA_LIST_ALIGN), (24, 8));
        assert_eq!((SAVE_AREA_SIZE, SAVE_AREA_ALIGN), (176, 16));
        assert_eq!(GP_LIMIT, FP_BASE);
        assert_eq!(FP_LIMIT, FP_BASE + 8 * 16);
        assert!(fits_registers(40, 160, 1, 1));
        assert!(!fits_registers(40, 160, 2, 1));
        assert!(!fits_registers(40, 160, 1, 2));
        assert!(!fits_registers(u32::MAX, u32::MAX, 1, 1));
        assert!(!fits_registers(1, 48, 1, 1));
        assert!(!fits_registers(0, 49, 1, 1));
        assert!(!fits_registers(0, 0, 1, 1));
    }

    #[test]
    fn independent_banks_overflow_and_named_vector_count() {
        let mut m = Module::new();
        let mut params = vec![Param::val(types::I32)];
        params.extend(vec![Param::val(types::F64); 9]);
        params.extend(vec![Param::val(types::I64); 6]);
        let sig = m.make_sig(params, vec![types::I64]);
        let p = plan(&m.types, &m.sigs[sig], 2).unwrap();
        assert_eq!(p.vector_registers, 8);
        assert_eq!(
            p.start,
            Start {
                gp_offset: 8,
                fp_offset: 64,
                overflow_offset: 0
            }
        );
        assert_eq!(
            p.params[9],
            Location::Stack {
                offset: 0,
                size: 8,
                align: 8
            }
        );
        assert_eq!(
            p.params[15],
            Location::Stack {
                offset: 8,
                size: 8,
                align: 8
            }
        );
        assert_eq!(p.stack_bytes, 16);
        assert_eq!(p.aligned_stack_bytes, 16);
    }

    #[test]
    fn named_overflow_is_not_the_whole_stack_area() {
        let mut m = Module::new();
        let sig = m.make_sig(vec![Param::val(types::I64); 9], vec![]);
        let p = plan(&m.types, &m.sigs[sig], 7).unwrap();
        assert_eq!(
            p.start,
            Start {
                gp_offset: 48,
                fp_offset: 48,
                overflow_offset: 8
            }
        );
        assert_eq!(p.stack_bytes, 24);
        assert_eq!(p.aligned_stack_bytes, 32);
    }

    #[test]
    fn mixed_aggregate_rollback_preserves_the_other_bank() {
        let mut m = Module::new();
        let mixed = m.types.intern(TypeData::Agg(vec![types::I64, types::F64]));
        let mut params = vec![Param::val(types::I64); 6];
        params.push(Param::val(mixed));
        params.push(Param::val(types::F64));
        let sig = m.make_sig(params, vec![]);
        let p = plan(&m.types, &m.sigs[sig], 7).unwrap();
        assert_eq!(
            p.params[6],
            Location::Stack {
                offset: 0,
                size: 16,
                align: 8
            }
        );
        assert_eq!(p.start.fp_offset, 48);
        assert_eq!(p.vector_registers, 1);
        assert!(matches!(&p.params[7], Location::Registers(us) if us[0].save_offset == 48));
    }

    #[test]
    fn hidden_sret_precedes_named_arguments_and_tokens_erase() {
        let mut m = Module::new();
        let big = m.types.intern(TypeData::Agg(vec![types::I64; 3]));
        let sig = m.make_sig(
            vec![
                Param::val(types::I32),
                Param::val(types::IO),
                Param::val(types::F64),
            ],
            vec![big],
        );
        let p = plan(&m.types, &m.sigs[sig], 1).unwrap();
        assert_eq!(p.start.gp_offset, 16);
        assert_eq!(p.params[1], Location::Erased);
        assert!(matches!(&p.params[0], Location::Registers(us) if us[0].save_offset == 8));
        assert!(plan(&m.types, &m.sigs[sig], 3).is_err());
    }
}
