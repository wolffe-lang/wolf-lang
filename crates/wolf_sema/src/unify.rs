//! The unification engine (s13, Target 3).
//!
//! Union-find over existential type variables in a flat arena, with
//! **levels** carrying the DK ordered-context scope discipline: every
//! variable records the scope depth at which it was minted, and a
//! variable never ends up bound to a type that mentions a *deeper*
//! variable — binding lowers the levels of flexible variables it
//! captures (the classic level-adjustment move) and refuses to capture
//! a deeper rigid. This replaces literal context reordering while
//! preserving the DK semantics (report 02 §Dunfield & Krishnaswami).
//!
//! Every mutation is logged, so [`VarStore::snapshot`] /
//! [`VarStore::rollback`] give O(entries-since-snapshot) speculative
//! checking — built now because s14's blanket-impl matching and s17's
//! method resolution both need trial unification, and s13 itself uses
//! it for `!T` ok-injection.
//!
//! The occurs check runs on find-resolved forms; an occurs failure
//! carries both sides so the diagnostic can render the cycle (E0404).

use wolf_span::Span;

use crate::types::{TyId, TyKind, TypeTable};

/// The closed literal kinds (D27): integer literals are polymorphic
/// over `{integer}`, float literals over `{float}`; `{number}` is the
/// meet of an operator that accepts either. Checked against context
/// first; defaulting (`i32`/`f64`) is a *rule* applied at body end,
/// not a solver.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumKind {
    Any,
    Num,
    Integer,
    Float,
}

impl NumKind {
    /// The meet of two kind constraints; `None` = incompatible.
    fn meet(self, other: NumKind) -> Option<NumKind> {
        use NumKind::*;
        Some(match (self, other) {
            (Any, k) | (k, Any) => k,
            (Num, k) | (k, Num) => k,
            (Integer, Integer) => Integer,
            (Float, Float) => Float,
            (Integer, Float) | (Float, Integer) => return None,
        })
    }

    /// Placeholder rendering for an unresolved variable of this kind.
    pub fn placeholder(self) -> &'static str {
        match self {
            NumKind::Any => "_",
            NumKind::Num => "{number}",
            NumKind::Integer => "{integer}",
            NumKind::Float => "{float}",
        }
    }
}

/// One existential variable's state in the flat arena.
#[derive(Clone, Copy, PartialEq, Debug)]
struct VarState {
    /// Union-find parent; self-index at a root.
    parent: u32,
    /// Scope depth at mint time (the ordered-context discipline).
    level: u32,
    kind: NumKind,
    /// The root's solution, if bound.
    solution: Option<TyId>,
    /// Where the variable came from (E0405's span).
    origin: Span,
}

/// One undo-log entry. Rollback replays these in reverse.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Undo {
    Parent(u32, u32),
    Kind(u32, NumKind),
    Level(u32, u32),
    Solution(u32),
}

/// A rollback point: log length + arena length, captured together so
/// variables minted during a trial vanish with it.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    log: usize,
    vars: usize,
}

/// Why a unification failed.
#[derive(Clone, Copy, Debug)]
pub enum UnifyErr {
    /// Structural mismatch between the two argument types.
    Mismatch,
    /// Occurs-check failure: binding `var` to `ty` would build an
    /// infinite type (E0404 renders the cycle).
    Occurs { var: TyId, ty: TyId },
    /// Two const-generic argument forms differ beyond the linear
    /// line ([`crate::ctfe::norm`], s16): they *may* be equal, but
    /// deciding it needs an explicit witness (E0707) — never a guess.
    NeedsWitness,
}

/// The per-body variable arena + undo log.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VarStore {
    vars: Vec<VarState>,
    log: Vec<Undo>,
}

impl VarStore {
    pub fn new() -> VarStore {
        VarStore::default()
    }

    /// Mint a fresh variable at `level` with kind `kind`, interned into
    /// `table` as a type.
    pub fn fresh(
        &mut self,
        table: &mut TypeTable,
        level: u32,
        kind: NumKind,
        origin: Span,
    ) -> TyId {
        let idx = u32::try_from(self.vars.len()).expect("var arena overflow");
        self.vars.push(VarState {
            parent: idx,
            level,
            kind,
            solution: None,
            origin,
        });
        table.intern(TyKind::Var(idx))
    }

    pub fn len(&self) -> usize {
        self.vars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// The root of `v`'s equivalence class (no path compression — the
    /// log stays small and rollback stays trivial; chains are shallow
    /// in practice because roots are chosen by level).
    fn find(&self, mut v: u32) -> u32 {
        while self.vars[v as usize].parent != v {
            v = self.vars[v as usize].parent;
        }
        v
    }

    /// The variable's solution, fully find-resolved at the top level.
    pub fn probe(&self, v: u32) -> Option<TyId> {
        self.vars[self.find(v) as usize].solution
    }

    /// Root kind of `v` (for rendering).
    pub fn kind_of(&self, v: u32) -> NumKind {
        self.vars[self.find(v) as usize].kind
    }

    /// Origin span of `v`'s root.
    pub fn origin_of(&self, v: u32) -> Span {
        self.vars[self.find(v) as usize].origin
    }

    /// Every unresolved root variable: (index, kind, origin). The
    /// defaulting rule and E0405 walk this at body end.
    pub fn unresolved_roots(&self) -> Vec<(u32, NumKind, Span)> {
        (0..self.vars.len() as u32)
            .filter(|&v| {
                self.vars[v as usize].parent == v && self.vars[v as usize].solution.is_none()
            })
            .map(|v| {
                let s = &self.vars[v as usize];
                (v, s.kind, s.origin)
            })
            .collect()
    }

    /// A rollback point. Rolling back undoes every union, binding,
    /// kind-narrowing, and level adjustment since — and forgets
    /// variables minted since — in O(entries since snapshot).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            log: self.log.len(),
            vars: self.vars.len(),
        }
    }

    pub fn rollback(&mut self, snap: Snapshot) {
        while self.log.len() > snap.log {
            match self.log.pop().expect("log entry") {
                Undo::Parent(v, old) => self.vars[v as usize].parent = old,
                Undo::Kind(v, old) => self.vars[v as usize].kind = old,
                Undo::Level(v, old) => self.vars[v as usize].level = old,
                Undo::Solution(v) => self.vars[v as usize].solution = None,
            }
        }
        self.vars.truncate(snap.vars);
    }

    fn set_parent(&mut self, v: u32, parent: u32) {
        self.log.push(Undo::Parent(v, self.vars[v as usize].parent));
        self.vars[v as usize].parent = parent;
    }

    fn set_kind(&mut self, v: u32, kind: NumKind) {
        self.log.push(Undo::Kind(v, self.vars[v as usize].kind));
        self.vars[v as usize].kind = kind;
    }

    fn set_level(&mut self, v: u32, level: u32) {
        self.log.push(Undo::Level(v, self.vars[v as usize].level));
        self.vars[v as usize].level = level;
    }

    fn set_solution(&mut self, v: u32, ty: TyId) {
        debug_assert!(self.vars[v as usize].solution.is_none());
        self.log.push(Undo::Solution(v));
        self.vars[v as usize].solution = Some(ty);
    }

    /// Resolve the *head* of `ty`: chase a solved variable to its
    /// solution (recursively at the head only).
    pub fn shallow(&self, table: &TypeTable, ty: TyId) -> TyId {
        let mut t = ty;
        loop {
            match table.kind(t) {
                TyKind::Var(v) => match self.probe(*v) {
                    Some(sol) => t = sol,
                    None => return t,
                },
                _ => return t,
            }
        }
    }
}

/// Does concrete kind `ty` satisfy the literal-kind constraint?
fn kind_admits(table: &TypeTable, kind: NumKind, ty: TyId) -> bool {
    let concrete = match table.kind(ty) {
        TyKind::Prim(p) => {
            if p.is_integer() {
                NumKind::Integer
            } else if p.is_float() {
                NumKind::Float
            } else {
                NumKind::Any
            }
        }
        // `wrapping[uN]` is an integer type for literal/operator kinds.
        TyKind::Wrapping(_) => NumKind::Integer,
        TyKind::Error | TyKind::Never => return true,
        _ => NumKind::Any,
    };
    match kind {
        NumKind::Any => true,
        NumKind::Num => matches!(concrete, NumKind::Integer | NumKind::Float),
        NumKind::Integer => concrete == NumKind::Integer,
        NumKind::Float => concrete == NumKind::Float,
    }
}

/// Occurs check + level discipline on the find-resolved form of `ty`,
/// prior to binding root `var`: fails if `var` itself occurs; lowers
/// deeper flexible vars to `var`'s level (the ordered-context move —
/// a var never stays bound under a type mentioning a deeper var).
fn occurs_adjust(
    store: &mut VarStore,
    table: &TypeTable,
    var: u32,
    level: u32,
    ty: TyId,
) -> Result<(), ()> {
    match table.kind(ty).clone() {
        TyKind::Var(w) => {
            let root = store.find(w);
            if root == var {
                return Err(());
            }
            if let Some(sol) = store.vars[root as usize].solution {
                return occurs_adjust(store, table, var, level, sol);
            }
            if store.vars[root as usize].level > level {
                store.set_level(root, level);
            }
            Ok(())
        }
        TyKind::Wrapping(t)
        | TyKind::Range(t)
        | TyKind::Ptr(t)
        | TyKind::Shared(t)
        | TyKind::Handle(t)
        | TyKind::Weak(t)
        | TyKind::Distinct(t)
        | TyKind::List(t)
        | TyKind::Pool(t)
        | TyKind::Chan(t)
        | TyKind::Mutex(t)
        | TyKind::Proj(t, _) => occurs_adjust(store, table, var, level, t),
        TyKind::ErrUnion(t, row) => {
            occurs_adjust(store, table, var, level, t)?;
            occurs_adjust(store, table, var, level, row)
        }
        TyKind::Row { tags, tail } => {
            for (_, payload) in tags {
                for t in payload {
                    occurs_adjust(store, table, var, level, t)?;
                }
            }
            if let Some(t) = tail {
                occurs_adjust(store, table, var, level, t)?;
            }
            Ok(())
        }
        TyKind::Tuple(ts) => {
            for t in ts {
                occurs_adjust(store, table, var, level, t)?;
            }
            Ok(())
        }
        TyKind::Fn(params, ret) => {
            for t in params {
                occurs_adjust(store, table, var, level, t)?;
            }
            occurs_adjust(store, table, var, level, ret)
        }
        _ => Ok(()),
    }
}

/// Unify `a` and `b` (equality-only — no subtyping until s15 adds width
/// subtyping at call/return boundaries). `<error>` unifies silently
/// with everything; `Never` (bottom) is compatible with everything.
pub fn unify(
    table: &mut TypeTable,
    store: &mut VarStore,
    a: TyId,
    b: TyId,
) -> Result<(), UnifyErr> {
    let a = store.shallow(table, a);
    let b = store.shallow(table, b);
    if a == b {
        return Ok(()); // hash-consing fast path
    }
    let (ka, kb) = (table.kind(a).clone(), table.kind(b).clone());
    match (ka, kb) {
        (TyKind::Error, _) | (_, TyKind::Error) => Ok(()),
        (TyKind::Never, _) | (_, TyKind::Never) => Ok(()),
        (TyKind::Var(va), TyKind::Var(vb)) => {
            let (ra, rb) = (store.find(va), store.find(vb));
            if ra == rb {
                return Ok(());
            }
            let merged = store.vars[ra as usize]
                .kind
                .meet(store.vars[rb as usize].kind)
                .ok_or(UnifyErr::Mismatch)?;
            // The shallower variable stays root (ordered contexts:
            // solutions must make sense at the outermost scope).
            let (root, child) = if store.vars[ra as usize].level <= store.vars[rb as usize].level {
                (ra, rb)
            } else {
                (rb, ra)
            };
            store.set_parent(child, root);
            if store.vars[root as usize].kind != merged {
                store.set_kind(root, merged);
            }
            Ok(())
        }
        (TyKind::Var(v), _) => bind(table, store, v, b),
        (_, TyKind::Var(v)) => bind(table, store, v, a),
        (TyKind::Prim(p), TyKind::Prim(q)) if p == q => Ok(()),
        (TyKind::Unit, TyKind::Unit) => Ok(()),
        (TyKind::Rigid(x), TyKind::Rigid(y)) if x == y => Ok(()),
        (
            TyKind::Nominal {
                module: ma,
                name: na,
            },
            TyKind::Nominal {
                module: mb,
                name: nb,
            },
        ) if ma == mb && na == nb => Ok(()),
        // Opaque forms (generic instantiations with possible
        // const-generic arguments): identical renderings are equal;
        // differing renderings go through const-expression
        // normalization (s16) — closed expressions compare by value,
        // linear arithmetic ring-normalizes, and anything beyond the
        // line asks for a witness rather than guessing.
        (TyKind::Unsupported(x), TyKind::Unsupported(y)) => {
            match crate::ctfe::norm::opaque_eq(&x, &y) {
                crate::ctfe::norm::OpaqueEq::Equal => Ok(()),
                crate::ctfe::norm::OpaqueEq::Distinct => Err(UnifyErr::Mismatch),
                crate::ctfe::norm::OpaqueEq::NeedsWitness => Err(UnifyErr::NeedsWitness),
            }
        }
        (TyKind::Meta(x), TyKind::Meta(y)) if x == y => Ok(()),
        (
            TyKind::Dyn {
                module: ma,
                name: na,
            },
            TyKind::Dyn {
                module: mb,
                name: nb,
            },
        ) if ma == mb && na == nb => Ok(()),
        // Projections are equal textually (after normalization — the
        // rewrite engine runs before unification; D28: no equality
        // solver): same associated name, unifiable bases.
        (TyKind::Proj(ba, na), TyKind::Proj(bb, nb)) if na == nb => unify(table, store, ba, bb),
        (TyKind::RegionTy, TyKind::RegionTy)
        | (TyKind::TypeTy, TyKind::TypeTy)
        | (TyKind::TaskScope, TyKind::TaskScope)
        | (TyKind::Proc, TyKind::Proc)
        | (TyKind::ExitReason, TyKind::ExitReason) => Ok(()),
        (TyKind::ErrUnion(x, rx), TyKind::ErrUnion(y, ry)) => {
            unify(table, store, x, y)?;
            unify(table, store, rx, ry)
        }
        (
            TyKind::Row {
                tags: ta,
                tail: tla,
            },
            TyKind::Row {
                tags: tb,
                tail: tlb,
            },
        ) => unify_rows(table, store, ta, tla, tb, tlb),
        (TyKind::Wrapping(x), TyKind::Wrapping(y))
        | (TyKind::Range(x), TyKind::Range(y))
        | (TyKind::Ptr(x), TyKind::Ptr(y))
        | (TyKind::Shared(x), TyKind::Shared(y))
        | (TyKind::Handle(x), TyKind::Handle(y))
        | (TyKind::Weak(x), TyKind::Weak(y))
        | (TyKind::Distinct(x), TyKind::Distinct(y))
        | (TyKind::List(x), TyKind::List(y))
        | (TyKind::Pool(x), TyKind::Pool(y))
        | (TyKind::Chan(x), TyKind::Chan(y))
        | (TyKind::Mutex(x), TyKind::Mutex(y)) => unify(table, store, x, y),
        (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
            if xs.len() != ys.len() {
                return Err(UnifyErr::Mismatch);
            }
            for (x, y) in xs.into_iter().zip(ys) {
                unify(table, store, x, y)?;
            }
            Ok(())
        }
        (TyKind::Fn(px, rx), TyKind::Fn(py, ry)) => {
            if px.len() != py.len() {
                return Err(UnifyErr::Mismatch);
            }
            for (x, y) in px.into_iter().zip(py) {
                unify(table, store, x, y)?;
            }
            unify(table, store, rx, ry)
        }
        _ => Err(UnifyErr::Mismatch),
    }
}

/// Row unification, Koka-style but over *sets* (duplicate labels were
/// rejected at elaboration): shared tags unify pointwise on payloads;
/// a one-sided remainder must be absorbed by the other side's tail
/// *variable* (a rigid tail never absorbs — rigids are the caller's
/// row, not ours to grow). A two-sided remainder would need fresh row
/// variables on both tails; no s15 surface produces that shape, so it
/// is a mismatch — deliberately, not accidentally.
fn unify_rows(
    table: &mut TypeTable,
    store: &mut VarStore,
    ta: Vec<(String, Vec<TyId>)>,
    tla: Option<TyId>,
    tb: Vec<(String, Vec<TyId>)>,
    tlb: Option<TyId>,
) -> Result<(), UnifyErr> {
    // Resolve solved tails into their rows first.
    let resolve_tail = |store: &VarStore, table: &TypeTable, t: Option<TyId>| -> Option<TyId> {
        t.map(|t| store.shallow(table, t))
    };
    let (tla, tlb) = (
        resolve_tail(store, table, tla),
        resolve_tail(store, table, tlb),
    );
    // A tail solved to a concrete row folds into its host side.
    if let Some(t) = tla
        && let TyKind::Row { tags, tail } = table.kind(t).clone()
    {
        let mut merged = ta;
        merged.extend(tags);
        let host = table.row(merged, tail);
        let other = table.row(tb, tlb);
        return unify(table, store, host, other);
    }
    if let Some(t) = tlb
        && let TyKind::Row { tags, tail } = table.kind(t).clone()
    {
        let mut merged = tb;
        merged.extend(tags);
        let host = table.row(merged, tail);
        let other = table.row(ta, tla);
        return unify(table, store, other, host);
    }
    // Shared tags: pointwise payload unification (arity is part of
    // the tag's identity — a mismatch is a mismatch).
    for (n, pa) in &ta {
        if let Some((_, pb)) = tb.iter().find(|(m, _)| m == n) {
            if pa.len() != pb.len() {
                return Err(UnifyErr::Mismatch);
            }
            for (x, y) in pa.iter().zip(pb.iter()) {
                unify(table, store, *x, *y)?;
            }
        }
    }
    let only_a: Vec<(String, Vec<TyId>)> = ta
        .iter()
        .filter(|(n, _)| !tb.iter().any(|(m, _)| m == n))
        .cloned()
        .collect();
    let only_b: Vec<(String, Vec<TyId>)> = tb
        .iter()
        .filter(|(n, _)| !ta.iter().any(|(m, _)| m == n))
        .cloned()
        .collect();
    let empty = table.row(Vec::new(), None);
    match (only_a.is_empty(), only_b.is_empty()) {
        (true, true) => match (tla, tlb) {
            (None, None) => Ok(()),
            (Some(x), Some(y)) => unify(table, store, x, y),
            (Some(x), None) | (None, Some(x)) => unify(table, store, x, empty),
        },
        (false, true) => {
            // b needs the tags only a has: its tail must absorb them.
            let Some(t) = tlb else {
                return Err(UnifyErr::Mismatch);
            };
            let remainder = table.row(only_a, tla);
            unify(table, store, t, remainder)
        }
        (true, false) => {
            let Some(t) = tla else {
                return Err(UnifyErr::Mismatch);
            };
            let remainder = table.row(only_b, tlb);
            unify(table, store, t, remainder)
        }
        (false, false) => Err(UnifyErr::Mismatch),
    }
}

/// Bind root-of-`v` to `ty` after kind, occurs, and level checks.
fn bind(table: &mut TypeTable, store: &mut VarStore, v: u32, ty: TyId) -> Result<(), UnifyErr> {
    let root = store.find(v);
    let kind = store.vars[root as usize].kind;
    if !kind_admits(table, kind, ty) {
        return Err(UnifyErr::Mismatch);
    }
    let level = store.vars[root as usize].level;
    let var_ty = table.intern(TyKind::Var(root));
    occurs_adjust(store, table, root, level, ty)
        .map_err(|()| UnifyErr::Occurs { var: var_ty, ty })?;
    store.set_solution(root, ty);
    Ok(())
}

/// Join two branch types for synthesis-mode orientation: `Never` (a
/// diverging branch) defers to the other side; otherwise the types must
/// unify and the first is the join.
pub fn join(
    table: &mut TypeTable,
    store: &mut VarStore,
    a: TyId,
    b: TyId,
) -> Result<TyId, UnifyErr> {
    let ra = store.shallow(table, a);
    let rb = store.shallow(table, b);
    if matches!(table.kind(ra), TyKind::Never) {
        return Ok(rb);
    }
    if matches!(table.kind(rb), TyKind::Never) {
        return Ok(ra);
    }
    unify(table, store, ra, rb)?;
    Ok(ra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Prim;
    use std::path::Path;
    use wolf_span::{SourceMap, Span};

    fn sp() -> Span {
        let mut sm = SourceMap::new();
        let f = sm.intern(Path::new("t.lu"));
        Span::new(f, 0, 0)
    }

    #[test]
    fn literals_check_against_context_kinds() {
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let intlit = s.fresh(&mut t, 0, NumKind::Integer, sp());
        let i64_ = t.prim(Prim::I64);
        assert!(unify(&mut t, &mut s, intlit, i64_).is_ok());
        let f64_ = t.prim(Prim::F64);
        let intlit2 = s.fresh(&mut t, 0, NumKind::Integer, sp());
        assert!(
            unify(&mut t, &mut s, intlit2, f64_).is_err(),
            "an integer literal is not a float"
        );
        // wrapping[u32] is an integer type for literal purposes (X3).
        let u32_ = t.prim(Prim::U32);
        let w = t.intern(TyKind::Wrapping(u32_));
        let intlit3 = s.fresh(&mut t, 0, NumKind::Integer, sp());
        assert!(unify(&mut t, &mut s, intlit3, w).is_ok());
    }

    #[test]
    fn occurs_check_rejects_infinite_types() {
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let a = s.fresh(&mut t, 0, NumKind::Any, sp());
        let int = t.prim(Prim::Int);
        let f = t.intern(TyKind::Fn(vec![a], int));
        match unify(&mut t, &mut s, a, f) {
            Err(UnifyErr::Occurs { .. }) => {}
            other => panic!("expected occurs failure, got {other:?}"),
        }
    }

    #[test]
    fn levels_keep_the_root_shallow_and_adjust_captures() {
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let shallow = s.fresh(&mut t, 0, NumKind::Any, sp());
        let deep = s.fresh(&mut t, 3, NumKind::Any, sp());
        // Binding the shallow var to a type mentioning the deep var
        // lowers the deep var to the shallow level.
        let int = t.prim(Prim::Int);
        let f = t.intern(TyKind::Fn(vec![deep], int));
        assert!(unify(&mut t, &mut s, shallow, f).is_ok());
        let TyKind::Var(d) = *t.kind(deep) else {
            panic!("deep is a var")
        };
        assert_eq!(
            s.vars[s.find(d) as usize].level,
            0,
            "capture lowered the deeper var"
        );
    }

    #[test]
    fn snapshot_rollback_is_bit_identical() {
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let a = s.fresh(&mut t, 0, NumKind::Integer, sp());
        let b = s.fresh(&mut t, 1, NumKind::Any, sp());
        let before = s.clone();
        let snap = s.snapshot();
        // A trial: unions, bindings, kind narrowings, fresh vars.
        assert!(unify(&mut t, &mut s, a, b).is_ok());
        let c = s.fresh(&mut t, 2, NumKind::Float, sp());
        let f64_ = t.prim(Prim::F64);
        assert!(unify(&mut t, &mut s, c, f64_).is_ok());
        let int = t.prim(Prim::Int);
        assert!(unify(&mut t, &mut s, a, int).is_ok());
        assert_ne!(before, s, "the trial mutated the store");
        s.rollback(snap);
        assert_eq!(before, s, "rollback restores the arena bit-for-bit");
    }

    #[test]
    fn rows_unify_by_tag_sets_with_pointwise_payloads() {
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let int = t.prim(Prim::Int);
        let str_ = t.prim(Prim::Str);
        // Same set, different source order: hash-consing already made
        // them equal (canonical sorted order at interning).
        let a = t.row(
            vec![("B".into(), vec![str_]), ("A".into(), vec![int])],
            None,
        );
        let b = t.row(
            vec![("A".into(), vec![int]), ("B".into(), vec![str_])],
            None,
        );
        assert_eq!(a, b, "rows are sets: interning canonicalizes");
        // Shared tag, payload disagreement: mismatch (invariant).
        let c = t.row(vec![("A".into(), vec![str_])], None);
        let a1 = t.row(vec![("A".into(), vec![int])], None);
        assert!(unify(&mut t, &mut s, a1, c).is_err());
        // A payload var solves pointwise.
        let v = s.fresh(&mut t, 0, NumKind::Any, sp());
        let open_payload = t.row(vec![("A".into(), vec![v])], None);
        assert!(unify(&mut t, &mut s, open_payload, a1).is_ok());
        assert_eq!(s.shallow(&t, v), int);
    }

    #[test]
    fn row_tail_variable_absorbs_the_remainder() {
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let int = t.prim(Prim::Int);
        let v = s.fresh(&mut t, 0, NumKind::Any, sp());
        // {A | v} ~ {A, B(int)} ⇒ v := {B(int)}
        let open = t.row(vec![("A".into(), vec![])], Some(v));
        let closed = t.row(vec![("A".into(), vec![]), ("B".into(), vec![int])], None);
        assert!(unify(&mut t, &mut s, open, closed).is_ok());
        let expected = t.row(vec![("B".into(), vec![int])], None);
        let TyKind::Var(idx) = *t.kind(v) else {
            panic!("var")
        };
        assert_eq!(s.probe(idx), Some(expected));
        // A rigid tail never absorbs: {A | e} ~ {A, B} is a mismatch.
        let e = t.intern(TyKind::Rigid("e".into()));
        let open_rigid = t.row(vec![("A".into(), vec![])], Some(e));
        assert!(unify(&mut t, &mut s, open_rigid, closed).is_err());
        // But {A | e} ~ {A | e} holds.
        let open_rigid2 = t.row(vec![("A".into(), vec![])], Some(e));
        assert!(unify(&mut t, &mut s, open_rigid2, open_rigid).is_ok());
    }

    #[test]
    fn err_unions_require_equal_rows_inside_the_unifier() {
        // Width subtyping lives at check boundaries, NEVER here.
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let int = t.prim(Prim::Int);
        let small = t.row(vec![("A".into(), vec![])], None);
        let big = t.row(vec![("A".into(), vec![]), ("B".into(), vec![])], None);
        let ua = t.err_union(int, small);
        let ub = t.err_union(int, big);
        assert!(unify(&mut t, &mut s, ua, ub).is_err());
        let ua2 = t.err_union(int, small);
        assert!(unify(&mut t, &mut s, ua2, ua).is_ok());
    }

    #[test]
    fn row_unification_rolls_back_bit_identical() {
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let int = t.prim(Prim::Int);
        let v = s.fresh(&mut t, 0, NumKind::Any, sp());
        let before = s.clone();
        let snap = s.snapshot();
        let open = t.row(vec![("A".into(), vec![])], Some(v));
        let closed = t.row(vec![("A".into(), vec![]), ("B".into(), vec![int])], None);
        assert!(unify(&mut t, &mut s, open, closed).is_ok());
        assert_ne!(before, s);
        s.rollback(snap);
        assert_eq!(before, s, "row trials roll back cleanly");
    }

    #[test]
    fn join_prefers_the_non_diverging_branch() {
        let mut t = TypeTable::new();
        let mut s = VarStore::new();
        let never = t.never();
        let int = t.prim(Prim::Int);
        assert_eq!(join(&mut t, &mut s, never, int).unwrap(), int);
        assert_eq!(join(&mut t, &mut s, int, never).unwrap(), int);
    }
}
