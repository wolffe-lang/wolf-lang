//! The miri-lite UB checker (s23) — the operational model, executable.
//!
//! An interpretive checker over sema's typed HIR plus this crate's own
//! facts: it executes the SAME dynamic semantics the corpus pins, at
//! the depth needed to decide `[mem.ub]` rows P1–P6/L1–L3/T1 on
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

use std::collections::{HashMap, HashSet};

use wolf_ast::{
    Arg, AssignStmt, Block as AstBlock, BorrowExpr, BracketApply, CallExpr, CastExpr, DeferStmt,
    ElseExpr, ExprStmt, FieldInit, ForExpr, GreenNode, IfExpr, InBlock, MatchExpr, MemberExpr,
    ParenExpr, PrefixExpr, RangeExpr, RegionBlock, ReturnExpr, StringExpr, StructLit, SyntaxKind,
    TupleExpr, UnsafeBlock, WhileExpr, is_pattern_kind,
};
use wolf_diag::{Diagnostic, codes};
use wolf_sema::check::{CallSig, CastKind, Dispatch};
use wolf_sema::sig::ItemSig;
use wolf_sema::types::{Prim, TyId, TyKind, TypeTable};
use wolf_sema::{BodyResult, Fold, NotYet, Package, Typecheck, TypedBody};
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
    /// kw07: a volatile access through a misaligned address
    /// (`[mem.unsafe.volatile.3]`).
    L3,
    /// s209 (ruling #36 = A): an ordinary raw access through an
    /// address not aligned to its pointee (`[mem.unsafe.raw.4]`).
    L4,
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
            UbRow::L3 => "L3",
            UbRow::L4 => "L4",
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
            UbRow::L3 => "mem.unsafe.volatile",
            UbRow::L4 => "mem.unsafe.raw.4",
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
            UbRow::L3 => {
                "O11: each volatile call is one aligned machine access of its \
                          width — no split into narrower accesses, no alignment check"
            }
            UbRow::L4 => {
                "O12: every ordinary raw access is emitted at the pointee's natural \
                          alignment — no alignment check, no split into narrower accesses"
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
    /// The PROGRAM's own words for this fault, when it wrote any:
    /// today exactly `assert(cond, msg)`'s second argument, evaluated
    /// on the failing path and only there (`[conf.trap.assert]`).
    ///
    /// s169 ([proto.record.trap], wolf-lang#150). The machine already
    /// evaluated this string — it has to, the clause says the message
    /// is evaluated on the failing path — and then dropped it on the
    /// floor, so every runner reading a record saw `trap(assert)` and
    /// nothing else. `None` is honest-absent, never an empty string.
    pub message: Option<String>,
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
    /// Bytes ever charged by allocations attributed to this region
    /// (s131, #187): this machine's own ledger for the accounting
    /// queries — cumulative over the region's lifetime, zero at
    /// creation, in the shadow-memory model's units (each modeled
    /// allocation's size, not the native tier's rounded container
    /// charges; `[mem.region.account.1]` pins the relations, not the
    /// units).
    charged: u64,
    /// The creation-time byte budget (s132, `[mem.region.cap.1]`,
    /// D68/#187): a charge that takes `charged` past this traps
    /// `alloc-contract` at the charging site — in THIS machine's
    /// units, like the ledger itself (the cap compares against the
    /// ledger, whatever the tier's ledger charges). `None` =
    /// unbounded.
    cap: Option<u64>,
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
    /// A `char` (s121, D58): a Unicode scalar value — the host `char`
    /// carries exactly the ruled domain (`0..=0x10FFFF` minus the
    /// surrogate gap), so an out-of-domain `Value::Char` is
    /// unconstructible by the same rule the compiled lanes trap on.
    Char(char),
    /// A `byte` (s135, D72): one octet. The host `u8` IS the ruled
    /// domain, so an out-of-range byte is unconstructible; the cast
    /// in truncates (`n as u8`, the low eight bits) and the cast out
    /// widens, and every operator widens first ([type.byte.op]) — a
    /// `Value::Byte` never reaches integer arithmetic as itself. The
    /// list slot it charges is ONE byte (`slot_bytes`), the property
    /// wolf-lang#203 asked for on this tier.
    Byte(u8),
    /// The executable float (s38): `f64` values under IEEE semantics —
    /// arithmetic never traps (X3 is integer law; inf/nan are values).
    /// `f32` stays an honest refusal until a use case rules its
    /// rounding story.
    F64(f64),
    Str(String),
    Range {
        start: i64,
        end: i64,
        /// `range[char]` rather than `range[int]` (s158,
        /// `[type.range.name]`): the endpoints are scalar values, and
        /// this says to read them back as `char` — `r.start` on
        /// `'a'..'d'` is `a`, not `97`.
        chars: bool,
    },
    Struct {
        fields: Vec<(String, Value)>,
    },
    /// A top-level fn as a VALUE (s95/s97's fn values, the checked
    /// twin): the body index. Copies — a fn value is a pointer.
    Fn(usize),
    /// A trait object (D47/s98's checked twin): the concrete type's
    /// name rides the value so `dyn_call` dispatch can resolve the
    /// impl at run time — the executor's answer to the native pair's
    /// vtable half. Value-semantic: the mem tier's static loan (the
    /// pair borrows its place) already refused every program where
    /// cloning the inner is observable.
    Dyn {
        concrete: String,
        inner: Box<Value>,
    },
    Enum {
        variant: String,
        payload: Vec<Value>,
    },
    List(usize),
    /// A `Map[K, V]` (s152, `[type.map]`): an index into the
    /// machine's map arena — entries in insertion order, keys one of
    /// the four `[type.map.key]` admits. Moves, like a `List`.
    Map(usize),
    Pool(usize),
    /// A `channel[T]` (#342, `[conc.chan]`): an index into the
    /// machine's channel arena. A handle — copying it names the same
    /// channel, as the native tier's pointer does.
    Chan(usize),
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
                | Value::Char(_)
                | Value::Byte(_)
                | Value::F64(_)
                | Value::Str(_)
                | Value::Range { .. }
                | Value::Handle { .. }
                | Value::Ptr(_)
                | Value::Fn(_)
                | Value::Chan(_)
        )
    }
}

/// A `Map` key as the machine compares it (s152, `[type.map.key]`):
/// exactly the four admitted key types, so equality is the language's
/// own and never a user impl's.
#[derive(Debug, Clone, PartialEq)]
enum MapKey {
    Int(i64),
    Str(String),
    Char(char),
    Bool(bool),
}

impl MapKey {
    fn of(v: &Value) -> Option<MapKey> {
        match v {
            Value::Int(i) => Some(MapKey::Int(*i)),
            Value::Str(s) => Some(MapKey::Str(s.clone())),
            Value::Char(c) => Some(MapKey::Char(*c)),
            Value::Bool(b) => Some(MapKey::Bool(*b)),
            _ => None,
        }
    }

    fn value(&self) -> Value {
        match self {
            MapKey::Int(i) => Value::Int(*i),
            MapKey::Str(s) => Value::Str(s.clone()),
            MapKey::Char(c) => Value::Char(*c),
            MapKey::Bool(b) => Value::Bool(*b),
        }
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
    /// `m[k]` as a WRITE target (`[mem.map.absent]`): the insert-or-
    /// replace place. Reads never build this step — an absent key
    /// answers the `none` row at the expression, not a place fault.
    MapKey {
        key: MapKey,
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
    /// `os_exit(code)` (s40): the program asked to stop — an ordinary
    /// exit verdict, not a trap. Defers do NOT run (the documented
    /// `os.exit` contract: immediate termination; native calls the
    /// runtime exit with the same rule).
    Exit(u8),
}

/// One channel (#342, `[conc.chan.buf]`/`[conc.chan.close]`): its
/// buffered payloads, oldest first, its capacity (0 is rendezvous), and
/// whether it is closed.
struct ChanState {
    buf: std::collections::VecDeque<Value>,
    cap: usize,
    closed: bool,
}

/// Control flow out of an expression.
enum Flow {
    Val(Value),
    Return(Value),
    /// A row error unwinding. The flag is PROPAGATION (#122): `false`
    /// for a raw row value (a raise site, a fallible call's error
    /// half) — the shape that BINDS at `let`/`var` (D52's
    /// declared-row-first reading; rows are values, and native and
    /// lupin both bind here) — `true` once a `?` has explicitly
    /// propagated it, which keeps unwinding through every binder.
    Err(Value, bool),
    Break,
    Continue,
}

/// A raw (non-propagating) row error — see [`Flow::Err`].
fn raise(v: Value) -> Flow {
    Flow::Err(v, false)
}

type E<T> = Result<T, Stop>;

/// What a place lookup (or a byte view) found (wolf-lang#481): the
/// thing itself, nothing ("not a place" — the caller evaluates the
/// expression instead, and no operand has run), or the FLOW an operand
/// produced on the way (a propagating `?`, a `return`, a `break`). An
/// operand runs once ([mem.model.order]), so a flow is handed back and
/// returned by the caller, never dropped and re-run by its fallback.
enum Found<T> {
    At(T),
    Not,
    Flow(Flow),
}

/// `Found` as an `Option`, returning an operand's flow from the
/// enclosing `E<Flow>` function at the point its `eval` fallback would
/// have returned it.
macro_rules! found {
    ($e:expr) => {
        match $e? {
            Found::At(x) => Some(x),
            Found::Not => None,
            Found::Flow(f) => return Ok(f),
        }
    };
}

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
    /// s136 (#227, `[os.net.unix]`): an `AF_UNIX` listener and its
    /// path — unlinked at `net_close` (the runtime's posture, mirrored).
    #[cfg(unix)]
    UnixListener(std::os::unix::net::UnixListener, std::path::PathBuf),
    #[cfg(unix)]
    UnixStream(std::os::unix::net::UnixStream),
}

impl NetSock {
    fn is_stream(&self) -> bool {
        match self {
            NetSock::Stream(_) => true,
            #[cfg(unix)]
            NetSock::UnixStream(_) => true,
            _ => false,
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::io::Read as _;
        match self {
            NetSock::Stream(s) => s.read(buf),
            #[cfg(unix)]
            NetSock::UnixStream(s) => s.read(buf),
            _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write as _;
        match self {
            NetSock::Stream(s) => s.write_all(bytes),
            #[cfg(unix)]
            NetSock::UnixStream(s) => s.write_all(bytes),
            _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
        }
    }

    /// s141 (#254, `[os.net.writev]`): every part in order as one
    /// gathered write — std's `write_vectored` (`writev(2)` on
    /// unix, `WSASend` on windows) driven to completion, the mirror
    /// of the runtime's `NetTable::writev`. Empty parts never reach
    /// the kernel; the checked machine's streams are blocking, so a
    /// short write is followed by the next syscall, never a park.
    fn write_all_vectored(&mut self, parts: &[Vec<u8>]) -> std::io::Result<()> {
        use std::io::Write as _;
        let mut slices: Vec<std::io::IoSlice<'_>> = parts
            .iter()
            .filter(|p| !p.is_empty())
            .map(|p| std::io::IoSlice::new(p))
            .collect();
        let mut bufs: &mut [std::io::IoSlice<'_>] = &mut slices;
        while !bufs.is_empty() {
            let n = match self {
                NetSock::Stream(s) => s.write_vectored(bufs),
                #[cfg(unix)]
                NetSock::UnixStream(s) => s.write_vectored(bufs),
                _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
            }?;
            if n == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
            }
            std::io::IoSlice::advance_slices(&mut bufs, n);
        }
        Ok(())
    }

    /// s141 (#254, `[os.net.nodelay]`): the TCP stream's Nagle
    /// switch; any other socket is `io` (the option is TCP's).
    fn set_nodelay(&mut self, on: bool) -> std::io::Result<()> {
        match self {
            NetSock::Stream(s) => s.set_nodelay(on),
            _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
        }
    }

    /// s141 (#254): the posture a TCP stream enters the table with,
    /// the runtime's `push_stream` mirrored — Nagle OFF. A unix
    /// stream has no such option and passes through.
    fn under_posture(self) -> std::io::Result<NetSock> {
        if let NetSock::Stream(s) = &self {
            s.set_nodelay(true)?;
        }
        Ok(self)
    }

    fn set_timeouts(&mut self, budget: Option<std::time::Duration>) -> std::io::Result<()> {
        match self {
            NetSock::Stream(s) => s
                .set_read_timeout(budget)
                .and_then(|()| s.set_write_timeout(budget)),
            #[cfg(unix)]
            NetSock::UnixStream(s) => s
                .set_read_timeout(budget)
                .and_then(|()| s.set_write_timeout(budget)),
            _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
        }
    }

    /// Is the stream's handle still live (EBADF says no)? The #224
    /// reset-socket probe, either family.
    fn alive(&self) -> bool {
        match self {
            NetSock::Stream(s) => s.local_addr().is_ok(),
            #[cfg(unix)]
            NetSock::UnixStream(s) => s.local_addr().is_ok(),
            _ => false,
        }
    }

    /// One accepted connection of the listener's family; an armed
    /// budget bounds the park (s106 — the `timeout` tag, reachable).
    fn accept_with(&self, budget: Option<std::time::Duration>) -> std::io::Result<NetSock> {
        match self {
            NetSock::Listener(l) => match budget {
                Some(b) => accept_deadline(l, b).map(|(s, _)| NetSock::Stream(s)),
                None => l.accept().map(|(s, _)| NetSock::Stream(s)),
            },
            #[cfg(unix)]
            NetSock::UnixListener(l, _) => match budget {
                Some(b) => accept_deadline_unix(l, b).map(|(s, _)| NetSock::UnixStream(s)),
                None => l.accept().map(|(s, _)| NetSock::UnixStream(s)),
            },
            _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
        }
    }
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
        // s136 (#227): the unix-domain path rows — declared only by
        // `net_listen_unix`/`net_connect_unix`; `coarse` folds them to
        // `io` everywhere else.
        K::NotFound => "not_found",
        K::PermissionDenied => "denied",
        K::AddrInUse => "exists",
        _ => "io",
    }
}

/// OS entropy for the checked lane's `os_random` (s118, #143): the
/// HAND MIRROR of `wolf_rt::random::fill` — wolf_mem and wolf_rt may
/// not see each other (the locked graph; D15), so the platform matrix
/// of spec `[os.random.platform]` is implemented here directly, the
/// [`net_err_tag`] precedent. `true` iff EVERY byte came from the
/// platform CSPRNG; on `false` the caller traps (`[os.random.trap]`)
/// — there is deliberately no fallback of any kind in this function.
///
/// Linux: `getrandom(2)` flags 0 (blocks only until the pool
/// initializes; EINTR retries; short reads continue).
#[cfg(target_os = "linux")]
fn os_entropy_fill(buf: &mut [u8]) -> bool {
    let mut done = 0usize;
    while done < buf.len() {
        let rest = &mut buf[done..];
        // SAFETY: live buffer, correct length, flags 0.
        let n = unsafe { libc::getrandom(rest.as_mut_ptr().cast(), rest.len(), 0) };
        if n < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return false;
        }
        done += n as usize;
    }
    true
}

/// macOS / FreeBSD: `getentropy(3)` in 256-byte chunks (the call's own
/// per-request cap — a larger request is EIO by contract).
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
fn os_entropy_fill(buf: &mut [u8]) -> bool {
    for chunk in buf.chunks_mut(256) {
        // SAFETY: live chunk, length <= 256 by construction.
        if unsafe { libc::getentropy(chunk.as_mut_ptr().cast(), chunk.len()) } != 0 {
            return false;
        }
    }
    true
}

/// Windows (tier-1): `BCryptGenRandom` with the system-preferred RNG —
/// the documented modern call, declared directly against bcrypt.dll
/// (no crate; the wolf_rt::random posture).
#[cfg(windows)]
fn os_entropy_fill(buf: &mut [u8]) -> bool {
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(
            halgorithm: *mut core::ffi::c_void,
            pbbuffer: *mut u8,
            cbbuffer: u32,
            dwflags: u32,
        ) -> i32;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    for chunk in buf.chunks_mut(1 << 30) {
        // SAFETY: live chunk; length fits u32 by construction.
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                chunk.as_mut_ptr(),
                chunk.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status != 0 {
            return false;
        }
    }
    true
}

/// The NAMED platform gap (`[os.random.platform]`): no entropy backend
/// here yet, so `os_random` TRAPS — never a PRNG, never silence.
#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    windows
)))]
fn os_entropy_fill(_buf: &mut [u8]) -> bool {
    false
}

/// Accept under a deadline budget on the checked lane's v0 blocking
/// path (s106, `net_deadline`): std's listener has no timed accept,
/// so the emulation polls nonblocking until the budget elapses — the
/// checked twin of the native reactor's timer wheel. A fired budget
/// is `TimedOut`, which [`net_err_tag`] resolves as the `timeout` row.
/// `accept_deadline`'s unix-domain twin (s136): the same 1 ms poll
/// against the budget, over an `AF_UNIX` listener.
#[cfg(unix)]
fn accept_deadline_unix(
    l: &std::os::unix::net::UnixListener,
    budget: std::time::Duration,
) -> std::io::Result<(
    std::os::unix::net::UnixStream,
    std::os::unix::net::SocketAddr,
)> {
    let t0 = std::time::Instant::now();
    l.set_nonblocking(true)?;
    let out = loop {
        match l.accept() {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if t0.elapsed() >= budget {
                    break Err(std::io::Error::from(std::io::ErrorKind::TimedOut));
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            other => break other,
        }
    };
    let _ = l.set_nonblocking(false);
    if let Ok((s, _)) = &out {
        let _ = s.set_nonblocking(false);
    }
    out
}

/// s137 (#234, `[os.net.listen.opts]`): the checked machine's bind
/// with listener options — the HAND MIRROR of `wolf_rt::net::bind_with`
/// (wolf_mem and wolf_rt may not see each other; D15), call for call:
/// a close-on-exec stream socket, `SO_REUSEADDR` as std sets it,
/// `SO_REUSEPORT` before the bind when asked, the caller's backlog
/// hint (`<= 0`: std's 128). The driver's `net_parity` test pins the
/// two tables' rows; the option semantics per host are the runtime's
/// module doc and the clause.
#[cfg(unix)]
fn checked_bind_with(
    addr: &str,
    reuse_port: bool,
    backlog: i64,
) -> std::io::Result<std::net::TcpListener> {
    use std::net::{SocketAddr, ToSocketAddrs as _};
    use std::os::fd::FromRawFd as _;
    let sa = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let family = match sa {
        SocketAddr::V4(_) => libc::AF_INET,
        SocketAddr::V6(_) => libc::AF_INET6,
    };
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let ty = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let ty = libc::SOCK_STREAM;
    // SAFETY: plain socket creation; the result is checked below.
    let fd = unsafe { libc::socket(family, ty, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a fresh, open stream socket nothing else owns; owned
    // from here so every early return closes it.
    let l = unsafe { std::net::TcpListener::from_raw_fd(fd) };
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    {
        // SAFETY: valid fd.
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    let one: libc::c_int = 1;
    let optlen = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    for (want, opt) in [(true, libc::SO_REUSEADDR), (reuse_port, libc::SO_REUSEPORT)] {
        if !want {
            continue;
        }
        // SAFETY: valid fd, a live c_int and its true length.
        if unsafe { libc::setsockopt(fd, libc::SOL_SOCKET, opt, (&raw const one).cast(), optlen) }
            < 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    // SAFETY: a zeroed sockaddr_storage is a valid, oversized buffer
    // for either family.
    let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let len: libc::socklen_t = match sa {
        SocketAddr::V4(v4) => {
            // SAFETY: sockaddr_in fits inside sockaddr_storage.
            let sin = unsafe { &mut *(&raw mut storage).cast::<libc::sockaddr_in>() };
            sin.sin_family = libc::AF_INET as libc::sa_family_t;
            sin.sin_port = v4.port().to_be();
            sin.sin_addr = libc::in_addr {
                s_addr: u32::from_ne_bytes(v4.ip().octets()),
            };
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t
        }
        SocketAddr::V6(v6) => {
            // SAFETY: sockaddr_in6 fits inside sockaddr_storage.
            let sin6 = unsafe { &mut *(&raw mut storage).cast::<libc::sockaddr_in6>() };
            sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sin6.sin6_port = v6.port().to_be();
            sin6.sin6_flowinfo = v6.flowinfo();
            sin6.sin6_addr = libc::in6_addr {
                s6_addr: v6.ip().octets(),
            };
            sin6.sin6_scope_id = v6.scope_id();
            std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t
        }
    };
    // SAFETY: valid fd, a live sockaddr and its length.
    if unsafe { libc::bind(fd, (&raw const storage).cast(), len) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let queue = if backlog > 0 {
        i32::try_from(backlog).unwrap_or(i32::MAX)
    } else {
        128
    };
    // SAFETY: valid, bound fd.
    if unsafe { libc::listen(fd, queue) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(l)
}

/// s137 (#127, `[os.net.wait]`): the checked machine's readiness poll
/// — the HAND MIRROR of the runtime's `reactor::poll_readable`
/// (wolf_mem and wolf_rt may not see each other; D15). One flag per
/// input descriptor, in the input's order: true when the next read
/// will not block (data, a pending connection, an ended peer, an
/// error). `EINTR` retries.
#[cfg(unix)]
fn checked_poll_readable(
    fds: &[std::os::fd::RawFd],
    timeout_ms: i32,
) -> std::io::Result<Vec<bool>> {
    let mut pfds: Vec<libc::pollfd> = fds
        .iter()
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    loop {
        // SAFETY: a live, correctly-counted pollfd array.
        let rc = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, timeout_ms) };
        if rc < 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EINTR) {
                for p in &mut pfds {
                    p.revents = 0;
                }
                continue;
            }
            return Err(e);
        }
        let ready = libc::POLLIN | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL;
        return Ok(pfds.iter().map(|p| p.revents & ready != 0).collect());
    }
}

/// The windows twin of [`checked_poll_readable`] — `WSAPoll` over the
/// caller's set, declared directly (the runtime's own windows rung is
/// built on the same call; D15 keeps the two implementations apart,
/// not the syscall). Without it `net_wait` would be the one net call
/// with a per-host row, and `[os.net.wait]` would owe a refusal
/// sentence for a host whose kernel answers the question perfectly
/// well.
#[cfg(windows)]
fn checked_poll_readable(
    fds: &[std::os::windows::io::RawSocket],
    timeout_ms: i32,
) -> std::io::Result<Vec<bool>> {
    #[repr(C)]
    struct PollFd {
        fd: usize,
        events: i16,
        revents: i16,
    }
    const POLLRDNORM: i16 = 0x0100;
    const POLLERR: i16 = 0x0001;
    const POLLHUP: i16 = 0x0002;
    const POLLNVAL: i16 = 0x0004;
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn WSAPoll(fds: *mut PollFd, count: u32, timeout_ms: i32) -> i32;
        fn WSAGetLastError() -> i32;
    }
    let mut pfds: Vec<PollFd> = fds
        .iter()
        .map(|&fd| PollFd {
            fd: fd as usize,
            events: POLLRDNORM,
            revents: 0,
        })
        .collect();
    // SAFETY: a live, correctly-counted WSAPOLLFD array.
    let rc = unsafe { WSAPoll(pfds.as_mut_ptr(), pfds.len() as u32, timeout_ms) };
    if rc < 0 {
        // SAFETY: a plain error read.
        return Err(std::io::Error::from_raw_os_error(unsafe {
            WSAGetLastError()
        }));
    }
    let ready = POLLRDNORM | POLLHUP | POLLERR | POLLNVAL;
    Ok(pfds.iter().map(|p| p.revents & ready != 0).collect())
}

fn accept_deadline(
    l: &std::net::TcpListener,
    budget: std::time::Duration,
) -> std::io::Result<(std::net::TcpStream, std::net::SocketAddr)> {
    let t0 = std::time::Instant::now();
    l.set_nonblocking(true)?;
    let out = loop {
        match l.accept() {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if t0.elapsed() >= budget {
                    break Err(std::io::Error::from(std::io::ErrorKind::TimedOut));
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            other => break other,
        }
    };
    let _ = l.set_nonblocking(false);
    // The accepted stream must come back BLOCKING whatever the
    // listener's mode was during the poll (the checked machine's
    // budgets are the socket's own timeouts, not a reactor's).
    if let Ok((s, _)) = &out {
        let _ = s.set_nonblocking(false);
    }
    out
}

struct Ctx<'t> {
    tb: &'t TypedBody,
    node: &'t GreenNode,
    calls: HashMap<Span, &'t CallSig>,
    /// Folded comptime call sites by span (s71): the site evaluates
    /// to this constant; the machine never steps into the callee.
    folds: HashMap<Span, &'t Fold>,
    casts: HashMap<Span, (TyId, TyId, CastKind)>,
    expr_tys: HashMap<Span, TyId>,
    dispatch: HashMap<Span, &'t Dispatch>,
    src_file: usize,
    /// s207 (wolf-lang#541, `[type.unit.discard]`): the `!T` tails
    /// sema accepted in a unit context — the block's value there is
    /// `()`, and the row is lost.
    unit_discards: HashSet<Span>,
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
    /// Trait-impl methods, keyed (self type, trait, method) — the
    /// trait in the key keeps two traits' same-named methods on one
    /// type apart (#12).
    trait_methods: HashMap<(String, String, String), usize>,
    /// Trait DEFAULT bodies, keyed (trait module, trait, method):
    /// executed when no impl overrides (s95's `Self ↦ subject`, the
    /// checked twin — the subject rides `self_tys`).
    trait_defaults: HashMap<(usize, String, String), usize>,
    /// Per-frame concrete `Self`, pushed beside `frames`: inside a
    /// trait default body the receiver types as `Rigid("Self")`, and
    /// this stack is what names the subject at nested dispatch.
    self_tys: Vec<Option<String>>,
    /// The next `call_body`'s `Self` — set by dispatch sites,
    /// consumed exactly once at frame push.
    pending_self_ty: Option<String>,
    /// Per-frame generic bindings, name → concrete nominal: built at
    /// the CALL site from the caller's own typed arguments, so a
    /// `Show.show(v)` inside `fn describe[T: Show](v: T)` can name
    /// the type `T` stands for this call (#12). The machine executes
    /// generic bodies directly — this map is its monomorphization.
    frame_rigids: Vec<HashMap<String, String>>,
    /// The next `call_body`'s rigid bindings, like `pending_self_ty`.
    pending_rigids: Option<HashMap<String, String>>,

    allocs: Vec<Allocation>,
    regions: Vec<DynRegion>,
    lists: Vec<Vec<Value>>,
    /// Each list's BIRTH region (s131, #187): the ambient region at
    /// the site that minted it — the native rule ("growth stays in
    /// the birth region"), so the ledger charges pushes to the region
    /// that owns the storage, not the region open at the push site.
    list_region: Vec<usize>,
    /// The map arena (s152): each map's entries in insertion order,
    /// and each map's birth region, as for lists.
    maps: Vec<Vec<(MapKey, Value)>>,
    map_region: Vec<usize>,
    pools: Vec<Vec<PoolSlot>>,
    /// The channel arena (#342): every channel the run made, in the
    /// order it made them.
    chans: Vec<ChanState>,
    cells: Vec<RcCell>,
    frames: Vec<Frame<'t>>,
    /// The dynamic ambient-region stack; `[0]` is the run's root
    /// region (never freed while the run lives).
    ambient: Vec<usize>,
    /// What the program wrote to standard output, as BYTES (s200,
    /// wolf-lang#405: `fs_write_chunk(1, …)` writes octets that need not
    /// be UTF-8). Decoded once, lossily, at the end of the run — the
    /// decode the driver applies to a native child's stdout for the
    /// record (`main.rs`), so the two tiers agree by construction.
    stdout: Vec<u8>,
    /// What the program wrote through `eprint`/`eprint_raw` (s38), and
    /// since s200 through `fs_write*` on descriptor 2, as bytes.
    stderr: Vec<u8>,
    /// s200 (#407, `[os.fs.error]`): the host's number for the most recent
    /// fallible fs-family call, 0 when it succeeded or failed before the
    /// host. This machine is one task, so the word is the task's.
    last_os_error: i64,
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
    /// Armed LISTENER deadline budgets (s106 `net_deadline`), keyed by
    /// the wolf fd: streams hold their budget on the socket itself
    /// (`set_read_timeout`/`set_write_timeout`), but std's listener
    /// has no timed accept, so the budget lives here and
    /// `accept_deadline` polls it out.
    sock_deadlines: HashMap<i64, std::time::Duration>,
    /// Spawned OS children (s40 os tier): a third handle namespace,
    /// same discipline — index = the `int` handle, `None` after a
    /// successful `os_wait` (the reap tombstones; double wait is
    /// `io`). `os_kill` does NOT tombstone: kill-then-wait is the
    /// natural pair and the wait observes the `signal` outcome.
    children: Vec<Option<std::process::Child>>,
    /// The program's argv (s40 `env_args`): conform-run supplies none
    /// — the checked lane's default is empty, mirroring the stdin
    /// posture (the native lane reads the process's real argv; `wolf
    /// run file.lu args…` passes them through).
    args: Vec<String>,
    /// Machine-local environment overlay (s40 `env_set`): checked
    /// writes land HERE, never in the host process's environment —
    /// the checked machine is a threaded test host and `setenv` is
    /// unsound under threads. Reads consult the overlay first, then
    /// the real environment. Documented lane asymmetry: native
    /// `env_set` writes the compiled program's own environment.
    env_overlay: HashMap<String, String>,
    /// s215 (`[os.fs.chdir]`): the machine-local working directory,
    /// `env_overlay`'s twin and for its reason — the checked machine
    /// runs inside threaded test hosts, where a real `chdir` would move
    /// every other thread's relative paths. `None` until the program's
    /// first `os_chdir`; then every relative path an fs or unix-socket
    /// call takes resolves against it, `os_cwd` answers it, and a
    /// spawned child starts in it. Always the canonical path (what the
    /// host's `getcwd` would answer after the same change).
    cwd: Option<std::path::PathBuf>,
    /// Signal RECEPTION (s114, #126): the checked machine models
    /// signals as a PURE IN-MACHINE queue (like `children`/
    /// `env_overlay`) — it never touches real OS signals, which would
    /// be unsound in a threaded test host (the `env_set` asymmetry).
    /// `os_signal_listen` records the interest set here; `os_signal_
    /// raise` enqueues a meaning if listened; `os_signal_wait` dequeues
    /// the first matching meaning. A wait with NO pending delivery is
    /// refused-by-name (the checked machine is single-threaded, run-to-
    /// completion — it has no concurrency to deliver one later; the
    /// spawn-driven witness likewise refuses at `SpawnExpr`). The
    /// sequential loopback (listen→raise→wait) is fully modeled.
    signal_listening: i64,
    signal_queue: std::collections::VecDeque<i64>,
    /// The monotonic anchor (s40 `time_now_ms`): an arbitrary
    /// process-local epoch, per X12's "monotonic, never wall".
    t0: std::time::Instant,
    steps: u64,
    mem_used: u64,
    budget: Budget,
    in_defer: bool,
    /// kw09 (`[mem.static]`): module state as ordinary memory — one
    /// slot per module `const`/`let`/`var` with a static value, started
    /// at its compile-time value. A [`Place`] whose frame is
    /// [`STATIC_FRAME`] names slot `local` here.
    statics: Vec<Value>,
    /// (module, item name) → its slot in [`Machine::statics`].
    static_slots: HashMap<(usize, String), usize>,
}

/// The frame index a module-state [`Place`] carries (kw09): no call
/// frame owns module state, so its places name [`Machine::statics`].
const STATIC_FRAME: usize = usize::MAX;

// The ubiquitous helpers.
impl<'t> Machine<'t> {
    fn refuse<T>(&self, construct: &'static str, span: Span) -> E<T> {
        Err(Stop::Refuse(NotYet { construct, span }))
    }

    fn trap<T>(&self, kind: &'static str, clause: &'static str, span: Span) -> E<T> {
        Err(Stop::Trap(TrapInfo {
            kind,
            clause,
            span,
            message: None,
        }))
    }

    /// [`Machine::trap`] carrying the program's own message for the
    /// fault (s169, `[proto.record.trap]`).
    fn trap_with<T>(
        &self,
        kind: &'static str,
        clause: &'static str,
        span: Span,
        message: Option<String>,
    ) -> E<T> {
        Err(Stop::Trap(TrapInfo {
            kind,
            clause,
            span,
            message,
        }))
    }

    /// The origin governing a subscript spelled at `site` (D61,
    /// `[gram.attr.index]`) — 0 everywhere in an unmarked package.
    fn origin_at(&self, site: Span) -> u8 {
        self.tc.sigs.origins.origin_at(site)
    }

    /// The 1-origin index shift (D61 `[gram.expr.index.origin]`),
    /// mirroring the WIR lowering's `isub.chk`: `int.min` traps
    /// `overflow`; every other index shifts down by one and the
    /// ordinary bounds check answers for it.
    fn shift_origin(&self, i: i64, site: Span) -> E<i64> {
        match i.checked_sub(1) {
            Some(v) => Ok(v),
            None => self.trap("overflow", "mem.ub.defined", site),
        }
    }

    /// The concrete nominal a checked expression's TYPE names, seen
    /// through this frame's generic bindings: `Nominal` directly,
    /// `Self` through `self_tys`, any other rigid through
    /// `frame_rigids` — so nested generic calls propagate.
    fn ty_concrete_name(&self, span: Span) -> Option<String> {
        match self.expr_ty(span)? {
            TyKind::Nominal { name, .. } => Some(name.clone()),
            // #119 (D49): a primitive is a dispatch target too —
            // `impl Ord for int` keys the trait index by the prim's
            // spelling, the same grammar the lowering mangles with.
            TyKind::Prim(p) => Some(p.name().to_string()),
            TyKind::Rigid(r) if r == "Self" => self.self_tys.last().cloned().flatten(),
            TyKind::Rigid(r) => self.frame_rigids.last().and_then(|m| m.get(r)).cloned(),
            _ => None,
        }
    }

    /// The concrete type a trait dispatch lands on (#12). Static when
    /// the checker typed the receiver as a nominal; the `self_tys`
    /// stack when it typed it `Self` (a trait default body's own
    /// receiver); the VALUE when the record says `dyn_call` (D47 —
    /// erasure makes the type a run-time fact, which is the one place
    /// this machine reads a type from a value).
    fn trait_concrete(
        &self,
        recv_span: Span,
        dyn_call: bool,
        recv_val: &Value,
        at: Span,
    ) -> E<String> {
        if dyn_call {
            return match recv_val {
                Value::Dyn { concrete, .. } => Ok(concrete.clone()),
                _ => self.refuse("a dyn dispatch on a non-dyn value", at),
            };
        }
        if let Some(name) = self.ty_concrete_name(recv_span) {
            return Ok(name);
        }
        match self.expr_ty(recv_span) {
            Some(TyKind::Dyn { .. }) => match recv_val {
                Value::Dyn { concrete, .. } => Ok(concrete.clone()),
                _ => self.refuse("a dyn dispatch on a non-dyn value", at),
            },
            _ => self.refuse("trait dispatch on a non-nominal receiver", at),
        }
    }

    /// The body a trait method call executes: the impl's override
    /// first, the trait's default second — s14's resolution order,
    /// read from the indexes `Machine::new` built.
    fn resolve_trait_body(
        &self,
        concrete: &str,
        tr_module: usize,
        tr_name: &str,
        method: &str,
        at: Span,
    ) -> E<usize> {
        if let Some(&b) = self.trait_methods.get(&(
            concrete.to_string(),
            tr_name.to_string(),
            method.to_string(),
        )) {
            return Ok(b);
        }
        if let Some(&b) =
            self.trait_defaults
                .get(&(tr_module, tr_name.to_string(), method.to_string()))
        {
            return Ok(b);
        }
        self.refuse("a trait method with neither an impl body nor a default", at)
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
    /// Mint a list in the CURRENT ambient region and charge its
    /// initial elements to that region's ledger (s131, #187): the
    /// checked machine's container twin of the native birth-region
    /// rule. The model's unit is 16 bytes per element slot — the
    /// `charge_mem` unit this machine already uses — so the pinned
    /// relations (`[mem.region.account.1]`) hold without pretending
    /// the two tiers share a layout.
    fn mint_list(&mut self, items: Vec<Value>, span: Span) -> E<usize> {
        let rid = self.ambient.last().copied().unwrap_or(0);
        let bytes: u64 = items.iter().map(slot_bytes).sum();
        self.charge_region_bytes(rid, bytes, span)?;
        let id = self.lists.len();
        self.lists.push(items);
        self.list_region.push(rid);
        Ok(id)
    }

    /// `Map[K, V]()` (s152): an empty map born in the ambient region;
    /// each insert charges the birth region one key slot and one
    /// value slot, the `push` rule applied to a keyed store.
    fn mint_map(&mut self, span: Span) -> E<usize> {
        let rid = self.ambient.last().copied().unwrap_or(0);
        self.charge_region_bytes(rid, 0, span)?;
        let id = self.maps.len();
        self.maps.push(Vec::new());
        self.map_region.push(rid);
        Ok(id)
    }

    /// `m[k]` as a READ (`[mem.map.absent]`): the bound value, or the
    /// payload-free `none` row. Never a trap, never `()`.
    fn map_lookup(&self, id: usize, key: &MapKey) -> Flow {
        match self.maps[id].iter().find(|(k, _)| k == key) {
            Some((_, v)) => Flow::Val(v.clone()),
            None => raise(Value::ErrTag {
                tag: "none".to_string(),
                payload: Vec::new(),
            }),
        }
    }

    /// `m[k] = v` (`[mem.map.absent]`): replace the bound value, or
    /// append a fresh entry — insertion order is the iteration order
    /// `pairs()` reports.
    fn map_insert(&mut self, id: usize, key: MapKey, v: Value, span: Span) -> E<()> {
        if let Some(slot) = self.maps[id].iter_mut().find(|(k, _)| *k == key) {
            slot.1 = v;
            return Ok(());
        }
        let bytes = slot_bytes(&key.value()) + slot_bytes(&v);
        self.charge_mem(bytes)?;
        let rid = self.map_region.get(id).copied().unwrap_or(0);
        self.charge_region_bytes(rid, bytes, span)?;
        self.maps[id].push((key, v));
        Ok(())
    }

    /// s77's byte VIEW on this tier (s136, wolf-lang#232; s153,
    /// wolf-lang#308): `<str>.bytes()` in a position that consumes it
    /// on the spot — iterated, indexed, asked for
    /// `len`/`count`/`is_empty`/`get`/`first`/`last` — is the
    /// receiver's own `{ptr, len}` and allocates nothing
    /// (`[mem.str.view]`). The machine hands the consumer the
    /// receiver's OCTETS and mints nothing: no list, no ledger charge,
    /// nothing retained past the expression — what native and lupin
    /// do, and what the ledger has said since s136. Between s136 and
    /// s153 the ledger said zero while the machine still materialized
    /// a `Vec<Value>` per call and pushed it onto `lists` for the rest
    /// of the run — uncharged AND retained, so a walk re-reading
    /// `rest.bytes()[0]` over a shrinking `rest` held n + (n-1) + …
    /// values: 1.9 GiB at n = 8192, 16.6 GiB on wolf-std's CAVP rows,
    /// under a step budget that could not see it (#308). This is the
    /// invariant `[exec.checked.budget]` states: a view retains nothing
    /// on the host that the ledger does not charge.
    ///
    /// `Some(octets)` when `e` is that call (the receiver read as a
    /// place or evaluated); `None` for every other expression — the
    /// caller evaluates as before, and `let bs = s.bytes()` still
    /// materializes through the `"bytes"` method arm, charged.
    fn eval_bytes_view(&mut self, e: &'t GreenNode) -> E<Found<Vec<u8>>> {
        let src = &self.pkg.files[self.ctx().src_file].raw.src;
        let Some(recv) = crate::byteview::view_recv(e, src, &|sp| self.expr_ty(sp).cloned()) else {
            return Ok(Found::Not);
        };
        // A receiver whose operand leaves early (`ss[i()?].bytes()`)
        // hands its flow back: answering "no view" made the caller
        // evaluate the receiver again (wolf-lang#481).
        let sv = match self.place_of(recv)? {
            Found::At(place) => self.read_place(&place, recv.span)?,
            Found::Flow(f) => return Ok(Found::Flow(f)),
            Found::Not => match self.eval(recv)? {
                Flow::Val(v) => v,
                f => return Ok(Found::Flow(f)),
            },
        };
        let Value::Str(s) = sv else {
            return Ok(Found::Not);
        };
        Ok(Found::At(s.into_bytes()))
    }

    /// A fresh `str` built by `+`/`+=` or by an interpolation with a
    /// hole (s153, wolf-lang#308/#310): an allocation in the ambient
    /// region (`[mem.region.escape]`, `[type.str.concat.cost]`),
    /// charged one byte per byte to the shadow budget and to that
    /// region's ledger — what the native strbuf path pays. Until s153
    /// neither ledger moved for a built `str`, so the byte budget was
    /// blind to `s = s + s` the way it was blind to the retained view.
    /// A literal, a slice and every `[mem.str.view]` product allocate
    /// nothing and stay uncharged.
    fn mint_str(&mut self, s: String, span: Span) -> E<Value> {
        self.charge_str(s.len() as u64, span)?;
        Ok(Value::Str(s))
    }

    /// The charge half of `mint_str`, callable BEFORE the bytes are
    /// built: `+` knows its result's length from its operands, so the
    /// refusal for a build the budget will not admit arrives before
    /// the host allocates it (the doubling witness in the driver's
    /// `checked_budget` test peaks at the operands, not at the
    /// refused result). An interpolation charges after assembling —
    /// one transient string, bounded by its own holes.
    fn charge_str(&mut self, bytes: u64, span: Span) -> E<()> {
        self.charge_mem(bytes)?;
        let rid = self.ambient.last().copied().unwrap_or(0);
        self.charge_region_bytes(rid, bytes, span)
    }

    /// A region's `cap:` budget, evaluated at creation (s132,
    /// `[mem.region.cap.1]`): an `int` byte count. Negative is the
    /// allocation-contract violation at the CREATING site
    /// (`[mem.region.cap.2]` — the compiled tiers' `region_set_cap`
    /// makes the same check).
    fn eval_region_cap(
        &mut self,
        cap: Option<wolf_ast::RegionCap<'t>>,
    ) -> E<Result<Option<u64>, Flow>> {
        let Some(cap) = cap else { return Ok(Ok(None)) };
        let Some(v) = cap.value() else {
            return Ok(Ok(None));
        };
        let span = v.span;
        match self.eval(v)? {
            Flow::Val(Value::Int(n)) => {
                if n < 0 {
                    return self.trap("alloc-contract", "mem.region.cap.2", span);
                }
                Ok(Ok(Some(n as u64)))
            }
            Flow::Val(_) => self.refuse("a non-int region cap", span),
            // Control flow out of a cap expr (a `?` error, a return)
            // — surfaces unchanged, before the region exists.
            other => Ok(Err(other)),
        }
    }

    /// Add to a region's cumulative ledger (never subtracted: the
    /// charge is monotone within the region's lifetime, the native
    /// arena's own rule) — and compare it against the cap
    /// (`[mem.region.cap.1]`, s132): a charge that takes the ledger
    /// PAST the budget is `trap(alloc-contract)` at the charging
    /// site; landing exactly at the cap is not a breach.
    fn charge_region_bytes(&mut self, rid: usize, bytes: u64, span: Span) -> E<()> {
        let mut breach = false;
        if let Some(r) = self.regions.get_mut(rid) {
            r.charged = r.charged.saturating_add(bytes);
            breach = r.cap.is_some_and(|cap| r.charged > cap);
        }
        if breach {
            return self.trap("alloc-contract", "mem.region.cap.1", span);
        }
        Ok(())
    }

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
        // The region's own ledger (s131, #187): attribution follows
        // the allocation's region, the way the native arena charges
        // the region that was ambient at the allocation site — and
        // the cap compare rides the charge (s132, [mem.region.cap.1]).
        self.charge_region_bytes(region, size, span)?;
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

    /// kw07 (`[mem.unsafe.volatile]`): `p.read_volatile()` (`value`
    /// `None`) or `p.write_volatile(v)`. On an allocation it is an
    /// ordinary access of the pointee's width at offset 0 — the same
    /// row-ordered check `p[0]` gets (`[mem.unsafe.volatile.3]`) —
    /// after one more: the address must be a multiple of the width
    /// (row L3; allocations are placed at `ALLOC_STRIDE` multiples, so
    /// the address alone decides it). Signed pointees read back
    /// sign-extended (kw06's [`Self::raw_read_at`]), `byte` as a byte.
    fn volatile_access(
        &mut self,
        p: PtrVal,
        pointee: Prim,
        value: Option<Value>,
        span: Span,
    ) -> E<Flow> {
        let write = value.is_some();
        let opdesc = if write {
            "a volatile write"
        } else {
            "a volatile read"
        };
        let size = prim_size(pointee);
        if p.alloc.is_some() && !p.addr.is_multiple_of(size) {
            let tag_span = p.alloc.map(|a| self.allocs[a].span).unwrap_or(span);
            return self.ub(
                UbRow::L3,
                format!(
                    "{opdesc} of {size} bytes at address {:#x}, which is not a multiple of {size}",
                    p.addr
                ),
                span,
                tag_span,
            );
        }
        // The access itself is kw06's raw read/write at `p` — the same
        // rows, the same sign extension (#561) — with `byte` carried as
        // a byte both ways.
        let signed = matches!(pointee, Prim::I8 | Prim::I16 | Prim::I32 | Prim::I64);
        match value {
            Some(v) => {
                let v = match v {
                    Value::Byte(b) => Value::Int(i64::from(b)),
                    other => other,
                };
                self.raw_write_at(p, size, signed, v, None, span, span)
            }
            None if pointee == Prim::Byte => {
                let bytes = self.raw_read_bytes(p, size, span, opdesc)?;
                Ok(Flow::Val(Value::Byte(bytes[0])))
            }
            None => self.raw_read_at(p, size, signed, span),
        }
    }

    /// kw11 (`[conc.mm.atomic.raw.5]`): an atomic operation on an
    /// allocation, run as `seq_cst` whatever its order — this machine
    /// interleaves whole operations, so every outcome it produces is
    /// one the hardware may produce, and the weaker-order outcomes it
    /// cannot produce are named in the clause as outside its reach. The
    /// access is kw06's raw read/write at `p` (rows P1–P4, L1, L2, T1;
    /// a misaligned address is row L4, `[conc.mm.atomic.raw.4]`); a
    /// read-modify-write reads, computes and writes in one step, its
    /// arithmetic wrapping at the width (`[conc.mm.atomic.raw]`), and
    /// yields the old value; a CAS writes only on a match and yields
    /// `(old, matched)`.
    fn atomic_access(
        &mut self,
        op: wolf_ast::atomic::AtomicOp,
        p: PtrVal,
        pointee: Prim,
        args: &[Value],
        span: Span,
    ) -> E<Flow> {
        use wolf_ast::atomic::AtomicOp;
        let size = prim_size(pointee);
        let signed = matches!(pointee, Prim::I8 | Prim::I16 | Prim::I32 | Prim::I64);
        let int = |v: Option<&Value>| match v {
            Some(Value::Int(n)) => Some(*n),
            _ => None,
        };
        if op == AtomicOp::Store {
            let Some(v) = args.first().cloned() else {
                return self.refuse("an atomic store without its value", span);
            };
            return self.raw_write_at(p, size, signed, v, None, span, span);
        }
        let Flow::Val(Value::Int(old)) = self.raw_read_at(p, size, signed, span)? else {
            return self.refuse("an atomic read of a non-integer", span);
        };
        let mask: u64 = if size >= 8 {
            u64::MAX
        } else {
            (1u64 << (8 * size)) - 1
        };
        let new = match op {
            AtomicOp::Load => return Ok(Flow::Val(Value::Int(old))),
            AtomicOp::Cas => {
                let (Some(expected), Some(new)) = (int(args.first()), int(args.get(1))) else {
                    return self.refuse("a compare-and-swap without its operands", span);
                };
                let matched = (old as u64 ^ expected as u64) & mask == 0;
                if matched {
                    self.raw_write_at(p, size, signed, Value::Int(new), None, span, span)?;
                }
                return Ok(Flow::Val(Value::Struct {
                    fields: vec![
                        ("0".to_string(), Value::Int(old)),
                        ("1".to_string(), Value::Bool(matched)),
                    ],
                }));
            }
            _ => {
                let Some(v) = int(args.first()) else {
                    return self.refuse("an atomic operation without its value", span);
                };
                match op {
                    AtomicOp::Swap => v,
                    AtomicOp::Add => old.wrapping_add(v),
                    AtomicOp::Sub => old.wrapping_sub(v),
                    AtomicOp::And => old & v,
                    AtomicOp::Or => old | v,
                    _ => old ^ v,
                }
            }
        };
        // raw_write_at stores the low `size` bytes: the wrap at the
        // width is the truncation.
        self.raw_write_at(p, size, signed, Value::Int(new), None, span, span)?;
        Ok(Flow::Val(Value::Int(old)))
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

/// The assembly roster (kw05, `[abi.asm.machines]`): the routines the
/// program's `wolf.pkg` lists under `asm`, so a call into one is refused
/// naming assembly rather than C. Process-wide, set once by the driver
/// before a run; the refusal is the same verdict either way — only the
/// construct's name differs.
static ASM_ROSTER: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// Name the assembly routines for later runs ([`ASM_ROSTER`]).
pub fn set_asm_roster(names: Vec<String>) {
    if let Ok(mut r) = ASM_ROSTER.write() {
        *r = names;
    }
}

fn on_asm_roster(name: &str) -> bool {
    ASM_ROSTER
        .read()
        .map(|r| r.iter().any(|n| n == name))
        .unwrap_or(false)
}

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
    // #382: the machine runs on a thread whose stack it sizes itself,
    // never on its caller's. The call-depth budget is a depth the
    // machine ALLOWS, so reaching it must answer `unsupported` on every
    // host; on the caller's stack it answered on a unix main thread
    // (8 MiB) and overflowed a windows one (1 MiB), where three
    // wolf-std json rows reach `CALL_DEPTH_BUDGET` with ~1.2 MiB of
    // host frames.
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("wolf-checked".into())
            .stack_size(CHECKED_STACK_BYTES)
            .spawn_scoped(scope, || run_checked_fn_here(pkg, tc, budget, stdin, entry));
        match worker {
            Ok(handle) => match handle.join() {
                Ok(out) => out,
                Err(panic) => std::panic::resume_unwind(panic),
            },
            // No thread to be had (a host at its thread limit): run
            // where we stand, which is exactly the old behavior.
            Err(_) => run_checked_fn_here(pkg, tc, budget, stdin, entry),
        }
    })
}

/// The deepest call chain the machine executes (`[exec.checked.budget]`):
/// a call past it is `unsupported — call depth budget exhausted`.
pub const CALL_DEPTH_BUDGET: usize = 128;

/// The checked machine's own stack (#382, `[exec.checked.budget]`), the
/// same on EVERY host, so the call-depth budget answers before the host
/// stack runs out. Measured at 0.2.14 (x86-64 linux): the deepest
/// wolf-std rows need ~1.23 MiB at `CALL_DEPTH_BUDGET` in a release
/// build, and a 128-deep recursion needs more than 8 MiB (and no more
/// than 16) in a debug one. Reserved address space; resident memory is
/// only the frames a program reaches.
pub const CHECKED_STACK_BYTES: usize = 64 << 20;

fn run_checked_fn_here(
    pkg: &Package,
    tc: &Typecheck,
    budget: Budget,
    stdin: &str,
    entry: &str,
) -> Result<RunOutcome, NotYet> {
    let root_span = pkg.files[0].parse.root.span;
    // kw09 (`[abi.link.section]`, `[abi.link.extern]`): this machine
    // has no image and no link, so a program that places a section or
    // names a link-time symbol is refused by name — never run as if the
    // attribute or the symbol were absent.
    if let Some(nyc) = link_refusal(pkg) {
        return Err(nyc);
    }
    // kw09 (`[mem.static.3]`): module state of a type that is not
    // static data yet has no value here, read or not — refused by
    // name, as both compiling tiers refuse it.
    for module in &tc.sigs.modules {
        for sig in module.values() {
            if let ItemSig::Global(g) = sig
                && g.kind != wolf_sema::GlobalKind::Extern
                && g.ty
                    .is_some_and(|t| !wolf_sema::is_static_data(&tc.sigs.table, t, g.kind))
            {
                return Err(NotYet {
                    construct: "module state of a type that is not static data ([mem.static.3])",
                    span: g.name_span,
                });
            }
        }
    }
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
            construct: "a test entry with parameters (only zero-parameter tests run)",
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
                    m.stdout
                        .extend_from_slice(format!("error: {tag}\n").as_bytes());
                    1
                }
                _ => 0,
            };
            Ok(RunOutcome {
                verdict: Verdict::Exit(code),
                stdout: String::from_utf8_lossy(&m.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&m.stderr).into_owned(),
            })
        }
        Err(Stop::Trap(t)) => Ok(RunOutcome {
            verdict: Verdict::Trap(t),
            stdout: String::from_utf8_lossy(&m.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&m.stderr).into_owned(),
        }),
        Err(Stop::Ub(f)) => Ok(RunOutcome {
            verdict: Verdict::Ub(f),
            stdout: String::from_utf8_lossy(&m.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&m.stderr).into_owned(),
        }),
        Err(Stop::Refuse(nyc)) => Err(nyc),
        Err(Stop::Budget(what)) => Err(NotYet {
            construct: what,
            span: root_span,
        }),
        // `os_exit` (s40): everything printed so far stands; the code
        // is the verdict.
        Err(Stop::Exit(code)) => Ok(RunOutcome {
            verdict: Verdict::Exit(code),
            stdout: String::from_utf8_lossy(&m.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&m.stderr).into_owned(),
        }),
    }
}

/// kw09: the first `#[section]` or `extern "c" let` in the package,
/// as the refusal the checked machine answers (see
/// [`run_checked_fn_here`]).
fn link_refusal(pkg: &Package) -> Option<NotYet> {
    fn walk(node: &GreenNode, src: &[u8]) -> Option<NotYet> {
        for child in node.nodes() {
            if matches!(
                child.kind,
                SyntaxKind::FnDecl | SyntaxKind::LetDecl | SyntaxKind::VarDecl
            ) && wolf_sema::attrs::has_section(child, src)
            {
                return Some(NotYet {
                    construct: "section placement",
                    span: child.span,
                });
            }
            if wolf_ast::is_extern_binding(child) {
                return Some(NotYet {
                    construct: "a link-time symbol (`extern \"c\" let`)",
                    span: child.span,
                });
            }
            if let Some(n) = walk(child, src) {
                return Some(n);
            }
        }
        None
    }
    pkg.files
        .iter()
        .find_map(|f| walk(&f.parse.root, &f.raw.src))
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
            trait_methods: HashMap::new(),
            trait_defaults: HashMap::new(),
            self_tys: Vec::new(),
            pending_self_ty: None,
            frame_rigids: Vec::new(),
            pending_rigids: None,
            allocs: Vec::new(),
            regions: Vec::new(),
            lists: Vec::new(),
            list_region: Vec::new(),
            maps: Vec::new(),
            map_region: Vec::new(),
            pools: Vec::new(),
            chans: Vec::new(),
            cells: Vec::new(),
            frames: Vec::new(),
            ambient: Vec::new(),
            stdout: Vec::new(),
            stderr: Vec::new(),
            last_os_error: 0,
            stdin: String::new(),
            stdin_pos: 0,
            files: Vec::new(),
            socks: Vec::new(),
            sock_deadlines: HashMap::new(),
            children: Vec::new(),
            signal_listening: 0,
            signal_queue: std::collections::VecDeque::new(),
            args: Vec::new(),
            env_overlay: HashMap::new(),
            cwd: None,
            t0: std::time::Instant::now(),
            steps: 0,
            mem_used: 0,
            budget: Budget::default(),
            in_defer: false,
            statics: Vec::new(),
            static_slots: HashMap::new(),
        };
        // kw09 (`[mem.static.3]`): module state starts at the value
        // the comptime engine computed; a `byte` item is a byte.
        for ((module, name), fold) in &tc.statics {
            let Some(ItemSig::Global(g)) = tc.sigs.get(*module, name) else {
                continue;
            };
            let is_byte =
                g.ty.is_some_and(|t| matches!(tc.sigs.table.kind(t), TyKind::Prim(Prim::Byte)));
            let v = match fold {
                Fold::Unit => Value::Unit,
                Fold::Bool(b) => Value::Bool(*b),
                Fold::Int(n) if is_byte => Value::Byte(*n as u8),
                Fold::Int(n) => Value::Int(*n as i64),
                Fold::Float(f) => Value::F64(*f),
                Fold::Str(st) => Value::Str(st.clone()),
            };
            m.static_slots
                .insert((*module, name.clone()), m.statics.len());
            m.statics.push(v);
        }
        // The run's root region: `main`'s caller (never freed, never
        // capped — the process root is outside `[mem.region.cap.1]`).
        m.regions.push(DynRegion {
            live: true,
            frozen: false,
            backing: None,
            span: pkg.files[0].parse.root.span,
            charged: 0,
            cap: None,
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
                        let d = wolf_ast::ImplDecl::cast(o);
                        let target = d.and_then(|d| {
                            d.self_ty()
                                .map(|t| t.span)
                                .or_else(|| d.trait_path().map(|p| p.syntax().span))
                        });
                        if let Some(span) = target {
                            let src = &pkg.files[b.file].raw.src;
                            let ty =
                                String::from_utf8_lossy(&src[span.lo as usize..span.hi as usize])
                                    .into_owned();
                            // A TRAIT impl (`impl T for V`) also keys
                            // (self, trait, method) so two traits'
                            // same-named methods stay apart (#12).
                            if let Some((tspan, _)) = d.and_then(|d| {
                                d.trait_path().map(|p| p.syntax().span).zip(d.self_ty())
                            }) {
                                let tr = String::from_utf8_lossy(
                                    &src[tspan.lo as usize..tspan.hi as usize],
                                )
                                .into_owned();
                                let tr = tr.rsplit('.').next().unwrap_or(tr.as_str()).to_string();
                                // ONLY the trait index: letting a
                                // trait impl into the inherent index
                                // let its `speak` overwrite `impl
                                // Dog`'s own, and `d.speak()` answered
                                // the trait (ty.method.order says
                                // inherent wins; method_inherent.lu
                                // is the witness).
                                m.trait_methods.insert((ty, tr, b.name.clone()), i);
                            } else {
                                m.methods.insert((ty, b.name.clone()), i);
                            }
                        }
                    }
                    Some(o) if o.kind == SyntaxKind::TraitDecl => {
                        // A default body: the trait's own method with
                        // a block, executed for any impl that does not
                        // override (s95's `Self ↦ subject`, checked).
                        if let Some(tok) = wolf_ast::TraitDecl::cast(o).and_then(|t| t.name()) {
                            let src = &pkg.files[b.file].raw.src;
                            let tr = String::from_utf8_lossy(
                                &src[tok.span.lo as usize..tok.span.hi as usize],
                            )
                            .into_owned();
                            m.trait_defaults.insert((b.module, tr, b.name.clone()), i);
                        }
                    }
                    _ => {}
                }
            }
            let ctx = Ctx {
                tb,
                node,
                calls: tb.calls.iter().map(|(s, c)| (*s, c)).collect(),
                folds: tb.comptime_folds.iter().map(|(s, f)| (*s, f)).collect(),
                casts: tb
                    .casts
                    .iter()
                    .map(|(s, a, b2, k)| (*s, (*a, *b2, *k)))
                    .collect(),
                expr_tys: tb.exprs.iter().map(|(s, t)| (*s, *t)).collect(),
                dispatch: tb.dispatch.iter().map(|(s, d)| (*s, d)).collect(),
                src_file: b.file,
                unit_discards: tb.unit_discards.iter().copied().collect(),
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
        if self.frames.len() > CALL_DEPTH_BUDGET {
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
        self.self_tys.push(self.pending_self_ty.take());
        self.frame_rigids
            .push(self.pending_rigids.take().unwrap_or_default());
        let result = self.eval_block(block, true);
        let out = match result {
            Ok(Flow::Val(v)) => self.exit_scopes_to(0, false).map(|()| v),
            Ok(Flow::Return(v)) => self.exit_scopes_to(0, false).map(|()| v),
            // A raise leaving the body (s15/s37): errdefers run, the
            // tag value crosses to the caller — the call site rewraps
            // it as `Flow::Err` (or `main` reports it, D30's process
            // behavior).
            Ok(Flow::Err(v, _)) => self.exit_scopes_to(0, true).map(|()| v),
            Ok(Flow::Break) | Ok(Flow::Continue) => Ok(Value::Unit),
            Err(e) => Err(e),
        };
        self.frames.pop();
        self.self_tys.pop();
        self.frame_rigids.pop();
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

    /// Resolve an lvalue-shaped expression to a place. `Not`: not a
    /// place (temporary, item reference). `Flow`: an index operand left
    /// early (wolf-lang#481) — the caller returns it; it never
    /// evaluates the expression again.
    fn place_of(&mut self, e: &'t GreenNode) -> E<Found<Place>> {
        match e.kind {
            SyntaxKind::PathExpr => {
                let name = self.text(e.span);
                if name.contains('.') || name.contains("::") {
                    return Ok(Found::Not);
                }
                match self.lookup(&name) {
                    Some((frame, local)) => Ok(Found::At(Place {
                        frame,
                        local,
                        path: Vec::new(),
                    })),
                    None => self.static_place(&name, e.span),
                }
            }
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.place_of(inner),
                None => Ok(Found::Not),
            },
            SyntaxKind::MemberExpr => {
                let m = MemberExpr::cast(e).expect("kind");
                let Some(base) = m.base() else {
                    return Ok(Found::Not);
                };
                // `(mut recv)` in receiver position unwraps.
                let base = match ParenExpr::cast(base) {
                    Some(p) if p.mode().is_some() => p.expr().unwrap_or(base),
                    _ => base,
                };
                let Some(member) = m.member() else {
                    return Ok(Found::Not);
                };
                let field = self.text(member.span);
                // s213 (wolf-lang#579, `[mem.static]`): `m.K` — another
                // module's item read (or, a `var`, written) through its
                // module's name is that item's state.
                if let Some(module) = self.bound_module(base)
                    && let Some(ItemSig::Global(_)) = self.tc.sigs.get(module, &field)
                {
                    return self.static_place_in(module, &field, e.span);
                }
                match self.place_of(base)? {
                    Found::At(mut place) => {
                        place.path.push(PStep::Field(field));
                        Ok(Found::At(place))
                    }
                    other => Ok(other),
                }
            }
            SyntaxKind::BracketApply => {
                let b = BracketApply::cast(e).expect("kind");
                let Some(recv) = b.callee() else {
                    return Ok(Found::Not);
                };
                // Raw-pointer indexing is never a place — the raw
                // tier owns it.
                if matches!(self.expr_ty(recv.span), Some(TyKind::Ptr(_))) {
                    return Ok(Found::Not);
                }
                // A slice (`xs[a..b]`, `s[a..b]`) is never a place: it
                // builds a value of its own. Decide that from the
                // syntax, as `eval` does, BEFORE any operand runs —
                // evaluating the endpoints here and then answering
                // "not a place" made every caller's `eval` fallback run
                // them again (wolf-lang#479: `xs[a()..2].len` ran `a`
                // twice, `"{xs[a()..2].len}"` three times).
                if b.args()
                    .into_iter()
                    .flat_map(|l| l.args())
                    .filter_map(Arg::value)
                    .any(|v| v.kind == SyntaxKind::RangeExpr)
                {
                    return Ok(Found::Not);
                }
                // The receiver's own operands run first (outermost
                // first); a flow among them is this place's flow.
                let base = match self.place_of(recv)? {
                    Found::At(p) => p,
                    other => return Ok(other),
                };
                let mut idx_val: Option<Value> = None;
                for a in b.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a)
                        && wolf_ast::is_expr_kind(v.kind)
                    {
                        // wolf-lang#481: an operand that leaves early
                        // (`xs[idx()?]`, a `return` or `break` inside
                        // it) has RUN; its flow is handed back.
                        // Answering "not a place" here made every
                        // caller's `eval` fallback run it again.
                        idx_val = Some(match self.eval(v)? {
                            Flow::Val(x) => x,
                            f => return Ok(Found::Flow(f)),
                        });
                    }
                }
                let mut place = base;
                // s152: a `Map` receiver keys the place, whatever the
                // key's value shape (an `int` key is not a list index).
                if matches!(self.expr_ty(recv.span), Some(TyKind::Map(..))) {
                    let Some(key) = idx_val.as_ref().and_then(MapKey::of) else {
                        return Ok(Found::Not);
                    };
                    place.path.push(PStep::MapKey { key, span: e.span });
                    return Ok(Found::At(place));
                }
                match idx_val {
                    Some(Value::Int(i)) => {
                        // The origin shift (D61) — the place spelling
                        // shifts exactly as the read form.
                        let i = if self.origin_at(e.span) == 1 {
                            self.shift_origin(i, e.span)?
                        } else {
                            i
                        };
                        place.path.push(PStep::ListIdx {
                            index: i,
                            span: e.span,
                        })
                    }
                    Some(Value::Handle { index, generation }) => place.path.push(PStep::PoolIdx {
                        index,
                        generation,
                        span: e.span,
                    }),
                    _ => return Ok(Found::Not),
                }
                Ok(Found::At(place))
            }
            _ => Ok(Found::Not),
        }
    }

    /// kw09 (`[mem.static]`): a bare name no local binds, read as the
    /// current module's state. `Found::Not` when the name is no module
    /// item (a fn value, a tag — the callers' other readings); a module
    /// item with no static value is refused by name, as is a link-time
    /// symbol, which this machine has no link to resolve.
    fn static_place(&mut self, name: &str, span: Span) -> E<Found<Place>> {
        let Some(frame) = self.frames.last() else {
            return Ok(Found::Not);
        };
        let module = self.tc.bodies[frame.body].body.module;
        self.static_place_in(module, name, span)
    }

    /// s213 (wolf-lang#579): the module an unshadowed bare name `m`
    /// binds in the current file (`use m`), if any.
    fn bound_module(&self, base: &'t GreenNode) -> Option<usize> {
        if base.kind != SyntaxKind::PathExpr {
            return None;
        }
        let bname = self.text(base.span);
        if bname.contains('.') || self.lookup(&bname).is_some() {
            return None;
        }
        let cur = &self.tc.bodies[self.frames.last()?.body].body;
        let md = &self.pkg.modules[cur.module];
        md.files
            .iter()
            .position(|&f| f == cur.file)
            .and_then(|slot| md.bindings[slot].iter().find(|b| b.name == bname))
            .and_then(|b| match b.target {
                wolf_sema::BindTarget::PkgModule(m) => Some(m),
                _ => None,
            })
    }

    /// [`Machine::static_place`] for `module`'s state — the frame's own
    /// module, or (s213, wolf-lang#579) the one a qualified `m.K` names.
    fn static_place_in(&mut self, module: usize, name: &str, span: Span) -> E<Found<Place>> {
        let Some(ItemSig::Global(g)) = self.tc.sigs.get(module, name) else {
            return Ok(Found::Not);
        };
        if g.kind == wolf_sema::GlobalKind::Extern {
            return self.refuse("a link-time symbol (`extern \"c\" let`)", span);
        }
        match self.static_slots.get(&(module, name.to_string())) {
            Some(&local) => Ok(Found::At(Place {
                frame: STATIC_FRAME,
                local,
                path: Vec::new(),
            })),
            None => self.refuse(
                "module state whose value is not static data ([mem.static.3])",
                span,
            ),
        }
    }

    /// Read through a place (bounds and generation checks fire here).
    fn read_place(&mut self, place: &Place, span: Span) -> E<Value> {
        if place.frame == STATIC_FRAME {
            let root = self.statics[place.local].clone();
            return self.walk_read(root, &place.path, span);
        }
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
            // s158 (`[type.range.accessor]`): a range's two endpoints.
            // `end` is exclusive here because it is exclusive
            // everywhere — this machine has normalized `a..=b` at
            // construction since its first range arm, which is the
            // clause's own argument for the rule.
            (PStep::Field(f), Value::Range { start, end, chars }) if f == "start" || f == "end" => {
                let v = if f == "start" { start } else { end };
                let v = match (chars, u32::try_from(v).ok().and_then(char::from_u32)) {
                    (true, Some(c)) => Value::Char(c),
                    _ => Value::Int(v),
                };
                self.walk_read(v, rest, span)
            }
            (PStep::Field(f), Value::Map(id)) if f == "len" => {
                let n = self.maps[id].len() as i64;
                self.walk_read(Value::Int(n), rest, span)
            }
            // A `Map` entry through a place PATH — an interpolation
            // hole reads `{m[k]}` as a place so the map is not moved
            // (`[type.interp.union]` renders the row when the key is
            // absent), and `m[k].field` walks on through the bound
            // value. An absent key at the end of the path is the
            // `none` row VALUE (#122's binding rule: a raw row value
            // reads like the err flow); deeper, nothing can answer.
            (PStep::MapKey { key, .. }, Value::Map(id)) => {
                match self.maps[id].iter().find(|(k, _)| k == key) {
                    Some((_, v)) => {
                        let v = v.clone();
                        self.walk_read(v, rest, span)
                    }
                    None if rest.is_empty() => Ok(Value::ErrTag {
                        tag: "none".to_string(),
                        payload: Vec::new(),
                    }),
                    None => self.refuse("reading through an absent Map entry's field", span),
                }
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
        if place.frame == STATIC_FRAME {
            // Module state holds scalars only (`[mem.static.3]`), so a
            // write is the whole slot.
            if !place.path.is_empty() {
                return self.refuse("a write into part of module state", span);
            }
            self.statics[place.local] = v;
            return Ok(());
        }
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
            // s152 (`[mem.map.absent]`): the whole-entry write inserts
            // or replaces; a deeper path patches a bound entry.
            (PStep::MapKey { key, span: ksp }, Value::Map(id)) => {
                let id = *id;
                let (key, ksp) = (key.clone(), *ksp);
                if rest.is_empty() {
                    return self.map_insert(id, key, v, ksp);
                }
                let Some(pos) = self.maps[id].iter().position(|(k, _)| *k == key) else {
                    return self.refuse("writing through an absent Map entry's field", span);
                };
                let mut elem = std::mem::replace(&mut self.maps[id][pos].1, Value::Moved);
                let r = self.patch(&mut elem, rest, v, span);
                self.maps[id][pos].1 = elem;
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
        if place.frame == STATIC_FRAME {
            // Module state is scalar: every read copies.
            return Ok(v);
        }
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
        let stmts: Vec<&'t GreenNode> = b.statements().collect();
        let last = stmts.len().saturating_sub(1);
        for (i, stmt) in stmts.into_iter().enumerate() {
            self.tick()?;
            match stmt.kind {
                SyntaxKind::ExprStmt => {
                    let d = ExprStmt::cast(stmt).expect("kind");
                    if let Some(e) = d.expr() {
                        // s207 (wolf-lang#541): a non-trailing `!T`
                        // statement is W0601's other half — consumed
                        // by no one, so its raw row is discarded and
                        // the block goes on.
                        let discarded =
                            i != last && matches!(self.expr_ty(e.span), Some(TyKind::ErrUnion(..)));
                        match self.eval(e)? {
                            Flow::Val(v) => {
                                if Some(e.span) == last_value {
                                    out = v;
                                }
                            }
                            Flow::Err(_, false) if discarded => {}
                            other => {
                                self.close_scope(matches!(other, Flow::Err(..)))?;
                                return Ok(other);
                            }
                        }
                    }
                }
                SyntaxKind::LetDecl | SyntaxKind::VarDecl => {
                    // A comma group binds in sequence, left to right
                    // (D63).
                    for b in wolf_ast::binding_binders(stmt) {
                        match self.bind_decl(b.pattern, b.init)? {
                            Flow::Val(_) => {}
                            other => {
                                self.close_scope(matches!(other, Flow::Err(..)))?;
                                return Ok(other);
                            }
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
                            self.close_scope(matches!(other, Flow::Err(..)))?;
                            return Ok(other);
                        }
                    }
                }
                SyntaxKind::AssignStmt => match self.eval_assign(stmt)? {
                    Flow::Val(_) => {}
                    other => {
                        self.close_scope(matches!(other, Flow::Err(..)))?;
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
                        self.close_scope(matches!(other, Flow::Err(..)))?;
                        return Ok(other);
                    }
                },
                // #116b: nested named fns lift on the native tiers;
                // the checked executor still refuses the closure
                // family by name (#12), and a nested fn is a named
                // capture-free closure.
                SyntaxKind::FnDecl => {
                    return self.refuse("a nested fn in checked execution", stmt.span);
                }
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
        // s128 (#173): a tuple pattern over a PLACE moves each bound
        // element out of its own sub-place (partial moves — the static
        // tier's element story, mirrored dynamically); `_` leaves its
        // element untouched, so the source stays element-wise live.
        if let (Some(pat), Some(e)) = (pat, init)
            && pat.kind == SyntaxKind::TuplePat
            && let Some(base) = found!(self.place_of(e))
        {
            self.bind_tuple_from_place(pat, &base)?;
            return Ok(Flow::Val(Value::Unit));
        }
        // s129 (#179): a struct pattern over a PLACE — the tuple
        // discipline over NAMED fields: each named field is taken
        // from `base.name` (copy or partial move); omission and `..`
        // leave the source field-wise live.
        if let (Some(pat), Some(e)) = (pat, init)
            && pat.kind == SyntaxKind::StructPat
            && let Some(base) = found!(self.place_of(e))
        {
            self.bind_struct_from_place(pat, &base)?;
            return Ok(Flow::Val(Value::Unit));
        }
        let v = match init {
            Some(e) => match self.eval(e)? {
                Flow::Val(v) => v,
                // #122: a RAW row value at `let`/`var` BINDS — the
                // spec's reading (D52 declared-row-first; rows are
                // values), which native and lupin already implement.
                // Only a `?`-propagated error keeps unwinding.
                Flow::Err(v, false) => v,
                other => return Ok(other),
            },
            None => Value::Uninit,
        };
        if let Some(pat) = pat {
            self.bind_pattern(pat, v)?;
        }
        Ok(Flow::Val(Value::Unit))
    }

    /// `else |Tag(p)|` (s71, #43): destructure the caught error at
    /// the handler. Sema proved coverage (E0809), so a tag mismatch
    /// here is unreachable for checked programs; it stays a defensive
    /// refusal rather than a silent bind.
    fn bind_handler_path(&mut self, pat: &'t GreenNode, err: Value) -> E<()> {
        let Value::ErrTag { tag, payload } = err else {
            return self.refuse("a payload handler over this error value", pat.span);
        };
        let dotted = pat
            .nodes()
            .find(|n| n.kind == SyntaxKind::Path)
            .map(|p| self.text(p.span))
            .unwrap_or_default();
        let last = dotted.rsplit('.').next().unwrap_or(dotted.as_str());
        if tag != dotted && tag != last {
            return self.refuse("a handler tag the row did not prove", pat.span);
        }
        let subs: Vec<&GreenNode> = pat.nodes().filter(|n| is_pattern_kind(n.kind)).collect();
        for (k, sub) in subs.iter().enumerate() {
            let Some(v) = payload.get(k) else {
                return self.refuse("a payload the tag does not carry", sub.span);
            };
            match sub.kind {
                SyntaxKind::WildcardPat => {}
                SyntaxKind::IdentPat | SyntaxKind::BindingPat => {
                    self.bind_pattern(sub, v.clone())?;
                }
                _ => {
                    return self.refuse(
                        "nested handler payload patterns in checked execution",
                        sub.span,
                    );
                }
            }
        }
        Ok(())
    }

    /// Element-wise destructure of a tuple PLACE (s128, #173): each
    /// bound element is taken from `base.i` — a copy for Copy values,
    /// a partial move otherwise; wildcards touch nothing; nested tuple
    /// patterns recurse into deeper sub-places.
    fn bind_tuple_from_place(&mut self, pat: &'t GreenNode, base: &Place) -> E<()> {
        let subs: Vec<&'t GreenNode> = pat.nodes().filter(|n| is_pattern_kind(n.kind)).collect();
        for (i, sub) in subs.iter().enumerate() {
            if sub.kind == SyntaxKind::WildcardPat {
                continue;
            }
            let mut ep = base.clone();
            ep.path.push(PStep::Field(i.to_string()));
            if sub.kind == SyntaxKind::TuplePat {
                self.bind_tuple_from_place(sub, &ep)?;
                continue;
            }
            if sub.kind == SyntaxKind::StructPat {
                self.bind_struct_from_place(sub, &ep)?;
                continue;
            }
            let v = self.take_value(&ep, sub.span)?;
            self.bind_pattern(sub, v)?;
        }
        Ok(())
    }

    /// Field-wise destructure of a struct PLACE (s129, #179): each
    /// field the pattern names is taken from `base.name` — a copy for
    /// Copy values, a partial move otherwise; omitted fields and `..`
    /// touch nothing; nested tuple/struct patterns recurse.
    fn bind_struct_from_place(&mut self, pat: &'t GreenNode, base: &Place) -> E<()> {
        let Some(d) = wolf_ast::StructPat::cast(pat) else {
            return self.refuse("this pattern shape", pat.span);
        };
        for f in d.fields() {
            let Some(nspan) = f.name_span() else { continue };
            let fname = self.text(nspan);
            let Some(sub) = f.pattern() else { continue };
            if sub.kind == SyntaxKind::WildcardPat {
                continue;
            }
            let mut ep = base.clone();
            ep.path.push(PStep::Field(fname));
            if sub.kind == SyntaxKind::TuplePat {
                self.bind_tuple_from_place(sub, &ep)?;
                continue;
            }
            if sub.kind == SyntaxKind::StructPat {
                self.bind_struct_from_place(sub, &ep)?;
                continue;
            }
            let v = self.take_value(&ep, sub.span)?;
            self.bind_pattern(sub, v)?;
        }
        Ok(())
    }

    fn bind_pattern(&mut self, pat: &'t GreenNode, v: Value) -> E<()> {
        // s128 (#173): tuple patterns bind element-wise — `_`
        // discards, nested tuples recurse. Tuples arrive as the
        // positional Struct value the TupleExpr evaluator builds.
        if pat.kind == SyntaxKind::TuplePat {
            let subs: Vec<&'t GreenNode> =
                pat.nodes().filter(|n| is_pattern_kind(n.kind)).collect();
            let Value::Struct { fields } = v else {
                return self.refuse(
                    "a tuple pattern over a non-tuple value in checked execution",
                    pat.span,
                );
            };
            if fields.len() != subs.len() {
                return self.refuse(
                    "a tuple pattern with a mismatched arity in checked execution",
                    pat.span,
                );
            }
            for (sub, (_, ev)) in subs.iter().zip(fields) {
                self.bind_pattern(sub, ev)?;
            }
            return Ok(());
        }
        // s129 (#179): a struct pattern binds by FIELD NAME over the
        // named Struct value — shorthand and explicit alike; omitted
        // fields and `..` are simply not read.
        if pat.kind == SyntaxKind::StructPat {
            let Some(d) = wolf_ast::StructPat::cast(pat) else {
                return self.refuse("this pattern shape", pat.span);
            };
            let Value::Struct { fields } = v else {
                return self.refuse(
                    "a struct pattern over a non-struct value in checked execution",
                    pat.span,
                );
            };
            for f in d.fields() {
                let Some(nspan) = f.name_span() else { continue };
                let fname = self.text(nspan);
                let Some(sub) = f.pattern() else { continue };
                let Some((_, ev)) = fields.iter().find(|(n, _)| *n == fname) else {
                    // Sema owns the unknown-field report (E0403);
                    // reaching here is a defensive refusal.
                    return self.refuse("a field the struct does not carry", nspan);
                };
                self.bind_pattern(sub, ev.clone())?;
            }
            return Ok(());
        }
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
            // wolf-lang#452 (ruled 2026-09-26, `[mem.model.place.rhs]`):
            // the place's operands — pointer, then index — run before
            // the right-hand side; the byte write lands after it.
            let (p, idx) = self.raw_index_parts(place_expr)?;
            let v = match d.value() {
                Some(e) => val!(self.eval(e)),
                None => Value::Unit,
            };
            let op = d.op().map(|t| t.kind).filter(|_| compound);
            let ty_span = d.value().map(|x| x.span).unwrap_or(place_expr.span);
            return self.raw_index_write(place_expr, (p, idx), v, op, ty_span, stmt.span);
        }
        // s213 (wolf-lang#577): `p[i].f = v`, `(*p).f = v` and nested
        // paths — the element's operands (pointer, then index) run
        // before the right-hand side (`[mem.model.place.rhs]`); row L4
        // is asked on the struct's alignment at the element, then the
        // field's bytes are written at its offset.
        if place_expr.kind == SyntaxKind::MemberExpr
            && let Some((elem, path, lay)) = self.raw_field_path(place_expr)
        {
            let base = self.raw_field_elem(elem, &lay)?;
            let v = match d.value() {
                Some(e) => val!(self.eval(e)),
                None => Value::Unit,
            };
            let op = d.op().map(|t| t.kind).filter(|_| compound);
            let ty_span = d.value().map(|x| x.span).unwrap_or(place_expr.span);
            let prim = self.raw_field_prim(elem, &path);
            let (at, size) = self.raw_field_at(base, &lay, &path, prim, true, stmt.span)?;
            let signed = Self::raw_field_signed(prim);
            return self.raw_write_unchecked(at, size, signed, v, op, ty_span, stmt.span);
        }
        // kw06: `*p = v` / `*p op= v` — the raw tier's write at offset
        // zero. The pointer runs before the right-hand side, as the
        // index form's operands do (`[mem.model.place.rhs]`).
        if place_expr.kind == SyntaxKind::PrefixExpr
            && let Some(pre) = PrefixExpr::cast(place_expr)
            && pre.op().is_some_and(|t| t.kind == SyntaxKind::Star)
            && let Some(operand) = pre.operand()
        {
            let p = self.raw_deref_ptr(place_expr)?;
            let v = match d.value() {
                Some(e) => val!(self.eval(e)),
                None => Value::Unit,
            };
            let op = d.op().map(|t| t.kind).filter(|_| compound);
            let ty_span = d.value().map(|x| x.span).unwrap_or(place_expr.span);
            let size = self.raw_ptr_size(operand, place_expr.span);
            let signed = self.raw_ptr_signed(operand);
            return self.raw_write_at(p, size, signed, v, op, ty_span, stmt.span);
        }
        // wolf-lang#438 (`[mem.region.edge.elem]`): a plain `=` through
        // a container index COPIES a place's value in, as a plain
        // `push` does (the static tier treats it as `xs[i] = copy v`);
        // `xs[i] = take v` moves it.
        //
        // wolf-lang#452 (ruled 2026-09-26: index first, then value;
        // `[mem.model.place.rhs]`): the place's OPERANDS — every index
        // and key, outermost first — are evaluated here, before the
        // right-hand side, as native, release and lupin do. A `Place`
        // carries their values, not an address: the element is found
        // by `write_place` at the store, after the right-hand side has
        // run, so a right-hand side that grows or rehashes the
        // container still stores into the container as it is after
        // the call.
        let Some(place) = found!(self.place_of(place_expr)) else {
            return self.refuse("assignment through this place shape", place_expr.span);
        };
        let elem_store = !compound && place_expr.kind == SyntaxKind::BracketApply;
        let copied = match d.value() {
            Some(e) if elem_store && !d.takes() => self.eval_elem_copy(e)?,
            _ => None,
        };
        let v = match (copied, d.value()) {
            (Some(Flow::Val(v)), _) => v,
            (Some(other), _) => return Ok(other),
            (None, Some(e)) => match self.eval(e)? {
                Flow::Val(v) => v,
                // #122's assignment sibling, measured in the same
                // sweep: a RAW row value assigned to a row-typed
                // place binds exactly as it does at `let` (native
                // does); only a `?`-propagated error unwinds. The
                // compound path below never sees rows (arith owns
                // it).
                Flow::Err(v, false) if !compound => v,
                other => return Ok(other),
            },
            (None, None) => Value::Unit,
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

    /// The copied right-hand side of a plain container-index store
    /// (wolf-lang#438): a PLACE is read — never moved — and deep-copied,
    /// exactly as a plain `push` element is, so the stored element and
    /// the binding it came from are independent; `deep_copy` hands a
    /// scalar, a `str` and every other non-heap value straight back, so
    /// `Copy` elements stay free (`[mem.tier0.move.3]`). `None` is a
    /// temporary — or a bracket read that is not a `List` element (a
    /// `Map` row, a slice), which builds a value of its own — evaluated
    /// by the caller exactly as before: it has no other owner.
    fn eval_elem_copy(&mut self, e: &'t GreenNode) -> E<Option<Flow>> {
        let mut inner = e;
        while inner.kind == SyntaxKind::ParenExpr {
            match ParenExpr::cast(inner).and_then(|p| p.expr()) {
                Some(x) => inner = x,
                None => return Ok(None),
            }
        }
        let list_elem = inner.kind == SyntaxKind::BracketApply
            && BracketApply::cast(inner).is_some_and(|b| {
                b.callee()
                    .is_some_and(|r| matches!(self.expr_ty(r.span), Some(TyKind::List(_))))
                    && !b
                        .args()
                        .into_iter()
                        .flat_map(|l| l.args())
                        .filter_map(Arg::value)
                        .any(|v| v.kind == SyntaxKind::RangeExpr)
            });
        let v = match inner.kind {
            SyntaxKind::PathExpr | SyntaxKind::MemberExpr => match self.place_of(inner)? {
                Found::At(place) => self.read_place(&place, inner.span)?,
                // A module item or a member of a temporary: read by
                // `eval` (neither is move-tracked), then copied.
                Found::Not => match self.eval(inner)? {
                    Flow::Val(v) => v,
                    other => return Ok(Some(other)),
                },
                Found::Flow(f) => return Ok(Some(f)),
            },
            SyntaxKind::BracketApply if list_elem => match self.place_of(inner)? {
                Found::At(place) => self.read_place(&place, inner.span)?,
                Found::Not => return Ok(None),
                // wolf-lang#481: `xs[i] = ys[j()?]` — the operand ran.
                Found::Flow(f) => return Ok(Some(f)),
            },
            _ => return Ok(None),
        };
        Ok(Some(Flow::Val(self.deep_copy(v, inner.span)?)))
    }

    fn eval(&mut self, e: &'t GreenNode) -> E<Flow> {
        // s207 (wolf-lang#541, `[type.unit.discard]`): "A `!T` tail in
        // a unit context is a discard, warned (W0601), never a
        // mismatch. The value's row is lost — and, when T is not `()`,
        // the value with it." Sema records each such tail; here its
        // value and its raw row both become `()`. A `?` that fired
        // inside it (`Flow::Err(_, true)`) still leaves: that row was
        // consumed, not discarded. Until s207 this machine handed the
        // row on — a unit fn's raising tail left `main` with
        // `error: bad` where native, release and lupin printed the
        // next line.
        let discarded = self
            .frames
            .last()
            .and_then(|f| self.ctxs[f.body].as_ref())
            .is_some_and(|c| c.unit_discards.contains(&e.span));
        if discarded {
            return match self.eval_expr(e)? {
                Flow::Val(_) | Flow::Err(_, false) => Ok(Flow::Val(Value::Unit)),
                other => Ok(other),
            };
        }
        self.eval_expr(e)
    }

    fn eval_expr(&mut self, e: &'t GreenNode) -> E<Flow> {
        self.tick()?;
        match e.kind {
            SyntaxKind::LiteralExpr => Ok(Flow::Val(self.literal(e)?)),
            SyntaxKind::StringExpr => self.eval_string(e),
            SyntaxKind::ParenExpr => match ParenExpr::cast(e).and_then(|p| p.expr()) {
                Some(inner) => self.eval(inner),
                None => Ok(Flow::Val(Value::Unit)),
            },
            SyntaxKind::PathExpr | SyntaxKind::MemberExpr => {
                if let Some(place) = found!(self.place_of(e)) {
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
                    return Ok(raise(Value::ErrTag {
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
                // s152 — `m[k]` reads as `V ! {none}` ([mem.map.absent]):
                // the receiver is read in place (never moved), the key
                // evaluated once, and a miss is the row — not a trap,
                // not `()`.
                if let Some(recv) = b.callee()
                    && matches!(self.expr_ty(recv.span), Some(TyKind::Map(..)))
                {
                    let mv = match found!(self.place_of(recv)) {
                        Some(place) => self.read_place(&place, recv.span)?,
                        None => val!(self.eval(recv)),
                    };
                    let Value::Map(id) = mv else {
                        return self.refuse("Map index on a non-map", e.span);
                    };
                    let Some(kx) = b
                        .args()
                        .into_iter()
                        .flat_map(|l| l.args())
                        .find_map(Arg::value)
                    else {
                        return self.refuse("Map index without a key", e.span);
                    };
                    let kv = val!(self.eval(kx));
                    let Some(key) = MapKey::of(&kv) else {
                        return self.refuse("a Map key outside the four admitted types", kx.span);
                    };
                    return Ok(self.map_lookup(id, &key));
                }
                // s37 — `s[a..b]` byte-offset checked slicing (D25).
                if let Some(recv) = b.callee()
                    && matches!(self.expr_ty(recv.span), Some(TyKind::Prim(Prim::Str)))
                {
                    return self.eval_str_slice(e);
                }
                // s128 (#171) — `cs[a..b]` List slicing, same endpoint
                // surface, fresh-List value.
                if let Some(recv) = b.callee()
                    && matches!(self.expr_ty(recv.span), Some(TyKind::List(_)))
                    && b.args()
                        .into_iter()
                        .flat_map(|l| l.args())
                        .filter_map(Arg::value)
                        .any(|v| v.kind == SyntaxKind::RangeExpr)
                {
                    return self.eval_list_slice(e);
                }
                if let Some(place) = found!(self.place_of(e)) {
                    let v = self.read_place(&place, e.span)?;
                    return Ok(Flow::Val(v));
                }
                // s89 (#85): the receiver is not a place — `s.bytes()[i]`,
                // `mk()[i]`. `place_of` roots an index in a frame local
                // and a temporary has none, but the ELEMENT read never
                // needed one: `walk_read` takes a plain value, so the
                // same `PStep::ListIdx` walk (and the same bounds trap)
                // runs over the evaluated receiver. This is the
                // indexing half of s77's byte view, which the checked
                // tier could not reach before.
                if let Some(recv) = b.callee()
                    && matches!(self.expr_ty(recv.span), Some(TyKind::List(_)))
                {
                    // `s.bytes()[i]` reads the view (#232; s153,
                    // #308): the receiver's octets — uncharged, and
                    // unretained: the byte is read off the `str`'s
                    // own storage and no list is minted.
                    let view = found!(self.eval_bytes_view(recv));
                    let base = match view {
                        Some(_) => None,
                        None => Some(val!(self.eval(recv))),
                    };
                    let Some(ix) = b
                        .args()
                        .into_iter()
                        .flat_map(|l| l.args())
                        .find_map(Arg::value)
                    else {
                        return self.refuse("an index without an operand", e.span);
                    };
                    let isp = ix.span;
                    let Value::Int(i) = val!(self.eval(ix)) else {
                        return self.refuse("indexing a List with a non-int", e.span);
                    };
                    let i = if self.origin_at(e.span) == 1 {
                        self.shift_origin(i, e.span)?
                    } else {
                        i
                    };
                    if let Some(octets) = view {
                        let Some(b) = usize::try_from(i).ok().and_then(|i| octets.get(i)) else {
                            return self.trap("bounds", "mem.ub.defined", isp);
                        };
                        return Ok(Flow::Val(Value::Byte(*b)));
                    }
                    let step = [PStep::ListIdx {
                        index: i,
                        span: isp,
                    }];
                    let base = base.expect("a non-view receiver was evaluated");
                    let v = self.walk_read(base, &step, e.span)?;
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
            // `[a, b, c]` (s158, `[type.list.lit.value]`): elements
            // left to right, once each, then a fresh list minted into
            // the ambient region and charged for — the same value
            // `List[T]()` plus a `push` per element produces, and the
            // same accounting.
            SyntaxKind::ListLit => {
                let mut items = Vec::new();
                for item in wolf_ast::ListLit::cast(e).expect("kind").elems() {
                    items.push(val!(self.eval(item)));
                }
                let id = self.mint_list(items, e.span)?;
                Ok(Flow::Val(Value::List(id)))
            }
            SyntaxKind::PrefixExpr => self.eval_prefix(e),
            SyntaxKind::BinExpr => self.eval_bin(e),
            SyntaxKind::CastExpr => self.eval_cast(e),
            SyntaxKind::RangeExpr => self.eval_range(e, true),
            SyntaxKind::TryExpr => {
                let d = wolf_ast::TryExpr::cast(e).expect("kind");
                let inner = match d.expr() {
                    Some(x) => self.eval(x)?,
                    None => Flow::Val(Value::Unit),
                };
                match inner {
                    // `?` is the one PROPAGATING consumer (#122): a
                    // row error reaching it — as a flow or as a bound
                    // row VALUE — unwinds toward the caller (D30),
                    // through any `let` on the way.
                    Flow::Err(err, _) => Ok(Flow::Err(err, true)),
                    Flow::Val(v @ Value::ErrTag { .. }) => Ok(Flow::Err(v, true)),
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
                    Some(x) => match self.eval(x)? {
                        Flow::Val(v) => v,
                        // #139: `return <error row>` (e.g. `return tag`)
                        // — the value expression is itself a raise. It
                        // must UNWIND as a returned error (propagating,
                        // so errdefers run and it crosses the call as the
                        // error flow), NEVER a raw row that a surrounding
                        // `else`/`let` swallows via the #122 raw-row-binds
                        // rule. Without this, a diverging `else` handler's
                        // `return tag` binds the tag to the `let` and
                        // execution falls through past it (the sc20
                        // `reject_tampered_row` finding: the bound row
                        // then fails to iterate at the mem tier).
                        Flow::Err(v, _) => return Ok(Flow::Err(v, true)),
                        other => return Ok(other),
                    },
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
                // The cap evaluates FIRST ([mem.model.order]; it may
                // itself touch the region table), the id is minted
                // after.
                let cap = match self
                    .eval_region_cap(wolf_ast::RegionValue::cast(e).expect("kind").cap())?
                {
                    Ok(c) => c,
                    Err(flow) => return Ok(flow),
                };
                let rid = self.regions.len();
                self.regions.push(DynRegion {
                    live: true,
                    frozen: false,
                    backing: None,
                    span: e.span,
                    charged: 0,
                    cap,
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
            SyntaxKind::ClosureExpr => self.refuse("closures in checked execution", e.span),
            SyntaxKind::ScopeExpr
            | SyntaxKind::SelectExpr
            | SyntaxKind::WhenExpr
            | SyntaxKind::SpawnExpr => self.refuse(
                "structured concurrency in checked execution (C1 deferred)",
                e.span,
            ),
            SyntaxKind::InlineC | SyntaxKind::AsmExpr => self.refuse("inline C / asm", e.span),
            _ => self.refuse("this expression shape in checked execution", e.span),
        }
    }

    /// A region-denoting expression (`in r { }`'s target): a place
    /// read that does NOT move the affine value (opening is not
    /// consumption).
    fn eval_region_ref(&mut self, e: &'t GreenNode) -> E<Flow> {
        if let Some(place) = found!(self.place_of(e)) {
            let v = self.read_place(&place, e.span)?;
            return Ok(Flow::Val(v));
        }
        self.eval(e)
    }

    fn eval_region_block(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = RegionBlock::cast(e).expect("kind");
        // The cap evaluates at creation, before the region opens
        // (s132, [mem.region.cap.1]).
        let cap = match self.eval_region_cap(d.cap())? {
            Ok(c) => c,
            Err(flow) => return Ok(flow),
        };
        let rid = self.regions.len();
        self.regions.push(DynRegion {
            live: true,
            frozen: false,
            backing: None,
            span: e.span,
            charged: 0,
            cap,
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
            Ok(Flow::Err(..)) => self.close_scope(true)?,
            _ => self.close_scope(false)?,
        }
        self.ambient.pop();
        // The sugar-block exit is the wholesale free
        // ([mem.region.intra.2]).
        self.free_region(rid);
        out
    }

    /// `copy region name? { … }` (s216, `[mem.region.copyout]`): the
    /// block runs as `region { … }` does; on the fall-through edge its
    /// value is deep-copied with the ENCLOSING region ambient — every
    /// list and map re-minted there, every `str` charged there — while
    /// the block's region is still live, and only then is the region
    /// freed. Every other edge (`return`, `?`, `break`) frees and
    /// leaves exactly as the plain block does: the static tier already
    /// refused what such an edge could carry out.
    fn eval_copied_block(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = RegionBlock::cast(e).expect("kind");
        let cap = match self.eval_region_cap(d.cap())? {
            Ok(c) => c,
            Err(flow) => return Ok(flow),
        };
        let rid = self.regions.len();
        self.regions.push(DynRegion {
            live: true,
            frozen: false,
            backing: None,
            span: e.span,
            charged: 0,
            cap,
        });
        self.ambient.push(rid);
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
            Ok(Flow::Err(..)) => self.close_scope(true)?,
            _ => self.close_scope(false)?,
        }
        self.ambient.pop();
        let out = match out {
            Ok(Flow::Val(v)) => self.copy_out(v, e.span).map(Flow::Val),
            other => other,
        };
        self.free_region(rid);
        out
    }

    /// The copy `copy region { … }` makes (s216): `deep_copy`'s rule,
    /// plus the parts a plain `copy` shares — a `str`'s bytes are
    /// charged to the ambient region as a fresh materialization, and
    /// enum, row and tuple payloads are copied too — so nothing in the
    /// result is the dying region's.
    fn copy_out(&mut self, v: Value, span: Span) -> E<Value> {
        Ok(match v {
            Value::Str(s) => {
                self.charge_str(s.len() as u64, span)?;
                Value::Str(s)
            }
            Value::List(id) => {
                let elems = self.lists[id].clone();
                let mut copied = Vec::with_capacity(elems.len());
                for e in elems {
                    copied.push(self.copy_out(e, span)?);
                }
                Value::List(self.mint_list(copied, span)?)
            }
            Value::Map(id) => {
                let entries = self.maps[id].clone();
                let nid = self.mint_map(span)?;
                for (k, v) in entries {
                    let cv = self.copy_out(v, span)?;
                    self.map_insert(nid, k, cv, span)?;
                }
                Value::Map(nid)
            }
            Value::Struct { fields } => {
                let mut out = Vec::with_capacity(fields.len());
                for (n, fv) in fields {
                    out.push((n, self.copy_out(fv, span)?));
                }
                Value::Struct { fields: out }
            }
            Value::Enum { variant, payload } => {
                let mut out = Vec::with_capacity(payload.len());
                for p in payload {
                    out.push(self.copy_out(p, span)?);
                }
                Value::Enum {
                    variant,
                    payload: out,
                }
            }
            Value::ErrTag { tag, payload } => {
                let mut out = Vec::with_capacity(payload.len());
                for p in payload {
                    out.push(self.copy_out(p, span)?);
                }
                Value::ErrTag { tag, payload: out }
            }
            other => self.deep_copy(other, span)?,
        })
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
        // The #30 rule on the handler side (#48): the scrutinee's
        // row/enum type names the identifiers that are TAG TESTS in
        // arm position — everything else is a binding. Resolved once,
        // statically, from the recorded scrutinee type.
        //
        // `[type.row.match]` (s197, #497, ruling #21): over a fallible
        // scrutinee the two halves are dispatched apart. A raw row
        // (`Flow::Err(_, false)`: the call's own failure) and a bound
        // row value (`Value::ErrTag`, #122's `let`-bound shape) go to
        // the row arms with the row's tags as the test domain; a value
        // goes to the value arms with the ok type's own domain; a
        // propagating row (`Flow::Err(_, true)`, a `?` that fired
        // inside the scrutinee) leaves past the arms exactly as it
        // leaves past an `else` (`[type.row.else]`, #492).
        let fallible = d
            .scrutinee()
            .and_then(|s| self.fallible_match_names(s.span));
        let domain = match &fallible {
            Some(_) => None,
            None => d.scrutinee().and_then(|s| self.match_domain_names(s.span)),
        };
        let flow = match d.scrutinee() {
            Some(s) => self.eval(s)?,
            None => Flow::Val(Value::Unit),
        };
        let (scrut, row_half, domain) = match (&fallible, flow) {
            (Some((tags, _)), Flow::Err(err, false)) => (err, true, Some(tags.clone())),
            (Some((tags, _)), Flow::Val(v @ Value::ErrTag { .. })) => (v, true, Some(tags.clone())),
            (Some((_, value_names)), Flow::Val(v)) => (v, false, value_names.clone()),
            (None, Flow::Val(v)) => (v, false, domain),
            (_, other) => return Ok(other),
        };
        for arm in d.arms() {
            let Some(pat) = arm.pattern() else { continue };
            if let Some((tags, _)) = &fallible {
                let half = {
                    let text = |s: Span| self.text(s);
                    let is_tag = |n: &str| tags.iter().any(|t| t == n);
                    wolf_sema::check::fallible_arm_half(pat, &text, &is_tag)
                };
                match half {
                    Ok(h) if h.admits(row_half) => {}
                    Ok(_) => continue,
                    Err(construct) => return self.refuse(construct, pat.span),
                }
            }
            let binds = match self.match_pattern(pat, &scrut, domain.as_deref(), false)? {
                Some(b) => b,
                None => continue,
            };
            self.push_scope();
            for (name, v) in binds {
                self.declare(&name, v);
            }
            if let Some(guard) = arm.guard() {
                // The guard node WRAPS its condition (the s27 lesson,
                // relearned here at s130): evaluate the condition
                // expression, not the wrapper.
                let Some(cond) = guard.nodes().find(|n| wolf_ast::is_expr_kind(n.kind)) else {
                    self.close_scope(false)?;
                    return self.refuse("a guard without a condition", guard.span);
                };
                let g = match self.eval(cond)? {
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
            self.close_scope(matches!(out, Flow::Err(..)))?;
            return Ok(out);
        }
        Ok(Flow::Val(Value::Unit))
    }

    /// The scrutinee's tag/variant names, when its recorded type is a
    /// row or an enum — the identifiers a bare arm pattern resolves
    /// against BEFORE it may bind (the #30 rule, handler side). `None`
    /// for scalar scrutinees: every identifier arm binds there.
    fn match_domain_names(&self, span: Span) -> Option<Vec<String>> {
        let ctx = self.ctx();
        let id = *ctx.expr_tys.get(&span)?;
        self.case_names_of(id)
    }

    /// `[type.row.match]` (s197): for a scrutinee recorded as
    /// `T ! {row}`, the row's tag names (the row arms' test domain)
    /// and `T`'s own case names, if it has any (the value arms'
    /// domain). `None` for every other scrutinee.
    #[allow(clippy::type_complexity)]
    fn fallible_match_names(&self, span: Span) -> Option<(Vec<String>, Option<Vec<String>>)> {
        let ctx = self.ctx();
        let mut id = *ctx.expr_tys.get(&span)?;
        for _ in 0..32 {
            match ctx.tb.table.kind(id) {
                TyKind::Distinct(inner) => id = *inner,
                _ => break,
            }
        }
        let TyKind::ErrUnion(ok, row) = ctx.tb.table.kind(id) else {
            return None;
        };
        let tags = match ctx.tb.table.kind(*row) {
            TyKind::Row { tags, .. } => tags.iter().map(|(n, _)| n.clone()).collect(),
            _ => Vec::new(),
        };
        Some((tags, self.case_names_of(*ok)))
    }

    /// The tag/variant names of a type id, through `distinct`
    /// wrappers: a row's tags or a (non-generic) enum's variants.
    fn case_names_of(&self, mut id: TyId) -> Option<Vec<String>> {
        let ctx = self.ctx();
        for _ in 0..32 {
            match ctx.tb.table.kind(id) {
                TyKind::Distinct(inner) => id = *inner,
                _ => break,
            }
        }
        match ctx.tb.table.kind(id) {
            TyKind::Row { tags, .. } => Some(tags.iter().map(|(n, _)| n.clone()).collect()),
            TyKind::Nominal { module, name, .. } => {
                match self.tc.sigs.get(*module as usize, name) {
                    Some(ItemSig::Enum { variants, .. }) => {
                        Some(variants.iter().map(|v| v.name.clone()).collect())
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Match a pattern against a value: `None` = no match;
    /// `Some(binds)` = matched, with the (name, value) pairs the arm
    /// binds. The product match domain (s130) executes here — tuple
    /// and struct patterns element-/field-wise over the positional
    /// and named `Value::Struct` shapes, `Tag(products…)` through
    /// enum/row payloads, literals at any product depth, and
    /// `@`-bindings. `domain` carries the scrutinee row/enum's tag
    /// names: a bare identifier naming one is a tag TEST (match iff
    /// the value carries that tag), mirroring native lowering's
    /// `domain_test` — never a catch-all binding. `nested` marks
    /// sub-value positions, where bind-vs-test for a bare name is
    /// answered by sema's OWN record (`tb.locals`): a name the
    /// checker did not bind is a case test on the sub-value.
    #[allow(clippy::type_complexity)]
    fn match_pattern(
        &mut self,
        pat: &'t GreenNode,
        scrut: &Value,
        domain: Option<&[String]>,
        nested: bool,
    ) -> E<Option<Vec<(String, Value)>>> {
        match pat.kind {
            SyntaxKind::WildcardPat => Ok(Some(Vec::new())),
            SyntaxKind::LiteralPat => {
                let text = self.text(pat.span);
                let matched = match scrut {
                    Value::Int(n) => parse_int_literal(&text) == Some(*n),
                    Value::Bool(b) => text == if *b { "true" } else { "false" },
                    // A char arm (s121): THE shared decoder, so all
                    // lanes agree on every escape spelling.
                    Value::Char(c) => wolf_sema::check::cook_char_literal(&text) == Some(*c),
                    // Cook the pattern exactly as string expressions
                    // cook (escapes, brace doubling, `"""` dedent) so
                    // both lanes compare the same bytes (#54).
                    Value::Str(s) => cooked_str_pattern(&text) == s.as_bytes(),
                    _ => false,
                };
                Ok(if matched { Some(Vec::new()) } else { None })
            }
            // `[gram.pat.range]` (s147, #287): `lo..hi` / `lo..=hi` over
            // an integer or `char` scrutinee — membership by scalar,
            // THE shared char decoder, the exclusive high end excluded.
            // The checker validated the endpoints and refused the
            // empty range (E0815), so a bad end here is a refusal.
            SyntaxKind::RangePat => {
                let d = wolf_ast::RangePat::cast(pat).expect("kind");
                let (Some(lo), Some(hi)) = (d.lo(), d.hi()) else {
                    return self.refuse("a range pattern with a missing end", pat.span);
                };
                let end = |this: &Self, n: &'t GreenNode| -> Option<i64> {
                    let text = this.text(n.span);
                    if text.starts_with('\'') {
                        wolf_sema::check::cook_char_literal(&text).map(|c| i64::from(u32::from(c)))
                    } else {
                        parse_int_literal(&text)
                    }
                };
                let (Some(lo_v), Some(hi_v)) = (end(self, lo), end(self, hi)) else {
                    return self.refuse("this range pattern's end", pat.span);
                };
                let v = match scrut {
                    Value::Int(n) => *n,
                    Value::Char(c) => i64::from(u32::from(*c)),
                    _ => return Ok(None),
                };
                let hit = lo_v <= v && if d.inclusive() { v <= hi_v } else { v < hi_v };
                Ok(if hit { Some(Vec::new()) } else { None })
            }
            SyntaxKind::IdentPat => {
                let name = self.text(pat.span);
                let last = name.rsplit('.').next().unwrap_or(name.as_str());
                if let Some(names) = domain
                    && let Some(tag) = names.iter().find(|n| *n == &name || n.as_str() == last)
                {
                    // The value's tag may carry its type qualifier
                    // (`Pairs.Pair` from a qualified ctor) — native
                    // compares tag IDS, so the checked twin compares
                    // spelling-blind by last segment.
                    let hit = match scrut {
                        Value::ErrTag { tag: t, .. } => tag_name_matches(t, tag),
                        Value::Enum { variant, .. } => tag_name_matches(variant, tag),
                        _ => false,
                    };
                    return Ok(if hit { Some(Vec::new()) } else { None });
                }
                if nested {
                    // Inside a product: sema's record is the ruling —
                    // an ident it bound is a binding, one it did not
                    // is a case test on the sub-value (#4's rule,
                    // decided by the checker, replayed here).
                    let tok = pat
                        .tokens()
                        .find(|t| t.kind == SyntaxKind::Ident)
                        .map(|t| t.span)
                        .unwrap_or(pat.span);
                    let bound = self.ctx().tb.locals.iter().any(|(_, s, _)| *s == tok);
                    if !bound {
                        let hit = match scrut {
                            Value::Enum { variant, .. } => tag_name_matches(variant, last),
                            Value::ErrTag { tag, .. } => tag_name_matches(tag, last),
                            _ => {
                                return self
                                    .refuse("a bare name the checker did not bind", pat.span);
                            }
                        };
                        return Ok(if hit { Some(Vec::new()) } else { None });
                    }
                }
                Ok(Some(vec![(name, scrut.clone())]))
            }
            SyntaxKind::BindingPat => {
                // `name @ pat`: the whole value binds under the name
                // AND the sub-pattern must match it.
                let Some(tok) = pat.tokens().find(|t| t.kind == SyntaxKind::Ident) else {
                    return self.refuse("an `@`-binding without a name", pat.span);
                };
                let mut binds = vec![(self.text(tok.span), scrut.clone())];
                if let Some(inner) = pat.nodes().find(|n| is_pattern_kind(n.kind)) {
                    match self.match_pattern(inner, scrut, domain, nested)? {
                        Some(bs) => binds.extend(bs),
                        None => return Ok(None),
                    }
                }
                Ok(Some(binds))
            }
            SyntaxKind::OrPat => {
                // First matching alternative wins; or-alternatives
                // never carry bindings (native lowering's rule).
                for alt in pat.nodes().filter(|n| is_pattern_kind(n.kind)) {
                    if let Some(bs) = self.match_pattern(alt, scrut, domain, nested)? {
                        if !bs.is_empty() {
                            return self.refuse(
                                "or-patterns with bindings in checked execution",
                                alt.span,
                            );
                        }
                        return Ok(Some(Vec::new()));
                    }
                }
                Ok(None)
            }
            // s130: tuple patterns in arm position — element-wise over
            // the positional Struct value the TupleExpr evaluator
            // builds, refutable sub-patterns and all.
            SyntaxKind::TuplePat => {
                let subs: Vec<&'t GreenNode> =
                    pat.nodes().filter(|n| is_pattern_kind(n.kind)).collect();
                let Value::Struct { fields } = scrut else {
                    return self.refuse(
                        "a tuple pattern over a non-tuple value in checked execution",
                        pat.span,
                    );
                };
                if fields.len() != subs.len() {
                    return self.refuse(
                        "a tuple pattern with a mismatched arity in checked execution",
                        pat.span,
                    );
                }
                let mut binds = Vec::new();
                for (sub, (_, ev)) in subs.iter().zip(fields) {
                    match self.match_pattern(sub, ev, None, true)? {
                        Some(bs) => binds.extend(bs),
                        None => return Ok(None),
                    }
                }
                Ok(Some(binds))
            }
            // s130: struct patterns in arm position — by FIELD NAME
            // over the named Struct value; omitted fields and `..`
            // are simply not read.
            SyntaxKind::StructPat => {
                let Some(d) = wolf_ast::StructPat::cast(pat) else {
                    return self.refuse("this pattern shape", pat.span);
                };
                let Value::Struct { fields } = scrut else {
                    return self.refuse(
                        "a struct pattern over a non-struct value in checked execution",
                        pat.span,
                    );
                };
                let mut binds = Vec::new();
                for f in d.fields() {
                    let Some(nspan) = f.name_span() else { continue };
                    let fname = self.text(nspan);
                    let Some(sub) = f.pattern() else { continue };
                    let Some((_, ev)) = fields.iter().find(|(n, _)| *n == fname) else {
                        // Sema owns the unknown-field report (E0403);
                        // reaching here is a defensive refusal.
                        return self.refuse("a field the struct does not carry", nspan);
                    };
                    match self.match_pattern(sub, &ev.clone(), None, true)? {
                        Some(bs) => binds.extend(bs),
                        None => return Ok(None),
                    }
                }
                Ok(Some(binds))
            }
            // s130: `Tag(sub…)` — the tag test plus payload
            // sub-patterns, the nesting the c06 family held out
            // (products, literals and `@`-bindings in payload slots).
            SyntaxKind::PathPat => {
                let end = pat
                    .nodes()
                    .find(|n| is_pattern_kind(n.kind))
                    .map(|n| n.span.lo)
                    .unwrap_or(pat.span.hi);
                let ctx = self.ctx();
                let src = &self.pkg.files[ctx.src_file].raw.src;
                let raw =
                    String::from_utf8_lossy(&src[pat.span.lo as usize..end as usize]).into_owned();
                let name: String = raw.trim_end().trim_end_matches('(').trim_end().to_string();
                let last = name.rsplit('.').next().unwrap_or(name.as_str()).to_string();
                let subs: Vec<&'t GreenNode> =
                    pat.nodes().filter(|n| is_pattern_kind(n.kind)).collect();
                let (tag, payload) = match scrut {
                    Value::Enum { variant, payload } => (variant, payload),
                    Value::ErrTag { tag, payload } => (tag, payload),
                    _ => {
                        return self.refuse(
                            "a tag pattern over this value in checked execution",
                            pat.span,
                        );
                    }
                };
                if !tag_name_matches(tag, &last) {
                    return Ok(None);
                }
                if subs.len() != payload.len() {
                    return self.refuse("payload arity in a pattern (checker contract)", pat.span);
                }
                let payload = payload.clone();
                let mut binds = Vec::new();
                for (sub, ev) in subs.iter().zip(&payload) {
                    match self.match_pattern(sub, ev, None, true)? {
                        Some(bs) => binds.extend(bs),
                        None => return Ok(None),
                    }
                }
                Ok(Some(binds))
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

    /// Build a range from its expression. `normalize` is the VALUE
    /// form (`[type.range.accessor]`): `a..=b` becomes the exclusive
    /// `b + 1` under checked arithmetic, and `0..=int.MAX` traps where
    /// it is built. A `for` HEADER passes `false` (#381): the header
    /// never materializes a range (`[type.range.value]`), so its
    /// inclusive end rides back un-normalized and the loop walks
    /// `a..=b` itself, the native rung's do-while over the inclusive
    /// bound.
    fn eval_range(&mut self, e: &'t GreenNode, normalize: bool) -> E<Flow> {
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
        // s158 (`[type.range.name]`): the closed family is
        // `int` and `char`. A char range carries its endpoints
        // as scalar values and remembers to read them back as
        // chars.
        let (s, mut en, chars) = match (start, end) {
            (Value::Int(a), Value::Int(b)) => (a, b, false),
            (Value::Char(a), Value::Char(b)) => (a as i64, b as i64, true),
            _ => return self.refuse("non-integer ranges", e.span),
        };
        if normalize && d.is_inclusive() {
            // `a..=b` normalizes to the exclusive `b + 1` HERE,
            // where the range is built (`[type.range.accessor]`,
            // s158) — under the checked arithmetic
            // `[mem.iter.range]` already rules, so `0..=int.MAX`
            // traps `overflow` at its construction. It was a
            // Rust `+= 1` until s158 made the value reachable
            // without a `for` header to defend it.
            match en.checked_add(1) {
                Some(v) => en = v,
                None => return self.trap("overflow", "mem.iter.range", e.span),
            }
        }
        Ok(Flow::Val(Value::Range {
            start: s,
            end: en,
            chars,
        }))
    }

    fn eval_for(&mut self, e: &'t GreenNode) -> E<Flow> {
        let d = ForExpr::cast(e).expect("kind");
        let mut view_items: Option<Vec<Value>> = None;
        // A `for` over a range header (#381): endpoints evaluated once,
        // left to right, and never normalized — `a..=b` is walked as
        // itself, so `0..=int.MAX` in a header does not trap.
        let mut header_inclusive = false;
        let iter = match d.iterable() {
            Some(it) if it.kind == SyntaxKind::RangeExpr => {
                header_inclusive = RangeExpr::cast(it).expect("kind").is_inclusive();
                val!(self.eval_range(it, false))
            }
            // s72, D40 ([mem.iter.excl]): iterating a place is a
            // READ, never a move — the container stays live behind
            // the walk and after it, exactly as the static tier now
            // guarantees. The machine keeps its loop-entry snapshot
            // (the items clone below); the dynamic claim-and-trap
            // mirror is the v0.1.8 interpreter's scope, and the
            // static E1013 rejects the mutating shapes before this
            // lane ever runs them.
            // `for b in s.bytes()` is the canonical consumed position
            // (#232): the view, uncharged — and since s153 (#308)
            // unretained: the octets ARE the loop's items, and no
            // list is minted for them.
            Some(it) => match found!(self.eval_bytes_view(it)) {
                Some(octets) => {
                    view_items = Some(octets.into_iter().map(Value::Byte).collect());
                    Value::Unit
                }
                None => match found!(self.place_of(it)) {
                    Some(place) => self.read_place(&place, it.span)?,
                    None => val!(self.eval(it)),
                },
            },
            None => Value::Unit,
        };
        // A range is walked, never collected (#381): the items come
        // off the endpoints one step at a time, so a range as wide as
        // the integer line costs one `Value` at a time, and a loop
        // that does not leave early runs into the step budget
        // (`[exec.checked.budget]`) — `unsupported`, never a host
        // allocation the byte ledger did not charge.
        let scalar = move |chars: bool, v: i64| match (
            chars,
            u32::try_from(v).ok().and_then(char::from_u32),
        ) {
            (true, Some(c)) => Value::Char(c),
            _ => Value::Int(v),
        };
        // `for v in ch` (#342, `[conc.chan.close]`): one receive per
        // step, ending at drained-close.
        let mut chan: Option<usize> = None;
        let mut items: Box<dyn Iterator<Item = Value>> = match (view_items, iter) {
            (Some(items), _) => Box::new(items.into_iter()),
            (None, Value::Chan(id)) => {
                chan = Some(id);
                Box::new(std::iter::empty())
            }
            (None, Value::Range { start, end, chars }) if header_inclusive => {
                Box::new((start..=end).map(move |v| scalar(chars, v)))
            }
            (None, Value::Range { start, end, chars }) => {
                Box::new((start..end).map(move |v| scalar(chars, v)))
            }
            (None, Value::List(id)) => Box::new(self.lists[id].clone().into_iter()),
            (None, Value::Map(_)) => {
                return self.refuse("iterating a `Map` directly (walk `m.pairs()`)", e.span);
            }
            _ => return self.refuse("iteration outside ranges and List", e.span),
        };
        loop {
            let item = match chan {
                Some(id) => match self.chan_recv(id, e.span)? {
                    Flow::Val(v) => v,
                    _ => break,
                },
                None => match items.next() {
                    Some(v) => v,
                    None => break,
                },
            };
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
                        self.close_scope(matches!(other, Flow::Err(..)))?;
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
        // A bound row VALUE scrutinizes exactly like the err flow
        // (#122: `let`-bound rows reach their handlers).
        let scrut = match scrut {
            Flow::Val(v @ Value::ErrTag { .. }) => raise(v),
            other => other,
        };
        match scrut {
            // `[type.row.else]` (#492, ruling #18): the handler runs for
            // the scrutinee's OWN row only — a raw row (a fallible
            // call's failure, an absent key, a block whose value is a
            // row). A `?` that fired inside the scrutinee, or a row
            // `return`ed from inside it, is already propagating
            // (`Flow::Err(_, true)`) and leaves past the `else`, as on
            // native, release and lupin; it falls to `other` below.
            Flow::Err(err, false) => {
                self.push_scope();
                if let Some(pat) = d.handler_pattern() {
                    if pat.kind == SyntaxKind::PathPat {
                        // `else |Tag(p)|` (s71, #43): sema proved the
                        // pattern covers the row, so the tag test
                        // cannot miss; the sub-patterns bind the
                        // payload slots, exactly as a match arm's
                        // would.
                        self.bind_handler_path(pat, err)?;
                    } else {
                        self.bind_pattern(pat, err)?;
                    }
                }
                let out = match d.fallback() {
                    Some(fb) => self.eval(fb)?,
                    None => Flow::Val(Value::Unit),
                };
                self.close_scope(matches!(out, Flow::Err(..)))?;
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
            // kw06: `*p` reads through a raw pointer.
            Some(SyntaxKind::Star) => self.raw_deref_read(e),
            // s216 (`[mem.region.copyout]`, wolf-lang#612): `copy
            // region { … }` — the value is copied into the enclosing
            // region before the block's region is freed.
            Some(SyntaxKind::CopyKw) if operand.kind == SyntaxKind::RegionBlock => {
                self.eval_copied_block(operand)
            }
            Some(SyntaxKind::CopyKw) => {
                // `copy x`: an independent deep duplicate.
                let v = if let Some(place) = found!(self.place_of(operand)) {
                    self.read_place(&place, operand.span)?
                } else {
                    val!(self.eval(operand))
                };
                let copied = self.deep_copy(v, operand.span)?;
                Ok(Flow::Val(copied))
            }
            Some(SyntaxKind::MoveKw) => {
                if let Some(place) = found!(self.place_of(operand)) {
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
                // s155 (`[type.trait.op]`): `-x` on a user type or a
                // type parameter runs `Neg.neg`.
                if let Some(Dispatch::Trait {
                    module,
                    name,
                    method,
                    ..
                }) = self.ctx().dispatch.get(&e.span).cloned()
                {
                    let v = val!(self.eval_arg(operand, None));
                    return self.op_dispatch_call(
                        operand,
                        v,
                        Vec::new(),
                        *module,
                        name,
                        method,
                        e.span,
                    );
                }
                // The direct `-<int literal>` spelling decodes as the
                // NEGATED value in one step (#151, mirroring WIR
                // lowering's rule): `i64::MIN` has no positive half,
                // so evaluating the literal first could only refuse.
                if operand.kind == SyntaxKind::LiteralExpr
                    && !matches!(
                        self.expr_ty(operand.span),
                        Some(TyKind::Prim(Prim::F64 | Prim::F32))
                    )
                    && self.wrapping_width(operand.span).is_none()
                {
                    let text = self.text(operand.span);
                    let plain = !text.starts_with('\'') && text != "true" && text != "false";
                    if plain && let Some(bits) = parse_uint_literal(&text) {
                        let neg = -i128::from(bits);
                        return match i64::try_from(neg) {
                            Ok(m) => Ok(Flow::Val(Value::Int(m))),
                            // Below i64::MIN: sema's E0415 owns this; a
                            // stray arrival refuses, never aborts.
                            Err(_) => {
                                self.refuse("this literal shape in checked execution", e.span)
                            }
                        };
                    }
                }
                let v = val!(self.eval(operand));
                // s220: `-x` at a wrapping type wraps at its width, and at
                // a plain unsigned type is `0 - x` there (any `x` but 0
                // traps `overflow`) — lupin's answers; both were the
                // signed negation before (`-(5 as u8)` was -5).
                if let Value::Int(n) = v {
                    if let Some((mask, bits, unsigned)) = self.wrapping_width(e.span) {
                        return Ok(Flow::Val(Value::Int(wrap_held(
                            n.wrapping_neg(),
                            mask,
                            bits,
                            unsigned,
                        ))));
                    }
                    if self.unsigned_at(e.span) {
                        if n != 0 {
                            return self.trap("overflow", "mem.ub.defined", e.span);
                        }
                        return Ok(Flow::Val(Value::Int(0)));
                    }
                }
                match v {
                    Value::Int(n) => match n.checked_neg() {
                        Some(m) => Ok(Flow::Val(Value::Int(m))),
                        None => self.trap("overflow", "mem.ub.defined", e.span),
                    },
                    // `-b` widens first (D72): int's negation of the octet.
                    Value::Byte(b) => Ok(Flow::Val(Value::Int(-i64::from(b)))),
                    // IEEE negation flips the sign bit; `-0.0` exists.
                    Value::F64(x) => Ok(Flow::Val(Value::F64(-x))),
                    _ => self.refuse("negation outside integers", e.span),
                }
            }
            Some(SyntaxKind::Not) => {
                let v = val!(self.eval(operand));
                match v {
                    Value::Bool(b) => Ok(Flow::Val(Value::Bool(!b))),
                    // s213 (wolf-lang#575, `[type.int.not]`): a byte
                    // widens to `int` first (`[type.byte.op]`).
                    Value::Byte(b) => Ok(Flow::Val(Value::Int(!i64::from(b)))),
                    Value::Int(n) => self.int_complement(n, operand.span),
                    _ => self.refuse("`!` outside booleans and integers", e.span),
                }
            }
            _ => self.eval(operand),
        }
    }

    fn deep_copy(&mut self, v: Value, span: Span) -> E<Value> {
        Ok(match v {
            Value::List(id) => {
                let elems = self.lists[id].clone();
                let mut copied = Vec::with_capacity(elems.len());
                for e in elems {
                    copied.push(self.deep_copy(e, span)?);
                }
                let nid = self.mint_list(copied, span)?;
                Value::List(nid)
            }
            Value::Pool(id) => {
                let slots = self.pools[id].clone();
                let nid = self.pools.len();
                self.pools.push(slots);
                Value::Pool(nid)
            }
            Value::Map(id) => {
                let entries = self.maps[id].clone();
                let nid = self.mint_map(span)?;
                for (k, v) in entries {
                    let cv = self.deep_copy(v, span)?;
                    self.map_insert(nid, k, cv, span)?;
                }
                Value::Map(nid)
            }
            Value::Struct { fields } => {
                let mut out = Vec::with_capacity(fields.len());
                for (n, fv) in fields {
                    out.push((n, self.deep_copy(fv, span)?));
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
        // s155 (`[type.trait.op]`): an operator carrying a dispatch
        // record runs the impl — the operands are its `read`
        // arguments, the answer is read back the way the operator
        // means it.
        if let Some(Dispatch::Trait {
            module,
            name,
            method,
            ..
        }) = self.ctx().dispatch.get(&e.span).cloned()
            && let (Some(le), Some(re), Some(op)) = (d.lhs(), d.rhs(), op)
        {
            let l = val!(self.eval_arg(le, None));
            let r = val!(self.eval_arg(re, None));
            let out = self.op_dispatch_call(le, l, vec![r], *module, name, method, e.span)?;
            let Flow::Val(out) = out else {
                return Ok(out);
            };
            return Ok(Flow::Val(match (op, out) {
                (SyntaxKind::NotEq, Value::Bool(b)) => Value::Bool(!b),
                (
                    SyntaxKind::Lt | SyntaxKind::Gt | SyntaxKind::LtEq | SyntaxKind::GtEq,
                    Value::Enum { variant, .. },
                ) => Value::Bool(match op {
                    SyntaxKind::Lt => variant == "Less",
                    SyntaxKind::Gt => variant == "Greater",
                    SyntaxKind::LtEq => variant != "Greater",
                    _ => variant != "Less",
                }),
                (SyntaxKind::Lt | SyntaxKind::Gt | SyntaxKind::LtEq | SyntaxKind::GtEq, _) => {
                    return self.refuse("an ordering dispatch answering a non-enum", e.span);
                }
                (_, out) => out,
            }));
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
            | SyntaxKind::Percent
            | SyntaxKind::Amp
            | SyntaxKind::Pipe
            | SyntaxKind::Caret
            | SyntaxKind::Shl
            | SyntaxKind::Shr => {
                let v = self.arith_binop_at(op, l, r, e.span, e.span)?;
                Ok(Flow::Val(v))
            }
            SyntaxKind::EqEq | SyntaxKind::NotEq => {
                let eq = values_equal(&l, &r);
                let want = op == SyntaxKind::EqEq;
                Ok(Flow::Val(Value::Bool(eq == want)))
            }
            SyntaxKind::Lt | SyntaxKind::Gt | SyntaxKind::LtEq | SyntaxKind::GtEq => match (l, r) {
                // s220 (wolf-lang#538, #551): a 64-bit unsigned operand
                // is held as its bit pattern, so it orders as `u64` —
                // the compiled tiers' `icmp u*`.
                (Value::Int(a), Value::Int(b))
                    if d.lhs().is_some_and(|l| self.wide_unsigned_at(l.span)) =>
                {
                    let (a, b) = (a as u64, b as u64);
                    let out = match op {
                        SyntaxKind::Lt => a < b,
                        SyntaxKind::Gt => a > b,
                        SyntaxKind::LtEq => a <= b,
                        _ => a >= b,
                    };
                    Ok(Flow::Val(Value::Bool(out)))
                }
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
                // `char` orders by scalar value (D58) — Rust's `char`
                // order IS scalar order, so the host compare is the
                // reference the compiled lanes' i32 icmp answers to.
                (Value::Char(a), Value::Char(b)) => {
                    let out = match op {
                        SyntaxKind::Lt => a < b,
                        SyntaxKind::Gt => a > b,
                        SyntaxKind::LtEq => a <= b,
                        _ => a >= b,
                    };
                    Ok(Flow::Val(Value::Bool(out)))
                }
                // `byte` orders by octet value (D72) — the unsigned
                // compare the compiled lanes' `icmp u*` answers to.
                (Value::Byte(a), Value::Byte(b)) => {
                    let out = match op {
                        SyntaxKind::Lt => a < b,
                        SyntaxKind::Gt => a > b,
                        SyntaxKind::LtEq => a <= b,
                        _ => a >= b,
                    };
                    Ok(Flow::Val(Value::Bool(out)))
                }
                _ => self.refuse("ordering outside integers, `char`, `byte` and str", e.span),
            },
            _ => self.refuse("this operator in checked execution", e.span),
        }
    }

    /// Run the impl an operator dispatches through (`[type.trait.op]`):
    /// the receiver's concrete type names the body (the impl's, else
    /// the trait's default), exactly as a qualified `Trait.method`
    /// call resolves; a raised row comes back as the raise.
    #[allow(clippy::too_many_arguments)]
    fn op_dispatch_call(
        &mut self,
        recv_expr: &'t GreenNode,
        recv: Value,
        rest: Vec<Value>,
        module: usize,
        name: &str,
        method: &str,
        at: Span,
    ) -> E<Flow> {
        let concrete = self.trait_concrete(recv_expr.span, false, &recv, at)?;
        let body = self.resolve_trait_body(&concrete, module, name, method, at)?;
        let mut call_args = vec![recv];
        call_args.extend(rest);
        self.pending_self_ty = Some(concrete);
        let out = self.call_body(body, call_args)?;
        if let Value::ErrTag { .. } = out {
            return Ok(raise(out));
        }
        Ok(Flow::Val(out))
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
            SyntaxKind::AmpEq => SyntaxKind::Amp,
            SyntaxKind::PipeEq => SyntaxKind::Pipe,
            SyntaxKind::CaretEq => SyntaxKind::Caret,
            SyntaxKind::ShlEq => SyntaxKind::Shl,
            SyntaxKind::ShrEq => SyntaxKind::Shr,
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
        // A byte operand widens to int before any operator (D72,
        // [type.byte.op]); the term is int-typed, so the checked int
        // rails below are exactly the compiled lanes' zext + op.
        let widen = |v: Value| match v {
            Value::Byte(b) => Value::Int(i64::from(b)),
            other => other,
        };
        let (l, r) = (widen(l), widen(r));
        match (l, r) {
            // D62 (s128): `+` on two strs is `"{s}{u}"` — UTF-8
            // concatenation, a fresh str per application. #278
            // (`[type.str.concat]`): a char on either side is the
            // same append, the scalar's UTF-8 bytes — `{c}`'s
            // rendering. Sema admits no other mix, so any other pair
            // here falls through to the modelled-surface refusal.
            // s153 (#308/#310): the fresh str is an allocation in the
            // ambient region — charged (`mint_str`).
            (Value::Str(mut a), Value::Str(b)) if op == SyntaxKind::Plus => {
                self.charge_str((a.len() + b.len()) as u64, span)?;
                a.push_str(&b);
                Ok(Value::Str(a))
            }
            (Value::Str(mut a), Value::Char(c)) if op == SyntaxKind::Plus => {
                self.charge_str((a.len() + c.len_utf8()) as u64, span)?;
                a.push(c);
                Ok(Value::Str(a))
            }
            (Value::Char(c), Value::Str(b)) if op == SyntaxKind::Plus => {
                self.charge_str((c.len_utf8() + b.len()) as u64, span)?;
                let mut out = String::with_capacity(c.len_utf8() + b.len());
                out.push(c);
                out.push_str(&b);
                Ok(Value::Str(out))
            }
            (Value::Int(a), Value::Int(b)) => {
                // Wrapping types wrap at their width; checked prims
                // trap (X3).
                if let Some((mask, bits, unsigned)) = self
                    .wrapping_width(ty_span)
                    .or_else(|| self.wrapping_width(span))
                {
                    let out = match op {
                        SyntaxKind::Plus => a.wrapping_add(b),
                        SyntaxKind::Minus => a.wrapping_sub(b),
                        SyntaxKind::Star => a.wrapping_mul(b),
                        // s220 (wolf-lang#538, `[type.wrap.div]`): the
                        // VALUES divide. A `wrapping[u64]` above
                        // `i64::MAX` is held as its bit pattern, so it
                        // divides as `u64`; every narrower value is
                        // held as itself (unsigned masked, signed
                        // sign-extended), so `i64`'s division is the
                        // width's, and `MIN / -1` wraps below.
                        SyntaxKind::Slash => {
                            if b == 0 {
                                return self.trap("div-zero", "mem.ub.defined", span);
                            }
                            if unsigned && bits == 64 {
                                ((a as u64) / (b as u64)) as i64
                            } else {
                                a.wrapping_div(b)
                            }
                        }
                        SyntaxKind::Percent => {
                            if b == 0 {
                                return self.trap("div-zero", "mem.ub.defined", span);
                            }
                            if unsigned && bits == 64 {
                                ((a as u64) % (b as u64)) as i64
                            } else {
                                a.wrapping_rem(b)
                            }
                        }
                        // Bitwise/shift arms mirror the native rung
                        // (#130): band/bor/bxor on the width's bits;
                        // shift amounts mask to the bit width (the
                        // WIR `shl`/`lshr`/`ashr` contract); `>>` is
                        // logical for unsigned wrapping types and
                        // arithmetic for signed ones (`sema_unsigned`,
                        // the s26 decision).
                        SyntaxKind::Amp => a & b,
                        SyntaxKind::Pipe => a | b,
                        SyntaxKind::Caret => a ^ b,
                        SyntaxKind::Shl => {
                            let c = (b as u64 & u64::from(bits - 1)) as u32;
                            a.wrapping_shl(c)
                        }
                        SyntaxKind::Shr => {
                            let c = (b as u64 & u64::from(bits - 1)) as u32;
                            if unsigned {
                                ((a as u64 & mask as u64) >> c) as i64
                            } else {
                                // Sign-extend from the wrap width,
                                // then shift arithmetically.
                                let sh = 64 - bits;
                                (a.wrapping_shl(sh) >> sh) >> c
                            }
                        }
                        _ => a,
                    };
                    return Ok(Value::Int(wrap_held(out, mask, bits, unsigned)));
                }
                // s213 (wolf-lang#575): `&`, `|` and `^` on CHECKED
                // (non-wrapping) integers are total and closed over
                // every range this machine holds — a signed width's
                // two's-complement bits stay sign-extended, an unsigned
                // width's stay in `0..=max` — so they are the bits'
                // own answer, as on every other machine (the mask
                // idiom `x & !m` needs them). Shifts can leave the
                // width and still have no ruled checked-tier semantics
                // — the honest refusal, never a silent identity.
                match op {
                    SyntaxKind::Amp => return Ok(Value::Int(a & b)),
                    SyntaxKind::Pipe => return Ok(Value::Int(a | b)),
                    SyntaxKind::Caret => return Ok(Value::Int(a ^ b)),
                    SyntaxKind::Shl | SyntaxKind::Shr => {
                        return self.refuse("this operator in checked execution", span);
                    }
                    _ => {}
                }
                // s220 (wolf-lang#551): a `u64`/`uint` is held as its bit
                // pattern (the compiled tiers' register), so its checked
                // arithmetic is `u64`'s: in range up to `2^64 - 1`,
                // `overflow` past it or below 0, exactly X3's rails.
                if self.wide_unsigned_at(ty_span) || self.wide_unsigned_at(span) {
                    let (x, y) = (a as u64, b as u64);
                    let out = match op {
                        SyntaxKind::Plus => x.checked_add(y),
                        SyntaxKind::Minus => x.checked_sub(y),
                        SyntaxKind::Star => x.checked_mul(y),
                        SyntaxKind::Slash | SyntaxKind::Percent => {
                            if y == 0 {
                                return self.trap("div-zero", "mem.ub.defined", span);
                            }
                            Some(if op == SyntaxKind::Slash { x / y } else { x % y })
                        }
                        _ => Some(x),
                    };
                    let Some(out) = out else {
                        return self.trap("overflow", "mem.ub.defined", span);
                    };
                    return Ok(Value::Int(out as i64));
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
            // nan are VALUES; X3's trap law is integer law. `%` is C
            // `fmod` (`[type.float.rem]`, s156) — which is exactly
            // what Rust's `f64 % f64` computes, what LLVM's `frem`
            // lowers to, and what CTFE has folded all along.
            (Value::F64(a), Value::F64(b)) => {
                let out = match op {
                    SyntaxKind::Plus => a + b,
                    SyntaxKind::Minus => a - b,
                    SyntaxKind::Star => a * b,
                    SyntaxKind::Slash => a / b,
                    SyntaxKind::Percent => a % b,
                    _ => a,
                };
                Ok(Value::F64(out))
            }
            (Value::Str(mut a), Value::Str(b)) if op == SyntaxKind::Plus => {
                self.charge_str((a.len() + b.len()) as u64, span)?;
                a.push_str(&b);
                Ok(Value::Str(a))
            }
            _ => self.refuse("arithmetic outside integers", span),
        }
    }

    /// s213 (wolf-lang#575, `[type.int.not]`): `!n`, the bitwise
    /// complement at the operand's type — total, never a trap. A
    /// signed width's complement is `-n - 1` (its range is symmetric
    /// about that map); an unsigned width's is `max - n`; a
    /// `wrapping[T]` is the complement at its width, held as the
    /// machine holds that width (`wrap_held`). A `u64`/`uint` is held
    /// as its bit pattern (s220, wolf-lang#551), so its complement is
    /// the pattern's; until s220 it lay past this machine's range and
    /// was refused by name.
    fn int_complement(&mut self, n: i64, operand: Span) -> E<Flow> {
        if let Some((mask, bits, unsigned)) = self.wrapping_width(operand) {
            return Ok(Flow::Val(Value::Int(wrap_held(!n, mask, bits, unsigned))));
        }
        let prim = {
            let ctx = self.ctx();
            ctx.expr_tys
                .get(&operand)
                .and_then(|id| match ctx.tb.table.kind(*id) {
                    TyKind::Prim(p) => Some(*p),
                    _ => None,
                })
        };
        match prim {
            // s220 (wolf-lang#551): the complement of the held bit
            // pattern is the complement of the `u64` value.
            Some(Prim::U64 | Prim::Uint) => Ok(Flow::Val(Value::Int(!n))),
            Some(p @ (Prim::U8 | Prim::U16 | Prim::U32)) => {
                let (_, hi) = prim_range(p).expect("a narrow unsigned width");
                Ok(Flow::Val(Value::Int(hi - n)))
            }
            _ => Ok(Flow::Val(Value::Int(!n))),
        }
    }

    /// Is the expression at `span` a `wrapping[T]`? Returns
    /// `(mask, bits, unsigned)` when so — the wrap mask, the wrap
    /// width, and the inner prim's signedness (mirrors the native
    /// rung's `sema_unsigned`: `uint`/`u8`/`u16`/`u32`/`u64`).
    fn wrapping_width(&self, span: Span) -> Option<(i64, u32, bool)> {
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
            let unsigned = matches!(p, Prim::Uint | Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64);
            return Some((mask, bits, unsigned));
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

    /// s220 (wolf-lang#551, #538): is the integer at `span` a 64-bit
    /// unsigned one — `u64` or `uint`, through `wrapping` and
    /// `distinct`? Such a value is held as its BIT PATTERN in the
    /// machine's `i64` cell (the compiled tiers' register convention),
    /// so a value above `i64::MAX` is a negative cell; every reader that
    /// cares about magnitude (literal, arithmetic, order, render, cast)
    /// reads it back as `u64`. Every other integer is held as its value.
    fn wide_unsigned_at(&self, span: Span) -> bool {
        let ctx = self.ctx();
        ctx.expr_tys
            .get(&span)
            .is_some_and(|id| wide_unsigned(&ctx.tb.table, *id))
    }

    /// s220: is the integer at `span` unsigned (`uint`, `u8`…`u64`,
    /// through `distinct`; a `wrapping` type answers through
    /// [`Self::wrapping_width`] instead)?
    fn unsigned_at(&self, span: Span) -> bool {
        let ctx = self.ctx();
        let Some(mut id) = ctx.expr_tys.get(&span).copied() else {
            return false;
        };
        for _ in 0..32 {
            match ctx.tb.table.kind(id) {
                TyKind::Distinct(i) => id = *i,
                _ => break,
            }
        }
        matches!(
            ctx.tb.table.kind(id),
            TyKind::Prim(Prim::Uint | Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64)
        )
    }

    /// s220: the target range of an integer cast to the prim at `span`,
    /// as `i128` so the whole of `u64` fits; `None` off the integer prims.
    fn int_target_range(&self, span: Span) -> Option<(i128, i128)> {
        let ctx = self.ctx();
        let id = ctx.expr_tys.get(&span)?;
        match ctx.tb.table.kind(*id) {
            TyKind::Prim(Prim::U64 | Prim::Uint) => Some((0, i128::from(u64::MAX))),
            TyKind::Prim(Prim::I64 | Prim::Int) => {
                Some((i128::from(i64::MIN), i128::from(i64::MAX)))
            }
            TyKind::Prim(p) => prim_range(*p).map(|(lo, hi)| (i128::from(lo), i128::from(hi))),
            _ => None,
        }
    }

    fn literal(&mut self, e: &'t GreenNode) -> E<Value> {
        let text = self.text(e.span);
        if text == "true" {
            return Ok(Value::Bool(true));
        }
        if text == "false" {
            return Ok(Value::Bool(false));
        }
        // `'a'` (s121, D58): THE shared decoder — the same cook WIR
        // lowering uses, so the lanes cannot drift on an escape.
        if text.starts_with('\'') {
            return match wolf_sema::check::cook_char_literal(&text) {
                Some(c) => Ok(Value::Char(c)),
                None => self.refuse("this char literal shape in checked execution", e.span),
            };
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
                return self.refuse(
                    "`f32` in checked execution (f64 is the supported float)",
                    e.span,
                );
            }
            _ => {}
        }
        // Unsigned WRAPPING literals are bit patterns on the wrap
        // width (#130, mirroring the native rung's s26 rule): the
        // full u64 range is admissible at `wrapping[u64]`
        // (`0xc19bf174cf692694` is a value, not an overflow), and a
        // literal beyond a narrower wrap width refuses exactly as
        // WIR lowering does. Storage keeps this machine's masked
        // convention (`arith_binop_at` masks every result the same
        // way).
        if let Some((mask, bits, true)) = self.wrapping_width(e.span)
            && let Some(v) = parse_uint_literal(&text)
        {
            if bits < 64 && v >= (1u64 << bits) {
                return self.refuse("an unsigned literal beyond its type's width", e.span);
            }
            return Ok(Value::Int((v as i64) & mask));
        }
        // A signed narrow WRAPPING literal is held as its value at the
        // width (`wrap_held`, s220): the identity on every literal sema
        // admits.
        if let Some((mask, bits, false)) = self.wrapping_width(e.span)
            && let Some(n) = parse_int_literal(&text)
        {
            return Ok(Value::Int(wrap_held(n, mask, bits, false)));
        }
        // s220 (wolf-lang#551): a `u64`/`uint` literal is admissible over
        // the type's whole range and is held as its bit pattern
        // (`wide_unsigned_at`); sema has already refused one past
        // `2^64 - 1`.
        if self.wide_unsigned_at(e.span)
            && let Some(v) = parse_uint_literal(&text)
        {
            return Ok(Value::Int(v as i64));
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
                    let hv = if let Some(place) = found!(self.place_of(hole)) {
                        self.read_place(&place, hole.span)?
                    } else {
                        val!(self.eval(hole))
                    };
                    let rendered = self.render(&hv, self.ctx().expr_tys.get(&hole.span).copied());
                    match i.format_spec() {
                        Some(spec) => {
                            let unsigned = self.wide_unsigned_at(hole.span);
                            self.apply_format_spec(spec, &hv, unsigned, rendered)?
                        }
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
                    construct: "interpolation inside a multiline string",
                    span: e.span,
                }));
            }
            let inner = &bytes[3..bytes.len().saturating_sub(3).max(3)];
            let dedented = dedent_multiline(inner);
            let decoded = decode_escapes(&dedented);
            let out = String::from_utf8_lossy(&decoded).into_owned();
            return Ok(Flow::Val(Value::Str(out)));
        }
        // Raw literal (#76): the whole opening delimiter — `r"`,
        // `r#"`, … — strips, and the inner bytes are the value
        // verbatim ([gram.lex.str.raw]: no escapes, no interpolation;
        // the lexer emits no `Interp` inside one, so `holes` is empty).
        if let Some(inner) = raw_str_inner(bytes) {
            let out = String::from_utf8_lossy(inner).into_owned();
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
        if holes.is_empty() {
            // A plain literal: static bytes, no allocation.
            return Ok(Flow::Val(Value::Str(out)));
        }
        // s153 (#308/#310): a hole makes this a BUILT str — the same
        // fresh allocation `+` is ([type.str.concat]), charged.
        Ok(Flow::Val(self.mint_str(out, e.span)?))
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
        unsigned: bool,
        rendered: String,
    ) -> Result<String, Stop> {
        use wolf_sema::fmtspec::{self, FmtValue};
        // A computed spec (`{x:{w}}`) has no pinned semantics.
        if spec.nodes().any(|n| n.kind == SyntaxKind::Interp) {
            return Err(Stop::Refuse(NotYet {
                construct: "a computed format spec",
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
        // A char hole takes the str spec surface (D58): render the
        // character, then fill/align/width apply to its UTF-8 bytes.
        let char_buf;
        let fv = match val {
            Value::Str(s) => FmtValue::Str(s),
            Value::Char(c) => {
                char_buf = c.to_string();
                FmtValue::Str(&char_buf)
            }
            Value::Bool(b) => FmtValue::Bool(*b),
            // Every integer is held as its value except a 64-bit
            // unsigned one, held as its bit pattern (s220,
            // wolf-lang#551): the hole's type says which, exactly the
            // native lane's PACK_UNSIGNED.
            Value::Int(n) => FmtValue::Int {
                v: *n,
                unsigned,
            },
            // `{b:x}` takes the integer spec surface (D72): the octet
            // widened, `ff` at most.
            Value::Byte(b) => FmtValue::Int {
                v: i64::from(*b),
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
    /// The fs family's entry: [`Self::io_fs_builtin_call`] with the
    /// task's host code kept (s200, wolf-lang#407, `[os.fs.error]`) — every
    /// fallible call clears it on entry, and a host refusal leaves its
    /// number behind ([`fs_host_error`]); the total predicates
    /// (`fs_exists`, `fs_is_file`, `fs_is_dir`) leave it alone.
    fn io_fs_builtin(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        if matches!(name, "fs_exists" | "fs_is_file" | "fs_is_dir") {
            return self.io_fs_builtin_call(name, argv, span);
        }
        FS_HOST_ERROR.with(|c| c.set(0));
        let r = self.io_fs_builtin_call(name, argv, span);
        self.last_os_error = FS_HOST_ERROR.with(std::cell::Cell::get);
        r
    }

    fn io_fs_builtin_call(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        use std::io::{Read as _, Write as _};
        fn tag(t: &str) -> Flow {
            raise(Value::ErrTag {
                tag: t.to_string(),
                payload: Vec::new(),
            })
        }
        // s90 widens the map with `exists` (AlreadyExists) and
        // `cross_device` (EXDEV / ERROR_NOT_SAME_DEVICE). Both arrive
        // from `io::ErrorKind` like `not_found`/`denied`, so both take
        // the SAME coarsening: a builtin whose row does not declare
        // the tag reports `io`. (`invalid` is never produced here — it
        // is a caller mistake the machine decides itself, before the
        // host is touched, exactly as the native runtime does.)
        fn errtag(e: &std::io::Error, declared: &[&str]) -> String {
            fs_host_error(e);
            let t = match e.kind() {
                std::io::ErrorKind::NotFound => "not_found",
                std::io::ErrorKind::PermissionDenied => "denied",
                std::io::ErrorKind::AlreadyExists => "exists",
                std::io::ErrorKind::CrossesDevices => "cross_device",
                _ => "io",
            };
            if declared.contains(&t) { t } else { "io" }.to_string()
        }
        /// A `SystemTime` as ms from the Unix epoch, negative before
        /// it — `time_unix_ms`'s unit, so the two compare. `None` when
        /// it does not fit an `i64` (the `io` row). Identical to
        /// `wolf_rt::fs::unix_ms`.
        fn unix_ms(t: std::time::SystemTime) -> Option<i64> {
            match t.duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => i64::try_from(d.as_millis()).ok(),
                Err(before) => i64::try_from(before.duration().as_millis())
                    .ok()
                    .map(|ms| -ms),
            }
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
        // s215 (`[os.fs.chdir]`): a path argument, resolved against the
        // machine-local working directory once the program has moved it.
        let base = self.cwd.clone();
        let path_arg = |i: usize| -> Option<String> { str_arg(i).map(|p| resolve_in(&base, p)) };
        match name {
            "read_line" => {
                if self.stdin_pos >= self.stdin.len() {
                    return Ok(tag("eof"));
                }
                // Bytes, not chars: since s200 (#405) a byte read of
                // descriptor 0 shares this buffer and may stop inside a
                // scalar, so a line here can begin mid-sequence — the
                // `utf8` row, as the native `read_line` answers it.
                let rest = &self.stdin.as_bytes()[self.stdin_pos..];
                let (line, consumed) = match rest.iter().position(|&b| b == b'\n') {
                    Some(i) => (&rest[..i], i + 1),
                    None => (rest, rest.len()),
                };
                let line = line.strip_suffix(b"\r").unwrap_or(line).to_vec();
                self.stdin_pos += consumed;
                let Ok(line) = String::from_utf8(line) else {
                    return Ok(tag("utf8"));
                };
                self.charge_mem(line.len() as u64)?;
                Ok(Flow::Val(Value::Str(line)))
            }
            "fs_read_text" => {
                let Some(path) = path_arg(0) else {
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
                let (Some(path), Some(contents)) = (path_arg(0), str_arg(1)) else {
                    return self.refuse("this fs call shape", span);
                };
                match std::fs::write(&path, contents.as_bytes()) {
                    Err(e) => Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // s90/#52: one moded open under three spellings.
            // `fs_open`/`fs_create` ARE modes 0 and 1 — they were the
            // two modes s38 happened to have — so widening the entry
            // left every existing call site exact. Mode 2 is a real
            // append handle, which is what stops `std.fs.append_text`
            // reading the file it appends to.
            "fs_open" | "fs_create" | "fs_open_mode" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                let mode = match name {
                    "fs_open" => 0,
                    "fs_create" => 1,
                    _ => match int_arg(1) {
                        Some(m) => m,
                        None => return self.refuse("this fs call shape", span),
                    },
                };
                let mut o = std::fs::OpenOptions::new();
                let opts = match mode {
                    0 => o.read(true),
                    1 => o.write(true).create(true).truncate(true),
                    2 => o.append(true).create(true),
                    3 => o.read(true).write(true).create(true),
                    4 => o.read(true).write(true).create_new(true),
                    // s149 (#289, `[os.fs.open]`): mode 5 is mode 0
                    // carrying `O_NONBLOCK` where the host has it, so
                    // an open cannot park on a fifo nobody writes.
                    // The checked machine opens the same file the
                    // same way — parity by construction, and the
                    // handle it hands back behaves as mode 0's on
                    // every regular file.
                    5 => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::OpenOptionsExt as _;
                            o.read(true).custom_flags(libc::O_NONBLOCK)
                        }
                        #[cfg(not(unix))]
                        {
                            o.read(true)
                        }
                    }
                    // Decided before the filesystem is touched, and
                    // `invalid` is only in `fs_open_mode`'s row: the
                    // 1-argument spellings cannot reach it.
                    _ => return Ok(tag("invalid")),
                };
                let declared: &[&str] = match name {
                    "fs_open" => &["not_found", "denied", "io"],
                    "fs_create" => &["denied", "io"],
                    _ => &["not_found", "denied", "exists", "invalid", "io"],
                };
                match opts.open(&path) {
                    Err(e) => Ok(tag(&errtag(&e, declared))),
                    Ok(f) => {
                        // s199 (#424, `[os.fs.std]`): 0, 1 and 2 are the
                        // standard streams, so the first open is 3 —
                        // `wolf_rt::fs::FIRST_HANDLE`, the same table.
                        if self.files.len() < FS_FIRST_HANDLE {
                            self.files.resize_with(FS_FIRST_HANDLE, || None);
                        }
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
                // s200 (#405, `[os.fs.std]`): 0, 1 and 2 are the
                // standard streams ([`Self::std_read`]).
                if fs_is_std(fd) {
                    if max <= 0 {
                        return Ok(Flow::Val(Value::Str(String::new())));
                    }
                    return match self.std_read(fd, (max as u64).min(1 << 20) as usize) {
                        None => Ok(tag("io")),
                        Some(Err(e)) => Ok(tag(&errtag(&e, &["io"]))),
                        Some(Ok(buf)) if buf.is_empty() => Ok(tag("eof")),
                        Some(Ok(buf)) => {
                            self.charge_mem(buf.len() as u64)?;
                            match String::from_utf8(buf) {
                                Ok(s) => Ok(Flow::Val(Value::Str(s))),
                                Err(_) => Ok(tag("utf8")),
                            }
                        }
                    };
                }
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
                // s200 (#405, `[os.fs.std]`): [`Self::std_write`].
                if fs_is_std(fd) {
                    return match self.std_write(fd, s.as_bytes()) {
                        None => Ok(tag("io")),
                        Some(Err(e)) => Ok(tag(&errtag(&e, &["io"]))),
                        Some(Ok(())) => Ok(Flow::Val(Value::Unit)),
                    };
                }
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
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                match std::fs::remove_file(&path) {
                    Err(e) => Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            "fs_exists" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                Ok(Flow::Val(Value::Bool(std::path::Path::new(&path).exists())))
            }
            // --------------------------------- s90 (#51): bytes --
            "fs_read_bytes" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                match std::fs::read(&path) {
                    Err(e) => Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(bytes) => {
                        self.charge_mem(bytes.len() as u64)?;
                        // No UTF-8 gate: bytes are bytes. This is the
                        // entry `copy_file` should always have had.
                        Ok(Flow::Val(self.byte_list_value(&bytes, span)?))
                    }
                }
            }
            "fs_write_bytes" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                let bytes = match self.bytes_of(argv.get(1)) {
                    None => return self.refuse("this fs call shape", span),
                    Some(Err(())) => return Ok(tag("invalid")),
                    Some(Ok(b)) => b,
                };
                match std::fs::write(&path, &bytes) {
                    Err(e) => Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            "fs_read_chunk" => {
                let (Some(fd), Some(max)) = (int_arg(0), int_arg(1)) else {
                    return self.refuse("this fs call shape", span);
                };
                // The HANDLE before the size, `fs_read`'s order: a
                // forged fd is `io` whatever `max` says. (The native
                // shim checks in the same order — s90 aligned the two
                // after finding `fs_read` disagreed with itself
                // across the lanes at `max <= 0`.)
                // s200 (#405, `[os.fs.std]`): 0, 1 and 2 are the
                // standard streams ([`Self::std_read`]).
                if fs_is_std(fd) {
                    if max <= 0 {
                        return Ok(Flow::Val(self.byte_list_value(&[], span)?));
                    }
                    return match self.std_read(fd, (max as u64).min(1 << 20) as usize) {
                        None => Ok(tag("io")),
                        Some(Err(e)) => Ok(tag(&errtag(&e, &["io"]))),
                        Some(Ok(buf)) if buf.is_empty() => Ok(tag("eof")),
                        Some(Ok(buf)) => {
                            self.charge_mem(buf.len() as u64)?;
                            Ok(Flow::Val(self.byte_list_value(&buf, span)?))
                        }
                    };
                }
                if !self.fd_open(fd) {
                    return Ok(tag("io"));
                }
                if max <= 0 {
                    return Ok(Flow::Val(self.byte_list_value(&[], span)?));
                }
                let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| self.files.get_mut(i))
                else {
                    return Ok(tag("io"));
                };
                // The `fs_read` clamp, byte for byte — the only
                // difference is that a boundary here cannot land
                // inside a code point.
                let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
                match f.read(&mut buf) {
                    Err(e) => Ok(tag(&errtag(&e, &["io"]))),
                    Ok(0) => Ok(tag("eof")),
                    Ok(n) => {
                        buf.truncate(n);
                        self.charge_mem(n as u64)?;
                        Ok(Flow::Val(self.byte_list_value(&buf, span)?))
                    }
                }
            }
            "fs_write_chunk" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                let bytes = match self.bytes_of(argv.get(1)) {
                    None => return self.refuse("this fs call shape", span),
                    Some(Err(())) => return Ok(tag("invalid")),
                    Some(Ok(b)) => b,
                };
                // s200 (#405, `[os.fs.std]`): [`Self::std_write`].
                if fs_is_std(fd) {
                    return match self.std_write(fd, &bytes) {
                        None => Ok(tag("io")),
                        Some(Err(e)) => Ok(tag(&errtag(&e, &["io"]))),
                        Some(Ok(())) => Ok(Flow::Val(Value::Unit)),
                    };
                }
                let Some(Some(f)) = usize::try_from(fd).ok().and_then(|i| self.files.get_mut(i))
                else {
                    return Ok(tag("io"));
                };
                match f.write_all(&bytes) {
                    Err(e) => Ok(tag(&errtag(&e, &["io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // ---------------------------- s90 (#51): directories --
            "fs_read_dir" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                let entries = match std::fs::read_dir(&path) {
                    Err(e) => return Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(rd) => rd,
                };
                let mut names: Vec<String> = Vec::new();
                for entry in entries {
                    match entry {
                        Err(e) => return Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                        Ok(e) => match e.file_name().into_string() {
                            Ok(n) => names.push(n),
                            // A name this str tier cannot hold fails
                            // the listing rather than vanishing from
                            // it (see `wolf_rt::fs`'s decision note).
                            Err(_) => return Ok(tag("utf8")),
                        },
                    }
                }
                // SORTED — the decision, in both lanes, for the same
                // reason: filesystem order is not a property a test
                // can depend on.
                names.sort();
                self.charge_mem(names.iter().map(|n| n.len() as u64).sum())?;
                let items: Vec<Value> = names.into_iter().map(Value::Str).collect();
                let id = self.mint_list(items, span)?;
                Ok(Flow::Val(Value::List(id)))
            }
            "fs_create_dir" | "fs_create_dir_all" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                let (r, declared): (_, &[&str]) = if name == "fs_create_dir" {
                    (
                        std::fs::create_dir(&path),
                        &["exists", "not_found", "denied", "io"],
                    )
                } else {
                    (std::fs::create_dir_all(&path), &["denied", "io"])
                };
                match r {
                    Err(e) => Ok(tag(&errtag(&e, declared))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            "fs_remove_dir" | "fs_remove_dir_all" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                let r = if name == "fs_remove_dir" {
                    std::fs::remove_dir(&path)
                } else {
                    std::fs::remove_dir_all(&path)
                };
                match r {
                    Err(e) => Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // -------------------------------- s90 (#51): rename --
            "fs_rename" => {
                let (Some(from), Some(to)) = (path_arg(0), path_arg(1)) else {
                    return self.refuse("this fs call shape", span);
                };
                match std::fs::rename(&from, &to) {
                    Err(e) => Ok(tag(&errtag(
                        &e,
                        &["not_found", "denied", "cross_device", "exists", "io"],
                    ))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // ------------------------------ s90 (#51): metadata --
            "fs_is_file" | "fs_is_dir" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                // TOTAL like `fs_exists`: an unreadable path is
                // neither, and never a row.
                let md = std::fs::metadata(&path);
                let yes = md
                    .map(|m| {
                        if name == "fs_is_file" {
                            m.is_file()
                        } else {
                            m.is_dir()
                        }
                    })
                    .unwrap_or(false);
                Ok(Flow::Val(Value::Bool(yes)))
            }
            "fs_size" | "fs_modified_ms" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                let md = match std::fs::metadata(&path) {
                    Err(e) => return Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(m) => m,
                };
                let v = if name == "fs_size" {
                    i64::try_from(md.len()).ok()
                } else {
                    md.modified().ok().and_then(unix_ms)
                };
                match v {
                    Some(n) => Ok(Flow::Val(Value::Int(n))),
                    None => Ok(tag("io")),
                }
            }
            // s142 (#261, `[os.fs.fstat]`): `fs_fstat(fd) -> List[int]
            // ! {not_found, denied, io}` — `[kind, size, modified_ms]`
            // from ONE `metadata()` on the handle (kind 0 file, 1
            // directory, 2 anything else). A closed or forged handle
            // is `io`, the family's rule; a size or a time outside
            // `i64` is `io`, `fs_size`/`fs_modified_ms`'s rule.
            "fs_fstat" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                // s199 (#424): 0, 1 and 2 are the standard streams.
                let Some(f) = self.fs_handle(fd) else {
                    return Ok(tag("io"));
                };
                let md = match f.metadata() {
                    Err(e) => return Ok(tag(&errtag(&e, &["not_found", "denied", "io"]))),
                    Ok(m) => m,
                };
                // windows: a handle that is not a disk file is `kind` 2
                // whatever std's metadata calls it (`fs_is_disk`).
                let kind = if md.is_file() && fs_is_disk(&f) {
                    0
                } else if md.is_dir() {
                    1
                } else {
                    2
                };
                let (Ok(size), Some(ms)) = (
                    i64::try_from(md.len()),
                    md.modified().ok().and_then(unix_ms),
                ) else {
                    return Ok(tag("io"));
                };
                let items = vec![Value::Int(kind), Value::Int(size), Value::Int(ms)];
                self.charge_mem(24)?;
                let id = self.mint_list(items, span)?;
                Ok(Flow::Val(Value::List(id)))
            }
            // s199 (#426, `[os.fs.seek]`, `[os.fs.tell]`,
            // `[os.fs.read_at]`): the handle's offset, `wolf_rt::fs`'s
            // three calls row for row. `unseekable` is `ESPIPE`
            // (`ErrorKind::NotSeekable`); a whence outside {0, 1, 2}
            // or a start offset below zero is `invalid` before the
            // host is touched, a result below zero is the host's
            // `EINVAL` (`InvalidInput`), also `invalid`.
            "fs_seek" | "fs_tell" => {
                use std::io::{Seek as _, SeekFrom};
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this fs call shape", span);
                };
                // `fs_tell` is a seek of nothing from the cursor.
                let (off, whence) = if name == "fs_tell" {
                    (0, 1)
                } else {
                    let (Some(off), Some(whence)) = (int_arg(1), int_arg(2)) else {
                        return self.refuse("this fs call shape", span);
                    };
                    (off, whence)
                };
                // The handle first, the whence second — the runtime's
                // order, so a forged handle is `io` whatever `whence`.
                let Some(f) = self.fs_handle(fd) else {
                    return Ok(tag("io"));
                };
                let to = match whence {
                    0 => match u64::try_from(off) {
                        Ok(o) => SeekFrom::Start(o),
                        Err(_) => return Ok(tag("invalid")),
                    },
                    1 => SeekFrom::Current(off),
                    2 => SeekFrom::End(off),
                    _ => return Ok(tag("invalid")),
                };
                #[cfg(windows)]
                {
                    if !fs_is_disk(&f) {
                        return Ok(tag("unseekable"));
                    }
                    let base = match to {
                        SeekFrom::Start(_) => 0,
                        SeekFrom::Current(_) => match (&*f).stream_position() {
                            Ok(p) => i128::from(p),
                            Err(_) => return Ok(tag("io")),
                        },
                        SeekFrom::End(_) => match f.metadata() {
                            Ok(m) => i128::from(m.len()),
                            Err(_) => return Ok(tag("io")),
                        },
                    };
                    if let SeekFrom::Current(o) | SeekFrom::End(o) = to
                        && base + i128::from(o) < 0
                    {
                        return Ok(tag("invalid"));
                    }
                }
                let r = (&*f).seek(to);
                drop(f);
                match r {
                    Err(e) => {
                        fs_host_error(&e);
                        Ok(tag(match e.kind() {
                            std::io::ErrorKind::NotSeekable => "unseekable",
                            std::io::ErrorKind::InvalidInput if name == "fs_seek" => "invalid",
                            _ => "io",
                        }))
                    }
                    Ok(at) => match i64::try_from(at) {
                        Ok(at) => Ok(Flow::Val(Value::Int(at))),
                        Err(_) => Ok(tag("io")),
                    },
                }
            }
            "fs_read_at" => {
                let (Some(fd), Some(off), Some(max)) = (int_arg(0), int_arg(1), int_arg(2)) else {
                    return self.refuse("this fs call shape", span);
                };
                // The family's order: the handle, the offset, `max`.
                let Some(f) = self.fs_handle(fd) else {
                    return Ok(tag("io"));
                };
                let Ok(off) = u64::try_from(off) else {
                    return Ok(tag("invalid"));
                };
                if max <= 0 {
                    drop(f);
                    return Ok(Flow::Val(self.byte_list_value(&[], span)?));
                }
                #[cfg(windows)]
                {
                    if !fs_is_disk(&f) {
                        return Ok(tag("unseekable"));
                    }
                }
                let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
                let r = fs_read_at_host(&f, &mut buf, off);
                drop(f);
                match r {
                    Err(e) => {
                        fs_host_error(&e);
                        Ok(tag(match e.kind() {
                            std::io::ErrorKind::NotSeekable => "unseekable",
                            _ => "io",
                        }))
                    }
                    Ok(0) => Ok(tag("eof")),
                    Ok(n) => {
                        buf.truncate(n);
                        self.charge_mem(n as u64)?;
                        Ok(Flow::Val(self.byte_list_value(&buf, span)?))
                    }
                }
            }
            // s200 (#417, `[os.fs.copy]`): one transfer through this
            // machine's own buffer — the observable bytes and offsets of
            // the native rungs, without a kernel path (the host process
            // is the checked program; its capture of 1 and 2 is the
            // record's). The count per call may differ from a native
            // rung's, which the clause leaves unpinned: at least one
            // byte, at most `max`, `eof` at the end.
            "fs_copy_chunk" => {
                let (Some(src), Some(dst), Some(max)) = (int_arg(0), int_arg(1), int_arg(2)) else {
                    return self.refuse("this fs call shape", span);
                };
                // Both handles first, then `max` — the family's order.
                if !(fs_is_std(src) || self.fd_open(src)) || !(fs_is_std(dst) || self.fd_open(dst))
                {
                    return Ok(tag("io"));
                }
                if max <= 0 {
                    return Ok(Flow::Val(Value::Int(0)));
                }
                let want = (max as u64).min(1 << 20) as usize;
                let bytes = if fs_is_std(src) {
                    match self.std_read(src, want) {
                        None => return Ok(tag("io")),
                        Some(Err(e)) => return Ok(tag(&errtag(&e, &["io"]))),
                        Some(Ok(b)) => b,
                    }
                } else {
                    let Some(Some(f)) = usize::try_from(src)
                        .ok()
                        .and_then(|i| self.files.get_mut(i))
                    else {
                        return Ok(tag("io"));
                    };
                    let mut buf = vec![0u8; want];
                    match f.read(&mut buf) {
                        Err(e) => return Ok(tag(&errtag(&e, &["io"]))),
                        Ok(n) => {
                            buf.truncate(n);
                            buf
                        }
                    }
                };
                if bytes.is_empty() {
                    return Ok(tag("eof"));
                }
                let wrote = if fs_is_std(dst) {
                    self.std_write(dst, &bytes)
                } else {
                    usize::try_from(dst)
                        .ok()
                        .and_then(|i| self.files.get_mut(i))
                        .and_then(Option::as_mut)
                        .map(|f| f.write_all(&bytes))
                };
                match wrote {
                    None => Ok(tag("io")),
                    Some(Err(e)) => Ok(tag(&errtag(&e, &["io"]))),
                    Some(Ok(())) => Ok(Flow::Val(Value::Int(bytes.len() as i64))),
                }
            }
            _ => self.refuse("this io/fs builtin", span),
        }
    }

    /// s200 (#405, `[os.fs.std]`): one read of at most `want` bytes from
    /// standard stream `fd`. Descriptor 0 is the input buffer a test
    /// hands the machine when it hands one (`read_line`'s, shared, so
    /// the two never disagree about where input stands), and the `wolf`
    /// process's own descriptor 0 otherwise — every production caller
    /// hands none, so `conform-run --checked` reads the real stream,
    /// as `[os.fs.std]`'s offset calls already do. `None` is a stream
    /// the process does not have (`io`); an interrupted read retries.
    fn std_read(&mut self, fd: i64, want: usize) -> Option<std::io::Result<Vec<u8>>> {
        use std::io::Read as _;
        if fd == 0 && !self.stdin.is_empty() {
            let rest = &self.stdin.as_bytes()[self.stdin_pos.min(self.stdin.len())..];
            let n = rest.len().min(want);
            let out = rest[..n].to_vec();
            self.stdin_pos += n;
            return Some(Ok(out));
        }
        let f = std_stream_dup(fd)?;
        let mut buf = vec![0u8; want];
        let mut g = &f;
        loop {
            match g.read(&mut buf) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Some(Err(e)),
                Ok(n) => {
                    buf.truncate(n);
                    return Some(Ok(buf));
                }
            }
        }
    }

    /// s200 (#405, `[os.fs.std]`): write all of `bytes` to standard
    /// stream `fd`. 1 and 2 are this machine's CAPTURE (the record's
    /// stdout and the `eprint` channel), in program order with `print`
    /// by construction; 0 is the `wolf` process's descriptor.
    fn std_write(&mut self, fd: i64, bytes: &[u8]) -> Option<std::io::Result<()>> {
        use std::io::Write as _;
        match fd {
            1 => {
                self.stdout.extend_from_slice(bytes);
                Some(Ok(()))
            }
            2 => {
                self.stderr.extend_from_slice(bytes);
                Some(Ok(()))
            }
            _ => {
                let f = std_stream_dup(fd)?;
                let mut g = &f;
                Some(g.write_all(bytes))
            }
        }
    }

    /// s200 (#411, `[mem.list.bytes]`): `bytes_find(xs, b, from) -> int
    /// ! {none}` and `bytes_count(xs, b) -> int` over this machine's
    /// list — the native shims' answers element for element: `none` for
    /// an absent byte or a `from` outside `0..len`.
    fn byte_scan(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        let Some(Ok(xs)) = self.bytes_of(argv.first()) else {
            return self.refuse("this byte-scan call shape", span);
        };
        let Some(Value::Byte(b)) = argv.get(1) else {
            return self.refuse("this byte-scan call shape", span);
        };
        if name == "bytes_count" {
            let n = xs.iter().filter(|&&x| x == *b).count();
            return Ok(Flow::Val(Value::Int(n as i64)));
        }
        let Some(Value::Int(from)) = argv.get(2) else {
            return self.refuse("this byte-scan call shape", span);
        };
        let hit = usize::try_from(*from)
            .ok()
            .filter(|&f| f < xs.len())
            .and_then(|f| xs[f..].iter().position(|&x| x == *b).map(|i| f + i));
        match hit {
            Some(i) => Ok(Flow::Val(Value::Int(i as i64))),
            None => Ok(raise(Value::ErrTag {
                tag: "none".to_string(),
                payload: Vec::new(),
            })),
        }
    }

    /// s199 (#424, `[os.fs.std]`): the file `fd` names — for 0, 1 and
    /// 2 the machine's own standard stream (the `wolf` process's, which
    /// is the checked program's), duplicated for this one call so the
    /// offset is shared and nothing the machine holds is ever closed;
    /// the table's slot otherwise. `None` is a closed or forged handle,
    /// or a standard stream the process does not have (`io`).
    fn fs_handle(&self, fd: i64) -> Option<FsHandle<'_>> {
        if (0..FS_FIRST_HANDLE as i64).contains(&fd) {
            return std_stream_dup(fd).map(FsHandle::Std);
        }
        match usize::try_from(fd).ok().and_then(|i| self.files.get(i)) {
            Some(Some(f)) => Some(FsHandle::Table(f)),
            _ => None,
        }
    }

    /// Is `fd` a live handle in the machine's table? A closed or
    /// forged one is the `io` row, never a trap.
    fn fd_open(&self, fd: i64) -> bool {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.files.get(i))
            .is_some_and(Option::is_some)
    }

    /// A `List[int]` argument as bytes. `None` is a call shape sema
    /// rules out; `Some(Err(()))` is an element that is not a byte —
    /// the `invalid` row, and the same refusal `str_from_utf8` makes
    /// with a different name on it (writing is not decoding).
    /// The octets of a `List[byte]` argument (s136, wolf-lang#231): a
    /// `Value::Byte` element IS a byte, so typed code can never reach
    /// the `invalid` row here — it stays for an `int` element (the
    /// pre-s136 carrier, unreachable through sema now) outside
    /// `0..=255`, the native shim's wrong-width refusal mirrored.
    fn bytes_of(&self, v: Option<&Value>) -> Option<Result<Vec<u8>, ()>> {
        let Some(Value::List(id)) = v else {
            return None;
        };
        let mut out = Vec::with_capacity(self.lists[*id].len());
        for e in &self.lists[*id] {
            match e {
                Value::Byte(b) => out.push(*b),
                Value::Int(n) => match u8::try_from(*n) {
                    Ok(b) => out.push(b),
                    Err(_) => return Some(Err(())),
                },
                _ => return None,
            }
        }
        Some(Ok(out))
    }

    /// Bytes as a fresh `List[byte]` value (s136): one `Value::Byte`
    /// per octet, so the list charges its region one byte per byte
    /// (`slot_bytes`) — the 1x wolf-lang#203 asked this tier for.
    fn byte_list_value(&mut self, bytes: &[u8], span: Span) -> E<Value> {
        let items: Vec<Value> = bytes.iter().map(|&b| Value::Byte(b)).collect();
        let id = self.mint_list(items, span)?;
        Ok(Value::List(id))
    }

    /// Bytes as a fresh `List[int]` value — the pre-s136 carrier, kept
    /// for the one producer whose clause still says `List[int]`:
    /// `os_random` (`[os.random]`; not a byte surface, not one of
    /// #231's eight).
    fn int_list_value(&mut self, bytes: &[u8], span: Span) -> E<Value> {
        let items: Vec<Value> = bytes.iter().map(|&b| Value::Int(i64::from(b))).collect();
        let id = self.mint_list(items, span)?;
        Ok(Value::List(id))
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
        fn tag(t: &str) -> Flow {
            raise(Value::ErrTag {
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
        // s215: a unix-domain path resolves like an fs path.
        let base = self.cwd.clone();
        let path_arg = |i: usize| -> Option<String> { str_arg(i).map(|p| resolve_in(&base, p)) };
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
            // s136 (#227, `[os.net.unix]`): the unix-domain family —
            // real `AF_UNIX` sockets on a unix host, the rows by name
            // (`exists` for a stale path at bind, `not_found` for no
            // path at dial, `denied`, `refused` for a path nobody
            // listens on); on a host the machine does not serve,
            // `unsupported`, by name — never a bare `io`.
            "net_listen_unix" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                #[cfg(unix)]
                {
                    match std::os::unix::net::UnixListener::bind(&path) {
                        Err(e) => Ok(tag(&coarse(
                            e.kind(),
                            &["exists", "not_found", "denied", "io"],
                        ))),
                        Ok(l) => {
                            let fd = self.socks.len() as i64;
                            self.socks.push(Some(NetSock::UnixListener(
                                l,
                                std::path::PathBuf::from(path),
                            )));
                            Ok(Flow::Val(Value::Int(fd)))
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = path;
                    Ok(tag("unsupported"))
                }
            }
            "net_connect_unix" => {
                let Some(path) = path_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                #[cfg(unix)]
                {
                    match std::os::unix::net::UnixStream::connect(&path) {
                        Err(e) => Ok(tag(&coarse(
                            e.kind(),
                            &["refused", "not_found", "denied", "io"],
                        ))),
                        Ok(s) => {
                            let fd = self.socks.len() as i64;
                            self.socks.push(Some(NetSock::UnixStream(s)));
                            Ok(Flow::Val(Value::Int(fd)))
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = path;
                    Ok(tag("unsupported"))
                }
            }
            // s137 (#234, `[os.net.listen.opts]`): the listener
            // options, served — a socket option is host state the
            // checked machine holds like any other; the hand mirror of
            // `wolf_rt::net::bind_with` sets `SO_REUSEPORT` before the
            // bind. Rows by name: `exists` for a held address, `denied`
            // for a privileged port; windows refuses `reuse_port` by
            // name (`SO_REUSEADDR` is not it) and serves the option-
            // less shape through std's bind.
            "net_listen_with" => {
                let (Some(addr), Some(Value::Bool(reuse)), Some(backlog)) =
                    (str_arg(0), argv.get(1), int_arg(2))
                else {
                    return self.refuse("this net call shape", span);
                };
                #[cfg(unix)]
                let bound = checked_bind_with(&addr, *reuse, backlog);
                #[cfg(not(unix))]
                let bound = {
                    let _ = backlog;
                    if *reuse {
                        return Ok(tag("unsupported"));
                    }
                    std::net::TcpListener::bind(&addr)
                };
                match bound {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["exists", "denied", "io"]))),
                    Ok(l) => {
                        let fd = self.socks.len() as i64;
                        self.socks.push(Some(NetSock::Listener(l)));
                        Ok(Flow::Val(Value::Int(fd)))
                    }
                }
            }
            // s137 (#235, `[os.proc.inherit]`): the checked machine
            // runs no descriptor handoff — it is the `wolf` binary
            // interpreting a program, so a "child of this program"
            // would be a child of the compiler — and says so BY NAME
            // (the s134 records rule: the record names the construct;
            // `os_spawn_with`'s non-empty set is the other half).
            "net_adopt_listener" => self.refuse("listener adoption in checked execution", span),
            // s137 (#127, `[os.net.wait]`): readiness over a SET — the
            // primitive a spawn-free serving loop blocks on instead of
            // time-slicing every socket. Served here like every other
            // net call: real host operations, D30 rows, the comptime
            // sandbox the one refusal site. The empty answer is the
            // deadline expiring with nothing ready — an answer, not a
            // row; `io` is a forged handle or a wait nothing could
            // end. Every tier-1 host answers this question, so unlike
            // the rest of s137 there is no per-host row: `poll(2)` on
            // unix, `WSAPoll` on windows.
            "net_wait" => {
                let (Some(Value::List(set_id)), Some(deadline)) = (argv.first(), int_arg(1)) else {
                    return self.refuse("this net call shape", span);
                };
                let mut handles: Vec<i64> = Vec::new();
                for v in self.lists.get(*set_id).into_iter().flatten() {
                    match v {
                        Value::Int(h) => handles.push(*h),
                        _ => return self.refuse("a non-int handle in a net_wait set", span),
                    }
                }
                {
                    #[cfg(unix)]
                    use std::os::fd::AsRawFd as _;
                    #[cfg(windows)]
                    use std::os::windows::io::AsRawSocket as _;
                    if handles.is_empty() {
                        if deadline < 0 {
                            return Ok(tag("io"));
                        }
                        let id = self.mint_list(Vec::new(), span)?;
                        return Ok(Flow::Val(Value::List(id)));
                    }
                    let mut raws = Vec::with_capacity(handles.len());
                    for &h in &handles {
                        let live = usize::try_from(h)
                            .ok()
                            .and_then(|i| self.socks.get(i))
                            .and_then(|s| s.as_ref());
                        let Some(sock) = live else {
                            return Ok(tag("io"));
                        };
                        #[cfg(unix)]
                        raws.push(match sock {
                            NetSock::Listener(l) => l.as_raw_fd(),
                            NetSock::Stream(st) => st.as_raw_fd(),
                            NetSock::UnixListener(l, _) => l.as_raw_fd(),
                            NetSock::UnixStream(st) => st.as_raw_fd(),
                        });
                        #[cfg(windows)]
                        raws.push(match sock {
                            NetSock::Listener(l) => l.as_raw_socket(),
                            NetSock::Stream(st) => st.as_raw_socket(),
                        });
                    }
                    let timeout = if deadline < 0 {
                        -1
                    } else {
                        i32::try_from(deadline).unwrap_or(i32::MAX)
                    };
                    let Ok(flags) = checked_poll_readable(&raws, timeout) else {
                        return Ok(tag("io"));
                    };
                    let ready: Vec<Value> = handles
                        .iter()
                        .zip(flags)
                        .filter_map(|(&h, r)| r.then_some(Value::Int(h)))
                        .collect();
                    let id = self.mint_list(ready, span)?;
                    Ok(Flow::Val(Value::List(id)))
                }
            }
            "net_port" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                let addr = match self.sock(fd) {
                    Some(NetSock::Listener(l)) => l.local_addr(),
                    Some(NetSock::Stream(s)) => s.local_addr(),
                    // A unix-domain socket has no port: `io`.
                    _ => return Ok(tag("io")),
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
                let budget = self.sock_deadlines.get(&fd).copied();
                let accepted = match self.sock(fd) {
                    // s106: an armed budget bounds the park — the
                    // `timeout` tag, reachable. Either family (s136).
                    Some(l) if !l.is_stream() => l.accept_with(budget),
                    _ => return Ok(tag("io")),
                };
                match accepted.and_then(NetSock::under_posture) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["timeout", "io"]))),
                    Ok(s) => {
                        let fd = self.socks.len() as i64;
                        self.socks.push(Some(s));
                        Ok(Flow::Val(Value::Int(fd)))
                    }
                }
            }
            "net_connect" => {
                let Some(addr) = str_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                match std::net::TcpStream::connect(&addr)
                    .and_then(|s| NetSock::Stream(s).under_posture())
                {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["refused", "timeout", "io"]))),
                    Ok(s) => {
                        let fd = self.socks.len() as i64;
                        self.socks.push(Some(s));
                        Ok(Flow::Val(Value::Int(fd)))
                    }
                }
            }
            "net_read" => {
                let (Some(fd), Some(max)) = (int_arg(0), int_arg(1)) else {
                    return self.refuse("this net call shape", span);
                };
                let Some(s) = self.sock(fd).filter(|s| s.is_stream()) else {
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
                let Some(s) = self.sock(fd).filter(|s| s.is_stream()) else {
                    return Ok(tag("io"));
                };
                match s.write_all(payload.as_bytes()) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["closed", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // s115/#137: the byte twins. No UTF-8 gate on read (bytes
            // are bytes — a `List[int]`); the write refuses an
            // out-of-range element as `invalid` before any syscall, the
            // `fs_write_bytes` posture over the socket.
            "net_read_bytes" => {
                let (Some(fd), Some(max)) = (int_arg(0), int_arg(1)) else {
                    return self.refuse("this net call shape", span);
                };
                let Some(s) = self.sock(fd).filter(|s| s.is_stream()) else {
                    return Ok(tag("io"));
                };
                if max <= 0 {
                    return Ok(Flow::Val(self.byte_list_value(&[], span)?));
                }
                let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
                match s.read(&mut buf) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["closed", "timeout", "io"]))),
                    Ok(0) => Ok(tag("closed")),
                    Ok(n) => {
                        buf.truncate(n);
                        self.charge_mem(n as u64)?;
                        Ok(Flow::Val(self.byte_list_value(&buf, span)?))
                    }
                }
            }
            "net_write_bytes" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                let bytes = match self.bytes_of(argv.get(1)) {
                    None => return self.refuse("this net call shape", span),
                    Some(Err(())) => return Ok(tag("invalid")),
                    Some(Ok(b)) => b,
                };
                let Some(s) = self.sock(fd).filter(|s| s.is_stream()) else {
                    return Ok(tag("io"));
                };
                match s.write_all(&bytes) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["closed", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // s141 (#254, `[os.net.writev]`): the gather — every
            // element of the `List[List[byte]]` read as a byte list,
            // in order, then one vectored write. The rows are
            // `net_write`'s; a nested list of the wrong shape cannot
            // arrive from typed code, and refuses the call shape.
            "net_writev" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                let Some(Value::List(outer)) = argv.get(1) else {
                    return self.refuse("this net call shape", span);
                };
                let heads: Vec<Value> = self.lists[*outer].clone();
                let mut parts = Vec::with_capacity(heads.len());
                for h in &heads {
                    match self.bytes_of(Some(h)) {
                        Some(Ok(b)) => parts.push(b),
                        _ => return self.refuse("this net call shape", span),
                    }
                }
                let Some(s) = self.sock(fd).filter(|s| s.is_stream()) else {
                    return Ok(tag("io"));
                };
                match s.write_all_vectored(&parts) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["closed", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // s160 (#299, `[os.net.writev.head]`): the gather with a
            // `str` head — the head's bytes first, then every part, in
            // one vectored write. `net_writev`'s rows exactly.
            "net_writev_head" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                let Some(head) = str_arg(1) else {
                    return self.refuse("this net call shape", span);
                };
                let Some(Value::List(outer)) = argv.get(2) else {
                    return self.refuse("this net call shape", span);
                };
                let heads: Vec<Value> = self.lists[*outer].clone();
                let mut parts: Vec<Vec<u8>> = Vec::with_capacity(heads.len() + 1);
                if !head.is_empty() {
                    parts.push(head.into_bytes());
                }
                for h in &heads {
                    match self.bytes_of(Some(h)) {
                        Some(Ok(b)) => parts.push(b),
                        _ => return self.refuse("this net call shape", span),
                    }
                }
                let Some(s) = self.sock(fd).filter(|s| s.is_stream()) else {
                    return Ok(tag("io"));
                };
                match s.write_all_vectored(&parts) {
                    Err(e) => Ok(tag(&coarse(e.kind(), &["closed", "io"]))),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // s141 (#254, `[os.net.nodelay]`): Nagle off or on for a
            // TCP stream; anything else is `io`, never a trap.
            "net_nodelay" => {
                let (Some(fd), Some(Value::Bool(on))) = (int_arg(0), argv.get(1)) else {
                    return self.refuse("this net call shape", span);
                };
                let Some(s) = self.sock(fd).filter(|s| s.is_stream()) else {
                    return Ok(tag("io"));
                };
                match s.set_nodelay(*on) {
                    Err(_) => Ok(tag("io")),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            "net_close" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this net call shape", span);
                };
                self.sock_deadlines.remove(&fd);
                match usize::try_from(fd).ok().and_then(|i| self.socks.get_mut(i)) {
                    Some(slot @ Some(_)) => {
                        // drop closes; double close is `io`. A unix
                        // listener's path is unlinked (s136, the
                        // runtime's cleanup posture, mirrored).
                        let gone = slot.take();
                        #[cfg(unix)]
                        if let Some(NetSock::UnixListener(l, path)) = gone {
                            drop(l);
                            let _ = std::fs::remove_file(path);
                        }
                        #[cfg(not(unix))]
                        drop(gone);
                        Ok(Flow::Val(Value::Unit))
                    }
                    _ => Ok(tag("io")),
                }
            }
            // s106 (#45's builtin half): arm (`ms > 0`) or clear
            // (`ms <= 0`) the socket's deadline budget. Streams hold
            // it on the socket (std's own read/write timeouts — a
            // fired one is `WouldBlock`/`TimedOut`, the `timeout` row
            // via `net_err_tag`); listeners hold it in the side table
            // `accept_deadline` polls. A forged or closed fd is `io`,
            // never a trap.
            "net_deadline" => {
                let (Some(fd), Some(ms)) = (int_arg(0), int_arg(1)) else {
                    return self.refuse("this net call shape", span);
                };
                let budget = u64::try_from(ms)
                    .ok()
                    .filter(|&m| m > 0)
                    .map(std::time::Duration::from_millis);
                if self.sock(fd).is_some_and(|l| !l.is_stream()) {
                    match budget {
                        Some(b) => self.sock_deadlines.insert(fd, b),
                        None => self.sock_deadlines.remove(&fd),
                    };
                    return Ok(Flow::Val(Value::Unit));
                }
                let Some(s) = self.sock(fd).filter(|s| s.is_stream()) else {
                    return Ok(tag("io"));
                };
                match s.set_timeouts(budget) {
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                    // wolf-lang#224: a peer that closed over UNREAD
                    // receive data reset the connection, and on macOS
                    // `setsockopt(SO_RCVTIMEO/SO_SNDTIMEO)` answers
                    // EINVAL on a reset socket (linux keeps answering
                    // 0) while the bytes the peer wrote before its
                    // close are still readable. That is not an `io`
                    // row — the handle is live and its next read
                    // cannot park: it returns the buffered bytes and
                    // then the `closed` row (ConnectionReset). The
                    // budget is therefore honoured trivially; record
                    // it and report the deadline ARMED, which is what
                    // the native reactor's timer wheel (never a
                    // setsockopt) reports on the same socket. A
                    // genuinely dead fd still fails `local_addr`
                    // (EBADF) and stays `io`.
                    Err(e) if e.kind() == std::io::ErrorKind::InvalidInput && s.alive() => {
                        match budget {
                            Some(b) => self.sock_deadlines.insert(fd, b),
                            None => self.sock_deadlines.remove(&fd),
                        };
                        Ok(Flow::Val(Value::Unit))
                    }
                    Err(e) => Ok(tag(&coarse(e.kind(), &["io"]))),
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

    /// The s40 os/env builtin tier (checked lane): real host argv/
    /// env/cwd/processes, D30 rows only. `env_set` writes the
    /// machine-local overlay (never the threaded host process's
    /// environment — the struct field documents the asymmetry);
    /// `os_spawn` is argv-array only; child stdout/stderr INHERIT
    /// (write-through, #129 — the native runtime's wiring, mirrored),
    /// stdin stays null-wired; every
    /// operation on a reaped or foreign child handle is `io`, never a
    /// trap.
    fn os_builtin(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        fn tag(t: &str) -> Flow {
            raise(Value::ErrTag {
                tag: t.to_string(),
                payload: Vec::new(),
            })
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
            "env_args" => {
                let items: Vec<Value> = self.args.iter().cloned().map(Value::Str).collect();
                self.charge_mem(self.args.iter().map(|a| a.len() as u64).sum())?;
                let id = self.mint_list(items, span)?;
                Ok(Flow::Val(Value::List(id)))
            }
            "env_get" => {
                let Some(key) = str_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                if let Some(v) = self.env_overlay.get(&key) {
                    let v = v.clone();
                    self.charge_mem(v.len() as u64)?;
                    return Ok(Flow::Val(Value::Str(v)));
                }
                match std::env::var(&key) {
                    Ok(v) => {
                        self.charge_mem(v.len() as u64)?;
                        Ok(Flow::Val(Value::Str(v)))
                    }
                    Err(std::env::VarError::NotPresent) => Ok(tag("missing")),
                    Err(std::env::VarError::NotUnicode(_)) => Ok(tag("utf8")),
                }
            }
            "env_set" => {
                let (Some(key), Some(val)) = (str_arg(0), str_arg(1)) else {
                    return self.refuse("this os call shape", span);
                };
                if key.is_empty() || key.contains('=') || key.contains('\0') || val.contains('\0') {
                    return Ok(tag("invalid"));
                }
                self.charge_mem((key.len() + val.len()) as u64)?;
                self.env_overlay.insert(key, val);
                Ok(Flow::Val(Value::Unit))
            }
            "env_vars" => {
                // Host vars under the overlay, non-UTF-8 entries
                // skipped (their values are unreachable through this
                // str tier), rendered `K=V` and SORTED — determinism
                // over environ order.
                let mut map: std::collections::BTreeMap<String, String> = std::env::vars_os()
                    .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
                    .collect();
                for (k, v) in &self.env_overlay {
                    map.insert(k.clone(), v.clone());
                }
                let items: Vec<Value> = map
                    .into_iter()
                    .map(|(k, v)| Value::Str(format!("{k}={v}")))
                    .collect();
                let bytes: u64 = items
                    .iter()
                    .map(|v| match v {
                        Value::Str(s) => s.len() as u64,
                        _ => 0,
                    })
                    .sum();
                self.charge_mem(bytes)?;
                let id = self.mint_list(items, span)?;
                Ok(Flow::Val(Value::List(id)))
            }
            // s137 (#233, `[os.cpus]`): the schedulable core count.
            // The checked machine reads the SAME source the runtime
            // does (`available_parallelism`: a cgroup quota and an
            // affinity mask on linux, `sysctl hw.ncpu` on macOS,
            // `GetSystemInfo` on windows), so the two lanes answer the
            // same number on one host — which is the point of a value
            // that decides how many workers a program starts. It is
            // machine state, so a witness pins the RELATION (`>= 1`)
            // and never the number.
            "os_cpus" => match std::thread::available_parallelism() {
                Err(_) => Ok(tag("io")),
                Ok(n) => Ok(Flow::Val(Value::Int(n.get() as i64))),
            },
            // s215: once the program has moved its working directory,
            // the answer is the machine-local one (`[os.fs.chdir]`).
            "os_cwd" if self.cwd.is_some() => {
                let s = self
                    .cwd
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.charge_mem(s.len() as u64)?;
                Ok(Flow::Val(Value::Str(s)))
            }
            "os_cwd" => match std::env::current_dir() {
                Err(_) => Ok(tag("io")),
                Ok(p) => match p.to_str() {
                    // A non-UTF-8 cwd is unreachable through the str
                    // tier: `io`, same coarsening rule as fs.
                    None => Ok(tag("io")),
                    Some(s) => {
                        self.charge_mem(s.len() as u64)?;
                        Ok(Flow::Val(Value::Str(s.to_string())))
                    }
                },
            },
            // s90/#69: the running executable's path — `os_cwd`'s
            // shape, and the reason std.process's rig can spawn
            // ITSELF instead of hunting for a host-universal binary.
            // In the CHECKED lane the running executable is the test
            // host, not the wolf program; that is the same asymmetry
            // `env_args` already carries and it is what makes the
            // answer spawnable on both lanes.
            "os_exe" => match std::env::current_exe() {
                Err(_) => Ok(tag("io")),
                Ok(p) => match p.to_str() {
                    None => Ok(tag("io")),
                    Some(s) => {
                        self.charge_mem(s.len() as u64)?;
                        Ok(Flow::Val(Value::Str(s.to_string())))
                    }
                },
            },
            "os_exit" => {
                let Some(code) = int_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                Err(Stop::Exit(code.rem_euclid(256) as u8))
            }
            "os_spawn" => {
                let Some(Value::List(id)) = argv.first() else {
                    return self.refuse("this os call shape", span);
                };
                let mut words = Vec::new();
                for v in self.lists.get(*id).into_iter().flatten() {
                    match v {
                        Value::Str(s) => words.push(s.clone()),
                        _ => return self.refuse("a non-str argv element", span),
                    }
                }
                // An empty argv names no program: `not_found`.
                let Some((prog, rest)) = words.split_first() else {
                    return Ok(tag("not_found"));
                };
                let spawned = std::process::Command::new(prog)
                    .args(rest)
                    .stdin(std::process::Stdio::null())
                    // Write-through (#129): the child shares the
                    // HOST process's stdout/stderr — this machine's
                    // buffered print stream cannot capture fd-level
                    // writes, a documented asymmetry (capture is the
                    // named upstream ask).
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    // s215: a child starts in the machine-local working
                    // directory once the program has moved it.
                    .current_dir(
                        self.cwd
                            .clone()
                            .unwrap_or_else(|| std::path::PathBuf::from(".")),
                    )
                    .spawn();
                match spawned {
                    Err(e) => Ok(tag(match e.kind() {
                        std::io::ErrorKind::NotFound => "not_found",
                        std::io::ErrorKind::PermissionDenied => "denied",
                        _ => "io",
                    })),
                    Ok(child) => {
                        let h = self.children.len() as i64;
                        self.children.push(Some(child));
                        Ok(Flow::Val(Value::Int(h)))
                    }
                }
            }
            // s137 (#235, `[os.proc.inherit]`): with an EMPTY inherit
            // set this is `os_spawn` with the program named apart from
            // its arguments; a non-empty set is refused BY NAME — the
            // checked machine is the `wolf` binary interpreting the
            // program, and a descriptor handed to "its child" would be
            // handed to the compiler's child (the s134 records rule:
            // the record names the construct, never a bare
            // `unsupported`).
            "os_spawn_with" => {
                let (Some(exe), Some(Value::List(args_id)), Some(Value::List(inherit_id))) =
                    (str_arg(0), argv.get(1), argv.get(2))
                else {
                    return self.refuse("this os call shape", span);
                };
                let inherited = self.lists.get(*inherit_id).is_some_and(|l| !l.is_empty());
                if inherited {
                    return self.refuse(
                        "fd inheritance across os_spawn_with in checked execution",
                        span,
                    );
                }
                if exe.is_empty() {
                    return Ok(tag("not_found"));
                }
                let mut words = Vec::new();
                for v in self.lists.get(*args_id).into_iter().flatten() {
                    match v {
                        Value::Str(s) => words.push(s.clone()),
                        _ => return self.refuse("a non-str argv element", span),
                    }
                }
                let spawned = std::process::Command::new(&exe)
                    .args(&words)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    // s215: a child starts in the machine-local working
                    // directory once the program has moved it.
                    .current_dir(
                        self.cwd
                            .clone()
                            .unwrap_or_else(|| std::path::PathBuf::from(".")),
                    )
                    .spawn();
                match spawned {
                    Err(e) => Ok(tag(match e.kind() {
                        std::io::ErrorKind::NotFound => "not_found",
                        std::io::ErrorKind::PermissionDenied => "denied",
                        _ => "io",
                    })),
                    Ok(child) => {
                        let h = self.children.len() as i64;
                        self.children.push(Some(child));
                        Ok(Flow::Val(Value::Int(h)))
                    }
                }
            }
            // s215 (`[os.proc.fds]`, pelt's H2): the spawn with a
            // descriptor map, SERVED here — unlike `os_spawn_with`'s net
            // set. Every source is a handle of the machine's own fs table
            // (a pipe end, a file the program opened) or one of 0..2, so
            // what the child receives is exactly what the program made;
            // nothing of the compiler's is handed over that the program
            // did not name. The map's rules and their order are
            // `wolf_rt::os`'s, entry for entry: shape (`invalid`), host
            // (`unsupported` on windows), sources (`io`), program.
            // Stated asymmetry, `[os.proc.spawn]`'s: 0..2 as SOURCES are
            // the `wolf` process's own streams, and this machine's
            // `print` stream is a buffer no child can enter.
            "os_spawn_fds" => {
                let (Some(exe), Some(Value::List(args_id)), Some(Value::List(map_id))) =
                    (str_arg(0), argv.get(1), argv.get(2))
                else {
                    return self.refuse("this os call shape", span);
                };
                let mut flat = Vec::new();
                for v in self.lists.get(*map_id).into_iter().flatten() {
                    match v {
                        Value::Int(n) => flat.push(*n),
                        _ => return self.refuse("a non-int descriptor map element", span),
                    }
                }
                let Some(entries) = fd_map_of(&flat) else {
                    return Ok(tag("invalid"));
                };
                if cfg!(not(unix)) && !entries.is_empty() {
                    return Ok(tag("unsupported"));
                }
                let mut placed = Vec::with_capacity(entries.len());
                for &(t, s) in &entries {
                    match s {
                        None => placed.push((t, None)),
                        Some(h) => match self.fs_handle(h).and_then(|f| f.try_clone().ok()) {
                            None => return Ok(tag("io")),
                            Some(f) => placed.push((t, Some(f))),
                        },
                    }
                }
                if exe.is_empty() {
                    return Ok(tag("not_found"));
                }
                let mut words = Vec::new();
                for v in self.lists.get(*args_id).into_iter().flatten() {
                    match v {
                        Value::Str(s) => words.push(s.clone()),
                        _ => return self.refuse("a non-str argv element", span),
                    }
                }
                match spawn_mapped(&exe, &words, &placed, self.cwd.as_deref()) {
                    Err(e) => Ok(tag(match e.kind() {
                        std::io::ErrorKind::NotFound => "not_found",
                        std::io::ErrorKind::PermissionDenied => "denied",
                        _ => "io",
                    })),
                    Ok(child) => {
                        let h = self.children.len() as i64;
                        self.children.push(Some(child));
                        Ok(Flow::Val(Value::Int(h)))
                    }
                }
            }
            // s215 (`[os.proc.pipe]`): a real host pipe, both ends in the
            // machine's fs table (close-on-exec, as std makes them), so
            // the fs calls serve them and a map can hand either end to a
            // child. The pair is a tuple: the positional Struct a tuple
            // expression builds.
            "os_pipe" => match std::io::pipe() {
                Err(_) => Ok(tag("io")),
                Ok((r, w)) => {
                    #[cfg(unix)]
                    let (rf, wf) = (
                        std::fs::File::from(std::os::fd::OwnedFd::from(r)),
                        std::fs::File::from(std::os::fd::OwnedFd::from(w)),
                    );
                    #[cfg(windows)]
                    let (rf, wf) = (
                        std::fs::File::from(std::os::windows::io::OwnedHandle::from(r)),
                        std::fs::File::from(std::os::windows::io::OwnedHandle::from(w)),
                    );
                    if self.files.len() < FS_FIRST_HANDLE {
                        self.files.resize_with(FS_FIRST_HANDLE, || None);
                    }
                    let rh = self.files.len() as i64;
                    self.files.push(Some(rf));
                    self.files.push(Some(wf));
                    Ok(Flow::Val(Value::Struct {
                        fields: vec![
                            ("0".to_string(), Value::Int(rh)),
                            ("1".to_string(), Value::Int(rh + 1)),
                        ],
                    }))
                }
            },
            // s215 (`[os.fs.chdir]`): the machine-local working directory
            // moves (see the `cwd` field for why it is not the host's).
            // The host's answers are asked of the host: the target is
            // resolved against the current one and canonicalized (the
            // path `getcwd` would then report), must be a directory
            // (`io` otherwise, as ENOTDIR is), and on unix must be
            // searchable (`access(X_OK)`, chdir's own permission).
            "os_chdir" => {
                let Some(path) = str_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                let joined = resolve_in(&self.cwd, path);
                let canon = match std::fs::canonicalize(&joined) {
                    Err(e) => {
                        return Ok(tag(match e.kind() {
                            std::io::ErrorKind::NotFound => "not_found",
                            std::io::ErrorKind::PermissionDenied => "denied",
                            _ => "io",
                        }));
                    }
                    Ok(c) => c,
                };
                if !canon.is_dir() {
                    return Ok(tag("io"));
                }
                if !dir_searchable(&canon) {
                    return Ok(tag("denied"));
                }
                self.cwd = Some(plain_path(canon));
                Ok(Flow::Val(Value::Unit))
            }
            // s215 (`[os.fs.isatty]`): asked of the host descriptor the
            // handle names — for 0..2 the `wolf` process's own streams.
            "os_isatty" => {
                let Some(fd) = int_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                use std::io::IsTerminal as _;
                match self.fs_handle(fd) {
                    None => Ok(tag("io")),
                    Some(f) => Ok(Flow::Val(Value::Bool(f.is_terminal()))),
                }
            }
            "os_wait" => {
                let Some(h) = int_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                let Some(slot) = usize::try_from(h)
                    .ok()
                    .and_then(|i| self.children.get_mut(i))
                else {
                    return Ok(tag("io"));
                };
                let Some(child) = slot.as_mut() else {
                    return Ok(tag("io")); // double wait
                };
                match child.wait() {
                    Err(_) => Ok(tag("io")),
                    Ok(status) => {
                        *slot = None; // reaped
                        match status.code() {
                            Some(c) => Ok(Flow::Val(Value::Int(i64::from(c)))),
                            // Died without a code (a signal, unix):
                            // its own outcome, never a fake code.
                            None => Ok(tag("signal")),
                        }
                    }
                }
            }
            "os_kill" => {
                let Some(h) = int_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                let Some(Some(child)) = usize::try_from(h)
                    .ok()
                    .and_then(|i| self.children.get_mut(i))
                else {
                    return Ok(tag("io"));
                };
                match child.kill() {
                    // Already exited is `io` (the child is not yours
                    // to kill anymore); the handle stays live for the
                    // wait that reaps it.
                    Err(_) => Ok(tag("io")),
                    Ok(()) => Ok(Flow::Val(Value::Unit)),
                }
            }
            // Signal RECEPTION (s114, #126) — modeled as a PURE
            // IN-MACHINE queue (no real OS signals: the checked machine
            // is a threaded test host, the `env_set` asymmetry). The
            // meaning bitmask matches `wolf_rt::signal::meaning`
            // (reload=1, terminate=2, quit=4, upgrade=8).
            "os_signal_listen" => {
                let Some(mask) = int_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                // Record interest for the mapped meanings only (ALL = 15).
                self.signal_listening |= mask & 0xF;
                Ok(Flow::Val(Value::Unit))
            }
            "os_signal_raise" => {
                let Some(m) = int_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                // A single mapped meaning; an unmapped one is `io`
                // (never a wild signal). A listened meaning becomes a
                // queued event; an unlistened raise is delivered to the
                // default disposition on a real host — the checked
                // machine does not model process death, so it drops it
                // (documented asymmetry, like `env_set`'s overlay).
                if m != 1 && m != 2 && m != 4 && m != 8 {
                    return Ok(tag("io"));
                }
                if self.signal_listening & m != 0 {
                    self.signal_queue.push_back(m);
                }
                Ok(Flow::Val(Value::Unit))
            }
            "os_signal_wait" => {
                let Some(mask) = int_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                let want = mask & 0xF;
                if want == 0 {
                    return Ok(tag("io")); // nothing could ever arrive
                }
                match self.signal_queue.iter().position(|&m| m & want != 0) {
                    Some(pos) => {
                        let m = self.signal_queue.remove(pos).expect("just found it");
                        Ok(Flow::Val(Value::Int(m)))
                    }
                    // A blocking wait with no pending delivery: the
                    // checked machine is single-threaded and run-to-
                    // completion — it has no concurrency to deliver one
                    // later. Refused by name (the honest ledger entry).
                    None => self.refuse(
                        "a blocking signal wait with no pending delivery in checked execution",
                        span,
                    ),
                }
            }
            // The OS random source (s118, #143). The checked machine
            // is a host process, so REAL OS entropy is available on
            // every tier-1 host — no in-machine model (the signal
            // asymmetry does not arise) and no refusal: the crypto
            // lanes (wolf-wws, std) run HERE, and a checked lane
            // without entropy would push them back into the dark.
            // Failure is the deterministic trap `assert` ruled by
            // [os.random.trap] — never a row, never a PRNG fallback;
            // `n < 0` is the [mem.str.repeat] caller-contract trap.
            "os_random" => {
                let Some(n) = int_arg(0) else {
                    return self.refuse("this os call shape", span);
                };
                let Ok(len) = usize::try_from(n) else {
                    return self.trap("assert", "os.random.fill", span);
                };
                self.charge_mem(len as u64)?;
                let mut buf = vec![0u8; len];
                if !os_entropy_fill(&mut buf) {
                    return self.trap("assert", "os.random.trap", span);
                }
                // `[os.random]` says `List[int]` — not a byte surface.
                Ok(Flow::Val(self.int_list_value(&buf, span)?))
            }
            _ => self.refuse("this os builtin", span),
        }
    }

    /// The region accounting queries (s131, #187), checked: the
    /// machine's own ledger answers. `region_bytes` reads a region
    /// value's cumulative charge; `live_region_bytes` sums the charge
    /// of every LIVE region except the run's root — the native
    /// counter's scope (the process-root arena is never counted there
    /// either). Units are per-tier by design (`[mem.region.account]`):
    /// this machine charges its shadow-memory sizes, the native tier
    /// its alignment-rounded arena charges; what agrees everywhere is
    /// the pinned relations — zero at creation, monotone growth,
    /// stability between allocations, wholesale disappearance of a
    /// freed region's live contribution.
    fn region_query_builtin(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        match name {
            "region_bytes" => {
                let Some(Value::Region(rid)) = argv.first() else {
                    return self.refuse("region_bytes over a non-region value", span);
                };
                let Some(r) = self.regions.get(*rid) else {
                    return self.refuse("region_bytes over an unknown region", span);
                };
                Ok(Flow::Val(Value::Int(r.charged.min(i64::MAX as u64) as i64)))
            }
            _ => {
                let total: u64 = self
                    .regions
                    .iter()
                    .skip(1)
                    .filter(|r| r.live)
                    .map(|r| r.charged)
                    .sum();
                Ok(Flow::Val(Value::Int(total.min(i64::MAX as u64) as i64)))
            }
        }
    }

    /// The s40 time builtin tier (checked lane): ms integers, X12
    /// posture — `time_now_ms` counts from the machine's own anchor
    /// (monotonic, arbitrary epoch), `time_unix_ms` is the wall clock,
    /// `time_sleep_ms` really blocks (the checked machine is a host
    /// process; virtualization under `--schedules`/`--replay` rides
    /// the s36 seam as it widens to clock reads — the tracked
    /// campaign-closeout item).
    fn time_builtin(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        match name {
            "time_now_ms" => {
                let ms = self.t0.elapsed().as_millis().min(i64::MAX as u128) as i64;
                Ok(Flow::Val(Value::Int(ms)))
            }
            "time_unix_ms" => {
                let ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
                    .unwrap_or(0);
                Ok(Flow::Val(Value::Int(ms)))
            }
            "time_sleep_ms" => {
                let Some(Value::Int(ms)) = argv.first() else {
                    return self.refuse("this time call shape", span);
                };
                if *ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(*ms as u64));
                }
                Ok(Flow::Val(Value::Unit))
            }
            _ => self.refuse("this time builtin", span),
        }
    }

    /// The s40 json builtin tier (checked lane): PURE — the reference
    /// implementation lives in [`crate::json`] (RFC 8259; the module
    /// doc pins rendering and error semantics), this dispatcher only
    /// maps its three error kinds onto the declared D30 rows.
    fn json_builtin(&mut self, name: &str, argv: Vec<Value>, span: Span) -> E<Flow> {
        use crate::json as jr;
        fn tag(e: jr::JsonErr) -> Flow {
            raise(Value::ErrTag {
                tag: match e {
                    jr::JsonErr::Parse => "parse",
                    jr::JsonErr::Missing => "missing",
                    jr::JsonErr::Kind => "kind",
                }
                .to_string(),
                payload: Vec::new(),
            })
        }
        let str_arg = |i: usize| -> Option<String> {
            match argv.get(i) {
                Some(Value::Str(s)) => Some(s.clone()),
                _ => None,
            }
        };
        match name {
            "json_valid" => {
                let Some(s) = str_arg(0) else {
                    return self.refuse("this json call shape", span);
                };
                Ok(Flow::Val(Value::Bool(jr::valid(&s))))
            }
            "json_get" => {
                let (Some(s), Some(path)) = (str_arg(0), str_arg(1)) else {
                    return self.refuse("this json call shape", span);
                };
                match jr::get(&s, &path) {
                    Err(e) => Ok(tag(e)),
                    Ok(out) => {
                        self.charge_mem(out.len() as u64)?;
                        Ok(Flow::Val(Value::Str(out)))
                    }
                }
            }
            "json_type" => {
                let (Some(s), Some(path)) = (str_arg(0), str_arg(1)) else {
                    return self.refuse("this json call shape", span);
                };
                match jr::type_of(&s, &path) {
                    Err(e) => Ok(tag(e)),
                    Ok(k) => Ok(Flow::Val(Value::Str(k.to_string()))),
                }
            }
            "json_len" => {
                let (Some(s), Some(path)) = (str_arg(0), str_arg(1)) else {
                    return self.refuse("this json call shape", span);
                };
                match jr::len_of(&s, &path) {
                    Err(e) => Ok(tag(e)),
                    Ok(n) => Ok(Flow::Val(Value::Int(n))),
                }
            }
            _ => self.refuse("this json builtin", span),
        }
    }

    /// `str_from_utf8(b: List[byte]) -> str ! {utf8}` (s81, wolf-lang#58;
    /// `List[byte]` since s136, #231) — the checked lane's half of the
    /// border post, and the ONLY way a wolf program can build a `str`
    /// out of bytes on any lane.
    ///
    /// It VALIDATES, which is the whole point: s77 refused an unchecked
    /// bytes-to-str path because that is the forging hole, and the
    /// language's "every `str` is valid UTF-8" invariant has to survive
    /// construction, not just narrowing. The octets go through
    /// `std::str::from_utf8`, so the refused set is exactly UTF-8's —
    /// lone continuations, truncations, overlong forms, surrogates,
    /// scalars past U+10FFFF. An interior NUL is valid text and is
    /// accepted (a wolf `str` carries its length; nothing terminates).
    ///
    /// The refusal is the `utf8` ROW, never a trap: bytes from a file or
    /// a socket are data, and mis-encoded data is an outcome a caller
    /// handles. `wolf_rt::str::__wolf_rt_str_from_utf8` is the same
    /// algorithm for the native lane, byte for byte.
    fn str_from_utf8(&mut self, argv: Vec<Value>, span: Span) -> E<Flow> {
        let utf8 = || {
            Ok(raise(Value::ErrTag {
                tag: "utf8".to_string(),
                payload: Vec::new(),
            }))
        };
        let Some(Value::List(id)) = argv.first() else {
            return self.refuse("this `str_from_utf8` call shape", span);
        };
        let elems = self.lists[*id].clone();
        let mut bytes: Vec<u8> = Vec::with_capacity(elems.len());
        for v in elems {
            match v {
                Value::Byte(b) => bytes.push(b),
                // The pre-s136 carrier: unreachable through sema, kept
                // as the same `utf8` answer the native shim gives a
                // wrong-width list.
                Value::Int(n) => match u8::try_from(n) {
                    Ok(b) => bytes.push(b),
                    Err(_) => return utf8(),
                },
                _ => return self.refuse("a `str_from_utf8` list of non-bytes", span),
            }
        }
        match String::from_utf8(bytes) {
            Ok(s) => {
                // s171 (#391): the free producer is a site too — it
                // builds fresh bytes in the ambient region, so it
                // charges that region's ledger, not the byte budget
                // alone.
                Ok(Flow::Val(self.mint_str(s, span)?))
            }
            Err(_) => utf8(),
        }
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
        // A QUALIFIED module-item fn in VALUE position (#116a):
        // `let f = strx.is_pos` — the s95 bare-name read's
        // cross-module twin, which the compiled tiers already run.
        // Only when the checker typed the whole member expression as
        // a fn, and only when the base names an imported module (a
        // local of the same name shadows it, exactly as resolution
        // ruled).
        if e.kind == SyntaxKind::MemberExpr
            && matches!(self.expr_ty(e.span), Some(TyKind::Fn(_, _)))
        {
            let m = MemberExpr::cast(e).expect("kind");
            if let (Some(base), Some(member)) = (m.base(), m.member())
                && base.kind == SyntaxKind::PathExpr
            {
                let bname = self.text(base.span);
                if !bname.contains('.') && self.lookup(&bname).is_none() {
                    let cur = &self.tc.bodies[self.frames.last().expect("frame").body].body;
                    let (cur_module, cur_file) = (cur.module, cur.file);
                    let md = &self.pkg.modules[cur_module];
                    let target = md
                        .files
                        .iter()
                        .position(|&f| f == cur_file)
                        .and_then(|slot| md.bindings[slot].iter().find(|b| b.name == bname))
                        .and_then(|b| match b.target {
                            wolf_sema::BindTarget::PkgModule(m) => Some(m),
                            _ => None,
                        });
                    if let Some(target) = target {
                        let mname = self.text(member.span);
                        if let Some(&b) = self.fns.get(&(target, mname)) {
                            return Ok(Flow::Val(Value::Fn(b)));
                        }
                    }
                }
            }
        }
        // A payload-free variant as a bare value (`Ordering.Less`,
        // #23's member form; s155 needs it for every `Ord.cmp` body):
        // the checker typed the expression as the enum, and the
        // member names one of its variants with no payload — the
        // value is that tag. The call form (`Color.Rgb(1, 2, 3)`)
        // is `eval_call`'s.
        if e.kind == SyntaxKind::MemberExpr
            && let Some(TyKind::Nominal { module, name, .. }) = self.expr_ty(e.span)
            && let Some(ItemSig::Enum { variants, .. }) = self.tc.sigs.get(*module as usize, name)
        {
            let m = MemberExpr::cast(e).expect("kind");
            if let (Some(base), Some(member)) = (m.base(), m.member())
                && base.kind == SyntaxKind::PathExpr
                && self.lookup(&self.text(base.span)).is_none()
            {
                let vname = self.text(member.span);
                if let Some(v) = variants.iter().find(|v| v.name == vname)
                    && v.payload.is_empty()
                {
                    return Ok(Flow::Val(Value::Enum {
                        variant: vname,
                        payload: Vec::new(),
                    }));
                }
            }
        }
        // Member of a temporary: evaluate the base and project.
        if e.kind == SyntaxKind::MemberExpr {
            let m = MemberExpr::cast(e).expect("kind");
            if let (Some(base), Some(member)) = (m.base(), m.member()) {
                let field = self.text(member.span);
                // s209: `p[i].f` through a raw pointer to a `#[repr(c)]`
                // struct reads the field's bytes at the clause's layout
                // (s213: `(*p).f` and nested paths too).
                if self.raw_field_path(e).is_some() {
                    return Ok(self.raw_field_read(e)?.expect("a raw field path"));
                }
                // `s.bytes().len` reads the view (#232; s153, #308):
                // the receiver's byte count, nothing minted.
                let bv = match found!(self.eval_bytes_view(base)) {
                    Some(octets) if field == "len" => {
                        return Ok(Flow::Val(Value::Int(octets.len() as i64)));
                    }
                    Some(_) => {
                        return self.refuse("field access outside the modelled surface", e.span);
                    }
                    None => val!(self.eval(base)),
                };
                return match bv {
                    Value::Struct { fields } => match fields.into_iter().find(|(n, _)| n == &field)
                    {
                        Some((_, v)) => Ok(Flow::Val(v)),
                        None => self.refuse("field access outside the modelled surface", e.span),
                    },
                    Value::List(id) if field == "len" => {
                        Ok(Flow::Val(Value::Int(self.lists[id].len() as i64)))
                    }
                    Value::Map(id) if field == "len" => {
                        Ok(Flow::Val(Value::Int(self.maps[id].len() as i64)))
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
        // A bare top-level fn name in VALUE position (s95/s97's fn
        // values, the checked twin) — only when the checker typed the
        // expression as a fn, so a module const stays an honest
        // refusal (#12).
        if e.kind == SyntaxKind::PathExpr && matches!(self.expr_ty(e.span), Some(TyKind::Fn(_, _)))
        {
            let name = self.text(e.span);
            if !name.contains('.') && !name.contains("::") {
                let module = self.tc.bodies[self.frames.last().expect("frame").body]
                    .body
                    .module;
                // F-0048's resolution order, minus the decl locus a
                // value read does not carry.
                if let Some(&b) = self.fns.get(&(module, name.clone())).or_else(|| {
                    self.fns
                        .iter()
                        .filter(|((_, n), _)| *n == name)
                        .map(|(_, b)| b)
                        .min()
                }) {
                    return Ok(Flow::Val(Value::Fn(b)));
                }
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
    /// Resolve a subscript-position range against `len` — the shared
    /// endpoint surface of `str` (sc24) and `List` (s128, #171)
    /// slices: open sides default to the edges, `^n` counts from the
    /// end, the D61 origin shift applies to spelled plain endpoints,
    /// and `lo <= hi <= len` traps `bounds` otherwise. The in-domain
    /// answer rides back as `Value::Range { lo, hi }`.
    fn slice_bounds(&mut self, rn: &'t GreenNode, len: i64, at: Span) -> E<Flow> {
        let bounds = self.range_endpoints(rn, len, self.origin_at(at))?;
        let Flow::Val(Value::Range {
            start: lo, end: hi, ..
        }) = bounds
        else {
            return Ok(bounds);
        };
        if lo < 0 || hi < lo || hi > len {
            return self.trap("bounds", "mem.ub.defined", at);
        }
        // A SUBSCRIPT range is never a `range[char]` (s158): `^n` and
        // the open sides resolve against a length, and only `int`
        // indexes a collection.
        Ok(Flow::Val(Value::Range {
            start: lo,
            end: hi,
            chars: false,
        }))
    }

    /// A range node's endpoints resolved against `len`, with NO
    /// domain question asked: open sides default to the edges, `^n`
    /// counts from the end, `..=` bumps the upper bound, and the
    /// origin shift applies to a spelled plain start. The trapping
    /// slice ([`Self::slice_bounds`]) and the recoverable `get`
    /// ([mem.str.get], #164) share this resolution exactly — which is
    /// what the clause means by "resolve exactly as in `s[a..b]`
    /// before the domain question is asked". Mirrors the native
    /// tier's `range_endpoints`.
    fn range_endpoints(&mut self, rn: &'t GreenNode, len: i64, origin: u8) -> E<Flow> {
        let d = RangeExpr::cast(rn).expect("kind");
        // Which side of the dots each endpoint sits on decides which
        // bound it names — open sides default to the edges.
        let dots = rn
            .tokens()
            .find(|t| matches!(t.kind, SyntaxKind::DotDot | SyntaxKind::DotDotEq))
            .map(|t| t.span.lo)
            .unwrap_or(rn.span.hi);
        // The origin shift (D61 `[gram.expr.index.origin]`): under
        // origin 1 a spelled plain START endpoint shifts down by one
        // (checked), a spelled plain END endpoint is inclusive — the
        // 0-based exclusive bound numerically, so `..=` adds nothing —
        // and `^n` endpoints, open sides, and their `..=` interaction
        // resolve exactly as in origin 0.
        let mut lo = 0i64;
        let mut hi = len;
        let mut plain_hi_spelled = false;
        for ep in d.endpoints() {
            let plain = ep.kind != SyntaxKind::FromEndExpr;
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
                lo = if plain && origin == 1 {
                    self.shift_origin(resolved, ep.span)?
                } else {
                    resolved
                };
            } else {
                hi = resolved;
                plain_hi_spelled = plain;
            }
        }
        if d.is_inclusive() && !(origin == 1 && plain_hi_spelled) {
            hi += 1;
        }
        Ok(Flow::Val(Value::Range {
            start: lo,
            end: hi,
            chars: false,
        }))
    }

    /// `cs[a..b]` (s128, #171): the List slice — the same endpoint
    /// surface as str slices, the same `bounds` trap, and a FRESH
    /// List as the value (copy semantics — lupin's, measured).
    fn eval_list_slice(&mut self, e: &'t GreenNode) -> E<Flow> {
        let b = BracketApply::cast(e).expect("kind");
        let Some(recv) = b.callee() else {
            return self.refuse("a slice without a receiver", e.span);
        };
        let lv = if let Some(place) = found!(self.place_of(recv)) {
            self.read_place(&place, recv.span)?
        } else {
            val!(self.eval(recv))
        };
        let Value::List(id) = lv else {
            return self.refuse("List slicing of a non-List", e.span);
        };
        let rn = b
            .args()
            .into_iter()
            .flat_map(|l| l.args())
            .filter_map(Arg::value)
            .find(|v| v.kind == SyntaxKind::RangeExpr);
        let Some(rn) = rn else {
            return self.refuse("this List index shape in checked execution", e.span);
        };
        let len = self.lists[id].len() as i64;
        let bounds = self.slice_bounds(rn, len, e.span)?;
        let Flow::Val(Value::Range { start, end, .. }) = bounds else {
            return Ok(bounds);
        };
        let items: Vec<Value> = self.lists[id][start as usize..end as usize].to_vec();
        self.charge_mem(16 * items.len() as u64 + 16)?;
        let nid = self.mint_list(items, e.span)?;
        Ok(Flow::Val(Value::List(nid)))
    }

    fn eval_str_slice(&mut self, e: &'t GreenNode) -> E<Flow> {
        let b = BracketApply::cast(e).expect("kind");
        let Some(recv) = b.callee() else {
            return self.refuse("a slice without a receiver", e.span);
        };
        let sv = if let Some(place) = found!(self.place_of(recv)) {
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
        let bounds = self.slice_bounds(rn, s.len() as i64, e.span)?;
        let Flow::Val(Value::Range { start, end, .. }) = bounds else {
            return Ok(bounds);
        };
        let (a, z) = (start as usize, end as usize);
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

    /// The element width of `p[i]`: the size of `p`'s own pointee type
    /// (`*T` → `T`), read off the receiver. The element expression's
    /// recorded type is not enough: a write's place carries none, so a
    /// `*i64` store wrote one byte at offset `i` and the read of the
    /// same element took eight at `8 * i` (kw01 — the checked machine
    /// printed `0` for `w[1] = 9; w[1]`). Falls back to the element
    /// expression's type where the receiver records none.
    fn raw_elem_size(&self, e: &'t GreenNode) -> u64 {
        match BracketApply::cast(e).and_then(|b| b.callee()) {
            Some(r) => self.raw_ptr_size(r, e.span),
            None => self.pointee_size(e.span),
        }
    }

    /// The pointee width through the pointer EXPRESSION `ptr` (`*T` →
    /// `T`), falling back to the access's own type at `access`.
    fn raw_ptr_size(&self, ptr: &GreenNode, access: Span) -> u64 {
        match self.raw_ptr_prim(ptr) {
            Some(p) => prim_size(p),
            None => self.pointee_size(access),
        }
    }

    fn raw_ptr_prim(&self, ptr: &GreenNode) -> Option<Prim> {
        match self.expr_ty(ptr.span) {
            Some(TyKind::Ptr(t)) => match self.ctx().tb.table.kind(*t) {
                TyKind::Prim(p) => Some(*p),
                _ => None,
            },
            _ => None,
        }
    }

    /// Does a read through `ptr` sign-extend? A signed integer pointee
    /// narrower than the word does (wolf-lang#561: before kw06
    /// every raw read zero-extended, so `*i32` holding -6 read back
    /// 4294967290 here while native, release and lupin read -6).
    fn raw_ptr_signed(&self, ptr: &GreenNode) -> bool {
        matches!(
            self.raw_ptr_prim(ptr),
            Some(Prim::I8 | Prim::I16 | Prim::I32 | Prim::I64 | Prim::Int)
        )
    }

    /// Little-endian bytes as the pointee's value.
    fn raw_decode(bytes: &[u8], signed: bool) -> i64 {
        let mut n: i64 = 0;
        for (i, b) in bytes.iter().enumerate() {
            n |= (*b as i64) << (8 * i);
        }
        let bits = 8 * bytes.len() as u32;
        if signed && bits < 64 {
            let shift = 64 - bits;
            n = (n << shift) >> shift;
        }
        n
    }

    fn raw_index_parts(&mut self, e: &'t GreenNode) -> E<(PtrVal, i64)> {
        let b = BracketApply::cast(e).expect("kind");
        let recv = b.callee().expect("raw index receiver");
        let pv = match self.place_of(recv)? {
            Found::At(place) => self.read_place(&place, recv.span)?,
            Found::Not => match self.eval(recv)? {
                Flow::Val(v) => v,
                _ => return self.refuse("control flow in a raw index", e.span),
            },
            Found::Flow(_) => return self.refuse("control flow in a raw index", e.span),
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
        let size = self.raw_elem_size(e);
        let signed = BracketApply::cast(e)
            .and_then(|b| b.callee())
            .is_some_and(|r| self.raw_ptr_signed(r));
        let at = PtrVal {
            offset: p.offset + idx * size as i64,
            addr: p.addr.wrapping_add((idx * size as i64) as u64),
            ..p
        };
        self.raw_read_at(at, size, signed, e.span)
    }

    /// s209 (`[mem.unsafe.raw.4]`, ruling #36 = A): an ordinary raw
    /// access whose pointee is `align`-aligned must sit at a multiple
    /// of `align`, or it is row L4. Asked before the row-ordered check
    /// and only of a pointer with an allocation, as L3 is
    /// (allocations are placed at `ALLOC_STRIDE` multiples, so the
    /// address alone decides it); a dangling pointer stays L2.
    fn raw_align_check(&mut self, at: PtrVal, align: u64, write: bool, span: Span) -> E<()> {
        if align > 1 && at.alloc.is_some() && !at.addr.is_multiple_of(align) {
            let opdesc = if write {
                "a raw pointer write"
            } else {
                "a raw pointer read"
            };
            let tag_span = at.alloc.map(|a| self.allocs[a].span).unwrap_or(span);
            return self.ub(
                UbRow::L4,
                format!(
                    "{opdesc} of a {align}-aligned pointee at address {:#x}, which is not a \
                     multiple of {align}",
                    at.addr
                ),
                span,
                tag_span,
            );
        }
        Ok(())
    }

    /// One raw read of `size` bytes at `at`, as the value the access
    /// at `span` is typed. A scalar pointee's alignment is its size
    /// (`[abi.layout.query]`), so `size` is also the alignment the
    /// access needs (row L4).
    fn raw_read_at(&mut self, at: PtrVal, size: u64, signed: bool, span: Span) -> E<Flow> {
        self.raw_align_check(at, size, false, span)?;
        self.raw_read_value(at, size, signed, span)
    }

    /// The read itself, once the address is known to be one the access
    /// may use: a scalar of `size` bytes at `at`, typed as `span` is.
    fn raw_read_value(&mut self, at: PtrVal, size: u64, signed: bool, span: Span) -> E<Flow> {
        let bytes = self.raw_read_bytes(at, size, span, "a raw pointer read")?;
        let n = Self::raw_decode(&bytes, signed);
        // T1 — a restricted type produced from raw bytes must be a
        // valid value of that type.
        if matches!(self.expr_ty(span), Some(TyKind::Prim(Prim::Bool))) {
            if n > 1 {
                let tag_span = at.alloc.map(|a| self.allocs[a].span).unwrap_or(span);
                return self.ub(
                    UbRow::T1,
                    format!("this read produces `{n}` as a `bool` — not a valid value of the type"),
                    span,
                    tag_span,
                );
            }
            return Ok(Flow::Val(Value::Bool(n == 1)));
        }
        Ok(Flow::Val(Value::Int(n)))
    }

    /// s209: the clause layout (`[abi.layout.c]`, `.packed`, `.align`)
    /// of the struct a raw element names — `p[i]`, or (s213) `*p` —
    /// when `p` is a `*S` whose `S` has one.
    fn raw_struct_pointee(&self, elem: &'t GreenNode) -> Option<wolf_sema::layout::CLayout> {
        let ptr = match elem.kind {
            SyntaxKind::BracketApply => BracketApply::cast(elem)?.callee()?,
            SyntaxKind::PrefixExpr => {
                let pre = PrefixExpr::cast(elem)?;
                if !pre.op().is_some_and(|t| t.kind == SyntaxKind::Star) {
                    return None;
                }
                pre.operand()?
            }
            _ => return None,
        };
        let Some(TyKind::Ptr(t)) = self.expr_ty(ptr.span) else {
            return None;
        };
        let TyKind::Nominal { module, name, .. } = self.ctx().tb.table.kind(*t) else {
            return None;
        };
        let lay = wolf_sema::layout::struct_c_layout(&self.tc.sigs, *module as usize, name).ok()?;
        (!lay.fields.is_empty()).then_some(lay)
    }

    /// The scalar a raw field path ends at, read from the struct
    /// signatures (an assignment's place carries no recorded type of
    /// its own): `None` when the leaf is not a primitive.
    fn raw_field_prim(&self, elem: &'t GreenNode, path: &[String]) -> Option<Prim> {
        let ptr = match elem.kind {
            SyntaxKind::BracketApply => BracketApply::cast(elem)?.callee()?,
            _ => PrefixExpr::cast(elem)?.operand()?,
        };
        let Some(TyKind::Ptr(t)) = self.expr_ty(ptr.span) else {
            return None;
        };
        let (mut module, mut name) = match self.ctx().tb.table.kind(*t) {
            TyKind::Nominal { module, name, .. } => (*module as usize, name.clone()),
            _ => return None,
        };
        let table = &self.tc.sigs.table;
        for (i, f) in path.iter().enumerate() {
            let Some(ItemSig::Struct(ss)) = self.tc.sigs.get(module, &name) else {
                return None;
            };
            let fty = ss.fields.iter().find(|x| &x.name == f)?.ty;
            match table.kind(fty) {
                TyKind::Prim(p) if i + 1 == path.len() => return Some(*p),
                TyKind::Nominal {
                    module: m, name: n, ..
                } if i + 1 < path.len() => {
                    (module, name) = (*m as usize, n.clone());
                }
                _ => return None,
            }
        }
        None
    }

    /// s213 (wolf-lang#577): a field path rooted at a raw element —
    /// `p[i].f`, `(*p).f`, `p[i].a.b` — as (the element expression,
    /// the field names root-outward, the element struct's layout).
    #[allow(clippy::type_complexity)]
    fn raw_field_path(
        &self,
        e: &'t GreenNode,
    ) -> Option<(&'t GreenNode, Vec<String>, wolf_sema::layout::CLayout)> {
        let mut path = Vec::new();
        let mut cur = e;
        loop {
            match cur.kind {
                SyntaxKind::MemberExpr => {
                    let m = MemberExpr::cast(cur)?;
                    path.push(self.text(m.member()?.span));
                    cur = m.base()?;
                }
                SyntaxKind::ParenExpr => cur = ParenExpr::cast(cur)?.expr()?,
                _ => break,
            }
        }
        if path.is_empty() {
            return None;
        }
        let lay = self.raw_struct_pointee(cur)?;
        path.reverse();
        Some((cur, path, lay))
    }

    /// The element a raw field path is rooted at: the pointer and the
    /// index run (in that order, before any right-hand side —
    /// wolf-lang#452), and the element's address is `p + i * size_of(S)`.
    fn raw_field_elem(
        &mut self,
        elem: &'t GreenNode,
        lay: &wolf_sema::layout::CLayout,
    ) -> E<PtrVal> {
        let (p, idx) = if elem.kind == SyntaxKind::BracketApply {
            self.raw_index_parts(elem)?
        } else {
            (self.raw_deref_ptr(elem)?, 0)
        };
        let step = idx.wrapping_mul(lay.size as i64);
        Ok(PtrVal {
            offset: p.offset + step,
            addr: p.addr.wrapping_add(step as u64),
            ..p
        })
    }

    /// s209 (`[mem.unsafe.raw.4]`, `[abi.layout.packed]`), s213: the
    /// access to a field path at the element `base`. It is the STRUCT
    /// that must be aligned (row L4): `align_of(S)`, which is 1 for a
    /// packed struct, so a packed `u64` at offset 2 is an ordinary
    /// defined access, as the compiled tiers' `align 1` load or store
    /// is. The field then sits at the offset the layout gives it, which
    /// the struct's alignment already makes aligned for a plain
    /// `#[repr(c)]` struct. Only an integer-shaped field is modelled;
    /// anything else is refused by name. Returns the field's address
    /// and size.
    fn raw_field_at(
        &mut self,
        base: PtrVal,
        lay: &wolf_sema::layout::CLayout,
        path: &[String],
        prim: Option<Prim>,
        write: bool,
        span: Span,
    ) -> E<(PtrVal, u64)> {
        let mut off = 0u64;
        let mut cur = lay;
        for name in path {
            let Some(f) = cur.field(name) else {
                return self.refuse("field access outside the modelled surface", span);
            };
            off += f.offset;
            cur = &f.layout;
        }
        let scalar = matches!(
            prim,
            Some(
                Prim::Bool
                    | Prim::Byte
                    | Prim::U8
                    | Prim::U16
                    | Prim::U32
                    | Prim::U64
                    | Prim::Uint
                    | Prim::I8
                    | Prim::I16
                    | Prim::I32
                    | Prim::I64
                    | Prim::Int
            )
        );
        if !scalar || !cur.fields.is_empty() {
            return self.refuse(
                if write {
                    "a raw write of a field that is not an integer"
                } else {
                    "a raw read of a field that is not an integer"
                },
                span,
            );
        }
        self.raw_align_check(base, lay.align, write, span)?;
        Ok((
            PtrVal {
                offset: base.offset + off as i64,
                addr: base.addr.wrapping_add(off),
                ..base
            },
            cur.size,
        ))
    }

    /// Whether a raw field path's scalar is a signed integer.
    fn raw_field_signed(prim: Option<Prim>) -> bool {
        matches!(
            prim,
            Some(Prim::I8 | Prim::I16 | Prim::I32 | Prim::I64 | Prim::Int)
        )
    }

    /// s209: `p[i].f`, a scalar field read through a raw element of a
    /// `#[repr(c)]` struct; s213: `(*p).f` and nested paths too.
    fn raw_field_read(&mut self, e: &'t GreenNode) -> E<Option<Flow>> {
        let Some((elem, path, lay)) = self.raw_field_path(e) else {
            return Ok(None);
        };
        let prim = self.raw_field_prim(elem, &path);
        let base = self.raw_field_elem(elem, &lay)?;
        let (at, size) = self.raw_field_at(base, &lay, &path, prim, false, e.span)?;
        self.raw_read_value(at, size, Self::raw_field_signed(prim), e.span)
            .map(Some)
    }

    /// kw06: the pointer a prefix `*p` reads or writes through — `p`
    /// evaluated as a read, never a move (a pointer is a copy).
    fn raw_deref_ptr(&mut self, e: &'t GreenNode) -> E<PtrVal> {
        let Some(operand) = PrefixExpr::cast(e).and_then(|d| d.operand()) else {
            return self.refuse("a dereference without an operand", e.span);
        };
        let pv = match self.place_of(operand)? {
            Found::At(place) => self.read_place(&place, operand.span)?,
            Found::Not => match self.eval(operand)? {
                Flow::Val(v) => v,
                _ => return self.refuse("control flow in a dereference", e.span),
            },
            Found::Flow(_) => return self.refuse("control flow in a dereference", e.span),
        };
        let Value::Ptr(p) = pv else {
            return self.refuse("a dereference of a non-pointer", e.span);
        };
        Ok(p)
    }

    /// kw06: `*p` — `p[0]` spelled as a dereference.
    fn raw_deref_read(&mut self, e: &'t GreenNode) -> E<Flow> {
        let p = self.raw_deref_ptr(e)?;
        let operand = PrefixExpr::cast(e)
            .and_then(|d| d.operand())
            .expect("checked");
        let size = self.raw_ptr_size(operand, e.span);
        let signed = self.raw_ptr_signed(operand);
        self.raw_read_at(p, size, signed, e.span)
    }

    /// `parts` is the place's pointer and index, evaluated by the
    /// caller BEFORE the right-hand side (wolf-lang#452).
    /// `op` is the compound operator (`+=`, `*=`, …) or `None` for a
    /// plain `=`; `ty_span` is the right-hand side's span, whose
    /// recorded type carries the checked range (as the non-raw
    /// compound path does).
    fn raw_index_write(
        &mut self,
        place_expr: &'t GreenNode,
        parts: (PtrVal, i64),
        v: Value,
        op: Option<SyntaxKind>,
        ty_span: Span,
        span: Span,
    ) -> E<Flow> {
        let (p, idx) = parts;
        let size = self.raw_elem_size(place_expr);
        let signed = BracketApply::cast(place_expr)
            .and_then(|b| b.callee())
            .is_some_and(|r| self.raw_ptr_signed(r));
        let at = PtrVal {
            offset: p.offset + idx * size as i64,
            addr: p.addr.wrapping_add((idx * size as i64) as u64),
            ..p
        };
        self.raw_write_at(at, size, signed, v, op, ty_span, span)
    }

    /// One raw write of `size` bytes at `at`; `op` is a compound
    /// operator, which reads the pointee first (as the pointee's type).
    #[allow(clippy::too_many_arguments)]
    fn raw_write_at(
        &mut self,
        at: PtrVal,
        size: u64,
        signed: bool,
        v: Value,
        op: Option<SyntaxKind>,
        ty_span: Span,
        span: Span,
    ) -> E<Flow> {
        // s209: a scalar pointee's alignment is its size (row L4); a
        // compound assignment is one access, asked once, as a write.
        self.raw_align_check(at, size, true, span)?;
        self.raw_write_unchecked(at, size, signed, v, op, ty_span, span)
    }

    /// [`Machine::raw_write_at`] once row L4 has been asked by the
    /// caller — s213's field store asks it on the STRUCT's alignment at
    /// the element, not on the field's size at the field.
    #[allow(clippy::too_many_arguments)]
    fn raw_write_unchecked(
        &mut self,
        at: PtrVal,
        size: u64,
        signed: bool,
        v: Value,
        op: Option<SyntaxKind>,
        ty_span: Span,
        span: Span,
    ) -> E<Flow> {
        let mut n = match v {
            Value::Int(n) => n,
            Value::Bool(b) => i64::from(b),
            _ => return self.refuse("raw write of a non-scalar", span),
        };
        // wolf-lang#542's checked half: the operator is the statement's
        // (it was always `+`, so `p[0] *= 31` added), with the same
        // checked arithmetic as every other compound assignment.
        if let Some(op) = op {
            let bytes = self.raw_read_bytes(at, size, span, "a raw pointer read")?;
            let cur = Self::raw_decode(&bytes, signed);
            n = match self.arith_binop(op, Value::Int(cur), Value::Int(n), span, ty_span)? {
                Value::Int(r) => r,
                _ => return self.refuse("raw write of a non-scalar", span),
            };
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
                let v = if let Some(place) = found!(self.place_of(inner)) {
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
                    // ptr -> int: exposes the tag. kw06 (the compiled
                    // tiers' rule, `[mem.prov.expose]`): a 64-bit target
                    // is the address's bits; a narrower one is the
                    // address as `uint` under `[type.numlit.cast.narrow]`.
                    (TyKind::Ptr(_), tgt, Value::Ptr(p)) => {
                        if let Some(a) = p.alloc {
                            self.allocs[a].tags[p.tag as usize].exposed = true;
                        }
                        if let TyKind::Prim(pr) = tgt
                            && prim_bits(pr).is_some_and(|b| b < 64)
                            && let Some((_, hi)) = prim_range(pr)
                            && p.addr > hi as u64
                        {
                            return self.trap("overflow", "type.numlit.cast.narrow", e.span);
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
            // s121 (D58): `char as int` — total; the scalar value.
            Some(CastKind::CharToInt) => {
                let v = val!(self.eval(inner));
                match v {
                    Value::Char(c) => Ok(Flow::Val(Value::Int(i64::from(u32::from(c))))),
                    _ => self.refuse("a char cast of a non-char value", e.span),
                }
            }
            // s121 (D58): `int as char` — D56's trapping family. A
            // value outside `0..=0x10FFFF` or inside the surrogate
            // gap `0xD800..=0xDFFF` is the overflow trap, by name:
            // `char::from_u32`'s domain IS the ruled domain, so the
            // host check and the compiled lanes' two rails agree.
            Some(CastKind::IntToChar) => {
                let v = val!(self.eval(inner));
                match v {
                    Value::Int(n) => match u32::try_from(n).ok().and_then(char::from_u32) {
                        Some(c) => Ok(Flow::Val(Value::Char(c))),
                        None => self.trap("overflow", "type.char.cast", e.span),
                    },
                    _ => self.refuse("an int-to-char cast of a non-int value", e.span),
                }
            }
            // s135 (D72): `byte as int` — total; the octet's value.
            Some(CastKind::ByteToInt) => {
                let v = val!(self.eval(inner));
                match v {
                    Value::Byte(b) => Ok(Flow::Val(Value::Int(i64::from(b)))),
                    _ => self.refuse("a byte cast of a non-byte value", e.span),
                }
            }
            // s135 (D72): `int as byte` — the low eight bits, never a
            // trap ([type.byte.cast]): `256 as byte` is 0, `-1 as byte`
            // is 255. `as u8` on the host is exactly that truncation.
            Some(CastKind::IntToByte) => {
                let v = val!(self.eval(inner));
                match v {
                    Value::Int(n) => Ok(Flow::Val(Value::Byte(n as u8))),
                    _ => self.refuse("an int-to-byte cast of a non-int value", e.span),
                }
            }
            Some(CastKind::Unsize) => {
                // D47 (s98's checked twin): `place as dyn Trait`. The
                // cast READS the place (a lend, never a move — the
                // static loan already guards every write under a live
                // pair), and the value carries the concrete type's
                // name — this machine's vtable half.
                let v = if let Some(place) = found!(self.place_of(inner)) {
                    self.read_place(&place, inner.span)?
                } else {
                    val!(self.eval(inner))
                };
                let concrete = match self.expr_ty(inner.span) {
                    Some(TyKind::Nominal { name, .. }) => name.clone(),
                    _ => return self.refuse("an unsize of a non-nominal receiver", e.span),
                };
                Ok(Flow::Val(Value::Dyn {
                    concrete,
                    inner: Box::new(v),
                }))
            }
            _ => {
                let v = val!(self.eval(inner));
                // A numeric cast CONVERTS (D54.4, `[type.numlit.cast]`).
                // This arm used to be "value-preserving" in the Rust
                // sense — it handed the operand's `Value` straight
                // back — so `n as f64` stayed a `Value::Int` and
                // `4 as f64 == 4.0` compared an `Int` against an `F64`
                // through `values_equal`'s variant pairs and answered
                // FALSE, while `a == a` answered true: the UB-detecting
                // machine gave a verdict, and the wrong one, where the
                // native rung and lupin both said true (wolf-lang#337).
                // The float→int direction was the same hole from the
                // other side: `3.7 as int` came back `F64(3.7)` and
                // printed `3.7`, and `1e300 as int` and `nan as int`
                // ran to exit 0 where `[type.numlit.cast.trunc]` traps.
                // The target type decides, exactly as the literal path
                // has decided since s38.
                match (self.expr_ty(e.span), &v) {
                    (Some(TyKind::Prim(Prim::F64)), Value::Int(n)) => {
                        // s220: a `u64` cell is its bit pattern.
                        let x = if self.wide_unsigned_at(inner.span) {
                            *n as u64 as f64
                        } else {
                            *n as f64
                        };
                        return Ok(Flow::Val(Value::F64(x)));
                    }
                    (Some(TyKind::Prim(Prim::F64)), Value::F64(_)) => {
                        return Ok(Flow::Val(v));
                    }
                    (Some(TyKind::Prim(Prim::F32)), Value::Int(_) | Value::F64(_)) => {
                        return self.refuse(
                            "`f32` in checked execution (f64 is the supported float)",
                            e.span,
                        );
                    }
                    (Some(TyKind::Prim(p)), Value::F64(x)) => {
                        if let Some((lo, hi)) = int_cast_bounds(*p) {
                            // Truncate TOWARD ZERO, and trap on a value
                            // no integer of the target represents — NaN
                            // and both infinities included
                            // (`int_cast_bounds`' edges, upper exclusive).
                            let t = x.trunc();
                            if !t.is_finite() || t < lo || t >= hi {
                                return self.trap("overflow", "mem.ub.defined", e.span);
                            }
                            // s220: a `u64`/`uint` target holds up to
                            // `2^64 - 1` (`int_cast_bounds`), kept as
                            // its bit pattern.
                            if matches!(p, Prim::U64 | Prim::Uint) {
                                return Ok(Flow::Val(Value::Int(t as u64 as i64)));
                            }
                            return Ok(Flow::Val(Value::Int(t as i64)));
                        }
                    }
                    _ => {}
                }
                // Adapter/identity casts are value-preserving here;
                // out-of-range narrowing traps (X3 posture).
                if let Value::Int(n) = v {
                    // The source's VALUE first, as `i128` so the whole
                    // of `u64` fits (s220, wolf-lang#551): a 64-bit
                    // unsigned source is held as its bit pattern, every
                    // other integer as its value — a signed narrow
                    // wrapping value sign-extended (wolf-lang#553). Until
                    // kw03 a signed wrapping value's stored mask was
                    // range-checked as if it were the value, and until
                    // s220 a `wrapping[u64]` above `i64::MAX` cast into
                    // `u64`/`uint` was refused by name.
                    let src: i128 = if self.wide_unsigned_at(inner.span) {
                        i128::from(n as u64)
                    } else {
                        i128::from(n)
                    };
                    // A WRAPPING-typed cast target wraps at its width
                    // (#131's checked twin): the low bits, the native
                    // rung's `itrunc` — never a trap — held as the
                    // target's value (`wrap_held`).
                    if let Some((mask, bits, unsigned)) = self.wrapping_width(e.span) {
                        let low = wrap_held(src as i64, mask, bits, unsigned);
                        return Ok(Flow::Val(Value::Int(low)));
                    }
                    // `[type.numlit.cast.narrow]` (K12, wolf-lang#533):
                    // an integer cast keeps the value or traps
                    // (overflow) when the target cannot hold it. D56's
                    // `wrapping[T] as int` is this rule's case.
                    if let Some((lo, hi)) = self.int_target_range(e.span)
                        && (src < lo || src > hi)
                    {
                        return self.trap("overflow", "type.numlit.cast.narrow", e.span);
                    }
                    // In range: the value, or a `u64`'s bit pattern.
                    return Ok(Flow::Val(Value::Int(src as i64)));
                }
                Ok(Flow::Val(v))
            }
        }
    }

    fn eval_assume(&mut self, stmt: &'t GreenNode) -> E<Flow> {
        let d = wolf_ast::AssumeStmt::cast(stmt).expect("kind");
        let mut ptrs: Vec<(PtrVal, Span)> = Vec::new();
        for op in d.exprs() {
            let v = if let Some(place) = found!(self.place_of(op)) {
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
                let v = if let Some(place) = found!(self.place_of(p)) {
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
        // A folded comptime call site (s71) IS its value: the machine
        // never steps into the comptime callee, and never evaluates
        // the arguments (a type argument has no runtime value at all).
        if let Some(f) = self.ctx().folds.get(&e.span) {
            let v = match f {
                Fold::Unit => Value::Unit,
                Fold::Bool(b) => Value::Bool(*b),
                Fold::Int(n) => Value::Int(*n as i64),
                Fold::Float(v) => Value::F64(*v),
                Fold::Str(s) => Value::Str(s.clone()),
            };
            return Ok(Flow::Val(v));
        }
        let cs: Option<&CallSig> = self.ctx().calls.get(&e.span).copied();
        // C intrinsics (the is04 modelled set).
        if cs.map(|c| c.c_call).unwrap_or(false) {
            let name = cs.expect("c_call has sig").callee.clone();
            return self.eval_c_call(&name, e);
        }
        // Container constructors (`List[int]()`, `Pool[Node]()`).
        match self.expr_ty(e.span) {
            Some(TyKind::List(_)) if is_container_ctor(d.callee()) => {
                let id = self.mint_list(Vec::new(), e.span)?;
                return Ok(Flow::Val(Value::List(id)));
            }
            Some(TyKind::Pool(_)) if is_container_ctor(d.callee()) => {
                let id = self.pools.len();
                self.pools.push(Vec::new());
                return Ok(Flow::Val(Value::Pool(id)));
            }
            Some(TyKind::Map(..)) if is_container_ctor(d.callee()) => {
                let id = self.mint_map(e.span)?;
                return Ok(Flow::Val(Value::Map(id)));
            }
            // `channel[T](n)` / `channel[T]()` (#342): no capacity is
            // rendezvous (`[conc.chan.default]`).
            Some(TyKind::Chan(_)) if is_container_ctor(d.callee()) => {
                let cap = match d
                    .args()
                    .into_iter()
                    .flat_map(|l| l.args())
                    .find_map(Arg::value)
                {
                    Some(v) => match val!(self.eval(v)) {
                        Value::Int(n) => match usize::try_from(n) {
                            Ok(n) => n,
                            Err(_) => return self.refuse("a negative channel capacity", v.span),
                        },
                        _ => return self.refuse("a channel capacity that is not an int", v.span),
                    },
                    None => 0,
                };
                self.charge_mem(16)?;
                let id = self.chans.len();
                self.chans.push(ChanState {
                    buf: std::collections::VecDeque::new(),
                    cap,
                    closed: false,
                });
                return Ok(Flow::Val(Value::Chan(id)));
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
        //
        // s157 (#44): a DECLARED item wins the bare name, so this
        // table is read only where sema recorded no call surface —
        // exactly where the name really is ambient
        // (`[conf.resolve.ambient]`). The native tier reads it the
        // same way.
        let callee_name = if cs.is_some() {
            String::new()
        } else {
            d.callee().map(|c| self.text(c.span)).unwrap_or_default()
        };
        match callee_name.as_str() {
            // kw11 (`[conc.mm.fence]`): one task at a time, every
            // operation whole — every fence is already in force here;
            // the operand is a mark, never evaluated.
            "fence" => return Ok(Flow::Val(Value::Unit)),
            "print" | "print_raw" | "eprint" | "eprint_raw" => {
                let mut out = String::new();
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        let x = if let Some(place) = found!(self.place_of(v)) {
                            self.read_place(&place, v.span)?
                        } else {
                            val!(self.eval(v))
                        };
                        let ty = self.ctx().expr_tys.get(&v.span).copied();
                        out.push_str(&self.render(&x, ty));
                    }
                }
                if callee_name.ends_with("print") {
                    out.push('\n');
                }
                if callee_name.starts_with('e') {
                    self.stderr.extend_from_slice(out.as_bytes());
                } else {
                    self.stdout.extend_from_slice(out.as_bytes());
                }
                return Ok(Flow::Val(Value::Unit));
            }
            // The s38 io/fs builtin tier. Errors are D30 payload rows,
            // never traps: the machine performs the REAL host
            // operation (checked execution is a host process; the
            // comptime sandbox is the one place these are refused) and
            // maps `io::ErrorKind` onto each builtin's declared tags.
            "read_line" | "fs_read_text" | "fs_write_text" | "fs_open" | "fs_create"
            | "fs_read" | "fs_write" | "fs_close" | "fs_remove" | "fs_exists"
            // The s90 additions (#51/#52): bytes, directories,
            // metadata, rename, and the moded open.
            | "fs_open_mode" | "fs_read_bytes" | "fs_write_bytes" | "fs_read_chunk"
            | "fs_write_chunk" | "fs_read_dir" | "fs_create_dir" | "fs_create_dir_all"
            | "fs_remove_dir" | "fs_remove_dir_all" | "fs_rename" | "fs_is_file"
            | "fs_is_dir" | "fs_size" | "fs_modified_ms"
            // s142 (#261): the stat on an open handle.
            | "fs_fstat"
            // s199 (#426): the handle's offset.
            | "fs_seek" | "fs_tell" | "fs_read_at"
            // s200 (#417): the fused chunk copy.
            | "fs_copy_chunk" => {
                let mut argv = Vec::new();
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        let x = if let Some(place) = found!(self.place_of(v)) {
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
            "net_listen" | "net_listen_unix" | "net_connect_unix" | "net_listen_with"
            | "net_adopt_listener" | "net_wait" | "net_port" | "net_accept" | "net_connect"
            | "net_read" | "net_write" | "net_read_bytes" | "net_write_bytes" | "net_close"
            | "net_deadline" | "net_writev" | "net_writev_head" | "net_nodelay" => {
                let mut argv = Vec::new();
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        let x = if let Some(place) = found!(self.place_of(v)) {
                            self.read_place(&place, v.span)?
                        } else {
                            val!(self.eval(v))
                        };
                        argv.push(x);
                    }
                }
                return self.net_builtin(&callee_name, argv, e.span);
            }
            // The s40 os/env, time, and json builtin tiers: fs/net
            // posture again — the checked machine performs the real
            // host operation (json is pure computation), errors are
            // the declared D30 rows, and the comptime sandbox is the
            // one refusal site.
            "env_args" | "env_get" | "env_set" | "env_vars" | "os_cwd" | "os_exe" | "os_cpus"
            | "os_exit"
            | "os_spawn" | "os_spawn_with" | "os_wait" | "os_kill" | "os_signal_listen"
            | "os_spawn_fds" | "os_pipe" | "os_chdir" | "os_isatty"
            | "os_signal_wait" | "os_signal_raise" | "os_random" | "time_now_ms" | "time_unix_ms"
            | "time_sleep_ms" | "json_valid" | "json_get" | "json_type" | "json_len"
            | "str_from_utf8"
            // s200 (#411, #407): the byte scan, and the task's host code.
            | "bytes_find" | "bytes_count" | "os_error" | "os_error_text" => {
                let mut argv = Vec::new();
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        let x = if let Some(place) = found!(self.place_of(v)) {
                            self.read_place(&place, v.span)?
                        } else {
                            val!(self.eval(v))
                        };
                        argv.push(x);
                    }
                }
                return match callee_name.as_str() {
                    "str_from_utf8" => self.str_from_utf8(argv, e.span),
                    "bytes_find" | "bytes_count" => self.byte_scan(&callee_name, argv, e.span),
                    "os_error" => Ok(Flow::Val(Value::Int(self.last_os_error))),
                    "os_error_text" => {
                        let Some(Value::Int(code)) = argv.first() else {
                            return self.refuse("this `os_error_text` call shape", e.span);
                        };
                        let text = host_error_text(*code);
                        Ok(Flow::Val(self.mint_str(text, e.span)?))
                    }
                    n if n.starts_with("json_") => self.json_builtin(n, argv, e.span),
                    n if n.starts_with("time_") => self.time_builtin(n, argv, e.span),
                    n => self.os_builtin(n, argv, e.span),
                };
            }
            // The region accounting queries (s131, #187): reads of
            // this machine's own region ledger — no host operation,
            // no row, the native tier's `wolf_rt` shims mirrored.
            "region_bytes" | "live_region_bytes" => {
                let mut argv = Vec::new();
                for a in d.args().into_iter().flat_map(|l| l.args()) {
                    if let Some(v) = Arg::value(a) {
                        let x = if let Some(place) = found!(self.place_of(v)) {
                            self.read_place(&place, v.span)?
                        } else {
                            val!(self.eval(v))
                        };
                        argv.push(x);
                    }
                }
                return self.region_query_builtin(&callee_name, argv, e.span);
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
                        // s169: the message was ALREADY evaluated here
                        // and then discarded. Keeping it is the whole
                        // of `trap_message` ([proto.record.trap]) —
                        // the evaluation order is untouched, so a
                        // message with an effect still has it exactly
                        // once and only on the failing path.
                        let mut message = None;
                        for a in rest {
                            if let Some(m) = Arg::value(a) {
                                let v = val!(self.eval(m));
                                if message.is_none()
                                    && let Value::Str(s) = v
                                {
                                    message = Some(s);
                                }
                            }
                        }
                        return self.trap_with("assert", "mem.ub.defined", e.span, message);
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
            return Ok(raise(Value::ErrTag { tag, payload }));
        }
        // A plain user fn call.
        let Some(sig) = cs else {
            return self.refuse("calls outside the modelled surface", e.span);
        };
        // A QUALIFIED dispatch (`Trait.method(recv, …)` or a
        // qualified inherent) carries an s17 record at the call span:
        // the first argument is the receiver, and the record — not
        // the callee name — names the body (#12, the s18 rule).
        if let Some(rec) = self.ctx().dispatch.get(&e.span) {
            enum Q {
                I(String),
                T(usize, String, bool),
            }
            // `sig.callee` is the dotted spelling (`Draw.draw`);
            // the record's own `method` field is the bare name the
            // indexes key on.
            let (q, mname) = match rec {
                Dispatch::Inherent { ty, method } => (Q::I(ty.clone()), method.clone()),
                Dispatch::Trait {
                    module,
                    name,
                    method,
                    dyn_call,
                } => (Q::T(*module, name.clone(), *dyn_call), method.clone()),
                Dispatch::Home { .. } => {
                    return self.refuse("home-module method calls in checked execution", e.span);
                }
            };
            let mut arg_exprs = d.args().into_iter().flat_map(|l| l.args());
            let Some(recv_expr) = arg_exprs.next().and_then(Arg::value) else {
                return self.refuse("a qualified dispatch without a receiver", e.span);
            };
            let self_mode = sig.params.first().and_then(|p| p.mode);
            let mut self_val = val!(self.eval_arg(recv_expr, self_mode));
            let (body, subject) = match q {
                Q::I(ty_name) => {
                    let Some(&body) = self.methods.get(&(ty_name.clone(), mname.clone())) else {
                        return self.refuse("methods without resolvable bodies", e.span);
                    };
                    (body, ty_name)
                }
                Q::T(module, name, dyn_call) => {
                    let concrete =
                        self.trait_concrete(recv_expr.span, dyn_call, &self_val, e.span)?;
                    if let Value::Dyn { inner, .. } = self_val {
                        self_val = *inner;
                    }
                    let body = self.resolve_trait_body(&concrete, module, &name, &mname, e.span)?;
                    (body, concrete)
                }
            };
            let mut call_args = vec![self_val];
            for (i, a) in arg_exprs.enumerate() {
                let Some(v) = Arg::value(a) else { continue };
                let mode = sig.params.get(i + 1).and_then(|p| p.mode);
                call_args.push(val!(self.eval_arg(v, mode)));
            }
            self.pending_self_ty = Some(subject);
            let out = self.call_body(body, call_args)?;
            if let Value::ErrTag { .. } = out {
                return Ok(raise(out));
            }
            return Ok(Flow::Val(out));
        }
        let module = self.tc.bodies[self.frames.last().expect("frame").body]
            .body
            .module;
        // Resolution order (F-0048, corrected by #400): the checker's
        // declaration locus names the body exactly (cross-module calls
        // included) and is authoritative wherever it exists.
        //
        // Where the checker recorded NO locus, the callee is not a
        // declared item at all — `CallSig::decl_span` is `None`
        // exactly for fn-typed VALUES — so the call goes through a
        // place holding `Value::Fn` (a param, a binding, a field).
        // That place is the INNERMOST binding of the name, and it wins
        // over any top-level fn that happens to share it (#400): a
        // library picks its own parameter names, so a caller cannot
        // defend against the collision. Consulting the value BEFORE
        // the name-only nets is what makes `fn apply(le: fn(int, int)
        // -> bool, …) { le(x, y) }` call its parameter rather than a
        // top-level `fn le`, which is what native and lupin do.
        //
        // Only when neither answers do the name-only nets run: the
        // caller's own module answers same-module calls, and the
        // last net picks the SMALLEST body index — a stable,
        // deterministic choice, never a hash order's.
        let by_decl = sig
            .decl_span
            .and_then(|ds| self.fns_by_decl.get(&ds))
            .copied();
        let through_value = match by_decl {
            Some(_) => None,
            None => match d.callee() {
                Some(callee) => match found!(self.place_of(callee)) {
                    Some(place) => match self.read_place(&place, callee.span)? {
                        Value::Fn(b) => Some(b),
                        _ => None,
                    },
                    None => None,
                },
                None => None,
            },
        };
        let by_name = by_decl.or(through_value).or_else(|| {
            self.fns
                .get(&(module, sig.callee.clone()))
                .or_else(|| {
                    self.fns
                        .iter()
                        .filter(|((_, n), _)| *n == sig.callee)
                        .map(|(_, b)| b)
                        .min()
                })
                .copied()
        });
        let body = match by_name {
            Some(b) => b,
            None => return self.refuse("calls into unresolvable bodies", e.span),
        };
        // Generic bindings for the callee (#12): a declared param type
        // that NAMES one of the callee's own generic params binds it
        // to the caller-side concrete type of the matching argument —
        // read through this frame's own bindings, so nesting
        // propagates. Spelled from the callee's source, not the
        // caller's (cross-file calls).
        {
            let callee_node = self.ctxs[body]
                .as_ref()
                .expect("callable body has ctx")
                .node;
            let callee_file = self.tc.bodies[body].body.file;
            let csrc = &self.pkg.files[callee_file].raw.src;
            let slice = |sp: Span| {
                String::from_utf8_lossy(&csrc[sp.lo as usize..sp.hi as usize]).into_owned()
            };
            if let Some(fd) = wolf_ast::FnDecl::cast(callee_node)
                && let Some(gl) = fd.generics()
            {
                let gnames: Vec<String> = gl
                    .params()
                    .filter_map(|gp| gp.name().map(|t| slice(t.span)))
                    .collect();
                if !gnames.is_empty() {
                    let mut map: HashMap<String, String> = HashMap::new();
                    let mut arg_iter = d.args().into_iter().flat_map(|l| l.args());
                    for pdecl in fd.params().into_iter().flat_map(|ps| ps.params()) {
                        let a = arg_iter.next();
                        let (Some(tynode), Some(a)) = (pdecl.ty(), a) else {
                            continue;
                        };
                        let tytext = slice(tynode.span);
                        if gnames.contains(&tytext)
                            && let Some(v) = Arg::value(a)
                            && let Some(c) = self.ty_concrete_name(v.span)
                        {
                            map.entry(tytext).or_insert(c);
                        }
                    }
                    if !map.is_empty() {
                        self.pending_rigids = Some(map);
                    }
                }
            }
        }
        let mut args = Vec::new();
        for (i, a) in d.args().into_iter().flat_map(|l| l.args()).enumerate() {
            let Some(v) = Arg::value(a) else { continue };
            let mode = sig.params.get(i).and_then(|p| p.mode);
            args.push(val!(self.eval_arg(v, mode)));
        }
        let out = self.call_body(body, args)?;
        // A raised row tag crosses the call as the error flow — the
        // caller's `?`/`else`/`match` observes it (D30).
        if let Value::ErrTag { .. } = out {
            return Ok(raise(out));
        }
        Ok(Flow::Val(out))
    }

    /// Evaluate one argument under its declared mode: `mut` lends the
    /// place (call-by-reference-result), `take` moves, `read` copies
    /// scalars and shares containers.
    fn eval_arg(&mut self, v: &'t GreenNode, mode: Option<wolf_ast::ParamMode>) -> E<Flow> {
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
                let Some(place) = found!(self.place_of(inner)) else {
                    return self.refuse("`mut` of a non-place in checked execution", v.span);
                };
                // kw02: a raw pointer lent by `mut` is the caller's
                // pointer VARIABLE (call-by-reference-result, what the
                // compiled tiers lower); the mode says nothing about
                // the pointee ([mem.unsafe.raw.1]). Before `[mem.unsafe.sig]`
                // no `*T` parameter type-checked and this machine
                // passed a retagged copy, so a callee's `p = q` never
                // reached the caller.
                Ok(Flow::Val(Value::Ref(place)))
            }
            Some(wolf_ast::ParamMode::Take) => {
                if let Some(place) = found!(self.place_of(inner)) {
                    self.take_value(&place, inner.span).map(Flow::Val)
                } else {
                    self.eval_arg_value(inner)
                }
            }
            _ => {
                if let Some(place) = found!(self.place_of(inner)) {
                    // kw02: a raw pointer passed by value is a copy of
                    // the pointer and nothing more — raw pointers carry
                    // no aliasing assumptions ([mem.unsafe.raw.1]) and
                    // the compiled tiers give a `*T` parameter no
                    // `readonly`/`noalias`. Before `[mem.unsafe.sig]`
                    // this path froze the pointee, so a module-private
                    // `fn poke(p: *u8)` writing through `p` was P2 here
                    // and ran natively.
                    return Ok(Flow::Val(self.read_place(&place, inner.span)?));
                }
                self.eval_arg_value(inner)
            }
        }
    }

    /// An argument that is not a place (#201). A RAW row value binds
    /// to the parameter exactly as it binds at `let`/`var` and at
    /// assignment (#122, D52's declared-row-first reading): the
    /// callee receives the row and discriminates it, which is what
    /// native and lupin do. Every other flow — a `?`-propagated
    /// error, a `return` or `break` out of a handler — leaves the
    /// call before the callee runs, the caller's own flow. The
    /// refusal this replaces answered `unsupported` only on the path
    /// where the row was taken, so the same source was one word or
    /// another by input.
    fn eval_arg_value(&mut self, inner: &'t GreenNode) -> E<Flow> {
        Ok(match self.eval(inner)? {
            Flow::Err(v, false) => Flow::Val(v),
            other => other,
        })
    }

    /// `send`, `recv` and `close` (#342, `[conc.chan]`). This machine
    /// runs one task: `spawn`, `scope`, `select` and `when` are refused
    /// by name before they run, so the root task is the only live task
    /// there is. A send on a full channel (or any rendezvous send) and a
    /// receive on an empty open one therefore block a task that nothing
    /// can ever wake — every live task blocked, `[conc.deadlock.def]`
    /// exactly — and the answer is `[conc.deadlock.trap]`'s
    /// `trap(deadlock)`, the verdict this deterministic machine is
    /// REQUIRED to detect. Nothing here is a guess about a schedule:
    /// with one task there is one.
    fn eval_chan_method(
        &mut self,
        method: &str,
        recv: &'t GreenNode,
        e: &'t GreenNode,
        args: Option<wolf_ast::ArgList<'t>>,
    ) -> E<Flow> {
        let ch = match found!(self.place_of(recv)) {
            Some(place) => self.read_place(&place, recv.span)?,
            None => val!(self.eval(recv)),
        };
        let Value::Chan(id) = ch else {
            return self.refuse("a channel method on a non-channel", e.span);
        };
        let closed = || {
            Ok(raise(Value::ErrTag {
                tag: "closed".to_string(),
                payload: Vec::new(),
            }))
        };
        match method {
            "send" => {
                let Some(v) = args.into_iter().flat_map(|l| l.args()).find_map(Arg::value) else {
                    return self.refuse("a send without a value", e.span);
                };
                let x = val!(self.eval_arg(v, None));
                let st = &self.chans[id];
                if st.closed {
                    return closed();
                }
                if st.buf.len() >= st.cap {
                    return self.trap("deadlock", "conc.deadlock.trap", e.span);
                }
                // The channel owns the copy in flight
                // (`[conc.chan.payload]`), charged when it is made.
                self.charge_mem(slot_bytes(&x))?;
                self.chans[id].buf.push_back(x);
                Ok(Flow::Val(Value::Unit))
            }
            "recv" => self.chan_recv(id, e.span),
            "close" => {
                self.chans[id].closed = true;
                Ok(Flow::Val(Value::Unit))
            }
            _ => self.refuse("this channel method", e.span),
        }
    }

    /// One receive (#342): the oldest payload, else `closed` on a
    /// drained-closed channel, else the root task blocks alone —
    /// `trap(deadlock)` (see [`Self::eval_chan_method`]).
    fn chan_recv(&mut self, id: usize, span: Span) -> E<Flow> {
        match self.chans[id].buf.pop_front() {
            Some(v) => Ok(Flow::Val(v)),
            None if self.chans[id].closed => Ok(raise(Value::ErrTag {
                tag: "closed".to_string(),
                payload: Vec::new(),
            })),
            None => self.trap("deadlock", "conc.deadlock.trap", span),
        }
    }

    fn eval_method(
        &mut self,
        sig: &'t CallSig,
        recv: &'t GreenNode,
        e: &'t GreenNode,
        args: Option<wolf_ast::ArgList<'t>>,
    ) -> E<Flow> {
        let method = sig.callee.as_str();
        // s166 — a home-module method (`[type.method.resolve]` step 2)
        // is the free call with the receiver first; the checked
        // machine's generic binding reads positional arguments only,
        // so it refuses the method spelling by name rather than guess.
        if matches!(
            self.ctx().dispatch.get(&e.span),
            Some(Dispatch::Home { .. })
        ) {
            return self.refuse("home-module method calls in checked execution", e.span);
        }
        let recv_ty = self.expr_ty(recv.span).cloned();
        // Raw-pointer provenance ops.
        if matches!(recv_ty, Some(TyKind::Ptr(_))) {
            let pv = if let Some(place) = found!(self.place_of(recv)) {
                self.read_place(&place, recv.span)?
            } else {
                val!(self.eval(recv))
            };
            let Value::Ptr(p) = pv else {
                return self.refuse("pointer op on a non-pointer", e.span);
            };
            // kw11: an atomic operation's order operands are marks
            // (`[conc.mm.atomic.order]`), never evaluated; this
            // machine runs every order as seq_cst
            // (`[conc.mm.atomic.raw.5]`), so it does not read them.
            let atomic = wolf_ast::atomic::AtomicOp::from_method(method);
            let mut arg_vals = Vec::new();
            for (i, a) in args.into_iter().flat_map(|l| l.args()).enumerate() {
                if atomic.is_some_and(|op| op.order_slots().contains(&i)) {
                    continue;
                }
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
                "read_volatile" | "write_volatile" => {
                    let Some(TyKind::Ptr(t)) = &recv_ty else {
                        unreachable!("matched above")
                    };
                    let pointee = match self.ctx().tb.table.kind(*t) {
                        TyKind::Prim(p) => *p,
                        _ => return self.refuse("a volatile access of a non-scalar", e.span),
                    };
                    self.volatile_access(p, pointee, arg_vals.first().cloned(), e.span)
                }
                _ if atomic.is_some() => {
                    let Some(TyKind::Ptr(t)) = &recv_ty else {
                        unreachable!("matched above")
                    };
                    let pointee = match self.ctx().tb.table.kind(*t) {
                        TyKind::Prim(p) => *p,
                        _ => return self.refuse("an atomic operation on a non-scalar", e.span),
                    };
                    let op = atomic.expect("guarded");
                    self.atomic_access(op, p, pointee, &arg_vals, e.span)
                }
                _ => self.refuse("this pointer method", e.span),
            };
        }
        // Channel methods from the root task (#342).
        if matches!(recv_ty, Some(TyKind::Chan(_))) {
            return self.eval_chan_method(method, recv, e, args);
        }
        // Container/cell builtins by receiver type.
        match recv_ty {
            Some(TyKind::List(_)) => {
                // s89 (#85): the receiver may be a PLACE or a
                // TEMPORARY. A place reads without moving (`read_place`
                // — `xs.len` must not consume `xs`); a temporary is
                // evaluated on the spot, which is what makes the four
                // query positions of s77's byte view — `count`,
                // `is_empty`, `get`, `first`/`last` on `s.bytes()` —
                // reachable here at all. The refusal that used to
                // stand in this spot said "List method on a temporary",
                // a place-model sentence that never mentioned views;
                // what is left of it below names the real rule.
                let recv_place = found!(self.place_of(recv));
                let recv_val = match &recv_place {
                    Some(place) => self.read_place(place, recv.span)?,
                    // `s.bytes().len` and the query family read the
                    // view (#232): uncharged — and since s153 (#308)
                    // unretained: the four queries answer off the
                    // receiver's octets and no list is minted. The
                    // mutators refuse by the same sentence a
                    // materialized temporary gets (below).
                    None => match found!(self.eval_bytes_view(recv)) {
                        Some(octets) => {
                            let none = || {
                                Ok(raise(Value::ErrTag {
                                    tag: "none".to_string(),
                                    payload: Vec::new(),
                                }))
                            };
                            return match method {
                                "len" | "count" => Ok(Flow::Val(Value::Int(octets.len() as i64))),
                                "is_empty" => Ok(Flow::Val(Value::Bool(octets.is_empty()))),
                                "get" => {
                                    let idx = args
                                        .into_iter()
                                        .flat_map(|l| l.args())
                                        .find_map(Arg::value);
                                    let Some(v) = idx else {
                                        return self.refuse("List.get without an index", e.span);
                                    };
                                    let Value::Int(i) = val!(self.eval(v)) else {
                                        return self
                                            .refuse("List.get with a non-int index", e.span);
                                    };
                                    match usize::try_from(i).ok().and_then(|i| octets.get(i)) {
                                        Some(b) => Ok(Flow::Val(Value::Byte(*b))),
                                        None => none(),
                                    }
                                }
                                "first" | "last" => {
                                    let b = if method == "first" {
                                        octets.first()
                                    } else {
                                        octets.last()
                                    };
                                    match b {
                                        Some(b) => Ok(Flow::Val(Value::Byte(*b))),
                                        None => none(),
                                    }
                                }
                                "push" | "pop" | "clear" => self.refuse(
                                    "mutating a temporary List (a `bytes()` view is read-only)",
                                    e.span,
                                ),
                                _ => self.refuse("this List method", e.span),
                            };
                        }
                        None => val!(self.eval(recv)),
                    },
                };
                let Value::List(id) = recv_val else {
                    return self.refuse("List method on a non-list", e.span);
                };
                // The mutators need a place, and this is the rule
                // rather than a modelling gap: a temporary list is
                // observable only through the expression that made it,
                // so `push`/`pop`/`clear` on one would write storage no
                // later read can reach. On the byte view specifically
                // there is no write path at all (s77: a str's bytes are
                // immutable, a literal's live in rodata), which is why
                // `wolf_wir` refuses the same three spellings.
                if matches!(method, "push" | "pop" | "clear") && recv_place.is_none() {
                    return self.refuse(
                        "mutating a temporary List (a `bytes()` view is read-only)",
                        e.span,
                    );
                }
                match method {
                    "push" => {
                        for a in args.into_iter().flat_map(|l| l.args()) {
                            if let Some(v) = Arg::value(a) {
                                // wolf-lang#385, ruled option 3: a
                                // plain element is COPIED into the
                                // list, so the caller keeps an
                                // independent value; `push(take x)`
                                // moves it. `deep_copy` hands scalars,
                                // `str` and every other non-heap value
                                // straight back, so `Copy` elements
                                // stay free ([mem.tier0.move.3]).
                                // `eval_arg` answers a Flow since s161,
                                // so each arm unwraps through `val!`.
                                let x = if a.mode() == Some(wolf_ast::ParamMode::Take) {
                                    val!(self.eval_arg(v, Some(wolf_ast::ParamMode::Take)))
                                } else {
                                    let x = val!(self.eval_arg(v, None));
                                    self.deep_copy(x, v.span)?
                                };
                                let slot = slot_bytes(&x);
                                self.charge_mem(slot)?;
                                // The ledger charges the BIRTH region
                                // (s131, #187): growth stays where the
                                // list lives, not where the push runs.
                                let rid = self.list_region.get(id).copied().unwrap_or(0);
                                self.charge_region_bytes(rid, slot, e.span)?;
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
                        None => Ok(raise(Value::ErrTag {
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
                            return Ok(raise(Value::ErrTag {
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
                            None => Ok(raise(Value::ErrTag {
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
            // s152 — the `Map` surface ([type.map]): the count, the
            // emptiness probe, the drain, and `pairs()` as a fresh
            // `List[(K, V)]` in insertion order (the tuple is the
            // positional struct the TupleExpr evaluator builds).
            Some(TyKind::Map(..)) => {
                let recv_place = found!(self.place_of(recv));
                let recv_val = match &recv_place {
                    Some(place) => self.read_place(place, recv.span)?,
                    None => val!(self.eval(recv)),
                };
                let Value::Map(id) = recv_val else {
                    return self.refuse("Map method on a non-map", e.span);
                };
                if matches!(method, "clear" | "remove") && recv_place.is_none() {
                    return self.refuse("mutating a temporary Map", e.span);
                }
                match method {
                    "len" | "count" => Ok(Flow::Val(Value::Int(self.maps[id].len() as i64))),
                    "is_empty" => Ok(Flow::Val(Value::Bool(self.maps[id].is_empty()))),
                    "pairs" => {
                        let items: Vec<Value> = self.maps[id]
                            .iter()
                            .map(|(k, v)| Value::Struct {
                                fields: vec![
                                    ("0".to_string(), k.value()),
                                    ("1".to_string(), v.clone()),
                                ],
                            })
                            .collect();
                        let bytes: u64 = items.iter().map(slot_bytes).sum();
                        self.charge_mem(bytes)?;
                        let nid = self.mint_list(items, e.span)?;
                        Ok(Flow::Val(Value::List(nid)))
                    }
                    "clear" => {
                        self.maps[id].clear();
                        Ok(Flow::Val(Value::Unit))
                    }
                    // s165 (#344): the key erase — the erased value, or
                    // the `none` row with the map unchanged; the
                    // survivors keep their insertion order.
                    "remove" => {
                        let kx = args.into_iter().flat_map(|l| l.args()).find_map(Arg::value);
                        let Some(kx) = kx else {
                            return self.refuse("Map.remove without a key", e.span);
                        };
                        let kv = val!(self.eval(kx));
                        let Some(key) = MapKey::of(&kv) else {
                            return self.refuse("Map.remove with a key outside the four", kx.span);
                        };
                        match self.maps[id].iter().position(|(k, _)| *k == key) {
                            Some(pos) => {
                                let (_, v) = self.maps[id].remove(pos);
                                Ok(Flow::Val(v))
                            }
                            None => Ok(raise(Value::ErrTag {
                                tag: "none".to_string(),
                                payload: Vec::new(),
                            })),
                        }
                    }
                    _ => self.refuse("this Map method", e.span),
                }
            }
            Some(TyKind::Pool(_)) => {
                let Some(place) = found!(self.place_of(recv)) else {
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
                    // s173 (D50): `has` is `alive` under the name the
                    // other containers use for the same question.
                    "alive" | "has" => {
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
                    // s173 (D50): every live slot removed, every
                    // generation bumped — so every handle the pool
                    // ever issued is stale afterwards, by exactly the
                    // rule `remove` uses one slot at a time (X5).
                    "clear" => {
                        for slot in &mut self.pools[id] {
                            if !slot.live {
                                continue;
                            }
                            slot.live = false;
                            slot.generation += 1;
                            slot.value = Value::Uninit;
                        }
                        Ok(Flow::Val(Value::Unit))
                    }
                    _ => self.refuse("this Pool method", e.span),
                }
            }
            Some(TyKind::Shared(_)) => {
                let Some(place) = found!(self.place_of(recv)) else {
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
                let Some(place) = found!(self.place_of(recv)) else {
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
                            Ok(raise(Value::ErrTag {
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
                let sv = if let Some(place) = found!(self.place_of(recv)) {
                    self.read_place(&place, recv.span)?
                } else {
                    val!(self.eval(recv))
                };
                let Value::Str(s) = sv else {
                    return self.refuse("str method on a non-str", e.span);
                };
                // `get`'s range is a SLICE-position range: its
                // endpoints resolve by the subscript rule — open ends
                // and `^n` included — before the domain question is
                // asked ([mem.str.get], #164). The generic argument
                // evaluation below cannot do that: it has no length
                // to count `^n` from. Method position is origin-free
                // (D61), so the origin is 0 whatever scope the call
                // sits in, exactly as the native tier passes it.
                let mut argv = Vec::new();
                if method == "get" {
                    let rn = args
                        .into_iter()
                        .flat_map(|l| l.args())
                        .find_map(Arg::value)
                        .filter(|v| v.kind == SyntaxKind::RangeExpr);
                    if let Some(rn) = rn {
                        let len = s.len() as i64;
                        argv.push(val!(self.range_endpoints(rn, len, 0)));
                    }
                }
                if argv.is_empty() {
                    for a in args.into_iter().flat_map(|l| l.args()) {
                        if let Some(v) = Arg::value(a) {
                            argv.push(val!(self.eval(v)));
                        }
                    }
                }
                let none_miss = || {
                    Ok(raise(Value::ErrTag {
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
                    let id = m.mint_list(items, e.span)?;
                    Ok(Flow::Val(Value::List(id)))
                };
                match method {
                    "is_empty" => Ok(Flow::Val(Value::Bool(s.is_empty()))),
                    // The boundary primitive (wolf-lang#17): the
                    // recoverable slice — OOB *and* split-code-point
                    // offsets are the same honest miss.
                    "get" => {
                        let Some(Value::Range { start, end, .. }) = argv.first() else {
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
                    // `bytes()` is `List[byte]` (s136, wolf-lang#231):
                    // one `Value::Byte` per octet, charged one byte per
                    // byte. This is the MATERIALIZING position — a
                    // binding, an argument, a return; the consumed
                    // positions (`for b in s.bytes()`, `s.bytes()[i]`,
                    // the `len` family) read the receiver's own bytes
                    // and mint nothing (s77's view; #232 on this tier).
                    "bytes" => {
                        let items: Vec<Value> = s.bytes().map(Value::Byte).collect();
                        self.charge_mem(items.len() as u64 + 16)?;
                        let id = self.mint_list(items, e.span)?;
                        Ok(Flow::Val(Value::List(id)))
                    }
                    // s120 (#17, [mem.str.chars]): code-point
                    // iteration — the Unicode scalar values in string
                    // order; a scalar's UTF-8 byte extent is a
                    // function of its value, so a scan advances by
                    // real width without a `char` type.
                    "chars" => {
                        let items: Vec<Value> = s.chars().map(Value::Char).collect();
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
                        // `[mem.str.empty]` (#56): an empty needle
                        // matches nothing — the count is 0.
                        Some(n) if n.is_empty() => Ok(Flow::Val(Value::Int(0))),
                        Some(n) => Ok(Flow::Val(Value::Int(s.matches(&n).count() as i64))),
                        None => self.refuse("count without a str needle", e.span),
                    },
                    "split" => match needle(0) {
                        // `[mem.str.empty]` (#56): an empty separator
                        // splits nowhere — the whole string, one piece.
                        Some(n) if n.is_empty() => make_list(self, vec![Value::Str(s.clone())]),
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
                    // s171 (#391): `lower`/`upper` build FRESH bytes,
                    // so each is a site exactly as `+` is
                    // (`[mem.region.escape]`) and charges the ambient
                    // region's ledger (`[mem.region.account.1]`).
                    // Until this pin they charged neither the byte
                    // budget nor the region, so `region_bytes` read 0
                    // and a `cap: 0` region ran them where native and
                    // lupin trapped `alloc-contract`.
                    "lower" => Ok(Flow::Val(self.mint_str(s.to_lowercase(), e.span)?)),
                    "upper" => Ok(Flow::Val(self.mint_str(s.to_uppercase(), e.span)?)),
                    // s142 (wolf-lang#263): `to_int() -> int !
                    // {parse}`, ruled by `[mem.str.to_int]` (s143,
                    // #265; the mark was `NotAnInt` until then). Surrounding `[mem.str.ws]` is ignored
                    // (`trim`'s set, which is `char::is_whitespace`'s
                    // twenty-five); then an optionally signed run of
                    // ASCII digits that fits `i64`, or the row. Out of
                    // range is the row too — there is no `int` the
                    // text names, and X3 forbids a quiet wrap. Never a
                    // trap.
                    "to_int" => match s.trim().parse::<i64>() {
                        Ok(v) => Ok(Flow::Val(Value::Int(v))),
                        Err(_) => Ok(raise(Value::ErrTag {
                            tag: "parse".to_string(),
                            payload: Vec::new(),
                        })),
                    },
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
                            // A negative count is a caller contract
                            // violation, ruled `assert` — not an
                            // out-of-range access ([mem.str.repeat],
                            // #57).
                            return self.trap("assert", "mem.str.repeat", e.span);
                        }
                        // s171 (#391): the ambient region's ledger too,
                        // not the byte budget alone — charged BEFORE the
                        // bytes exist, as `+` does, so a build the cap
                        // will not admit is refused before the host
                        // allocates it.
                        self.charge_str(s.len() as u64 * *n as u64 + 16, e.span)?;
                        Ok(Flow::Val(Value::Str(s.repeat(*n as usize))))
                    }
                    "replace" => {
                        let (Some(from), Some(to)) = (needle(0), needle(1)) else {
                            return self.refuse("replace without str arguments", e.span);
                        };
                        if from.is_empty() {
                            // `[mem.str.empty]` (#56): an empty needle
                            // matches nothing — replace is identity.
                            return Ok(Flow::Val(Value::Str(s.clone())));
                        }
                        // s171 (#391): the ambient region's ledger too.
                        self.charge_str(s.len() as u64 + 16, e.span)?;
                        Ok(Flow::Val(Value::Str(s.replace(&from, &to))))
                    }
                    _ => self.refuse("this `str` method in checked execution", e.span),
                }
            }
            _ => {
                // A user method — inherent or trait — through the s17
                // dispatch record (the s18 rule: READ the record,
                // never re-derive; #12).
                enum Target {
                    Inherent(String),
                    Trait {
                        module: usize,
                        name: String,
                        dyn_call: bool,
                    },
                }
                let target = match self.ctx().dispatch.get(&e.span) {
                    Some(Dispatch::Inherent { ty, .. }) => Target::Inherent(ty.clone()),
                    Some(Dispatch::Trait {
                        module,
                        name,
                        dyn_call,
                        ..
                    }) => Target::Trait {
                        module: *module,
                        name: name.clone(),
                        dyn_call: *dyn_call,
                    },
                    Some(Dispatch::Home { .. }) => {
                        return self
                            .refuse("home-module method calls in checked execution", e.span);
                    }
                    None => return self.refuse("this method call shape", e.span),
                };
                let self_mode = sig.params.first().and_then(|p| p.mode);
                let mut self_val = val!(self.eval_arg(recv, self_mode));
                let (body, subject) = match target {
                    Target::Inherent(ty_name) => {
                        let Some(&body) = self.methods.get(&(ty_name.clone(), sig.callee.clone()))
                        else {
                            return self.refuse("methods without resolvable bodies", e.span);
                        };
                        (body, ty_name)
                    }
                    Target::Trait {
                        module,
                        name,
                        dyn_call,
                    } => {
                        let concrete =
                            self.trait_concrete(recv.span, dyn_call, &self_val, e.span)?;
                        if let Value::Dyn { inner, .. } = self_val {
                            // The erased receiver enters the body as
                            // the concrete value (the data half).
                            self_val = *inner;
                        }
                        let body =
                            self.resolve_trait_body(&concrete, module, &name, &sig.callee, e.span)?;
                        (body, concrete)
                    }
                };
                self.pending_self_ty = Some(subject);
                let mut call_args = vec![self_val];
                for (i, a) in args.into_iter().flat_map(|l| l.args()).enumerate() {
                    let Some(v) = Arg::value(a) else { continue };
                    let mode = sig.params.get(i + 1).and_then(|p| p.mode);
                    call_args.push(val!(self.eval_arg(v, mode)));
                }
                let out = self.call_body(body, call_args)?;
                if let Value::ErrTag { .. } = out {
                    return Ok(raise(out));
                }
                Ok(Flow::Val(out))
            }
        }
    }

    fn eval_c_call(&mut self, name: &str, e: &'t GreenNode) -> E<Flow> {
        // kw02 (`[abi.c.import]`): a bodyless `extern "c" fn` is C the
        // program links; this machine has no C membrane, so the call is
        // refused by name — never modelled from its declared signature.
        if !name.starts_with("c.") {
            // kw05 (`[abi.asm.machines]`): a routine the manifest's
            // `asm` sources define is assembly, named as such.
            let routine = name.rsplit('.').next().unwrap_or(name);
            if on_asm_roster(routine) {
                let construct: &'static str = Box::leak(
                    format!(
                        "a call into assembly `{routine}` (a routine wolf.pkg's `asm` sources \
                         define; the checked machine has no assembly membrane)"
                    )
                    .into_boxed_str(),
                );
                return self.refuse(construct, e.span);
            }
            return self.refuse(
                "a call into C through a hand-declared `extern \"c\" fn` (the checked \
                 machine has no C membrane)",
                e.span,
            );
        }
        let d = CallExpr::cast(e).expect("kind");
        let mut args = Vec::new();
        for a in d.args().into_iter().flat_map(|l| l.args()) {
            if let Some(v) = Arg::value(a) {
                let x = if let Some(place) = found!(self.place_of(v)) {
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
        // `char` equality is scalar-value equality (D58), total.
        (Value::Char(x), Value::Char(y)) => x == y,
        // `byte` equality is octet equality (D72), total.
        (Value::Byte(x), Value::Byte(y)) => x == y,
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

/// Does a value's tag spell the same case as a pattern's name? Native
/// lowering compares tag IDS, so spelling never matters there; the
/// checked twin compares by full name or last dotted segment — a value
/// constructed as `Pairs.Pair` matches the arm `Pair` and vice versa
/// (s130).
fn tag_name_matches(value_tag: &str, pat_name: &str) -> bool {
    if value_tag == pat_name {
        return true;
    }
    let vlast = value_tag.rsplit('.').next().unwrap_or(value_tag);
    let plast = pat_name.rsplit('.').next().unwrap_or(pat_name);
    vlast == plast
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
/// Cook a str-literal PATTERN's source text into its runtime bytes:
/// quote strip, the shared escape set, `"""` dedent — the same steps
/// native lowering's `cooked_str_lit` takes (#54 lane parity).
fn cooked_str_pattern(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    if bytes.starts_with(b"\"\"\"") {
        let inner = &bytes[3..bytes.len().saturating_sub(3).max(3)];
        return decode_escapes(&dedent_multiline(inner));
    }
    // Raw literal (#76): the full delimiter strips, the inner bytes
    // are the value ([gram.lex.str.raw]).
    if let Some(inner) = raw_str_inner(bytes) {
        return inner.to_vec();
    }
    let inner = if bytes.len() >= 2 {
        &bytes[1..bytes.len() - 1]
    } else {
        bytes
    };
    decode_escapes(inner)
}

/// The inner bytes of a raw string literal's source text, or `None`
/// when the text is not raw-delimited. `r"…"`, `r#"…"#`, `r##"…"##` —
/// the whole opening delimiter (`r`, the `#` fence, the quote) and its
/// balancing close strip; what remains IS the value
/// ([gram.lex.str.raw]: no escapes, no interpolation). Byte-identical
/// with native lowering's implementation (wolf_wir::lower) — #76
/// retired the naive first/last-byte quote strip that left the
/// opening `"` of `r"` in the value.
fn raw_str_inner(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.first() != Some(&b'r') {
        return None;
    }
    let hashes = bytes[1..].iter().take_while(|&&b| b == b'#').count();
    let open = 1 + hashes; // index of the opening `"`
    if bytes.get(open) != Some(&b'"') {
        return None;
    }
    let start = open + 1;
    let end = bytes.len().saturating_sub(1 + hashes).max(start);
    Some(&bytes[start..end])
}

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

impl<'t> Machine<'t> {
    /// Render a hole or print value (s143, wolf-lang#268 —
    /// `[type.interp.value]`): the reference interpreter's `Display`,
    /// byte for byte. Type-directed where the value alone cannot tell:
    /// a positional `Struct` is a tuple or a named struct by its type,
    /// a nested field's type comes from the signature table, and a
    /// `!T` hole's ok half renders as its payload.
    fn render(&self, v: &Value, ty: Option<TyId>) -> String {
        let table = &self.ctx().tb.table;
        self.render_in(v, table, ty)
    }

    fn render_in(&self, v: &Value, table: &TypeTable, ty: Option<TyId>) -> String {
        let mut ty = ty;
        for _ in 0..32 {
            match ty.map(|t| table.kind(t)) {
                Some(TyKind::Wrapping(i) | TyKind::Distinct(i)) => ty = Some(*i),
                _ => break,
            }
        }
        // A `!T` hole: the ok payload as `T`, the row as itself.
        if let Some(TyKind::ErrUnion(ok, row)) = ty.map(|t| table.kind(t)) {
            return match v {
                Value::ErrTag { .. } => self.render_in(v, table, Some(*row)),
                _ => self.render_in(v, table, Some(*ok)),
            };
        }
        let kind = ty.map(|t| table.kind(t));
        match v {
            // s220 (wolf-lang#551, #538, `[type.interp.value]`): the value
            // the type holds — a `u64`/`uint` cell is its bit pattern.
            Value::Int(n) if matches!(kind, Some(TyKind::Prim(Prim::U64 | Prim::Uint))) => {
                (*n as u64).to_string()
            }
            Value::Int(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            // The shortest round-trip decimal, `std.fmt.decimal.to_str`'s
            // layout — the s38 reference rendering (spec §7.4 candidate).
            Value::F64(x) => wolf_sema::fmtspec::f64_shortest(*x),
            Value::Str(s) => s.clone(),
            // `{c}` prints the CHARACTER (D58), never the code point.
            Value::Char(c) => c.to_string(),
            // `{b}` prints the NUMBER (D72), never a character.
            Value::Byte(b) => b.to_string(),
            Value::Unit => "()".to_string(),
            Value::Range { start, end, .. } => format!("{start}..{end}"),
            Value::Struct { fields } => match kind {
                Some(TyKind::Nominal { module, name, .. }) => {
                    let sig_table = &self.tc.sigs.table;
                    let ftys: Vec<Option<TyId>> = match self.tc.sigs.get(*module as usize, name) {
                        Some(ItemSig::Struct(ss)) => fields
                            .iter()
                            .map(|(n, _)| ss.fields.iter().find(|f| &f.name == n).map(|f| f.ty))
                            .collect(),
                        _ => vec![None; fields.len()],
                    };
                    let mut out = format!("{name} {{");
                    for (i, ((fname, fv), fty)) in fields.iter().zip(ftys).enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        out.push(' ');
                        out.push_str(fname);
                        out.push_str(": ");
                        out.push_str(&self.render_in(fv, sig_table, fty));
                    }
                    out.push_str(" }");
                    out
                }
                other => {
                    // A tuple (positional fields), typed or not.
                    let tys: Vec<Option<TyId>> = match other {
                        Some(TyKind::Tuple(ts)) if ts.len() == fields.len() => {
                            ts.iter().map(|t| Some(*t)).collect()
                        }
                        _ => vec![None; fields.len()],
                    };
                    let items: Vec<String> = fields
                        .iter()
                        .zip(tys)
                        .map(|((_, fv), t)| self.render_in(fv, table, t))
                        .collect();
                    format!("({})", items.join(", "))
                }
            },
            Value::List(id) => {
                let et = match kind {
                    Some(TyKind::List(e)) => Some(*e),
                    _ => None,
                };
                let items: Vec<String> = self.lists[*id]
                    .iter()
                    .map(|item| self.render_in(item, table, et))
                    .collect();
                format!("[{}]", items.join(", "))
            }
            // A caught row value: the tag's name, then `(p1, p2)` when
            // the tag carries a payload — `[type.interp.row]`.
            Value::ErrTag { tag, payload } => {
                let ptys: Vec<Option<TyId>> = match kind {
                    Some(TyKind::Row { tags, .. }) => tags
                        .iter()
                        .find(|(n, _)| n == tag)
                        .map(|(_, p)| p.iter().map(|t| Some(*t)).collect())
                        .unwrap_or_else(|| vec![None; payload.len()]),
                    _ => vec![None; payload.len()],
                };
                let mut out = tag.clone();
                if !payload.is_empty() {
                    let items: Vec<String> = payload
                        .iter()
                        .zip(ptys)
                        .map(|(pv, t)| self.render_in(pv, table, t))
                        .collect();
                    out.push('(');
                    out.push_str(&items.join(", "));
                    out.push(')');
                }
                out
            }
            // An enum variant: `Enum.Variant`, as the interpreter
            // spells it (the machine records the qualified callee for
            // the call form; the bare member form is qualified here
            // from the type), then its payload.
            Value::Enum { variant, payload } => {
                let vname = variant.rsplit('.').next().unwrap_or(variant);
                let sig_table = &self.tc.sigs.table;
                let qualified = match kind {
                    _ if variant.contains('.') => variant.clone(),
                    Some(TyKind::Nominal { name, .. }) => format!("{name}.{vname}"),
                    _ => variant.clone(),
                };
                let ptys: Vec<Option<TyId>> = match kind {
                    Some(TyKind::Nominal { module, name, .. }) => {
                        match self.tc.sigs.get(*module as usize, name) {
                            Some(ItemSig::Enum { variants, .. }) => variants
                                .iter()
                                .find(|vs| vs.name == vname)
                                .map(|vs| vs.payload.iter().map(|t| Some(*t)).collect())
                                .unwrap_or_else(|| vec![None; payload.len()]),
                            _ => vec![None; payload.len()],
                        }
                    }
                    _ => vec![None; payload.len()],
                };
                let mut out = qualified;
                if !payload.is_empty() {
                    let items: Vec<String> = payload
                        .iter()
                        .zip(ptys)
                        .map(|(pv, t)| self.render_in(pv, sig_table, t))
                        .collect();
                    out.push('(');
                    out.push_str(&items.join(", "));
                    out.push(')');
                }
                out
            }
            Value::Dyn { inner, .. } => self.render_in(inner, table, None),
            _ => "<value>".to_string(),
        }
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

/// Unsigned literal as a bit pattern: full u64 range, decimal or
/// `0x…` hex — the native rung's `parse_uint_literal`, mirrored
/// (#130). Other spellings fall back to [`parse_int_literal`].
fn parse_uint_literal(text: &str) -> Option<u64> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16).ok();
    }
    t.parse::<u64>().ok()
}

fn prim_bits(p: Prim) -> Option<u32> {
    Some(match p {
        // `char` is 4 bytes (D58) but NOT an integer: no width for
        // the shift/arith rails — the casts are its only bridges.
        Prim::Char => return None,
        Prim::I8 | Prim::U8 | Prim::Byte => 8,
        Prim::I16 | Prim::U16 => 16,
        Prim::I32 | Prim::U32 | Prim::F32 => 32,
        Prim::I64 | Prim::U64 | Prim::Int | Prim::Uint | Prim::F64 => 64,
        Prim::Bool | Prim::Str => return None,
    })
}

/// The ledger bytes one list slot charges on this machine (s131's
/// 16-byte value slot), and the one exception s135 rules: a `byte`
/// element charges ONE byte, so `List[byte]` charges 1x its payload
/// here as it does natively ([type.byte], [mem.region.account.1] —
/// wolf-lang#203's ask, measured by `memory/byte_list_ledger.lu`).
fn slot_bytes(v: &Value) -> u64 {
    match v {
        Value::Byte(_) => 1,
        _ => 16,
    }
}

fn prim_size(p: Prim) -> u64 {
    // `char` has no arithmetic width (`prim_bits` is the shift rail)
    // but a fixed 4-byte layout (D58).
    if p == Prim::Char {
        return 4;
    }
    match prim_bits(p) {
        Some(b) => (b / 8) as u64,
        None => 1,
    }
}

/// The integer domain a FLOAT may be truncated into, as `[lo, hi)`
/// float edges: the truncated value must satisfy `lo <= t < hi`.
/// [`prim_range`] declines the 64-bit prims (they ride their own
/// checked arithmetic and have no narrowing question), but the
/// float→int trap of `[type.numlit.cast.trunc]` needs their edges by
/// name — `1e300 as int` has to find one. The upper edge is exclusive
/// because `i64::MAX as f64` rounds UP to 2^63, and `t == 2^63` is the
/// first value that does not fit; a `u64`/`uint` reaches 2^64 (s220,
/// wolf-lang#551). `byte` and `char` are not float targets and keep
/// their own cast arms.
fn int_cast_bounds(p: Prim) -> Option<(f64, f64)> {
    match p {
        Prim::I64 | Prim::Int => Some((i64::MIN as f64, 9_223_372_036_854_775_808.0)),
        Prim::U64 | Prim::Uint => Some((0.0, 18_446_744_073_709_551_616.0)),
        Prim::Byte | Prim::Char | Prim::Bool | Prim::Str | Prim::F32 | Prim::F64 => None,
        _ => prim_range(p).map(|(lo, hi)| (lo as f64, hi as f64 + 1.0)),
    }
}

/// s220 (wolf-lang#551, #538): a 64-bit unsigned integer type — `u64` or
/// `uint`, through `wrapping` and `distinct`. The checked machine holds
/// such a value as its bit pattern in an `i64` cell.
fn wide_unsigned(table: &TypeTable, mut id: TyId) -> bool {
    for _ in 0..32 {
        match table.kind(id) {
            TyKind::Wrapping(i) | TyKind::Distinct(i) => id = *i,
            _ => break,
        }
    }
    matches!(table.kind(id), TyKind::Prim(Prim::U64 | Prim::Uint))
}

/// s220 (wolf-lang#553, #602): a wrapping result as this machine holds
/// it — the low `bits` bits, read by the inner type's signedness. An
/// unsigned narrow value is the mask (non-negative); a signed narrow one
/// is sign-extended from its width, so `200 as wrapping[i8]` is held as
/// -56, prints -56 and orders below 0; a 64-bit value is its bits.
fn wrap_held(out: i64, mask: i64, bits: u32, unsigned: bool) -> i64 {
    if unsigned || bits >= 64 {
        out & mask
    } else {
        let sh = 64 - bits;
        (out << sh) >> sh
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
        // 64-bit prims ride their own checked arithmetic: `i64`'s, and
        // `u64`'s over the bit pattern a `u64`/`uint` is held as (s220,
        // wolf-lang#551; `wide_unsigned`).
        Prim::I64 | Prim::Int | Prim::U64 | Prim::Uint => return None,
        // `char`'s domain is not an interval (the surrogate gap):
        // the IntToChar cast arm owns its check, never this table.
        Prim::Bool | Prim::Str | Prim::F32 | Prim::F64 | Prim::Char => return None,
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

/// s199 (#424, `[os.fs.std]`): the first number an open answers; 0, 1
/// and 2 are the standard streams. `wolf_rt::fs::FIRST_HANDLE`'s twin.
const FS_FIRST_HANDLE: usize = 3;

/// s215 (`[os.fs.chdir]`): `p` as the checked machine's host call must
/// see it — joined to the machine-local working directory when the
/// program has moved it and `p` is relative, verbatim otherwise
/// (`[os.fs.path]`: no other rewriting; an empty path stays empty, so it
/// is `not_found` as the host says). A non-UTF-8 join is
/// unreachable: the base came from a `str` the host canonicalized.
fn resolve_in(base: &Option<std::path::PathBuf>, p: String) -> String {
    match base {
        Some(b) if !p.is_empty() && std::path::Path::new(&p).is_relative() => {
            b.join(&p).to_string_lossy().into_owned()
        }
        _ => p,
    }
}

/// s215 (`[os.proc.fds]`): `wolf_rt::os::map_of`'s twin — the flat
/// `[target, source, …]` list as pairs, `None` for a list that is not a
/// map (the `invalid` row): odd, a target outside `0..=255`, a repeated
/// target, a source below -1, more than 64 pairs. A source of -1 is
/// "closed in the child".
fn fd_map_of(flat: &[i64]) -> Option<Vec<(i64, Option<i64>)>> {
    if !flat.len().is_multiple_of(2) || flat.len() / 2 > 64 {
        return None;
    }
    let mut out: Vec<(i64, Option<i64>)> = Vec::with_capacity(flat.len() / 2);
    for pair in flat.chunks_exact(2) {
        let (t, s) = (pair[0], pair[1]);
        if !(0..=255).contains(&t) || s < -1 || out.iter().any(|&(u, _)| u == t) {
            return None;
        }
        out.push((t, (s >= 0).then_some(s)));
    }
    Some(out)
}

/// s215: `canonicalize`'s answer as the host's `getcwd` spells it. On
/// windows the call answers a verbatim path (`\\?\C:\…`, `\\?\UNC\…`)
/// that `GetCurrentDirectory` never reports, so `os_cwd` after a change
/// back to the first directory compared unequal (run 37555223637, windows
/// job 112579792864: `back=false`); the prefix is dropped. Elsewhere the
/// path is already the host's.
fn plain_path(p: std::path::PathBuf) -> std::path::PathBuf {
    if cfg!(windows) {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return std::path::PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\")
            && rest.as_bytes().get(1) == Some(&b':')
        {
            return std::path::PathBuf::from(rest);
        }
    }
    p
}

/// s215: may this process enter `dir`? chdir needs search permission;
/// unix asks `access(X_OK)`, elsewhere a directory is enterable.
#[cfg(unix)]
fn dir_searchable(dir: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt as _;
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: a NUL-terminated path the call only reads.
    unsafe { libc::access(c.as_ptr(), libc::X_OK) == 0 }
}

#[cfg(not(unix))]
fn dir_searchable(_dir: &std::path::Path) -> bool {
    true
}

/// s215 (`[os.proc.fds]`): spawn `exe` with `args` in `cwd` (the
/// process's own when `None`) and the child's descriptors placed as
/// `map` says — `wolf_rt::os::ChildTable::spawn_fds`'s mechanics,
/// mirrored: stdio inherited at the `Command` level so 0..2 are still
/// this process's inside the hook; every source staged above the
/// highest target, `dup2`'d onto its target, the stage closed; `None`
/// targets closed; the null device onto 0 when the map leaves 0 out.
/// The hook is async-signal-safe (fixed arrays, no allocation).
#[cfg(unix)]
fn spawn_mapped(
    exe: &str,
    args: &[String],
    map: &[(i64, Option<std::fs::File>)],
    cwd: Option<&std::path::Path>,
) -> std::io::Result<std::process::Child> {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::process::CommandExt as _;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    if map.is_empty() {
        cmd.stdin(std::process::Stdio::null());
        return cmd.spawn();
    }
    const MAX: usize = 64;
    let n = map.len();
    let mut targets = [-1 as libc::c_int; MAX];
    let mut sources = [-1 as libc::c_int; MAX];
    let mut floor: libc::c_int = 3;
    let mut names_zero = false;
    for (i, (t, s)) in map.iter().enumerate() {
        let t = *t as libc::c_int;
        targets[i] = t;
        sources[i] = s.as_ref().map_or(-1, |f| f.as_raw_fd());
        floor = floor.max(t + 1);
        names_zero |= t == 0;
    }
    // SAFETY: only async-signal-safe calls (`fcntl`, `dup2`, `close`,
    // `open` on a static path) over fixed arrays — the `pre_exec`
    // contract in a process that holds other threads.
    unsafe {
        cmd.pre_exec(move || {
            let mut staged = [-1 as libc::c_int; MAX];
            for i in 0..n {
                if sources[i] >= 0 {
                    let s = libc::fcntl(sources[i], libc::F_DUPFD_CLOEXEC, floor);
                    if s < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    staged[i] = s;
                }
            }
            for i in 0..n {
                if staged[i] >= 0 {
                    if libc::dup2(staged[i], targets[i]) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    libc::close(staged[i]);
                } else {
                    libc::close(targets[i]);
                }
            }
            if !names_zero {
                let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
                if null < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if null != 0 {
                    if libc::dup2(null, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    libc::close(null);
                }
            }
            Ok(())
        });
    }
    cmd.spawn()
}

/// s215: windows places no map (the caller answered `unsupported` for a
/// non-empty one), so only the plain spawn arrives here.
#[cfg(not(unix))]
fn spawn_mapped(
    exe: &str,
    args: &[String],
    _map: &[(i64, Option<std::fs::File>)],
    cwd: Option<&std::path::Path>,
) -> std::io::Result<std::process::Child> {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    cmd.spawn()
}

/// A file an fs call reads through: a standard stream duplicated for
/// the call, or a slot of the machine's table.
enum FsHandle<'a> {
    Std(std::fs::File),
    Table(&'a std::fs::File),
}

impl std::ops::Deref for FsHandle<'_> {
    type Target = std::fs::File;
    fn deref(&self) -> &std::fs::File {
        match self {
            FsHandle::Std(f) => f,
            FsHandle::Table(f) => f,
        }
    }
}

/// s200 (#405): is `fd` one of the three standard streams?
fn fs_is_std(fd: i64) -> bool {
    (0..FS_FIRST_HANDLE as i64).contains(&fd)
}

thread_local! {
    /// s200 (#407, `[os.fs.error]`): the host's number for the fs call in
    /// flight — cleared by [`Machine::io_fs_builtin`] on entry, set by
    /// [`fs_host_error`] when the host refuses, and copied into the
    /// machine's word when the call returns. The machine runs on one
    /// thread, so the transport cannot cross runs.
    static FS_HOST_ERROR: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// Note the host's number for `e` (0 when std built the error itself).
fn fs_host_error(e: &std::io::Error) {
    FS_HOST_ERROR.with(|c| c.set(e.raw_os_error().map_or(0, i64::from)));
}

/// `os_error_text(code)`: `wolf_rt::fs::os_error_text`'s answer — std's
/// rendering of the host's message without its ` (os error N)` suffix,
/// "" for a code at or below zero or outside the host's `i32`. The
/// runtime is not a dependency of this crate (D15), so the dozen lines
/// are mirrored and the driver's parity test holds them equal.
pub fn host_error_text(code: i64) -> String {
    let Some(n) = i32::try_from(code).ok().filter(|n| *n > 0) else {
        return String::new();
    };
    let full = std::io::Error::from_raw_os_error(n).to_string();
    let suffix = format!(" (os error {n})");
    full.strip_suffix(&suffix)
        .unwrap_or(&full)
        .trim_end()
        .to_string()
}

/// Descriptor 0, 1 or 2, duplicated (`dup`; `DuplicateHandle`): the
/// duplicate shares the stream's offset, so a seek through it is a
/// seek of the stream, and dropping it closes only the duplicate.
fn std_stream_dup(fd: i64) -> Option<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd as _;
        let owned = match fd {
            0 => std::io::stdin().as_fd().try_clone_to_owned(),
            1 => std::io::stdout().as_fd().try_clone_to_owned(),
            2 => std::io::stderr().as_fd().try_clone_to_owned(),
            _ => return None,
        };
        owned.ok().map(std::fs::File::from)
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsHandle as _;
        let owned = match fd {
            0 => std::io::stdin().as_handle().try_clone_to_owned(),
            1 => std::io::stdout().as_handle().try_clone_to_owned(),
            2 => std::io::stderr().as_handle().try_clone_to_owned(),
            _ => return None,
        };
        owned.ok().map(std::fs::File::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = fd;
        None
    }
}

/// s199: is `f` a disk file, by the host's own classification?
/// `wolf_rt::fs::is_disk`'s twin: unix `true` (the mode bits and
/// `ESPIPE` already tell), windows `GetFileType == FILE_TYPE_DISK` —
/// std's metadata calls an anonymous pipe a regular file there
/// (wolf-lang CI run 37062798818, job 111023227162).
#[cfg(windows)]
fn fs_is_disk(f: &std::fs::File) -> bool {
    use std::os::windows::io::AsRawHandle as _;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileType(h: *mut core::ffi::c_void) -> u32;
    }
    const FILE_TYPE_DISK: u32 = 1;
    // SAFETY: the handle is `f`'s, live for the duration of the call.
    unsafe { GetFileType(f.as_raw_handle()) == FILE_TYPE_DISK }
}

#[cfg(not(windows))]
fn fs_is_disk(_f: &std::fs::File) -> bool {
    true
}

/// One positional read at `off`, the cursor untouched — `pread` on
/// unix; on windows `seek_read` moves the file pointer, so it is put
/// back. `wolf_rt::fs::read_at`'s twin; an interrupted read is retried.
fn fs_read_at_host(f: &std::fs::File, buf: &mut [u8], off: u64) -> std::io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt as _;
        loop {
            match f.read_at(buf, off) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                r => return r,
            }
        }
    }
    #[cfg(windows)]
    {
        use std::io::{Seek as _, SeekFrom};
        use std::os::windows::fs::FileExt as _;
        let mut g = f;
        let at = g.stream_position()?;
        let r = f.seek_read(buf, off);
        g.seek(SeekFrom::Start(at))?;
        r
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (f, buf, off);
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
}
