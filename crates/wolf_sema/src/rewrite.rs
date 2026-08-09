//! The rewrite-constraint engine (s14, Target 2): associated-type
//! equalities as **textual canonicalization to a fixed point** —
//! Carbon's model (report 02 Done-well #3), adopted because it is
//! deterministic and terminating *by construction*: no general
//! equality solver, no undecidability cliff.
//!
//! A rule set maps associated-type names to their bindings for one
//! `Self` (an impl's `type Item = …` members). [`canonicalize`]
//! replaces every projection `Self.Name` in a type by its binding,
//! bottom-up, and repeats until nothing changes. Cyclic rule sets
//! (`type A = Self.B; type B = Self.A`) are **rejected up front** by
//! [`check_cycles`] — chasing them would never reach a fixed point,
//! so the impl is the error, not the traversal (E0513).
//!
//! Confluence: rules are keyed by name and applied exhaustively, so
//! the normal form is independent of rule order and of the order
//! projections are visited — a property test below permutes the rule
//! set and demands identical output.
//!
//! There is **no surface syntax** for rewrite constraints this sprint
//! (`where .Item = T` spellings are an s15/s17 spec-amendment
//! candidate); the engine serves impl-supplied bindings and the
//! checker's projection normalization only.

use std::collections::BTreeMap;

use crate::types::{TyId, TyKind, TypeTable};

/// A rule set for one `Self`: associated-type name → bound type. The
/// bound type may itself mention `Self.Other` projections (resolved
/// transitively by canonicalization).
pub type Rules = BTreeMap<String, TyId>;

/// A cycle among rewrite rules: the associated-type names on the
/// cycle, in a deterministic order starting from its smallest name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle(pub Vec<String>);

/// Reject cyclic rule sets before any canonicalization runs. `base`
/// is the `Self` the rules bind for (projections off *other* bases do
/// not participate — they are opaque here).
pub fn check_cycles(table: &TypeTable, base: TyId, rules: &Rules) -> Result<(), Cycle> {
    // DFS over the name → names-mentioned graph; colors: 0 white,
    // 1 on-stack, 2 done.
    let mut color: BTreeMap<&str, u8> = BTreeMap::new();
    for name in rules.keys() {
        if visit(table, base, rules, name, &mut color, &mut Vec::new()) {
            // Rebuild the cycle deterministically for the report.
            let mut stack = Vec::new();
            let mut c2: BTreeMap<&str, u8> = BTreeMap::new();
            if find_cycle(table, base, rules, name, &mut c2, &mut stack) {
                let start = stack
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, n)| n.as_str())
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                stack.rotate_left(start);
                return Err(Cycle(stack));
            }
        }
    }
    Ok(())
}

fn deps<'t>(table: &'t TypeTable, base: TyId, ty: TyId, out: &mut Vec<&'t str>) {
    match table.kind(ty) {
        TyKind::Proj(b, name) if *b == base => out.push(name.as_str()),
        TyKind::Proj(b, _) => deps(table, base, *b, out),
        TyKind::Wrapping(t)
        | TyKind::ErrUnion(t)
        | TyKind::Range(t)
        | TyKind::Ptr(t)
        | TyKind::Shared(t)
        | TyKind::Handle(t)
        | TyKind::Weak(t)
        | TyKind::Distinct(t) => deps(table, base, *t, out),
        TyKind::Tuple(ts) => {
            for t in ts {
                deps(table, base, *t, out);
            }
        }
        TyKind::Fn(ps, r) => {
            for t in ps {
                deps(table, base, *t, out);
            }
            deps(table, base, *r, out);
        }
        _ => {}
    }
}

fn visit<'t>(
    table: &'t TypeTable,
    base: TyId,
    rules: &'t Rules,
    name: &'t str,
    color: &mut BTreeMap<&'t str, u8>,
    stack: &mut Vec<String>,
) -> bool {
    match color.get(name) {
        Some(1) => return true,
        Some(2) => return false,
        _ => {}
    }
    color.insert(name, 1);
    stack.push(name.to_string());
    if let Some(&ty) = rules.get(name) {
        let mut ds = Vec::new();
        deps(table, base, ty, &mut ds);
        for d in ds {
            if rules.contains_key(d) && visit(table, base, rules, d, color, stack) {
                return true;
            }
        }
    }
    stack.pop();
    color.insert(name, 2);
    false
}

fn find_cycle<'t>(
    table: &'t TypeTable,
    base: TyId,
    rules: &'t Rules,
    name: &'t str,
    color: &mut BTreeMap<&'t str, u8>,
    stack: &mut Vec<String>,
) -> bool {
    match color.get(name) {
        Some(1) => {
            // Trim the stack to the cycle proper.
            if let Some(pos) = stack.iter().position(|n| n == name) {
                stack.drain(..pos);
            }
            return true;
        }
        Some(2) => return false,
        _ => {}
    }
    color.insert(name, 1);
    stack.push(name.to_string());
    if let Some(&ty) = rules.get(name) {
        let mut ds = Vec::new();
        deps(table, base, ty, &mut ds);
        for d in ds {
            if rules.contains_key(d) && find_cycle(table, base, rules, d, color, stack) {
                return true;
            }
        }
    }
    stack.pop();
    color.insert(name, 2);
    false
}

/// Canonicalize `ty` under `rules` for `base`: replace `base.Name`
/// projections by their bindings, bottom-up, to a fixed point. The
/// caller must have run [`check_cycles`] (a cyclic set would spin);
/// as a hard rail, iteration is bounded by the rule count and bails
/// to the last form if ever exceeded.
pub fn canonicalize(table: &mut TypeTable, base: TyId, rules: &Rules, ty: TyId) -> TyId {
    let mut cur = ty;
    // Each pass eliminates at least one layer of rule application, so
    // rules.len() + 1 passes always suffice for an acyclic set.
    for _ in 0..=rules.len() {
        let next = apply_once(table, base, rules, cur);
        if next == cur {
            return cur;
        }
        cur = next;
    }
    cur
}

fn apply_once(table: &mut TypeTable, base: TyId, rules: &Rules, ty: TyId) -> TyId {
    match table.kind(ty).clone() {
        TyKind::Proj(b, name) => {
            let nb = apply_once(table, base, rules, b);
            if nb == base
                && let Some(&bound) = rules.get(&name)
            {
                return bound;
            }
            table.intern(TyKind::Proj(nb, name))
        }
        TyKind::Wrapping(t) => {
            let s = apply_once(table, base, rules, t);
            table.intern(TyKind::Wrapping(s))
        }
        TyKind::ErrUnion(t) => {
            let s = apply_once(table, base, rules, t);
            table.intern(TyKind::ErrUnion(s))
        }
        TyKind::Range(t) => {
            let s = apply_once(table, base, rules, t);
            table.intern(TyKind::Range(s))
        }
        TyKind::Ptr(t) => {
            let s = apply_once(table, base, rules, t);
            table.intern(TyKind::Ptr(s))
        }
        TyKind::Shared(t) => {
            let s = apply_once(table, base, rules, t);
            table.intern(TyKind::Shared(s))
        }
        TyKind::Handle(t) => {
            let s = apply_once(table, base, rules, t);
            table.intern(TyKind::Handle(s))
        }
        TyKind::Weak(t) => {
            let s = apply_once(table, base, rules, t);
            table.intern(TyKind::Weak(s))
        }
        TyKind::Distinct(t) => {
            let s = apply_once(table, base, rules, t);
            table.intern(TyKind::Distinct(s))
        }
        TyKind::Tuple(ts) => {
            let s: Vec<TyId> = ts
                .into_iter()
                .map(|t| apply_once(table, base, rules, t))
                .collect();
            table.intern(TyKind::Tuple(s))
        }
        TyKind::Fn(ps, r) => {
            let sp: Vec<TyId> = ps
                .into_iter()
                .map(|t| apply_once(table, base, rules, t))
                .collect();
            let sr = apply_once(table, base, rules, r);
            table.intern(TyKind::Fn(sp, sr))
        }
        _ => ty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Prim, render};

    fn no_vars(_: u32) -> Result<TyId, &'static str> {
        Err("_")
    }

    fn setup() -> (TypeTable, TyId) {
        let mut t = TypeTable::new();
        let s = t.intern(TyKind::Rigid("Self".to_string()));
        (t, s)
    }

    #[test]
    fn single_rule_normalizes_to_fixed_point() {
        let (mut t, base) = setup();
        let int = t.prim(Prim::Int);
        let item = t.intern(TyKind::Proj(base, "Item".to_string()));
        let rules: Rules = [("Item".to_string(), int)].into();
        assert!(check_cycles(&t, base, &rules).is_ok());
        let got = canonicalize(&mut t, base, &rules, item);
        assert_eq!(got, int);
    }

    #[test]
    fn transitive_rules_resolve_through_each_other() {
        // Item = Self.Key, Key = str ⇒ Self.Item normalizes to str.
        let (mut t, base) = setup();
        let str_ = t.prim(Prim::Str);
        let key = t.intern(TyKind::Proj(base, "Key".to_string()));
        let rules: Rules = [("Item".to_string(), key), ("Key".to_string(), str_)].into();
        assert!(check_cycles(&t, base, &rules).is_ok());
        let item = t.intern(TyKind::Proj(base, "Item".to_string()));
        assert_eq!(canonicalize(&mut t, base, &rules, item), str_);
        // Nested occurrence too: fn(Self.Item) -> Self.Key.
        let item2 = t.intern(TyKind::Proj(base, "Item".to_string()));
        let key2 = t.intern(TyKind::Proj(base, "Key".to_string()));
        let f = t.intern(TyKind::Fn(vec![item2], key2));
        let nf = canonicalize(&mut t, base, &rules, f);
        assert_eq!(render(&t, nf, &no_vars), "fn(str) -> str");
    }

    #[test]
    fn cycles_are_rejected_not_chased() {
        let (mut t, base) = setup();
        let a = t.intern(TyKind::Proj(base, "A".to_string()));
        let b = t.intern(TyKind::Proj(base, "B".to_string()));
        let rules: Rules = [("A".to_string(), b), ("B".to_string(), a)].into();
        let err = check_cycles(&t, base, &rules).expect_err("cyclic");
        assert_eq!(err.0, vec!["A".to_string(), "B".to_string()]);
        // Self-cycle as well.
        let sa = t.intern(TyKind::Proj(base, "S".to_string()));
        let rules: Rules = [("S".to_string(), sa)].into();
        assert!(check_cycles(&t, base, &rules).is_err());
    }

    #[test]
    fn foreign_base_projections_are_opaque() {
        // T.Item (a different rigid) is untouched by Self's rules.
        let (mut t, base) = setup();
        let other = t.intern(TyKind::Rigid("T".to_string()));
        let int = t.prim(Prim::Int);
        let rules: Rules = [("Item".to_string(), int)].into();
        let proj = t.intern(TyKind::Proj(other, "Item".to_string()));
        assert_eq!(canonicalize(&mut t, base, &rules, proj), proj);
    }

    /// The confluence property: normalization is invariant under
    /// reordering of the rule set (insertion order shuffled every
    /// way for a 3-rule set — 6 permutations, one normal form).
    #[test]
    fn normalization_is_confluent_under_rule_reordering() {
        let (mut t, base) = setup();
        let int = t.prim(Prim::Int);
        let c = t.intern(TyKind::Proj(base, "C".to_string()));
        let b = t.intern(TyKind::Proj(base, "B".to_string()));
        let pairs: [(&str, TyId); 3] = [("A", b), ("B", c), ("C", int)];
        let perms: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let a = t.intern(TyKind::Proj(base, "A".to_string()));
        let tup = t.intern(TyKind::Tuple(vec![a, b, c]));
        let mut expected: Option<TyId> = None;
        for p in perms {
            let mut rules = Rules::new();
            for &i in &p {
                rules.insert(pairs[i].0.to_string(), pairs[i].1);
            }
            assert!(check_cycles(&t, base, &rules).is_ok());
            let got = canonicalize(&mut t, base, &rules, tup);
            match expected {
                None => expected = Some(got),
                Some(e) => assert_eq!(e, got, "rule order changed the normal form"),
            }
        }
        assert_eq!(
            render(&t, expected.unwrap(), &no_vars),
            "(int, int, int)",
            "everything bottomed out"
        );
    }
}
