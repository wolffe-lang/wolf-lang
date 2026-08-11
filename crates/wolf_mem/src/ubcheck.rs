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
    /// The `eprint` channel (s38). The differential record never
    /// hashes stderr — it is the rich human channel — but tests
    /// assert it and the driver forwards it.
    pub stderr: String,
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
    /// The executable float (s38): `f64` values under IEEE semantics —
    /// arithmetic never traps (X3 is integer law; inf/nan are values).
    /// `f32` stays an honest refusal until a use case rules its
    /// rounding story.
    F64(f64),
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
                | Value::F64(_)
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
/// One open socket in the checked machine's table (s39 net tier).
#[derive(Debug)]
enum NetSock {
    Listener(std::net::TcpListener),
    Stream(std::net::TcpStream),
}

/// The s39 net row-tag mapping: `io::ErrorKind` → net row tag. Mirrors
/// `wolf_rt::net::err_tag` by hand — wolf_mem and wolf_rt may not see
/// each other (the locked graph; D15) — and the driver's `net_parity`
/// test pins the two, exactly the fmt-shim precedent. Public for that
/// test alone.
pub fn net_err_tag(kind: std::io::ErrorKind) -> &'static str {
    use std::io::ErrorKind as K;
    match kind {
        K::ConnectionRefused => "refused",
        K::TimedOut | K::WouldBlock => "timeout",
        K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe | K::NotConnected => "closed",
        _ => "io",
    }
}

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
    /// Defining name-token span -> body index, top-level fns. The
    /// span is the same one the checker records as `CallSig::
    /// decl_span`, so a resolved call site names its body exactly —
    /// never "whichever same-named fn a hash order surfaces first"
    /// (the F-0048 verdict flake).
    fns_by_decl: HashMap<Span, usize>,
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
    /// What the program wrote through `eprint`/`eprint_raw` (s38).
    stderr: String,
    /// The program's standard input (s38): a caller-supplied buffer,
    /// consumed by `read_line`. Conform-run supplies none — the
    /// checked lane's default stdin is empty, so `read_line` raises
    /// `eof` deterministically.
    stdin: String,
    stdin_pos: usize,
    /// Open file handles (s38 fs tier): index = the `int` fd wolf code
    /// holds; `None` after close.
    files: Vec<Option<std::fs::File>>,
    /// Open sockets (s39 net tier): a separate handle namespace from
    /// `files`, same discipline — index = the `int` fd, `None` after
    /// close, forged/foreign fds are the `io` row.
    socks: Vec<Option<NetSock>>,
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
    run_checked_with_input(pkg, tc, budget, "")
}

/// [`run_checked`] with a standard-input buffer for `read_line` (s38).
/// Conform-run has no stdin channel, so the default is empty (every
/// `read_line` raises `eof`); tests feed programs here.
pub fn run_checked_with_input(
    pkg: &Package,
    tc: &Typecheck,
    budget: Budget,
    stdin: &str,
) -> Result<RunOutcome, NotYet> {
    run_checked_fn(pkg, tc, budget, stdin, "main")
}

/// The `wolf test` discovery seam (s39): the entry file's top-level
/// `test_*` functions in declaration order, each with its parameter
/// count (the runner executes zero-parameter tests and reports
/// parameter-carrying ones as unsupported — never silently skipped).
pub fn discover_tests(pkg: &Package) -> Vec<(String, usize)> {
    let file = &pkg.files[0];
    let src = &file.raw.src;
    let mut out = Vec::new();
    for node in file.parse.root.nodes().filter(|n| n.kind.is_item()) {
        let Some(d) = wolf_ast::FnDecl::cast(node) else {
            continue;
        };
        let Some(tok) = d.name() else { continue };
        let name =
            String::from_utf8_lossy(&src[tok.span.lo as usize..tok.span.hi as usize]).into_owned();
        if !name.starts_with("test_") {
            continue;
        }
        let arity = d.params().map(|p| p.params().count()).unwrap_or(0);
        out.push((name, arity));
    }
    out
}

/// Does the package declare a top-level `main`? (`wolf test` uses this
/// to run a `_test.lu` file black-box when it carries no `test_*`
/// functions.)
pub fn has_main(pkg: &Package) -> bool {
    for file in &pkg.files {
        let src = &file.raw.src;
        for node in file.parse.root.nodes().filter(|n| n.kind.is_item()) {
            let Some(d) = wolf_ast::FnDecl::cast(node) else {
                continue;
            };
            let Some(tok) = d.name() else { continue };
            if &src[tok.span.lo as usize..tok.span.hi as usize] == b"main" {
                return true;
            }
        }
    }
    false
}

/// Execute one named zero-parameter entry function under the UB
/// machine — the `wolf test` execution seam (s39). `entry == "main"`
/// is exactly [`run_checked_with_input`]; any other name must be a
/// checked, zero-parameter, top-level fn (entry file preferred) or the
/// run refuses honestly.
pub fn run_checked_fn(
    pkg: &Package,
    tc: &Typecheck,
    budget: Budget,
    stdin: &str,
    entry: &str,
) -> Result<RunOutcome, NotYet> {
    let root_span = pkg.files[0].parse.root.span;
    let mut m = Machine::new(pkg, tc);
    m.budget = budget;
    m.stdin = stdin.to_string();
    let main = match m.find_entry(entry) {
        Some(b) => b,
        None => {
            return Err(NotYet {
                construct: if entry == "main" {
                    "checked execution without a `main` entry"
                } else {
                    "checked execution without the requested entry fn"
                },
                span: root_span,
            });
        }
    };
    // A parameter-carrying entry has no argument source; refuse rather
    // than invent values.
    if let Some(ctx) = m.ctxs[main].as_ref()
        && let Some(d) = wolf_ast::FnDecl::cast(ctx.node)
        && d.params().map(|p| p.params().count()).unwrap_or(0) != 0
        && entry != "main"
    {
        return Err(NotYet {
            construct: "a test entry with parameters (s39 runs zero-parameter tests)",
            span: ctx.node.span,
        });
    }
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
                stderr: m.stderr,
            })
        }
        Err(Stop::Trap(t)) => Ok(RunOutcome {
            verdict: Verdict::Trap(t),
            stdout: m.stdout,
            stderr: m.stderr,
        }),
        Err(Stop::Ub(f)) => Ok(RunOutcome {
            verdict: Verdict::Ub(f),
            stdout: m.stdout,
            stderr: m.stderr,
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
            fns_by_decl: HashMap::new(),
            methods: HashMap::new(),
            allocs: Vec::new(),
            regions: Vec::new(),
            lists: Vec::new(),
            pools: Vec::new(),
            cells: Vec::new(),
            frames: Vec::new(),
            ambient: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            stdin: String::new(),
            stdin_pos: 0,
            files: Vec::new(),
            socks: Vec::new(),
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
                        if let Some(name) = wolf_ast::FnDecl::cast(node).and_then(|d| d.name()) {
                            m.fns_by_decl.insert(name.span, i);
                        }
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

    /// Find a top-level entry fn by name, preferring the entry file's
    /// (file 0), else any. `"main"` is the classic caller; `wolf test`
    /// asks for `test_*` names (s39).
    fn find_entry(&self, entry: &str) -> Option<usize> {
        // Deterministic across runs (F-0048): the entry file's match
        // wins; otherwise the smallest body index does — never
        // whichever match a hash order happens to visit last.
        let mut best: Option<usize> = None;
        for ((_, name), &idx) in &self.fns {
            if name == entry {
                let file = self.tc.bodies[idx].body.file;
                if file == 0 {
                    return Some(idx);
                }
                best = Some(best.map_or(idx, |b| b.min(idx)));
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
            // A raise leaving the body (s15/s37): errdefers run, the
            // tag value crosses to the caller — the call site rewraps
            // it as `Flow::Err` (or `main` reports it, D30's process
            // behavior).
            Ok(Flow::Err(v)) => self.exit_scopes_to(0, true).map(|()| v),
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
            // s37: `s.len` through a place — bytes, O(1) (D24/D25).
            (PStep::Field(f), Value::Str(s)) if f == "len" => {
                let n = s.len() as i64;
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
                // A raise site (s15/s37): the checker injected a row
                // tag here — the recorded type is the `!T` union and
                // the bare name is one of its declared tags. The
                // declared row wins over the value namespace
                // (wolf-lang#30), so this fires before the module-item
                // fallback.
                if e.kind == SyntaxKind::PathExpr
                    && let Some(tag) = self.raised_tag(e)
                {
                    return Ok(Flow::Err(Value::ErrTag {
                        tag,
                        payload: Vec::new(),
                    }));
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
                // s37 — `s[a..b]` byte-offset checked slicing (D25).
                if let Some(recv) = b.callee()
                    && matches!(self.expr_ty(recv.span), Some(TyKind::Prim(Prim::Str)))
                {
                    return self.eval_str_slice(e);
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
                    // IEEE negation flips the sign bit; `-0.0` exists.
                    Value::F64(x) => Ok(Flow::Val(Value::F64(-x))),
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
            SyntaxKind::Lt | SyntaxKind::Gt | SyntaxKind::LtEq | SyntaxKind::GtEq => match (l, r) {
                (Value::Int(a), Value::Int(b)) => {
                    let out = match op {
                        SyntaxKind::Lt => a < b,
                        SyntaxKind::Gt => a > b,
                        SyntaxKind::LtEq => a <= b,
                        _ => a >= b,
                    };
                    Ok(Flow::Val(Value::Bool(out)))
                }
                // IEEE partial order (s38): any comparison against
                // nan is false.
                (Value::F64(a), Value::F64(b)) => {
                    let out = match op {
                        SyntaxKind::Lt => a < b,
                        SyntaxKind::Gt => a > b,
                        SyntaxKind::LtEq => a <= b,
                        _ => a >= b,
                    };
                    Ok(Flow::Val(Value::Bool(out)))
                }
                // `[mem.str.order]` (s37): byte-lexicographic over the
                // UTF-8 bytes, unsigned compare, shorter-first on a
                // shared prefix — exactly Rust's `str` ordering.
                (Value::Str(a), Value::Str(b)) => {
                    let out = match op {
                        SyntaxKind::Lt => a < b,
                        SyntaxKind::Gt => a > b,
                        SyntaxKind::LtEq => a <= b,
                        _ => a >= b,
                    };
                    Ok(Flow::Val(Value::Bool(out)))
                }
                _ => self.refuse("ordering outside integers and str", e.span),
            },
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
            // Floats are IEEE (s38): arithmetic never traps — inf and
            // nan are VALUES; X3's trap law is integer law. `%` on
            // floats has no ruled semantics (fmod vs IEEE remainder)
            // and refuses honestly.
            (Value::F64(a), Value::F64(b)) => {
                let out = match op {
                    SyntaxKind::Plus => a + b,
                    SyntaxKind::Minus => a - b,
                    SyntaxKind::Star => a * b,
                    SyntaxKind::Slash => a / b,
                    SyntaxKind::Percent => {
                        return self.refuse("`%` on floats (unruled: fmod vs remainder)", span);
                    }
                    _ => a,
                };
                Ok(Value::F64(out))
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
        // Type-driven float literals (s38): a literal the checker
        // typed `f64` is a float even when its spelling is integral
        // (`let x: f64 = 3`). `f32` refuses until its rounding story
        // is ruled.
        match self.expr_ty(e.span) {
            Some(TyKind::Prim(Prim::F64)) => {
                let t: String = text.chars().filter(|&c| c != '_').collect();
                return match t.parse::<f64>() {
                    Ok(x) => Ok(Value::F64(x)),
                    Err(_) => self.refuse("this float literal shape", e.span),
                };
            }
            Some(TyKind::Prim(Prim::F32)) => {
                return self.refuse("`f32` in checked execution (f64 is the s38 float)", e.span);
            }
            _ => {}
        }
        match parse_int_literal(&text) {
            Some(n) => Ok(Value::Int(n)),
            None => self.refuse("this literal shape in checked execution", e.span),
        }
    }

    fn eval_string(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = StringExpr::cast(e).expect("kind");
        // Rebuild: literal segments from source, values spliced at
        // interpolation holes ({x} f-strings, D26). Format specs
        // (`{x:>8}`) apply per s38's amendment candidate (#28): the
        // implemented subset is `[[fill]align][width]` — width in
        // BYTES (D25), default alignment left for `str`/`bool` and
        // right for numbers; everything beyond it refuses honestly
        // (the wolf-lang#10 rule: a spec is never silently ignored).
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
                    let rendered = format_value(&hv);
                    match i.format_spec() {
                        Some(spec) => self.apply_format_spec(spec, &hv, rendered)?,
                        None => rendered,
                    }
                }
                None => String::new(),
            };
            holes.push((ispan.lo - base, ispan.hi - base, v));
        }
        let bytes = raw.as_bytes();
        // `"""` multiline strings dedent by the closing delimiter's
        // column (D26). Holes inside one shift every offset after
        // dedent, so that combination refuses honestly for now
        // (printing undedented text was a silent wrong answer).
        if bytes.starts_with(b"\"\"\"") {
            if !holes.is_empty() {
                return Err(Stop::Refuse(NotYet {
                    construct: "interpolation inside a multiline string (s38 formatting)",
                    span: e.span,
                }));
            }
            let inner = &bytes[3..bytes.len().saturating_sub(3).max(3)];
            let dedented = dedent_multiline(inner);
            let decoded = decode_escapes(&dedented);
            let out = String::from_utf8_lossy(&decoded).into_owned();
            return Ok(Flow::Val(Value::Str(out)));
        }
        // Byte-accurate rebuild: literal segments are copied as UTF-8
        // *bytes* (a per-byte `as char` push double-encoded every
        // non-ASCII literal — the c06 latin-1 divergence, retired
        // here), escapes push their single byte, holes splice their
        // rendered text.
        let mut outb: Vec<u8> = Vec::new();
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
                outb.extend_from_slice(v.as_bytes());
                i = *hi as usize;
                continue;
            }
            let c = bytes[i];
            if c == b'\\' && i + 1 < end {
                // Code-point escapes: `\xNN` and `\u{…}` (s37 — the
                // wolf-std whitespace-set pin exercises both).
                if let Some((ch, consumed)) = decode_codepoint_escape(&bytes[i..end]) {
                    let mut buf = [0u8; 4];
                    outb.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    i += consumed;
                    continue;
                }
                let esc = bytes[i + 1];
                outb.push(match esc {
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
                outb.push(c);
                i += 2;
                continue;
            }
            outb.push(c);
            i += 1;
        }
        let out = String::from_utf8_lossy(&outb).into_owned();
        Ok(Flow::Val(Value::Str(out)))
    }

    /// The checked tier's format-spec application (s38): the full
    /// §7.4 candidate grammar — `[[fill]align][+][0][width]
    /// [.precision][type]` — through `wolf_sema::fmtspec`, the single
    /// reference implementation (semantics member-by-member the
    /// wolf-std sc05 functions; the native shims mirror it and the
    /// driver's parity test pins the two). Malformed and mismatched
    /// specs never reach here: sema diagnoses them (E0412/E0413) and
    /// the ladder stops. What still refuses, honestly: a computed
    /// spec (`{x:{w}}`, no pinned semantics — a #28 question), and a
    /// spec whose hole type the checker could not classify.
    fn apply_format_spec(
        &mut self,
        spec: &'t GreenNode,
        val: &Value,
        rendered: String,
    ) -> Result<String, Stop> {
        use wolf_sema::fmtspec::{self, FmtValue};
        // A computed spec (`{x:{w}}`) has no pinned semantics.
        if spec.nodes().any(|n| n.kind == SyntaxKind::Interp) {
            return Err(Stop::Refuse(NotYet {
                construct: "a computed format spec (s38 formatting)",
                span: spec.span,
            }));
        }
        let text = self.text(spec.span);
        let s = text.strip_prefix(':').unwrap_or(&text);
        let Ok(parsed) = fmtspec::parse(s) else {
            // Sema diagnosed E0412; execution never starts. Defensive
            // honesty if a path ever slips.
            return Err(Stop::Refuse(NotYet {
                construct: "a format spec outside the ruled grammar (E0412)",
                span: spec.span,
            }));
        };
        if parsed.is_default() {
            return Ok(rendered);
        }
        let fv = match val {
            Value::Str(s) => FmtValue::Str(s),
            Value::Bool(b) => FmtValue::Bool(*b),
            // The checked machine models every integer as its value
            // in `i64` (narrow prims range-trap on arithmetic), so
            // rendering is signed here; the native lane's unsigned
            // flag matters only beyond `i64::MAX`, which no checked
            // value reaches.
            Value::Int(n) => FmtValue::Int {
                v: *n,
                unsigned: false,
            },
            Value::F64(x) => FmtValue::F64(*x),
            _ => {
                return Err(Stop::Refuse(NotYet {
                    construct: "a format spec on a non-primitive value",
                    span: spec.span,
                }));
            }
        };
        match fmtspec::apply(&parsed, fv) {
            Ok(out) => Ok(out),
            // A mismatch the checker skipped (unresolved hole class).
            Err(_) => Err(Stop::Refuse(NotYet {
                construct: "a format spec the checker did not rule on for this value",
                span: spec.span,
            })),
        }
    }

    /// The s38 io/fs builtin tier (checked lane): real host
    /// operations with D30 row errors — `not_found`/`denied`/`io` per
    /// `io::ErrorKind`, `utf8` on text-decode failure, `eof` where an
    /// end is an outcome. An error outside a builtin's declared row
    /// coarsens to `io` (rule 3 of the wolf-std taxonomy: one tag per
    /// actionable response, never per internal cause). File handles
    /// are plain `int` fds into the machine's table; every operation
    /// on a closed or foreign fd is the `io` row, never a trap — a
    /// forged fd is a checkable condition, not a contract violation.
    fn io_fs_builtin(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        use std::io::{Read as _, Write as _};
        fn tag(t: &str) -> Flow {
            Flow::Err(Value::ErrTag {
                tag: t.to_string(),
                payload: Vec::new(),
            })
        }
        fn errtag(e: &std::io::Error, declared: &[&str]) -> String {
            let t = match e.kind() {
                std::io::ErrorKind::NotFound => "not_found",
                std::io::ErrorKind::PermissionDenied => "denied",
                _ => "io",
            };
            if declared.contains(&t) { t } else { "io" }.to_string()
        }
        let str_arg = |i: usize| -> Option<String> {
            match argv.get(i) {
                Some(Value::Str(s)) => Some(s.clone()),
                _ => None,
            }
        };
        let int_arg = |i: usize| -> Option<i64> {
            match argv.get(i) {
                Some(Value::Int(n)) => Some(*n),
                _ => None,
            }
        };
        match name {
            "read_line" => {
                if self.stdin_pos >= self.stdin.len() {
                    return Ok(tag("eof"));
                }
                let rest = &self.stdin[self.stdin_pos..];
                let (line, consumed) = match rest.find('\n') {
                    Some(i) => (&rest[..i], i + 1),
                    None => (rest, rest.len()),
                };
                let line = line.strip_suffix('\r').unwrap_or(line).to_string();
                self.stdin_pos += consumed;
                self.charge_mem(line.len() as u64)?;
                Ok(Flow::Val(Value::Str(line)))
            }
            "fs_read_text" => {
                let Some(path) = str_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                match std::fs::read(&path) {
                    Err(e) => Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(bytes) => {
                        self.charge_mem(bytes.len() as u64)?;
                        match String::from_utf8(bytes) {
                            Ok(s) => Ok(Flow::Val(Value::Str(s))),
                            Err(_) => Ok(tag("utf8")),
                        }
                    }
                }
            }
            "fs_write_text" => {
                let (Some(path), Some(contents)) = (str_arg(0), str_arg(1)) else {
                    return self.refuse("this fs call shape", span);
                };
                match std::fs::write(&path, contents.as_bytes()) {
                    Err(e) => Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            "fs_open" | "fs_create" => {
                let Some(path) = str_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                let opened = if name == "fs_open" {
                    std::fs::OpenOptions::new().read(true).open(&path)
                } else {
                    std::fs::File::create(&path)
                };
                let declared: &[&str] = if name == "fs_open" {
                    &["not_found", "denied", "io"]
                } else {
                    &["denied", "io"]
                };
                match opened {
                    Err(e) => Ok(tag(&errtag(&e, declared))),
                    Ok(f) => {
                        let fd = self.files.len() as i64;
                        self.files.push(Some(f));
                        Ok(Flow::Val(Value::Int(fd)))
                    }
                }
            }
            "fs_read" => {
                let (Some(fd), Some(max)) = (int_arg(0), int_arg(1)) else {
                    return self.refuse("this fs call shape", span);
                };
                let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| self.files.get_mut(i))
                else {
                    return Ok(tag("io"));
                };
                if max <= 0 {
                    return Ok(Flow::Val(Value::Str(String::new())));
                }
                let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
                match f.read(&mut buf) {
                    Err(e) => Ok(tag(&errtag(&e, &["io"]))),
                    Ok(0) => Ok(tag("eof")),
                    Ok(n) => {
                        buf.truncate(n);
                        self.charge_mem(n as u64)?;
                        match String::from_utf8(buf) {
                            Ok(s) => Ok(Flow::Val(Value::Str(s))),
                            Err(_) => Ok(tag("utf8")),
                        }
                    }
                }
            }
            "fs_write" => {
                let (Some(fd), Some(s)) = (int_arg(0), str_arg(1)) else {
                    return self.refuse("this fs call shape", span);
                };
                let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| self.files.get_mut(i))
                else {
                    return Ok(tag("io"));
                };
                match f.write_all(s.as_bytes()) {
                    Err(e) => Ok(tag(&errtag(&e, &["io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            "fs_close" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                match usize::try_from(fd).ok().and_then(|i| self.files.get_mut(i)) {
                    Some(slot @ Some(_)) => {
                        *slot = None; // drop closes; double close is `io`
                        Ok(Flow::Val(Value::Unit))
                    }
                    _ => Ok(tag("io")),
                }
            }
            "fs_remove" => {
                let Some(path) = str_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                match std::fs::remove_file(&path) {
                    Err(e) => Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            "fs_exists" => {
                let Some(path) = str_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                Ok(Flow::Val(Value::Bool(std::path::Path::new(&path).exists())))
            }
            _ => self.refuse("this io/fs builtin", span),
        }
    }

    /// The s39 net builtin tier (checked lane): REAL blocking TCP over
    /// the host's loopback-capable stack, D30 rows only — `refused`,
    /// `timeout`, `closed` (the peer's finish: the socket `eof`),
    /// `utf8` on text decode, everything else coarsened to `io`. A
    /// forged, foreign, or wrong-kind fd is `io`, never a trap. v0 is
    /// blocking-syscall-shaped: the interpreter thread blocks in the
    /// kernel, so no schedule point exists here (spec/07 untouched);
    /// the s35 reactor owns the async story and appends its own
    /// completion-arrival kind when it lands.
    fn net_builtin(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        use std::io::{Read as _, Write as _};
        fn tag(t: &str) -> Flow {
            Flow::Err(Value::ErrTag {
                tag: t.to_string(),
                payload: Vec::new(),
            })
        }
        fn coarse(kind: std::io::ErrorKind, declared: &[&str]) -> String {
            let t = net_err_tag(kind);
            if declared.contains(&t) { t } else { "io" }.to_string()
        }
        let str_arg = |i: usize| -> Option<String> {
            match argv.get(i) {
                Some(Value::Str(s)) => Some(s.clone()),
                _ => None,
            }
        };
        let int_arg = |i: usize| -> Option<i64> {
            match argv.get(i) {
                Some(Value::Int(n)) => Some(*n),
                _ => None,
            }
        };
        match name {
            "net_listen" => {
                let Some(addr) = str_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                match std::net::TcpListener::bind(&addr) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["io"]))),
                    Ok(l) => {
                        let fd = self.socks.len() as i64;
                        self.socks.push(Some(NetSock::Listener(l)));
                        Ok(Flow::Val(Value::Int(fd)))
                    }
                }
            }
            "net_port" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                let addr = match self.sock(fd) {
                    Some(NetSock::Listener(l)) => l.local_addr(),
                    Some(NetSock::Stream(s)) => s.local_addr(),
                    None => return Ok(tag("io")),
                };
                match addr {
                    Ok(a) => Ok(Flow::Val(Value::Int(i64::from(a.port())))),
                    Err(e) => Ok(tag(&coarse(e.kind(), &["io"]))),
                }
            }
            "net_accept" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                let accepted = match self.sock(fd) {
                    Some(NetSock::Listener(l)) => l.accept(),
                    _ => return Ok(tag("io")),
                };
                match accepted {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["timeout", "io"]))),
                    Ok((s, _peer)) => {
                        let fd = self.socks.len() as i64;
                        self.socks.push(Some(NetSock::Stream(s)));
                        Ok(Flow::Val(Value::Int(fd)))
                    }
                }
            }
            "net_connect" => {
                let Some(addr) = str_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                match std::net::TcpStream::connect(&addr) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["refused", "timeout", "io"]))),
                    Ok(s) => {
                        let fd = self.socks.len() as i64;
                        self.socks.push(Some(NetSock::Stream(s)));
                        Ok(Flow::Val(Value::Int(fd)))
                    }
                }
            }
            "net_read" => {
                let (Some(fd), Some(max)) = (int_arg(0), int_arg(1)) else {
                    return self.refuse("this net call shape", span);
                };
                let Some(NetSock::Stream(s)) = self.sock(fd) else {
                    return Ok(tag("io"));
                };
                if max <= 0 {
                    return Ok(Flow::Val(Value::Str(String::new())));
                }
                let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
                match s.read(&mut buf) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["closed", "timeout", "io"]))),
                    Ok(0) => Ok(tag("closed")),
                    Ok(n) => {
                        buf.truncate(n);
                        self.charge_mem(n as u64)?;
                        match String::from_utf8(buf) {
                            Ok(s) => Ok(Flow::Val(Value::Str(s))),
                            Err(_) => Ok(tag("utf8")),
                        }
                    }
                }
            }
            "net_write" => {
                let (Some(fd), Some(payload)) = (int_arg(0), str_arg(1)) else {
                    return self.refuse("this net call shape", span);
                };
                let Some(NetSock::Stream(s)) = self.sock(fd) else {
                    return Ok(tag("io"));
                };
                match s.write_all(payload.as_bytes()) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["closed", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            "net_close" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                match usize::try_from(fd).ok().and_then(|i| self.socks.get_mut(i)) {
                    Some(slot @ Some(_)) => {
                        *slot = None; // drop closes; double close is `io`
                        Ok(Flow::Val(Value::Unit))
                    }
                    _ => Ok(tag("io")),
                }
            }
            _ => self.refuse("this net builtin", span),
        }
    }

    fn sock(&mut self, fd: i64) -> Option<&mut NetSock> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.socks.get_mut(i))
            .and_then(Option::as_mut)
    }

    /// Is this expression an injected raise of a declared row tag?
    /// True when the checker recorded the `!T` union as the node's
    /// type and the node's own text names one of the row's tags —
    /// exactly the shape `inject_tag` leaves behind.
    fn raised_tag(&self, e: &'t GreenNode) -> Option<String> {
        let Some(TyKind::ErrUnion(_, row)) = self.expr_ty(e.span) else {
            return None;
        };
        let name = self.text(e.span);
        let TyKind::Row { tags, .. } = self.ctx().tb.table.kind(*row) else {
            return None;
        };
        tags.iter().any(|(n, _)| n == &name).then_some(name)
    }

    /// The call-shaped raise: the whole call's recorded type is the
    /// `!T` union and the callee text names a declared tag.
    fn raised_tag_call(&self, e: &'t GreenNode, callee: &'t GreenNode) -> Option<String> {
        let Some(TyKind::ErrUnion(_, row)) = self.expr_ty(e.span) else {
            return None;
        };
        let name = self.text(callee.span);
        let TyKind::Row { tags, .. } = self.ctx().tb.table.kind(*row) else {
            return None;
        };
        tags.iter().any(|(n, _)| n == &name).then_some(name)
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
                    // s37: `s.len` — bytes, O(1) (D24/D25).
                    Value::Str(s) if field == "len" => Ok(Flow::Val(Value::Int(s.len() as i64))),
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

    // ---------------------------------------------------- str tier --

    /// `s[a..b]` — the D25 checked slice: byte offsets, `^n` from the
    /// end, open ends default to the string's edges. OOB and
    /// split-code-point offsets are *defined checks*: a deterministic
    /// `bounds` trap, never UB and never a garbled slice. The
    /// recoverable twin is `s.get(a..b) -> str ! {none}` (a method,
    /// below).
    fn eval_str_slice(&mut self, e: &'t GreenNode) -> E<Flow> {
        let b = BracketApply::cast(e).expect("kind");
        let Some(recv) = b.callee() else {
            return self.refuse("a slice without a receiver", e.span);
        };
        let sv = if let Some(place) = self.place_of(recv)? {
            self.read_place(&place, recv.span)?
        } else {
            val!(self.eval(recv))
        };
        let Value::Str(s) = sv else {
            return self.refuse("str slicing of a non-str", e.span);
        };
        let mut range_node = None;
        for a in b.args().into_iter().flat_map(|l| l.args()) {
            if let Some(v) = Arg::value(a)
                && v.kind == SyntaxKind::RangeExpr
            {
                range_node = Some(v);
            }
        }
        let Some(rn) = range_node else {
            return self.refuse("this str index shape in checked execution", e.span);
        };
        let d = RangeExpr::cast(rn).expect("kind");
        let len = s.len() as i64;
        // Which side of the dots each endpoint sits on decides which
        // bound it names — open sides default to the edges.
        let dots = rn
            .tokens()
            .find(|t| matches!(t.kind, SyntaxKind::DotDot | SyntaxKind::DotDotEq))
            .map(|t| t.span.lo)
            .unwrap_or(rn.span.hi);
        let mut lo = 0i64;
        let mut hi = len;
        for ep in d.endpoints() {
            let resolved = if ep.kind == SyntaxKind::FromEndExpr {
                let inner = wolf_ast::FromEndExpr::cast(ep).and_then(|f| f.expr());
                let Some(inner) = inner else {
                    return self.refuse("a bare `^` endpoint", ep.span);
                };
                let Value::Int(n) = val!(self.eval(inner)) else {
                    return self.refuse("a non-integer `^n` endpoint", ep.span);
                };
                len - n
            } else {
                let Value::Int(n) = val!(self.eval(ep)) else {
                    return self.refuse("a non-integer slice endpoint", ep.span);
                };
                n
            };
            if ep.span.lo < dots {
                lo = resolved;
            } else {
                hi = resolved;
            }
        }
        if d.is_inclusive() {
            hi += 1;
        }
        if lo < 0 || hi < lo || hi > len {
            return self.trap("bounds", "mem.ub.defined", e.span);
        }
        let (a, z) = (lo as usize, hi as usize);
        if !s.is_char_boundary(a) || !s.is_char_boundary(z) {
            return self.trap("bounds", "mem.ub.defined", e.span);
        }
        Ok(Flow::Val(Value::Str(s[a..z].to_string())))
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
            "print" | "print_raw" | "eprint" | "eprint_raw" => {
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
                if callee_name.ends_with("print") {
                    out.push('\n');
                }
                if callee_name.starts_with('e') {
                    self.stderr.push_str(&out);
                } else {
                    self.stdout.push_str(&out);
                }
                return Ok(Flow::Val(Value::Unit));
            }
            // The s38 io/fs builtin tier. Errors are D30 payload rows,
            // never traps: the machine performs the REAL host
            // operation (checked execution is a host process; the
            // comptime sandbox is the one place these are refused) and
            // maps `io::ErrorKind` onto each builtin's declared tags.
            "read_line" | "fs_read_text" | "fs_write_text" | "fs_open" | "fs_create"
            | "fs_read" | "fs_write" | "fs_close" | "fs_remove" | "fs_exists" => {
                let mut argv = Vec::new();
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        let x = if let Some(place) = self.place_of(v)? {
                            self.read_place(&place, v.span)?
                        } else {
                            val!(self.eval(v))
                        };
                        argv.push(x);
                    }
                }
                return self.io_fs_builtin(&callee_name, argv, e.span);
            }
            // The s39 net builtin tier: same posture as fs — real host
            // operations, D30 rows, comptime is the one refusal site.
            "net_listen" | "net_port" | "net_accept" | "net_connect" | "net_read" | "net_write"
            | "net_close" => {
                let mut argv = Vec::new();
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        let x = if let Some(place) = self.place_of(v)? {
                            self.read_place(&place, v.span)?
                        } else {
                            val!(self.eval(v))
                        };
                        argv.push(x);
                    }
                }
                return self.net_builtin(&callee_name, argv, e.span);
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
        // A payload-carrying raise (`return tag(x)`, s15/s37): no
        // CallSig — the checker injected the tag; the recorded type
        // is the `!T` union and the callee names a declared tag.
        if cs.is_none()
            && let Some(callee) = d.callee()
            && callee.kind == SyntaxKind::PathExpr
            && let Some(tag) = self.raised_tag_call(e, callee)
        {
            let mut payload = Vec::new();
            for a in d.args().into_iter().flat_map(|l| l.args()) {
                if let Some(v) = Arg::value(a) {
                    payload.push(val!(self.eval(v)));
                }
            }
            return Ok(Flow::Err(Value::ErrTag { tag, payload }));
        }
        // A plain user fn call.
        let Some(sig) = cs else {
            return self.refuse("calls outside the modelled surface", e.span);
        };
        let module = self.tc.bodies[self.frames.last().expect("frame").body]
            .body
            .module;
        // Resolution order (F-0048): the checker's declaration locus
        // names the body exactly (cross-module calls included), the
        // caller's own module answers same-module calls, and the
        // name-only net picks the SMALLEST body index — a stable,
        // deterministic choice, never a hash order's.
        let Some(&body) = sig
            .decl_span
            .and_then(|ds| self.fns_by_decl.get(&ds))
            .or_else(|| self.fns.get(&(module, sig.callee.clone())))
            .or_else(|| {
                self.fns
                    .iter()
                    .filter(|((_, n), _)| *n == sig.callee)
                    .map(|(_, b)| b)
                    .min()
            })
        else {
            return self.refuse("calls into unresolvable bodies", e.span);
        };
        let mut args = Vec::new();
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let Some(v) = Arg::value(a) else { continue };
            let mode = sig.params.get(i).and_then(|p| p.mode);
            args.push(self.eval_arg(v, mode)?);
        }
        let out = self.call_body(body, args)?;
        // A raised row tag crosses the call as the error flow — the
        // caller's `?`/`else`/`match` observes it (D30).
        if let Value::ErrTag { .. } = out {
            return Ok(Flow::Err(out));
        }
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
                    "len" | "count" => Ok(Flow::Val(Value::Int(self.lists[id].len() as i64))),
                    "is_empty" => Ok(Flow::Val(Value::Bool(self.lists[id].is_empty()))),
                    // The recoverable reads (s37): misses are `{none}`
                    // rows — absence is a row, not a trap.
                    "pop" => match self.lists[id].pop() {
                        Some(v) => Ok(Flow::Val(v)),
                        None => Ok(Flow::Err(Value::ErrTag {
                            tag: "none".to_string(),
                            payload: Vec::new(),
                        })),
                    },
                    "get" => {
                        let idx = args.into_iter().flat_map(|l| l.args()).find_map(Arg::value);
                        let Some(v) = idx else {
                            return self.refuse("List.get without an index", e.span);
                        };
                        let Value::Int(i) = val!(self.eval(v)) else {
                            return self.refuse("List.get with a non-int index", e.span);
                        };
                        if i < 0 || i as usize >= self.lists[id].len() {
                            return Ok(Flow::Err(Value::ErrTag {
                                tag: "none".to_string(),
                                payload: Vec::new(),
                            }));
                        }
                        Ok(Flow::Val(self.lists[id][i as usize].clone()))
                    }
                    "first" | "last" => {
                        let v = if method == "first" {
                            self.lists[id].first().cloned()
                        } else {
                            self.lists[id].last().cloned()
                        };
                        match v {
                            Some(v) => Ok(Flow::Val(v)),
                            None => Ok(Flow::Err(Value::ErrTag {
                                tag: "none".to_string(),
                                payload: Vec::new(),
                            })),
                        }
                    }
                    "clear" => {
                        self.lists[id].clear();
                        Ok(Flow::Val(Value::Unit))
                    }
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
                    // s37 — Pool observability (wolf-lang#11's gap):
                    // live-slot count and the non-trapping probe.
                    "len" | "is_empty" => {
                        let live = self.pools[id].iter().filter(|s| s.live).count() as i64;
                        if method == "len" {
                            Ok(Flow::Val(Value::Int(live)))
                        } else {
                            Ok(Flow::Val(Value::Bool(live == 0)))
                        }
                    }
                    "alive" => {
                        let h = args.into_iter().flat_map(|l| l.args()).find_map(Arg::value);
                        let Some(v) = h else {
                            return self.refuse("pool.alive without a handle", e.span);
                        };
                        let Value::Handle { index, generation } = val!(self.eval(v)) else {
                            return self.refuse("pool.alive with a non-handle", e.span);
                        };
                        let live = index < self.pools[id].len()
                            && self.pools[id][index].generation == generation
                            && self.pools[id][index].live;
                        Ok(Flow::Val(Value::Bool(live)))
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
            // s37 — the builtin `str` surface (D24/D25): pure reads
            // over the two-word slice; byte offsets out; misses are
            // `{none}` rows, never traps. Views materialize `List`s
            // at v0 (the zero-copy protocol is D28's).
            Some(TyKind::Prim(Prim::Str)) => {
                let sv = if let Some(place) = self.place_of(recv)? {
                    self.read_place(&place, recv.span)?
                } else {
                    val!(self.eval(recv))
                };
                let Value::Str(s) = sv else {
                    return self.refuse("str method on a non-str", e.span);
                };
                let mut argv = Vec::new();
                for a in args.into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        argv.push(val!(self.eval(v)));
                    }
                }
                let none_miss = || {
                    Ok(Flow::Err(Value::ErrTag {
                        tag: "none".to_string(),
                        payload: Vec::new(),
                    }))
                };
                let needle = |i: usize| -> Option<String> {
                    match argv.get(i) {
                        Some(Value::Str(n)) => Some(n.clone()),
                        _ => None,
                    }
                };
                let make_list = |m: &mut Self, items: Vec<Value>| -> E<Flow> {
                    m.charge_mem(16 * items.len() as u64 + 16)?;
                    let id = m.lists.len();
                    m.lists.push(items);
                    Ok(Flow::Val(Value::List(id)))
                };
                match method {
                    "is_empty" => Ok(Flow::Val(Value::Bool(s.is_empty()))),
                    // The boundary primitive (wolf-lang#17): the
                    // recoverable slice — OOB *and* split-code-point
                    // offsets are the same honest miss.
                    "get" => {
                        let Some(Value::Range { start, end }) = argv.first() else {
                            return self.refuse("str.get without a range", e.span);
                        };
                        let (a, z) = (*start, *end);
                        if a < 0 || z < a || z > s.len() as i64 {
                            return none_miss();
                        }
                        let (a, z) = (a as usize, z as usize);
                        if !s.is_char_boundary(a) || !s.is_char_boundary(z) {
                            return none_miss();
                        }
                        Ok(Flow::Val(Value::Str(s[a..z].to_string())))
                    }
                    "bytes" => {
                        let items: Vec<Value> = s.bytes().map(|b| Value::Int(b as i64)).collect();
                        make_list(self, items)
                    }
                    "starts_with" => match needle(0) {
                        Some(n) => Ok(Flow::Val(Value::Bool(s.starts_with(&n)))),
                        None => self.refuse("starts_with without a str needle", e.span),
                    },
                    "ends_with" => match needle(0) {
                        Some(n) => Ok(Flow::Val(Value::Bool(s.ends_with(&n)))),
                        None => self.refuse("ends_with without a str needle", e.span),
                    },
                    "contains" => match needle(0) {
                        Some(n) => Ok(Flow::Val(Value::Bool(s.contains(&n)))),
                        None => self.refuse("contains without a str needle", e.span),
                    },
                    "find" | "rfind" => {
                        let Some(n) = needle(0) else {
                            return self.refuse("find without a str needle", e.span);
                        };
                        let hit = if method == "find" {
                            s.find(&n)
                        } else {
                            s.rfind(&n)
                        };
                        match hit {
                            Some(off) => Ok(Flow::Val(Value::Int(off as i64))),
                            None => none_miss(),
                        }
                    }
                    "count" => match needle(0) {
                        Some(n) if n.is_empty() => self.refuse("count of an empty needle", e.span),
                        Some(n) => Ok(Flow::Val(Value::Int(s.matches(&n).count() as i64))),
                        None => self.refuse("count without a str needle", e.span),
                    },
                    "split" => match needle(0) {
                        Some(n) if n.is_empty() => {
                            self.refuse("split on an empty separator", e.span)
                        }
                        Some(n) => {
                            let items: Vec<Value> = s
                                .split(n.as_str())
                                .map(|p| Value::Str(p.to_string()))
                                .collect();
                            make_list(self, items)
                        }
                        None => self.refuse("split without a str separator", e.span),
                    },
                    // Unicode `White_Space`, matching the builtin set
                    // wolf-std pinned code point by code point (#18).
                    "words" => {
                        let items: Vec<Value> = s
                            .split_whitespace()
                            .map(|p| Value::Str(p.to_string()))
                            .collect();
                        make_list(self, items)
                    }
                    "lines" => {
                        let items: Vec<Value> =
                            s.lines().map(|p| Value::Str(p.to_string())).collect();
                        make_list(self, items)
                    }
                    "trim" => Ok(Flow::Val(Value::Str(s.trim().to_string()))),
                    "trim_start" => Ok(Flow::Val(Value::Str(s.trim_start().to_string()))),
                    "trim_end" => Ok(Flow::Val(Value::Str(s.trim_end().to_string()))),
                    "lower" => Ok(Flow::Val(Value::Str(s.to_lowercase()))),
                    "upper" => Ok(Flow::Val(Value::Str(s.to_uppercase()))),
                    "strip_prefix" | "strip_suffix" => {
                        let Some(n) = needle(0) else {
                            return self.refuse("strip without a str needle", e.span);
                        };
                        let hit = if method == "strip_prefix" {
                            s.strip_prefix(&n)
                        } else {
                            s.strip_suffix(&n)
                        };
                        match hit {
                            Some(rest) => Ok(Flow::Val(Value::Str(rest.to_string()))),
                            None => none_miss(),
                        }
                    }
                    "repeat" => {
                        let Some(Value::Int(n)) = argv.first() else {
                            return self.refuse("repeat without a count", e.span);
                        };
                        if *n < 0 {
                            return self.trap("bounds", "mem.ub.defined", e.span);
                        }
                        self.charge_mem(s.len() as u64 * *n as u64 + 16)?;
                        Ok(Flow::Val(Value::Str(s.repeat(*n as usize))))
                    }
                    "replace" => {
                        let (Some(from), Some(to)) = (needle(0), needle(1)) else {
                            return self.refuse("replace without str arguments", e.span);
                        };
                        if from.is_empty() {
                            return self.refuse("replace of an empty needle", e.span);
                        }
                        self.charge_mem(s.len() as u64 + 16)?;
                        Ok(Flow::Val(Value::Str(s.replace(&from, &to))))
                    }
                    _ => self.refuse("this `str` method in checked execution", e.span),
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
                if let Value::ErrTag { .. } = out {
                    return Ok(Flow::Err(out));
                }
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
        // IEEE equality: `nan != nan`, `-0.0 == 0.0`.
        (Value::F64(x), Value::F64(y)) => x == y,
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

/// Dedent a `"""` string's inner bytes by the closing delimiter's
/// column (D26): content starts on the line after the opening
/// delimiter; the whitespace run after the last newline — the closing
/// quotes' own indentation — strips from every line and is dropped
/// itself.
fn dedent_multiline(inner: &[u8]) -> Vec<u8> {
    let mut inner = inner;
    if inner.starts_with(b"\r\n") {
        inner = &inner[2..];
    } else if inner.first() == Some(&b'\n') {
        inner = &inner[1..];
    }
    let last_nl = inner.iter().rposition(|&b| b == b'\n');
    let (body, indent) = match last_nl {
        Some(i) => inner.split_at(i + 1),
        None => return inner.to_vec(),
    };
    if !indent.iter().all(|&b| b == b' ' || b == b'\t') {
        // The closing quotes share the last content line: no dedent.
        return inner.to_vec();
    }
    let mut out = Vec::with_capacity(body.len());
    let mut start = 0;
    while start < body.len() {
        let end = body[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p + 1)
            .unwrap_or(body.len());
        let line = &body[start..end];
        let stripped = if line.starts_with(indent) {
            &line[indent.len()..]
        } else {
            line
        };
        out.extend_from_slice(stripped);
        start = end;
    }
    out
}

/// The escape decoder over a hole-free byte run — the same set the
/// segmented rebuild uses, factored for the multiline path.
fn decode_escapes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && i + 1 < bytes.len() {
            if let Some((ch, consumed)) = decode_codepoint_escape(&bytes[i..]) {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                i += consumed;
                continue;
            }
            out.push(match bytes[i + 1] {
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
        if (c == b'{' || c == b'}') && bytes.get(i + 1) == Some(&c) {
            out.push(c);
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Decode a `\xNN` or `\u{…}` escape at the start of `bytes` (which
/// begins at the backslash). Returns the code point and the total
/// bytes consumed, or `None` when the shape is not one of the two —
/// the caller falls back to the single-byte escape set.
fn decode_codepoint_escape(bytes: &[u8]) -> Option<(char, usize)> {
    match bytes.get(1)? {
        b'x' => {
            let hex = bytes.get(2..4)?;
            let s = std::str::from_utf8(hex).ok()?;
            let n = u32::from_str_radix(s, 16).ok()?;
            Some((char::from_u32(n)?, 4))
        }
        b'u' => {
            if bytes.get(2) != Some(&b'{') {
                return None;
            }
            let close = bytes[3..].iter().position(|&b| b == b'}')?;
            let s = std::str::from_utf8(&bytes[3..3 + close]).ok()?;
            if s.is_empty() || s.len() > 6 {
                return None;
            }
            let n = u32::from_str_radix(s, 16).ok()?;
            Some((char::from_u32(n)?, 3 + close + 1))
        }
        _ => None,
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        // The shortest round-trip decimal, `std.fmt.decimal.to_str`'s
        // layout — the s38 reference rendering (spec §7.4 candidate).
        Value::F64(x) => wolf_sema::fmtspec::f64_shortest(*x),
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
