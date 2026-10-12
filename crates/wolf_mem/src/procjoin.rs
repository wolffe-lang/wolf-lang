//! s233 (wolf-lang#678's helper half, `[conc.proc.arg]`): the must-join
//! check — a function that leaves a proc running with bytes its caller
//! owns.
//!
//! s229 holds a `spawn proc` argument to the frame rule: bytes in a
//! region local to the spawning frame are E1010 at the argument. A
//! PARAMETER's bytes pass there, because they outlive the frame — but
//! the proc may outlive the frame too. `fn helper(s: str) { spawn proc
//! child(s) }` called from `region pass { helper(s) }` returns while
//! `child` runs, the block frees `s`'s bytes, and native and release
//! print the chunk the next region reused. What separates that helper
//! from lobo's `budget.run_small`, which passes request-lived strings
//! the same way, is that `run_small` waits for its proc on every path
//! before it returns.
//!
//! The rule, in two halves:
//!
//! 1. **Must-join.** A spawn whose arguments can name bytes the frame
//!    does not own — a parameter's ([`Mark::Param`]) or its caller's
//!    current region's ([`Mark::Ambient`]) — raises an obligation that
//!    must be met on every path out of the frame: `p.join()`, an exit
//!    received from `p.monitor()`, or `p` handed to a function that
//!    meets it on every path. A kill is not a join (wolf-lang#683: a
//!    killed proc runs on to its next park). A forward dataflow over
//!    the check CFG decides "every path", carrying integer and bool
//!    literals across branches, so `cls = 4` in a `select`'s timeout
//!    arm followed by `if cls == 4 { end_body(w) }` reads path by path.
//! 2. **Sinks.** A function that misses an obligation LEAVES A PROC
//!    RUNNING with those parameters (or its caller's region): a
//!    per-callee summary, a least fixpoint over the package's calls
//!    and spawns. Every call to such a function is held to
//!    `[conc.proc.arg]`'s frame rule at the argument (or, for the
//!    caller's region, at the call): bytes in a region block, a region
//!    value made in the frame or the spawning proc's own region are
//!    E1010; a parameter's or the caller region's bytes make the
//!    calling function a sink in turn.
//!
//! What no summary can name is refused where it is made: a closure (or
//! nested fn) that leaves a proc running with a value that can name
//! bytes, and a sink taken as a fn value — a call through the value
//! cannot be asked.
//!
//! Every new verdict is compile-time and shared by the checked,
//! native and release machines; nothing is added to an accepted
//! program.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use wolf_diag::{Diagnostic, codes};
use wolf_span::Span;

use crate::cfg::{Cfg, Stmt};
use crate::place::Base;

/// Whose bytes a proc left running may read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Mark {
    /// The declared parameter at this index (the receiver is 0).
    Param(u32),
    /// The function's caller's current region (D12's ambient).
    Ambient,
}

/// A comparison an `if` branches on, with a literal on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cmp {
    fn holds(self, a: i128, b: i128) -> bool {
        match self {
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
            Cmp::Gt => a > b,
            Cmp::Ge => a >= b,
        }
    }
}

/// One event in the statement stream, at a block and a statement
/// index: it happens after the statement before that index.
#[derive(Debug, Clone)]
pub(crate) enum Ev {
    /// A spawn raised obligation `ob`.
    Gen(u32),
    /// `local` now holds the handle (or the monitor) of `ob`.
    Bind(u32, u32),
    /// `dst` now holds whatever `src` holds (a copy of a handle, or
    /// `dst = src.monitor()`).
    Alias(u32, u32),
    /// `local.join()` returned: what it holds is met.
    Join(u32),
    /// `local` was handed to `callee`'s parameter `param`, which meets
    /// what it holds when `callee` joins that parameter on every path.
    JoinVia {
        callee: Span,
        param: u32,
        local: u32,
    },
    /// An exit was received from `local` (a monitor channel).
    RecvExit(u32),
    /// `local` now holds this integer (a `bool` is 0 or 1).
    Const(u32, i128),
}

/// An `if` whose condition is `local <cmp> lit` (or a bare `bool`
/// local): the edge to `then_b` is taken only when it holds, the edge
/// to `else_b` only when it does not.
#[derive(Debug, Clone)]
pub(crate) struct Branch {
    pub block: u32,
    pub then_b: u32,
    pub else_b: u32,
    pub local: u32,
    pub cmp: Cmp,
    pub lit: i128,
}

/// What E1010 says about a frame-local region the bytes live in.
#[derive(Debug, Clone)]
pub(crate) struct Hit {
    pub region: String,
    pub alloc: Span,
    pub intro: Option<(String, Span)>,
    pub is_proc: bool,
}

/// One `spawn proc` that raised an obligation.
#[derive(Debug, Clone)]
pub(crate) struct SpawnOb {
    pub span: Span,
    pub callee: String,
    pub marks: BTreeSet<Mark>,
    /// `None`: met by the function's exit. `Some(b)`: the spawn is in
    /// a closure or a nested fn whose exit is block `b`, and the
    /// obligation is that frame's (nothing summarizes it).
    pub inner_exit: Option<u32>,
    /// For an inner frame: the first argument that can name bytes and
    /// is not a literal (span and text), the refusal's anchor.
    pub inner_arg: Option<(Span, String)>,
}

/// One argument of a call to a declared callee (or of a spawn).
#[derive(Debug, Clone)]
pub(crate) struct ArgEdge {
    /// The callee's declared parameter index (the receiver is 0).
    pub param: u32,
    pub span: Span,
    pub text: String,
    pub marks: BTreeSet<Mark>,
    pub hit: Option<Hit>,
    /// The argument can name bytes and is not a literal (read only
    /// for a call inside an inner frame, whose parameters carry no
    /// pseudo-site).
    pub opaque: bool,
}

/// A call (or a spawn, `spawn: true`) of a declared function.
#[derive(Debug, Clone)]
pub(crate) struct CallEdge {
    pub span: Span,
    pub callee: Span,
    pub name: String,
    pub args: Vec<ArgEdge>,
    /// The call's ambient region is this function's caller's region.
    pub ambient_mark: bool,
    /// The call's ambient region is frame-local (a region block, `in r`,
    /// the proc's own region).
    pub ambient_hit: Option<Hit>,
    pub spawn: bool,
    /// Inside a closure or a nested fn.
    pub inner: bool,
}

/// What one lowered body tells the must-join check.
#[derive(Debug, Clone, Default)]
pub struct BodyProcs {
    /// The function's NAME span (a `CallSig::decl_span`), when it is a
    /// declared fn.
    pub(crate) key: Option<Span>,
    /// For a method of a trait impl: the trait method's NAME span, the
    /// `decl_span` of a call dispatched through a bound — such a call
    /// answers for every impl of it.
    pub(crate) trait_key: Option<Span>,
    pub(crate) name: String,
    /// `Proc`-typed parameters: (declared index, local).
    pub(crate) proc_params: Vec<(u32, u32)>,
    /// (block, statement index, the inner frame it happened in — a
    /// closure's or nested fn's exit block, `None` for the function's
    /// own — and the event).
    pub(crate) events: Vec<(u32, u32, Option<u32>, Ev)>,
    pub(crate) branches: Vec<Branch>,
    pub(crate) spawns: Vec<SpawnOb>,
    pub(crate) calls: Vec<CallEdge>,
    /// A declared fn named in value position: (span, its NAME span,
    /// its rendered name).
    pub(crate) values: Vec<(Span, Span, String)>,
}

// ---------------------------------------------------------- dataflow --

/// One path's state: pending obligations, those that can no longer be
/// met (a second instance raised while the first was pending), which
/// local holds which obligation, and the literal each local holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
struct St {
    pending: BTreeSet<u32>,
    stuck: BTreeSet<u32>,
    holds: BTreeMap<u32, u32>,
    consts: BTreeMap<u32, i128>,
}

impl St {
    /// `self` is at least as conservative as `o`: every obligation `o`
    /// leaves open, `self` leaves open, and `self` knows no more.
    fn subsumes(&self, o: &St) -> bool {
        self.pending.is_superset(&o.pending)
            && self.stuck.is_superset(&o.stuck)
            && self.holds.iter().all(|(k, v)| o.holds.get(k) == Some(v))
            && self.consts.iter().all(|(k, v)| o.consts.get(k) == Some(v))
    }

    fn join_all(states: &[St]) -> St {
        let mut out = St::default();
        let Some(first) = states.first() else {
            return out;
        };
        out.holds = first.holds.clone();
        out.consts = first.consts.clone();
        for s in states {
            out.pending.extend(s.pending.iter().copied());
            out.stuck.extend(s.stuck.iter().copied());
            out.holds.retain(|k, v| s.holds.get(k) == Some(v));
            out.consts.retain(|k, v| s.consts.get(k) == Some(v));
        }
        out
    }

    fn forget(&mut self, local: u32) {
        self.holds.remove(&local);
        self.consts.remove(&local);
    }
}

const CAP: usize = 48;
const BUDGET: usize = 200_000;

/// The obligations left open at their deadlines: for each spawn
/// obligation, whether some feasible path reaches its deadline with it
/// open; for each `Proc` parameter, whether some path reaches the exit
/// with it open.
struct Outcome {
    spawn_open: Vec<bool>,
    param_open: Vec<bool>,
}

fn run(cfg: &Cfg, bp: &BodyProcs, joins: &HashSet<(Span, u32)>) -> Outcome {
    let n_spawn = bp.spawns.len() as u32;
    let nb = cfg.blocks.len();
    // Events per block, in stream order.
    let mut evs: Vec<Vec<(u32, Option<u32>, &Ev)>> = vec![Vec::new(); nb];
    for (b, i, f, e) in &bp.events {
        if let Some(v) = evs.get_mut(*b as usize) {
            v.push((*i, *f, e));
        }
    }
    // The frame each obligation belongs to: a join only meets an
    // obligation of its own frame (a closure that joins the outer
    // proc may never be called; it is walked where it is written).
    let frame_of =
        |ob: u32| -> Option<u32> { bp.spawns.get(ob as usize).and_then(|s| s.inner_exit) };
    let mut br: HashMap<(u32, u32), Vec<&Branch>> = HashMap::new();
    for b in &bp.branches {
        br.entry((b.block, b.then_b)).or_default().push(b);
        br.entry((b.block, b.else_b)).or_default().push(b);
    }
    // Deadlines: an inner frame's exit block, per obligation.
    let mut inner_at: HashMap<u32, Vec<u32>> = HashMap::new();
    for (k, s) in bp.spawns.iter().enumerate() {
        if let Some(x) = s.inner_exit {
            inner_at.entry(x).or_default().push(k as u32);
        }
    }
    let mut out = Outcome {
        spawn_open: vec![false; bp.spawns.len()],
        param_open: vec![false; bp.proc_params.len()],
    };
    let mut init = St::default();
    for (k, &(_, local)) in bp.proc_params.iter().enumerate() {
        let ob = n_spawn + k as u32;
        init.pending.insert(ob);
        init.holds.insert(local, ob);
    }
    let mut ins: Vec<Vec<St>> = vec![Vec::new(); nb];
    let entry = cfg.entry.0 as usize;
    ins[entry].push(init);
    let mut work: Vec<usize> = vec![entry];
    let mut queued = vec![false; nb];
    queued[entry] = true;
    let mut steps = 0usize;
    let exit = cfg.exit.0 as usize;
    while let Some(b) = work.pop() {
        queued[b] = false;
        steps += 1;
        if steps > BUDGET {
            // Out of budget: every obligation is open (conservative).
            out.spawn_open.iter_mut().for_each(|x| *x = true);
            out.param_open.iter_mut().for_each(|x| *x = true);
            return out;
        }
        let states = ins[b].clone();
        // Deadlines at this block's entry.
        if let Some(obs) = inner_at.get(&(b as u32)) {
            for s in &states {
                for &ob in obs {
                    if s.pending.contains(&ob) || s.stuck.contains(&ob) {
                        out.spawn_open[ob as usize] = true;
                    }
                }
            }
        }
        if b == exit {
            for s in &states {
                for &ob in s.pending.iter().chain(s.stuck.iter()) {
                    if ob < n_spawn {
                        if bp.spawns[ob as usize].inner_exit.is_none() {
                            out.spawn_open[ob as usize] = true;
                        }
                    } else if let Some(x) = out.param_open.get_mut((ob - n_spawn) as usize) {
                        *x = true;
                    }
                }
            }
        }
        let block = &cfg.blocks[b];
        let mut outs: Vec<St> = Vec::new();
        for s0 in states {
            let mut s = s0;
            let mut ei = 0usize;
            let list = &evs[b];
            for (si, stmt) in block.stmts.iter().enumerate() {
                while ei < list.len() && list[ei].0 as usize <= si {
                    apply(&mut s, list[ei].1, list[ei].2, joins, &frame_of);
                    ei += 1;
                }
                stmt_writes(cfg, stmt, &mut s);
            }
            while ei < list.len() {
                apply(&mut s, list[ei].1, list[ei].2, joins, &frame_of);
                ei += 1;
            }
            outs.push(s);
        }
        for &succ in &block.succs {
            let si = succ.0 as usize;
            let conds = br.get(&(b as u32, succ.0));
            let mut grew = false;
            for s in &outs {
                if let Some(cs) = conds
                    && !cs.iter().all(|c| feasible(s, c, succ.0))
                {
                    continue;
                }
                if add(&mut ins[si], s.clone()) {
                    grew = true;
                }
            }
            if grew && !queued[si] {
                queued[si] = true;
                work.push(si);
            }
        }
    }
    out
}

/// May the edge into `succ` be taken in state `s`?
fn feasible(s: &St, c: &Branch, succ: u32) -> bool {
    let Some(&v) = s.consts.get(&c.local) else {
        return true;
    };
    let holds = c.cmp.holds(v, c.lit);
    // An `if` with no else arm whose then block IS its join cannot
    // happen (blocks are fresh), so the two targets differ.
    if succ == c.then_b { holds } else { !holds }
}

/// Add `s` to a block's entry set; `true` when the set changed.
fn add(set: &mut Vec<St>, s: St) -> bool {
    if set.iter().any(|t| t.subsumes(&s)) {
        return false;
    }
    set.retain(|t| !s.subsumes(t));
    set.push(s);
    if set.len() > CAP {
        let j = St::join_all(set);
        set.clear();
        set.push(j);
    }
    true
}

fn apply(
    s: &mut St,
    frame: Option<u32>,
    e: &Ev,
    joins: &HashSet<(Span, u32)>,
    frame_of: &dyn Fn(u32) -> Option<u32>,
) {
    match e {
        Ev::Gen(ob) => {
            if s.pending.contains(ob) {
                s.stuck.insert(*ob);
            }
            s.pending.insert(*ob);
        }
        Ev::Bind(l, ob) => {
            s.holds.insert(*l, *ob);
        }
        Ev::Alias(d, src) => match s.holds.get(src).copied() {
            Some(ob) => {
                s.holds.insert(*d, ob);
            }
            None => {
                s.holds.remove(d);
            }
        },
        Ev::Join(l) | Ev::RecvExit(l) => {
            if let Some(ob) = s.holds.get(l).copied()
                && frame_of(ob) == frame
            {
                s.pending.remove(&ob);
            }
        }
        Ev::JoinVia {
            callee,
            param,
            local,
        } => {
            if joins.contains(&(*callee, *param))
                && let Some(ob) = s.holds.get(local).copied()
                && frame_of(ob) == frame
            {
                s.pending.remove(&ob);
            }
        }
        Ev::Const(l, v) => {
            s.consts.insert(*l, *v);
        }
    }
}

/// A statement that writes a whole local forgets what it held.
fn stmt_writes(cfg: &Cfg, stmt: &Stmt, s: &mut St) {
    let whole = |p: crate::place::PlaceId| -> Option<u32> {
        let pl = cfg.places.get(p);
        match pl.base {
            Base::Local(l) => Some(l),
            Base::Global(..) => None,
        }
    };
    match stmt {
        Stmt::Init { place, .. }
        | Stmt::Move { place, .. }
        | Stmt::Mutate { place, .. }
        | Stmt::Uninit { place, .. } => {
            if let Some(l) = whole(*place) {
                s.forget(l);
            }
        }
        Stmt::Call(c) => {
            for &(p, _) in c.mut_args.iter().chain(c.take_args.iter()) {
                if let Some(l) = whole(p) {
                    s.forget(l);
                }
            }
        }
        _ => {}
    }
}

// ----------------------------------------------------------- solving --

/// Why a function is a sink for a mark: its own spawn, or a call.
#[derive(Debug, Clone)]
enum Why {
    Spawn { span: Span, callee: String },
    Call { span: Span, callee: String },
}

/// Solve the package: who joins which `Proc` parameter, who leaves a
/// proc running with what, and every E1010 that follows.
pub(crate) fn solve(bodies: &[(&Cfg, &BodyProcs)]) -> Vec<Vec<Diagnostic>> {
    // A callee's NAME span -> the bodies that answer for it: its own,
    // and for a trait method every impl's (a call through a bound may
    // reach any of them).
    let mut by_key: HashMap<Span, Vec<usize>> = HashMap::new();
    for (i, (_, bp)) in bodies.iter().enumerate() {
        for k in [bp.key, bp.trait_key].into_iter().flatten() {
            let v = by_key.entry(k).or_default();
            if !v.contains(&i) {
                v.push(i);
            }
        }
    }
    let callee_marks = |marks: &[BTreeMap<Mark, Why>], key: &Span| -> BTreeMap<Mark, Why> {
        let mut out: BTreeMap<Mark, Why> = BTreeMap::new();
        for &ci in by_key.get(key).map(Vec::as_slice).unwrap_or(&[]) {
            for (m, w) in &marks[ci] {
                out.entry(*m).or_insert_with(|| w.clone());
            }
        }
        out
    };
    // 1. Who joins which parameter: a least fixpoint (nothing joins
    //    until shown to).
    let mut joins: HashSet<(Span, u32)> = HashSet::new();
    loop {
        let mut grew = false;
        for (cfg, bp) in bodies {
            let Some(k) = bp.key else { continue };
            if bp.proc_params.is_empty() {
                continue;
            }
            let o = run(cfg, bp, &joins);
            for (j, &(idx, _)) in bp.proc_params.iter().enumerate() {
                if !o.param_open[j] && joins.insert((k, idx)) {
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    // 2. Each body's open spawn obligations.
    let outcomes: Vec<Outcome> = bodies
        .iter()
        .map(|(cfg, bp)| {
            if bp.spawns.is_empty() {
                Outcome {
                    spawn_open: Vec::new(),
                    param_open: vec![false; bp.proc_params.len()],
                }
            } else {
                run(cfg, bp, &joins)
            }
        })
        .collect();
    // 3. Sinks: a least fixpoint over calls and spawns.
    let mut marks: Vec<BTreeMap<Mark, Why>> = vec![BTreeMap::new(); bodies.len()];
    for (i, (_, bp)) in bodies.iter().enumerate() {
        for (k, s) in bp.spawns.iter().enumerate() {
            if s.inner_exit.is_none() && outcomes[i].spawn_open[k] {
                for &m in &s.marks {
                    marks[i].entry(m).or_insert(Why::Spawn {
                        span: s.span,
                        callee: s.callee.clone(),
                    });
                }
            }
        }
    }
    loop {
        let mut grew = false;
        for (i, (_, bp)) in bodies.iter().enumerate() {
            for c in &bp.calls {
                let cm: Vec<Mark> = callee_marks(&marks, &c.callee).into_keys().collect();
                let mut add_marks: Vec<Mark> = Vec::new();
                for m in cm {
                    match m {
                        Mark::Param(j) => {
                            for a in c.args.iter().filter(|a| a.param == j) {
                                add_marks.extend(a.marks.iter().copied());
                            }
                        }
                        Mark::Ambient => {
                            if c.ambient_mark && !c.spawn {
                                add_marks.push(Mark::Ambient);
                            }
                        }
                    }
                }
                for m in add_marks {
                    if let std::collections::btree_map::Entry::Vacant(v) = marks[i].entry(m) {
                        v.insert(Why::Call {
                            span: c.span,
                            callee: c.name.clone(),
                        });
                        grew = true;
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    // 4. The refusals.
    let mut out: Vec<Vec<Diagnostic>> = vec![Vec::new(); bodies.len()];
    for (i, (_, bp)) in bodies.iter().enumerate() {
        let mut told: HashSet<Span> = HashSet::new();
        for c in &bp.calls {
            if c.spawn {
                continue;
            }
            let cm = callee_marks(&marks, &c.callee);
            for (&m, why) in &cm {
                match m {
                    Mark::Param(j) => {
                        for a in c.args.iter().filter(|a| a.param == j) {
                            if !told.insert(a.span) {
                                continue;
                            }
                            if let Some(h) = &a.hit {
                                out[i].push(refuse_arg(c, a, h, why));
                            } else if c.inner && a.opaque && a.marks.is_empty() {
                                out[i].push(refuse_inner_arg(c, a, why));
                            }
                        }
                    }
                    Mark::Ambient => {
                        if let Some(h) = &c.ambient_hit
                            && told.insert(c.span)
                        {
                            out[i].push(refuse_ambient(c, h, why));
                        }
                    }
                }
            }
        }
        for (k, s) in bp.spawns.iter().enumerate() {
            if s.inner_exit.is_some()
                && outcomes[i].spawn_open[k]
                && let Some((span, text)) = &s.inner_arg
            {
                out[i].push(refuse_inner_spawn(s, *span, text));
            }
        }
        for (span, key, name) in &bp.values {
            if let Some((_, why)) = callee_marks(&marks, key).iter().next() {
                out[i].push(refuse_value(*span, name, why));
            }
        }
    }
    out
}

fn why_label(why: &Why) -> (Span, String) {
    match why {
        Why::Spawn { span, callee } => (
            *span,
            format!(
                "`{callee}` is spawned here, and not every path out of the function joins it \
                 before it returns"
            ),
        ),
        Why::Call { span, callee } => (
            *span,
            format!("this calls `{callee}`, which may leave a proc running with what it is handed"),
        ),
    }
}

const NOTE: &str = "a function that spawns a proc with bytes its caller owns must join that \
     proc on every path before it returns — `p.join()`, an exit received from `p.monitor()`, \
     or `p` handed to a function that does so; a kill is not a join, the proc may run on until \
     it is joined (wolf-lang#683) — or the bytes must outlive the calling frame \
     [conc.proc.arg]. Here the function may return with the proc still running, so the bytes \
     it was handed must outlive this frame [mem.region.escape]. Join the proc in the callee \
     on every path, or build the value where it outlives this frame — outside the region \
     block; a literal or a scalar argument owns no region storage.";

fn with_hit(mut d: Diagnostic, h: &Hit) -> Diagnostic {
    d = d.with_secondary(h.alloc, format!("allocated here, into {}", h.region));
    if let Some((name, intro)) = &h.intro {
        d = d.with_secondary(
            *intro,
            if h.is_proc {
                format!(
                    "`{name}` is a `spawn proc` entry: a proc owns its regions and they \
                     bulk-free at its exit ([conc.proc.kill])"
                )
            } else {
                format!(
                    "region `{name}` is created here and freed with its scope — everything in \
                     it is freed wholesale"
                )
            },
        );
    }
    d
}

fn dies(h: &Hit) -> &'static str {
    if h.is_proc {
        "the spawning proc's own region, which is freed when that proc exits"
    } else {
        "which is freed with its scope"
    }
}

fn shown(text: &str) -> String {
    if text.len() <= 40 && !text.contains('\n') {
        format!("`{text}`")
    } else {
        "this argument".to_string()
    }
}

fn refuse_arg(c: &CallEdge, a: &ArgEdge, h: &Hit, why: &Why) -> Diagnostic {
    let (ws, wl) = why_label(why);
    let d = Diagnostic::error(
        codes::E1010,
        a.span,
        format!(
            "`{}` may leave a proc running that reads {} after it returns, and its bytes live \
             in {}, {} — while the proc may still be reading them",
            c.name,
            shown(&a.text),
            h.region,
            dies(h)
        ),
    )
    .with_label(format!("this argument's bytes are in {}", h.region))
    .with_secondary(ws, wl);
    with_hit(d, h).with_note(NOTE)
}

fn refuse_ambient(c: &CallEdge, h: &Hit, why: &Why) -> Diagnostic {
    let (ws, wl) = why_label(why);
    let d = Diagnostic::error(
        codes::E1010,
        c.span,
        format!(
            "`{}` builds in its caller's region and may leave a proc running that reads those \
             bytes after it returns; here that is {}, {} — while the proc may still be reading \
             them",
            c.name,
            h.region,
            dies(h)
        ),
    )
    .with_label(format!(
        "`{}` allocates into {} at this call ([mem.region.create.3])",
        c.name, h.region
    ))
    .with_secondary(ws, wl);
    let mut d = d;
    if let Some((name, intro)) = &h.intro {
        d = d.with_secondary(
            *intro,
            if h.is_proc {
                format!(
                    "`{name}` is a `spawn proc` entry: a proc owns its regions and they \
                     bulk-free at its exit ([conc.proc.kill])"
                )
            } else {
                format!(
                    "region `{name}` is created here and freed with its scope — everything in \
                     it is freed wholesale"
                )
            },
        );
    }
    d.with_note(NOTE)
}

fn refuse_inner_arg(c: &CallEdge, a: &ArgEdge, why: &Why) -> Diagnostic {
    let (ws, wl) = why_label(why);
    Diagnostic::error(
        codes::E1010,
        a.span,
        format!(
            "`{}` may leave a proc running that reads {} after it returns, and inside a \
             closure nothing says where those bytes live or how long they last",
            c.name,
            shown(&a.text)
        ),
    )
    .with_label("this argument's bytes may die before the proc reads them")
    .with_secondary(ws, wl)
    .with_note(
        "a closure's parameters carry no region a summary can name, so a proc left running \
         with them is refused where the closure makes it [conc.proc.arg]. Call the function \
         from a named function, or join the proc in it on every path.",
    )
}

fn refuse_inner_spawn(s: &SpawnOb, span: Span, text: &str) -> Diagnostic {
    Diagnostic::error(
        codes::E1010,
        span,
        format!(
            "this closure may return while `{}` still runs, and {} can name bytes nothing \
             here says outlive it",
            s.callee,
            shown(text)
        ),
    )
    .with_label("the proc may read these bytes after the closure returns")
    .with_secondary(
        s.span,
        "spawned here, and not every path out of the closure joins it",
    )
    .with_note(
        "a closure (or a nested fn) is not a function a caller can ask about, so a proc it \
         leaves running with a value that can name bytes is refused where it is spawned \
         [conc.proc.arg]: join the proc on every path out of the closure — `p.join()`, an exit \
         received from `p.monitor()` — or spawn it from a named function.",
    )
}

fn refuse_value(span: Span, name: &str, why: &Why) -> Diagnostic {
    let (ws, wl) = why_label(why);
    Diagnostic::error(
        codes::E1010,
        span,
        format!(
            "`{name}` may leave a proc running with what it is handed, so it cannot be taken \
             as a fn value: a call through the value cannot be asked what it leaves running"
        ),
    )
    .with_label("a fn value hides the proc from every call made through it")
    .with_secondary(ws, wl)
    .with_note(
        "a function that may return while a proc it spawned still reads its arguments is held \
         to [conc.proc.arg] at every call, where the arguments are visible; through a fn value \
         they are not. Call it by name, or join the proc inside it on every path so it leaves \
         nothing running.",
    )
}
