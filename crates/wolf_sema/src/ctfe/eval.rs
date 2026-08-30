//! The comptime evaluator (s16, Target 1): a tree-walking machine over
//! **typed HIR** — the checked bodies' green trees plus their recorded
//! span→type tables — never WIR (CTFE runs *during* semantic analysis;
//! lowering mid-sema would invert the c03→c05 dependency).
//!
//! The machine is an explicit value stack plus environment frames: no
//! host-stack recursion anywhere, so recursion depth is a *resource
//! limit* (E0704), not a segfault. Every step charges fuel, every new
//! value charges heap cells, and none of the three budgets is
//! disableable (D33).
//!
//! Evaluation order is the language's: strict left-to-right everywhere
//! (`[mem.model.order]`) — comptime evaluation obeys the same clause
//! the runtime does, so folding a call can never reorder its effects.
//!
//! The engine boundary is [`Engine::evaluate`]: `(function, argument
//! values) → value`, values from [`super::value`] only — no checker
//! types in results, so a faster (WIR-based) back end can replace this
//! walker post-M2 without breaking a single client. Memoization keys
//! on `(function identity, argument content hash)` — Zig's model,
//! content-addressed so it composes with the D7 incremental spine.

use std::collections::HashMap;

use wolf_ast::{
    Arg, AssignStmt, BinExpr, Block, BracketApply, CallExpr, ConstDecl, ExprStmt, FieldInit,
    FnDecl, IfExpr, MemberExpr, ParenExpr, PathExpr, PrefixExpr, StringExpr, StructLit, SyntaxKind,
    TupleExpr, WhileExpr, is_expr_kind, is_type_kind,
};
use wolf_span::Span;

use crate::check::{BodyResult, TypedBody};
use crate::graph::{BindTarget, Package};
use crate::sig::{ItemSig, SigTables, bindings_for};
use crate::traits::TraitRef;
use crate::typecheck::BodyOutcome;
use crate::types::{Prim, TyId, TyKind};

use super::intrinsics::{self, Intrinsic, SandboxCategory};
use super::value::{CtType, ValId, ValueArena, ValueKind, canon_f32, canon_f64, int_range};

// ------------------------------------------------------------ budgets ---

/// The three resource limits (D33): defaults, per-site overrides via
/// `#[budget(fuel = …, heap = …, depth = …)]`, hard ceilings —
/// never disableable.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Machine steps.
    pub fuel: u64,
    /// Arena cells a single evaluation may allocate.
    pub heap: u64,
    /// Call frames.
    pub depth: u32,
}

pub const DEFAULT_FUEL: u64 = 1_000_000;
pub const DEFAULT_HEAP: u64 = 65_536;
pub const DEFAULT_DEPTH: u32 = 256;
/// Hard ceilings: an attribute may raise a budget this far and no
/// further (the "never disableable" half of D33).
pub const MAX_FUEL: u64 = 1 << 32;
pub const MAX_HEAP: u64 = 1 << 24;
pub const MAX_DEPTH: u32 = 1 << 16;

impl Default for Budget {
    fn default() -> Budget {
        Budget {
            fuel: DEFAULT_FUEL,
            heap: DEFAULT_HEAP,
            depth: DEFAULT_DEPTH,
        }
    }
}

// ------------------------------------------------------------- faults ---

/// One comptime call-stack frame of a fault's backtrace, innermost
/// first.
#[derive(Clone, Debug)]
pub struct FrameNote {
    pub name: String,
    pub call_span: Span,
}

/// Why comptime evaluation stopped.
#[derive(Clone, Debug)]
pub enum FaultKind {
    /// D33: the sandbox refused an ambient-IO reach (E0701).
    Sandbox {
        name: String,
        category: SandboxCategory,
    },
    /// Fuel exhausted after `spent` steps (E0702).
    Fuel { spent: u64 },
    /// Heap cells exhausted after `cells` (E0703).
    Heap { cells: u64 },
    /// Call depth exceeded `frames` (E0704).
    Depth { frames: u32 },
    /// A value that must be comptime-known is not (E0705): a runtime
    /// local, a runtime-only callee, a non-const argument.
    NotComptime { what: String },
    /// X3 at comptime: checked arithmetic faulted (E0706).
    Arith {
        op: String,
        ty: String,
        detail: String,
    },
    /// Layout is c05's: `size_of`/offsets on non-trivial types (E0708).
    Layout { what: String },
    /// A comptime `assert` failed (E0710). The message is `assert`'s
    /// optional second argument when it evaluated to a string.
    AssertFailed { msg: Option<String> },
    /// An honest engine gap: the construct is fine, the s16 evaluator
    /// subset does not cover it yet — reported as NotYetCheckable,
    /// never as a user error.
    EngineGap { construct: &'static str },
}

/// A fault plus its provenance: where, and through which comptime
/// call frames.
#[derive(Clone, Debug)]
pub struct Fault {
    pub kind: FaultKind,
    pub span: Span,
    /// Innermost first.
    pub frames: Vec<FrameNote>,
}

// -------------------------------------------------------------- stats ---

/// The memoization/evaluation counters the D5 bench line reports.
#[derive(Clone, Copy, Debug, Default)]
pub struct CtfeStats {
    /// Completed root evaluations.
    pub evals: u64,
    pub memo_hits: u64,
    pub memo_misses: u64,
    pub fuel_used: u64,
}

impl CtfeStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.memo_hits + self.memo_misses;
        if total == 0 {
            0.0
        } else {
            self.memo_hits as f64 / total as f64
        }
    }
}

// ------------------------------------------------------------- engine ---

struct CheckedBody<'a> {
    file: usize,
    decl: usize,
    tb: &'a TypedBody,
}

enum GlobalSlot {
    InProgress,
    Done(ValId),
}

/// The comptime engine over one checked package.
pub struct Engine<'a> {
    pub(crate) pkg: &'a Package,
    pub(crate) sigs: &'a SigTables,
    pub arena: ValueArena,
    /// (module, item name) → its checked body (top-level items only).
    bodies: HashMap<(usize, String), CheckedBody<'a>>,
    /// Span→type caches, one per body actually evaluated.
    ty_maps: HashMap<(usize, String), HashMap<(u32, u32), TyId>>,
    /// Module-level `const` values, evaluated on demand.
    globals: HashMap<(usize, String), GlobalSlot>,
    /// (fn identity ‖ argument hashes) → result.
    memo: HashMap<[u8; 32], ValId>,
    pub stats: CtfeStats,
}

// ------------------------------------------------------------ machine ---

/// One pending machine step. Expansion is one level per step, so the
/// machine never recurses on the host stack.
enum Step<'a> {
    Eval(&'a GreenNodeAlias),
    PushVal(ValId),
    PushScope,
    PopScope,
    Drop,
    Bind {
        name: String,
    },
    Assign {
        name: String,
        op: Option<SyntaxKind>,
        span: Span,
    },
    BinOp {
        op: SyntaxKind,
        span: Span,
        opspan: Span,
    },
    ShortCircuit {
        op: SyntaxKind,
        rhs: &'a GreenNodeAlias,
    },
    Prefix {
        op: SyntaxKind,
        span: Span,
    },
    Branch {
        then_b: &'a GreenNodeAlias,
        else_b: Option<&'a GreenNodeAlias>,
    },
    LoopTest {
        cond: &'a GreenNodeAlias,
        body: &'a GreenNodeAlias,
    },
    Member {
        name: String,
        span: Span,
    },
    /// Pop hi then lo; push the `lo..hi` range value (s71).
    RangeBuild {
        span: Span,
    },
    /// Pop the scrutinee; an error value evaluates the handler with
    /// an optional binder scope, anything else passes through (s71).
    ElseUnwrap {
        binder: Option<String>,
        fallback: Option<&'a GreenNodeAlias>,
        span: Span,
    },
    /// Pop one rendered value per hole (deepest = first hole) and
    /// splice them into the literal segments (s71).
    StrBuild {
        node: &'a GreenNodeAlias,
        holes: Vec<Span>,
    },
    TupleBuild {
        n: usize,
    },
    StructBuild {
        module: u32,
        name: String,
        fields: Vec<String>,
    },
    Call {
        callee: Callee,
        argc: usize,
        span: Span,
    },
    Return,
}

type GreenNodeAlias = wolf_ast::GreenNode;

enum Callee {
    UserFn {
        module: usize,
        name: String,
    },
    /// A builtin method on a value receiver (s71 subset): the
    /// receiver evaluates as the first "argument".
    Method {
        name: String,
    },
    Intrinsic(Intrinsic),
    Implements {
        tr: Option<TraitRef>,
    },
    HostStub {
        name: String,
        category: SandboxCategory,
    },
}

/// Why a frame exists — and what to do with its result.
enum Done {
    Root,
    Call { memo_key: Option<[u8; 32]> },
    Global { module: usize, name: String },
    ConstLocal { name: String },
}

struct Frame<'a> {
    /// The lexical home of the code being evaluated.
    module: usize,
    file: usize,
    /// The body whose span→type table serves this frame.
    body: (usize, String),
    /// Display name + call site for backtraces.
    fn_name: String,
    call_span: Span,
    scopes: Vec<Vec<(String, ValId)>>,
    stack: Vec<ValId>,
    work: Vec<Step<'a>>,
    done: Done,
}

/// Root-context extras: the enclosing *runtime* body the site sits in.
struct RootCtx<'a> {
    /// `const` locals declared before the site (name → initializer).
    consts: HashMap<String, &'a GreenNodeAlias>,
    /// Evaluated const-locals.
    values: HashMap<String, ValId>,
    in_progress: Vec<String>,
    /// The runtime body's local names — hitting one is E0705.
    runtime_locals: Vec<String>,
}

impl<'a> Engine<'a> {
    /// Build the engine over the typecheck pass's per-body results.
    pub fn new(pkg: &'a Package, sigs: &'a SigTables, outcomes: &'a [BodyOutcome]) -> Engine<'a> {
        let mut bodies = HashMap::new();
        for o in outcomes {
            if o.body.member.is_some() {
                continue;
            }
            if let BodyResult::Checked(tb) = &o.result {
                bodies.insert(
                    (o.body.module, o.body.name.clone()),
                    CheckedBody {
                        file: o.body.file,
                        decl: o.body.decl,
                        tb,
                    },
                );
            }
        }
        Engine {
            pkg,
            sigs,
            arena: ValueArena::new(),
            bodies,
            ty_maps: HashMap::new(),
            globals: HashMap::new(),
            memo: HashMap::new(),
            stats: CtfeStats::default(),
        }
    }

    // ------------------------------------------------- public surface --

    /// A span that points at nothing in particular (fault plumbing
    /// when no better locus exists).
    fn nowhere(&self) -> Span {
        let f = self.pkg.files.first().expect("a package has files");
        Span::new(f.raw.file, 0, 0)
    }

    /// The contract boundary: evaluate a named function against
    /// argument values. No checker types in, none out.
    pub fn evaluate(
        &mut self,
        module: usize,
        name: &str,
        args: &[ValId],
        budget: Budget,
    ) -> Result<ValId, Fault> {
        let span = self
            .bodies
            .get(&(module, name.to_string()))
            .map(|cb| (cb.file, cb.decl))
            .and_then(|(f, d)| self.item_node(f, d))
            .map(|n| n.span)
            .unwrap_or_else(|| self.nowhere());
        let mut frames = Vec::new();
        let mut root = RootCtx {
            consts: HashMap::new(),
            values: HashMap::new(),
            in_progress: Vec::new(),
            runtime_locals: Vec::new(),
        };
        if let Some(memoized) = self.push_call_frame(&mut frames, module, name, args, span)? {
            self.stats.evals += 1;
            return Ok(memoized);
        }
        let out = self.run(&mut frames, &mut root, budget);
        if out.is_ok() {
            self.stats.evals += 1;
        }
        out
    }

    /// Evaluate one registered comptime call site inside a checked
    /// runtime body: `site` is the `CallExpr` node itself.
    pub fn evaluate_site(
        &mut self,
        body: &crate::check::BodyRef,
        tb: &'a TypedBody,
        body_node: &'a GreenNodeAlias,
        site: &'a GreenNodeAlias,
        budget: Budget,
    ) -> Result<ValId, Fault> {
        // Adopt this body if the constructor did not see it (tests).
        self.bodies
            .entry((body.module, body.name.clone()))
            .or_insert(CheckedBody {
                file: body.file,
                decl: body.decl,
                tb,
            });
        let mut consts = HashMap::new();
        collect_const_locals(body_node, self.src(body.file), site.span.lo, &mut consts);
        let mut root = RootCtx {
            consts,
            values: HashMap::new(),
            in_progress: Vec::new(),
            runtime_locals: tb.locals.iter().map(|(n, _, _)| n.clone()).collect(),
        };
        let mut frames = vec![Frame {
            module: body.module,
            file: body.file,
            body: (body.module, body.name.clone()),
            fn_name: body.name.clone(),
            call_span: site.span,
            scopes: vec![Vec::new()],
            stack: Vec::new(),
            work: vec![Step::Eval(site)],
            done: Done::Root,
        }];
        let out = self.run(&mut frames, &mut root, budget);
        if out.is_ok() {
            self.stats.evals += 1;
        }
        out
    }

    // ---------------------------------------------------- infrastructure --

    fn item_node(&self, file: usize, decl: usize) -> Option<&'a GreenNodeAlias> {
        self.pkg.files[file]
            .parse
            .root
            .nodes()
            .filter(|n| n.kind.is_item())
            .nth(decl)
    }

    fn src(&self, file: usize) -> &'a [u8] {
        &self.pkg.files[file].raw.src
    }

    fn text(&self, file: usize, span: Span) -> String {
        String::from_utf8_lossy(&self.src(file)[span.lo as usize..span.hi as usize]).into_owned()
    }

    /// The recorded type of a span within `body`, resolved to a width
    /// decision: `(width prim, is wrapping)`.
    fn width_at(&mut self, body: &(usize, String), span: Span) -> (Prim, bool) {
        let map = self.ty_maps.entry(body.clone()).or_insert_with(|| {
            let mut m = HashMap::new();
            if let Some(cb) = self.bodies.get(body) {
                for &(s, t) in &cb.tb.exprs {
                    m.insert((s.lo, s.hi), t);
                }
            }
            m
        });
        let Some(&ty) = map.get(&(span.lo, span.hi)) else {
            return (Prim::Int, false);
        };
        let Some(cb) = self.bodies.get(body) else {
            return (Prim::Int, false);
        };
        resolve_width(cb.tb, ty)
    }

    fn push_call_frame(
        &mut self,
        frames: &mut Vec<Frame<'a>>,
        module: usize,
        name: &str,
        args: &[ValId],
        call_span: Span,
    ) -> Result<Option<ValId>, Fault> {
        let fault = |kind: FaultKind| Fault {
            kind,
            span: call_span,
            frames: backtrace(frames),
        };
        // Memoization: (fn identity, argument tuple hash).
        let mut key_input = Vec::new();
        key_input.extend_from_slice(&(module as u64).to_le_bytes());
        key_input.extend_from_slice(name.as_bytes());
        key_input.push(0);
        for &a in args {
            key_input.extend_from_slice(&self.arena.content_hash(a));
        }
        let key = *blake3::hash(&key_input).as_bytes();
        if let Some(&v) = self.memo.get(&key) {
            self.stats.memo_hits += 1;
            return Ok(Some(v));
        }
        self.stats.memo_misses += 1;
        let Some(cb) = self.bodies.get(&(module, name.to_string())) else {
            // Not a checked body: extern (FFI) or missing.
            if let Some(node) = self.fn_decl_node(module, name)
                && FnDecl::cast(node)
                    .is_some_and(|d| d.extern_abi().is_some() || d.body().is_none())
            {
                return Err(fault(FaultKind::Sandbox {
                    name: name.to_string(),
                    category: SandboxCategory::Ffi,
                }));
            }
            return Err(fault(FaultKind::NotComptime {
                what: format!("`{name}` has no comptime-evaluable body"),
            }));
        };
        let (file, decl) = (cb.file, cb.decl);
        let Some(node) = self.item_node(file, decl) else {
            return Err(fault(FaultKind::EngineGap {
                construct: "a body outside the item table",
            }));
        };
        let Some(block) = FnDecl::cast(node).and_then(|d| d.body()) else {
            return Err(fault(FaultKind::Sandbox {
                name: name.to_string(),
                category: SandboxCategory::Ffi,
            }));
        };
        let Some(ItemSig::Fn(sig)) = self.sigs.get(module, name) else {
            return Err(fault(FaultKind::EngineGap {
                construct: "a callee without an elaborated signature",
            }));
        };
        if sig.params.len() != args.len() {
            return Err(fault(FaultKind::EngineGap {
                construct: "a call with mismatched arity at comptime",
            }));
        }
        let params: Vec<(String, ValId)> = sig
            .params
            .iter()
            .zip(args.iter())
            .map(|(p, &v)| (p.name.clone(), v))
            .collect();
        frames.push(Frame {
            module,
            file,
            body: (module, name.to_string()),
            fn_name: name.to_string(),
            call_span,
            scopes: vec![params],
            stack: Vec::new(),
            work: vec![Step::Eval(block.syntax())],
            done: Done::Call {
                memo_key: Some(key),
            },
        });
        Ok(None)
    }

    fn fn_decl_node(&self, module: usize, name: &str) -> Option<&'a GreenNodeAlias> {
        let item = self.pkg.tables[module].get(name)?;
        self.item_node(item.file, item.decl)
    }

    // ----------------------------------------------------- the machine --

    fn run(
        &mut self,
        frames: &mut Vec<Frame<'a>>,
        root: &mut RootCtx<'a>,
        budget: Budget,
    ) -> Result<ValId, Fault> {
        let heap_start = self.arena.cells_used;
        let mut fuel: u64 = 0;
        loop {
            fuel += 1;
            self.stats.fuel_used += 1;
            let here = frames
                .last()
                .map(|f| f.call_span)
                .unwrap_or_else(|| self.nowhere());
            if fuel > budget.fuel {
                return Err(Fault {
                    kind: FaultKind::Fuel { spent: budget.fuel },
                    span: here,
                    frames: backtrace(frames),
                });
            }
            if self.arena.cells_used.saturating_sub(heap_start) > budget.heap {
                return Err(Fault {
                    kind: FaultKind::Heap { cells: budget.heap },
                    span: here,
                    frames: backtrace(frames),
                });
            }
            let Some(frame) = frames.last_mut() else {
                return Err(Fault {
                    kind: FaultKind::EngineGap {
                        construct: "an empty machine",
                    },
                    span: here,
                    frames: Vec::new(),
                });
            };
            let Some(step) = frame.work.pop() else {
                // Frame complete: deliver its value.
                let v = frame.stack.pop().unwrap_or_else(|| self.arena.unit());
                let done = frames.pop().expect("frame exists").done;
                match done {
                    Done::Root => return Ok(v),
                    Done::Call { memo_key } => {
                        if let Some(k) = memo_key {
                            self.memo.insert(k, v);
                        }
                    }
                    Done::Global { module, name } => {
                        self.globals.insert((module, name), GlobalSlot::Done(v));
                    }
                    Done::ConstLocal { name } => {
                        root.values.insert(name.clone(), v);
                        root.in_progress.retain(|n| n != &name);
                    }
                }
                match frames.last_mut() {
                    Some(parent) => parent.stack.push(v),
                    None => return Ok(v),
                }
                continue;
            };
            if let Some(fault) = self.step(frames, root, step, budget)? {
                return Err(fault);
            }
        }
    }

    /// Process one step. `Ok(Some(fault))` aborts; faults that need a
    /// backtrace are built here where the frames are visible.
    #[allow(clippy::too_many_lines)]
    fn step(
        &mut self,
        frames: &mut Vec<Frame<'a>>,
        root: &mut RootCtx<'a>,
        step: Step<'a>,
        budget: Budget,
    ) -> Result<Option<Fault>, Fault> {
        let bt = |frames: &Vec<Frame<'a>>| backtrace(frames);
        macro_rules! fault {
            ($span:expr, $kind:expr) => {
                return Ok(Some(Fault {
                    kind: $kind,
                    span: $span,
                    frames: bt(frames),
                }))
            };
        }
        macro_rules! gap {
            ($span:expr, $c:expr) => {
                fault!($span, FaultKind::EngineGap { construct: $c })
            };
        }
        match step {
            Step::PushVal(v) => frames.last_mut().expect("frame").stack.push(v),
            Step::PushScope => frames.last_mut().expect("frame").scopes.push(Vec::new()),
            Step::PopScope => {
                frames.last_mut().expect("frame").scopes.pop();
            }
            Step::Drop => {
                frames.last_mut().expect("frame").stack.pop();
            }
            Step::Bind { name } => {
                let frame = frames.last_mut().expect("frame");
                let v = frame.stack.pop().expect("bind value");
                frame.scopes.last_mut().expect("scope").push((name, v));
            }
            Step::Assign { name, op, span } => {
                let frame = frames.last_mut().expect("frame");
                let v = frame.stack.pop().expect("assign value");
                let Some(slot) = frame
                    .scopes
                    .iter_mut()
                    .rev()
                    .flat_map(|s| s.iter_mut().rev())
                    .find(|(n, _)| n == &name)
                else {
                    gap!(span, "assignment to a non-local place at comptime");
                };
                match op {
                    None | Some(SyntaxKind::Eq) => slot.1 = v,
                    Some(k) => {
                        let cur = slot.1;
                        let body = frame.body.clone();
                        let bin = match k {
                            SyntaxKind::PlusEq => SyntaxKind::Plus,
                            SyntaxKind::MinusEq => SyntaxKind::Minus,
                            SyntaxKind::StarEq => SyntaxKind::Star,
                            SyntaxKind::SlashEq => SyntaxKind::Slash,
                            SyntaxKind::PercentEq => SyntaxKind::Percent,
                            _ => gap!(span, "this compound assignment at comptime"),
                        };
                        match self.binop(&body, bin, cur, v, span, span) {
                            Ok(r) => {
                                let frame = frames.last_mut().expect("frame");
                                let slot = frame
                                    .scopes
                                    .iter_mut()
                                    .rev()
                                    .flat_map(|s| s.iter_mut().rev())
                                    .find(|(n, _)| n == &name)
                                    .expect("slot");
                                slot.1 = r;
                            }
                            Err(kind) => fault!(span, kind),
                        }
                    }
                }
            }
            Step::BinOp { op, span, opspan } => {
                let frame = frames.last_mut().expect("frame");
                let rhs = frame.stack.pop().expect("rhs");
                let lhs = frame.stack.pop().expect("lhs");
                let body = frame.body.clone();
                match self.binop(&body, op, lhs, rhs, span, opspan) {
                    Ok(v) => frames.last_mut().expect("frame").stack.push(v),
                    Err(kind) => fault!(span, kind),
                }
            }
            Step::ShortCircuit { op, rhs } => {
                let frame = frames.last_mut().expect("frame");
                let lhs = frame.stack.pop().expect("lhs");
                let ValueKind::Bool(b) = *self.arena.kind(lhs) else {
                    gap!(rhs.span, "a non-boolean short-circuit operand");
                };
                let done = (op == SyntaxKind::AmpAmp && !b) || (op == SyntaxKind::PipePipe && b);
                if done {
                    frames.last_mut().expect("frame").stack.push(lhs);
                } else {
                    frames.last_mut().expect("frame").work.push(Step::Eval(rhs));
                }
            }
            Step::Prefix { op, span } => {
                let frame = frames.last_mut().expect("frame");
                let v = frame.stack.pop().expect("operand");
                let body = frame.body.clone();
                match self.prefix(&body, op, v, span) {
                    Ok(v) => frames.last_mut().expect("frame").stack.push(v),
                    Err(kind) => fault!(span, kind),
                }
            }
            Step::Branch { then_b, else_b } => {
                let frame = frames.last_mut().expect("frame");
                let c = frame.stack.pop().expect("condition");
                let ValueKind::Bool(b) = *self.arena.kind(c) else {
                    gap!(then_b.span, "a non-boolean condition at comptime");
                };
                if b {
                    frame.work.push(Step::Eval(then_b));
                } else {
                    match else_b {
                        Some(e) => frame.work.push(Step::Eval(e)),
                        None => {
                            let u = self.arena.unit();
                            frames.last_mut().expect("frame").stack.push(u);
                        }
                    }
                }
            }
            Step::LoopTest { cond, body } => {
                let frame = frames.last_mut().expect("frame");
                let c = frame.stack.pop().expect("condition");
                let ValueKind::Bool(b) = *self.arena.kind(c) else {
                    gap!(cond.span, "a non-boolean loop condition at comptime");
                };
                if b {
                    // After the body runs (value dropped), re-test.
                    frame.work.push(Step::LoopTest { cond, body });
                    frame.work.push(Step::Eval(cond));
                    frame.work.push(Step::Drop);
                    frame.work.push(Step::Eval(body));
                } else {
                    let u = self.arena.unit();
                    frames.last_mut().expect("frame").stack.push(u);
                }
            }
            Step::Member { name, span } => {
                let frame = frames.last_mut().expect("frame");
                let base = frame.stack.pop().expect("member base");
                let out = match self.arena.kind(base).clone() {
                    ValueKind::Struct { fields, .. } => {
                        match fields.iter().find(|(n, _)| n == &name) {
                            Some((_, v)) => *v,
                            None => gap!(span, "a missing field at comptime"),
                        }
                    }
                    ValueKind::Slice(xs) if name == "len" => {
                        self.arena.int(xs.len() as i128, Prim::Int)
                    }
                    // `str.len` is the byte length (D25), exactly the
                    // executing lanes' answer.
                    ValueKind::Str(s) if name == "len" => {
                        self.arena.int(s.len() as i128, Prim::Int)
                    }
                    ValueKind::Tuple(xs) => match name.parse::<usize>() {
                        Ok(i) if i < xs.len() => xs[i],
                        _ => gap!(span, "a tuple member at comptime"),
                    },
                    _ => gap!(span, "member access on this value at comptime"),
                };
                frames.last_mut().expect("frame").stack.push(out);
            }
            Step::RangeBuild { span } => {
                let frame = frames.last_mut().expect("frame");
                let hi = frame.stack.pop().expect("range hi");
                let lo = frame.stack.pop().expect("range lo");
                let (ValueKind::Int { v: a, .. }, ValueKind::Int { v: b, .. }) =
                    (self.arena.kind(lo).clone(), self.arena.kind(hi).clone())
                else {
                    gap!(span, "non-integer range endpoints at comptime");
                };
                let v = self.arena.intern(ValueKind::Range { lo: a, hi: b });
                frames.last_mut().expect("frame").stack.push(v);
            }
            Step::ElseUnwrap {
                binder,
                fallback,
                span,
            } => {
                let frame = frames.last_mut().expect("frame");
                let scrut = frame.stack.pop().expect("else scrutinee");
                if let ValueKind::ErrTag(_) = self.arena.kind(scrut) {
                    let Some(fb) = fallback else {
                        gap!(span, "an `else` without a fallback at comptime");
                    };
                    let frame = frames.last_mut().expect("frame");
                    let mut scope = Vec::new();
                    if let Some(name) = binder {
                        // The binder sees the caught error value (the
                        // tag itself in the s71 comptime subset).
                        scope.push((name, scrut));
                    }
                    frame.scopes.push(scope);
                    frame.work.push(Step::PopScope);
                    frame.work.push(Step::Eval(fb));
                } else {
                    frame.stack.push(scrut);
                }
            }
            Step::StrBuild { node, holes } => {
                let file = frames.last().expect("frame").file;
                let frame = frames.last_mut().expect("frame");
                let mut vals = Vec::with_capacity(holes.len());
                for _ in 0..holes.len() {
                    vals.push(frame.stack.pop().expect("interpolation hole"));
                }
                vals.reverse();
                let mut rendered = Vec::with_capacity(vals.len());
                for v in vals {
                    let text = match self.arena.kind(v) {
                        ValueKind::Str(s) => s.clone(),
                        ValueKind::Int { v, .. } => v.to_string(),
                        ValueKind::Bool(b) => b.to_string(),
                        _ => gap!(node.span, "interpolating this value at comptime"),
                    };
                    rendered.push(text);
                }
                let raw = self.text(file, node.span);
                match splice_interp(&raw, node.span.lo, &holes, &rendered) {
                    Ok(s) => {
                        let v = self.arena.str_(s);
                        frames.last_mut().expect("frame").stack.push(v);
                    }
                    Err(c) => gap!(node.span, c),
                }
            }
            Step::TupleBuild { n } => {
                let frame = frames.last_mut().expect("frame");
                let mut xs = Vec::with_capacity(n);
                for _ in 0..n {
                    xs.push(frame.stack.pop().expect("tuple elem"));
                }
                xs.reverse();
                let v = self.arena.intern(ValueKind::Tuple(xs));
                frames.last_mut().expect("frame").stack.push(v);
            }
            Step::StructBuild {
                module,
                name,
                fields,
            } => {
                let frame = frames.last_mut().expect("frame");
                let mut vals = Vec::with_capacity(fields.len());
                for _ in 0..fields.len() {
                    vals.push(frame.stack.pop().expect("field value"));
                }
                vals.reverse();
                let fields: Vec<(String, ValId)> = fields.into_iter().zip(vals).collect();
                let v = self.arena.intern(ValueKind::Struct {
                    module,
                    name,
                    fields,
                });
                frames.last_mut().expect("frame").stack.push(v);
            }
            Step::Return => {
                let frame = frames.last_mut().expect("frame");
                let v = frame.stack.pop().unwrap_or_else(|| self.arena.unit());
                frame.work.clear();
                frame.scopes.truncate(1);
                frame.stack.clear();
                frame.stack.push(v);
            }
            Step::Call { callee, argc, span } => {
                let frame = frames.last_mut().expect("frame");
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(frame.stack.pop().expect("argument"));
                }
                args.reverse();
                match callee {
                    Callee::HostStub { name, category } => {
                        fault!(span, FaultKind::Sandbox { name, category })
                    }
                    Callee::Intrinsic(i) => match self.eval_intrinsic(i, &args, span) {
                        Ok(v) => frames.last_mut().expect("frame").stack.push(v),
                        Err(kind) => fault!(span, kind),
                    },
                    Callee::Implements { tr } => {
                        let Some(tr) = tr else {
                            gap!(span, "`implements` with an unresolvable trait argument");
                        };
                        match self.eval_implements(&args, &tr, span) {
                            Ok(v) => frames.last_mut().expect("frame").stack.push(v),
                            Err(kind) => fault!(span, kind),
                        }
                    }
                    Callee::Method { name } => {
                        // The s71 builtin-method subset: the boundary
                        // primitive `str.get` ([mem.str.get]) — the
                        // one str method a comptime string walker
                        // cannot spell any other way.
                        let recv = args.first().map(|&v| self.arena.kind(v).clone());
                        match (recv, name.as_str()) {
                            (Some(ValueKind::Str(s)), "get") => {
                                let Some(ValueKind::Range { lo, hi }) =
                                    args.get(1).map(|&v| self.arena.kind(v).clone())
                                else {
                                    gap!(span, "`str.get` without a range at comptime");
                                };
                                let miss = lo < 0
                                    || hi < lo
                                    || hi > s.len() as i128
                                    || !s.is_char_boundary(lo as usize)
                                    || !s.is_char_boundary(hi as usize);
                                let v = if miss {
                                    self.arena.intern(ValueKind::ErrTag("none".to_string()))
                                } else {
                                    self.arena.str_(&s[lo as usize..hi as usize])
                                };
                                frames.last_mut().expect("frame").stack.push(v);
                            }
                            _ => gap!(span, "this method at comptime"),
                        }
                    }
                    Callee::UserFn { module, name } => {
                        if frames.len() as u32 >= budget.depth {
                            fault!(
                                span,
                                FaultKind::Depth {
                                    frames: budget.depth
                                }
                            );
                        }
                        if let Some(memoized) =
                            self.push_call_frame(frames, module, &name, &args, span)?
                        {
                            frames.last_mut().expect("frame").stack.push(memoized);
                        }
                    }
                }
            }
            Step::Eval(node) => {
                if let Some(f) = self.eval_node(frames, root, node)? {
                    return Ok(Some(f));
                }
            }
        }
        Ok(None)
    }

    // ------------------------------------------------- node expansion --

    /// Expand one AST node into steps (one level — no recursion).
    #[allow(clippy::too_many_lines)]
    fn eval_node(
        &mut self,
        frames: &mut Vec<Frame<'a>>,
        root: &mut RootCtx<'a>,
        node: &'a GreenNodeAlias,
    ) -> Result<Option<Fault>, Fault> {
        macro_rules! fault {
            ($span:expr, $kind:expr) => {
                return Ok(Some(Fault {
                    kind: $kind,
                    span: $span,
                    frames: backtrace(frames),
                }))
            };
        }
        macro_rules! gap {
            ($span:expr, $c:expr) => {
                fault!($span, FaultKind::EngineGap { construct: $c })
            };
        }
        let (module, file, body, is_root) = {
            let f = frames.last().expect("frame");
            (
                f.module,
                f.file,
                f.body.clone(),
                // Const-local frames evaluate in the root context too:
                // one const local may name another.
                matches!(f.done, Done::Root | Done::ConstLocal { .. }),
            )
        };
        match node.kind {
            SyntaxKind::LiteralExpr => {
                let Some(t) = node.tokens().next() else {
                    gap!(node.span, "a broken literal at comptime");
                };
                let v = match t.kind {
                    SyntaxKind::TrueKw => self.arena.bool_(true),
                    SyntaxKind::FalseKw => self.arena.bool_(false),
                    SyntaxKind::Int => {
                        let (w, _) = self.width_at(&body, node.span);
                        let text = self.text(file, t.span);
                        let Some(v) = parse_int(&text) else {
                            gap!(node.span, "an unparseable integer literal");
                        };
                        let (lo, hi) = int_range(w);
                        if v < lo || v > hi {
                            fault!(
                                node.span,
                                FaultKind::Arith {
                                    op: "literal".to_string(),
                                    ty: w.name().to_string(),
                                    detail: format!("{v} does not fit `{}`", w.name()),
                                }
                            );
                        }
                        self.arena.int(v, w)
                    }
                    SyntaxKind::Float => {
                        let (w, _) = self.width_at(&body, node.span);
                        let text: String = self
                            .text(file, t.span)
                            .chars()
                            .filter(|&c| c != '_')
                            .collect();
                        if w == Prim::F32 {
                            let Ok(x) = text.parse::<f32>() else {
                                gap!(node.span, "an unparseable float literal");
                            };
                            self.arena.float(canon_f32(x) as u64, Prim::F32)
                        } else {
                            let Ok(x) = text.parse::<f64>() else {
                                gap!(node.span, "an unparseable float literal");
                            };
                            self.arena.float(canon_f64(x), Prim::F64)
                        }
                    }
                    _ => gap!(node.span, "this literal at comptime"),
                };
                frames.last_mut().expect("frame").stack.push(v);
            }
            SyntaxKind::StringExpr => {
                let d = StringExpr::cast(node).expect("kind");
                let raw = self.text(file, node.span);
                let interps: Vec<_> = d.interps().collect();
                if interps.iter().any(|i| i.expr().is_some()) {
                    // The s71 interpolation subset: single-line
                    // strings, plain `{x}` holes. Format specs and
                    // multiline dedent stay honest gaps.
                    if raw.starts_with("\"\"\"") {
                        gap!(
                            node.span,
                            "interpolation inside a multiline string at comptime"
                        );
                    }
                    if interps.iter().any(|i| i.format_spec().is_some()) {
                        gap!(node.span, "format specs at comptime");
                    }
                    let mut holes = Vec::with_capacity(interps.len());
                    let mut exprs = Vec::with_capacity(interps.len());
                    for i in &interps {
                        holes.push(i.syntax().span);
                        exprs.push(i.expr());
                    }
                    let empty = self.arena.str_("");
                    let frame = frames.last_mut().expect("frame");
                    frame.work.push(Step::StrBuild { node, holes });
                    for x in exprs.into_iter().rev() {
                        match x {
                            Some(n) => frame.work.push(Step::Eval(n)),
                            None => frame.work.push(Step::PushVal(empty)),
                        }
                    }
                    return Ok(None);
                }
                let v = self.arena.str_(unquote(&raw));
                frames.last_mut().expect("frame").stack.push(v);
            }
            SyntaxKind::PathExpr => {
                let Some(t) = PathExpr::cast(node).and_then(|p| p.ident()) else {
                    gap!(node.span, "a broken path at comptime");
                };
                let name = self.text(file, t.span);
                // Scoped locals first.
                let local = frames
                    .last()
                    .expect("frame")
                    .scopes
                    .iter()
                    .rev()
                    .flat_map(|s| s.iter().rev())
                    .find(|(n, _)| n == &name)
                    .map(|(_, v)| *v);
                if let Some(v) = local {
                    frames.last_mut().expect("frame").stack.push(v);
                    return Ok(None);
                }
                if is_root {
                    if let Some(&v) = root.values.get(&name) {
                        frames.last_mut().expect("frame").stack.push(v);
                        return Ok(None);
                    }
                    if let Some(&init) = root.consts.get(&name) {
                        if root.in_progress.contains(&name) {
                            fault!(
                                node.span,
                                FaultKind::NotComptime {
                                    what: format!("`{name}` depends on itself at comptime"),
                                }
                            );
                        }
                        root.in_progress.push(name.clone());
                        // The child frame's completion delivers the
                        // value straight onto this frame's stack — it
                        // *is* this path's value.
                        frames.push(Frame {
                            module,
                            file,
                            body: body.clone(),
                            fn_name: format!("const {name}"),
                            call_span: node.span,
                            scopes: vec![Vec::new()],
                            stack: Vec::new(),
                            work: vec![Step::Eval(init)],
                            done: Done::ConstLocal { name },
                        });
                        return Ok(None);
                    }
                }
                match self
                    .resolve_path_value(frames, module, file, &name, node.span, is_root, root)?
                {
                    PathVal::Value(v) => frames.last_mut().expect("frame").stack.push(v),
                    // The pushed global-init frame delivers its value
                    // onto this frame's stack on completion — nothing
                    // more to schedule.
                    PathVal::PushedFrame => {}
                    PathVal::Fault(f) => return Ok(Some(f)),
                }
            }
            SyntaxKind::ParenExpr => match ParenExpr::cast(node).and_then(|p| p.expr()) {
                Some(inner) => frames
                    .last_mut()
                    .expect("frame")
                    .work
                    .push(Step::Eval(inner)),
                None => gap!(node.span, "an empty parenthesis at comptime"),
            },
            SyntaxKind::Block => {
                let b = Block::cast(node).expect("kind");
                let stmts: Vec<&GreenNodeAlias> = b.statements().collect();
                let trailing = b.trailing_expr();
                let take = if trailing.is_some() {
                    stmts.len().saturating_sub(1)
                } else {
                    stmts.len()
                };
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::PopScope);
                match trailing {
                    Some(t) => frame.work.push(Step::Eval(t)),
                    None => {
                        let u = self.arena.unit();
                        frames
                            .last_mut()
                            .expect("frame")
                            .work
                            .push(Step::PushVal(u));
                    }
                }
                let frame = frames.last_mut().expect("frame");
                for s in stmts[..take].iter().rev() {
                    frame.work.push(Step::Eval(s));
                }
                frame.work.push(Step::PushScope);
            }
            SyntaxKind::ExprStmt => {
                if let Some(e) = ExprStmt::cast(node).and_then(|x| x.expr()) {
                    let frame = frames.last_mut().expect("frame");
                    frame.work.push(Step::Drop);
                    frame.work.push(Step::Eval(e));
                }
            }
            SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
                // Binders schedule right to left on the work stack so
                // they EVALUATE left to right (D63: a group is the
                // sequence of single bindings).
                let binders = wolf_ast::binding_binders(node);
                let mut steps = Vec::new();
                for b in &binders {
                    let (Some(pat), Some(init)) = (b.pattern, b.init) else {
                        gap!(node.span, "a binding without an initializer at comptime");
                    };
                    let Some(name) = single_ident_name(pat, self.src(file)) else {
                        gap!(node.span, "a pattern binding at comptime");
                    };
                    steps.push((name, init));
                }
                let frame = frames.last_mut().expect("frame");
                for (name, init) in steps.into_iter().rev() {
                    frame.work.push(Step::Bind { name });
                    frame.work.push(Step::Eval(init));
                }
            }
            SyntaxKind::ConstDecl => {
                let d = ConstDecl::cast(node);
                let (Some(nt), Some(init)) = (d.and_then(|c| c.name()), d.and_then(|c| c.init()))
                else {
                    gap!(node.span, "a `const` without an initializer at comptime");
                };
                let name = self.text(file, nt.span);
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::Bind { name });
                frame.work.push(Step::Eval(init));
            }
            SyntaxKind::AssignStmt => {
                let Some(a) = AssignStmt::cast(node) else {
                    return Ok(None);
                };
                let (Some(place), Some(value)) = (a.place(), a.value()) else {
                    return Ok(None);
                };
                if place.kind != SyntaxKind::PathExpr {
                    gap!(place.span, "assignment through this place at comptime");
                }
                let Some(t) = PathExpr::cast(place).and_then(|p| p.ident()) else {
                    gap!(place.span, "a broken assignment place");
                };
                let name = self.text(file, t.span);
                let op = a.op().map(|t| t.kind);
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::Assign {
                    name,
                    op,
                    span: node.span,
                });
                frame.work.push(Step::Eval(value));
            }
            SyntaxKind::BinExpr => {
                let d = BinExpr::cast(node).expect("kind");
                let (Some(lhs), Some(rhs), Some(opt)) = (d.lhs(), d.rhs(), d.op()) else {
                    gap!(node.span, "a broken operator at comptime");
                };
                let frame = frames.last_mut().expect("frame");
                match opt.kind {
                    SyntaxKind::AmpAmp | SyntaxKind::PipePipe => {
                        frame.work.push(Step::ShortCircuit { op: opt.kind, rhs });
                        frame.work.push(Step::Eval(lhs));
                    }
                    op => {
                        frame.work.push(Step::BinOp {
                            op,
                            span: node.span,
                            opspan: opt.span,
                        });
                        frame.work.push(Step::Eval(rhs));
                        frame.work.push(Step::Eval(lhs));
                    }
                }
            }
            SyntaxKind::PrefixExpr => {
                let d = PrefixExpr::cast(node).expect("kind");
                let (Some(op), Some(operand)) = (d.op(), d.operand()) else {
                    gap!(node.span, "a broken prefix operator at comptime");
                };
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::Prefix {
                    op: op.kind,
                    span: node.span,
                });
                frame.work.push(Step::Eval(operand));
            }
            SyntaxKind::IfExpr => {
                let d = IfExpr::cast(node).expect("kind");
                let (Some(cond), Some(then_b)) = (d.condition(), d.then_block()) else {
                    gap!(node.span, "a broken `if` at comptime");
                };
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::Branch {
                    then_b: then_b.syntax(),
                    else_b: d.else_branch(),
                });
                frame.work.push(Step::Eval(cond));
            }
            SyntaxKind::WhileExpr => {
                let d = WhileExpr::cast(node).expect("kind");
                let Some(cond) = d.condition() else {
                    gap!(node.span, "a broken `while` at comptime");
                };
                let Some(b) = d.body() else {
                    gap!(node.span, "a `while` without a body at comptime");
                };
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::LoopTest {
                    cond,
                    body: b.syntax(),
                });
                frame.work.push(Step::Eval(cond));
            }
            SyntaxKind::ReturnExpr => {
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::Return);
                match node.nodes().find(|n| is_expr_kind(n.kind)) {
                    Some(e) => frame.work.push(Step::Eval(e)),
                    None => {
                        let u = self.arena.unit();
                        frames
                            .last_mut()
                            .expect("frame")
                            .work
                            .push(Step::PushVal(u));
                    }
                }
            }
            SyntaxKind::TupleExpr => {
                let d = TupleExpr::cast(node).expect("kind");
                let elems: Vec<&GreenNodeAlias> = d.elems().collect();
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::TupleBuild { n: elems.len() });
                for e in elems.iter().rev() {
                    frame.work.push(Step::Eval(e));
                }
            }
            SyntaxKind::StructLit => {
                let d = StructLit::cast(node).expect("kind");
                let Some((smodule, sname)) = self.struct_target(module, file, d.path()) else {
                    gap!(node.span, "this struct-literal head at comptime");
                };
                let mut names = Vec::new();
                let mut inits = Vec::new();
                for init in d.fields() {
                    let (Some(nt), Some(v)) = (FieldInit::name(init), FieldInit::value(init))
                    else {
                        gap!(node.span, "a shorthand field initializer at comptime");
                    };
                    names.push(self.text(file, nt.span));
                    inits.push(v);
                }
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::StructBuild {
                    module: smodule as u32,
                    name: sname,
                    fields: names,
                });
                // Fields evaluate in written order ([mem.model.order]).
                for v in inits.iter().rev() {
                    frame.work.push(Step::Eval(v));
                }
            }
            SyntaxKind::MemberExpr => {
                let d = MemberExpr::cast(node).expect("kind");
                let (Some(base), Some(member)) = (d.base(), d.member()) else {
                    gap!(node.span, "a broken member access at comptime");
                };
                let name = self.text(file, member.span);
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::Member {
                    name,
                    span: node.span,
                });
                frame.work.push(Step::Eval(base));
            }
            SyntaxKind::CallExpr => {
                let d = CallExpr::cast(node).expect("kind");
                let Some(callee_node) = d.callee() else {
                    gap!(node.span, "a broken call at comptime");
                };
                let args: Vec<Arg<'a>> = d.args().into_iter().flat_map(|a| a.args()).collect();
                let callee = match self.resolve_callee(frames, module, file, callee_node, &args) {
                    Ok(c) => c,
                    Err(kind) => fault!(callee_node.span, kind),
                };
                // `implements(T, Trait)` evaluates only its first
                // argument; the trait is resolved statically.
                let arg_nodes: Vec<&GreenNodeAlias> = match &callee {
                    Callee::Implements { .. } => {
                        args.iter().take(1).filter_map(|a| Arg::value(*a)).collect()
                    }
                    _ => args.iter().filter_map(|a| Arg::value(*a)).collect(),
                };
                let mut prepared: Vec<Step<'a>> = Vec::new();
                // A method callee evaluates its receiver as the first
                // "argument" (s71).
                if let Callee::Method { .. } = &callee {
                    let base = MemberExpr::cast(callee_node).and_then(|m| m.base());
                    let Some(base) = base else {
                        gap!(callee_node.span, "a broken method receiver at comptime");
                    };
                    prepared.push(Step::Eval(base));
                }
                let recv = usize::from(matches!(&callee, Callee::Method { .. }));
                for v in &arg_nodes {
                    if is_type_kind(v.kind) {
                        match self.type_node_value(module, file, v) {
                            Some(ct) => {
                                let tv = self.arena.ty(ct);
                                prepared.push(Step::PushVal(tv));
                            }
                            None => gap!(v.span, "this type-shaped argument at comptime"),
                        }
                    } else {
                        prepared.push(Step::Eval(v));
                    }
                }
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::Call {
                    callee,
                    argc: arg_nodes.len() + recv,
                    span: node.span,
                });
                for s in prepared.into_iter().rev() {
                    frame.work.push(s);
                }
            }
            // Honest gaps — constructs the s16 evaluator subset does
            // not cover. Each is a NotYetCheckable-style refusal.
            SyntaxKind::MatchExpr => gap!(node.span, "`match` at comptime"),
            SyntaxKind::ForExpr => gap!(node.span, "`for` at comptime"),
            SyntaxKind::LoopExpr => gap!(node.span, "`loop` at comptime"),
            SyntaxKind::BreakExpr | SyntaxKind::ContinueExpr => {
                gap!(node.span, "`break`/`continue` at comptime")
            }
            SyntaxKind::RangeExpr => {
                // The s71 subset: the exclusive two-endpoint form
                // (`a..b`) — `str.get`'s argument shape. Inclusive
                // and end-relative forms stay honest gaps.
                let d = wolf_ast::RangeExpr::cast(node).expect("kind");
                if d.is_inclusive() {
                    gap!(node.span, "inclusive ranges at comptime");
                }
                let eps: Vec<&GreenNodeAlias> = d.endpoints().collect();
                if eps.len() != 2 || eps.iter().any(|n| n.kind == SyntaxKind::FromEndExpr) {
                    gap!(node.span, "this range shape at comptime");
                }
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::RangeBuild { span: node.span });
                frame.work.push(Step::Eval(eps[1]));
                frame.work.push(Step::Eval(eps[0]));
            }
            SyntaxKind::CastExpr => gap!(node.span, "`as` conversions at comptime"),
            SyntaxKind::ElseExpr => {
                // `expr else fallback` / `expr else |e| body` (s71
                // subset): the caught value is the tag itself; payload
                // destructuring stays a gap.
                let d = wolf_ast::ElseExpr::cast(node).expect("kind");
                let Some(scrut) = d.scrutinized() else {
                    gap!(node.span, "a broken `else` at comptime");
                };
                let binder = match d.handler_pattern() {
                    None => None,
                    Some(p) if p.kind == SyntaxKind::WildcardPat => None,
                    Some(p) if p.kind == SyntaxKind::IdentPat => Some(self.text(file, p.span)),
                    Some(p) => gap!(p.span, "this handler pattern at comptime"),
                };
                let frame = frames.last_mut().expect("frame");
                frame.work.push(Step::ElseUnwrap {
                    binder,
                    fallback: d.fallback(),
                    span: node.span,
                });
                frame.work.push(Step::Eval(scrut));
            }
            SyntaxKind::TryExpr => {
                gap!(node.span, "the error channel at comptime")
            }
            SyntaxKind::ClosureExpr => gap!(node.span, "closures at comptime"),
            SyntaxKind::BracketApply => {
                gap!(node.span, "indexing/generic application at comptime")
            }
            SyntaxKind::DeferStmt => gap!(node.span, "`defer` at comptime"),
            _ => gap!(
                node.span,
                "this construct at comptime (outside the evaluator's subset)"
            ),
        }
        Ok(None)
    }
}

// Continued in impl blocks below (operators, resolution, intrinsics).

enum PathVal {
    Value(ValId),
    PushedFrame,
    Fault(Fault),
}

impl<'a> Engine<'a> {
    /// Resolve a bare name in value position at comptime.
    #[allow(clippy::too_many_arguments)]
    fn resolve_path_value(
        &mut self,
        frames: &mut Vec<Frame<'a>>,
        module: usize,
        file: usize,
        name: &str,
        span: Span,
        is_root: bool,
        root: &RootCtx<'a>,
    ) -> Result<PathVal, Fault> {
        let bt = backtrace(frames);
        let fault = |kind: FaultKind| {
            PathVal::Fault(Fault {
                kind,
                span,
                frames: bt.clone(),
            })
        };
        // Imports, then module items — the resolver's order.
        let mut target: Option<(usize, String)> = None;
        for b in bindings_for(self.pkg, module, file) {
            if b.name == name {
                if let BindTarget::Item { module: m, name: n } = &b.target {
                    target = Some((*m, n.clone()));
                }
                break;
            }
        }
        if target.is_none() && self.pkg.tables[module].get(name).is_some() {
            target = Some((module, name.to_string()));
        }
        if let Some((m, n)) = target {
            return Ok(match self.sigs.get(m, &n) {
                Some(ItemSig::Fn(_)) => {
                    let v = self.arena.intern(ValueKind::Fn {
                        module: m as u32,
                        name: n,
                    });
                    PathVal::Value(v)
                }
                Some(ItemSig::Struct(_) | ItemSig::Enum { .. }) => {
                    let v = self.arena.ty(CtType::Nominal {
                        module: m as u32,
                        name: n,
                    });
                    PathVal::Value(v)
                }
                Some(ItemSig::Global(_)) => {
                    match self.globals.get(&(m, n.clone())) {
                        Some(GlobalSlot::Done(v)) => PathVal::Value(*v),
                        Some(GlobalSlot::InProgress) => fault(FaultKind::NotComptime {
                            what: format!("`{n}` depends on itself at comptime"),
                        }),
                        None => {
                            // Only `const` items are comptime-known.
                            let Some(cb) = self.bodies.get(&(m, n.clone())) else {
                                return Ok(fault(FaultKind::NotComptime {
                                    what: format!(
                                        "`{n}` is not a `const` item checked for comptime use"
                                    ),
                                }));
                            };
                            let (gfile, gdecl) = (cb.file, cb.decl);
                            let Some(nod) = self.item_node(gfile, gdecl) else {
                                return Ok(fault(FaultKind::EngineGap {
                                    construct: "a global outside the item table",
                                }));
                            };
                            if nod.kind != SyntaxKind::ConstDecl {
                                return Ok(fault(FaultKind::NotComptime {
                                    what: format!(
                                        "`{n}` is a runtime global — only `const` items are \
                                         comptime-known"
                                    ),
                                }));
                            }
                            let Some(init) = nod.nodes().find(|x| is_expr_kind(x.kind)) else {
                                return Ok(fault(FaultKind::NotComptime {
                                    what: format!("`{n}` has no initializer"),
                                }));
                            };
                            self.globals.insert((m, n.clone()), GlobalSlot::InProgress);
                            frames.push(Frame {
                                module: m,
                                file: gfile,
                                body: (m, n.clone()),
                                fn_name: format!("const {n}"),
                                call_span: span,
                                scopes: vec![Vec::new()],
                                stack: Vec::new(),
                                work: vec![Step::Eval(init)],
                                done: Done::Global { module: m, name: n },
                            });
                            PathVal::PushedFrame
                        }
                    }
                }
                _ => fault(FaultKind::EngineGap {
                    construct: "this item as a comptime value",
                }),
            });
        }
        if let Some(p) = Prim::from_name(name) {
            let v = self.arena.ty(CtType::Prim(p));
            return Ok(PathVal::Value(v));
        }
        if is_root && root.runtime_locals.iter().any(|n| n == name) {
            return Ok(fault(FaultKind::NotComptime {
                what: format!("`{name}` is a runtime value"),
            }));
        }
        Ok(fault(FaultKind::EngineGap {
            construct: "an unresolved name at comptime",
        }))
    }

    /// Resolve a call's callee to its comptime meaning.
    fn resolve_callee(
        &self,
        frames: &[Frame<'a>],
        module: usize,
        file: usize,
        callee: &'a GreenNodeAlias,
        args: &[Arg<'a>],
    ) -> Result<Callee, FaultKind> {
        if callee.kind == SyntaxKind::PathExpr {
            let Some(t) = PathExpr::cast(callee).and_then(|p| p.ident()) else {
                return Err(FaultKind::EngineGap {
                    construct: "a broken callee at comptime",
                });
            };
            let name = self.text(file, t.span);
            // Locals shadow everything (a fn-typed value).
            let local = frames
                .last()
                .expect("frame")
                .scopes
                .iter()
                .rev()
                .flat_map(|s| s.iter().rev())
                .find(|(n, _)| n == &name)
                .map(|(_, v)| *v);
            if let Some(v) = local {
                return match self.arena.kind(v).clone() {
                    ValueKind::Fn { module, name } => Ok(Callee::UserFn {
                        module: module as usize,
                        name,
                    }),
                    _ => Err(FaultKind::EngineGap {
                        construct: "calling this value at comptime",
                    }),
                };
            }
            if let Some(category) = intrinsics::host_stub(&name) {
                return Ok(Callee::HostStub { name, category });
            }
            // The pure builtins (s40 json, s81 `str_from_utf8`): no
            // sandbox category — they reach nothing ambient — but the
            // engine models neither a json evaluator nor `List` values,
            // so the refusal is an honest gap, not E0701.
            if intrinsics::pure_builtin(&name) {
                return Err(FaultKind::EngineGap {
                    construct: if name == "str_from_utf8" {
                        "`str_from_utf8` at comptime (the engine models no \
                         `List` values; growing the D33 allowlist is a design \
                         decision, not a convenience)"
                    } else {
                        "json builtins at comptime (no json evaluator in the \
                         D33 allowlist at v0)"
                    },
                });
            }
            if let Some(i) = intrinsics::intrinsic(&name) {
                if i == Intrinsic::Implements {
                    let tr = args
                        .get(1)
                        .and_then(|a| Arg::value(*a))
                        .and_then(|v| self.trait_of_node(module, file, v));
                    return Ok(Callee::Implements { tr });
                }
                return Ok(Callee::Intrinsic(i));
            }
            // Imports then module items.
            for b in bindings_for(self.pkg, module, file) {
                if b.name == name {
                    if let BindTarget::Item { module: m, name: n } = &b.target {
                        return Ok(Callee::UserFn {
                            module: *m,
                            name: n.clone(),
                        });
                    }
                    break;
                }
            }
            if self.pkg.tables[module].get(&name).is_some() {
                return Ok(Callee::UserFn { module, name });
            }
            return Err(FaultKind::EngineGap {
                construct: "an unresolved callee at comptime",
            });
        }
        // `List[int]()`, `Pool[Node]()`, `channel[int]()` — a container
        // constructor spells as a `BracketApply` head, so it never
        // reached the `PathExpr` arm above and used to fall out of the
        // bottom as "calling through this callee", a message naming
        // nothing the author wrote (wolf-lang#101, wolf-std F-0077).
        // The engine's value arena is hash-consed and immutable
        // (`ctfe::value`): it has scalars, `str`, tuples and comptime
        // slices, and no container with identity. Say so, so a package
        // author learns which subset they left rather than bisecting
        // for it.
        if callee.kind == SyntaxKind::BracketApply
            && let Some(b) = BracketApply::cast(callee)
            && let Some(head) = b.callee()
            && let Some(t) = PathExpr::cast(head).and_then(|p| p.ident())
        {
            let name = self.text(file, t.span);
            if let Some(construct) = Self::container_gap(&name) {
                return Err(FaultKind::EngineGap { construct });
            }
        }
        if callee.kind == SyntaxKind::MemberExpr
            && let Some(m) = MemberExpr::cast(callee)
        {
            if let Some((tm, tn)) = self.namespace_member(module, file, m) {
                return Ok(Callee::UserFn {
                    module: tm,
                    name: tn,
                });
            }
            // A method on a value receiver (s71): the receiver
            // evaluates as the first "argument"; the dispatch is on
            // its comptime value.
            if let Some(t) = m.member() {
                return Ok(Callee::Method {
                    name: self.text(file, t.span),
                });
            }
        }
        Err(FaultKind::EngineGap {
            construct: "calling through this callee at comptime",
        })
    }

    /// The named engine gap for a prelude container constructor
    /// (wolf-lang#101). `None` for every other bracket-applied head —
    /// those keep the generic message, which is honest for them.
    fn container_gap(name: &str) -> Option<&'static str> {
        Some(match name {
            "List" => {
                "`List` construction at comptime (the engine's values are \
                 hash-consed and immutable — scalars, `str`, tuples and \
                 comptime slices; no container with identity yet, so a \
                 builtin whose argument is a `List` is unreachable here \
                 whatever the D33 sandbox says about it)"
            }
            "Pool" => {
                "`Pool` construction at comptime (the engine models no \
                 container with identity — see `List`)"
            }
            "channel" => {
                "`channel` construction at comptime (channels are a runtime \
                 concurrency surface; comptime evaluation is single-threaded \
                 and hermetic)"
            }
            _ => return None,
        })
    }

    fn namespace_member(
        &self,
        module: usize,
        file: usize,
        m: MemberExpr<'a>,
    ) -> Option<(usize, String)> {
        let base = m.base()?;
        let t = PathExpr::cast(base)?.ident()?;
        let name = self.text(file, t.span);
        let b = bindings_for(self.pkg, module, file)
            .iter()
            .find(|b| b.name == name)?;
        let BindTarget::PkgModule(target) = b.target else {
            return None;
        };
        let member = m.member()?;
        Some((target, self.text(file, member.span)))
    }

    /// The trait a path argument names (`implements(T, Show)`).
    fn trait_of_node(
        &self,
        module: usize,
        file: usize,
        node: &'a GreenNodeAlias,
    ) -> Option<TraitRef> {
        if node.kind != SyntaxKind::PathExpr {
            return None;
        }
        let t = PathExpr::cast(node)?.ident()?;
        let name = self.text(file, t.span);
        for b in bindings_for(self.pkg, module, file) {
            if b.name == name {
                if let BindTarget::Item { module: m, name: n } = &b.target {
                    let tr = TraitRef {
                        module: *m,
                        name: n.clone(),
                    };
                    if self.sigs.traits.contains_key(&tr) {
                        return Some(tr);
                    }
                }
                return None;
            }
        }
        let tr = TraitRef { module, name };
        self.sigs.traits.contains_key(&tr).then_some(tr)
    }

    fn struct_target(
        &self,
        module: usize,
        file: usize,
        path: Option<&'a GreenNodeAlias>,
    ) -> Option<(usize, String)> {
        let path = path?;
        if path.kind == SyntaxKind::PathExpr {
            let t = PathExpr::cast(path)?.ident()?;
            let name = self.text(file, t.span);
            for b in bindings_for(self.pkg, module, file) {
                if b.name == name {
                    if let BindTarget::Item { module: m, name: n } = &b.target {
                        return Some((*m, n.clone()));
                    }
                    return None;
                }
            }
            if self.pkg.tables[module].get(&name).is_some() {
                return Some((module, name));
            }
            return None;
        }
        if path.kind == SyntaxKind::MemberExpr {
            return self.namespace_member(module, file, MemberExpr::cast(path)?);
        }
        None
    }

    /// A *type-shaped* argument node as a comptime type value.
    fn type_node_value(
        &self,
        module: usize,
        file: usize,
        node: &'a GreenNodeAlias,
    ) -> Option<CtType> {
        match node.kind {
            SyntaxKind::PathType => {
                let d = wolf_ast::PathType::cast(node)?;
                let segs: Vec<String> = d
                    .path()?
                    .segments()
                    .map(|t| self.text(file, t.span))
                    .collect();
                if segs.len() == 1 {
                    let name = &segs[0];
                    if let Some(p) = Prim::from_name(name) {
                        return Some(CtType::Prim(p));
                    }
                    for b in bindings_for(self.pkg, module, file) {
                        if &b.name == name
                            && let BindTarget::Item { module: m, name: n } = &b.target
                        {
                            return Some(CtType::Nominal {
                                module: *m as u32,
                                name: n.clone(),
                            });
                        }
                    }
                    if self.pkg.tables[module].get(name).is_some() {
                        return Some(CtType::Nominal {
                            module: module as u32,
                            name: name.clone(),
                        });
                    }
                }
                Some(CtType::Opaque(self.text(file, node.span)))
            }
            SyntaxKind::TypeType => Some(CtType::TypeTy),
            _ => Some(CtType::Opaque(self.text(file, node.span))),
        }
    }

    // -------------------------------------------------------- operators --

    fn binop(
        &mut self,
        body: &(usize, String),
        op: SyntaxKind,
        lhs: ValId,
        rhs: ValId,
        span: Span,
        _opspan: Span,
    ) -> Result<ValId, FaultKind> {
        use SyntaxKind as K;
        let (_, wrapping) = self.width_at(body, span);
        let lk = self.arena.kind(lhs).clone();
        let rk = self.arena.kind(rhs).clone();
        let op_text = |op: K| -> &'static str {
            match op {
                K::Plus => "+",
                K::Minus => "-",
                K::Star => "*",
                K::Slash => "/",
                K::Percent => "%",
                K::Amp => "&",
                K::Pipe => "|",
                K::Caret => "^",
                K::Shl => "<<",
                K::Shr => ">>",
                K::EqEq => "==",
                K::NotEq => "!=",
                K::Lt => "<",
                K::Gt => ">",
                K::LtEq => "<=",
                K::GtEq => ">=",
                K::Spaceship => "<=>",
                _ => "?",
            }
        };
        match (lk, rk) {
            (ValueKind::Int { v: a, w }, ValueKind::Int { v: b, .. }) => match op {
                K::EqEq => Ok(self.arena.bool_(a == b)),
                K::NotEq => Ok(self.arena.bool_(a != b)),
                K::Lt => Ok(self.arena.bool_(a < b)),
                K::Gt => Ok(self.arena.bool_(a > b)),
                K::LtEq => Ok(self.arena.bool_(a <= b)),
                K::GtEq => Ok(self.arena.bool_(a >= b)),
                K::Spaceship => Ok(self.arena.int(
                    match a.cmp(&b) {
                        std::cmp::Ordering::Less => -1,
                        std::cmp::Ordering::Equal => 0,
                        std::cmp::Ordering::Greater => 1,
                    },
                    Prim::Int,
                )),
                K::Plus
                | K::Minus
                | K::Star
                | K::Slash
                | K::Percent
                | K::Amp
                | K::Pipe
                | K::Caret
                | K::Shl
                | K::Shr => {
                    let (lo, hi) = int_range(w);
                    let raw = match op {
                        K::Plus => a.checked_add(b),
                        K::Minus => a.checked_sub(b),
                        K::Star => a.checked_mul(b),
                        K::Slash => {
                            if b == 0 {
                                return Err(FaultKind::Arith {
                                    op: "/".to_string(),
                                    ty: w.name().to_string(),
                                    detail: "division by zero".to_string(),
                                });
                            }
                            a.checked_div(b)
                        }
                        K::Percent => {
                            if b == 0 {
                                return Err(FaultKind::Arith {
                                    op: "%".to_string(),
                                    ty: w.name().to_string(),
                                    detail: "division by zero".to_string(),
                                });
                            }
                            a.checked_rem(b)
                        }
                        K::Amp => Some(a & b),
                        K::Pipe => Some(a | b),
                        K::Caret => Some(a ^ b),
                        K::Shl | K::Shr => {
                            let width_bits = width_bits(w);
                            if b < 0 || b >= width_bits {
                                return Err(FaultKind::Arith {
                                    op: op_text(op).to_string(),
                                    ty: w.name().to_string(),
                                    detail: format!("shift by {b} exceeds the width"),
                                });
                            }
                            if op == K::Shl {
                                a.checked_shl(b as u32)
                            } else {
                                a.checked_shr(b as u32)
                            }
                        }
                        _ => None,
                    };
                    let Some(v) = raw else {
                        return Err(FaultKind::Arith {
                            op: op_text(op).to_string(),
                            ty: w.name().to_string(),
                            detail: "the result does not fit".to_string(),
                        });
                    };
                    if v < lo || v > hi {
                        if wrapping {
                            let span_w = (hi - lo) + 1;
                            let wrapped = ((v - lo).rem_euclid(span_w)) + lo;
                            return Ok(self.arena.int(wrapped, w));
                        }
                        return Err(FaultKind::Arith {
                            op: op_text(op).to_string(),
                            ty: w.name().to_string(),
                            detail: format!(
                                "{a} {} {b} leaves `{}`'s range",
                                op_text(op),
                                w.name()
                            ),
                        });
                    }
                    Ok(self.arena.int(v, w))
                }
                _ => Err(FaultKind::EngineGap {
                    construct: "this integer operator at comptime",
                }),
            },
            (ValueKind::Float { bits: ab, w }, ValueKind::Float { bits: bb, .. }) => {
                if w == Prim::F32 {
                    let (x, y) = (f32::from_bits(ab as u32), f32::from_bits(bb as u32));
                    self.float_op32(op, x, y)
                } else {
                    let (x, y) = (f64::from_bits(ab), f64::from_bits(bb));
                    self.float_op64(op, x, y)
                }
            }
            (ValueKind::Bool(a), ValueKind::Bool(b)) => match op {
                K::EqEq => Ok(self.arena.bool_(a == b)),
                K::NotEq => Ok(self.arena.bool_(a != b)),
                _ => Err(FaultKind::EngineGap {
                    construct: "this boolean operator at comptime",
                }),
            },
            (ValueKind::Str(a), ValueKind::Str(b)) => match op {
                K::EqEq => Ok(self.arena.bool_(a == b)),
                K::NotEq => Ok(self.arena.bool_(a != b)),
                _ => Err(FaultKind::EngineGap {
                    construct: "this string operator at comptime",
                }),
            },
            _ => Err(FaultKind::EngineGap {
                construct: "mixed operand values at comptime",
            }),
        }
    }

    fn float_op64(&mut self, op: SyntaxKind, x: f64, y: f64) -> Result<ValId, FaultKind> {
        use SyntaxKind as K;
        Ok(match op {
            K::Plus => self.arena.float(canon_f64(x + y), Prim::F64),
            K::Minus => self.arena.float(canon_f64(x - y), Prim::F64),
            K::Star => self.arena.float(canon_f64(x * y), Prim::F64),
            K::Slash => self.arena.float(canon_f64(x / y), Prim::F64),
            K::Percent => self.arena.float(canon_f64(x % y), Prim::F64),
            K::EqEq => self.arena.bool_(x == y),
            K::NotEq => self.arena.bool_(x != y),
            K::Lt => self.arena.bool_(x < y),
            K::Gt => self.arena.bool_(x > y),
            K::LtEq => self.arena.bool_(x <= y),
            K::GtEq => self.arena.bool_(x >= y),
            _ => {
                return Err(FaultKind::EngineGap {
                    construct: "this float operator at comptime",
                });
            }
        })
    }

    fn float_op32(&mut self, op: SyntaxKind, x: f32, y: f32) -> Result<ValId, FaultKind> {
        use SyntaxKind as K;
        Ok(match op {
            K::Plus => self.arena.float(canon_f32(x + y) as u64, Prim::F32),
            K::Minus => self.arena.float(canon_f32(x - y) as u64, Prim::F32),
            K::Star => self.arena.float(canon_f32(x * y) as u64, Prim::F32),
            K::Slash => self.arena.float(canon_f32(x / y) as u64, Prim::F32),
            K::Percent => self.arena.float(canon_f32(x % y) as u64, Prim::F32),
            K::EqEq => self.arena.bool_(x == y),
            K::NotEq => self.arena.bool_(x != y),
            K::Lt => self.arena.bool_(x < y),
            K::Gt => self.arena.bool_(x > y),
            K::LtEq => self.arena.bool_(x <= y),
            K::GtEq => self.arena.bool_(x >= y),
            _ => {
                return Err(FaultKind::EngineGap {
                    construct: "this float operator at comptime",
                });
            }
        })
    }

    fn prefix(
        &mut self,
        body: &(usize, String),
        op: SyntaxKind,
        v: ValId,
        span: Span,
    ) -> Result<ValId, FaultKind> {
        match (op, self.arena.kind(v).clone()) {
            (SyntaxKind::Minus, ValueKind::Int { v: x, w }) => {
                let (_, wrapping) = self.width_at(body, span);
                let (lo, hi) = int_range(w);
                let neg = -x;
                if neg < lo || neg > hi {
                    if wrapping {
                        let span_w = (hi - lo) + 1;
                        return Ok(self.arena.int(((neg - lo).rem_euclid(span_w)) + lo, w));
                    }
                    return Err(FaultKind::Arith {
                        op: "-".to_string(),
                        ty: w.name().to_string(),
                        detail: format!("-({x}) leaves `{}`'s range", w.name()),
                    });
                }
                Ok(self.arena.int(neg, w))
            }
            (SyntaxKind::Minus, ValueKind::Float { bits, w }) => {
                if w == Prim::F32 {
                    let x = f32::from_bits(bits as u32);
                    Ok(self.arena.float(canon_f32(-x) as u64, Prim::F32))
                } else {
                    let x = f64::from_bits(bits);
                    Ok(self.arena.float(canon_f64(-x), Prim::F64))
                }
            }
            (SyntaxKind::Not, ValueKind::Bool(b)) => Ok(self.arena.bool_(!b)),
            _ => Err(FaultKind::EngineGap {
                construct: "this prefix operator at comptime",
            }),
        }
    }
}

// -------------------------------------------------------- free helpers --

/// A width's bit count (shift-amount rail).
fn width_bits(w: Prim) -> i128 {
    match w {
        Prim::I8 | Prim::U8 | Prim::Byte => 8,
        Prim::I16 | Prim::U16 => 16,
        Prim::I32 | Prim::U32 => 32,
        _ => 64,
    }
}

fn backtrace(frames: &[Frame<'_>]) -> Vec<FrameNote> {
    frames
        .iter()
        .rev()
        .map(|f| FrameNote {
            name: f.fn_name.clone(),
            call_span: f.call_span,
        })
        .collect()
}

/// Resolve a recorded type to `(width prim, is wrapping)`.
fn resolve_width(tb: &TypedBody, ty: TyId) -> (Prim, bool) {
    match tb.table.kind(ty) {
        TyKind::Prim(p) => (*p, false),
        TyKind::Wrapping(inner) => match tb.table.kind(*inner) {
            TyKind::Prim(p) => (*p, true),
            _ => (Prim::Int, true),
        },
        _ => (Prim::Int, false),
    }
}

/// `const` locals of a body declared before `before` (byte offset),
/// name → initializer node — the lazy root evaluation environment.
/// Left-to-right order holds by construction: only consts textually
/// before the site are visible ([mem.model.order]).
fn collect_const_locals<'a>(
    node: &'a GreenNodeAlias,
    src: &[u8],
    before: u32,
    out: &mut HashMap<String, &'a GreenNodeAlias>,
) {
    let mut stack: Vec<&'a GreenNodeAlias> = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind == SyntaxKind::ConstDecl && n.span.lo < before {
            let d = ConstDecl::cast(n);
            if let (Some(nt), Some(init)) = (d.and_then(|c| c.name()), d.and_then(|c| c.init())) {
                let name = String::from_utf8_lossy(&src[nt.span.lo as usize..nt.span.hi as usize])
                    .into_owned();
                out.entry(name).or_insert(init);
            }
        }
        for c in n.nodes() {
            stack.push(c);
        }
    }
}

fn parse_int(text: &str) -> Option<i128> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i128::from_str_radix(hex, 16).ok();
    }
    if let Some(bin) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return i128::from_str_radix(bin, 2).ok();
    }
    if let Some(oct) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return i128::from_str_radix(oct, 8).ok();
    }
    t.parse::<i128>().ok()
}

/// Strip quotes and process escapes of a string literal's source text.
/// Rebuild an interpolated single-line string at comptime (s71):
/// literal bytes copied as-is, escapes decoded exactly as the checked
/// executor decodes them, rendered hole text spliced at each hole's
/// span. Code-point escapes (`\xNN`, `\u{…}`) stay a gap here — the
/// checked lane owns their decoding and a silent mismatch would be a
/// fold that lies.
fn splice_interp(
    raw: &str,
    base: u32,
    holes: &[Span],
    rendered: &[String],
) -> Result<String, &'static str> {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let (start, end) = if bytes.len() >= 2 {
        (1usize, bytes.len() - 1)
    } else {
        (0, bytes.len())
    };
    let mut i = start;
    while i < end {
        if let Some(k) = holes.iter().position(|h| (h.lo - base) as usize == i) {
            out.extend_from_slice(rendered[k].as_bytes());
            i = (holes[k].hi - base) as usize;
            continue;
        }
        let c = bytes[i];
        if c == b'\\' && i + 1 < end {
            let esc = bytes[i + 1];
            if esc == b'x' || esc == b'u' {
                return Err("code-point escapes in an interpolated string at comptime");
            }
            out.push(match esc {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'\\' => b'\\',
                b'"' => b'"',
                b'{' => b'{',
                b'}' => b'}',
                b'0' => b'\0',
                other => other,
            });
            i += 2;
            continue;
        }
        // `{{` / `}}` are literal braces ([gram.lex.str]).
        if (c == b'{' || c == b'}') && i + 1 < end && bytes[i + 1] == c {
            out.push(c);
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn unquote(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('0') => out.push('\0'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The single-identifier name of a binding pattern, if it is one.
fn single_ident_name(pat: &GreenNodeAlias, src: &[u8]) -> Option<String> {
    if pat.kind != SyntaxKind::IdentPat {
        return None;
    }
    let t = pat.child_token(SyntaxKind::Ident)?;
    Some(String::from_utf8_lossy(&src[t.span.lo as usize..t.span.hi as usize]).into_owned())
}
