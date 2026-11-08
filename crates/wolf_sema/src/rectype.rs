//! `[type.rec]` (s287, wolf-lang#1062): a type may contain itself only
//! through storage that is not inline.
//!
//! A struct's fields, an enum's payloads and a tuple's elements are laid
//! out inline (`[abi.native.enum]`: "an enum value is its variant tag,
//! then its payload slots"), and so are the ok type and the tag payloads
//! of an error union, `wrapping[T]`, a `distinct` type's base and a range
//! type's bounds. A type that reaches itself through those alone would be
//! a value with no finite size, so it is E0404 ("this would be an infinite
//! type"), once per cycle, at the member through which the cycle's first
//! declared type reaches itself, the cycle named in full. `List`, `Map`, `Pool`, `handle`, `weak`, raw pointers, channels,
//! mutexes, function types and `shared` hold their element elsewhere, so a
//! cycle through one of them is no cycle here; a strong `shared` cycle is
//! E1006 (`[mem.shared.rc.2]`), checked at `mem`.
//!
//! A generic struct or enum is walked at its arguments: its field types
//! are substituted (`struct Box[T] { v: T }` makes `Box[L]` hold an `L`
//! inline, and `struct Bag[T] { items: List[T] }` does not), and a type is
//! on the cycle when the same instantiation recurs. A recursion that only
//! grows its arguments (`L[T]` holding `L[(T, T)]`) never recurs exactly;
//! the walk stops at a fixed depth and reports nothing for it.

use std::collections::{BTreeMap, BTreeSet};

use wolf_diag::{Diagnostic, codes};
use wolf_span::Span;

use crate::graph::Package;
use crate::sig::{ItemSig, SigTables};
use crate::types::{TyId, TyKind, TypeTable, subst};

/// The deepest chain of inline nominal members the walk follows.
const DEPTH: usize = 32;

/// One frame of the walk: the nominal type being expanded, its display
/// name, and the member (field or variant) whose type is being walked.
struct Frame {
    ty: TyId,
    node: (usize, String),
    member: Span,
}

struct Walk<'a> {
    sigs: &'a SigTables,
    table: TypeTable,
    stack: Vec<Frame>,
    reported: BTreeSet<Vec<(usize, String)>>,
    out: Vec<Diagnostic>,
}

/// E0404 for every by-value cycle among the package's struct, enum and
/// `distinct` types (`[type.rec]`), walked from each declaration in
/// declaration order.
pub(crate) fn check(pkg: &Package, sigs: &SigTables) -> Vec<Diagnostic> {
    let mut w = Walk {
        sigs,
        table: sigs.table.clone(),
        stack: Vec::new(),
        reported: BTreeSet::new(),
        out: Vec::new(),
    };
    for &m in &pkg.topo {
        for item in &pkg.tables[m].items {
            let generics = match sigs.get(m, &item.name) {
                Some(ItemSig::Struct(ss)) => ss.generics.clone(),
                Some(ItemSig::Enum { generics, .. }) => generics.clone(),
                Some(ItemSig::Distinct { .. }) => Vec::new(),
                _ => continue,
            };
            let args: Vec<TyId> = generics
                .into_iter()
                .map(|g| w.table.intern(TyKind::Rigid(g)))
                .collect();
            let ty = w.table.intern(TyKind::Nominal {
                module: m as u32,
                name: item.name.clone(),
                args,
            });
            w.nominal(ty, m, &item.name);
        }
    }
    w.out
}

impl Walk<'_> {
    /// Expand the nominal `ty` (`module.name` at its arguments): each member
    /// type walked with this type on the stack.
    fn nominal(&mut self, ty: TyId, module: usize, name: &str) {
        if let Some(at) = self.stack.iter().position(|f| f.ty == ty) {
            self.report(at);
            return;
        }
        if self.stack.len() >= DEPTH {
            return;
        }
        let args: Vec<TyId> = match self.table.kind(ty) {
            TyKind::Nominal { args, .. } => args.clone(),
            _ => Vec::new(),
        };
        let members: Vec<(Span, Vec<TyId>, Vec<String>)> = match self.sigs.get(module, name) {
            Some(ItemSig::Struct(ss)) => ss
                .fields
                .iter()
                .map(|f| (f.span, vec![f.ty], ss.generics.clone()))
                .collect(),
            Some(ItemSig::Enum {
                variants, generics, ..
            }) => variants
                .iter()
                .map(|v| (v.span, v.payload.clone(), generics.clone()))
                .collect(),
            Some(ItemSig::Distinct { base, name_span }) => {
                vec![(*name_span, vec![*base], Vec::new())]
            }
            _ => return,
        };
        for (span, tys, generics) in members {
            let map: BTreeMap<String, TyId> =
                generics.into_iter().zip(args.iter().copied()).collect();
            self.stack.push(Frame {
                ty,
                node: (module, name.to_string()),
                member: span,
            });
            for t in tys {
                let t = if map.is_empty() {
                    t
                } else {
                    subst(&mut self.table, t, &map)
                };
                self.inline(t);
            }
            self.stack.pop();
        }
    }

    /// Walk what a type holds inline.
    fn inline(&mut self, ty: TyId) {
        match self.table.kind(ty).clone() {
            TyKind::Nominal { module, name, .. } => self.nominal(ty, module as usize, &name),
            TyKind::Tuple(ts) => {
                for t in ts {
                    self.inline(t);
                }
            }
            TyKind::ErrUnion(ok, row) => {
                self.inline(ok);
                self.inline(row);
            }
            TyKind::Row { tags, .. } => {
                for (_, payload) in tags {
                    for t in payload {
                        self.inline(t);
                    }
                }
            }
            TyKind::Wrapping(t) | TyKind::Distinct(t) | TyKind::Range(t) => self.inline(t),
            // Everything else holds its element elsewhere, or holds none.
            _ => {}
        }
    }

    /// The stack from `at` up is one cycle: one E0404 for it, the first
    /// time it is met.
    fn report(&mut self, at: usize) {
        let mut members: Vec<(usize, String)> =
            self.stack[at..].iter().map(|f| f.node.clone()).collect();
        let names: Vec<String> = self.stack[at..]
            .iter()
            .map(|f| f.node.1.clone())
            .chain(std::iter::once(self.stack[at].node.1.clone()))
            .collect();
        members.sort();
        members.dedup();
        if !self.reported.insert(members) {
            return;
        }
        // Reported at the member through which the cycle's first type, the
        // one the walk started from, reaches itself: inside a generic
        // struct's own definition (`v` of `Box[T]`) no cycle is visible.
        let member = self.stack[at].member;
        let head = &self.stack[at].node.1;
        let cycle = names.join(" -> ");
        self.out.push(
            Diagnostic::error(
                codes::E0404,
                member,
                format!("`{head}` contains itself by value, so it would have no finite size"),
            )
            .with_label(format!("`{head}` reaches itself through here: {cycle}"))
            .with_note(format!(
                "a struct's fields, an enum's payloads and a tuple's elements are laid \
                 out inline, so a type that reaches itself through them alone would be \
                 a value with no finite size ([type.rec]). Recurse through a container \
                 that holds its element elsewhere: `List[{head}]`, `Map[K, {head}]`, or \
                 a `handle {head}` into a pool."
            )),
        );
    }
}
