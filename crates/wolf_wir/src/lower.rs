//! Typed HIR → WIR lowering (s25): sema's typed AST in, verified SSA
//! out, in one forward walk through [`crate::build::FuncBuilder`].
//!
//! The s25 surface (the Braun proving ground): scalar expression
//! bodies, `let`/`var`/assignment, direct calls, `if`/`else` as
//! expressions (result as block parameter), `while`/`loop` with
//! `break`/`continue`, short-circuit `&&`/`||`/`!`, `assert`, numeric
//! and adapter casts, and checked arithmetic to `.chk` ops (X3;
//! `wrapping[T]` to `.wrap`). Everything else refuses with an honest
//! [`NotYet`] naming its owning sprint — regions/aggregates/facts are
//! s26, error unions/`defer`/`match`/`for` are s27, closures and
//! concurrency are c05. A body is either fully lowered or refused,
//! never half-guessed (the conservatism-ledger contract).
//!
//! Error-union RETURN TYPES with a statically empty row (`fn main() ->
//! !int` with a sealed empty row) lower as their ok type: the error
//! case is uninhabited, so ok-injection is the identity — no `eu.*`
//! op is emitted or needed before s27.
//!
//! `mut`/`take` parameter MODES lower into the WIR signature (the s26
//! fact vocabulary attaches to those slots then); what refuses today
//! is the part with missing semantics: passing a `mut` argument and
//! writing through a `mut` parameter are s26's pointer-shaped lowering.
//!
//! Coercions from the s17 closed set need no ops at this surface:
//! ok-injection into an empty row and `NeverToAny` are identities;
//! `RowWiden` only occurs on rows with tags, which refuse as s27.

use std::collections::HashMap;

use wolf_ast::{
    Arg, AssignStmt, Block as AstBlock, BreakExpr, CallExpr, CastExpr, ConstDecl, ExprStmt,
    GreenNode, IfExpr, LetDecl, LoopExpr, ParamMode, ParenExpr, PrefixExpr, ReturnExpr, SyntaxKind,
    VarDecl, WhileExpr,
};
use wolf_sema::check::{CallSig, CastKind};
use wolf_sema::sig::{FnSig, ItemSig, SigTables};
use wolf_sema::types::{Prim, TyId, TyKind, TypeTable};
use wolf_sema::{BodyResult, NotYet, Package, Typecheck, TypedBody};
use wolf_span::Span;

use crate::build::{FuncBuilder, InsOut, Stats, Var};
use crate::ir::{Aux, Block, ExtFunc, Mode, Module, Param, SigId};
use crate::ops::{FloatCc, IntCc, Opcode};
use crate::types::{self, TypeId};

type R<T> = Result<T, NotYet>;

/// The result of lowering one package.
pub struct Build {
    /// The WIR module: one function per lowered body, in body order.
    pub module: Module,
    /// Honest refusals, in body order. Empty ⇔ every checked body
    /// lowered (the `wir` rung's completion condition).
    pub not_yet: Vec<NotYet>,
    /// Summed peephole counters (`--zstats`, the `wir-build` bench).
    pub stats: Stats,
}

/// Lower every checked body of a package. `tc` must come from
/// [`wolf_sema::typecheck_package`] over the same `pkg`; callers gate
/// on `tc.not_yet`/`mem` cleanliness for the rung verdict — this
/// function lowers whatever is lowerable and refuses the rest.
pub fn lower_package(pkg: &Package, tc: &Typecheck) -> Build {
    let mut module = Module::new();
    let mut not_yet = Vec::new();
    let mut stats = Stats::default();
    // Callee resolution: fn name → its unique (module, sig); names
    // declared in more than one module refuse at the call site.
    let mut fns: HashMap<&str, Vec<(usize, &FnSig)>> = HashMap::new();
    for (m, items) in tc.sigs.modules.iter().enumerate() {
        for (name, sig) in items {
            if let ItemSig::Fn(f) = sig {
                fns.entry(name.as_str()).or_default().push((m, f));
            }
        }
    }
    let mut sig_cache: HashMap<String, SigId> = HashMap::new();
    for outcome in &tc.bodies {
        let BodyResult::Checked(tb) = &outcome.result else {
            continue;
        };
        let body = &outcome.body;
        match lower_body(pkg, &tc.sigs, tb, body, &fns, &mut module, &mut sig_cache) {
            Ok(Some(s)) => stats.add(s),
            Ok(None) => {}
            Err(nyc) => not_yet.push(nyc),
        }
    }
    Build {
        module,
        not_yet,
        stats,
    }
}

/// Lower one checked body. `Ok(None)`: nothing to lower (bodyless
/// item, broken tree — not a refusal).
#[allow(clippy::too_many_arguments)]
fn lower_body(
    pkg: &Package,
    sigs: &SigTables,
    tb: &TypedBody,
    body: &wolf_sema::BodyRef,
    fns: &HashMap<&str, Vec<(usize, &FnSig)>>,
    module: &mut Module,
    sig_cache: &mut HashMap<String, SigId>,
) -> R<Option<Stats>> {
    let root = &pkg.files[body.file].parse.root;
    let Some(node) = root.nodes().filter(|n| n.kind.is_item()).nth(body.decl) else {
        return Ok(None);
    };
    let span = node.span;
    if body.member.is_some() {
        return Err(refuse("method body lowering (receiver modes, s26)", span));
    }
    match node.kind {
        SyntaxKind::FnDecl => {}
        SyntaxKind::LetDecl | SyntaxKind::VarDecl | SyntaxKind::ConstDecl => {
            return Err(refuse("item-initializer lowering (globals, s26)", span));
        }
        _ => return Ok(None),
    }
    let d = wolf_ast::FnDecl::cast(node).expect("kind");
    let Some(block) = d.body() else {
        return Ok(None);
    };
    let Some(ItemSig::Fn(fsig)) = sigs.get(body.module, &body.name) else {
        return Ok(None);
    };
    if !fsig.generics.is_empty() {
        return Err(refuse("generic-function lowering (monomorphization)", span));
    }
    if fsig.comptime {
        return Err(refuse(
            "comptime-function lowering (D29 CTFE owns these)",
            span,
        ));
    }
    if fns.get(body.name.as_str()).map(|v| v.len()).unwrap_or(0) > 1 {
        return Err(refuse(
            "two modules declare a function with this name (WIR name mangling)",
            span,
        ));
    }
    // The WIR signature (modes carried; s26 attaches the fact slots).
    let sig = wir_fn_sig(module, sig_cache, sigs, &body.name, fsig, span)?;
    let mut b = FuncBuilder::new(module, body.name.clone(), sig);
    let mut lowerer = Lowerer {
        src: &pkg.files[body.file].raw.src,
        table: &tb.table,
        sig_table: &sigs.table,
        sigs,
        calls: tb.calls.iter().map(|(s, c)| (*s, c)).collect(),
        expr_tys: tb.exprs.iter().map(|(s, t)| (*s, *t)).collect(),
        local_tys: tb.locals.iter().map(|(_, s, t)| (*s, *t)).collect(),
        casts: tb
            .casts
            .iter()
            .map(|(s, f, t, k)| (*s, (*f, *t, *k)))
            .collect(),
        fns,
        scopes: Vec::new(),
        loops: Vec::new(),
        callees: HashMap::new(),
        straight_line: false,
        b: &mut b,
    };
    lowerer.lower_fn(fsig, block)?;
    let stats = b.stats;
    let func = b.finish();
    module.add_func(func);
    Ok(Some(stats))
}

fn refuse(construct: &'static str, span: Span) -> NotYet {
    NotYet { construct, span }
}

/// Map one sema type to a WIR type. `Ok(None)` is the unit/never
/// "no value" case; unsupported types refuse. `sigs` resolves nominal
/// ADAPTER types (`type X = distinct B` — layout identity, D28) to
/// their base scalar; their `base` id lives in the signature table.
fn wir_ty(table: &TypeTable, sigs: &SigTables, id: TyId, span: Span) -> R<Option<TypeId>> {
    match table.kind(id) {
        TyKind::Unit | TyKind::Never => Ok(None),
        TyKind::Prim(p) => match p {
            Prim::Bool => Ok(Some(types::BOOL)),
            Prim::I8 => Ok(Some(types::I8)),
            Prim::I16 => Ok(Some(types::I16)),
            Prim::I32 => Ok(Some(types::I32)),
            Prim::I64 | Prim::Int => Ok(Some(types::I64)),
            Prim::F32 => Ok(Some(types::F32)),
            Prim::F64 => Ok(Some(types::F64)),
            Prim::Uint | Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64 => Err(refuse(
                "unsigned-integer lowering (checked unsigned ops are an op-set \
                 decision for s26)",
                span,
            )),
            Prim::Str | Prim::Byte => Err(refuse("string/byte lowering (s26 memory)", span)),
        },
        TyKind::Wrapping(inner) => match wir_ty(table, sigs, *inner, span)? {
            Some(t) if types_is_int(t) => Ok(Some(t)),
            _ => Err(refuse("wrapping over a non-signed-integer type", span)),
        },
        TyKind::Distinct(inner) => wir_ty(table, sigs, *inner, span),
        TyKind::ErrUnion(ok, row) => {
            if row_is_empty(table, *row) {
                wir_ty(table, sigs, *ok, span)
            } else {
                Err(refuse(
                    "error-union lowering (eu.* control flow, s27)",
                    span,
                ))
            }
        }
        TyKind::Nominal { module, name } => {
            // Adapter types are scalars in disguise (layout identity);
            // struct/enum nominals are s26 aggregates.
            match sigs.get(*module as usize, name) {
                Some(ItemSig::Distinct { base, .. }) => wir_ty(&sigs.table, sigs, *base, span),
                _ => Err(refuse("aggregate lowering (s26)", span)),
            }
        }
        TyKind::Tuple(_) => Err(refuse("aggregate lowering (s26)", span)),
        TyKind::Range(_) => Err(refuse("range-value lowering (for/ranges, s27)", span)),
        TyKind::Shared(_)
        | TyKind::Weak(_)
        | TyKind::Handle(_)
        | TyKind::List(_)
        | TyKind::Pool(_) => Err(refuse("shared-tier lowering (rc.*, s26)", span)),
        TyKind::Ptr(_) => Err(refuse("raw-pointer lowering (unsafe tier, s26)", span)),
        _ => Err(refuse("this type in WIR lowering", span)),
    }
}

fn types_is_int(t: TypeId) -> bool {
    matches!(t, types::I8 | types::I16 | types::I32 | types::I64)
}

fn row_is_empty(table: &TypeTable, row: TyId) -> bool {
    matches!(
        table.kind(row),
        TyKind::Row { tags, tail } if tags.is_empty() && tail.is_none()
    )
}

/// Build (and cache) the WIR signature for a fn item.
fn wir_fn_sig(
    module: &mut Module,
    cache: &mut HashMap<String, SigId>,
    sigs: &SigTables,
    name: &str,
    fsig: &FnSig,
    span: Span,
) -> R<SigId> {
    if let Some(&sig) = cache.get(name) {
        return Ok(sig);
    }
    let mut params = Vec::with_capacity(fsig.params.len());
    for p in &fsig.params {
        let Some(ty) = wir_ty(&sigs.table, sigs, p.ty, p.span)? else {
            return Err(refuse("unit-typed parameters", p.span));
        };
        let mode = match p.mode {
            None => Mode::Val,
            Some(ParamMode::Mut) => Mode::Mut,
            Some(ParamMode::Take) => Mode::Take,
        };
        params.push(Param { ty, mode });
    }
    let results = match wir_ty(&sigs.table, sigs, fsig.ret, span)? {
        Some(t) => vec![t],
        None => vec![],
    };
    let sig = module.make_sig(params, results);
    cache.insert(name.to_string(), sig);
    Ok(sig)
}

/// Control-flow result of lowering one expression/statement.
#[derive(Clone, Copy, Debug)]
enum Flow {
    /// Evaluation fell through with a value (`None` = unit).
    Val(Option<Value>),
    /// Control diverged (return/break/continue/provable trap): the
    /// current block is filled; stop emitting on this path.
    Diverged,
}

use crate::ir::Value;

macro_rules! flow_val {
    ($e:expr) => {
        match $e? {
            Flow::Val(v) => v,
            Flow::Diverged => return Ok(Flow::Diverged),
        }
    };
}

/// One name binding in scope.
#[derive(Clone, Copy)]
enum LocalBind {
    Val {
        var: Var,
        /// The binding is `wrapping[T]`-typed (compound assignment
        /// selects `.wrap` ops). Computed at bind time against the
        /// binding's OWN interner — sig and body tables never mix.
        wrapping: bool,
        wir_ty: TypeId,
        /// Bound from a `mut`-mode parameter: writes refuse (s26
        /// write-back).
        mut_param: bool,
    },
    /// A unit-typed binding (no runtime value).
    Unit,
}

struct LoopFrame {
    header: Block,
    exit: Option<Block>,
}

struct Lowerer<'t, 'b, 'm> {
    src: &'t [u8],
    /// The body's own type table.
    table: &'t TypeTable,
    /// The signature table's interner (param/return types live there).
    sig_table: &'t TypeTable,
    /// The whole signature set (adapter-type base resolution).
    sigs: &'t SigTables,
    calls: HashMap<Span, &'t CallSig>,
    expr_tys: HashMap<Span, TyId>,
    local_tys: HashMap<Span, TyId>,
    casts: HashMap<Span, (TyId, TyId, CastKind)>,
    fns: &'t HashMap<&'t str, Vec<(usize, &'t FnSig)>>,
    scopes: Vec<Vec<(String, LocalBind)>>,
    loops: Vec<LoopFrame>,
    /// Per-function callee import cache.
    callees: HashMap<String, ExtFunc>,
    /// Semi-pruned pre-scan verdict: a function whose body contains no
    /// control construct is one block, so every variable is
    /// single-block and bypasses the global Braun maps.
    straight_line: bool,
    b: &'b mut FuncBuilder<'m>,
}

impl<'t, 'b, 'm> Lowerer<'t, 'b, 'm> {
    fn text(&self, span: Span) -> String {
        String::from_utf8_lossy(&self.src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn expr_sema_ty(&self, span: Span) -> Option<TyId> {
        self.expr_tys.get(&span).copied()
    }

    fn lower_fn(&mut self, fsig: &FnSig, block: AstBlock<'t>) -> R<()> {
        self.straight_line = !contains_control(block.syntax());
        // Prologue: signature params are the entry block's params.
        let entry_params = self.b.block_params(self.b.current_block());
        self.scopes.push(Vec::new());
        for (i, p) in fsig.params.iter().enumerate() {
            let Some(wty) = wir_ty(self.sig_table, self.sigs, p.ty, p.span)? else {
                unreachable!("unit params refused at sig build");
            };
            let var = self.b.declare_var(wty);
            if self.straight_line {
                self.b.mark_single_block(var);
            }
            self.b.def_var(var, entry_params[i]);
            self.scopes.last_mut().expect("scope").push((
                p.name.clone(),
                LocalBind::Val {
                    var,
                    wrapping: matches!(self.sig_table.kind(p.ty), TyKind::Wrapping(_)),
                    wir_ty: wty,
                    mut_param: p.mode == Some(ParamMode::Mut),
                },
            ));
        }
        let ret = wir_ty(self.sig_table, self.sigs, fsig.ret, fsig.name_span)?;
        match self.lower_block(block, ret.is_some())? {
            Flow::Diverged => {}
            Flow::Val(v) => match (ret, v) {
                (Some(_), Some(val)) => self.b.ins_ret(&[val]),
                (Some(_), None) => {
                    // Typed return with a unit trailing value: the
                    // checker guarantees this cannot happen on a
                    // fall-through path of a value-returning fn.
                    return Err(refuse(
                        "fall-through without a return value",
                        fsig.name_span,
                    ));
                }
                (None, _) => self.b.ins_ret(&[]),
            },
        }
        self.scopes.pop();
        Ok(())
    }

    // ---------------------------------------------------- blocks ----

    fn lower_block(&mut self, block: AstBlock<'t>, want_value: bool) -> R<Flow> {
        self.scopes.push(Vec::new());
        let last_value = if want_value {
            block.trailing_expr().map(|e| e.span)
        } else {
            None
        };
        let mut out: Option<Value> = None;
        for stmt in block.statements() {
            let flow = self.lower_stmt(stmt, last_value, &mut out);
            match flow {
                Ok(Flow::Val(_)) => {}
                Ok(Flow::Diverged) => {
                    self.scopes.pop();
                    return Ok(Flow::Diverged);
                }
                Err(e) => {
                    self.scopes.pop();
                    return Err(e);
                }
            }
        }
        self.scopes.pop();
        Ok(Flow::Val(out))
    }

    fn lower_stmt(
        &mut self,
        stmt: &'t GreenNode,
        last_value: Option<Span>,
        out: &mut Option<Value>,
    ) -> R<Flow> {
        match stmt.kind {
            SyntaxKind::ExprStmt => {
                let d = ExprStmt::cast(stmt).expect("kind");
                if let Some(e) = d.expr() {
                    let wanted = Some(e.span) == last_value;
                    let v = flow_val!(self.lower_expr_w(e, wanted));
                    if wanted {
                        *out = v;
                    }
                }
                Ok(Flow::Val(None))
            }
            SyntaxKind::LetDecl => {
                let d = LetDecl::cast(stmt).expect("kind");
                self.lower_binding(d.pattern(), d.init(), stmt.span)
            }
            SyntaxKind::VarDecl => {
                let d = VarDecl::cast(stmt).expect("kind");
                self.lower_binding(d.pattern(), d.init(), stmt.span)
            }
            SyntaxKind::ConstDecl => {
                let d = ConstDecl::cast(stmt).expect("kind");
                let name = d.name().map(|t| t.span);
                self.lower_binding_named(name, d.init(), stmt.span)
            }
            SyntaxKind::AssignStmt => self.lower_assign(stmt),
            SyntaxKind::DeferStmt => Err(refuse(
                "defer/errdefer lowering (strict LIFO, s27)",
                stmt.span,
            )),
            SyntaxKind::AssumeStmt => Err(refuse("assume noalias (unsafe tier, s26)", stmt.span)),
            k if k.is_item() => Err(refuse("nested item declarations", stmt.span)),
            _ => Ok(Flow::Val(None)),
        }
    }

    fn lower_binding(
        &mut self,
        pat: Option<&'t GreenNode>,
        init: Option<&'t GreenNode>,
        span: Span,
    ) -> R<Flow> {
        let Some(pat) = pat else {
            return Ok(Flow::Val(None));
        };
        match pat.kind {
            SyntaxKind::IdentPat => self.lower_binding_named(Some(pat.span), init, span),
            SyntaxKind::WildcardPat => {
                if let Some(e) = init {
                    flow_val!(self.lower_expr(e));
                }
                Ok(Flow::Val(None))
            }
            _ => Err(refuse("destructuring bindings (aggregates, s26)", pat.span)),
        }
    }

    fn lower_binding_named(
        &mut self,
        name_span: Option<Span>,
        init: Option<&'t GreenNode>,
        span: Span,
    ) -> R<Flow> {
        let Some(init) = init else {
            return Err(refuse(
                "uninitialized bindings (definite-assignment lowering)",
                span,
            ));
        };
        let v = flow_val!(self.lower_expr(init));
        let Some(name_span) = name_span else {
            return Ok(Flow::Val(None));
        };
        let name = self.text(name_span);
        let sema_ty = self
            .local_tys
            .get(&name_span)
            .copied()
            .or_else(|| self.expr_sema_ty(init.span));
        let Some(sema_ty) = sema_ty else {
            return Err(refuse("a binding without a recorded type", span));
        };
        let bind = match (wir_ty(self.table, self.sigs, sema_ty, span)?, v) {
            (Some(wty), Some(val)) => {
                let var = self.b.declare_var(wty);
                if self.straight_line {
                    self.b.mark_single_block(var);
                }
                self.b.def_var(var, val);
                LocalBind::Val {
                    var,
                    wrapping: matches!(self.table.kind(sema_ty), TyKind::Wrapping(_)),
                    wir_ty: wty,
                    mut_param: false,
                }
            }
            (None, _) => LocalBind::Unit,
            (Some(_), None) => {
                return Err(refuse("a typed binding of a valueless expression", span));
            }
        };
        self.scopes.last_mut().expect("scope").push((name, bind));
        Ok(Flow::Val(None))
    }

    fn lookup(&self, name: &str) -> Option<LocalBind> {
        for scope in self.scopes.iter().rev() {
            for (n, b) in scope.iter().rev() {
                if n == name {
                    return Some(*b);
                }
            }
        }
        None
    }

    fn lower_assign(&mut self, stmt: &'t GreenNode) -> R<Flow> {
        let d = AssignStmt::cast(stmt).expect("kind");
        let Some(place) = d.place() else {
            return Ok(Flow::Val(None));
        };
        if place.kind != SyntaxKind::PathExpr {
            return Err(refuse(
                "assignment through non-local places (s26)",
                place.span,
            ));
        }
        let name = self.text(place.span);
        let (var, wrapping, wty, mut_param) = match self.lookup(&name) {
            Some(LocalBind::Val {
                var,
                wrapping,
                wir_ty,
                mut_param,
            }) => (var, wrapping, wir_ty, mut_param),
            Some(LocalBind::Unit) => {
                // Unit-typed assignment: evaluate for effect.
                if let Some(e) = d.value() {
                    flow_val!(self.lower_expr(e));
                }
                return Ok(Flow::Val(None));
            }
            None => {
                return Err(refuse(
                    "assignment to a non-local name (globals, s26)",
                    place.span,
                ));
            }
        };
        if mut_param {
            return Err(refuse(
                "writes through `mut` parameters (pointer-shaped lowering, s26)",
                place.span,
            ));
        }
        let Some(value_expr) = d.value() else {
            return Ok(Flow::Val(None));
        };
        let rhs = flow_val!(self.lower_expr(value_expr));
        let Some(rhs) = rhs else {
            return Err(refuse(
                "assignment of a valueless expression",
                value_expr.span,
            ));
        };
        let op = d.op().map(|t| t.kind).unwrap_or(SyntaxKind::Eq);
        let newval = if op == SyntaxKind::Eq {
            rhs
        } else {
            let cur = self.b.use_var(var);
            let bin = match op {
                SyntaxKind::PlusEq => SyntaxKind::Plus,
                SyntaxKind::MinusEq => SyntaxKind::Minus,
                SyntaxKind::StarEq => SyntaxKind::Star,
                SyntaxKind::SlashEq => SyntaxKind::Slash,
                SyntaxKind::PercentEq => SyntaxKind::Percent,
                SyntaxKind::AmpEq => SyntaxKind::Amp,
                SyntaxKind::PipeEq => SyntaxKind::Pipe,
                SyntaxKind::CaretEq => SyntaxKind::Caret,
                SyntaxKind::ShlEq => SyntaxKind::Shl,
                SyntaxKind::ShrEq => SyntaxKind::Shr,
                _ => return Err(refuse("this compound assignment operator", stmt.span)),
            };
            match self.arith(bin, cur, rhs, wrapping, wty, stmt.span)? {
                Some(v) => v,
                None => return Ok(Flow::Diverged),
            }
        };
        self.b.def_var(var, newval);
        Ok(Flow::Val(None))
    }

    // ----------------------------------------------- expressions ----

    fn lower_expr(&mut self, e: &'t GreenNode) -> R<Flow> {
        self.lower_expr_w(e, true)
    }

    /// `want`: the surrounding context consumes the value. Only block
    /// and if lowering care — an else-if chain inherits the demand even
    /// when sema recorded no type for the nested if node.
    fn lower_expr_w(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        match e.kind {
            SyntaxKind::LiteralExpr => self.lower_literal(e),
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.lower_expr(inner),
                None => Ok(Flow::Val(None)),
            },
            SyntaxKind::PathExpr => {
                let name = self.text(e.span);
                match self.lookup(&name) {
                    Some(LocalBind::Val { var, .. }) => Ok(Flow::Val(Some(self.b.use_var(var)))),
                    Some(LocalBind::Unit) => Ok(Flow::Val(None)),
                    None => Err(refuse("module-item reads (globals, s26)", e.span)),
                }
            }
            SyntaxKind::Block => {
                let d = AstBlock::cast(e).expect("kind");
                self.lower_block(d, want)
            }
            SyntaxKind::PrefixExpr => self.lower_prefix(e),
            SyntaxKind::BinExpr => self.lower_bin(e),
            SyntaxKind::CastExpr => self.lower_cast(e),
            SyntaxKind::IfExpr => self.lower_if(e, want),
            SyntaxKind::WhileExpr => self.lower_while(e),
            SyntaxKind::LoopExpr => self.lower_loop(e),
            SyntaxKind::CallExpr => self.lower_call(e),
            SyntaxKind::ReturnExpr => {
                let d = ReturnExpr::cast(e).expect("kind");
                let v = match d.value() {
                    Some(x) => flow_val!(self.lower_expr(x)),
                    None => None,
                };
                match v {
                    Some(val) => self.b.ins_ret(&[val]),
                    None => self.b.ins_ret(&[]),
                }
                Ok(Flow::Diverged)
            }
            SyntaxKind::BreakExpr => {
                let d = BreakExpr::cast(e).expect("kind");
                if d.value().is_some() {
                    return Err(refuse("break-with-value (loop results, s27)", e.span));
                }
                if self.loops.is_empty() {
                    return Err(refuse("break outside a loop", e.span));
                }
                // The exit block is created lazily so a break-less
                // `loop` has no unreachable exit.
                let exit = match self.loops.last().expect("frame").exit {
                    Some(x) => x,
                    None => {
                        let x = self.b.create_block();
                        self.loops.last_mut().expect("frame").exit = Some(x);
                        x
                    }
                };
                self.b.ins_jmp(exit, &[]);
                Ok(Flow::Diverged)
            }
            SyntaxKind::ContinueExpr => {
                let Some(frame) = self.loops.last() else {
                    return Err(refuse("continue outside a loop", e.span));
                };
                let header = frame.header;
                self.b.ins_jmp(header, &[]);
                Ok(Flow::Diverged)
            }
            SyntaxKind::TryExpr => Err(refuse("`?` propagation (eu.* control flow, s27)", e.span)),
            SyntaxKind::ElseExpr => {
                Err(refuse("`else` defaulting (eu.* control flow, s27)", e.span))
            }
            SyntaxKind::MatchExpr => Err(refuse("match lowering (decision trees, s27)", e.span)),
            SyntaxKind::ForExpr => Err(refuse("for-loop lowering (ranges, s27)", e.span)),
            SyntaxKind::StringExpr => Err(refuse("string lowering (s26 memory)", e.span)),
            SyntaxKind::StructLit | SyntaxKind::TupleExpr | SyntaxKind::MemberExpr => {
                Err(refuse("aggregate lowering (s26)", e.span))
            }
            SyntaxKind::BracketApply => Err(refuse("indexing lowering (s26)", e.span)),
            SyntaxKind::RangeExpr | SyntaxKind::FromEndExpr => {
                Err(refuse("range-value lowering (s27)", e.span))
            }
            SyntaxKind::RegionBlock
            | SyntaxKind::RegionValue
            | SyntaxKind::InBlock
            | SyntaxKind::FreezeExpr => Err(refuse("region lowering (region.*, s26)", e.span)),
            SyntaxKind::UnsafeBlock | SyntaxKind::BorrowExpr => {
                Err(refuse("unsafe-tier lowering (s26)", e.span))
            }
            SyntaxKind::ClosureExpr => Err(refuse("closure lowering (c05)", e.span)),
            SyntaxKind::ScopeExpr
            | SyntaxKind::SelectExpr
            | SyntaxKind::WhenExpr
            | SyntaxKind::SpawnExpr => Err(refuse("concurrency lowering (c05)", e.span)),
            SyntaxKind::InlineC | SyntaxKind::AsmExpr => Err(refuse("inline C/asm (c10)", e.span)),
            _ => Err(refuse("this expression shape in WIR lowering", e.span)),
        }
    }

    fn lower_literal(&mut self, e: &'t GreenNode) -> R<Flow> {
        let text = self.text(e.span);
        if text == "true" {
            return Ok(Flow::Val(Some(self.b.bconst(true))));
        }
        if text == "false" {
            return Ok(Flow::Val(Some(self.b.bconst(false))));
        }
        let Some(sema_ty) = self.expr_sema_ty(e.span) else {
            return Err(refuse("a literal without a recorded type", e.span));
        };
        let Some(wty) = wir_ty(self.table, self.sigs, sema_ty, e.span)? else {
            return Err(refuse("a unit-typed literal", e.span));
        };
        if wty == types::F32 || wty == types::F64 {
            let Some(v) = parse_float_literal(&text) else {
                return Err(refuse("this float literal shape", e.span));
            };
            let bits = if wty == types::F32 {
                (v as f32).to_bits() as u64
            } else {
                v.to_bits()
            };
            return Ok(Flow::Val(Some(self.b.fconst(wty, bits))));
        }
        let Some(n) = parse_int_literal(&text) else {
            return Err(refuse("this literal shape in WIR lowering", e.span));
        };
        Ok(Flow::Val(Some(self.b.iconst(wty, n))))
    }

    fn lower_prefix(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = PrefixExpr::cast(e).expect("kind");
        let Some(operand) = d.operand() else {
            return Ok(Flow::Val(None));
        };
        match d.op().map(|t| t.kind) {
            Some(SyntaxKind::Minus) => {
                let v = flow_val!(self.lower_expr(operand));
                let Some(v) = v else {
                    return Err(refuse("negation of a valueless expression", e.span));
                };
                let ty = self.b.func.value_ty(v);
                if ty == types::F32 || ty == types::F64 {
                    return Ok(Flow::Val(Some(
                        self.b.ins(Opcode::Fneg, &[v], &[ty], Aux::None).one(),
                    )));
                }
                // A constant operand folds directly (no dead zero
                // const left behind by the speculative 0 - x shape).
                if let Some(n) = self.b.as_int_const(v) {
                    self.b.stats.fold += 1;
                    let neg = -(n as i128);
                    let (lo, hi) = self
                        .b
                        .module
                        .types
                        .int_bounds(ty)
                        .expect("negation folds on integers");
                    return if neg < lo || neg > hi {
                        // Negating MIN traps in checked arithmetic (X3).
                        self.b.ins_trap();
                        Ok(Flow::Diverged)
                    } else {
                        Ok(Flow::Val(Some(self.b.iconst(ty, neg as i64))))
                    };
                }
                let zero = self.b.iconst(ty, 0);
                match self.b.ins(Opcode::IsubChk, &[zero, v], &[ty], Aux::None) {
                    InsOut::Vals(r) => Ok(Flow::Val(Some(r[0]))),
                    InsOut::Trapped => Ok(Flow::Diverged),
                }
            }
            Some(SyntaxKind::Not) => {
                let v = flow_val!(self.lower_expr(operand));
                let Some(v) = v else {
                    return Err(refuse("`!` of a valueless expression", e.span));
                };
                if let Some(c) = self.b.as_bool_const(v) {
                    self.b.stats.fold += 1;
                    return Ok(Flow::Val(Some(self.b.bconst(!c))));
                }
                // No boolean-not op exists: route through a two-edge
                // branch into one merge param (br const → jmp keeps
                // this clean when the operand later folds).
                let t = self.b.bconst(true);
                let f = self.b.bconst(false);
                let merge = self.b.create_block();
                let out = self.b.add_block_param(merge, types::BOOL);
                self.b.ins_br(v, merge, &[f], merge, &[t]);
                self.b.seal_block(merge);
                self.b.switch_to_block(merge);
                Ok(Flow::Val(Some(out)))
            }
            Some(SyntaxKind::CopyKw) | Some(SyntaxKind::MoveKw) => self.lower_expr(operand),
            Some(SyntaxKind::SharedKw) => Err(refuse("shared-cell lowering (rc.*, s26)", e.span)),
            Some(SyntaxKind::Amp) | Some(SyntaxKind::Star) => {
                Err(refuse("borrow/deref lowering (s26)", e.span))
            }
            _ => self.lower_expr(operand),
        }
    }

    fn lower_bin(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = wolf_ast::BinExpr::cast(e).expect("kind");
        let op = d.op().map(|t| t.kind);
        if matches!(op, Some(SyntaxKind::AmpAmp | SyntaxKind::PipePipe)) {
            return self.lower_short_circuit(d, op == Some(SyntaxKind::AmpAmp), e.span);
        }
        let lhs = match d.lhs() {
            Some(l) => flow_val!(self.lower_expr(l)),
            None => None,
        };
        let rhs = match d.rhs() {
            Some(r) => flow_val!(self.lower_expr(r)),
            None => None,
        };
        let (Some(a), Some(bv)) = (lhs, rhs) else {
            return Err(refuse("operator on valueless operands", e.span));
        };
        let Some(op) = op else {
            return Ok(Flow::Val(Some(a)));
        };
        match op {
            SyntaxKind::Plus
            | SyntaxKind::Minus
            | SyntaxKind::Star
            | SyntaxKind::Slash
            | SyntaxKind::Percent
            | SyntaxKind::Amp
            | SyntaxKind::Pipe
            | SyntaxKind::Caret
            | SyntaxKind::Shl
            | SyntaxKind::Shr => {
                let Some(sema_ty) = self.expr_sema_ty(e.span) else {
                    return Err(refuse("an operator without a recorded type", e.span));
                };
                let Some(wty) = wir_ty(self.table, self.sigs, sema_ty, e.span)? else {
                    return Err(refuse("a unit-typed operator", e.span));
                };
                let wrapping = matches!(self.table.kind(sema_ty), TyKind::Wrapping(_));
                match self.arith(op, a, bv, wrapping, wty, e.span)? {
                    Some(v) => Ok(Flow::Val(Some(v))),
                    None => Ok(Flow::Diverged),
                }
            }
            SyntaxKind::EqEq
            | SyntaxKind::NotEq
            | SyntaxKind::Lt
            | SyntaxKind::Gt
            | SyntaxKind::LtEq
            | SyntaxKind::GtEq => {
                let ty = self.b.func.value_ty(a);
                if ty == types::F32 || ty == types::F64 {
                    let cc = match op {
                        SyntaxKind::EqEq => FloatCc::Eq,
                        SyntaxKind::NotEq => FloatCc::Ne,
                        SyntaxKind::Lt => FloatCc::Lt,
                        SyntaxKind::Gt => FloatCc::Gt,
                        SyntaxKind::LtEq => FloatCc::Le,
                        _ => FloatCc::Ge,
                    };
                    return Ok(Flow::Val(Some(
                        self.b
                            .ins(Opcode::Fcmp, &[a, bv], &[types::BOOL], Aux::FloatCc(cc))
                            .one(),
                    )));
                }
                if !types_is_int(ty) {
                    return Err(refuse(
                        "comparison outside integers/floats (s26/s27)",
                        e.span,
                    ));
                }
                let cc = match op {
                    SyntaxKind::EqEq => IntCc::Eq,
                    SyntaxKind::NotEq => IntCc::Ne,
                    SyntaxKind::Lt => IntCc::Slt,
                    SyntaxKind::Gt => IntCc::Sgt,
                    SyntaxKind::LtEq => IntCc::Sle,
                    _ => IntCc::Sge,
                };
                Ok(Flow::Val(Some(
                    self.b
                        .ins(Opcode::Icmp, &[a, bv], &[types::BOOL], Aux::IntCc(cc))
                        .one(),
                )))
            }
            _ => Err(refuse("this operator in WIR lowering", e.span)),
        }
    }

    /// Arithmetic/bitwise emission: X3 checked by default, `.wrap` for
    /// `wrapping[T]`-typed expressions, float ops for floats. `None`
    /// means the op provably trapped.
    fn arith(
        &mut self,
        op: SyntaxKind,
        a: Value,
        b: Value,
        wrapping: bool,
        wty: TypeId,
        span: Span,
    ) -> R<Option<Value>> {
        if wty == types::F32 || wty == types::F64 {
            let fop = match op {
                SyntaxKind::Plus => Opcode::Fadd,
                SyntaxKind::Minus => Opcode::Fsub,
                SyntaxKind::Star => Opcode::Fmul,
                SyntaxKind::Slash => Opcode::Fdiv,
                _ => return Err(refuse("this float operator (no frem op)", span)),
            };
            return Ok(Some(self.b.ins(fop, &[a, b], &[wty], Aux::None).one()));
        }
        let iop = match (op, wrapping) {
            (SyntaxKind::Plus, false) => Opcode::IaddChk,
            (SyntaxKind::Minus, false) => Opcode::IsubChk,
            (SyntaxKind::Star, false) => Opcode::ImulChk,
            (SyntaxKind::Slash, false) => Opcode::IdivChk,
            (SyntaxKind::Percent, false) => Opcode::IremChk,
            (SyntaxKind::Plus, true) => Opcode::IaddWrap,
            (SyntaxKind::Minus, true) => Opcode::IsubWrap,
            (SyntaxKind::Star, true) => Opcode::ImulWrap,
            (SyntaxKind::Slash | SyntaxKind::Percent, true) => {
                return Err(refuse("wrapping division (no idiv.wrap op)", span));
            }
            (SyntaxKind::Amp, _) => Opcode::Band,
            (SyntaxKind::Pipe, _) => Opcode::Bor,
            (SyntaxKind::Caret, _) => Opcode::Bxor,
            (SyntaxKind::Shl, _) => Opcode::Shl,
            (SyntaxKind::Shr, _) => Opcode::Ashr,
            _ => return Err(refuse("this operator in WIR lowering", span)),
        };
        match self.b.ins(iop, &[a, b], &[wty], Aux::None) {
            InsOut::Vals(r) => Ok(Some(r[0])),
            InsOut::Trapped => Ok(None),
        }
    }

    fn lower_short_circuit(
        &mut self,
        d: wolf_ast::BinExpr<'t>,
        is_and: bool,
        span: Span,
    ) -> R<Flow> {
        let lhs = match d.lhs() {
            Some(l) => flow_val!(self.lower_expr(l)),
            None => None,
        };
        let Some(lhs) = lhs else {
            return Err(refuse("logic on a valueless operand", span));
        };
        let Some(rhs_expr) = d.rhs() else {
            return Ok(Flow::Val(Some(lhs)));
        };
        if let Some(c) = self.b.as_bool_const(lhs) {
            // Branch-on-const at the walker: the not-taken side is
            // never lowered (dead code costs nothing, and no
            // unreachable block is created).
            self.b.stats.identity += 1;
            if c == is_and {
                return self.lower_expr(rhs_expr);
            }
            return Ok(Flow::Val(Some(self.b.bconst(!is_and))));
        }
        let short = self.b.bconst(!is_and);
        let rhs_bb = self.b.create_block();
        let merge = self.b.create_block();
        let out = self.b.add_block_param(merge, types::BOOL);
        if is_and {
            self.b.ins_br(lhs, rhs_bb, &[], merge, &[short]);
        } else {
            self.b.ins_br(lhs, merge, &[short], rhs_bb, &[]);
        }
        self.b.seal_block(rhs_bb);
        self.b.switch_to_block(rhs_bb);
        self.b.gvn_push_scope();
        let rhs = self.lower_expr(rhs_expr);
        let diverged = match rhs {
            Ok(Flow::Val(Some(v))) => {
                self.b.ins_jmp(merge, &[v]);
                false
            }
            Ok(Flow::Val(None)) => {
                self.b.gvn_pop_scope();
                return Err(refuse("logic on a valueless operand", span));
            }
            Ok(Flow::Diverged) => true,
            Err(e) => {
                self.b.gvn_pop_scope();
                return Err(e);
            }
        };
        let _ = diverged;
        self.b.gvn_pop_scope();
        self.b.seal_block(merge);
        self.b.switch_to_block(merge);
        Ok(Flow::Val(Some(out)))
    }

    fn lower_cast(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = CastExpr::cast(e).expect("kind");
        let Some(inner) = d.expr() else {
            return Ok(Flow::Val(None));
        };
        let v = flow_val!(self.lower_expr(inner));
        let Some((from, to, kind)) = self.casts.get(&e.span).copied() else {
            // No recorded cast: sema treated it as identity.
            return Ok(Flow::Val(v));
        };
        match kind {
            CastKind::Identity | CastKind::Adapter => Ok(Flow::Val(v)),
            CastKind::Numeric => {
                let Some(v) = v else {
                    return Err(refuse("cast of a valueless expression", e.span));
                };
                let src = wir_ty(self.table, self.sigs, from, e.span)?;
                let dst = wir_ty(self.table, self.sigs, to, e.span)?;
                let (Some(src), Some(dst)) = (src, dst) else {
                    return Err(refuse("cast on unit types", e.span));
                };
                if src == dst {
                    return Ok(Flow::Val(Some(v)));
                }
                match (int_bits(src), int_bits(dst)) {
                    (Some(fb), Some(tb)) if tb > fb => Ok(Flow::Val(Some(
                        self.b.ins(Opcode::Sext, &[v], &[dst], Aux::None).one(),
                    ))),
                    (Some(_), Some(_)) => Err(refuse(
                        "narrowing numeric casts (range-check semantics, s27)",
                        e.span,
                    )),
                    _ => Err(refuse("int↔float casts (no conversion op yet)", e.span)),
                }
            }
            CastKind::Raw => Err(refuse("raw-pointer casts (unsafe tier, s26)", e.span)),
        }
    }

    fn lower_if(&mut self, e: &'t GreenNode, want: bool) -> R<Flow> {
        let d = IfExpr::cast(e).expect("kind");
        let cond = match d.condition() {
            Some(c) => flow_val!(self.lower_expr(c)),
            None => None,
        };
        let Some(cond) = cond else {
            return Err(refuse("an if without a condition value", e.span));
        };
        // Does this if produce a value? Sema's recorded type decides
        // when present; else-if chains inherit the caller's demand
        // (their nested node may carry no recorded type of its own).
        let want_v = match self.expr_sema_ty(e.span) {
            Some(t) => wir_ty(self.table, self.sigs, t, e.span)?.is_some(),
            None => want,
        };
        if let Some(c) = self.b.as_bool_const(cond) {
            // Branch-on-const: lower only the taken arm.
            self.b.stats.identity += 1;
            return if c {
                match d.then_block() {
                    Some(tb) => self.lower_block(tb, want_v),
                    None => Ok(Flow::Val(None)),
                }
            } else {
                match d.else_branch() {
                    Some(el) if el.kind == SyntaxKind::Block => {
                        self.lower_block(AstBlock::cast(el).expect("kind"), want_v)
                    }
                    Some(el) => self.lower_expr_w(el, want_v),
                    None => Ok(Flow::Val(None)),
                }
            };
        }
        let then_bb = self.b.create_block();
        let else_bb = self.b.create_block();
        self.b.ins_br(cond, then_bb, &[], else_bb, &[]);
        self.b.seal_block(then_bb);
        self.b.seal_block(else_bb);
        // Then arm.
        self.b.switch_to_block(then_bb);
        self.b.gvn_push_scope();
        let then_flow = match d.then_block() {
            Some(tb) => self.lower_block(tb, want_v),
            None => Ok(Flow::Val(None)),
        };
        let then_out = match then_flow {
            Ok(f) => f,
            Err(e) => {
                self.b.gvn_pop_scope();
                return Err(e);
            }
        };
        let then_end = self.b.current_block();
        self.b.gvn_pop_scope();
        // Else arm.
        self.b.switch_to_block(else_bb);
        self.b.gvn_push_scope();
        let else_flow = match d.else_branch() {
            Some(el) if el.kind == SyntaxKind::Block => {
                self.lower_block(AstBlock::cast(el).expect("kind"), want_v)
            }
            Some(el) => self.lower_expr_w(el, want_v),
            None => Ok(Flow::Val(None)),
        };
        let else_out = match else_flow {
            Ok(f) => f,
            Err(e) => {
                self.b.gvn_pop_scope();
                return Err(e);
            }
        };
        let else_end = self.b.current_block();
        self.b.gvn_pop_scope();
        // Join.
        match (then_out, else_out) {
            (Flow::Diverged, Flow::Diverged) => Ok(Flow::Diverged),
            (Flow::Val(v), Flow::Diverged) => {
                // Continue in the then arm's end block.
                self.b.switch_to_block(then_end);
                Ok(Flow::Val(v))
            }
            (Flow::Diverged, Flow::Val(v)) => {
                self.b.switch_to_block(else_end);
                Ok(Flow::Val(v))
            }
            (Flow::Val(tv), Flow::Val(ev)) => {
                let merge = self.b.create_block();
                // The result rides a merge parameter, typed off the
                // arm value; equal arm values need no parameter (the
                // trivial-param test, applied before the param exists).
                let (param, targs, eargs): (Option<Value>, Vec<Value>, Vec<Value>) =
                    match (want_v, tv, ev) {
                        (true, Some(a), Some(b)) if a != b => {
                            let ty = self.b.func.value_ty(a);
                            let p = self.b.add_block_param(merge, ty);
                            (Some(p), vec![a], vec![b])
                        }
                        (true, Some(a), Some(_)) => (Some(a), vec![], vec![]),
                        _ => (None, vec![], vec![]),
                    };
                self.b.switch_to_block(then_end);
                self.b.ins_jmp(merge, &targs);
                self.b.switch_to_block(else_end);
                self.b.ins_jmp(merge, &eargs);
                self.b.seal_block(merge);
                self.b.switch_to_block(merge);
                Ok(Flow::Val(param))
            }
        }
    }

    fn lower_while(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = WhileExpr::cast(e).expect("kind");
        let header = self.b.create_block();
        self.b.ins_jmp(header, &[]);
        self.b.switch_to_block(header);
        self.b.gvn_push_scope();
        self.loops.push(LoopFrame { header, exit: None });
        let out = self.lower_while_inner(d, header);
        let frame = self.loops.pop().expect("frame");
        self.b.gvn_pop_scope();
        out?;
        self.finish_loop(header, frame.exit)
    }

    fn lower_while_inner(&mut self, d: WhileExpr<'t>, header: Block) -> R<()> {
        let cond = match d.condition() {
            Some(c) => match self.lower_expr(c)? {
                Flow::Val(Some(v)) => v,
                Flow::Val(None) => {
                    return Err(refuse("a while without a condition value", d.syntax().span));
                }
                // The condition itself diverged: no loop edges beyond
                // it; the header chain is already terminated.
                Flow::Diverged => return Ok(()),
            },
            None => return Err(refuse("a while without a condition", d.syntax().span)),
        };
        match self.b.as_bool_const(cond) {
            Some(false) => {
                // The loop never runs: fall straight to the exit; the
                // body is dead code and never lowered.
                self.b.stats.identity += 1;
                let exit = self.b.create_block();
                self.loops.last_mut().expect("frame").exit = Some(exit);
                self.b.ins_jmp(exit, &[]);
            }
            Some(true) => {
                self.b.stats.identity += 1;
                let body = self.b.create_block();
                self.b.ins_jmp(body, &[]);
                self.b.seal_block(body);
                self.b.switch_to_block(body);
                self.lower_loop_body(d.body(), header)?;
            }
            None => {
                let body = self.b.create_block();
                let exit = self.b.create_block();
                self.loops.last_mut().expect("frame").exit = Some(exit);
                self.b.ins_br(cond, body, &[], exit, &[]);
                self.b.seal_block(body);
                self.b.switch_to_block(body);
                self.lower_loop_body(d.body(), header)?;
            }
        }
        Ok(())
    }

    fn lower_loop(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = LoopExpr::cast(e).expect("kind");
        let header = self.b.create_block();
        self.b.ins_jmp(header, &[]);
        self.b.switch_to_block(header);
        self.b.gvn_push_scope();
        self.loops.push(LoopFrame { header, exit: None });
        let out = self.lower_loop_body(d.body(), header);
        let frame = self.loops.pop().expect("frame");
        self.b.gvn_pop_scope();
        out?;
        self.finish_loop(header, frame.exit)
    }

    fn lower_loop_body(&mut self, body: Option<AstBlock<'t>>, header: Block) -> R<()> {
        self.b.gvn_push_scope();
        let flow = match body {
            Some(bl) => self.lower_block(bl, false),
            None => Ok(Flow::Val(None)),
        };
        let flow = match flow {
            Ok(f) => f,
            Err(e) => {
                self.b.gvn_pop_scope();
                return Err(e);
            }
        };
        if let Flow::Val(_) = flow {
            // The back edge.
            self.b.ins_jmp(header, &[]);
        }
        self.b.gvn_pop_scope();
        Ok(())
    }

    fn finish_loop(&mut self, header: Block, exit: Option<Block>) -> R<Flow> {
        self.b.seal_block(header);
        match exit {
            Some(exit) => {
                self.b.seal_block(exit);
                self.b.switch_to_block(exit);
                Ok(Flow::Val(None))
            }
            None => Ok(Flow::Diverged),
        }
    }

    // --------------------------------------------------- calls ----

    fn lower_call(&mut self, e: &'t GreenNode) -> R<Flow> {
        let d = CallExpr::cast(e).expect("kind");
        let cs: Option<&CallSig> = self.calls.get(&e.span).copied();
        // Builtins without a signature: assert / print.
        let callee_text = d.callee().map(|c| self.text(c.span)).unwrap_or_default();
        if cs.is_none() {
            match callee_text.as_str() {
                "assert" => return self.lower_assert(d),
                "print" | "print_raw" => {
                    return Err(refuse("print lowering (io token calls, s26)", e.span));
                }
                _ => return Err(refuse("calls outside the resolved surface", e.span)),
            }
        }
        let cs = cs.expect("checked");
        if cs.c_call {
            return Err(refuse("C calls (unsafe tier, c10)", e.span));
        }
        if cs.ctor {
            return Err(refuse(
                "enum-variant construction (aggregates, s26/s27)",
                e.span,
            ));
        }
        if cs.has_self {
            return Err(refuse("method calls (receiver lowering, s26)", e.span));
        }
        if cs.decl_span.is_none() {
            return Err(refuse("indirect calls through fn values (c05)", e.span));
        }
        // Resolve the callee to its unique package fn.
        let Some(cands) = self.fns.get(cs.callee.as_str()) else {
            return Err(refuse("calls into unresolvable bodies", e.span));
        };
        if cands.len() != 1 {
            return Err(refuse(
                "two modules declare a function with this name (WIR name mangling)",
                e.span,
            ));
        }
        let (_, callee_sig) = cands[0];
        if !callee_sig.generics.is_empty() {
            return Err(refuse("generic-function calls (monomorphization)", e.span));
        }
        if callee_sig.comptime {
            return Err(refuse("comptime calls (D29 CTFE owns these)", e.span));
        }
        // Arguments under their declared modes.
        let mut args = Vec::new();
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let mode = cs.params.get(i).and_then(|p| p.mode);
            if mode == Some(ParamMode::Mut) {
                return Err(refuse(
                    "`mut` argument passing (pointer-shaped lowering, s26)",
                    a.syntax().span,
                ));
            }
            let Some(vexpr) = Arg::value(a) else { continue };
            let v = flow_val!(self.lower_expr(vexpr));
            let Some(v) = v else {
                return Err(refuse("unit-typed arguments", vexpr.span));
            };
            args.push(v);
        }
        // Import the callee (per-function cache) and emit the call.
        let ext = match self.callees.get(&cs.callee) {
            Some(&ext) => ext,
            None => {
                let mut sig_params = Vec::with_capacity(callee_sig.params.len());
                for p in &callee_sig.params {
                    let Some(ty) = wir_ty(self.sig_table, self.sigs, p.ty, p.span)? else {
                        return Err(refuse("unit-typed parameters", p.span));
                    };
                    let mode = match p.mode {
                        None => Mode::Val,
                        Some(ParamMode::Mut) => Mode::Mut,
                        Some(ParamMode::Take) => Mode::Take,
                    };
                    sig_params.push(Param { ty, mode });
                }
                let results = match wir_ty(self.sig_table, self.sigs, callee_sig.ret, e.span)? {
                    Some(t) => vec![t],
                    None => vec![],
                };
                let sig = self.b.module.make_sig(sig_params, results);
                let ext = self.b.func.import_func(cs.callee.clone(), sig);
                self.callees.insert(cs.callee.clone(), ext);
                ext
            }
        };
        let results = self.b.ins_call(ext, &args);
        Ok(Flow::Val(results.first().copied()))
    }

    /// `assert(cond)` — the one user-raised trap: `br cond, continue,
    /// trap`. A constant condition folds to nothing (true) or a plain
    /// `trap` (false) — X3 semantics exactly.
    fn lower_assert(&mut self, d: CallExpr<'t>) -> R<Flow> {
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            let Some(vexpr) = Arg::value(a) else { continue };
            let v = flow_val!(self.lower_expr(vexpr));
            let Some(v) = v else {
                return Err(refuse("assert of a valueless expression", vexpr.span));
            };
            match self.b.as_bool_const(v) {
                Some(true) => {
                    self.b.stats.fold += 1;
                }
                Some(false) => {
                    self.b.stats.fold += 1;
                    self.b.ins_trap();
                    return Ok(Flow::Diverged);
                }
                None => {
                    let cont = self.b.create_block();
                    let trap_bb = self.b.create_block();
                    self.b.ins_br(v, cont, &[], trap_bb, &[]);
                    self.b.seal_block(trap_bb);
                    self.b.switch_to_block(trap_bb);
                    self.b.ins_trap();
                    self.b.seal_block(cont);
                    self.b.switch_to_block(cont);
                }
            }
        }
        Ok(Flow::Val(None))
    }
}

fn int_bits(t: TypeId) -> Option<u32> {
    match t {
        types::I8 => Some(8),
        types::I16 => Some(16),
        types::I32 => Some(32),
        types::I64 => Some(64),
        _ => None,
    }
}

/// Does this subtree contain any construct that forces new blocks?
/// (The semi-pruned pre-scan: a function without any is one block, so
/// every variable is single-block.)
fn contains_control(node: &GreenNode) -> bool {
    fn is_control(k: SyntaxKind) -> bool {
        matches!(
            k,
            SyntaxKind::IfExpr
                | SyntaxKind::WhileExpr
                | SyntaxKind::LoopExpr
                | SyntaxKind::ForExpr
                | SyntaxKind::MatchExpr
                | SyntaxKind::ElseExpr
                | SyntaxKind::TryExpr
                | SyntaxKind::ReturnExpr
                | SyntaxKind::BreakExpr
                | SyntaxKind::ContinueExpr
                | SyntaxKind::CallExpr
                | SyntaxKind::ClosureExpr
        )
    }
    fn walk(n: &GreenNode) -> bool {
        if is_control(n.kind) {
            return true;
        }
        // `!` and `&&`/`||` lower through branches too.
        if n.kind == SyntaxKind::PrefixExpr
            && n.tokens().next().is_some_and(|t| t.kind == SyntaxKind::Not)
        {
            return true;
        }
        if n.kind == SyntaxKind::BinExpr
            && n.tokens()
                .any(|t| matches!(t.kind, SyntaxKind::AmpAmp | SyntaxKind::PipePipe))
        {
            return true;
        }
        n.nodes().any(walk)
    }
    walk(node)
}

/// Integer literal: decimal with `_` separators, or `0x…` hex (bit
/// pattern, admitted up to 64 bits).
fn parse_int_literal(text: &str) -> Option<i64> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16).ok().map(|v| v as i64);
    }
    t.parse::<i64>().ok()
}

fn parse_float_literal(text: &str) -> Option<f64> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    t.parse::<f64>().ok()
}
