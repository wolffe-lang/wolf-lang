//! The miri-lite UB checker (s23) — the operational model, executable.
//!
//! An interpretive checker over sema's typed HIR plus this crate's own
//! facts: it executes the SAME dynamic semantics the corpus pins, at
//! the depth needed to decide `[mem.ub]` rows P1–P6/L1/L2/T1 on
//! unsafe-tier programs. It is deliberately NOT a second full
//! interpreter — the independent oracle is wolf-interp's is04 machine;
//! this machine exists so the *compiler* can check the model it
//! enforces statically (D11's "shipped Miri-equivalent checker", the
//! validation budget of 01 Q6), and so `wolf run --checked` (s31) has
//! an engine today.
//!
//! # The shadow machine
//!
//! - **Allocations** carry bytes, per-byte initialization (L1), an
//!   owning region (`[mem.boundary.ffi]`: C allocations belong to the
//!   region current at the call), and a **tree of tags** per
//!   `[mem.prov.tag]`/`[mem.prov.state]` — Reserved/Active/Frozen/
//!   Disabled with child/foreign transitions, protectors escalating
//!   foreign writes.
//! - **Regions** are dynamic values: created (`region()`, `region r
//!   { }`), opened (ambient stack), frozen (`freeze` → every owned
//!   tag Frozen, `[mem.prov.region]`), and freed (every owned tag
//!   tree Disabled; later access is P4). A region's backing base
//!   (`r as *u8`) is a zero-initialized arena block.
//! - **Row order is the is04 choice**, adopted deliberately so the
//!   two machines agree where both detect (their approximation
//!   contract §7.1): P3 → P4 → P1/P2 → L2 → L1. One access true of
//!   several rows reports the first.
//! - **The modelled C set is is04's** (s22): `c.malloc`, `c.calloc`,
//!   `c.free`, `c.memset`, `c.memcpy`. `malloc` bytes are
//!   uninitialized (L1 reachable); `free` Disables the whole tag tree
//!   (later tagged access P1, wildcard access L2); a double free or a
//!   free of an interior/foreign pointer is L2 (`free` dereferences
//!   the block it releases).
//! - **Attribution (s22 → s23):** every verdict names its `[mem.ub]`
//!   row, the licensed optimization the D2 pairing says the UB would
//!   break, and the responsible operation's span — the span of an s22
//!   attribution fact (raw access, expose, door, assume, c-call)
//!   recorded by the lowerer. [`attribute`] cross-checks a finding
//!   against [`crate::FnFacts`].
//!
//! # The checker-side quarantine equivalent (D21)
//!
//! Freed memory is never reused and never forgotten: a freed
//! allocation stays in the shadow store, poisoned, so use-after-free
//! and OOB-into-freed-span are deterministic verdicts — the same
//! contract the debug quarantine allocator (`wolf_rt::quarantine`,
//! stubbed this sprint, wired s32) gives compiled `--checked` builds.
//!
//! # Honest scope
//!
//! Single-threaded (C1 stays deferred with the concurrency campaign);
//! T2 (torn writes) is unreachable single-threaded — both are
//! reported as out of scope, never silently absent. Constructs beyond
//! the executable surface refuse with [`NotYet`] and the driver
//! reports `unsupported` (the conservatism ledger). Execution is
//! budget-bounded ([`Budget`]) with honest exhaustion.

use std::collections::HashMap;

use wolf_ast::{
    Arg, AssignStmt, Block as AstBlock, BorrowExpr, BracketApply, CallExpr, CastExpr, DeferStmt,
    ElseExpr, ExprStmt, FieldInit, ForExpr, GreenNode, IfExpr, InBlock, LetDecl, MatchExpr,
    MemberExpr, ParenExpr, PrefixExpr, RangeExpr, RegionBlock, ReturnExpr, StringExpr, StructLit,
    SyntaxKind, TupleExpr, UnsafeBlock, VarDecl, WhileExpr, is_pattern_kind,
};
use wolf_diag::{Diagnostic, codes};
use wolf_sema::check::{CallSig, CastKind, Dispatch};
use wolf_sema::types::{Prim, TyId, TyKind};
use wolf_sema::{BodyResult, NotYet, Package, Typecheck, TypedBody};
use wolf_span::Span;

// ------------------------------------------------------------ verdicts --

/// One `[mem.ub]` row this machine can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UbRow {
    P1,
    P2,
    P3,
    P4,
    P5,
    P6,
    L1,
    L2,
    T1,
}

impl UbRow {
    pub fn as_str(self) -> &'static str {
        match self {
            UbRow::P1 => "P1",
            UbRow::P2 => "P2",
            UbRow::P3 => "P3",
            UbRow::P4 => "P4",
            UbRow::P5 => "P5",
            UbRow::P6 => "P6",
            UbRow::L1 => "L1",
            UbRow::L2 => "L2",
            UbRow::T1 => "T1",
        }
    }

    /// The spec clause the row's rule lives at (the is04 assignment,
    /// adopted so `x-ub-clause` compares).
    pub fn clause(self) -> &'static str {
        match self {
            UbRow::P1 | UbRow::P2 => "mem.prov.state",
            UbRow::P4 => "mem.prov.region",
            UbRow::P5 => "mem.unsafe.raw.2",
            UbRow::P6 => "mem.unsafe.door",
            UbRow::L2 => "mem.unsafe.raw.1",
            UbRow::P3 | UbRow::L1 | UbRow::T1 => "mem.ub",
        }
    }

    /// The licensed optimization the D2 pairing says this UB would
    /// break (spec/02 §7's table, verbatim spine).
    pub fn licensed(self) -> &'static str {
        match self {
            UbRow::P1 => {
                "O1: `mut` params lower to `noalias` + `dereferenceable`; \
                          unique-tag stores forward without memory checks"
            }
            UbRow::P2 => {
                "O2: `read` params are immutable-for-the-call — loads hoist/CSE \
                          across opaque calls; `imm` data const-propagates"
            }
            UbRow::P3 => {
                "O3a: `dereferenceable(n)` on known-size accesses; bounds-based \
                          alias disproof between distinct allocations"
            }
            UbRow::P4 => {
                "O3b: one alias-scope domain per region — pointers into distinct \
                          regions never alias; O4: closed regions yield `invariant.load`"
            }
            UbRow::P5 => {
                "O5: the asserted ranges get `noalias` treatment in Tier-3 code — \
                          vectorization/reordering as if proven"
            }
            UbRow::P6 => {
                "O6: safe-tier code after the door keeps all safe-tier \
                          entitlements (O1–O4) — safe code never re-checks"
            }
            UbRow::L1 => {
                "O7: moves lower to memcpy-and-forget; dead-store elimination on \
                          moved-from places; no zero-init of locals"
            }
            UbRow::L2 => {
                "O8: escape analysis / stack promotion without conservatively \
                          pinning addresses"
            }
            UbRow::T1 => {
                "O9: niche packing; match jump tables without default arms; \
                          UTF-8 fast paths without re-validation"
            }
        }
    }
}

/// A UB verdict: the D2 pairing made executable — row, clause,
/// licensed optimization, the access span, and the span that created
/// the responsible tag (allocation site for roots).
#[derive(Debug, Clone)]
pub struct UbFinding {
    pub row: UbRow,
    /// What happened, in operation vocabulary.
    pub message: String,
    /// The access/operation span (`x-ub-span`).
    pub span: Span,
    /// Where the responsible tag/allocation was created
    /// (`x-ub-tag-span`).
    pub tag_span: Span,
}

/// A deterministic fault (`[conf.trap.set]` kind + the clause that
/// defines it + the faulting site).
#[derive(Debug, Clone)]
pub struct TrapInfo {
    pub kind: &'static str,
    pub clause: &'static str,
    pub span: Span,
}

/// The outcome of one checked execution.
#[derive(Debug, Clone)]
pub enum Verdict {
    Exit(u8),
    Trap(TrapInfo),
    Ub(UbFinding),
}

/// A completed run: verdict plus everything the program printed.
#[derive(Debug)]
pub struct RunOutcome {
    pub verdict: Verdict,
    pub stdout: String,
}

/// Execution budget: steps (every expression evaluation counts one)
/// and total shadow-memory bytes. Exhaustion is an honest refusal —
/// never a verdict.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub steps: u64,
    pub mem_bytes: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            steps: 20_000_000,
            mem_bytes: 256 << 20,
        }
    }
}

/// Render a UB finding as the user-facing E1401 diagnostic: the row,
/// the responsible operation, and the licensed optimization it would
/// break (the D2 pairing, executable).
pub fn ub_diagnostic(f: &UbFinding) -> Diagnostic {
    Diagnostic::error(
        codes::E1401,
        f.span,
        format!(
            "undefined behavior: [mem.ub] row {} — {}",
            f.row.as_str(),
            f.message
        ),
    )
    .with_label("the operation that reaches undefined behavior")
    .with_secondary(
        f.tag_span,
        "the provenance this operation violates was created here",
    )
    .with_note(format!(
        "this row licenses {} — compiled code may already have been transformed \
         under that assumption, so the behavior of an unchecked build is undefined \
         ([{}]). The `--checked` machine reports it deterministically instead.",
        f.row.licensed(),
        f.row.clause(),
    ))
}

// ------------------------------------------------------------- machine --

const ALLOC_STRIDE: u64 = 0x1_0000;
const REGION_BACKING: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TagState {
    Reserved,
    Active,
    Frozen,
    Disabled,
}

#[derive(Debug, Clone)]
struct Tag {
    parent: Option<u32>,
    state: TagState,
    protected: u32,
    exposed: bool,
    origin: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeadReason {
    CFree,
    RegionFreed,
}

#[derive(Debug)]
struct Allocation {
    size: u64,
    bytes: Vec<u8>,
    init: Vec<bool>,
    /// The dynamic region owning this allocation
    /// (`[mem.boundary.ffi]` for C allocations).
    region: usize,
    from_malloc: bool,
    live: bool,
    dead: Option<DeadReason>,
    tags: Vec<Tag>,
    span: Span,
}

impl Allocation {
    fn base_addr(id: usize) -> u64 {
        (id as u64 + 1) * ALLOC_STRIDE
    }
}

/// A raw pointer value: provenance (allocation + tag) or dangling.
#[derive(Debug, Clone, Copy)]
struct PtrVal {
    /// `None`: wildcard that resolved to no live exposed allocation —
    /// dangling; deref is L2.
    alloc: Option<usize>,
    tag: u32,
    offset: i64,
    /// The absolute address (implementation-specified layout: base +
    /// offset; a protocol fact, never a comparison surface).
    addr: u64,
}

#[derive(Debug)]
struct DynRegion {
    live: bool,
    frozen: bool,
    backing: Option<usize>,
    span: Span,
}

#[derive(Debug, Clone)]
struct PoolSlot {
    generation: i64,
    live: bool,
    value: Value,
}

#[derive(Debug)]
struct RcCell {
    strong: u32,
    weak: u32,
    value: Value,
}

/// A dynamic value. Copy-ness mirrors the static `is_copy` set:
/// scalars, strings (immutable views), ranges, handles, raw pointers
/// copy; aggregates, containers, cells and regions move.
#[derive(Debug, Clone)]
enum Value {
    Unit,
    Int(i64),
    Bool(bool),
    Str(String),
    Range {
        start: i64,
        end: i64,
    },
    Struct {
        fields: Vec<(String, Value)>,
    },
    Enum {
        variant: String,
        payload: Vec<Value>,
    },
    List(usize),
    Pool(usize),
    Handle {
        index: usize,
        generation: i64,
    },
    Shared(usize),
    Weak(usize),
    Region(usize),
    Ptr(PtrVal),
    /// An error-channel value (`!T`'s row half): tag + payload.
    ErrTag {
        tag: String,
        payload: Vec<Value>,
    },
    /// A `mut` parameter: an alias of the caller's place.
    Ref(Place),
    Moved,
    Uninit,
}

impl Value {
    fn is_copy(&self) -> bool {
        matches!(
            self,
            Value::Unit
                | Value::Int(_)
                | Value::Bool(_)
                | Value::Str(_)
                | Value::Range { .. }
                | Value::Handle { .. }
                | Value::Ptr(_)
        )
    }
}

/// One step of a place path, indices resolved at path-build time.
#[derive(Debug, Clone, PartialEq)]
enum PStep {
    Field(String),
    ListIdx {
        index: i64,
        span: Span,
    },
    PoolIdx {
        index: usize,
        generation: i64,
        span: Span,
    },
}

/// An absolute place: frame-indexed local plus a resolved path.
#[derive(Debug, Clone, PartialEq)]
struct Place {
    frame: usize,
    local: usize,
    path: Vec<PStep>,
}

/// Why evaluation stopped (beyond a value).
#[derive(Debug)]
enum Stop {
    Trap(TrapInfo),
    Ub(UbFinding),
    Refuse(NotYet),
    Budget(&'static str),
}

/// Control flow out of an expression.
enum Flow {
    Val(Value),
    Return(Value),
    Err(Value),
    Break,
    Continue,
}

type E<T> = Result<T, Stop>;

macro_rules! val {
    ($e:expr) => {
        match $e? {
            Flow::Val(v) => v,
            other => return Ok(other),
        }
    };
}

/// Scope-exit obligation, LIFO with the defers
/// (`[mem.shared.drop.1]`).
enum Cleanup<'t> {
    Defer(&'t GreenNode, bool),
    /// A `shared`/`weak` local's drop-if-live.
    DropLocal(usize),
    /// A first-class region value's binding-scope free.
    FreeRegionLocal(usize),
}

struct Scope<'t> {
    names: Vec<(String, usize)>,
    cleanup: Vec<Cleanup<'t>>,
}

struct Frame<'t> {
    body: usize,
    locals: Vec<Value>,
    scopes: Vec<Scope<'t>>,
}

/// Per-body evaluation context: the typed side tables, span-keyed.
struct Ctx<'t> {
    tb: &'t TypedBody,
    node: &'t GreenNode,
    calls: HashMap<Span, &'t CallSig>,
    casts: HashMap<Span, (TyId, TyId, CastKind)>,
    expr_tys: HashMap<Span, TyId>,
    dispatch: HashMap<Span, &'t Dispatch>,
    src_file: usize,
}

struct Machine<'t> {
    pkg: &'t Package,
    tc: &'t Typecheck,
    /// body index (into `tc.bodies`) -> resolved AST + tables.
    ctxs: Vec<Option<Ctx<'t>>>,
    /// (module, fn name) -> body index, top-level fns.
    fns: HashMap<(usize, String), usize>,
    /// (self-type name, method name) -> body index, inherent impls.
    methods: HashMap<(String, String), usize>,

    allocs: Vec<Allocation>,
    regions: Vec<DynRegion>,
    lists: Vec<Vec<Value>>,
    pools: Vec<Vec<PoolSlot>>,
    cells: Vec<RcCell>,
    frames: Vec<Frame<'t>>,
    /// The dynamic ambient-region stack; `[0]` is the run's root
    /// region (never freed while the run lives).
    ambient: Vec<usize>,
    stdout: String,
    steps: u64,
    mem_used: u64,
    budget: Budget,
    in_defer: bool,
}

// The ubiquitous helpers.
impl<'t> Machine<'t> {
    fn refuse<T>(&self, construct: &'static str, span: Span) -> E<T> {
        Err(Stop::Refuse(NotYet { construct, span }))
    }

    fn trap<T>(&self, kind: &'static str, clause: &'static str, span: Span) -> E<T> {
        Err(Stop::Trap(TrapInfo { kind, clause, span }))
    }

    fn ub<T>(&self, row: UbRow, message: String, span: Span, tag_span: Span) -> E<T> {
        Err(Stop::Ub(UbFinding {
            row,
            message,
            span,
            tag_span,
        }))
    }

    fn tick(&mut self) -> E<()> {
        self.steps += 1;
        if self.steps > self.budget.steps {
            return Err(Stop::Budget("step budget exhausted"));
        }
        Ok(())
    }

    fn charge_mem(&mut self, bytes: u64) -> E<()> {
        self.mem_used += bytes;
        if self.mem_used > self.budget.mem_bytes {
            return Err(Stop::Budget("shadow-memory budget exhausted"));
        }
        Ok(())
    }

    fn ctx(&self) -> &Ctx<'t> {
        let body = self.frames.last().expect("frame").body;
        self.ctxs[body].as_ref().expect("running body has ctx")
    }

    fn text(&self, span: Span) -> String {
        let src = &self.pkg.files[self.ctx().src_file].raw.src;
        String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize]).into_owned()
    }

    fn expr_ty(&self, span: Span) -> Option<&'t TyKind> {
        let ctx = self.ctx();
        ctx.expr_tys.get(&span).map(|&id| ctx.tb.table.kind(id))
    }
}

// ------------------------------------------------- the tag machine ------

impl<'t> Machine<'t> {
    fn new_alloc(
        &mut self,
        size: u64,
        region: usize,
        from_malloc: bool,
        zeroed: bool,
        exposed: bool,
        span: Span,
    ) -> E<usize> {
        self.charge_mem(size)?;
        let id = self.allocs.len();
        self.allocs.push(Allocation {
            size,
            bytes: vec![0; size as usize],
            init: vec![zeroed; size as usize],
            region,
            from_malloc,
            live: true,
            dead: None,
            tags: vec![Tag {
                parent: None,
                state: TagState::Active,
                protected: 0,
                exposed,
                origin: span,
            }],
            span,
        });
        Ok(id)
    }

    /// Is `anc` an ancestor of (or equal to) `t` in the tag tree?
    fn is_path(alloc: &Allocation, anc: u32, t: u32) -> bool {
        let mut cur = Some(t);
        while let Some(c) = cur {
            if c == anc {
                return true;
            }
            cur = alloc.tags[c as usize].parent;
        }
        false
    }

    /// One typed access through a pointer: the row-ordered check
    /// (P3 → P4 → P1/P2 → L2 → L1, the published is04 order) plus the
    /// `[mem.prov.state]` transitions. `opdesc` names the operation
    /// for the finding's message.
    fn mem_access(&mut self, p: PtrVal, len: u64, write: bool, span: Span, opdesc: &str) -> E<()> {
        // L2 — no allocation to consult: a wildcard into nothing, or
        // a dangling survivor.
        let Some(aid) = p.alloc else {
            return self.ub(
                UbRow::L2,
                format!(
                    "{opdesc} through a dangling raw pointer (no live allocation at this address)"
                ),
                span,
                span,
            );
        };
        // P3 — bounds first: an OOB access has no location to have a
        // permission at.
        let (size, alloc_span) = {
            let a = &self.allocs[aid];
            (a.size, a.span)
        };
        if p.offset < 0 || (p.offset as u64).saturating_add(len) > size {
            return self.ub(
                UbRow::P3,
                format!(
                    "{opdesc} at offset {} of a {size}-byte allocation — outside its bounds",
                    p.offset
                ),
                span,
                alloc_span,
            );
        }
        // P4 — the owning region was freed (the more specific fact
        // than the tag; it licenses O3b/O4, not O1).
        let region = self.allocs[aid].region;
        if !self.regions[region].live {
            let rspan = self.regions[region].span;
            return self.ub(
                UbRow::P4,
                format!("{opdesc} into an allocation whose region was already freed"),
                span,
                rspan,
            );
        }
        // P1/P2 — the tag tree ([mem.prov.state]): the accessing
        // tag's path must permit the access.
        {
            let a = &self.allocs[aid];
            let mut cur = Some(p.tag);
            while let Some(c) = cur {
                let t = &a.tags[c as usize];
                match t.state {
                    TagState::Disabled => {
                        let origin = t.origin;
                        let why = match a.dead {
                            Some(DeadReason::CFree) => "freed by `c.free`",
                            Some(DeadReason::RegionFreed) => "its region was freed",
                            None => "invalidated by a conflicting access",
                        };
                        return self.ub(
                            UbRow::P1,
                            format!("{opdesc} through a Disabled tag ({why})"),
                            span,
                            origin,
                        );
                    }
                    TagState::Frozen if write => {
                        let origin = t.origin;
                        return self.ub(
                            UbRow::P2,
                            format!("{opdesc}: write through a Frozen tag"),
                            span,
                            origin,
                        );
                    }
                    _ => {}
                }
                cur = t.parent;
            }
        }
        // Foreign transitions + protector escalation.
        let ntags = self.allocs[aid].tags.len() as u32;
        for v in 0..ntags {
            let child = Self::is_path(&self.allocs[aid], v, p.tag);
            if child {
                // Child write activates a Reserved tag on the path.
                if write && self.allocs[aid].tags[v as usize].state == TagState::Reserved {
                    self.allocs[aid].tags[v as usize].state = TagState::Active;
                }
                continue;
            }
            let (state, protected, origin) = {
                let t = &self.allocs[aid].tags[v as usize];
                (t.state, t.protected, t.origin)
            };
            if write {
                if protected > 0 && state != TagState::Disabled {
                    // §6: protected tags escalate the foreign-write
                    // transition to immediate UB; §7 carries it as P1
                    // ("use of an invalidated borrow" — the is04
                    // reading, adopted).
                    return self.ub(
                        UbRow::P1,
                        format!("{opdesc}: foreign write invalidates a protected tag"),
                        span,
                        origin,
                    );
                }
                self.allocs[aid].tags[v as usize].state = TagState::Disabled;
            } else if state == TagState::Active {
                self.allocs[aid].tags[v as usize].state = TagState::Frozen;
            }
        }
        // L1 — uninitialized read, last: "what was written here" is
        // only a question once the access is otherwise legal.
        if !write {
            let a = &self.allocs[aid];
            let lo = p.offset as usize;
            if a.init[lo..lo + len as usize].iter().any(|b| !b) {
                let origin = a.span;
                return self.ub(
                    UbRow::L1,
                    format!("{opdesc} reads uninitialized memory"),
                    span,
                    origin,
                );
            }
        }
        Ok(())
    }

    fn raw_read_bytes(&mut self, p: PtrVal, len: u64, span: Span, opdesc: &str) -> E<Vec<u8>> {
        self.mem_access(p, len, false, span, opdesc)?;
        let a = &self.allocs[p.alloc.expect("checked")];
        let lo = p.offset as usize;
        Ok(a.bytes[lo..lo + len as usize].to_vec())
    }

    fn raw_write_bytes(&mut self, p: PtrVal, data: &[u8], span: Span, opdesc: &str) -> E<()> {
        self.mem_access(p, data.len() as u64, true, span, opdesc)?;
        let a = &mut self.allocs[p.alloc.expect("checked")];
        let lo = p.offset as usize;
        a.bytes[lo..lo + data.len()].copy_from_slice(data);
        for b in &mut a.init[lo..lo + data.len()] {
            *b = true;
        }
        Ok(())
    }

    /// Resolve an integer address among live allocations with an
    /// exposed tag ([mem.prov.expose]'s angelic resolution: a defined
    /// execution is chosen if one exists).
    fn resolve_exposed(&mut self, addr: u64, span: Span) -> PtrVal {
        for (id, a) in self.allocs.iter_mut().enumerate() {
            let base = Allocation::base_addr(id);
            if !a.live || addr < base || addr >= base + a.size {
                continue;
            }
            if let Some(exp) = (0..a.tags.len()).find(|&i| a.tags[i].exposed) {
                let child = a.tags.len() as u32;
                let state = match a.tags[exp].state {
                    TagState::Frozen => TagState::Frozen,
                    _ => TagState::Active,
                };
                a.tags.push(Tag {
                    parent: Some(exp as u32),
                    state,
                    protected: 0,
                    exposed: false,
                    origin: span,
                });
                return PtrVal {
                    alloc: Some(id),
                    tag: child,
                    offset: (addr - base) as i64,
                    addr,
                };
            }
        }
        PtrVal {
            alloc: None,
            tag: 0,
            offset: 0,
            addr,
        }
    }

    /// Free a region: every allocation it owns has its whole tag tree
    /// Disabled (`[mem.prov.region]`); nothing is reused — the
    /// checker-side quarantine (D21).
    fn free_region(&mut self, rid: usize) {
        self.regions[rid].live = false;
        for a in &mut self.allocs {
            if a.region == rid && a.live {
                a.live = false;
                a.dead = Some(DeadReason::RegionFreed);
                for t in &mut a.tags {
                    t.state = TagState::Disabled;
                }
            }
        }
    }

    /// `freeze r`: every owned tag transitions to Frozen
    /// (`[mem.prov.region]`); the region is never freed (imm data
    /// outlives every frame).
    fn freeze_region(&mut self, rid: usize) {
        self.regions[rid].frozen = true;
        for a in &mut self.allocs {
            if a.region == rid && a.live {
                for t in &mut a.tags {
                    if t.state != TagState::Disabled {
                        t.state = TagState::Frozen;
                    }
                }
            }
        }
    }

    /// The region's backing base (`r as *u8`): a zero-initialized
    /// arena block, minted on first demand.
    fn region_backing(&mut self, rid: usize, span: Span) -> E<PtrVal> {
        let aid = match self.regions[rid].backing {
            Some(a) => a,
            None => {
                let a = self.new_alloc(REGION_BACKING, rid, false, true, true, span)?;
                if self.regions[rid].frozen {
                    for t in &mut self.allocs[a].tags {
                        t.state = TagState::Frozen;
                    }
                }
                self.regions[rid].backing = Some(a);
                a
            }
        };
        Ok(PtrVal {
            alloc: Some(aid),
            tag: 0,
            offset: 0,
            addr: Allocation::base_addr(aid),
        })
    }
}

// ------------------------------------------------------- entry points --

/// Execute the package's `main` under the UB machine. `Err(NotYet)` is
/// the honest refusal (construct outside the executable surface, or
/// budget exhaustion); the driver reports `unsupported`.
pub fn run_checked(pkg: &Package, tc: &Typecheck, budget: Budget) -> Result<RunOutcome, NotYet> {
    let root_span = pkg.files[0].parse.root.span;
    let mut m = Machine::new(pkg, tc);
    m.budget = budget;
    let main = match m.find_main() {
        Some(b) => b,
        None => {
            return Err(NotYet {
                construct: "checked execution without a `main` entry",
                span: root_span,
            });
        }
    };
    match m.call_body(main, Vec::new()) {
        Ok(v) => {
            let code = match v {
                Value::Int(n) => n.rem_euclid(256) as u8,
                Value::Unit => 0,
                // `main` returned an error value: the documented D30
                // process behavior (s29, matching the interpreter and
                // the native `__wolf_rt_main_err` path) — the tag on
                // stdout, exit 1.
                Value::ErrTag { ref tag, .. } => {
                    m.stdout.push_str(&format!("error: {tag}\n"));
                    1
                }
                _ => 0,
            };
            Ok(RunOutcome {
                verdict: Verdict::Exit(code),
                stdout: m.stdout,
            })
        }
        Err(Stop::Trap(t)) => Ok(RunOutcome {
            verdict: Verdict::Trap(t),
            stdout: m.stdout,
        }),
        Err(Stop::Ub(f)) => Ok(RunOutcome {
            verdict: Verdict::Ub(f),
            stdout: m.stdout,
        }),
        Err(Stop::Refuse(nyc)) => Err(nyc),
        Err(Stop::Budget(what)) => Err(NotYet {
            construct: what,
            span: root_span,
        }),
    }
}

/// Cross-check a finding against the s22 attribution facts: the
/// recorded raw-tier operation whose span contains the finding's, if
/// any — the fact the verdict is attributed to.
pub fn attribute<'f>(finding: &UbFinding, facts: &'f [crate::FnFacts]) -> Option<(&'f str, Span)> {
    let hit = |s: &Span| s.lo <= finding.span.lo && finding.span.hi <= s.hi;
    for f in facts {
        for (ptr, _, s) in &f.raw_accesses {
            if hit(s) {
                return Some((ptr.as_str(), *s));
            }
        }
        for (ops, s) in &f.assumes {
            if hit(s) {
                return Some((ops.as_str(), *s));
            }
        }
        for (region, _, s) in &f.doors {
            if hit(s) {
                return Some((region.as_str(), *s));
            }
        }
        for (what, _, s) in &f.exposes {
            if hit(s) {
                return Some((what.as_str(), *s));
            }
        }
        for (callee, s) in &f.c_calls {
            if hit(s) {
                return Some((callee.as_str(), *s));
            }
        }
    }
    None
}

impl<'t> Machine<'t> {
    fn new(pkg: &'t Package, tc: &'t Typecheck) -> Machine<'t> {
        let mut m = Machine {
            pkg,
            tc,
            ctxs: Vec::new(),
            fns: HashMap::new(),
            methods: HashMap::new(),
            allocs: Vec::new(),
            regions: Vec::new(),
            lists: Vec::new(),
            pools: Vec::new(),
            cells: Vec::new(),
            frames: Vec::new(),
            ambient: Vec::new(),
            stdout: String::new(),
            steps: 0,
            mem_used: 0,
            budget: Budget::default(),
            in_defer: false,
        };
        // The run's root region: `main`'s caller (never freed).
        m.regions.push(DynRegion {
            live: true,
            frozen: false,
            backing: None,
            span: pkg.files[0].parse.root.span,
        });
        m.ambient.push(0);
        for (i, outcome) in tc.bodies.iter().enumerate() {
            let BodyResult::Checked(tb) = &outcome.result else {
                m.ctxs.push(None);
                continue;
            };
            let b = &outcome.body;
            let root = &pkg.files[b.file].parse.root;
            let Some(node) = root.nodes().filter(|n| n.kind.is_item()).nth(b.decl) else {
                m.ctxs.push(None);
                continue;
            };
            let (node, outer) = match b.member {
                None => (node, None),
                Some(mi) => match node.nodes().filter(|n| n.kind.is_item()).nth(mi) {
                    Some(inner) => (inner, Some(node)),
                    None => {
                        m.ctxs.push(None);
                        continue;
                    }
                },
            };
            if node.kind == SyntaxKind::FnDecl {
                match outer {
                    None => {
                        m.fns.insert((b.module, b.name.clone()), i);
                    }
                    Some(o) if o.kind == SyntaxKind::ImplDecl => {
                        // Inherent impls spell the target as the
                        // path (`impl V {`); trait impls carry the
                        // self type after `for`.
                        let target = wolf_ast::ImplDecl::cast(o).and_then(|d| {
                            d.self_ty()
                                .map(|t| t.span)
                                .or_else(|| d.trait_path().map(|p| p.syntax().span))
                        });
                        if let Some(span) = target {
                            let src = &pkg.files[b.file].raw.src;
                            let ty =
                                String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize])
                                    .into_owned();
                            m.methods.insert((ty, b.name.clone()), i);
                        }
                    }
                    _ => {}
                }
            }
            let ctx = Ctx {
                tb,
                node,
                calls: tb.calls.iter().map(|(s, c)| (*s, c)).collect(),
                casts: tb
                    .casts
                    .iter()
                    .map(|(s, a, b2, k)| (*s, (*a, *b2, *k)))
                    .collect(),
                expr_tys: tb.exprs.iter().map(|(s, t)| (*s, *t)).collect(),
                dispatch: tb.dispatch.iter().map(|(s, d)| (*s, d)).collect(),
                src_file: b.file,
            };
            m.ctxs.push(Some(ctx));
        }
        m
    }

    fn find_main(&self) -> Option<usize> {
        // Prefer the entry file's `main` (file 0), else any.
        let mut best: Option<usize> = None;
        for ((_, name), &idx) in &self.fns {
            if name == "main" {
                let file = self.tc.bodies[idx].body.file;
                if file == 0 {
                    return Some(idx);
                }
                best = Some(idx);
            }
        }
        best
    }

    /// Call a body with already-bound argument values (parameters in
    /// declaration order).
    fn call_body(&mut self, body: usize, args: Vec<Value>) -> E<Value> {
        if self.frames.len() > 128 {
            return Err(Stop::Budget("call depth budget exhausted"));
        }
        let ctx = self.ctxs[body].as_ref().expect("callable body has ctx");
        let node = ctx.node;
        let decl = wolf_ast::FnDecl::cast(node).expect("fn body");
        let Some(block) = decl.body() else {
            return self.refuse("extern fn without a body", node.span);
        };
        let mut frame = Frame {
            body,
            locals: Vec::new(),
            scopes: vec![Scope {
                names: Vec::new(),
                cleanup: Vec::new(),
            }],
        };
        // Parameter names from the declaration, values from the call.
        let mut names: Vec<String> = Vec::new();
        if let Some(params) = decl.params() {
            let src = &self.pkg.files[self.tc.bodies[body].body.file].raw.src;
            for p in params.params() {
                if p.is_self() {
                    names.push("self".to_string());
                } else if let Some(n) = p.name() {
                    names.push(
                        String::from_utf8_lossy(&src[n.span.lo as usize..n.span.hi as usize])
                            .into_owned(),
                    );
                }
            }
        }
        for (i, v) in args.into_iter().enumerate() {
            let name = names.get(i).cloned().unwrap_or_else(|| format!("_{i}"));
            frame.scopes[0].names.push((name, frame.locals.len()));
            frame.locals.push(v);
        }
        self.frames.push(frame);
        let result = self.eval_block(block, true);
        let out = match result {
            Ok(Flow::Val(v)) => self.exit_scopes_to(0, false).map(|()| v),
            Ok(Flow::Return(v)) => self.exit_scopes_to(0, false).map(|()| v),
            Ok(Flow::Err(_)) => {
                let r = self.exit_scopes_to(0, true);
                match r {
                    Ok(()) => Err(Stop::Refuse(NotYet {
                        construct: "an error value escaping the checked entry",
                        span: node.span,
                    })),
                    Err(e) => Err(e),
                }
            }
            Ok(Flow::Break) | Ok(Flow::Continue) => Ok(Value::Unit),
            Err(e) => Err(e),
        };
        self.frames.pop();
        out
    }

    // ------------------------------------------------ frames/scopes --

    fn frame(&mut self) -> &mut Frame<'t> {
        self.frames.last_mut().expect("frame")
    }

    fn push_scope(&mut self) {
        self.frame().scopes.push(Scope {
            names: Vec::new(),
            cleanup: Vec::new(),
        });
    }

    fn declare(&mut self, name: &str, v: Value) -> usize {
        let f = self.frames.last_mut().expect("frame");
        let idx = f.locals.len();
        f.locals.push(v);
        f.scopes
            .last_mut()
            .expect("scope")
            .names
            .push((name.to_string(), idx));
        idx
    }

    fn lookup(&self, name: &str) -> Option<(usize, usize)> {
        let fi = self.frames.len() - 1;
        let f = self.frames.last()?;
        for scope in f.scopes.iter().rev() {
            for (n, idx) in scope.names.iter().rev() {
                if n == name {
                    return Some((fi, *idx));
                }
            }
        }
        None
    }

    /// Run one scope's cleanup (LIFO: defers, RC drops, region frees
    /// in reverse declaration order) and pop it.
    fn close_scope(&mut self, error_path: bool) -> E<()> {
        let cleanup: Vec<Cleanup<'t>> = {
            let f = self.frames.last_mut().expect("frame");
            let scope = f.scopes.last_mut().expect("scope");
            std::mem::take(&mut scope.cleanup)
        };
        for c in cleanup.into_iter().rev() {
            match c {
                Cleanup::Defer(node, is_err) => {
                    if is_err && !error_path {
                        continue;
                    }
                    if self.in_defer {
                        continue;
                    }
                    self.in_defer = true;
                    let r = self.eval(node);
                    self.in_defer = false;
                    match r {
                        Ok(_) => {}
                        Err(e) => return Err(e),
                    }
                }
                Cleanup::DropLocal(idx) => {
                    let fi = self.frames.len() - 1;
                    let v = self.frames[fi].locals[idx].clone();
                    match v {
                        Value::Shared(c) => {
                            self.cells[c].strong = self.cells[c].strong.saturating_sub(1);
                        }
                        Value::Weak(c) => {
                            self.cells[c].weak = self.cells[c].weak.saturating_sub(1);
                        }
                        _ => {}
                    }
                }
                Cleanup::FreeRegionLocal(idx) => {
                    let fi = self.frames.len() - 1;
                    if let Value::Region(rid) = self.frames[fi].locals[idx]
                        && self.regions[rid].live
                        && !self.regions[rid].frozen
                    {
                        self.free_region(rid);
                    }
                }
            }
        }
        self.frame().scopes.pop();
        Ok(())
    }

    /// Unwind scopes down to `depth` (exclusive), running cleanups.
    fn exit_scopes_to(&mut self, depth: usize, error_path: bool) -> E<()> {
        while self.frames.last().expect("frame").scopes.len() > depth {
            self.close_scope(error_path)?;
        }
        Ok(())
    }

    // ------------------------------------------------------ places --

    /// Resolve an lvalue-shaped expression to a place. `None`: not a
    /// place (temporary, item reference).
    fn place_of(&mut self, e: &'t GreenNode) -> E<Option<Place>> {
        match e.kind {
            SyntaxKind::PathExpr => {
                let name = self.text(e.span);
                if name.contains('.') || name.contains("::") {
                    return Ok(None);
                }
                match self.lookup(&name) {
                    Some((frame, local)) => Ok(Some(Place {
                        frame,
                        local,
                        path: Vec::new(),
                    })),
                    None => Ok(None),
                }
            }
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.place_of(inner),
                None => Ok(None),
            },
            SyntaxKind::MemberExpr => {
                let m = MemberExpr::cast(e).expect("kind");
                let Some(base) = m.base() else {
                    return Ok(None);
                };
                // `(mut recv)` in receiver position unwraps.
                let base = match ParenExpr::cast(base) {
                    Some(p) if p.mode().is_some() => p.expr().unwrap_or(base),
                    _ => base,
                };
                let Some(member) = m.member() else {
                    return Ok(None);
                };
                let field = self.text(member.span);
                match self.place_of(base)? {
                    Some(mut place) => {
                        place.path.push(PStep::Field(field));
                        Ok(Some(place))
                    }
                    None => Ok(None),
                }
            }
            SyntaxKind::BracketApply => {
                let b = BracketApply::cast(e).expect("kind");
                let Some(recv) = b.callee() else {
                    return Ok(None);
                };
                // Raw-pointer indexing is never a place — the raw
                // tier owns it.
                if matches!(self.expr_ty(recv.span), Some(TyKind::Ptr(_))) {
                    return Ok(None);
                }
                let Some(base) = self.place_of(recv)? else {
                    return Ok(None);
                };
                let mut idx_val: Option<Value> = None;
                for a in b.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a)
                        && wolf_ast::is_expr_kind(v.kind)
                    {
                        idx_val = Some(match self.eval(v)? {
                            Flow::Val(x) => x,
                            _ => return Ok(None),
                        });
                    }
                }
                let mut place = base;
                match idx_val {
                    Some(Value::Int(i)) => place.path.push(PStep::ListIdx {
                        index: i,
                        span: e.span,
                    }),
                    Some(Value::Handle { index, generation }) => place.path.push(PStep::PoolIdx {
                        index,
                        generation,
                        span: e.span,
                    }),
                    _ => return Ok(None),
                }
                Ok(Some(place))
            }
            _ => Ok(None),
        }
    }

    /// Read through a place (bounds and generation checks fire here).
    fn read_place(&mut self, place: &Place, span: Span) -> E<Value> {
        let root = self.frames[place.frame].locals[place.local].clone();
        // A `mut` parameter aliases the caller's place.
        if let Value::Ref(inner) = root {
            let mut chained = inner.clone();
            chained.path.extend(place.path.iter().cloned());
            return self.read_place(&chained, span);
        }
        self.walk_read(root, &place.path, span)
    }

    fn walk_read(&mut self, cur: Value, path: &[PStep], span: Span) -> E<Value> {
        let Some(step) = path.first() else {
            return Ok(cur);
        };
        let rest = &path[1..];
        match (step, cur) {
            (PStep::Field(f), Value::Struct { fields }) => {
                match fields.into_iter().find(|(n, _)| n == f) {
                    Some((_, v)) => self.walk_read(v, rest, span),
                    None => self.refuse("field access outside the modelled surface", span),
                }
            }
            (PStep::Field(f), Value::Shared(cell)) => {
                // Cell auto-deref: the payload's field.
                let v = self.cells[cell].value.clone();
                match v {
                    Value::Struct { fields } => match fields.into_iter().find(|(n, _)| n == f) {
                        Some((_, v)) => self.walk_read(v, rest, span),
                        None => self.refuse("field access outside the modelled surface", span),
                    },
                    _ => self.refuse("field access through this cell shape", span),
                }
            }
            (PStep::Field(f), Value::List(id)) if f == "len" => {
                let n = self.lists[id].len() as i64;
                self.walk_read(Value::Int(n), rest, span)
            }
            (PStep::ListIdx { index, span: isp }, Value::List(id)) => {
                let list = &self.lists[id];
                if *index < 0 || *index as usize >= list.len() {
                    return self.trap("bounds", "mem.ub.defined", *isp);
                }
                let v = list[*index as usize].clone();
                self.walk_read(v, rest, span)
            }
            (
                PStep::PoolIdx {
                    index,
                    generation,
                    span: isp,
                },
                Value::Pool(id),
            ) => {
                let pool = &self.pools[id];
                let stale = *index >= pool.len()
                    || pool[*index].generation != *generation
                    || !pool[*index].live;
                if stale {
                    return self.trap("stale-handle", "mem.shared.handle.2", *isp);
                }
                let v = pool[*index].value.clone();
                self.walk_read(v, rest, span)
            }
            (_, Value::Moved) => self.trap("use-after-move", "mem.tier0.move.2", span),
            _ => self.refuse("place projection outside the modelled surface", span),
        }
    }

    /// Write through a place.
    fn write_place(&mut self, place: &Place, v: Value, span: Span) -> E<()> {
        let root = self.frames[place.frame].locals[place.local].clone();
        if let Value::Ref(inner) = root {
            let mut chained = inner.clone();
            chained.path.extend(place.path.iter().cloned());
            return self.write_place(&chained, v, span);
        }
        if place.path.is_empty() {
            self.frames[place.frame].locals[place.local] = v;
            return Ok(());
        }
        // In-place update via a take/patch cycle (containers are
        // machine-arena values, so struct paths are frame-local).
        let mut root = std::mem::replace(
            &mut self.frames[place.frame].locals[place.local],
            Value::Moved,
        );
        let r = self.patch(&mut root, &place.path, v, span);
        self.frames[place.frame].locals[place.local] = root;
        r
    }

    fn patch(&mut self, cur: &mut Value, path: &[PStep], v: Value, span: Span) -> E<()> {
        let Some(step) = path.first() else {
            *cur = v;
            return Ok(());
        };
        let rest = &path[1..];
        match (step, &mut *cur) {
            (PStep::Field(f), Value::Struct { fields }) => {
                match fields.iter_mut().find(|(n, _)| n == f) {
                    Some((_, slot)) => self.patch_slot(slot, rest, v, span),
                    None => self.refuse("field write outside the modelled surface", span),
                }
            }
            (PStep::ListIdx { index, span: isp }, Value::List(id)) => {
                let id = *id;
                let (index, isp) = (*index, *isp);
                if index < 0 || index as usize >= self.lists[id].len() {
                    return self.trap("bounds", "mem.ub.defined", isp);
                }
                let mut elem = std::mem::replace(&mut self.lists[id][index as usize], Value::Moved);
                let r = self.patch(&mut elem, rest, v, span);
                self.lists[id][index as usize] = elem;
                r
            }
            (
                PStep::PoolIdx {
                    index,
                    generation,
                    span: isp,
                },
                Value::Pool(id),
            ) => {
                let id = *id;
                let (index, generation, isp) = (*index, *generation, *isp);
                let stale = index >= self.pools[id].len()
                    || self.pools[id][index].generation != generation
                    || !self.pools[id][index].live;
                if stale {
                    return self.trap("stale-handle", "mem.shared.handle.2", isp);
                }
                let mut elem = std::mem::replace(&mut self.pools[id][index].value, Value::Moved);
                let r = self.patch(&mut elem, rest, v, span);
                self.pools[id][index].value = elem;
                r
            }
            _ => self.refuse("place write outside the modelled surface", span),
        }
    }

    fn patch_slot(&mut self, slot: &mut Value, path: &[PStep], v: Value, span: Span) -> E<()> {
        if path.is_empty() {
            *slot = v;
            return Ok(());
        }
        let mut taken = std::mem::replace(slot, Value::Moved);
        let r = self.patch(&mut taken, path, v, span);
        *slot = taken;
        r
    }

    /// Use a place in value position: `Copy` copies, everything else
    /// moves out (`[mem.tier0.move.1]`; the static tier already
    /// guaranteed no later use).
    fn take_value(&mut self, place: &Place, span: Span) -> E<Value> {
        let v = self.read_place(place, span)?;
        if !v.is_copy() && place.path.is_empty() {
            // Whole-local move: mark the slot.
            let root = &mut self.frames[place.frame].locals[place.local];
            if !matches!(root, Value::Ref(_)) {
                *root = Value::Moved;
            }
        } else if !v.is_copy() {
            // Partial move: mark the field.
            self.write_place(place, Value::Moved, span)?;
        }
        Ok(v)
    }
}

// ---------------------------------------------------------- evaluator --

impl<'t> Machine<'t> {
    fn eval_block(&mut self, b: AstBlock<'t>, want_value: bool) -> E<Flow> {
        self.push_scope();
        let last_value = if want_value {
            b.trailing_expr().map(|e| e.span)
        } else {
            None
        };
        let mut out = Value::Unit;
        for stmt in b.statements() {
            self.tick()?;
            match stmt.kind {
                SyntaxKind::ExprStmt => {
                    let d = ExprStmt::cast(stmt).expect("kind");
                    if let Some(e) = d.expr() {
                        match self.eval(e)? {
                            Flow::Val(v) => {
                                if Some(e.span) == last_value {
                                    out = v;
                                }
                            }
                            other => {
                                self.close_scope(matches!(other, Flow::Err(_)))?;
                                return Ok(other);
                            }
                        }
                    }
                }
                SyntaxKind::LetDecl => {
                    let d = LetDecl::cast(stmt).expect("kind");
                    match self.bind_decl(d.pattern(), d.init())? {
                        Flow::Val(_) => {}
                        other => {
                            self.close_scope(matches!(other, Flow::Err(_)))?;
                            return Ok(other);
                        }
                    }
                }
                SyntaxKind::VarDecl => {
                    let d = VarDecl::cast(stmt).expect("kind");
                    match self.bind_decl(d.pattern(), d.init())? {
                        Flow::Val(_) => {}
                        other => {
                            self.close_scope(matches!(other, Flow::Err(_)))?;
                            return Ok(other);
                        }
                    }
                }
                SyntaxKind::ConstDecl => {
                    let d = wolf_ast::ConstDecl::cast(stmt).expect("kind");
                    let flow = match d.init() {
                        Some(init) => self.eval(init)?,
                        None => Flow::Val(Value::Uninit),
                    };
                    match flow {
                        Flow::Val(v) => {
                            if let Some(name) = d.name() {
                                let n = self.text(name.span);
                                self.declare(&n, v);
                            }
                        }
                        other => {
                            self.close_scope(matches!(other, Flow::Err(_)))?;
                            return Ok(other);
                        }
                    }
                }
                SyntaxKind::AssignStmt => match self.eval_assign(stmt)? {
                    Flow::Val(_) => {}
                    other => {
                        self.close_scope(matches!(other, Flow::Err(_)))?;
                        return Ok(other);
                    }
                },
                SyntaxKind::DeferStmt => {
                    let d = DeferStmt::cast(stmt).expect("kind");
                    if let Some(e) = d.expr() {
                        let is_err = d.is_errdefer();
                        self.frame()
                            .scopes
                            .last_mut()
                            .expect("scope")
                            .cleanup
                            .push(Cleanup::Defer(e, is_err));
                    }
                }
                SyntaxKind::AssumeStmt => match self.eval_assume(stmt)? {
                    Flow::Val(_) => {}
                    other => {
                        self.close_scope(matches!(other, Flow::Err(_)))?;
                        return Ok(other);
                    }
                },
                k if k.is_item() => {
                    return self.refuse("nested item declarations", stmt.span);
                }
                _ => {}
            }
        }
        self.close_scope(false)?;
        Ok(Flow::Val(out))
    }

    fn bind_decl(&mut self, pat: Option<&'t GreenNode>, init: Option<&'t GreenNode>) -> E<Flow> {
        let v = match init {
            Some(e) => val!(self.eval(e)),
            None => Value::Uninit,
        };
        if let Some(pat) = pat {
            self.bind_pattern(pat, v)?;
        }
        Ok(Flow::Val(Value::Unit))
    }

    fn bind_pattern(&mut self, pat: &'t GreenNode, v: Value) -> E<()> {
        let mut binds = Vec::new();
        collect_binding_spans(pat, &mut binds);
        if binds.is_empty() {
            return Ok(()); // wildcard
        }
        if binds.len() > 1 {
            return self.refuse("destructuring bindings in checked execution", pat.span);
        }
        let name = self.text(binds[0]);
        let idx = self.declare(&name, v);
        // Scope-exit obligations by value shape.
        let cleanup = match &self.frames.last().expect("frame").locals[idx] {
            Value::Shared(_) | Value::Weak(_) => Some(Cleanup::DropLocal(idx)),
            Value::Region(_) => Some(Cleanup::FreeRegionLocal(idx)),
            _ => None,
        };
        if let Some(c) = cleanup {
            self.frame()
                .scopes
                .last_mut()
                .expect("scope")
                .cleanup
                .push(c);
        }
        Ok(())
    }

    fn eval_assign(&mut self, stmt: &'t GreenNode) -> E<Flow> {
        let d = AssignStmt::cast(stmt).expect("kind");
        let Some(place_expr) = d.place() else {
            return Ok(Flow::Val(Value::Unit));
        };
        let compound = d.op().map(|t| t.kind != SyntaxKind::Eq).unwrap_or(false);
        // Raw-pointer element write: the raw tier owns it.
        if place_expr.kind == SyntaxKind::BracketApply
            && let Some(b) = BracketApply::cast(place_expr)
            && let Some(recv) = b.callee()
            && matches!(self.expr_ty(recv.span), Some(TyKind::Ptr(_)))
        {
            let v = match d.value() {
                Some(e) => val!(self.eval(e)),
                None => Value::Unit,
            };
            return self.raw_index_write(place_expr, v, compound, stmt.span);
        }
        let v = match d.value() {
            Some(e) => val!(self.eval(e)),
            None => Value::Unit,
        };
        let Some(place) = self.place_of(place_expr)? else {
            return self.refuse("assignment through this place shape", place_expr.span);
        };
        if compound {
            let cur = self.read_place(&place, place_expr.span)?;
            let op = d.op().map(|t| t.kind).expect("compound op");
            // The value expression's recorded type carries the
            // checked range (the place itself may not be a recorded
            // expression).
            let ty_span = d.value().map(|x| x.span).unwrap_or(place_expr.span);
            let combined = self.arith_binop(op, cur, v, stmt.span, ty_span)?;
            self.write_place(&place, combined, place_expr.span)?;
        } else {
            self.write_place(&place, v, place_expr.span)?;
        }
        Ok(Flow::Val(Value::Unit))
    }

    fn eval(&mut self, e: &'t GreenNode) -> E<Flow> {
        self.tick()?;
        match e.kind {
            SyntaxKind::LiteralExpr => Ok(Flow::Val(self.literal(e)?)),
            SyntaxKind::StringExpr => self.eval_string(e),
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.eval(inner),
                None => Ok(Flow::Val(Value::Unit)),
            },
            SyntaxKind::PathExpr | SyntaxKind::MemberExpr => {
                if let Some(place) = self.place_of(e)? {
                    let v = self.take_value(&place, e.span)?;
                    return Ok(Flow::Val(v));
                }
                // Not a local place: a module item (global const) or
                // an unmodelled member.
                self.item_value(e)
            }
            SyntaxKind::BracketApply => {
                let b = BracketApply::cast(e).expect("kind");
                if let Some(recv) = b.callee()
                    && matches!(self.expr_ty(recv.span), Some(TyKind::Ptr(_)))
                {
                    return self.raw_index_read(e);
                }
                if let Some(place) = self.place_of(e)? {
                    let v = self.read_place(&place, e.span)?;
                    return Ok(Flow::Val(v));
                }
                self.refuse("indexing outside the modelled surface", e.span)
            }
            SyntaxKind::Block => {
                let b = AstBlock::cast(e).expect("kind");
                self.eval_block(b, true)
            }
            SyntaxKind::TupleExpr => {
                let mut fields = Vec::new();
                for (i, elem) in TupleExpr::cast(e).expect("kind").elems().enumerate() {
                    let v = val!(self.eval(elem));
                    fields.push((format!("{i}"), v));
                }
                Ok(Flow::Val(Value::Struct { fields }))
            }
            SyntaxKind::PrefixExpr => self.eval_prefix(e),
            SyntaxKind::BinExpr => self.eval_bin(e),
            SyntaxKind::CastExpr => self.eval_cast(e),
            SyntaxKind::RangeExpr => {
                let d = RangeExpr::cast(e).expect("kind");
                let mut ends = d.endpoints();
                let start = match ends.next() {
                    Some(x) => val!(self.eval(x)),
                    None => Value::Int(0),
                };
                let end = match ends.next() {
                    Some(x) => val!(self.eval(x)),
                    None => Value::Int(0),
                };
                let (Value::Int(s), Value::Int(mut en)) = (start, end) else {
                    return self.refuse("non-integer ranges", e.span);
                };
                if d.is_inclusive() {
                    en += 1;
                }
                Ok(Flow::Val(Value::Range { start: s, end: en }))
            }
            SyntaxKind::TryExpr => {
                let d = wolf_ast::TryExpr::cast(e).expect("kind");
                let inner = match d.expr() {
                    Some(x) => self.eval(x)?,
                    None => Flow::Val(Value::Unit),
                };
                match inner {
                    Flow::Err(err) => Ok(Flow::Err(err)),
                    other => Ok(other),
                }
            }
            SyntaxKind::CallExpr => self.eval_call(e),
            SyntaxKind::StructLit => {
                let d = StructLit::cast(e).expect("kind");
                let mut fields = Vec::new();
                for f in d.fields() {
                    if let Some(v) = FieldInit::value(f) {
                        let fv = val!(self.eval(v));
                        let name = f
                            .name()
                            .map(|t| self.text(t.span))
                            .unwrap_or_else(|| format!("{}", fields.len()));
                        fields.push((name, fv));
                    }
                }
                Ok(Flow::Val(Value::Struct { fields }))
            }
            SyntaxKind::IfExpr => self.eval_if(e),
            SyntaxKind::MatchExpr => self.eval_match(e),
            SyntaxKind::WhileExpr => self.eval_while(e),
            SyntaxKind::ForExpr => self.eval_for(e),
            SyntaxKind::LoopExpr => {
                let d = wolf_ast::LoopExpr::cast(e).expect("kind");
                loop {
                    self.tick()?;
                    if let Some(b) = d.body() {
                        match self.eval_block(b, false)? {
                            Flow::Break => break,
                            Flow::Continue | Flow::Val(_) => {}
                            other => return Ok(other),
                        }
                    }
                }
                Ok(Flow::Val(Value::Unit))
            }
            SyntaxKind::ElseExpr => self.eval_else(e),
            SyntaxKind::ReturnExpr => {
                let d = ReturnExpr::cast(e).expect("kind");
                let v = match d.value() {
                    Some(x) => val!(self.eval(x)),
                    None => Value::Unit,
                };
                // Scopes close (defers run, LIFO) as the flow unwinds
                // through each block; call_body closes the remainder.
                Ok(Flow::Return(v))
            }
            SyntaxKind::BreakExpr => Ok(Flow::Break),
            SyntaxKind::ContinueExpr => Ok(Flow::Continue),
            SyntaxKind::UnsafeBlock => {
                let d = UnsafeBlock::cast(e).expect("kind");
                match d.body() {
                    Some(b) => self.eval_block(b, true),
                    None => Ok(Flow::Val(Value::Unit)),
                }
            }
            SyntaxKind::RegionBlock => self.eval_region_block(e),
            SyntaxKind::RegionValue => {
                let rid = self.regions.len();
                self.regions.push(DynRegion {
                    live: true,
                    frozen: false,
                    backing: None,
                    span: e.span,
                });
                Ok(Flow::Val(Value::Region(rid)))
            }
            SyntaxKind::InBlock => {
                let d = InBlock::cast(e).expect("kind");
                let rid = match d.region() {
                    Some(r) => match val!(self.eval_region_ref(r)) {
                        Value::Region(rid) => rid,
                        _ => return self.refuse("`in` over a non-region value", e.span),
                    },
                    None => return self.refuse("`in` without a region", e.span),
                };
                self.ambient.push(rid);
                let out = match d.body() {
                    Some(b) => self.eval_block(b, true),
                    None => Ok(Flow::Val(Value::Unit)),
                };
                self.ambient.pop();
                out
            }
            SyntaxKind::FreezeExpr => {
                let d = wolf_ast::FreezeExpr::cast(e).expect("kind");
                let Some(operand) = d.expr() else {
                    return Ok(Flow::Val(Value::Unit));
                };
                let v = val!(self.eval(operand));
                match v {
                    Value::Region(rid) => {
                        self.freeze_region(rid);
                        Ok(Flow::Val(Value::Region(rid)))
                    }
                    _ => self.refuse("freeze of a non-region value", e.span),
                }
            }
            SyntaxKind::BorrowExpr => self.eval_door(e),
            SyntaxKind::ClosureExpr => self.refuse("closures in checked execution (c05)", e.span),
            SyntaxKind::ScopeExpr
            | SyntaxKind::SelectExpr
            | SyntaxKind::WhenExpr
            | SyntaxKind::SpawnExpr => self.refuse(
                "structured concurrency in checked execution (C1 deferred)",
                e.span,
            ),
            SyntaxKind::InlineC | SyntaxKind::AsmExpr => {
                self.refuse("inline C / asm (c10)", e.span)
            }
            _ => self.refuse("this expression shape in checked execution", e.span),
        }
    }

    /// A region-denoting expression (`in r { }`'s target): a place
    /// read that does NOT move the affine value (opening is not
    /// consumption).
    fn eval_region_ref(&mut self, e: &'t GreenNode) -> E<Flow> {
        if let Some(place) = self.place_of(e)? {
            let v = self.read_place(&place, e.span)?;
            return Ok(Flow::Val(v));
        }
        self.eval(e)
    }

    fn eval_region_block(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = RegionBlock::cast(e).expect("kind");
        let rid = self.regions.len();
        self.regions.push(DynRegion {
            live: true,
            frozen: false,
            backing: None,
            span: e.span,
        });
        self.ambient.push(rid);
        // The sugar block's name is usable inside it (`in a { }`
        // re-opens); the block itself owns the free, so the binding
        // carries no cleanup.
        self.push_scope();
        if let Some(name) = d.name() {
            let n = self.text(name.span);
            self.declare(&n, Value::Region(rid));
        }
        let out = match d.body() {
            Some(b) => self.eval_block(b, true),
            None => Ok(Flow::Val(Value::Unit)),
        };
        match &out {
            Ok(Flow::Err(_)) => self.close_scope(true)?,
            _ => self.close_scope(false)?,
        }
        self.ambient.pop();
        // The sugar-block exit is the wholesale free
        // ([mem.region.intra.2]).
        self.free_region(rid);
        out
    }

    fn eval_if(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = IfExpr::cast(e).expect("kind");
        let cond = match d.condition() {
            Some(c) => val!(self.eval(c)),
            None => Value::Bool(false),
        };
        let Value::Bool(b) = cond else {
            return self.refuse("non-boolean condition", e.span);
        };
        if b {
            match d.then_block() {
                Some(tb) => self.eval_block(tb, true),
                None => Ok(Flow::Val(Value::Unit)),
            }
        } else {
            match d.else_branch() {
                Some(el) if el.kind == SyntaxKind::Block => {
                    self.eval_block(AstBlock::cast(el).expect("kind"), true)
                }
                Some(el) => self.eval(el),
                None => Ok(Flow::Val(Value::Unit)),
            }
        }
    }

    fn eval_match(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = MatchExpr::cast(e).expect("kind");
        let scrut = match d.scrutinee() {
            Some(s) => val!(self.eval(s)),
            None => Value::Unit,
        };
        for arm in d.arms() {
            let Some(pat) = arm.pattern() else { continue };
            let bind = match self.match_pattern(pat, &scrut)? {
                Some(b) => b,
                None => continue,
            };
            self.push_scope();
            if let Some((name, v)) = bind {
                self.declare(&name, v);
            }
            if let Some(guard) = arm.guard() {
                let g = match self.eval(guard)? {
                    Flow::Val(Value::Bool(g)) => g,
                    Flow::Val(_) => false,
                    other => {
                        self.close_scope(false)?;
                        return Ok(other);
                    }
                };
                if !g {
                    self.close_scope(false)?;
                    continue;
                }
            }
            let out = match arm.body() {
                Some(body) => self.eval(body)?,
                None => Flow::Val(Value::Unit),
            };
            self.close_scope(matches!(out, Flow::Err(_)))?;
            return Ok(out);
        }
        Ok(Flow::Val(Value::Unit))
    }

    /// Match a pattern against a value: `None` = no match;
    /// `Some(None)` = matched, no binding; `Some(Some((name, v)))` =
    /// matched with one binding. Literal, wildcard, and single-ident
    /// patterns only — the modelled subset.
    #[allow(clippy::type_complexity)]
    fn match_pattern(
        &mut self,
        pat: &'t GreenNode,
        scrut: &Value,
    ) -> E<Option<Option<(String, Value)>>> {
        match pat.kind {
            SyntaxKind::WildcardPat => Ok(Some(None)),
            SyntaxKind::LiteralPat => {
                let text = self.text(pat.span);
                let matched = match scrut {
                    Value::Int(n) => parse_int_literal(&text) == Some(*n),
                    Value::Bool(b) => text == if *b { "true" } else { "false" },
                    Value::Str(s) => text.len() >= 2 && &text[1..text.len() - 1] == s.as_str(),
                    _ => false,
                };
                Ok(if matched { Some(None) } else { None })
            }
            SyntaxKind::IdentPat | SyntaxKind::BindingPat => {
                let name = self.text(pat.span);
                Ok(Some(Some((name, scrut.clone()))))
            }
            _ => self.refuse("this pattern shape in checked execution", pat.span),
        }
    }

    fn eval_while(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = WhileExpr::cast(e).expect("kind");
        loop {
            self.tick()?;
            let cond = match d.condition() {
                Some(c) => val!(self.eval(c)),
                None => Value::Bool(false),
            };
            let Value::Bool(b) = cond else {
                return self.refuse("non-boolean condition", e.span);
            };
            if !b {
                break;
            }
            if let Some(body) = d.body() {
                match self.eval_block(body, false)? {
                    Flow::Break => break,
                    Flow::Continue | Flow::Val(_) => {}
                    other => return Ok(other),
                }
            }
        }
        Ok(Flow::Val(Value::Unit))
    }

    fn eval_for(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = ForExpr::cast(e).expect("kind");
        let iter = match d.iterable() {
            Some(it) => val!(self.eval(it)),
            None => Value::Unit,
        };
        let items: Vec<Value> = match iter {
            Value::Range { start, end } => (start..end).map(Value::Int).collect(),
            Value::List(id) => self.lists[id].clone(),
            _ => return self.refuse("iteration outside ranges and List", e.span),
        };
        for item in items {
            self.tick()?;
            self.push_scope();
            if let Some(pat) = d.pattern() {
                self.bind_pattern(pat, item)?;
            }
            if let Some(body) = d.body() {
                match self.eval_block(body, false)? {
                    Flow::Break => {
                        self.close_scope(false)?;
                        return Ok(Flow::Val(Value::Unit));
                    }
                    Flow::Continue | Flow::Val(_) => {}
                    other => {
                        self.close_scope(matches!(other, Flow::Err(_)))?;
                        return Ok(other);
                    }
                }
            }
            self.close_scope(false)?;
        }
        Ok(Flow::Val(Value::Unit))
    }

    fn eval_else(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = ElseExpr::cast(e).expect("kind");
        let scrut = match d.scrutinized() {
            Some(s) => self.eval(s)?,
            None => Flow::Val(Value::Unit),
        };
        match scrut {
            Flow::Err(err) => {
                self.push_scope();
                if let Some(pat) = d.handler_pattern() {
                    self.bind_pattern(pat, err)?;
                }
                let out = match d.fallback() {
                    Some(fb) => self.eval(fb)?,
                    None => Flow::Val(Value::Unit),
                };
                self.close_scope(matches!(out, Flow::Err(_)))?;
                Ok(out)
            }
            other => Ok(other),
        }
    }

    fn eval_prefix(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = PrefixExpr::cast(e).expect("kind");
        let Some(operand) = d.operand() else {
            return Ok(Flow::Val(Value::Unit));
        };
        match d.op().map(|t| t.kind) {
            Some(SyntaxKind::CopyKw) => {
                // `copy x`: an independent deep duplicate.
                let v = if let Some(place) = self.place_of(operand)? {
                    self.read_place(&place, operand.span)?
                } else {
                    val!(self.eval(operand))
                };
                let copied = self.deep_copy(v)?;
                Ok(Flow::Val(copied))
            }
            Some(SyntaxKind::MoveKw) => {
                if let Some(place) = self.place_of(operand)? {
                    let v = self.take_value(&place, operand.span)?;
                    Ok(Flow::Val(v))
                } else {
                    self.eval(operand)
                }
            }
            Some(SyntaxKind::SharedKw) => {
                let v = val!(self.eval(operand));
                let cell = self.cells.len();
                self.cells.push(RcCell {
                    strong: 1,
                    weak: 0,
                    value: v,
                });
                Ok(Flow::Val(Value::Shared(cell)))
            }
            Some(SyntaxKind::Minus) => {
                let v = val!(self.eval(operand));
                match v {
                    Value::Int(n) => match n.checked_neg() {
                        Some(m) => Ok(Flow::Val(Value::Int(m))),
                        None => self.trap("overflow", "mem.ub.defined", e.span),
                    },
                    _ => self.refuse("negation outside integers", e.span),
                }
            }
            Some(SyntaxKind::Not) => {
                let v = val!(self.eval(operand));
                match v {
                    Value::Bool(b) => Ok(Flow::Val(Value::Bool(!b))),
                    _ => self.refuse("`!` outside booleans", e.span),
                }
            }
            _ => self.eval(operand),
        }
    }

    fn deep_copy(&mut self, v: Value) -> E<Value> {
        Ok(match v {
            Value::List(id) => {
                let elems = self.lists[id].clone();
                let mut copied = Vec::with_capacity(elems.len());
                for e in elems {
                    copied.push(self.deep_copy(e)?);
                }
                let nid = self.lists.len();
                self.lists.push(copied);
                Value::List(nid)
            }
            Value::Pool(id) => {
                let slots = self.pools[id].clone();
                let nid = self.pools.len();
                self.pools.push(slots);
                Value::Pool(nid)
            }
            Value::Struct { fields } => {
                let mut out = Vec::with_capacity(fields.len());
                for (n, fv) in fields {
                    out.push((n, self.deep_copy(fv)?));
                }
                Value::Struct { fields: out }
            }
            Value::Shared(c) => {
                self.cells[c].strong += 1;
                Value::Shared(c)
            }
            other => other,
        })
    }

    fn eval_bin(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = wolf_ast::BinExpr::cast(e).expect("kind");
        let op = d.op().map(|t| t.kind);
        // Short-circuit forms first.
        if matches!(op, Some(SyntaxKind::AmpAmp | SyntaxKind::PipePipe)) {
            let l = match d.lhs() {
                Some(l) => val!(self.eval(l)),
                None => Value::Bool(false),
            };
            let Value::Bool(lb) = l else {
                return self.refuse("non-boolean logic operand", e.span);
            };
            let and = op == Some(SyntaxKind::AmpAmp);
            if (and && !lb) || (!and && lb) {
                return Ok(Flow::Val(Value::Bool(lb)));
            }
            let r = match d.rhs() {
                Some(r) => val!(self.eval(r)),
                None => Value::Bool(false),
            };
            return Ok(Flow::Val(r));
        }
        let l = match d.lhs() {
            Some(l) => val!(self.eval(l)),
            None => Value::Unit,
        };
        let r = match d.rhs() {
            Some(r) => val!(self.eval(r)),
            None => Value::Unit,
        };
        let Some(op) = op else {
            return Ok(Flow::Val(l));
        };
        match op {
            SyntaxKind::Plus
            | SyntaxKind::Minus
            | SyntaxKind::Star
            | SyntaxKind::Slash
            | SyntaxKind::Percent => {
                let v = self.arith_binop_at(op, l, r, e.span, e.span)?;
                Ok(Flow::Val(v))
            }
            SyntaxKind::EqEq | SyntaxKind::NotEq => {
                let eq = values_equal(&l, &r);
                let want = op == SyntaxKind::EqEq;
                Ok(Flow::Val(Value::Bool(eq == want)))
            }
            SyntaxKind::Lt | SyntaxKind::Gt | SyntaxKind::LtEq | SyntaxKind::GtEq => {
                let (Value::Int(a), Value::Int(b)) = (l, r) else {
                    return self.refuse("ordering outside integers", e.span);
                };
                let out = match op {
                    SyntaxKind::Lt => a < b,
                    SyntaxKind::Gt => a > b,
                    SyntaxKind::LtEq => a <= b,
                    _ => a >= b,
                };
                Ok(Flow::Val(Value::Bool(out)))
            }
            _ => self.refuse("this operator in checked execution", e.span),
        }
    }

    /// Compound-assignment arithmetic reuses the checked core with the
    /// statement's span for the trap site.
    fn arith_binop(
        &mut self,
        op: SyntaxKind,
        l: Value,
        r: Value,
        span: Span,
        ty_span: Span,
    ) -> E<Value> {
        let op = match op {
            SyntaxKind::PlusEq => SyntaxKind::Plus,
            SyntaxKind::MinusEq => SyntaxKind::Minus,
            SyntaxKind::StarEq => SyntaxKind::Star,
            SyntaxKind::SlashEq => SyntaxKind::Slash,
            SyntaxKind::PercentEq => SyntaxKind::Percent,
            other => other,
        };
        self.arith_binop_at(op, l, r, span, ty_span)
    }

    fn arith_binop_at(
        &mut self,
        op: SyntaxKind,
        l: Value,
        r: Value,
        span: Span,
        ty_span: Span,
    ) -> E<Value> {
        match (l, r) {
            (Value::Int(a), Value::Int(b)) => {
                // Wrapping types wrap at their width; checked prims
                // trap (X3).
                if let Some((lo, hi, mask)) = self
                    .wrapping_width(ty_span)
                    .or_else(|| self.wrapping_width(span))
                {
                    let _ = (lo, hi);
                    let out = match op {
                        SyntaxKind::Plus => a.wrapping_add(b),
                        SyntaxKind::Minus => a.wrapping_sub(b),
                        SyntaxKind::Star => a.wrapping_mul(b),
                        SyntaxKind::Slash => {
                            if b == 0 {
                                return self.trap("div-zero", "mem.ub.defined", span);
                            }
                            a.wrapping_div(b)
                        }
                        SyntaxKind::Percent => {
                            if b == 0 {
                                return self.trap("div-zero", "mem.ub.defined", span);
                            }
                            a.wrapping_rem(b)
                        }
                        _ => a,
                    };
                    return Ok(Value::Int(out & mask));
                }
                let out = match op {
                    SyntaxKind::Plus => a.checked_add(b),
                    SyntaxKind::Minus => a.checked_sub(b),
                    SyntaxKind::Star => a.checked_mul(b),
                    SyntaxKind::Slash => {
                        if b == 0 {
                            return self.trap("div-zero", "mem.ub.defined", span);
                        }
                        a.checked_div(b)
                    }
                    SyntaxKind::Percent => {
                        if b == 0 {
                            return self.trap("div-zero", "mem.ub.defined", span);
                        }
                        a.checked_rem(b)
                    }
                    _ => Some(a),
                };
                let Some(out) = out else {
                    return self.trap("overflow", "mem.ub.defined", span);
                };
                // Narrow prim ranges trap too.
                if let Some((lo, hi)) = self.prim_range(ty_span).or_else(|| self.prim_range(span))
                    && (out < lo || out > hi)
                {
                    return self.trap("overflow", "mem.ub.defined", span);
                }
                Ok(Value::Int(out))
            }
            (Value::Str(a), Value::Str(b)) if op == SyntaxKind::Plus => Ok(Value::Str(a + &b)),
            _ => self.refuse("arithmetic outside integers", span),
        }
    }

    /// Is the expression at `span` a `wrapping[T]`? Returns the wrap
    /// mask when so.
    fn wrapping_width(&self, span: Span) -> Option<(i64, i64, i64)> {
        let ctx = self.ctx();
        let id = ctx.expr_tys.get(&span)?;
        if let TyKind::Wrapping(inner) = ctx.tb.table.kind(*id)
            && let TyKind::Prim(p) = ctx.tb.table.kind(*inner)
        {
            let bits = prim_bits(*p)?;
            let mask = if bits >= 64 {
                -1i64
            } else {
                (1i64 << bits) - 1
            };
            return Some((0, mask, mask));
        }
        None
    }

    /// The checked range of the expression's prim type, when narrower
    /// than i64.
    fn prim_range(&self, span: Span) -> Option<(i64, i64)> {
        let ctx = self.ctx();
        let id = ctx.expr_tys.get(&span)?;
        if let TyKind::Prim(p) = ctx.tb.table.kind(*id) {
            return prim_range(*p);
        }
        None
    }

    fn literal(&mut self, e: &'t GreenNode) -> E<Value> {
        let text = self.text(e.span);
        if text == "true" {
            return Ok(Value::Bool(true));
        }
        if text == "false" {
            return Ok(Value::Bool(false));
        }
        match parse_int_literal(&text) {
            Some(n) => Ok(Value::Int(n)),
            None => self.refuse("this literal shape in checked execution", e.span),
        }
    }

    fn eval_string(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = StringExpr::cast(e).expect("kind");
        // Rebuild: literal segments from source, values spliced at
        // interpolation holes ({x} f-strings, D26).
        let raw = self.text(e.span);
        let base = e.span.lo;
        let mut holes: Vec<(u32, u32, String)> = Vec::new();
        for i in d.interps() {
            let ispan = i.syntax().span;
            let v = match i.expr() {
                Some(hole) => {
                    let hv = if let Some(place) = self.place_of(hole)? {
                        self.read_place(&place, hole.span)?
                    } else {
                        val!(self.eval(hole))
                    };
                    format_value(&hv)
                }
                None => String::new(),
            };
            holes.push((ispan.lo - base, ispan.hi - base, v));
        }
        let bytes = raw.as_bytes();
        let mut out = String::new();
        let mut i = 0usize;
        // Strip the surrounding quotes.
        let (start, end) = if bytes.len() >= 2 {
            (1usize, bytes.len() - 1)
        } else {
            (0, bytes.len())
        };
        i = i.max(start);
        while i < end {
            if let Some((_, hi, v)) = holes.iter().find(|(lo, _, _)| *lo as usize == i) {
                out.push_str(v);
                i = *hi as usize;
                continue;
            }
            let c = bytes[i];
            if c == b'\\' && i + 1 < end {
                let esc = bytes[i + 1];
                out.push(match esc {
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'{' => '{',
                    b'}' => '}',
                    b'0' => '\0',
                    other => other as char,
                });
                i += 2;
                continue;
            }
            out.push(c as char);
            i += 1;
        }
        Ok(Flow::Val(Value::Str(out)))
    }

    /// A path that is not a local: a module global (item initializer)
    /// or an unmodelled reference.
    fn item_value(&mut self, e: &'t GreenNode) -> E<Flow> {
        // Member of a temporary: evaluate the base and project.
        if e.kind == SyntaxKind::MemberExpr {
            let m = MemberExpr::cast(e).expect("kind");
            if let (Some(base), Some(member)) = (m.base(), m.member()) {
                let field = self.text(member.span);
                let bv = val!(self.eval(base));
                return match bv {
                    Value::Struct { fields } => match fields.into_iter().find(|(n, _)| n == &field)
                    {
                        Some((_, v)) => Ok(Flow::Val(v)),
                        None => self.refuse("field access outside the modelled surface", e.span),
                    },
                    Value::List(id) if field == "len" => {
                        Ok(Flow::Val(Value::Int(self.lists[id].len() as i64)))
                    }
                    Value::Shared(c) => {
                        let payload = self.cells[c].value.clone();
                        match payload {
                            Value::Struct { fields } => {
                                match fields.into_iter().find(|(n, _)| n == &field) {
                                    Some((_, v)) => Ok(Flow::Val(v)),
                                    None => self.refuse(
                                        "field access outside the modelled surface",
                                        e.span,
                                    ),
                                }
                            }
                            _ => self.refuse("cell payload projection", e.span),
                        }
                    }
                    _ => self.refuse("member access outside the modelled surface", e.span),
                };
            }
        }
        self.refuse("module items in checked execution", e.span)
    }

    // ---------------------------------------------------- raw tier --

    /// Pointee byte width of a raw access at `span` (the element
    /// expression's own type).
    fn pointee_size(&self, span: Span) -> u64 {
        match self.expr_ty(span) {
            Some(TyKind::Prim(p)) => prim_size(*p),
            _ => 1,
        }
    }

    fn raw_index_parts(&mut self, e: &'t GreenNode) -> E<(PtrVal, i64)> {
        let b = BracketApply::cast(e).expect("kind");
        let recv = b.callee().expect("raw index receiver");
        let pv = if let Some(place) = self.place_of(recv)? {
            self.read_place(&place, recv.span)?
        } else {
            match self.eval(recv)? {
                Flow::Val(v) => v,
                _ => return self.refuse("control flow in a raw index", e.span),
            }
        };
        let Value::Ptr(p) = pv else {
            return self.refuse("raw index through a non-pointer", e.span);
        };
        let mut idx = 0i64;
        for a in b.args().into_iter().flat_map(|l| l.args()) {
            if let Some(v) = Arg::value(a)
                && wolf_ast::is_expr_kind(v.kind)
            {
                match self.eval(v)? {
                    Flow::Val(Value::Int(i)) => idx = i,
                    Flow::Val(_) => {}
                    _ => return self.refuse("control flow in a raw index", e.span),
                }
            }
        }
        Ok((p, idx))
    }

    fn raw_index_read(&mut self, e: &'t GreenNode) -> E<Flow> {
        let (p, idx) = self.raw_index_parts(e)?;
        let size = self.pointee_size(e.span);
        let at = PtrVal {
            offset: p.offset + idx * size as i64,
            addr: p.addr.wrapping_add((idx * size as i64) as u64),
            ..p
        };
        let bytes = self.raw_read_bytes(at, size, e.span, "a raw pointer read")?;
        let mut n: i64 = 0;
        for (i, b) in bytes.iter().enumerate() {
            n |= (*b as i64) << (8 * i);
        }
        // T1 — a restricted type produced from raw bytes must be a
        // valid value of that type.
        if matches!(self.expr_ty(e.span), Some(TyKind::Prim(Prim::Bool))) {
            if n > 1 {
                let tag_span = p.alloc.map(|a| self.allocs[a].span).unwrap_or(e.span);
                return self.ub(
                    UbRow::T1,
                    format!("this read produces `{n}` as a `bool` — not a valid value of the type"),
                    e.span,
                    tag_span,
                );
            }
            return Ok(Flow::Val(Value::Bool(n == 1)));
        }
        Ok(Flow::Val(Value::Int(n)))
    }

    fn raw_index_write(
        &mut self,
        place_expr: &'t GreenNode,
        v: Value,
        compound: bool,
        span: Span,
    ) -> E<Flow> {
        let (p, idx) = self.raw_index_parts(place_expr)?;
        let size = self.pointee_size(place_expr.span);
        let at = PtrVal {
            offset: p.offset + idx * size as i64,
            addr: p.addr.wrapping_add((idx * size as i64) as u64),
            ..p
        };
        let mut n = match v {
            Value::Int(n) => n,
            Value::Bool(b) => i64::from(b),
            _ => return self.refuse("raw write of a non-scalar", span),
        };
        if compound {
            let bytes = self.raw_read_bytes(at, size, span, "a raw pointer read")?;
            let mut cur: i64 = 0;
            for (i, b) in bytes.iter().enumerate() {
                cur |= (*b as i64) << (8 * i);
            }
            n += cur;
        }
        let data: Vec<u8> = (0..size).map(|i| ((n >> (8 * i)) & 0xff) as u8).collect();
        self.raw_write_bytes(at, &data, span, "a raw pointer write")?;
        Ok(Flow::Val(Value::Unit))
    }

    fn eval_cast(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = CastExpr::cast(e).expect("kind");
        let Some(inner) = d.expr() else {
            return Ok(Flow::Val(Value::Unit));
        };
        let kind = self.ctx().casts.get(&e.span).map(|(_, _, k)| *k);
        match kind {
            Some(CastKind::Raw) => {
                // Bridges are reads, never moves (deriving is not a
                // use).
                let v = if let Some(place) = self.place_of(inner)? {
                    self.read_place(&place, inner.span)?
                } else {
                    val!(self.eval(inner))
                };
                let (src_t, tgt_t) = {
                    let ctx = self.ctx();
                    let (s, t, _) = ctx.casts[&e.span];
                    (ctx.tb.table.kind(s).clone(), ctx.tb.table.kind(t).clone())
                };
                match (src_t, tgt_t, v) {
                    // ptr -> ptr: free retyping, same tag.
                    (TyKind::Ptr(_), TyKind::Ptr(_), Value::Ptr(p)) => Ok(Flow::Val(Value::Ptr(p))),
                    // ptr -> int: exposes the tag.
                    (TyKind::Ptr(_), _, Value::Ptr(p)) => {
                        if let Some(a) = p.alloc {
                            self.allocs[a].tags[p.tag as usize].exposed = true;
                        }
                        Ok(Flow::Val(Value::Int(p.addr as i64)))
                    }
                    // region -> ptr: the backing base.
                    (TyKind::RegionTy, _, Value::Region(rid)) => {
                        let p = self.region_backing(rid, e.span)?;
                        Ok(Flow::Val(Value::Ptr(p)))
                    }
                    // int -> ptr: angelic resolution among exposed
                    // tags ([mem.prov.expose]).
                    (_, TyKind::Ptr(_), Value::Int(n)) => {
                        let p = self.resolve_exposed(n as u64, e.span);
                        Ok(Flow::Val(Value::Ptr(p)))
                    }
                    _ => self.refuse("this raw bridge shape", e.span),
                }
            }
            _ => {
                let v = val!(self.eval(inner));
                // Numeric/adapter/identity casts are value-preserving
                // here; out-of-range narrowing traps (X3 posture).
                if let Value::Int(n) = v {
                    if let Some((lo, hi)) = self.prim_range(e.span)
                        && (n < lo || n > hi)
                    {
                        return self.trap("overflow", "mem.ub.defined", e.span);
                    }
                    return Ok(Flow::Val(Value::Int(n)));
                }
                Ok(Flow::Val(v))
            }
        }
    }

    fn eval_assume(&mut self, stmt: &'t GreenNode) -> E<Flow> {
        let d = wolf_ast::AssumeStmt::cast(stmt).expect("kind");
        let mut ptrs: Vec<(PtrVal, Span)> = Vec::new();
        for op in d.exprs() {
            let v = if let Some(place) = self.place_of(op)? {
                self.read_place(&place, op.span)?
            } else {
                val!(self.eval(op))
            };
            if let Value::Ptr(p) = v {
                ptrs.push((p, op.span));
            }
        }
        // P5 is checked where the assertion is written (the is04
        // reading): the reachable ranges [addr, allocation end) must
        // not overlap.
        for i in 0..ptrs.len() {
            for j in i + 1..ptrs.len() {
                let (a, _) = ptrs[i];
                let (b, _) = ptrs[j];
                let (Some(aa), Some(bb)) = (a.alloc, b.alloc) else {
                    continue;
                };
                if aa != bb {
                    continue;
                }
                let size = self.allocs[aa].size as i64;
                let (alo, ahi) = (a.offset, size);
                let (blo, bhi) = (b.offset, size);
                if alo < bhi && blo < ahi {
                    let origin = self.allocs[aa].span;
                    return self.ub(
                        UbRow::P5,
                        "this `assume noalias` is false: the asserted ranges overlap \
                         inside one allocation"
                            .to_string(),
                        stmt.span,
                        origin,
                    );
                }
            }
        }
        Ok(Flow::Val(Value::Unit))
    }

    /// Re-entry door 1 (`borrow r from p`): the P6 obligation checked
    /// at the door — p addresses a live allocation wholly inside r's
    /// footprint. A true claim yields the loaded value; a false one is
    /// UB at the door, never later.
    fn eval_door(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = BorrowExpr::cast(e).expect("kind");
        let rid = match d.borrowed() {
            Some(r) => match val!(self.eval_region_ref(r)) {
                Value::Region(rid) => Some(rid),
                _ => None,
            },
            None => None,
        };
        let ptr = match d.source() {
            Some(p) => {
                let v = if let Some(place) = self.place_of(p)? {
                    self.read_place(&place, p.span)?
                } else {
                    val!(self.eval(p))
                };
                match v {
                    Value::Ptr(p) => Some(p),
                    _ => None,
                }
            }
            None => None,
        };
        let (Some(rid), Some(p)) = (rid, ptr) else {
            return self.refuse("door operands outside the modelled surface", e.span);
        };
        let size = self.pointee_size(e.span).max(1);
        let claim_holds = match p.alloc {
            Some(aid) => {
                let a = &self.allocs[aid];
                a.live
                    && a.region == rid
                    && self.regions[rid].live
                    && p.offset >= 0
                    && (p.offset as u64).saturating_add(size) <= a.size
            }
            None => false,
        };
        if !claim_holds {
            let rspan = self.regions[rid].span;
            return self.ub(
                UbRow::P6,
                "false discharge of the `borrow … from …` door: the pointer does not \
                 address a live allocation inside the region's footprint"
                    .to_string(),
                e.span,
                rspan,
            );
        }
        // The door's read is a child read through the pointer's tag.
        let bytes = self.raw_read_bytes(p, size, e.span, "the door's borrow")?;
        let mut n: i64 = 0;
        for (i, b) in bytes.iter().enumerate() {
            n |= (*b as i64) << (8 * i);
        }
        Ok(Flow::Val(Value::Int(n)))
    }

    // -------------------------------------------------------- calls --

    fn eval_call(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = CallExpr::cast(e).expect("kind");
        let cs: Option<&CallSig> = self.ctx().calls.get(&e.span).copied();
        // C intrinsics (the is04 modelled set).
        if cs.map(|c| c.c_call).unwrap_or(false) {
            let name = cs.expect("c_call has sig").callee.clone();
            return self.eval_c_call(&name, e);
        }
        // Container constructors (`List[int]()`, `Pool[Node]()`).
        match self.expr_ty(e.span) {
            Some(TyKind::List(_)) if is_container_ctor(d.callee()) => {
                let id = self.lists.len();
                self.lists.push(Vec::new());
                return Ok(Flow::Val(Value::List(id)));
            }
            Some(TyKind::Pool(_)) if is_container_ctor(d.callee()) => {
                let id = self.pools.len();
                self.pools.push(Vec::new());
                return Ok(Flow::Val(Value::Pool(id)));
            }
            _ => {}
        }
        // Method calls (has_self): builtins on container/cell/pointer
        // receivers, else inherent user methods.
        if let Some(sig) = cs
            && sig.has_self
            && let Some(callee) = d.callee()
            && callee.kind == SyntaxKind::MemberExpr
            && let Some(m) = MemberExpr::cast(callee)
            && let Some(base) = m.base()
        {
            let recv_expr = match ParenExpr::cast(base) {
                Some(p) if p.mode().is_some() => p.expr().unwrap_or(base),
                _ => base,
            };
            return self.eval_method(sig, recv_expr, e, d.args());
        }
        // Enum-variant construction.
        if cs.map(|c| c.ctor).unwrap_or(false) {
            let mut payload = Vec::new();
            for a in d.args().into_iter().flat_map(|l| l.args()) {
                if let Some(v) = Arg::value(a) {
                    payload.push(val!(self.eval(v)));
                }
            }
            let variant = cs.map(|c| c.callee.clone()).unwrap_or_default();
            return Ok(Flow::Val(Value::Enum { variant, payload }));
        }
        // Builtins without a signature (`print`, `print_raw`,
        // `assert`).
        let callee_name = d.callee().map(|c| self.text(c.span)).unwrap_or_default();
        match callee_name.as_str() {
            "print" | "print_raw" => {
                let mut out = String::new();
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        let x = if let Some(place) = self.place_of(v)? {
                            self.read_place(&place, v.span)?
                        } else {
                            val!(self.eval(v))
                        };
                        out.push_str(&format_value(&x));
                    }
                }
                self.stdout.push_str(&out);
                if callee_name == "print" {
                    self.stdout.push('\n');
                }
                return Ok(Flow::Val(Value::Unit));
            }
            "assert" => {
                // Only the FIRST argument is the condition; the
                // optional second is the message, evaluated ONLY on
                // the failing path ([conf.trap.assert]). Treating the
                // message as a condition made every holding two-arg
                // assert trap (#19).
                let mut rest = d.args().into_iter().flat_map(|l| l.args());
                if let Some(first) = rest.next()
                    && let Some(v) = Arg::value(first)
                {
                    let x = val!(self.eval(v));
                    if !matches!(x, Value::Bool(true)) {
                        for a in rest {
                            if let Some(m) = Arg::value(a) {
                                val!(self.eval(m));
                            }
                        }
                        return self.trap("assert", "mem.ub.defined", e.span);
                    }
                }
                return Ok(Flow::Val(Value::Unit));
            }
            _ => {}
        }
        // A plain user fn call.
        let Some(sig) = cs else {
            return self.refuse("calls outside the modelled surface", e.span);
        };
        let module = self.tc.bodies[self.frames.last().expect("frame").body]
            .body
            .module;
        let Some(&body) = self.fns.get(&(module, sig.callee.clone())).or_else(|| {
            self.fns
                .iter()
                .find(|((_, n), _)| *n == sig.callee)
                .map(|(_, b)| b)
        }) else {
            return self.refuse("calls into unresolvable bodies", e.span);
        };
        let mut args = Vec::new();
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let Some(v) = Arg::value(a) else { continue };
            let mode = sig.params.get(i).and_then(|p| p.mode);
            args.push(self.eval_arg(v, mode)?);
        }
        let out = self.call_body(body, args)?;
        Ok(Flow::Val(out))
    }

    /// Evaluate one argument under its declared mode: `mut` lends the
    /// place (call-by-reference-result), `take` moves, `read` copies
    /// scalars and shares containers.
    fn eval_arg(&mut self, v: &'t GreenNode, mode: Option<wolf_ast::ParamMode>) -> E<Value> {
        // The call-site mode spelling wraps the value expression.
        let inner = match v.kind {
            SyntaxKind::PrefixExpr => {
                let p = PrefixExpr::cast(v).expect("kind");
                match p.op().map(|t| t.kind) {
                    Some(SyntaxKind::MutKw | SyntaxKind::TakeKw) => p.operand().unwrap_or(v),
                    _ => v,
                }
            }
            _ => v,
        };
        match mode {
            Some(wolf_ast::ParamMode::Mut) => {
                let Some(place) = self.place_of(inner)? else {
                    return self.refuse("`mut` of a non-place in checked execution", v.span);
                };
                // Raw-pointer arguments retag at parameter entry
                // (their tag travels in the value).
                let cur = self.read_place(&place, inner.span)?;
                if let Value::Ptr(p) = cur {
                    let child = self.retag(p, TagState::Active, inner.span)?;
                    return Ok(Value::Ptr(child));
                }
                Ok(Value::Ref(place))
            }
            Some(wolf_ast::ParamMode::Take) => {
                if let Some(place) = self.place_of(inner)? {
                    self.take_value(&place, inner.span)
                } else {
                    match self.eval(inner)? {
                        Flow::Val(x) => Ok(x),
                        _ => self.refuse("control flow in an argument", v.span),
                    }
                }
            }
            _ => {
                if let Some(place) = self.place_of(inner)? {
                    let cur = self.read_place(&place, inner.span)?;
                    if let Value::Ptr(p) = cur {
                        // `read` retag: a Frozen child, protected for
                        // the call ([mem.prov.tag]); writes through it
                        // inside the callee are P2.
                        let child = self.retag(p, TagState::Frozen, inner.span)?;
                        return Ok(Value::Ptr(child));
                    }
                    return Ok(cur);
                }
                match self.eval(inner)? {
                    Flow::Val(x) => Ok(x),
                    _ => self.refuse("control flow in an argument", v.span),
                }
            }
        }
    }

    fn retag(&mut self, p: PtrVal, state: TagState, span: Span) -> E<PtrVal> {
        let Some(aid) = p.alloc else {
            return Ok(p);
        };
        let child = self.allocs[aid].tags.len() as u32;
        self.allocs[aid].tags.push(Tag {
            parent: Some(p.tag),
            state,
            protected: 0,
            exposed: false,
            origin: span,
        });
        Ok(PtrVal { tag: child, ..p })
    }

    fn eval_method(
        &mut self,
        sig: &'t CallSig,
        recv: &'t GreenNode,
        e: &'t GreenNode,
        args: Option<wolf_ast::ArgList<'t>>,
    ) -> E<Flow> {
        let method = sig.callee.as_str();
        let recv_ty = self.expr_ty(recv.span).cloned();
        // Raw-pointer provenance ops.
        if matches!(recv_ty, Some(TyKind::Ptr(_))) {
            let pv = if let Some(place) = self.place_of(recv)? {
                self.read_place(&place, recv.span)?
            } else {
                val!(self.eval(recv))
            };
            let Value::Ptr(p) = pv else {
                return self.refuse("pointer op on a non-pointer", e.span);
            };
            let mut arg_vals = Vec::new();
            for a in args.into_iter().flat_map(|l| l.args()) {
                if let Some(v) = Arg::value(a) {
                    arg_vals.push(val!(self.eval(v)));
                }
            }
            return match method {
                "is_null" => Ok(Flow::Val(Value::Bool(p.addr == 0))),
                "addr" => Ok(Flow::Val(Value::Int(p.addr as i64))),
                "expose" => {
                    if let Some(a) = p.alloc {
                        self.allocs[a].tags[p.tag as usize].exposed = true;
                    }
                    Ok(Flow::Val(Value::Int(p.addr as i64)))
                }
                "with_addr" => {
                    let Some(Value::Int(n)) = arg_vals.first() else {
                        return self.refuse("with_addr without an address", e.span);
                    };
                    match p.alloc {
                        Some(aid) => {
                            let base = Allocation::base_addr(aid);
                            Ok(Flow::Val(Value::Ptr(PtrVal {
                                offset: *n - base as i64,
                                addr: *n as u64,
                                ..p
                            })))
                        }
                        None => Ok(Flow::Val(Value::Ptr(PtrVal {
                            alloc: None,
                            tag: 0,
                            offset: 0,
                            addr: *n as u64,
                        }))),
                    }
                }
                "with_exposed" => {
                    let Some(Value::Int(n)) = arg_vals.first() else {
                        return self.refuse("with_exposed without an address", e.span);
                    };
                    let out = self.resolve_exposed(*n as u64, e.span);
                    Ok(Flow::Val(Value::Ptr(out)))
                }
                _ => self.refuse("this pointer method", e.span),
            };
        }
        // Container/cell builtins by receiver type.
        match recv_ty {
            Some(TyKind::List(_)) => {
                let Some(place) = self.place_of(recv)? else {
                    return self.refuse("List method on a temporary", e.span);
                };
                let Value::List(id) = self.read_place(&place, recv.span)? else {
                    return self.refuse("List method on a non-list", e.span);
                };
                match method {
                    "push" => {
                        for a in args.into_iter().flat_map(|l| l.args()) {
                            if let Some(v) = Arg::value(a) {
                                let x = self.eval_arg(v, None)?;
                                self.charge_mem(16)?;
                                self.lists[id].push(x);
                            }
                        }
                        Ok(Flow::Val(Value::Unit))
                    }
                    "len" => Ok(Flow::Val(Value::Int(self.lists[id].len() as i64))),
                    _ => self.refuse("this List method", e.span),
                }
            }
            Some(TyKind::Pool(_)) => {
                let Some(place) = self.place_of(recv)? else {
                    return self.refuse("Pool method on a temporary", e.span);
                };
                let Value::Pool(id) = self.read_place(&place, recv.span)? else {
                    return self.refuse("Pool method on a non-pool", e.span);
                };
                match method {
                    "reserve" => {
                        self.charge_mem(32)?;
                        let index = self.pools[id].len();
                        self.pools[id].push(PoolSlot {
                            generation: 0,
                            live: true,
                            value: Value::Uninit,
                        });
                        Ok(Flow::Val(Value::Handle {
                            index,
                            generation: 0,
                        }))
                    }
                    "init" => {
                        let mut it = args.into_iter().flat_map(|l| l.args());
                        let h = match it.next().and_then(Arg::value) {
                            Some(v) => val!(self.eval(v)),
                            None => return self.refuse("pool.init without a handle", e.span),
                        };
                        let payload = match it.next().and_then(Arg::value) {
                            Some(v) => val!(self.eval(v)),
                            None => return self.refuse("pool.init without a value", e.span),
                        };
                        let Value::Handle { index, generation } = h else {
                            return self.refuse("pool.init with a non-handle", e.span);
                        };
                        let stale = index >= self.pools[id].len()
                            || self.pools[id][index].generation != generation
                            || !self.pools[id][index].live;
                        if stale {
                            return self.trap("stale-handle", "mem.shared.handle.2", e.span);
                        }
                        self.pools[id][index].value = payload;
                        Ok(Flow::Val(Value::Unit))
                    }
                    "remove" => {
                        let mut it = args.into_iter().flat_map(|l| l.args());
                        let h = match it.next().and_then(Arg::value) {
                            Some(v) => val!(self.eval(v)),
                            None => return self.refuse("pool.remove without a handle", e.span),
                        };
                        let Value::Handle { index, generation } = h else {
                            return self.refuse("pool.remove with a non-handle", e.span);
                        };
                        let stale = index >= self.pools[id].len()
                            || self.pools[id][index].generation != generation
                            || !self.pools[id][index].live;
                        if stale {
                            return self.trap("stale-handle", "mem.shared.handle.2", e.span);
                        }
                        // The slot's generation bumps: every extant
                        // handle goes stale (X5).
                        self.pools[id][index].live = false;
                        self.pools[id][index].generation += 1;
                        self.pools[id][index].value = Value::Uninit;
                        Ok(Flow::Val(Value::Unit))
                    }
                    _ => self.refuse("this Pool method", e.span),
                }
            }
            Some(TyKind::Shared(_)) => {
                let Some(place) = self.place_of(recv)? else {
                    return self.refuse("cell method on a temporary", e.span);
                };
                let Value::Shared(cell) = self.read_place(&place, recv.span)? else {
                    return self.refuse("cell method on a non-cell", e.span);
                };
                match method {
                    "clone" => {
                        self.cells[cell].strong += 1;
                        Ok(Flow::Val(Value::Shared(cell)))
                    }
                    "downgrade" => {
                        self.cells[cell].weak += 1;
                        Ok(Flow::Val(Value::Weak(cell)))
                    }
                    _ => self.refuse("this cell method", e.span),
                }
            }
            Some(TyKind::Weak(_)) => {
                let Some(place) = self.place_of(recv)? else {
                    return self.refuse("cell method on a temporary", e.span);
                };
                let Value::Weak(cell) = self.read_place(&place, recv.span)? else {
                    return self.refuse("weak method on a non-weak", e.span);
                };
                match method {
                    "upgrade" => {
                        if self.cells[cell].strong > 0 {
                            self.cells[cell].strong += 1;
                            Ok(Flow::Val(Value::Shared(cell)))
                        } else {
                            Ok(Flow::Err(Value::ErrTag {
                                tag: "gone".to_string(),
                                payload: Vec::new(),
                            }))
                        }
                    }
                    _ => self.refuse("this weak method", e.span),
                }
            }
            _ => {
                // An inherent user method (s17 dispatch record).
                let ty_name = match self.ctx().dispatch.get(&e.span) {
                    Some(Dispatch::Inherent { ty, .. }) => ty.clone(),
                    Some(Dispatch::Trait { .. }) => {
                        return self.refuse("trait dispatch in checked execution", e.span);
                    }
                    None => return self.refuse("this method call shape", e.span),
                };
                let Some(&body) = self.methods.get(&(ty_name, sig.callee.clone())) else {
                    return self.refuse("methods without resolvable bodies", e.span);
                };
                let self_mode = sig.params.first().and_then(|p| p.mode);
                let self_val = self.eval_arg(recv, self_mode)?;
                let mut call_args = vec![self_val];
                for (i, a) in args.into_iter().flat_map(|l| l.args()).enumerate() {
                    let Some(v) = Arg::value(a) else { continue };
                    let mode = sig.params.get(i + 1).and_then(|p| p.mode);
                    call_args.push(self.eval_arg(v, mode)?);
                }
                let out = self.call_body(body, call_args)?;
                Ok(Flow::Val(out))
            }
        }
    }

    fn eval_c_call(&mut self, name: &str, e: &'t GreenNode) -> E<Flow> {
        let d = CallExpr::cast(e).expect("kind");
        let mut args = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            if let Some(v) = Arg::value(a) {
                let x = if let Some(place) = self.place_of(v)? {
                    self.read_place(&place, v.span)?
                } else {
                    val!(self.eval(v))
                };
                args.push(x);
            }
        }
        // Passing a pointer to C exposes it ([mem.prov.expose]).
        for a in &args {
            if let Value::Ptr(p) = a
                && let Some(aid) = p.alloc
            {
                self.allocs[aid].tags[p.tag as usize].exposed = true;
            }
        }
        match name {
            "c.malloc" | "c.calloc" => {
                let n = match args.first() {
                    Some(Value::Int(n)) if *n >= 0 => *n as u64,
                    _ => return self.refuse("allocation size outside the model", e.span),
                };
                let size = if name == "c.calloc" {
                    let m = match args.get(1) {
                        Some(Value::Int(m)) if *m >= 0 => *m as u64,
                        _ => 1,
                    };
                    n.saturating_mul(m)
                } else {
                    n
                };
                let zeroed = name == "c.calloc";
                let region = *self.ambient.last().expect("ambient");
                // The wildcard-shaped C result: root tag exposed at
                // creation.
                let aid = self.new_alloc(size, region, true, zeroed, true, e.span)?;
                Ok(Flow::Val(Value::Ptr(PtrVal {
                    alloc: Some(aid),
                    tag: 0,
                    offset: 0,
                    addr: Allocation::base_addr(aid),
                })))
            }
            "c.free" => {
                let Some(Value::Ptr(p)) = args.first() else {
                    return self.refuse("free of a non-pointer", e.span);
                };
                // `free` dereferences the block it releases: a double
                // free, an interior free, or a foreign pointer is L2.
                let valid = match p.alloc {
                    Some(aid) => {
                        let a = &self.allocs[aid];
                        a.live && a.from_malloc && p.offset == 0
                    }
                    None => false,
                };
                if !valid {
                    return self.ub(
                        UbRow::L2,
                        "`c.free` of a pointer that does not address a live C allocation's \
                         base"
                            .to_string(),
                        e.span,
                        p.alloc.map(|a| self.allocs[a].span).unwrap_or(e.span),
                    );
                }
                let aid = p.alloc.expect("checked");
                let a = &mut self.allocs[aid];
                a.live = false;
                a.dead = Some(DeadReason::CFree);
                for t in &mut a.tags {
                    t.state = TagState::Disabled;
                }
                // Quarantined, never reused (D21's checker half).
                Ok(Flow::Val(Value::Unit))
            }
            "c.memset" => {
                let (Some(Value::Ptr(p)), Some(Value::Int(v)), Some(Value::Int(n))) =
                    (args.first(), args.get(1), args.get(2))
                else {
                    return self.refuse("memset outside the model", e.span);
                };
                let data = vec![(*v & 0xff) as u8; (*n).max(0) as usize];
                self.raw_write_bytes(*p, &data, e.span, "`c.memset`")?;
                Ok(Flow::Val(Value::Ptr(*p)))
            }
            "c.memcpy" => {
                let (Some(Value::Ptr(dst)), Some(Value::Ptr(src)), Some(Value::Int(n))) =
                    (args.first(), args.get(1), args.get(2))
                else {
                    return self.refuse("memcpy outside the model", e.span);
                };
                let len = (*n).max(0) as u64;
                let data = self.raw_read_bytes(*src, len, e.span, "`c.memcpy` (source)")?;
                self.raw_write_bytes(*dst, &data, e.span, "`c.memcpy` (destination)")?;
                Ok(Flow::Val(Value::Ptr(*dst)))
            }
            _ => self.refuse("imported C beyond the modelled intrinsic set", e.span),
        }
    }
}

// ------------------------------------------------------------- helpers --

/// Is this call's callee the container-constructor head
/// (`List[int]()` / `Pool[Node]()`)? A plain fn returning a container
/// has a PathExpr callee naming the fn — never a ctor.
fn is_container_ctor(callee: Option<&GreenNode>) -> bool {
    let Some(c) = callee else { return false };
    match c.kind {
        SyntaxKind::BracketApply => wolf_ast::BracketApply::cast(c)
            .and_then(|b| b.callee())
            .is_some_and(|h| h.kind == SyntaxKind::PathExpr),
        _ => false,
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Unit, Value::Unit) => true,
        (
            Value::Handle {
                index: xi,
                generation: xg,
            },
            Value::Handle {
                index: yi,
                generation: yg,
            },
        ) => xi == yi && xg == yg,
        (Value::Ptr(x), Value::Ptr(y)) => x.addr == y.addr,
        (
            Value::Enum {
                variant: xv,
                payload: xp,
            },
            Value::Enum {
                variant: yv,
                payload: yp,
            },
        ) => xv == yv && xp.len() == yp.len() && xp.iter().zip(yp).all(|(m, n)| values_equal(m, n)),
        (
            Value::ErrTag {
                tag: xt,
                payload: xp,
            },
            Value::ErrTag {
                tag: yt,
                payload: yp,
            },
        ) => xt == yt && xp.len() == yp.len() && xp.iter().zip(yp).all(|(m, n)| values_equal(m, n)),
        _ => false,
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.clone(),
        Value::Unit => "()".to_string(),
        Value::Range { start, end } => format!("{start}..{end}"),
        Value::ErrTag { tag, .. } => format!("{{{tag}}}"),
        _ => "<value>".to_string(),
    }
}

fn parse_int_literal(text: &str) -> Option<i64> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok();
    }
    if let Some(bin) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return i64::from_str_radix(bin, 2).ok();
    }
    if let Some(oct) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return i64::from_str_radix(oct, 8).ok();
    }
    t.parse().ok()
}

fn prim_bits(p: Prim) -> Option<u32> {
    Some(match p {
        Prim::I8 | Prim::U8 | Prim::Byte => 8,
        Prim::I16 | Prim::U16 => 16,
        Prim::I32 | Prim::U32 | Prim::F32 => 32,
        Prim::I64 | Prim::U64 | Prim::Int | Prim::Uint | Prim::F64 => 64,
        Prim::Bool | Prim::Str => return None,
    })
}

fn prim_size(p: Prim) -> u64 {
    match prim_bits(p) {
        Some(b) => (b / 8) as u64,
        None => 1,
    }
}

fn prim_range(p: Prim) -> Option<(i64, i64)> {
    Some(match p {
        Prim::I8 => (i64::from(i8::MIN), i64::from(i8::MAX)),
        Prim::I16 => (i64::from(i16::MIN), i64::from(i16::MAX)),
        Prim::I32 => (i64::from(i32::MIN), i64::from(i32::MAX)),
        Prim::U8 | Prim::Byte => (0, 0xff),
        Prim::U16 => (0, 0xffff),
        Prim::U32 => (0, 0xffff_ffff),
        // 64-bit prims ride i64's own checked arithmetic; uint's
        // upper half is out of this machine's honest range.
        Prim::I64 | Prim::Int => return None,
        Prim::U64 | Prim::Uint => (0, i64::MAX),
        Prim::Bool | Prim::Str | Prim::F32 | Prim::F64 => return None,
    })
}

fn collect_binding_spans(pat: &GreenNode, out: &mut Vec<Span>) {
    if matches!(pat.kind, SyntaxKind::IdentPat | SyntaxKind::BindingPat)
        && let Some(t) = pat.tokens().find(|t| t.kind == SyntaxKind::Ident)
    {
        out.push(t.span);
    }
    for child in pat.nodes().filter(|n| is_pattern_kind(n.kind)) {
        collect_binding_spans(child, out);
    }
}
