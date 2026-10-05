//! kw11 (`[conc.mm.atomic.order]`, `[conc.mm.atomic.raw]`,
//! `[conc.mm.fence]`, K5 = A): the shape every pass shares.
//!
//! An ORDER OPERAND is not an expression: it is one of the five
//! lowercase marks of the closed builtin enum `Order`, spelled
//! `Order.<mark>` in the operand's slot, and it means the builtin
//! whatever else `Order` names in scope. Sema checks it; wolf_mem, the
//! WIR lowering and the checked machine read the same mark from the
//! same syntax through [`order_operand`] and never evaluate it.

use crate::{GreenNode, MemberExpr, ParenExpr, PathExpr, SyntaxKind};
use wolf_span::Span;

/// The five marks (C++20's set without `consume`), weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Order {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

impl Order {
    pub const ALL: [Order; 5] = [
        Order::Relaxed,
        Order::Acquire,
        Order::Release,
        Order::AcqRel,
        Order::SeqCst,
    ];

    /// The mark as written after `Order.`.
    pub fn mark(self) -> &'static str {
        match self {
            Order::Relaxed => "relaxed",
            Order::Acquire => "acquire",
            Order::Release => "release",
            Order::AcqRel => "acq_rel",
            Order::SeqCst => "seq_cst",
        }
    }

    pub fn from_mark(s: &str) -> Option<Order> {
        Order::ALL.into_iter().find(|o| o.mark() == s)
    }

    /// Does this order have an acquire half (it orders later accesses
    /// after the one it labels)?
    pub fn acquires(self) -> bool {
        matches!(self, Order::Acquire | Order::AcqRel | Order::SeqCst)
    }

    /// Does this order have a release half?
    pub fn releases(self) -> bool {
        matches!(self, Order::Release | Order::AcqRel | Order::SeqCst)
    }
}

/// The atomic operations on `*T` (`[conc.mm.atomic.raw]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomicOp {
    Load,
    Store,
    Swap,
    Add,
    Sub,
    And,
    Or,
    Xor,
    Cas,
}

impl AtomicOp {
    pub const ALL: [AtomicOp; 9] = [
        AtomicOp::Load,
        AtomicOp::Store,
        AtomicOp::Swap,
        AtomicOp::Add,
        AtomicOp::Sub,
        AtomicOp::And,
        AtomicOp::Or,
        AtomicOp::Xor,
        AtomicOp::Cas,
    ];

    /// The method name on `*T`.
    pub fn method(self) -> &'static str {
        match self {
            AtomicOp::Load => "atomic_load",
            AtomicOp::Store => "atomic_store",
            AtomicOp::Swap => "atomic_swap",
            AtomicOp::Add => "atomic_add",
            AtomicOp::Sub => "atomic_sub",
            AtomicOp::And => "atomic_and",
            AtomicOp::Or => "atomic_or",
            AtomicOp::Xor => "atomic_xor",
            AtomicOp::Cas => "atomic_cas",
        }
    }

    pub fn from_method(name: &str) -> Option<AtomicOp> {
        AtomicOp::ALL.into_iter().find(|o| o.method() == name)
    }

    /// The short name the intrinsic carries (`wolf.atomic.<op>.…`).
    pub fn short(self) -> &'static str {
        &self.method()["atomic_".len()..]
    }

    pub fn from_short(s: &str) -> Option<AtomicOp> {
        AtomicOp::ALL.into_iter().find(|o| o.short() == s)
    }

    /// How many value operands come before the order operand(s).
    pub fn value_args(self) -> usize {
        match self {
            AtomicOp::Load => 0,
            AtomicOp::Cas => 2,
            _ => 1,
        }
    }

    /// The argument positions that are order operands.
    pub fn order_slots(self) -> &'static [usize] {
        match self {
            AtomicOp::Load => &[0],
            AtomicOp::Cas => &[2, 3],
            _ => &[1],
        }
    }

    /// Does the operation write the pointee?
    pub fn writes(self) -> bool {
        self != AtomicOp::Load
    }

    /// Does the operation read the pointee?
    pub fn reads(self) -> bool {
        self != AtomicOp::Store
    }

    /// Is `o` an order this operation admits (`[conc.mm.atomic.raw.2]`)?
    /// A load has no release half to give and a store no acquire half;
    /// a read-modify-write admits all five. (A CAS's FAILURE order is
    /// [`cas_failure_admitted`]'s.)
    pub fn admits(self, o: Order) -> bool {
        match self {
            AtomicOp::Load => !matches!(o, Order::Release | Order::AcqRel),
            AtomicOp::Store => !matches!(o, Order::Acquire | Order::AcqRel),
            _ => true,
        }
    }
}

/// `[conc.mm.atomic.raw.2]`: a CAS's failure is a LOAD, so its order is
/// relaxed, acquire or seq_cst, and never stronger than the success
/// order's load half (acquire needs an acquiring success, seq_cst a
/// seq_cst one).
pub fn cas_failure_admitted(success: Order, failure: Order) -> bool {
    match failure {
        Order::Relaxed => true,
        Order::Acquire => success.acquires(),
        Order::SeqCst => success == Order::SeqCst,
        Order::Release | Order::AcqRel => false,
    }
}

/// `[conc.mm.fence]`: every order but relaxed (a relaxed fence orders
/// nothing).
pub fn fence_admits(o: Order) -> bool {
    o != Order::Relaxed
}

/// What an order operand's syntax says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderOperand {
    /// `Order.<mark>` with one of the five marks.
    Mark(Order),
    /// `Order.<name>` where `<name>` is no mark (the name's span).
    UnknownMark(Span),
    /// Anything else: an expression in the operand's slot.
    NotAMark,
}

/// Read an order operand. `text` maps a span to its source text.
/// Parentheses are transparent (`(Order.acquire)`).
pub fn order_operand(n: &GreenNode, text: impl Fn(Span) -> String) -> OrderOperand {
    let mut n = n;
    while n.kind == SyntaxKind::ParenExpr {
        match ParenExpr::cast(n).and_then(|p| p.expr()) {
            Some(inner) => n = inner,
            None => return OrderOperand::NotAMark,
        }
    }
    let Some(m) = MemberExpr::cast(n) else {
        return OrderOperand::NotAMark;
    };
    let (Some(base), Some(member)) = (m.base(), m.member()) else {
        return OrderOperand::NotAMark;
    };
    if base.kind != SyntaxKind::PathExpr {
        return OrderOperand::NotAMark;
    }
    let Some(head) = PathExpr::cast(base).and_then(|p| p.ident()) else {
        return OrderOperand::NotAMark;
    };
    if text(head.span) != "Order" {
        return OrderOperand::NotAMark;
    }
    match Order::from_mark(&text(member.span)) {
        Some(o) => OrderOperand::Mark(o),
        None => OrderOperand::UnknownMark(member.span),
    }
}

/// Is `n` syntactically `Order.<something>` — the shape the resolver
/// defers and sema refuses outside an order operand?
pub fn is_order_path(n: &GreenNode, text: impl Fn(Span) -> String) -> bool {
    !matches!(order_operand(n, text), OrderOperand::NotAMark)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_round_trip() {
        for o in Order::ALL {
            assert_eq!(Order::from_mark(o.mark()), Some(o));
        }
        assert_eq!(Order::from_mark("consume"), None);
        for op in AtomicOp::ALL {
            assert_eq!(AtomicOp::from_method(op.method()), Some(op));
            assert_eq!(AtomicOp::from_short(op.short()), Some(op));
        }
    }

    #[test]
    fn the_admitted_orders() {
        use Order::*;
        assert!(AtomicOp::Load.admits(Acquire) && !AtomicOp::Load.admits(Release));
        assert!(!AtomicOp::Load.admits(AcqRel) && AtomicOp::Load.admits(SeqCst));
        assert!(AtomicOp::Store.admits(Release) && !AtomicOp::Store.admits(Acquire));
        assert!(!AtomicOp::Store.admits(AcqRel));
        for o in Order::ALL {
            assert!(AtomicOp::Add.admits(o) && AtomicOp::Cas.admits(o));
        }
        // failure <= success's load half; never a release-type failure
        assert!(cas_failure_admitted(Relaxed, Relaxed));
        assert!(!cas_failure_admitted(Relaxed, Acquire));
        assert!(!cas_failure_admitted(Release, Acquire));
        assert!(cas_failure_admitted(AcqRel, Acquire));
        assert!(!cas_failure_admitted(AcqRel, SeqCst));
        assert!(cas_failure_admitted(SeqCst, SeqCst));
        assert!(!cas_failure_admitted(SeqCst, Release));
        assert!(!cas_failure_admitted(SeqCst, AcqRel));
        assert!(!fence_admits(Relaxed) && fence_admits(Acquire));
    }
}
