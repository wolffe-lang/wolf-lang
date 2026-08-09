//! Match exhaustiveness and reachability (s17, Target 1).
//!
//! The usefulness algorithm over the pattern matrix — Maranget's
//! specialization scheme, the `rustc` `_match` lineage, not a bespoke
//! one. Two questions, one engine:
//!
//! - **Exhaustiveness**: is the all-wildcard row useful after every
//!   unguarded arm? If so, the recursion *constructs the witnesses* —
//!   concrete values no arm matches ("`Timeout` not covered",
//!   "not covered: `2`") — and the `match` is E0801.
//! - **Reachability**: is arm *i*'s row useful after arms `0..i`? If
//!   not, the arm is dead (E0802, warning) and the minimal covering
//!   prefix names the subsuming arm.
//!
//! Guarded arms contribute to neither side: a guard can be false, so
//! they prove nothing (the matrix holds unguarded rows only), and a
//! guarded arm's own reachability is still checked against the
//! unguarded rows before it.
//!
//! Constructor universes come from the *type* of each column
//! ([`ColTy`], built by the checker from zonked scrutinee types):
//! bools and enums split completely; sealed error rows are closed over
//! their tag set while open rows (`{…, ..}` / row variables) always
//! demand a rest arm; integers and strings are treated as infinite —
//! literals never cover them, and the missing-value witness is
//! computed ("not covered: `2`"); floats likewise (and float equality
//! patterns are a corpus-reviewed smell, not a coverage tool). Types
//! the engine cannot split ([`ColTy::Opaque`]) are covered only by a
//! binding or `_`.
//!
//! The engine is pure data — no spans, no diagnostics; the checker
//! owns rendering and the catalog codes.

/// One column's constructor universe, derived from the (zonked)
/// scrutinee type by the checker.
#[derive(Clone, Debug)]
pub(crate) enum ColTy {
    Bool,
    /// Any integer primitive (incl. `byte`, `wrapping[T]`). Literal
    /// constructors never complete it; the witness is a concrete
    /// uncovered value.
    Int,
    /// Float primitives: literal equality patterns are admitted but
    /// never complete the column.
    Float,
    Str,
    Tuple(Vec<ColTy>),
    /// A nominal enum: variant name → payload columns, in declaration
    /// order. The set is closed — full case analysis.
    Enum(Vec<(String, Vec<ColTy>)>),
    /// An error-row value: tag name → payload columns. `open` rows
    /// (`{…, ..}` tails or row variables) are never complete.
    Row {
        tags: Vec<(String, Vec<ColTy>)>,
        open: bool,
    },
    /// A type the engine cannot split (structs, functions, regions,
    /// archetypes, …): only a wildcard/binding covers it.
    Opaque,
}

/// One deconstructed pattern (or-alternatives preserved; bindings and
/// `_` are both [`Pat::Wild`] — a binding matches everything).
#[derive(Clone, Debug)]
pub(crate) enum Pat {
    Wild,
    Ctor { ctor: Ctor, args: Vec<Pat> },
    Or(Vec<Pat>),
}

/// A pattern-head constructor.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ctor {
    Bool(bool),
    Int(i128),
    /// Float literal, compared by its source text.
    Float(String),
    Str(String),
    Tuple,
    /// An enum variant or error-row tag, by name.
    Named(String),
}

impl Ctor {
    /// Payload column types of this constructor under `col`.
    fn fields(&self, col: &ColTy) -> Vec<ColTy> {
        match (self, col) {
            (Ctor::Tuple, ColTy::Tuple(fs)) => fs.clone(),
            (Ctor::Named(n), ColTy::Enum(vs)) => vs
                .iter()
                .find(|(v, _)| v == n)
                .map(|(_, fs)| fs.clone())
                .unwrap_or_default(),
            (Ctor::Named(n), ColTy::Row { tags, .. }) => tags
                .iter()
                .find(|(t, _)| t == n)
                .map(|(_, fs)| fs.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}

/// The complete constructor set of a column, or `None` when it cannot
/// be completed by constructors (infinite or opaque — a wildcard is
/// the only cover).
fn complete_set(col: &ColTy) -> Option<Vec<Ctor>> {
    match col {
        ColTy::Bool => Some(vec![Ctor::Bool(false), Ctor::Bool(true)]),
        ColTy::Tuple(_) => Some(vec![Ctor::Tuple]),
        ColTy::Enum(vs) => Some(vs.iter().map(|(n, _)| Ctor::Named(n.clone())).collect()),
        ColTy::Row { tags, open } if !open => {
            Some(tags.iter().map(|(n, _)| Ctor::Named(n.clone())).collect())
        }
        _ => None,
    }
}

/// The step rail: usefulness is worst-case exponential (or-patterns ×
/// columns); past the budget the engine goes silent rather than slow —
/// no E0801/E0802 is reported for that match.
const STEP_BUDGET: u32 = 100_000;

struct Engine {
    steps: u32,
    exhausted: bool,
}

impl Engine {
    fn tick(&mut self) -> bool {
        self.steps += 1;
        if self.steps > STEP_BUDGET {
            self.exhausted = true;
        }
        !self.exhausted
    }
}

/// Expand a row's head or-patterns into plain-headed rows.
fn expand_head(row: &[Pat]) -> Vec<Vec<Pat>> {
    match row.first() {
        Some(Pat::Or(alts)) => alts
            .iter()
            .flat_map(|a| {
                let mut r = row.to_vec();
                r[0] = a.clone();
                expand_head(&r)
            })
            .collect(),
        _ => vec![row.to_vec()],
    }
}

/// Specialize `matrix` by `ctor` (arity `arity`).
fn specialize(matrix: &[Vec<Pat>], ctor: &Ctor, arity: usize) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for row in matrix {
        for row in expand_head(row) {
            match &row[0] {
                Pat::Wild => {
                    let mut r: Vec<Pat> = std::iter::repeat_n(Pat::Wild, arity).collect();
                    r.extend_from_slice(&row[1..]);
                    out.push(r);
                }
                Pat::Ctor { ctor: c, args } if c == ctor => {
                    let mut r = args.clone();
                    r.resize(arity, Pat::Wild);
                    r.extend_from_slice(&row[1..]);
                    out.push(r);
                }
                _ => {}
            }
        }
    }
    out
}

/// The default matrix: rows whose head is a wildcard, heads dropped.
fn default_matrix(matrix: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for row in matrix {
        for row in expand_head(row) {
            if matches!(row[0], Pat::Wild) {
                out.push(row[1..].to_vec());
            }
        }
    }
    out
}

/// Every constructor appearing at the head of `matrix`.
fn head_ctors(matrix: &[Vec<Pat>]) -> Vec<Ctor> {
    let mut out: Vec<Ctor> = Vec::new();
    for row in matrix {
        for row in expand_head(row) {
            if let Pat::Ctor { ctor, .. } = &row[0]
                && !out.contains(ctor)
            {
                out.push(ctor.clone());
            }
        }
    }
    out
}

/// A witness head for a column no listed constructor completes: for
/// integers, the smallest non-negative value not used as a literal
/// ("not covered: `2`"); everything else is `_`.
fn synthetic_missing(col: &ColTy, used: &[Ctor]) -> Ctor {
    if matches!(col, ColTy::Int) {
        let mut n: i128 = 0;
        while used.contains(&Ctor::Int(n)) {
            n += 1;
        }
        return Ctor::Int(n);
    }
    // Rendered as `_` by the checker.
    Ctor::Named("_".to_string())
}

/// Is `q` useful after `matrix` (would it match anything the rows
/// above miss)? The reachability question.
pub(crate) fn is_useful(matrix: &[Vec<Pat>], q: &[Pat], tys: &[ColTy]) -> bool {
    let mut eng = Engine {
        steps: 0,
        exhausted: false,
    };
    let r = useful(&mut eng, matrix, q, tys);
    // On a blown budget stay silent: report the arm as reachable.
    r || eng.exhausted
}

fn useful(eng: &mut Engine, matrix: &[Vec<Pat>], q: &[Pat], tys: &[ColTy]) -> bool {
    if !eng.tick() {
        return true;
    }
    if tys.is_empty() {
        return matrix.is_empty();
    }
    match &q[0] {
        Pat::Or(alts) => alts.iter().any(|a| {
            let mut r = q.to_vec();
            r[0] = a.clone();
            useful(eng, matrix, &r, tys)
        }),
        Pat::Ctor { ctor, .. } => {
            let fields = ctor.fields(&tys[0]);
            let arity = fields.len();
            let spec = specialize(matrix, ctor, arity);
            let mut sq = match &q[0] {
                Pat::Ctor { args, .. } => {
                    let mut a = args.clone();
                    a.resize(arity, Pat::Wild);
                    a
                }
                _ => unreachable!(),
            };
            sq.extend_from_slice(&q[1..]);
            let mut stys = fields;
            stys.extend_from_slice(&tys[1..]);
            useful(eng, &spec, &sq, &stys)
        }
        Pat::Wild => {
            let used = head_ctors(matrix);
            if let Some(all) = complete_set(&tys[0])
                && all.iter().all(|c| used.contains(c))
            {
                // The column is fully split: q must be useful under
                // some constructor.
                return all.iter().any(|c| {
                    let fields = c.fields(&tys[0]);
                    let arity = fields.len();
                    let spec = specialize(matrix, c, arity);
                    let mut sq: Vec<Pat> = std::iter::repeat_n(Pat::Wild, arity).collect();
                    sq.extend_from_slice(&q[1..]);
                    let mut stys = fields;
                    stys.extend_from_slice(&tys[1..]);
                    useful(eng, &spec, &sq, &stys)
                });
            }
            useful(eng, &default_matrix(matrix), &q[1..], &tys[1..])
        }
    }
}

/// Witnesses of non-exhaustiveness: up to `cap` concrete value shapes
/// the (unguarded) rows of `matrix` never match. Empty ⇒ exhaustive.
pub(crate) fn witnesses(matrix: &[Vec<Pat>], tys: &[ColTy], cap: usize) -> Vec<Pat> {
    let mut eng = Engine {
        steps: 0,
        exhausted: false,
    };
    let ws = witness_rows(&mut eng, matrix, tys, cap);
    if eng.exhausted {
        // Silence over false alarms on a blown budget.
        return Vec::new();
    }
    ws.into_iter().map(|mut w| w.remove(0)).collect()
}

fn witness_rows(eng: &mut Engine, matrix: &[Vec<Pat>], tys: &[ColTy], cap: usize) -> Vec<Vec<Pat>> {
    if !eng.tick() || cap == 0 {
        return Vec::new();
    }
    if tys.is_empty() {
        return if matrix.is_empty() {
            vec![Vec::new()]
        } else {
            Vec::new()
        };
    }
    let used = head_ctors(matrix);
    let complete = complete_set(&tys[0]);
    let fully_split = complete
        .as_ref()
        .is_some_and(|all| all.iter().all(|c| used.contains(c)));
    let mut out: Vec<Vec<Pat>> = Vec::new();
    if fully_split {
        for c in complete.expect("fully_split implies complete") {
            if out.len() >= cap {
                break;
            }
            let fields = c.fields(&tys[0]);
            let arity = fields.len();
            let spec = specialize(matrix, &c, arity);
            let mut stys = fields;
            stys.extend_from_slice(&tys[1..]);
            for w in witness_rows(eng, &spec, &stys, cap - out.len()) {
                let (head, rest) = w.split_at(arity);
                let mut row = vec![Pat::Ctor {
                    ctor: c.clone(),
                    args: head.to_vec(),
                }];
                row.extend_from_slice(rest);
                out.push(row);
            }
        }
        return out;
    }
    // Not fully split: the default matrix decides, and the missing
    // heads are the witnesses.
    let sub = witness_rows(eng, &default_matrix(matrix), &tys[1..], cap);
    if sub.is_empty() {
        return Vec::new();
    }
    let missing: Vec<Ctor> = match &complete {
        Some(all) => all.iter().filter(|c| !used.contains(c)).cloned().collect(),
        None => vec![synthetic_missing(&tys[0], &used)],
    };
    for w in &sub {
        for m in &missing {
            if out.len() >= cap {
                return out;
            }
            let arity = m.fields(&tys[0]).len();
            let mut row = vec![Pat::Ctor {
                ctor: m.clone(),
                args: std::iter::repeat_n(Pat::Wild, arity).collect(),
            }];
            row.extend_from_slice(w);
            out.push(row);
        }
    }
    out
}

/// Render a witness pattern as source would spell it.
pub(crate) fn render_pat(p: &Pat) -> String {
    match p {
        Pat::Wild => "_".to_string(),
        Pat::Or(alts) => alts.iter().map(render_pat).collect::<Vec<_>>().join(" | "),
        Pat::Ctor { ctor, args } => match ctor {
            Ctor::Bool(b) => b.to_string(),
            Ctor::Int(n) => n.to_string(),
            Ctor::Float(s) => s.clone(),
            Ctor::Str(s) => format!("\"{s}\""),
            Ctor::Tuple => {
                let parts: Vec<String> = args.iter().map(render_pat).collect();
                format!("({})", parts.join(", "))
            }
            Ctor::Named(n) => {
                if args.is_empty() {
                    n.clone()
                } else {
                    let parts: Vec<String> = args.iter().map(render_pat).collect();
                    format!("{n}({})", parts.join(", "))
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctor(c: Ctor, args: Vec<Pat>) -> Pat {
        Pat::Ctor { ctor: c, args }
    }

    #[test]
    fn bool_full_split_is_exhaustive() {
        let matrix = vec![
            vec![ctor(Ctor::Bool(true), vec![])],
            vec![ctor(Ctor::Bool(false), vec![])],
        ];
        assert!(witnesses(&matrix, &[ColTy::Bool], 3).is_empty());
    }

    #[test]
    fn bool_missing_arm_witnesses_false() {
        let matrix = vec![vec![ctor(Ctor::Bool(true), vec![])]];
        let ws = witnesses(&matrix, &[ColTy::Bool], 3);
        assert_eq!(ws.len(), 1);
        assert_eq!(render_pat(&ws[0]), "false");
    }

    #[test]
    fn int_literals_never_complete() {
        let matrix = vec![
            vec![ctor(Ctor::Int(0), vec![])],
            vec![ctor(Ctor::Int(1), vec![])],
        ];
        let ws = witnesses(&matrix, &[ColTy::Int], 3);
        assert_eq!(ws.len(), 1);
        assert_eq!(render_pat(&ws[0]), "2");
    }

    #[test]
    fn enum_missing_variants_named() {
        let col = ColTy::Enum(vec![
            ("Red".into(), vec![]),
            ("Green".into(), vec![]),
            ("Rgb".into(), vec![ColTy::Int, ColTy::Int]),
        ]);
        let matrix = vec![vec![ctor(Ctor::Named("Red".into()), vec![])]];
        let ws = witnesses(&matrix, &[col], 3);
        let rendered: Vec<String> = ws.iter().map(render_pat).collect();
        assert_eq!(rendered, vec!["Green", "Rgb(_, _)"]);
    }

    #[test]
    fn wildcard_closes_everything() {
        let col = ColTy::Enum(vec![("A".into(), vec![]), ("B".into(), vec![])]);
        let matrix = vec![vec![Pat::Wild]];
        assert!(witnesses(&matrix, &[col], 3).is_empty());
    }

    #[test]
    fn open_row_needs_rest() {
        let col = ColTy::Row {
            tags: vec![("Io".into(), vec![])],
            open: true,
        };
        let matrix = vec![vec![ctor(Ctor::Named("Io".into()), vec![])]];
        let ws = witnesses(&matrix, &[col], 3);
        assert_eq!(ws.len(), 1);
        assert_eq!(render_pat(&ws[0]), "_");
    }

    #[test]
    fn sealed_row_closes() {
        let col = ColTy::Row {
            tags: vec![("Io".into(), vec![ColTy::Int]), ("Timeout".into(), vec![])],
            open: false,
        };
        let matrix = vec![
            vec![ctor(Ctor::Named("Io".into()), vec![Pat::Wild])],
            vec![ctor(Ctor::Named("Timeout".into()), vec![])],
        ];
        assert!(witnesses(&matrix, &[col], 3).is_empty());
    }

    #[test]
    fn redundant_arm_is_useless() {
        let matrix = vec![vec![Pat::Wild]];
        let q = vec![ctor(Ctor::Int(3), vec![])];
        assert!(!is_useful(&matrix, &q, &[ColTy::Int]));
    }

    #[test]
    fn or_pattern_expands() {
        let col = ColTy::Enum(vec![("A".into(), vec![]), ("B".into(), vec![])]);
        let matrix = vec![vec![Pat::Or(vec![
            ctor(Ctor::Named("A".into()), vec![]),
            ctor(Ctor::Named("B".into()), vec![]),
        ])]];
        assert!(witnesses(&matrix, std::slice::from_ref(&col), 3).is_empty());
        // And each alternative is reachable on its own…
        let q = vec![ctor(Ctor::Named("B".into()), vec![])];
        let prior = vec![vec![ctor(Ctor::Named("A".into()), vec![])]];
        assert!(is_useful(&prior, &q, &[col]));
    }

    #[test]
    fn tuple_nesting() {
        let col = ColTy::Tuple(vec![ColTy::Bool, ColTy::Bool]);
        let matrix = vec![
            vec![ctor(
                Ctor::Tuple,
                vec![ctor(Ctor::Bool(true), vec![]), Pat::Wild],
            )],
            vec![ctor(
                Ctor::Tuple,
                vec![
                    ctor(Ctor::Bool(false), vec![]),
                    ctor(Ctor::Bool(true), vec![]),
                ],
            )],
        ];
        let ws = witnesses(&matrix, &[col], 3);
        assert_eq!(ws.len(), 1);
        assert_eq!(render_pat(&ws[0]), "(false, false)");
    }
}
