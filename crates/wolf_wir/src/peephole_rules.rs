//! The algebraic identity table — peephole stage 3, as DATA.
//!
//! Rules are matcher entries, not ad-hoc `if`s, because s42/X11 wants
//! to derive and verify this set offline with egg (egg-egraphs.txt):
//! a rule is (opcode, pattern, rewrite), the interpreter lives in
//! [`crate::build`], and adding a rule is adding a row.
//!
//! Soundness notes baked into the rows:
//! - Checked ops keep their trap semantics: only rewrites that cannot
//!   change trap behavior are listed (`x+0`, `x*1`, `x*0` — none can
//!   overflow; `x/1`, `x%1` — no zero or MIN/-1 case appears).
//! - `x - x → 0` is listed for the checked/wrapping/saturating forms
//!   alike (subtraction of a value from itself never overflows).
//! - Float identities are deliberately ABSENT: `x + 0.0` is not an
//!   identity at `-0.0`, and this table never speculates on IEEE edge
//!   cases.
//! - Bitwise `-1` rows rely on `iconst` payloads being sign-extended:
//!   `-1` is the all-ones pattern at every width.
//!
//! `br const → jmp` and the same-operand `icmp` family live beside the
//! table ([`icmp_same`], `FuncBuilder::ins_br`) — they rewrite control,
//! not operands, but count as identity hits in the stats.

use crate::ops::{IntCc, Opcode};

/// Which operand a pattern inspects.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Operand {
    Lhs,
    Rhs,
}

/// The pattern half of a rule (binary ops only — unary identity is
/// vacuous in the closed op set).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pat {
    /// The given operand is an integer constant with this payload.
    ConstIs(Operand, i64),
    /// Both operands are the same SSA value.
    Same,
}

/// The rewrite half of a rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rewrite {
    /// Replace with the left operand.
    Lhs,
    /// Replace with the right operand.
    Rhs,
    /// Replace with an integer constant of the result type.
    ConstInt(i64),
    /// Replace with a boolean constant.
    ConstBool(bool),
}

/// One matcher entry.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub op: Opcode,
    pub pat: Pat,
    pub rw: Rewrite,
}

macro_rules! rule {
    ($op:ident, $pat:expr, $rw:expr) => {
        Rule {
            op: Opcode::$op,
            pat: $pat,
            rw: $rw,
        }
    };
}

use Operand::{Lhs, Rhs};
use Pat::{ConstIs, Same};

/// The rule set, first match wins (row order is part of the table).
pub static RULES: &[Rule] = &[
    // x + 0 / 0 + x — additive identity (cannot trap).
    rule!(IaddChk, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(IaddChk, ConstIs(Lhs, 0), Rewrite::Rhs),
    rule!(IaddWrap, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(IaddWrap, ConstIs(Lhs, 0), Rewrite::Rhs),
    rule!(IaddSat, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(IaddSat, ConstIs(Lhs, 0), Rewrite::Rhs),
    // x - 0 and x - x.
    rule!(IsubChk, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(IsubChk, Same, Rewrite::ConstInt(0)),
    rule!(IsubWrap, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(IsubWrap, Same, Rewrite::ConstInt(0)),
    rule!(IsubSat, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(IsubSat, Same, Rewrite::ConstInt(0)),
    // x * 1 / 1 * x and x * 0 / 0 * x (zero product cannot overflow).
    rule!(ImulChk, ConstIs(Rhs, 1), Rewrite::Lhs),
    rule!(ImulChk, ConstIs(Lhs, 1), Rewrite::Rhs),
    rule!(ImulChk, ConstIs(Rhs, 0), Rewrite::ConstInt(0)),
    rule!(ImulChk, ConstIs(Lhs, 0), Rewrite::ConstInt(0)),
    rule!(ImulWrap, ConstIs(Rhs, 1), Rewrite::Lhs),
    rule!(ImulWrap, ConstIs(Lhs, 1), Rewrite::Rhs),
    rule!(ImulWrap, ConstIs(Rhs, 0), Rewrite::ConstInt(0)),
    rule!(ImulWrap, ConstIs(Lhs, 0), Rewrite::ConstInt(0)),
    rule!(ImulSat, ConstIs(Rhs, 1), Rewrite::Lhs),
    rule!(ImulSat, ConstIs(Lhs, 1), Rewrite::Rhs),
    rule!(ImulSat, ConstIs(Rhs, 0), Rewrite::ConstInt(0)),
    rule!(ImulSat, ConstIs(Lhs, 0), Rewrite::ConstInt(0)),
    // x / 1 and x % 1 (no zero, no MIN/-1: trap-free).
    rule!(IdivChk, ConstIs(Rhs, 1), Rewrite::Lhs),
    rule!(IremChk, ConstIs(Rhs, 1), Rewrite::ConstInt(0)),
    // The unsigned checked family (s26): the same trap-free identities.
    // u + 0, u - 0, u - u (no borrow), u * 1, u * 0, u / 1, u % 1.
    rule!(UaddChk, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(UaddChk, ConstIs(Lhs, 0), Rewrite::Rhs),
    rule!(UsubChk, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(UsubChk, Same, Rewrite::ConstInt(0)),
    rule!(UmulChk, ConstIs(Rhs, 1), Rewrite::Lhs),
    rule!(UmulChk, ConstIs(Lhs, 1), Rewrite::Rhs),
    rule!(UmulChk, ConstIs(Rhs, 0), Rewrite::ConstInt(0)),
    rule!(UmulChk, ConstIs(Lhs, 0), Rewrite::ConstInt(0)),
    rule!(UdivChk, ConstIs(Rhs, 1), Rewrite::Lhs),
    rule!(UremChk, ConstIs(Rhs, 1), Rewrite::ConstInt(0)),
    // Bitwise identity / absorption / annihilation.
    rule!(Band, Same, Rewrite::Lhs),
    rule!(Band, ConstIs(Rhs, 0), Rewrite::ConstInt(0)),
    rule!(Band, ConstIs(Lhs, 0), Rewrite::ConstInt(0)),
    rule!(Band, ConstIs(Rhs, -1), Rewrite::Lhs),
    rule!(Band, ConstIs(Lhs, -1), Rewrite::Rhs),
    rule!(Bor, Same, Rewrite::Lhs),
    rule!(Bor, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(Bor, ConstIs(Lhs, 0), Rewrite::Rhs),
    rule!(Bor, ConstIs(Rhs, -1), Rewrite::ConstInt(-1)),
    rule!(Bor, ConstIs(Lhs, -1), Rewrite::ConstInt(-1)),
    rule!(Bxor, Same, Rewrite::ConstInt(0)),
    rule!(Bxor, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(Bxor, ConstIs(Lhs, 0), Rewrite::Rhs),
    // Shifts by zero.
    rule!(Shl, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(Lshr, ConstIs(Rhs, 0), Rewrite::Lhs),
    rule!(Ashr, ConstIs(Rhs, 0), Rewrite::Lhs),
];

/// Run the table for one speculative instruction. `same` is the
/// both-operands-equal bit; `lhs_c`/`rhs_c` are integer-constant
/// payloads when the operands' defs are `iconst`.
pub fn match_rules(
    op: Opcode,
    same: bool,
    lhs_c: Option<i64>,
    rhs_c: Option<i64>,
) -> Option<Rewrite> {
    for rule in RULES {
        if rule.op != op {
            continue;
        }
        let hit = match rule.pat {
            Pat::Same => same,
            Pat::ConstIs(Operand::Lhs, n) => lhs_c == Some(n),
            Pat::ConstIs(Operand::Rhs, n) => rhs_c == Some(n),
        };
        if hit {
            return Some(rule.rw);
        }
    }
    None
}

/// `icmp.CC x, x` — the same-operand comparison family (reflexive
/// conditions are true, irreflexive false).
pub fn icmp_same(cc: IntCc) -> bool {
    matches!(
        cc,
        IntCc::Eq | IntCc::Sle | IntCc::Sge | IntCc::Ule | IntCc::Uge
    )
}
