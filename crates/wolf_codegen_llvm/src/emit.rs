//! WIR function → LLVM IR text (s41).
//!
//! The structural 80% mirrors the Cranelift translator (the two tiers
//! share WIR-in and the `wolf_backend::abi` plans; they diverge at
//! instruction selection). The engineered 20% is THE POINT of this
//! sprint — the fact encoding:
//!
//! - **Region identity → scoped-noalias domains**: one metadata domain
//!   per function, one `!alias.scope` node per region CLASS; every WIR
//!   load/store carries its own class's scope and every OTHER class's
//!   scope in `!noalias`. Region disjointness is a c04 theorem (D3):
//!   interprocedural-strength aliasing C and Rust cannot express. The
//!   class, not the region id, is the unit — every `region.foreign`
//!   region of one role shares a scope, because a foreign root is
//!   per-function over storage it does not own and two of them can name
//!   the same bytes (s80; the region-keyed version miscompiled).
//! - **`mut`/`read` params → `noalias`/`readonly` + `noundef`**
//!   argument attributes; `deref` facts add `dereferenceable(n)` at
//!   the definition site.
//! - **Frozen memory → `!invariant.load`**: loads through a
//!   `sync.freeze`-minted token (or a pointer with a `frozen` fact)
//!   are never invalidated.
//! - **`range` facts → `!range`** on integer loads (X3's claw-back);
//!   checked-arith postconditions need no separate metadata — the
//!   overflow-checked CFG itself carries them to LLVM.
//! - **Alignment facts (report 10 delta 2)**: `region.alloc` /
//!   `stack.alloc` fix 16-byte alignment, emitted as `align 16` on the
//!   allocation's pointer; scalar accesses carry their natural
//!   alignment (the same claim the debug tier's `MemFlags::trusted`
//!   makes).
//! - **Semantic coldness (report 10 delta 3)**: every trap arc and
//!   every error-row arc (`br` on `eu.is_err`) gets cold
//!   `!prof` branch weights; trap bodies are out-of-line blocks
//!   calling the `cold noreturn` runtime trap shim, then `unreachable`.
//! - **No unwinding, ever (D30)**: every define and declare is
//!   `nounwind`; `invoke`/`landingpad` are unrepresentable in this
//!   emitter (a CI check asserts they never appear).
//!
//! The region scope channel reaches EVERY load and store through a
//! region-tagged pointer, including the ones whose pointer was itself
//! loaded out of region memory — a container element access, whose
//! address comes from the header's `data` field, is scoped by the
//! region its token names, exactly like an access through an
//! allocation result (s78 acceptance, machine-checked by
//! `every_region_access_carries_scopes`; fuzzed by the
//! `loaded_pointer` shape).
//!
//! ## What is NOT claimed (s78, wolf-lang#82)
//!
//! Facts declined for want of a theorem — the asymmetry that makes
//! this list mandatory is that a dropped fact costs speed while a
//! wrong one costs correctness (D2):
//!
//! - **Per-container buffer disjointness.** Two containers' element
//!   buffers get the SAME scope, so no `!noalias` separates them. `let
//!   b = a` copies a header pointer, so two `List` values may share one
//!   buffer; c04's exclusivity theorem is a syntactic path claim about
//!   the argument PLACES (`[mem.model.path.disjoint]`), and
//!   `wolf_mem`'s place language has no dereference projection at all.
//!   Splitting the buffers would hand LLVM a `!noalias` pair nothing
//!   proved. It needs a c04 payload-disjointness theorem first; the
//!   lowering makes the same call for the same reason (`ops.rs`, the
//!   `region.foreign` roots).
//! - **`read`-parameter payload immutability.** `!invariant.load` is
//!   NOT propagated through a load: `frozen` is claimed only where a
//!   `sync.freeze` minted the token (`[mem.region.freeze.1]`: deep, in
//!   place, forever). The `read` mode's "immutable for the whole call"
//!   is enforced as a per-binding syntactic rule inside the callee, so
//!   a second path to the same payload — including the `copy` the
//!   E1014 diagnostic recommends — is invisible to it. A deep claim
//!   here would be a miscompile, not a missed hoist.
//! - **Call-site region effects, for FOREIGN roots only.** s78
//!   declined this fact whole; s80 audited the decline and let it
//!   stand; s83 SPENT THE HALF THAT HAS A THEOREM and left the rest
//!   declined for a stated reason. A call now carries `!noalias` over
//!   every region [`wolf_wir::midend::exhaustive_regions`] admits and
//!   whose token the call does not take — see
//!   [`Fx::call_noalias_suffix`]. What is still not claimed is the
//!   foreign roots: a foreign root is minted per function over storage
//!   the RUNTIME owns, so a callee reaches a caller's container buffers
//!   by minting its own root while holding none of the caller's tokens
//!   (s80, wolf-lang#83). "No token ⇒ no effect" is FALSE there, and
//!   claiming it would be s80's miscompile handed to LLVM on purpose.
//!   Making it true needs the tokens PROPAGATED THROUGH SIGNATURES —
//!   wolf-lang#91, whose ABI inventory s83 wrote and whose size is why
//!   s83 did not land it. The predicate is IMPORTED from the mid-end
//!   rather than re-derived here, so this file and `memopt`/`licm`
//!   cannot drift: an unenforced asymmetry in this file is how the
//!   miscompile below happened.
//!
//! What s80 found while auditing that decline was NOT a declined fact
//! but an ASSERTED one that was false: one `!alias.scope` per region
//! id. See [`Fx::build_scopes`] — after inlining, two foreign roots
//! over the same buffers were told they could not alias, and LLVM
//! forwarded a stale load across a store on the strength of it. Scopes
//! are keyed by region CLASS now.
//!
//! Every fact channel is disabled by [`EmitOptions::strip_facts`] —
//! the metadata-stripped control lane of the s41 differential fuzz rig
//! (and the s44 sentinel's substrate). Dropping facts is always sound
//! (D2); *wrong* facts miscompile, which is why the rig exists.
//!
//! Determinism: this is a pure text emitter — iteration orders are
//! layout/BTree orders throughout, so byte-identical WIR yields
//! byte-identical IR (an s41 acceptance criterion, tested).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use wolf_backend::BackendError;
use wolf_backend::abi::{self, Conv, ParamPass, RegClass, RetPass, Unit};
use wolf_backend::layout::{self, Layout};
use wolf_wir::entity::EntityRef;
use wolf_wir::facts::{DerefSize, FactKind};
use wolf_wir::ir::{
    Aux, Block as WBlock, Function as WFunction, Mode, Module as WModule, SigId, Value as WValue,
    ValueDef,
};
use wolf_wir::ops::{FloatCc, ForeignRole, IntCc, Opcode, TrapKind};
use wolf_wir::types::{TypeData, TypeId};

/// What one `!alias.scope` node stands for (s80). Regions are NOT the
/// unit: every foreign region of one [`ForeignRole`] names the same
/// class of runtime-owned storage and shares a scope, while every local
/// region keeps its own. See [`FnCx::build_scopes`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum ScopeClass {
    Local(u32),
    Foreign(ForeignRole),
}

/// Per-function block execution counts from a `.wprof`, positional
/// over [`wolf_wir::block_order`], keyed by WIR function name (s45).
///
/// Built by `wolf_wir::midend::branch_weights` against the FINAL
/// module, so a name is present only when this build's body for it
/// hashes to a profile record — the counts therefore describe these
/// blocks, one for one, or they are not here at all.
pub type BranchWeights = std::sync::Arc<BTreeMap<String, Vec<u64>>>;

/// How the emitter treats the proven-fact channels.
#[derive(Clone, Debug, Default)]
pub struct EmitOptions {
    /// Emit NO optimizer-feeding facts: no `!alias.scope`/`!noalias`,
    /// no `!invariant.load`, no `!range`, no `!prof` weights, no
    /// `noalias`/`readonly`/`dereferenceable`/`noundef` attributes,
    /// alignment claims dropped to 1. ABI-required attributes
    /// (`sret`/`byval`) stay — they are calling convention, not facts.
    /// This is the fuzz rig's control lane.
    pub strip_facts: bool,
    /// PGO (s45): measured block counts, so LLVM's block placement and
    /// register allocator see the same hotness wolf's own mid-end
    /// does. `None` — no profile — is the default and the normal case,
    /// and produces byte-identical IR to a pre-s45 build. Also
    /// silenced by `strip_facts`, since `!prof` is a fact channel like
    /// any other (the s79 sentinel prices it).
    pub branch_weights: Option<BranchWeights>,
}

/// Trap codes shared with `wolf_rt` (the single authority — see the
/// unit test pinning these to `wolf_rt::native::trap_code`).
pub(crate) fn trap_code(kind: TrapKind) -> i32 {
    match kind {
        TrapKind::Overflow => 1,
        TrapKind::DivZero => 2,
        TrapKind::Bounds => 3,
        TrapKind::Assert => 4,
    }
}

/// One declared function the emitter can call (the callee's sig
/// arrives with each call site's `ExtFunc`, so only the linker symbol
/// and its convention live here).
pub(crate) struct FuncEntry {
    pub symbol: String,
    pub export: bool,
}

/// Module-level accumulators shared by every function emission.
pub(crate) struct ModuleCx {
    pub opts: EmitOptions,
    /// Referenced external symbols → full `declare` line. BTreeMap so
    /// the rendered order is deterministic.
    pub decls: BTreeMap<String, String>,
    /// Data globals in first-reference order (an object carries
    /// exactly the data it uses, like the debug tier).
    pub globals: Vec<String>,
    pub global_names: HashSet<String>,
    /// Intrinsic `declare` lines (sorted at render).
    pub intrinsics: BTreeSet<String>,
    /// Metadata nodes; id = index.
    pub meta: Vec<String>,
    /// The cold-branch `!prof` node, minted on first use.
    prof_cold: Option<usize>,
    /// Measured `!prof` nodes, interned by their (then, else) weights
    /// so a module with one hot loop shape carries one node.
    prof_weights: BTreeMap<(u32, u32), usize>,
}

impl ModuleCx {
    pub fn new(opts: EmitOptions) -> ModuleCx {
        ModuleCx {
            opts,
            decls: BTreeMap::new(),
            globals: Vec::new(),
            global_names: HashSet::new(),
            intrinsics: BTreeSet::new(),
            meta: Vec::new(),
            prof_cold: None,
            prof_weights: BTreeMap::new(),
        }
    }

    fn meta_node(&mut self, body: String) -> usize {
        self.meta.push(body);
        self.meta.len() - 1
    }

    /// `!prof` node marking the FIRST successor cold (weights 1 vs
    /// 2000 — the uniform semantic-coldness rule, report 10 delta 3).
    fn prof_cold_first(&mut self) -> usize {
        if let Some(id) = self.prof_cold {
            return id;
        }
        let id = self.meta_node("!{!\"branch_weights\", i32 1, i32 2000}".to_string());
        self.prof_cold = Some(id);
        id
    }

    /// A measured `!prof` node for a two-way branch (s45).
    ///
    /// LLVM's weights are relative i32s, and a training run's counts
    /// are u64s with no bound, so the pair is rescaled into
    /// `1..=PROF_WEIGHT_SCALE` preserving their ratio. Both weights
    /// are at least 1: a zero weight tells LLVM an edge is
    /// unreachable, which is a claim a profile is not entitled to make
    /// — "this arm did not run on the training input" is not "this arm
    /// cannot run", and the difference is a miscompile.
    fn prof_measured(&mut self, then: u64, other: u64) -> usize {
        let (t, e) = scale_weights(then, other);
        if let Some(&id) = self.prof_weights.get(&(t, e)) {
            return id;
        }
        let id = self.meta_node(format!("!{{!\"branch_weights\", i32 {t}, i32 {e}}}"));
        self.prof_weights.insert((t, e), id);
        id
    }
}

/// Blocks with exactly one predecessor — the blocks whose execution
/// count is also the count of the single arc into them. The entry
/// block is excluded: its "predecessor" is the caller.
fn single_pred_blocks(f: &WFunction) -> HashSet<WBlock> {
    let mut preds: BTreeMap<WBlock, u32> = BTreeMap::new();
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            match f.insts[inst].aux {
                Aux::Jump(e) => *preds.entry(e.block).or_insert(0) += 1,
                Aux::Br(t, e) => {
                    *preds.entry(t.block).or_insert(0) += 1;
                    *preds.entry(e.block).or_insert(0) += 1;
                }
                _ => {}
            }
        }
    }
    preds
        .into_iter()
        .filter(|&(_, n)| n == 1)
        .map(|(b, _)| b)
        .collect()
}

/// The top of the rescaled `!prof` weight range.
const PROF_WEIGHT_SCALE: u64 = 100_000;

/// Rescale a measured count pair into `1..=PROF_WEIGHT_SCALE`,
/// preserving the ratio and never producing a zero. Pure and integral,
/// so the emitted IR is a function of the profile and nothing else.
fn scale_weights(then: u64, other: u64) -> (u32, u32) {
    let top = then.max(other);
    if top == 0 {
        return (1, 1); // both arms unmeasured: say nothing
    }
    let scale = |c: u64| -> u32 {
        let v = (u128::from(c) * u128::from(PROF_WEIGHT_SCALE)) / u128::from(top);
        (v as u64).max(1) as u32
    };
    (scale(then), scale(other))
}

fn ice(msg: impl Into<String>) -> BackendError {
    BackendError::Internal(msg.into())
}

fn nyi(msg: impl Into<String>) -> BackendError {
    BackendError::Unsupported(msg.into())
}

/// One loaded byte chunk of a clobber-safe copy: (offset, type, temp).
type Chunk = (u32, &'static str, String);

/// The LLVM value type of a WIR scalar. Bools ride `i8` end-to-end
/// (the ABI's crossing type); `i1` exists only transiently at compare
/// results and branch conditions.
fn scalar_ty(m: &WModule, ty: TypeId) -> Option<&'static str> {
    Some(match m.types.get(ty) {
        TypeData::I8 | TypeData::Bool => "i8",
        TypeData::I16 => "i16",
        TypeData::I32 => "i32",
        TypeData::I64 => "i64",
        TypeData::F32 => "float",
        TypeData::F64 => "double",
        TypeData::Ptr => "ptr",
        TypeData::Mem(_) | TypeData::Io | TypeData::Agg(_) | TypeData::Eu { .. } => return None,
    })
}

fn is_agg(m: &WModule, ty: TypeId) -> bool {
    matches!(m.types.get(ty), TypeData::Agg(_) | TypeData::Eu { .. })
}

/// Natural alignment claimed for a scalar access (parity with the
/// debug tier's `MemFlags::trusted`, which claims aligned on every
/// access the verifier blessed).
fn natural_align(ty: &str) -> u32 {
    match ty {
        "i8" => 1,
        "i16" => 2,
        "i32" | "float" => 4,
        _ => 8,
    }
}

/// The LLVM type carrying one ABI eightbyte unit.
fn unit_ty(u: &Unit) -> &'static str {
    match u.class {
        RegClass::Int => "i64",
        RegClass::Sse => {
            if u.bytes <= 4 {
                "float"
            } else {
                "double"
            }
        }
    }
}

/// How one WIR parameter crosses the boundary (plan execution shape).
#[derive(Clone, Debug)]
pub(crate) enum Slot {
    Direct(&'static str),
    Split(Vec<Unit>),
    /// Wolf MEMORY-class aggregate: by pointer.
    Ref,
    /// C MEMORY-class aggregate: byval, (size, align).
    CStruct(u32, u32),
    Token,
}

/// How one WIR result crosses the boundary.
#[derive(Clone, Debug)]
pub(crate) enum RSlot {
    Direct(&'static str),
    Split(Vec<Unit>),
    Sret {
        disc_in_reg: bool,
        c: bool,
        size: u32,
        align: u32,
    },
    Token,
}

/// The LLVM view of one WIR signature under one convention.
pub(crate) struct LSig {
    pub params: Vec<Slot>,
    pub rets: Vec<RSlot>,
    /// Rendered LLVM parameter list entries (`ty attrs`), call order.
    pub ll_params: Vec<String>,
    /// The flat return components (empty = void).
    pub ret_comps: Vec<String>,
    pub sret_first: bool,
}

impl LSig {
    pub fn ret_ty(&self) -> String {
        match self.ret_comps.len() {
            0 => "void".to_string(),
            1 => self.ret_comps[0].clone(),
            _ => format!("{{ {} }}", self.ret_comps.join(", ")),
        }
    }
}

/// Build the LLVM signature view, executing the shared plan from
/// [`wolf_backend::abi`] (all classification lives THERE).
pub(crate) fn sig_info(m: &WModule, sig: SigId, conv: Conv, opts: &EmitOptions) -> LSig {
    let data = &m.sigs[sig];
    let plan = abi::plan_sig(&m.types, data, conv);
    let sret_first = conv == Conv::C;
    let mut ll_params = Vec::new();
    let mut ret_comps = Vec::new();
    let mut rets = Vec::with_capacity(data.results.len());
    let mut sret_trailing: Vec<String> = Vec::new();
    for (pass, &r) in plan.rets.iter().zip(data.results.iter()) {
        match pass {
            RetPass::Token => rets.push(RSlot::Token),
            RetPass::Direct(ty) => {
                let t = scalar_ty(m, *ty).expect("scalar result");
                ret_comps.push(t.to_string());
                rets.push(RSlot::Direct(t));
            }
            RetPass::Split(units) => {
                for u in units {
                    ret_comps.push(unit_ty(u).to_string());
                }
                rets.push(RSlot::Split(units.clone()));
            }
            RetPass::Sret { disc_in_reg } => {
                let l = layout::layout_of(&m.types, r).expect("sized result");
                if sret_first {
                    // psABI sret: pointer first; LLVM handles the %rax
                    // return itself.
                    ll_params.push(format!("ptr sret([{} x i8]) align {}", l.size, l.align));
                } else {
                    // Wolf-native: a PLAIN trailing pointer param. The
                    // out slot is freshly reserved by every caller —
                    // noalias is a truth, not a hope.
                    sret_trailing.push(if opts.strip_facts {
                        "ptr".to_string()
                    } else {
                        "ptr noalias".to_string()
                    });
                    if *disc_in_reg {
                        // [abi.err.repr]: the discriminant still rides
                        // the first INTEGER return register.
                        ret_comps.push("i64".to_string());
                    }
                }
                rets.push(RSlot::Sret {
                    disc_in_reg: *disc_in_reg,
                    c: sret_first,
                    size: l.size,
                    align: l.align,
                });
            }
        }
    }
    let mut params = Vec::with_capacity(data.params.len());
    for (pass, p) in plan.params.iter().zip(data.params.iter()) {
        match pass {
            ParamPass::Token => params.push(Slot::Token),
            ParamPass::Direct(ty) => {
                let t = scalar_ty(m, *ty).expect("scalar param");
                let attrs = if t == "ptr" && !opts.strip_facts {
                    // The c04 mode theorems, as argument attributes
                    // (protector-equivalent: valid for the whole call).
                    match p.mode {
                        Mode::Mut => " noalias noundef",
                        Mode::Read => " noalias readonly noundef",
                        Mode::Val | Mode::Take => "",
                    }
                } else {
                    ""
                };
                ll_params.push(format!("{t}{attrs}"));
                params.push(Slot::Direct(t));
            }
            ParamPass::Split(units) => {
                for u in units {
                    ll_params.push(unit_ty(u).to_string());
                }
                params.push(Slot::Split(units.clone()));
            }
            ParamPass::Memory => match conv {
                Conv::Wolf => {
                    // By-value aggregate passed by pointer; SSA
                    // immutability means the callee only reads it.
                    ll_params.push(if opts.strip_facts {
                        "ptr".to_string()
                    } else {
                        "ptr readonly".to_string()
                    });
                    params.push(Slot::Ref);
                }
                Conv::C => {
                    let l = layout::layout_of(&m.types, p.ty).expect("sized param");
                    ll_params.push(format!(
                        "ptr byval([{} x i8]) align {}",
                        l.size.next_multiple_of(8),
                        l.align.max(8)
                    ));
                    params.push(Slot::CStruct(l.size, l.align));
                }
            },
        }
    }
    ll_params.extend(sret_trailing);
    LSig {
        params,
        rets,
        ll_params,
        ret_comps,
        sret_first,
    }
}

/// How a WIR value exists in the IR text.
#[derive(Clone, Debug)]
enum Repr {
    /// One LLVM operand string: a `%name` or an inline constant.
    Scalar(String),
    /// An aggregate/error union: the operand naming its bytes' address.
    Addr(String),
    /// An effect token: nothing.
    Token,
}

/// One rendered LLVM basic block.
struct LBlock {
    label: String,
    lines: Vec<String>,
}

/// Phi accumulation for one scalar block parameter.
struct PhiAcc {
    name: String,
    ty: &'static str,
    incoming: Vec<(String, String)>,
}

pub(crate) struct Fx<'a> {
    cx: &'a mut ModuleCx,
    m: &'a WModule,
    f: &'a WFunction,
    conv: Conv,
    funcs: &'a HashMap<String, FuncEntry>,
    /// Symbols defined in this module (no `declare` for them).
    defined: &'a HashSet<String>,
    vals: HashMap<WValue, Repr>,
    /// Entry-block `alloca` lines (LLVM allocas must not sit in loops).
    allocas: Vec<String>,
    blocks: Vec<LBlock>,
    cur: usize,
    wlabel: HashMap<WBlock, String>,
    /// WIR block → scalar-param phi accumulators (layout param order).
    phis: HashMap<WBlock, Vec<PhiAcc>>,
    /// Aggregate block params own an entry alloca.
    param_slots: HashMap<(WBlock, usize), String>,
    tmp: u32,
    nslot: u32,
    /// Per-kind cold trap blocks (BTreeMap: deterministic render).
    trap_blocks: BTreeMap<i32, String>,
    /// Fact indexes over this function's values.
    deref: HashMap<WValue, u64>,
    range: HashMap<WValue, (i128, i128)>,
    frozen: HashSet<WValue>,
    /// Region index → its `!alias.scope` node id.
    region_scope: BTreeMap<u32, usize>,
    /// Region index → (`!alias.scope` LIST id, `!noalias` LIST id).
    scope_lists: BTreeMap<u32, (usize, Option<usize>)>,
    /// Regions whose effect token names every writer of their storage
    /// (`wolf_wir::midend::exhaustive_regions`) — the call-site
    /// `!noalias` fact's whole license (s83).
    exhaustive: HashSet<u32>,
    /// Interned `!noalias` lists for call sites, keyed by scope-id set:
    /// one node per distinct set keeps the metadata small and the
    /// output byte-stable.
    /// PGO (s45): measured execution count per WIR block, when a
    /// profile record matched this body. Empty otherwise — and empty
    /// means "say nothing to LLVM", never "say zero".
    bcount: HashMap<WBlock, u64>,
    call_noalias: BTreeMap<Vec<usize>, usize>,
    /// Entry sret pointers, in aggregate-result order.
    sret: Vec<String>,
    reachable: HashSet<WBlock>,
}

/// Emit one WIR function as an LLVM `define`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_function(
    cx: &mut ModuleCx,
    m: &WModule,
    f: &WFunction,
    symbol: &str,
    linkage: &str,
    funcs: &HashMap<String, FuncEntry>,
    defined: &HashSet<String>,
) -> Result<String, BackendError> {
    let conv = if f.export { Conv::C } else { Conv::Wolf };
    let opts = cx.opts.clone();
    let si = sig_info(m, f.sig, conv, &opts);
    // PGO block counts for THIS body, positional over the canonical
    // order. Silenced by `strip_facts` with every other fact channel.
    //
    // Only blocks with EXACTLY ONE predecessor are kept, because only
    // for those is the block count also the EDGE count — and an edge
    // count is what `!prof` means. A join reached from several places
    // carries the sum of all of them, and handing that to LLVM as the
    // weight of one incoming arc overstates that arc by however much
    // the others contributed. The profile is instrumented per block,
    // not per edge (edge counters are the sprint's next rung, not this
    // one), so the honest move is to say nothing where the block count
    // is not the edge count.
    let bcount: HashMap<WBlock, u64> = match (&opts.branch_weights, opts.strip_facts) {
        (Some(w), false) => match w.get(&f.name) {
            Some(counts) => {
                let order = wolf_wir::block_order(f);
                // Length equality is checked where the map is built;
                // re-check here so a hand-constructed map cannot shift
                // a body's weights by one.
                if counts.len() == order.len() {
                    let single = single_pred_blocks(f);
                    order
                        .into_iter()
                        .zip(counts.iter().copied())
                        .filter(|(b, _)| single.contains(b))
                        .collect()
                } else {
                    HashMap::new()
                }
            }
            None => HashMap::new(),
        },
        _ => HashMap::new(),
    };
    let mut fx = Fx {
        cx,
        m,
        f,
        conv,
        funcs,
        defined,
        vals: HashMap::new(),
        allocas: Vec::new(),
        blocks: Vec::new(),
        cur: 0,
        wlabel: HashMap::new(),
        phis: HashMap::new(),
        param_slots: HashMap::new(),
        tmp: 0,
        nslot: 0,
        trap_blocks: BTreeMap::new(),
        deref: HashMap::new(),
        range: HashMap::new(),
        frozen: HashSet::new(),
        region_scope: BTreeMap::new(),
        scope_lists: BTreeMap::new(),
        exhaustive: if opts.strip_facts {
            HashSet::new()
        } else {
            wolf_wir::midend::exhaustive_regions(m, f)
        },
        bcount,
        call_noalias: BTreeMap::new(),
        sret: Vec::new(),
        reachable: HashSet::new(),
    };
    fx.index_facts();
    fx.compute_reachable()?;
    fx.build_scopes();
    fx.run(&si)?;
    fx.render(symbol, linkage, &si)
}

impl<'a> Fx<'a> {
    // ---- pre-passes ------------------------------------------------------

    fn index_facts(&mut self) {
        if self.cx.opts.strip_facts {
            return;
        }
        for (_, fact) in self.f.facts.iter() {
            match fact.kind {
                FactKind::Deref(v, DerefSize::Const(n)) => {
                    let e = self.deref.entry(v).or_insert(n);
                    *e = (*e).max(n);
                }
                FactKind::Deref(..) => {} // scaled: no constant claim at v0
                FactKind::Range(v, lo, hi) => {
                    self.range.insert(v, (lo, hi));
                }
                FactKind::Frozen(v) => {
                    self.frozen.insert(v);
                }
                // Pairwise noalias reaches LLVM through the mode
                // attributes and the region scope domains; region
                // provenance through the scope assignment below.
                FactKind::Noalias(..) | FactKind::Region(..) => {}
            }
        }
    }

    fn compute_reachable(&mut self) -> Result<(), BackendError> {
        let Some(entry) = self.f.entry() else {
            return Err(ice("function has no entry block"));
        };
        let mut stack = vec![entry];
        while let Some(b) = stack.pop() {
            if !self.reachable.insert(b) {
                continue;
            }
            let Some(&term) = self.f.blocks[b].insts.last() else {
                continue;
            };
            match self.f.insts[term].aux {
                Aux::Jump(bc) => stack.push(bc.block),
                Aux::Br(t, e) => {
                    stack.push(t.block);
                    stack.push(e.block);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// One scoped-noalias domain per function, one scope per region
    /// CLASS that this function's memory ops touch. Naming is
    /// deterministic (`fname.rN`, `fname.foreign.ROLE`) and survives
    /// clustering because the domain node is `distinct` — merges rename
    /// nothing (report 10: the class of historical bugs is scope
    /// CLONING, which the fuzz rig watches).
    ///
    /// # Why CLASS and not region (s80, wolf-lang#83)
    ///
    /// One scope per region id was a miscompile. A `region.foreign`
    /// root is minted per FUNCTION over storage the runtime owns, so
    /// inlining a container-touching callee leaves the caller holding
    /// two roots — its own and the callee's, freshened by the inliner
    /// — over the SAME element buffers. Under one-scope-per-region that
    /// pair became a `!noalias` claim, and LLVM cashed it: a store
    /// through the caller's root was declared not to alias a load
    /// through the inlined root, so the load forwarded a stale value
    /// across it (the s80 witness, `fuzzgen::shape_foreign_inline_unify`).
    ///
    /// The class is therefore: a local region (`region.new`,
    /// `stack.alloc`, a token parameter) is its own class — a bump
    /// arena's allocations really are disjoint from another's; every
    /// foreign region with the SAME [`ForeignRole`] shares ONE class,
    /// because they name one class of runtime storage and nothing
    /// separates them. The header/buffer separation survives intact
    /// (D46: the runtime always allocates them apart), which is the
    /// whole disjointness s75 was after.
    fn build_scopes(&mut self) {
        if self.cx.opts.strip_facts {
            return;
        }
        // Region id -> role, for every foreign root in this function.
        let mut foreign: BTreeMap<u32, ForeignRole> = BTreeMap::new();
        let mut regions: BTreeSet<u32> = BTreeSet::new();
        for &b in &self.f.layout {
            if !self.reachable.contains(&b) {
                continue;
            }
            for &i in &self.f.blocks[b].insts {
                let data = self.f.insts[i];
                if data.op == Opcode::RegionForeign
                    && let Some(&tok) = self.f.vpool.get(data.results).first()
                    && let TypeData::Mem(r) = self.m.types.get(self.f.value_ty(tok))
                    && let Aux::Int(code) = data.aux
                    && let Some(role) = ForeignRole::from_code(code)
                {
                    foreign.insert(r.index() as u32, role);
                }
                let tok = match data.op {
                    Opcode::Load => self.f.vpool.get(data.args).get(1).copied(),
                    Opcode::Store => self.f.vpool.get(data.args).get(2).copied(),
                    _ => None,
                };
                if let Some(tok) = tok
                    && let TypeData::Mem(r) = self.m.types.get(self.f.value_ty(tok))
                {
                    regions.insert(r.index() as u32);
                }
            }
        }
        // A single region yields no exploitable disjointness, but the
        // scope still names the memory (harmless, deterministic).
        if regions.is_empty() {
            return;
        }
        let fname = &self.f.name;
        let domain = self
            .cx
            .meta_node(format!("distinct !{{!\"wolf.region.domain.{fname}\"}}"));
        // One scope per CLASS, in deterministic class order.
        let class_of = |r: u32| -> ScopeClass {
            match foreign.get(&r) {
                Some(&role) => ScopeClass::Foreign(role),
                None => ScopeClass::Local(r),
            }
        };
        let mut class_scope: BTreeMap<ScopeClass, usize> = BTreeMap::new();
        for &r in &regions {
            let c = class_of(r);
            if class_scope.contains_key(&c) {
                continue;
            }
            // LangRef: a scope's first operand is self-referential or a
            // string — the deterministic name IS the first operand.
            let name = match c {
                ScopeClass::Local(r) => format!("{fname}.r{r}"),
                ScopeClass::Foreign(role) => format!("{fname}.foreign.{}", role.name()),
            };
            let id = self
                .cx
                .meta_node(format!("distinct !{{!\"{name}\", !{domain}}}"));
            class_scope.insert(c, id);
        }
        for &r in &regions {
            self.region_scope.insert(r, class_scope[&class_of(r)]);
        }
        let all: Vec<(ScopeClass, usize)> = class_scope.iter().map(|(&c, &s)| (c, s)).collect();
        for &(c, s) in &all {
            let own = self.cx.meta_node(format!("!{{!{s}}}"));
            let others: Vec<String> = all
                .iter()
                .filter(|&&(o, _)| o != c)
                .map(|&(_, os)| format!("!{os}"))
                .collect();
            let noalias = if others.is_empty() {
                None
            } else {
                Some(self.cx.meta_node(format!("!{{{}}}", others.join(", "))))
            };
            for &r in &regions {
                if class_of(r) == c {
                    self.scope_lists.insert(r, (own, noalias));
                }
            }
        }
    }

    // ---- small helpers ---------------------------------------------------

    fn tmp(&mut self) -> String {
        self.tmp += 1;
        format!("%t{}", self.tmp)
    }

    fn vname(v: WValue) -> String {
        format!("%v{}", v.index())
    }

    fn line(&mut self, s: String) {
        self.blocks[self.cur].lines.push(s);
    }

    fn new_block(&mut self, label: String) -> usize {
        self.blocks.push(LBlock {
            label,
            lines: Vec::new(),
        });
        self.blocks.len() - 1
    }

    fn cur_label(&self) -> String {
        self.blocks[self.cur].label.clone()
    }

    fn layout_of(&self, ty: TypeId) -> Result<Layout, BackendError> {
        layout::layout_of(&self.m.types, ty).ok_or_else(|| ice("layout of a token type"))
    }

    /// A fresh entry-block byte slot; returns its `%slot.N` name.
    fn new_slot(&mut self, l: Layout) -> String {
        self.nslot += 1;
        let name = format!("%slot{}", self.nslot);
        self.allocas.push(format!(
            "  {name} = alloca [{} x i8], align {}",
            l.size.max(1),
            l.align.max(1)
        ));
        name
    }

    fn op(&self, v: WValue) -> Result<String, BackendError> {
        match self.vals.get(&v) {
            Some(Repr::Scalar(s)) => Ok(s.clone()),
            other => Err(ice(format!(
                "expected scalar repr for %{v:?}, found {other:?}"
            ))),
        }
    }

    fn addr(&self, v: WValue) -> Result<String, BackendError> {
        match self.vals.get(&v) {
            Some(Repr::Addr(s)) => Ok(s.clone()),
            other => Err(ice(format!(
                "expected aggregate repr for %{v:?}, found {other:?}"
            ))),
        }
    }

    fn args_of(&self, inst: wolf_wir::ir::Inst) -> Vec<WValue> {
        self.f.vpool.get(self.f.insts[inst].args)
    }

    fn results_of(&self, inst: wolf_wir::ir::Inst) -> Vec<WValue> {
        self.f.vpool.get(self.f.insts[inst].results)
    }

    fn sty(&self, v: WValue) -> Result<&'static str, BackendError> {
        scalar_ty(self.m, self.f.value_ty(v)).ok_or_else(|| ice("scalar type of non-scalar"))
    }

    /// The access metadata suffix for a memory op on region `tok`'s
    /// memory: `!alias.scope` (its own region) + `!noalias` (every
    /// provably-disjoint region this function touches).
    fn scope_suffix(&self, tok: WValue) -> String {
        if self.cx.opts.strip_facts {
            return String::new();
        }
        let TypeData::Mem(r) = self.m.types.get(self.f.value_ty(tok)) else {
            return String::new(); // io-token ops carry no region scopes
        };
        let Some(&(own, noalias)) = self.scope_lists.get(&(r.index() as u32)) else {
            return String::new();
        };
        let mut s = format!(", !alias.scope !{own}");
        if let Some(na) = noalias {
            let _ = write!(s, ", !noalias !{na}");
        }
        s
    }

    /// The `!noalias` suffix for a CALL: every region this function
    /// touches whose memory the callee provably cannot reach (s83 —
    /// the fact s78 declined, in the half that has a theorem).
    ///
    /// The claim is "no token ⇒ no effect", and it is a theorem exactly
    /// for the regions `wolf_wir::midend::exhaustive_regions` admits:
    /// rooted here (or lent by the caller), with no pointer escaped to
    /// a tokenless seam and no handle handed out. For those, a call
    /// that does not take the token has no way to name the storage —
    /// tokens are linear, and the only other door is a raw pointer.
    ///
    /// What is deliberately NOT listed, and why the list is computed
    /// from the mid-end's own predicate rather than from the signature:
    ///
    /// - **`region.foreign` roots.** A foreign root is minted per
    ///   function over storage the RUNTIME owns, so a callee reaches a
    ///   caller's container buffers by minting its own root, holding
    ///   none of the caller's tokens (s80, wolf-lang#83). "No token ⇒
    ///   no effect" is simply false there. Claiming it would be the
    ///   s80 miscompile handed to LLVM on purpose. The theorem needs
    ///   the tokens PROPAGATED THROUGH SIGNATURES — see wolf-lang#91
    ///   and the s83 ABI inventory for what that costs.
    /// - **Regions whose pointer escaped.** Runtime read/write shims
    ///   take raw pointers with no token operand.
    /// - **Regions whose token this call DOES take.** The callee is
    ///   licensed to write them; that is what the token means.
    ///
    /// The predicate is imported, not re-derived, so the emitter and
    /// `memopt`/`licm` cannot drift apart — an unenforced asymmetry in
    /// this file is how s80's miscompile happened.
    fn call_noalias_suffix(&mut self, args: &[WValue]) -> String {
        if self.cx.opts.strip_facts {
            return String::new();
        }
        let taken: HashSet<u32> = args
            .iter()
            .filter_map(|&a| match self.m.types.get(self.f.value_ty(a)) {
                TypeData::Mem(r) => Some(r.index() as u32),
                _ => None,
            })
            .collect();
        // Scope ids of the regions the callee cannot reach, minus any
        // scope shared with a region it can (a class is only ever as
        // strong as its weakest member).
        let mut claim: BTreeSet<usize> = BTreeSet::new();
        let mut deny: BTreeSet<usize> = BTreeSet::new();
        for (&r, &scope) in &self.region_scope {
            if self.exhaustive.contains(&r) && !taken.contains(&r) {
                claim.insert(scope);
            } else {
                deny.insert(scope);
            }
        }
        let ids: Vec<usize> = claim.difference(&deny).copied().collect();
        if ids.is_empty() {
            return String::new();
        }
        let node = match self.call_noalias.get(&ids) {
            Some(&id) => id,
            None => {
                let body = format!(
                    "!{{{}}}",
                    ids.iter()
                        .map(|s| format!("!{s}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let id = self.cx.meta_node(body);
                self.call_noalias.insert(ids, id);
                id
            }
        };
        format!(", !noalias !{node}")
    }

    /// Is this load's memory frozen for the token's whole extent?
    /// v0 detection: the token was minted by `sync.freeze`, or the
    /// pointer carries a `frozen` fact.
    fn is_invariant_load(&self, ptr: WValue, tok: WValue) -> bool {
        if self.cx.opts.strip_facts {
            return false;
        }
        if self.frozen.contains(&ptr) {
            return true;
        }
        match self.f.values[tok].def {
            ValueDef::Result(inst, _) => self.f.insts[inst].op == Opcode::SyncFreeze,
            ValueDef::Param(..) => false,
        }
    }

    /// Truncate an i128 to the two's-complement value of an N-bit int,
    /// rendered as LLVM's signed decimal.
    fn int_render(bits: u32, v: i128) -> i128 {
        if bits >= 128 {
            return v;
        }
        let m = 1i128 << bits;
        let t = v.rem_euclid(m);
        if t >= m / 2 { t - m } else { t }
    }

    /// `!range` suffix for an integer load whose result has a range
    /// fact (half-open per LangRef; the full range is no claim).
    fn range_suffix(&mut self, res: WValue, ty: &'static str) -> String {
        if self.cx.opts.strip_facts || ty == "ptr" || ty == "float" || ty == "double" {
            return String::new();
        }
        let Some(&(lo, hi)) = self.range.get(&res) else {
            return String::new();
        };
        let Some(bits) = self.m.types.int_bits(self.f.value_ty(res)) else {
            return String::new();
        };
        let Some((tmin, tmax)) = self.m.types.int_bounds(self.f.value_ty(res)) else {
            return String::new();
        };
        let (lo, hi) = (lo.max(tmin), hi.min(tmax));
        if lo <= tmin && hi >= tmax {
            return String::new();
        }
        let l = Self::int_render(bits, lo);
        let h = Self::int_render(bits, hi + 1);
        if l == h {
            return String::new();
        }
        let id = self.cx.meta_node(format!("!{{{ty} {l}, {ty} {h}}}"));
        format!(", !range !{id}")
    }

    fn align_of(&self, natural: u32) -> u32 {
        if self.cx.opts.strip_facts { 1 } else { natural }
    }

    // ---- runtime + external declarations ----------------------------------

    /// Ensure a `declare` exists for `symbol` (skipped when defined in
    /// this module).
    fn ensure_decl(&mut self, symbol: &str, line: String) {
        if self.defined.contains(symbol) {
            return;
        }
        self.cx.decls.entry(symbol.to_string()).or_insert(line);
    }

    /// The built-in runtime shims this backend itself emits calls to
    /// (everything else arrives as ordinary WIR calls with real sigs).
    fn rt_builtin(&mut self, name: &str) -> Result<(), BackendError> {
        let strip = self.cx.opts.strip_facts;
        let line = match name {
            "__wolf_rt_trap" => {
                "declare void @__wolf_rt_trap(i32) cold noreturn nounwind".to_string()
            }
            "__wolf_rt_region_new" => "declare ptr @__wolf_rt_region_new() nounwind".to_string(),
            "__wolf_rt_region_alloc" => {
                // The bump allocator's contract: fresh, 16-aligned
                // (report 10 delta 2: allocation sites fix alignment).
                if strip {
                    "declare ptr @__wolf_rt_region_alloc(ptr, i64) nounwind".to_string()
                } else {
                    "declare noalias noundef align 16 ptr @__wolf_rt_region_alloc(ptr, i64) nounwind"
                        .to_string()
                }
            }
            "__wolf_rt_region_free" => {
                "declare void @__wolf_rt_region_free(ptr) nounwind".to_string()
            }
            "__wolf_rt_region_freeze" => {
                "declare void @__wolf_rt_region_freeze(ptr) nounwind".to_string()
            }
            other => return Err(ice(format!("unknown builtin runtime symbol {other}"))),
        };
        self.ensure_decl(name, line);
        Ok(())
    }

    fn intrinsic(&mut self, decl: &str) {
        self.cx.intrinsics.insert(decl.to_string());
    }

    // ---- trap machinery ----------------------------------------------------

    fn trap_block(&mut self, code: i32) -> Result<String, BackendError> {
        if let Some(l) = self.trap_blocks.get(&code) {
            return Ok(l.clone());
        }
        self.rt_builtin("__wolf_rt_trap")?;
        let label = format!("trap{code}");
        self.trap_blocks.insert(code, label.clone());
        Ok(label)
    }

    /// Branch to the kind's cold trap block when `cond` (an i1) is
    /// true; continue in a fresh block. The trap arc gets cold weights
    /// (the uniform rule).
    fn trap_if(&mut self, cond: &str, kind: TrapKind) -> Result<(), BackendError> {
        let tb = self.trap_block(trap_code(kind))?;
        self.tmp += 1;
        let cont = format!("cont{}", self.tmp);
        let prof = if self.cx.opts.strip_facts {
            String::new()
        } else {
            format!(", !prof !{}", self.cx.prof_cold_first())
        };
        self.line(format!("  br i1 {cond}, label %{tb}, label %{cont}{prof}"));
        self.cur = self.new_block(cont);
        Ok(())
    }

    // ---- byte copies -------------------------------------------------------

    /// Load `size` bytes from `src` into chunk temps (phase one of a
    /// clobber-safe copy). `meta` rides on the loads (region scopes
    /// when the source is region memory).
    fn load_chunks(
        &mut self,
        src: &str,
        size: u32,
        meta: &str,
    ) -> Result<Vec<Chunk>, BackendError> {
        let mut out = Vec::new();
        let mut off = 0u32;
        for (chunk, ty) in [(8u32, "i64"), (4, "i32"), (2, "i16"), (1, "i8")] {
            while size - off >= chunk {
                let p = self.gep(src, off as i64);
                let t = self.tmp();
                self.line(format!("  {t} = load {ty}, ptr {p}, align 1{meta}"));
                out.push((off, ty, t));
                off += chunk;
            }
        }
        Ok(out)
    }

    fn store_chunks(
        &mut self,
        dst: &str,
        chunks: &[Chunk],
        meta: &str,
    ) -> Result<(), BackendError> {
        for (off, ty, v) in chunks {
            let p = self.gep(dst, *off as i64);
            self.line(format!("  store {ty} {v}, ptr {p}, align 1{meta}"));
        }
        Ok(())
    }

    fn copy_bytes(
        &mut self,
        dst: &str,
        src: &str,
        size: u32,
        load_meta: &str,
        store_meta: &str,
    ) -> Result<(), BackendError> {
        let chunks = self.load_chunks(src, size, load_meta)?;
        self.store_chunks(dst, &chunks, store_meta)
    }

    /// `base + off` as a fresh ptr temp (off 0 returns base).
    fn gep(&mut self, base: &str, off: i64) -> String {
        if off == 0 {
            return base.to_string();
        }
        let t = self.tmp();
        self.line(format!("  {t} = getelementptr i8, ptr {base}, i64 {off}"));
        t
    }

    /// A fresh slot of whole eightbytes for `units`, address returned.
    fn units_slot(&mut self, units: &[Unit]) -> String {
        self.new_slot(Layout {
            size: (units.len() as u32).max(1) * 8,
            align: 8,
        })
    }

    /// Store one register unit at `base + u.offset` (full-eightbyte
    /// stores; callers guarantee a rounded slot).
    fn store_unit(&mut self, base: &str, u: &Unit, v: &str) -> Result<(), BackendError> {
        let p = self.gep(base, u.offset as i64);
        let ty = unit_ty(u);
        self.line(format!("  store {ty} {v}, ptr {p}, align 1"));
        Ok(())
    }

    /// Load one register unit from `base + u.offset`; partial INTEGER
    /// units assemble from exact chunks (no byte beyond the value's
    /// real extent is read — `base` may be an interior view).
    fn load_unit(&mut self, base: &str, u: &Unit) -> Result<String, BackendError> {
        match u.class {
            RegClass::Sse => {
                let ty = unit_ty(u);
                let p = self.gep(base, u.offset as i64);
                let t = self.tmp();
                self.line(format!("  {t} = load {ty}, ptr {p}, align 1"));
                Ok(t)
            }
            RegClass::Int => {
                if u.bytes >= 8 {
                    let p = self.gep(base, u.offset as i64);
                    let t = self.tmp();
                    self.line(format!("  {t} = load i64, ptr {p}, align 1"));
                    return Ok(t);
                }
                let mut acc: Option<String> = None;
                let mut off = 0u32;
                let bytes = u.bytes as u32;
                for (chunk, ty) in [(4u32, "i32"), (2, "i16"), (1, "i8")] {
                    while bytes - off >= chunk {
                        let p = self.gep(base, (u.offset + off) as i64);
                        let ld = self.tmp();
                        self.line(format!("  {ld} = load {ty}, ptr {p}, align 1"));
                        let wide = self.tmp();
                        self.line(format!("  {wide} = zext {ty} {ld} to i64"));
                        let shifted = if off == 0 {
                            wide
                        } else {
                            let sh = self.tmp();
                            self.line(format!("  {sh} = shl i64 {wide}, {}", off * 8));
                            sh
                        };
                        acc = Some(match acc {
                            None => shifted,
                            Some(a) => {
                                let o = self.tmp();
                                self.line(format!("  {o} = or i64 {a}, {shifted}"));
                                o
                            }
                        });
                        off += chunk;
                    }
                }
                Ok(acc.unwrap_or_else(|| "0".to_string()))
            }
        }
    }

    // ---- edges -------------------------------------------------------------

    /// Prepare one branch edge from the CURRENT block: aggregate args
    /// copy into the target's param slots (loads first, then stores —
    /// swapping edges can't clobber); scalar args are recorded as phi
    /// incoming under `pred_label`. Returns the target label.
    fn edge(
        &mut self,
        bc: wolf_wir::ir::BlockCall,
        pred_label: &str,
    ) -> Result<String, BackendError> {
        let target = bc.block;
        if Some(target) == self.f.entry() {
            return Err(ice("branch to the entry block (unrepresentable phi)"));
        }
        let args = self.f.vpool.get(bc.args);
        let params = self.f.block_params(target);
        // Phase 1: load every aggregate arg's bytes.
        let mut copies: Vec<(String, Vec<Chunk>)> = Vec::new();
        let mut scalars: Vec<(usize, String)> = Vec::new();
        for (i, (&av, &pv)) in args.iter().zip(params.iter()).enumerate() {
            let ty = self.f.value_ty(pv);
            if self.m.types.is_token(ty) {
                continue;
            }
            if is_agg(self.m, ty) {
                let slot = self
                    .param_slots
                    .get(&(target, i))
                    .cloned()
                    .ok_or_else(|| ice("aggregate block param has no slot"))?;
                let src = self.addr(av)?;
                let size = self.layout_of(ty)?.size;
                let chunks = self.load_chunks(&src, size, "")?;
                copies.push((slot, chunks));
            } else {
                scalars.push((i, self.op(av)?));
            }
        }
        // Phase 2: store into the target slots.
        for (slot, chunks) in copies {
            self.store_chunks(&slot, &chunks, "")?;
        }
        // Record phi incoming (scalar params only, in param order).
        let accs = self
            .phis
            .get_mut(&target)
            .ok_or_else(|| ice("no phi accs"))?;
        let mut k = 0usize;
        for (i, wv) in params.iter().enumerate() {
            let ty = self.f.value_ty(*wv);
            if self.m.types.is_token(ty) || is_agg(self.m, ty) {
                continue;
            }
            let (ai, op) = scalars
                .iter()
                .find(|(ai, _)| *ai == i)
                .cloned()
                .ok_or_else(|| ice("edge arity mismatch"))?;
            debug_assert_eq!(ai, i);
            accs[k].incoming.push((pred_label.to_string(), op));
            k += 1;
        }
        Ok(self.wlabel[&target].clone())
    }

    // ---- the driver --------------------------------------------------------

    fn run(&mut self, si: &LSig) -> Result<(), BackendError> {
        let entry = self.f.entry().ok_or_else(|| ice("no entry"))?;
        // The WALK order (s74, #72): reverse postorder, not `layout`.
        // `layout` is a printing order and lowering may append a block
        // that uses a value before the block defining it (a `select` arm
        // inside a loop). This emitter is one forward pass over the
        // text, so defs must come first — RPO is that guarantee.
        let order = wolf_wir::block_order(self.f);
        // Labels for every reachable WIR block.
        for &wb in &order {
            if self.reachable.contains(&wb) {
                self.wlabel.insert(wb, format!("b{}", wb.index()));
            }
        }
        // The entry block opens first.
        let entry_idx = self.new_block(self.wlabel[&entry].clone());
        self.cur = entry_idx;

        // Entry params per plan.
        let wparams = self.f.block_params(entry);
        if wparams.len() != si.params.len() {
            return Err(ice("entry params do not match the signature"));
        }
        let n_sret = si
            .rets
            .iter()
            .filter(|r| matches!(r, RSlot::Sret { .. }))
            .count();
        let mut next = if si.sret_first { n_sret } else { 0 };
        for (i, &wv) in wparams.iter().enumerate() {
            match &si.params[i] {
                Slot::Token => {
                    self.vals.insert(wv, Repr::Token);
                }
                Slot::Direct(_) => {
                    self.vals.insert(wv, Repr::Scalar(format!("%p{next}")));
                    next += 1;
                }
                Slot::Split(units) => {
                    let units = units.clone();
                    let slot = self.units_slot(&units);
                    for u in &units {
                        let v = format!("%p{next}");
                        next += 1;
                        self.store_unit(&slot, u, &v)?;
                    }
                    self.vals.insert(wv, Repr::Addr(slot));
                }
                Slot::Ref | Slot::CStruct(..) => {
                    self.vals.insert(wv, Repr::Addr(format!("%p{next}")));
                    next += 1;
                }
            }
        }
        let sret_base = if si.sret_first { 0 } else { next };
        for k in 0..n_sret {
            self.sret.push(format!("%p{}", sret_base + k));
        }

        // Non-entry block params: scalar → named phi; aggregate → slot.
        for &wb in &order {
            if wb == entry || !self.reachable.contains(&wb) {
                continue;
            }
            let mut accs = Vec::new();
            for (i, wv) in self.f.block_params(wb).into_iter().enumerate() {
                let ty = self.f.value_ty(wv);
                if self.m.types.is_token(ty) {
                    self.vals.insert(wv, Repr::Token);
                } else if is_agg(self.m, ty) {
                    let l = self.layout_of(ty)?;
                    let slot = self.new_slot(l);
                    self.param_slots.insert((wb, i), slot.clone());
                    self.vals.insert(wv, Repr::Addr(slot));
                } else {
                    let name = Self::vname(wv);
                    let sty = self.sty(wv)?;
                    self.vals.insert(wv, Repr::Scalar(name.clone()));
                    accs.push(PhiAcc {
                        name,
                        ty: sty,
                        incoming: Vec::new(),
                    });
                }
            }
            self.phis.insert(wb, accs);
        }

        // Translate every reachable block in walk order.
        for &wb in &order {
            if !self.reachable.contains(&wb) {
                continue;
            }
            if wb != entry {
                let idx = self.new_block(self.wlabel[&wb].clone());
                self.cur = idx;
            }
            for ii in 0..self.f.blocks[wb].insts.len() {
                let inst = self.f.blocks[wb].insts[ii];
                self.inst(inst)?;
            }
        }

        // Cold out-of-line trap bodies (the uniform rule).
        for (code, label) in std::mem::take(&mut self.trap_blocks) {
            let idx = self.new_block(label);
            self.cur = idx;
            self.line(format!("  call void @__wolf_rt_trap(i32 {code})"));
            self.line("  unreachable".to_string());
        }
        Ok(())
    }

    // ---- instruction dispatch ----------------------------------------------

    fn inst(&mut self, inst: wolf_wir::ir::Inst) -> Result<(), BackendError> {
        let data = self.f.insts[inst];
        let op = data.op;
        let args = self.args_of(inst);
        let results = self.results_of(inst);
        match op {
            // s86: the compiled task-entry pointer. A module function's
            // address is a link-time constant exactly like `data.addr`'s
            // — the symbol itself is the `ptr` operand, so there is
            // nothing to materialize and GVN may dedup it freely. The
            // callee must live in THIS object's subset (the task shim
            // always does: lowering synthesizes it into the same
            // module); anything else is an honest refusal, the same
            // line the debug tier draws.
            Opcode::FuncAddr => {
                let Aux::Callee(ef) = data.aux else {
                    return Err(ice("func.addr without callee"));
                };
                let callee = self.f.ext_funcs[ef].name.clone();
                let Some(entry) = self.funcs.get(&callee) else {
                    return Err(nyi(format!(
                        "func.addr of `@{callee}` outside this object's subset"
                    )));
                };
                let symbol = entry.symbol.clone();
                self.vals
                    .insert(results[0], Repr::Scalar(format!("@\"{symbol}\"")));
            }
            Opcode::Iconst => {
                let Aux::Int(n) = data.aux else {
                    return Err(ice("iconst without payload"));
                };
                self.vals.insert(results[0], Repr::Scalar(n.to_string()));
            }
            Opcode::Fconst => {
                let Aux::FloatBits(bits) = data.aux else {
                    return Err(ice("fconst without payload"));
                };
                let rty = self.f.value_ty(results[0]);
                // LLVM text renders both float widths as the 16-hex-digit
                // double whose value converts exactly.
                let dbits = if matches!(self.m.types.get(rty), TypeData::F32) {
                    (f32::from_bits(bits as u32) as f64).to_bits()
                } else {
                    bits
                };
                self.vals
                    .insert(results[0], Repr::Scalar(format!("0x{dbits:016X}")));
            }
            Opcode::Bconst => {
                let Aux::Bool(v) = data.aux else {
                    return Err(ice("bconst without payload"));
                };
                self.vals
                    .insert(results[0], Repr::Scalar(i64::from(v).to_string()));
            }

            // ---- checked / wrapping / saturating integer arithmetic ----
            Opcode::IaddChk
            | Opcode::IsubChk
            | Opcode::ImulChk
            | Opcode::UaddChk
            | Opcode::UsubChk
            | Opcode::UmulChk => {
                let r = self.checked_arith(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::IdivChk | Opcode::IremChk | Opcode::UdivChk | Opcode::UremChk => {
                let r = self.checked_div(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::IaddWrap | Opcode::IsubWrap | Opcode::ImulWrap => {
                let ty = self.sty(results[0])?;
                let (a, b) = (self.op(args[0])?, self.op(args[1])?);
                let mn = match op {
                    Opcode::IaddWrap => "add",
                    Opcode::IsubWrap => "sub",
                    _ => "mul",
                };
                let t = self.tmp();
                self.line(format!("  {t} = {mn} {ty} {a}, {b}"));
                self.vals.insert(results[0], Repr::Scalar(t));
            }
            Opcode::IaddSat | Opcode::IsubSat | Opcode::ImulSat => {
                let r = self.sat_arith(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- bit ops (shift amounts mask to width, matching WIR) ----
            Opcode::Band | Opcode::Bor | Opcode::Bxor => {
                let ty = self.sty(results[0])?;
                let (a, b) = (self.op(args[0])?, self.op(args[1])?);
                let mn = match op {
                    Opcode::Band => "and",
                    Opcode::Bor => "or",
                    _ => "xor",
                };
                let t = self.tmp();
                self.line(format!("  {t} = {mn} {ty} {a}, {b}"));
                self.vals.insert(results[0], Repr::Scalar(t));
            }
            Opcode::Shl | Opcode::Lshr | Opcode::Ashr => {
                let ty = self.sty(results[0])?;
                let bits = self
                    .m
                    .types
                    .int_bits(self.f.value_ty(results[0]))
                    .ok_or_else(|| ice("shift on non-int"))?;
                let (a, b) = (self.op(args[0])?, self.op(args[1])?);
                // WIR masks the amount; LLVM's shift by >= width is
                // poison — mask explicitly.
                let masked = self.tmp();
                self.line(format!("  {masked} = and {ty} {b}, {}", bits - 1));
                let mn = match op {
                    Opcode::Shl => "shl",
                    Opcode::Lshr => "lshr",
                    _ => "ashr",
                };
                let t = self.tmp();
                self.line(format!("  {t} = {mn} {ty} {a}, {masked}"));
                self.vals.insert(results[0], Repr::Scalar(t));
            }

            // ---- floats (IEEE, no traps) ----
            Opcode::Fadd | Opcode::Fsub | Opcode::Fmul | Opcode::Fdiv => {
                let ty = self.sty(results[0])?;
                let (a, b) = (self.op(args[0])?, self.op(args[1])?);
                let mn = match op {
                    Opcode::Fadd => "fadd",
                    Opcode::Fsub => "fsub",
                    Opcode::Fmul => "fmul",
                    _ => "fdiv",
                };
                let t = self.tmp();
                self.line(format!("  {t} = {mn} {ty} {a}, {b}"));
                self.vals.insert(results[0], Repr::Scalar(t));
            }
            Opcode::Fneg => {
                let ty = self.sty(results[0])?;
                let a = self.op(args[0])?;
                let t = self.tmp();
                self.line(format!("  {t} = fneg {ty} {a}"));
                self.vals.insert(results[0], Repr::Scalar(t));
            }
            Opcode::Fma => {
                let ty = self.sty(results[0])?;
                let (a, b, c) = (self.op(args[0])?, self.op(args[1])?, self.op(args[2])?);
                let suffix = if ty == "float" { "f32" } else { "f64" };
                self.intrinsic(&format!(
                    "declare {ty} @llvm.fma.{suffix}({ty}, {ty}, {ty})"
                ));
                let t = self.tmp();
                self.line(format!(
                    "  {t} = call {ty} @llvm.fma.{suffix}({ty} {a}, {ty} {b}, {ty} {c})"
                ));
                self.vals.insert(results[0], Repr::Scalar(t));
            }

            // ---- compares ----
            Opcode::Icmp => {
                let Aux::IntCc(cc) = data.aux else {
                    return Err(ice("icmp without condition"));
                };
                let ty = self.sty(args[0])?;
                let (a, b) = (self.op(args[0])?, self.op(args[1])?);
                let t = self.tmp();
                self.line(format!("  {t} = icmp {} {ty} {a}, {b}", int_cc(cc)));
                let r = self.tmp();
                self.line(format!("  {r} = zext i1 {t} to i8"));
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Fcmp => {
                let Aux::FloatCc(cc) = data.aux else {
                    return Err(ice("fcmp without condition"));
                };
                let ty = self.sty(args[0])?;
                let (a, b) = (self.op(args[0])?, self.op(args[1])?);
                let t = self.tmp();
                self.line(format!("  {t} = fcmp {} {ty} {a}, {b}", float_cc(cc)));
                let r = self.tmp();
                self.line(format!("  {r} = zext i1 {t} to i8"));
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- conversions ----
            Opcode::Sext | Opcode::Zext | Opcode::Itrunc => {
                let from = self.sty(args[0])?;
                let to = self.sty(results[0])?;
                let a = self.op(args[0])?;
                let mn = match op {
                    Opcode::Sext => "sext",
                    Opcode::Zext => "zext",
                    _ => "trunc",
                };
                let t = self.tmp();
                self.line(format!("  {t} = {mn} {from} {a} to {to}"));
                self.vals.insert(results[0], Repr::Scalar(t));
            }

            // ---- memory ----
            Opcode::PtrOff => {
                let Aux::Scale(scale) = data.aux else {
                    return Err(ice("ptr.off without scale"));
                };
                let p = self.op(args[0])?;
                let i = self.op(args[1])?;
                let scaled = self.tmp();
                self.line(format!("  {scaled} = mul i64 {i}, {scale}"));
                let t = self.tmp();
                self.line(format!("  {t} = getelementptr i8, ptr {p}, i64 {scaled}"));
                self.vals.insert(results[0], Repr::Scalar(t));
            }
            Opcode::Load => {
                let p = self.op(args[0])?;
                let rty = self.f.value_ty(results[0]);
                let scopes = self.scope_suffix(args[1]);
                if is_agg(self.m, rty) {
                    let l = self.layout_of(rty)?;
                    let slot = self.new_slot(l);
                    self.copy_bytes(&slot, &p, l.size, &scopes, "")?;
                    self.vals.insert(results[0], Repr::Addr(slot));
                } else {
                    let ty = scalar_ty(self.m, rty).ok_or_else(|| ice("load of token type"))?;
                    let mut meta = scopes;
                    if self.is_invariant_load(args[0], args[1]) {
                        let id = self.cx.meta_node("!{}".to_string());
                        let _ = write!(meta, ", !invariant.load !{id}");
                    }
                    let range = self.range_suffix(results[0], ty);
                    let t = self.tmp();
                    self.line(format!(
                        "  {t} = load {ty}, ptr {p}, align {}{range}{meta}",
                        self.align_of(natural_align(ty))
                    ));
                    self.vals.insert(results[0], Repr::Scalar(t));
                }
            }
            Opcode::Store => {
                let vty = self.f.value_ty(args[0]);
                let p = self.op(args[1])?;
                let scopes = self.scope_suffix(args[2]);
                if is_agg(self.m, vty) {
                    let src = self.addr(args[0])?;
                    let size = self.layout_of(vty)?.size;
                    self.copy_bytes(&p, &src, size, "", &scopes)?;
                } else {
                    let ty = scalar_ty(self.m, vty).ok_or_else(|| ice("store of token type"))?;
                    let v = self.op(args[0])?;
                    self.line(format!(
                        "  store {ty} {v}, ptr {p}, align {}{scopes}",
                        self.align_of(natural_align(ty))
                    ));
                }
                self.vals.insert(results[0], Repr::Token);
            }

            // ---- aggregates ----
            Opcode::AggMake => {
                let rty = self.f.value_ty(results[0]);
                let TypeData::Agg(fields) = self.m.types.get(rty).clone() else {
                    return Err(ice("agg.make of non-aggregate"));
                };
                let l = self.layout_of(rty)?;
                let offs = layout::field_offsets(&self.m.types, &fields)
                    .ok_or_else(|| ice("aggregate with token field"))?;
                let slot = self.new_slot(l);
                for ((&av, &fty), &off) in args.iter().zip(fields.iter()).zip(offs.iter()) {
                    self.store_field(&slot, off, fty, av)?;
                }
                self.vals.insert(results[0], Repr::Addr(slot));
            }
            Opcode::AggGet => {
                let Aux::Int(k) = data.aux else {
                    return Err(ice("agg.get without index"));
                };
                let aty = self.f.value_ty(args[0]);
                let TypeData::Agg(fields) = self.m.types.get(aty).clone() else {
                    return Err(ice("agg.get of non-aggregate"));
                };
                let offs = layout::field_offsets(&self.m.types, &fields)
                    .ok_or_else(|| ice("aggregate with token field"))?;
                let base = self.addr(args[0])?;
                let repr = self.load_field(&base, offs[k as usize], fields[k as usize])?;
                self.vals.insert(results[0], repr);
            }

            // ---- error unions ----
            Opcode::EuMakeOk => {
                let rty = self.f.value_ty(results[0]);
                let l = self.layout_of(rty)?;
                let slot = self.new_slot(l);
                self.line(format!("  store i64 0, ptr {slot}, align 1"));
                if let Some(&ok) = args.first() {
                    let off = layout::eu_ok_offset(&self.m.types, rty)
                        .ok_or_else(|| ice("eu.make.ok operand for a unit ok"))?;
                    self.store_field(&slot, off, self.f.value_ty(ok), ok)?;
                }
                self.vals.insert(results[0], Repr::Addr(slot));
            }
            Opcode::EuMakeErr => {
                let rty = self.f.value_ty(results[0]);
                let l = self.layout_of(rty)?;
                let slot = self.new_slot(l);
                let tag = self.op(args[0])?;
                self.line(format!("  store i64 {tag}, ptr {slot}, align 1"));
                let slot_offs = layout::eu_slot_offset(&self.m.types, rty);
                let TypeData::Eu { slots, .. } = self.m.types.get(rty).clone() else {
                    return Err(ice("eu.make.err of non-eu"));
                };
                for (i, &pv) in args.iter().skip(1).enumerate() {
                    self.store_field(&slot, slot_offs[i], slots[i], pv)?;
                }
                self.vals.insert(results[0], Repr::Addr(slot));
            }
            Opcode::EuIsErr => {
                let base = self.addr(args[0])?;
                let tag = self.tmp();
                self.line(format!("  {tag} = load i64, ptr {base}, align 1"));
                let c = self.tmp();
                self.line(format!("  {c} = icmp ne i64 {tag}, 0"));
                let r = self.tmp();
                self.line(format!("  {r} = zext i1 {c} to i8"));
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::EuOk => {
                let ety = self.f.value_ty(args[0]);
                let base = self.addr(args[0])?;
                let off = layout::eu_ok_offset(&self.m.types, ety)
                    .ok_or_else(|| ice("eu.ok of a unit ok"))?;
                let TypeData::Eu { ok: Some(okt), .. } = self.m.types.get(ety).clone() else {
                    return Err(ice("eu.ok of non-eu"));
                };
                let repr = self.load_field(&base, off, okt)?;
                self.vals.insert(results[0], repr);
            }
            Opcode::EuErr => {
                let ety = self.f.value_ty(args[0]);
                let base = self.addr(args[0])?;
                match data.aux {
                    Aux::Int(k) => {
                        let offs = layout::eu_slot_offset(&self.m.types, ety);
                        let TypeData::Eu { slots, .. } = self.m.types.get(ety).clone() else {
                            return Err(ice("eu.err of non-eu"));
                        };
                        let repr = self.load_field(&base, offs[k as usize], slots[k as usize])?;
                        self.vals.insert(results[0], repr);
                    }
                    _ => {
                        let t = self.tmp();
                        self.line(format!("  {t} = load i64, ptr {base}, align 1"));
                        self.vals.insert(results[0], Repr::Scalar(t));
                    }
                }
            }

            // ---- module data ----
            Opcode::DataAddr => {
                let Aux::Data(idx) = data.aux else {
                    return Err(ice("data.addr without payload"));
                };
                let d = self
                    .m
                    .data
                    .get(idx as usize)
                    .ok_or_else(|| ice("data.addr names missing data"))?;
                let sym = format!("_W.{}", d.name);
                if self.cx.global_names.insert(sym.clone()) {
                    let line = format!(
                        "@\"{sym}\" = private unnamed_addr constant [{} x i8] {}, align 1",
                        d.bytes.len().max(1),
                        bytes_const(&d.bytes),
                    );
                    self.cx.globals.push(line);
                }
                self.vals
                    .insert(results[0], Repr::Scalar(format!("@\"{sym}\"")));
            }

            // ---- calls ----
            Opcode::Call => {
                let Aux::Callee(ef) = data.aux else {
                    return Err(ice("call without callee"));
                };
                let callee = self.f.ext_funcs[ef].clone();
                self.lower_call(&callee.name, callee.sig, &args, &results)?;
            }
            Opcode::CallInd => {
                let Aux::Sig(sig) = data.aux else {
                    return Err(ice("call.ind without a signature"));
                };
                self.lower_call_ind(sig, &args, &results)?;
            }

            // ---- the memory story ----
            Opcode::RegionNew => {
                self.rt_builtin("__wolf_rt_region_new")?;
                let t = self.tmp();
                self.line(format!("  {t} = call ptr @__wolf_rt_region_new()"));
                self.vals.insert(results[0], Repr::Scalar(t));
                self.vals.insert(results[1], Repr::Token);
            }
            Opcode::RegionAlloc => {
                self.rt_builtin("__wolf_rt_region_alloc")?;
                let h = self.op(args[0])?;
                let size = self.op(args[1])?;
                // The result's deref fact rides the call site as a
                // return attribute; the decl carries `align 16`
                // (report 10 delta 2).
                let attrs = if self.cx.opts.strip_facts {
                    String::new()
                } else {
                    match self.deref.get(&results[0]) {
                        Some(n) => format!("noalias align 16 dereferenceable({n}) "),
                        None => "noalias align 16 ".to_string(),
                    }
                };
                let t = self.tmp();
                self.line(format!(
                    "  {t} = call {attrs}ptr @__wolf_rt_region_alloc(ptr {h}, i64 {size})"
                ));
                self.vals.insert(results[0], Repr::Scalar(t));
                self.vals.insert(results[1], Repr::Token);
            }
            Opcode::RegionFree => {
                self.rt_builtin("__wolf_rt_region_free")?;
                let h = self.op(args[0])?;
                self.line(format!("  call void @__wolf_rt_region_free(ptr {h})"));
            }
            Opcode::SyncFreeze => {
                self.rt_builtin("__wolf_rt_region_freeze")?;
                let h = self.op(args[0])?;
                self.line(format!("  call void @__wolf_rt_region_freeze(ptr {h})"));
                self.vals.insert(results[0], Repr::Token);
            }
            Opcode::StackAlloc => {
                let size = self.const_of(args[0]).ok_or_else(|| {
                    nyi("stack.alloc with a non-constant size (not emitted by lowering)")
                })?;
                let size32 =
                    u32::try_from(size).map_err(|_| ice("stack.alloc size out of range"))?;
                let slot = self.new_slot(Layout {
                    size: size32,
                    align: 16,
                });
                self.vals.insert(results[0], Repr::Scalar(slot));
                self.vals.insert(results[1], Repr::Token);
            }
            // s75: a token root over runtime-owned storage. Tokens are
            // an ordering discipline, not a machine value — there is
            // nothing to emit, and that is the whole point: container
            // element traffic becomes plain `load`/`store` the
            // optimizer can see.
            Opcode::RegionForeign => {
                self.vals.insert(results[0], Repr::Token);
            }
            Opcode::RcDup | Opcode::RcDrop => {
                return Err(nyi(
                    "shared-tier rc ops (runtime shape lands with the shared cells, s42)",
                ));
            }
            Opcode::SyncTransfer => {
                return Err(nyi("sync.transfer is reserved (concurrency campaign)"));
            }

            // ---- terminators ----
            Opcode::Jmp => {
                let Aux::Jump(bc) = data.aux else {
                    return Err(ice("jmp without edge"));
                };
                let pred = self.cur_label();
                let target = self.edge(bc, &pred)?;
                self.line(format!("  br label %{target}"));
            }
            Opcode::Br => {
                let Aux::Br(t, e) = data.aux else {
                    return Err(ice("br without edges"));
                };
                let cv = self.op(args[0])?;
                let cond = self.tmp();
                self.line(format!("  {cond} = icmp ne i8 {cv}, 0"));
                // Error-row arcs are cold (report 10 delta 3): a `br`
                // whose condition is `eu.is_err` takes its then-edge
                // only on the error path.
                let err_arc = !self.cx.opts.strip_facts
                    && matches!(self.f.values[args[0]].def,
                        ValueDef::Result(i, _) if self.f.insts[i].op == Opcode::EuIsErr);
                // Semantic coldness outranks a measured weight: an
                // error arc is cold because of what it MEANS, and a
                // training run that happened to take it often does not
                // make the error path the expected one.
                let measured = (!err_arc)
                    .then(|| {
                        Some((
                            self.bcount.get(&t.block).copied()?,
                            self.bcount.get(&e.block).copied()?,
                        ))
                    })
                    .flatten();
                let prof = if err_arc {
                    format!(", !prof !{}", self.cx.prof_cold_first())
                } else if let Some((ct, ce)) = measured {
                    format!(", !prof !{}", self.cx.prof_measured(ct, ce))
                } else {
                    String::new()
                };
                // Per-edge trampolines whenever an edge carries
                // aggregate copies or both edges share a target (phi
                // incoming needs distinct predecessor labels).
                let has_agg = |fx: &Self, bc: wolf_wir::ir::BlockCall| {
                    fx.f.block_params(bc.block)
                        .iter()
                        .any(|&p| is_agg(fx.m, fx.f.value_ty(p)))
                };
                if has_agg(self, t) || has_agg(self, e) || t.block == e.block {
                    self.tmp += 1;
                    let lt = format!("edge{}", self.tmp);
                    self.tmp += 1;
                    let le = format!("edge{}", self.tmp);
                    self.line(format!("  br i1 {cond}, label %{lt}, label %{le}{prof}"));
                    for (lab, bc) in [(lt, t), (le, e)] {
                        let idx = self.new_block(lab.clone());
                        self.cur = idx;
                        let target = self.edge(bc, &lab)?;
                        self.line(format!("  br label %{target}"));
                    }
                } else {
                    let pred = self.cur_label();
                    let lt = self.edge(t, &pred)?;
                    let le = self.edge(e, &pred)?;
                    self.line(format!("  br i1 {cond}, label %{lt}, label %{le}{prof}"));
                }
            }
            Opcode::Ret => {
                let si = sig_info(self.m, self.f.sig, self.conv, &self.cx.opts.clone());
                let mut comps: Vec<(String, String)> = Vec::new();
                let mut sret_i = 0usize;
                for (&rv, slot) in args.iter().zip(si.rets.iter()) {
                    match slot {
                        RSlot::Token => {}
                        RSlot::Direct(ty) => comps.push((ty.to_string(), self.op(rv)?)),
                        RSlot::Split(units) => {
                            let src = self.addr(rv)?;
                            for u in units {
                                let v = self.load_unit(&src, u)?;
                                comps.push((unit_ty(u).to_string(), v));
                            }
                        }
                        RSlot::Sret {
                            disc_in_reg,
                            c,
                            size,
                            ..
                        } => {
                            let dst = self.sret[sret_i].clone();
                            sret_i += 1;
                            let src = self.addr(rv)?;
                            self.copy_bytes(&dst, &src, *size, "", "")?;
                            if *disc_in_reg && !c {
                                let tag = self.tmp();
                                self.line(format!("  {tag} = load i64, ptr {src}, align 1"));
                                comps.push(("i64".to_string(), tag));
                            }
                        }
                    }
                }
                match comps.len() {
                    0 => self.line("  ret void".to_string()),
                    1 => self.line(format!("  ret {} {}", comps[0].0, comps[0].1)),
                    _ => {
                        let sty = format!(
                            "{{ {} }}",
                            comps
                                .iter()
                                .map(|(t, _)| t.clone())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        let mut cur = "undef".to_string();
                        for (i, (ty, v)) in comps.iter().enumerate() {
                            let t = self.tmp();
                            self.line(format!("  {t} = insertvalue {sty} {cur}, {ty} {v}, {i}"));
                            cur = t;
                        }
                        self.line(format!("  ret {sty} {cur}"));
                    }
                }
            }
            Opcode::Trap => {
                let kind = match data.aux {
                    Aux::Trap(k) => k,
                    _ => TrapKind::Assert,
                };
                self.rt_builtin("__wolf_rt_trap")?;
                self.line(format!(
                    "  call void @__wolf_rt_trap(i32 {})",
                    trap_code(kind)
                ));
                self.line("  unreachable".to_string());
            }
        }
        Ok(())
    }

    /// Constant payload of a value defined by `iconst`, if any.
    fn const_of(&self, v: WValue) -> Option<i64> {
        match self.f.values[v].def {
            ValueDef::Result(inst, 0) if self.f.insts[inst].op == Opcode::Iconst => {
                match self.f.insts[inst].aux {
                    Aux::Int(n) => Some(n),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Store one field value (scalar or aggregate) at `base + off`.
    fn store_field(
        &mut self,
        base: &str,
        off: u32,
        fty: TypeId,
        v: WValue,
    ) -> Result<(), BackendError> {
        if is_agg(self.m, fty) {
            let src = self.addr(v)?;
            let size = self.layout_of(fty)?.size;
            let dst = self.gep(base, off as i64);
            self.copy_bytes(&dst, &src, size, "", "")
        } else {
            let ty = scalar_ty(self.m, fty).ok_or_else(|| ice("field of token type"))?;
            let cv = self.op(v)?;
            let p = self.gep(base, off as i64);
            self.line(format!(
                "  store {ty} {cv}, ptr {p}, align {}",
                self.align_of(natural_align(ty))
            ));
            Ok(())
        }
    }

    /// Load one field: scalar loads a value; aggregate is a view into
    /// the parent's bytes (SSA immutability makes sharing sound).
    fn load_field(&mut self, base: &str, off: u32, fty: TypeId) -> Result<Repr, BackendError> {
        if is_agg(self.m, fty) {
            Ok(Repr::Addr(self.gep(base, off as i64)))
        } else {
            let ty = scalar_ty(self.m, fty).ok_or_else(|| ice("field of token type"))?;
            let p = self.gep(base, off as i64);
            let t = self.tmp();
            self.line(format!(
                "  {t} = load {ty}, ptr {p}, align {}",
                self.align_of(natural_align(ty))
            ));
            Ok(Repr::Scalar(t))
        }
    }

    /// Checked add/sub/mul via the `*.with.overflow` intrinsics (X3):
    /// the overflow bit branches to the cold per-kind trap block.
    fn checked_arith(
        &mut self,
        op: Opcode,
        wa: WValue,
        wb: WValue,
        res: WValue,
    ) -> Result<String, BackendError> {
        let ty = self.sty(res)?;
        let (a, b) = (self.op(wa)?, self.op(wb)?);
        let base = match op {
            Opcode::IaddChk => "sadd",
            Opcode::IsubChk => "ssub",
            Opcode::ImulChk => "smul",
            Opcode::UaddChk => "uadd",
            Opcode::UsubChk => "usub",
            _ => "umul",
        };
        let iname = format!("llvm.{base}.with.overflow.{ty}");
        self.intrinsic(&format!("declare {{ {ty}, i1 }} @{iname}({ty}, {ty})"));
        let pair = self.tmp();
        self.line(format!(
            "  {pair} = call {{ {ty}, i1 }} @{iname}({ty} {a}, {ty} {b})"
        ));
        let r = self.tmp();
        self.line(format!("  {r} = extractvalue {{ {ty}, i1 }} {pair}, 0"));
        let o = self.tmp();
        self.line(format!("  {o} = extractvalue {{ {ty}, i1 }} {pair}, 1"));
        self.trap_if(&o, TrapKind::Overflow)?;
        Ok(r)
    }

    /// Checked division/remainder: explicit zero (and MIN/-1 for
    /// signed) checks branch to the trap blocks; the native op runs on
    /// the safe residue.
    fn checked_div(
        &mut self,
        op: Opcode,
        wa: WValue,
        wb: WValue,
        res: WValue,
    ) -> Result<String, BackendError> {
        let ty = self.sty(res)?;
        let (a, b) = (self.op(wa)?, self.op(wb)?);
        let z = self.tmp();
        self.line(format!("  {z} = icmp eq {ty} {b}, 0"));
        self.trap_if(&z, TrapKind::DivZero)?;
        match op {
            Opcode::IdivChk | Opcode::IremChk => {
                let (lo, _) = self
                    .m
                    .types
                    .int_bounds(self.f.value_ty(res))
                    .ok_or_else(|| ice("div on non-int"))?;
                let mn = self.tmp();
                self.line(format!("  {mn} = icmp eq {ty} {a}, {lo}"));
                let m1 = self.tmp();
                self.line(format!("  {m1} = icmp eq {ty} {b}, -1"));
                let ov = self.tmp();
                self.line(format!("  {ov} = and i1 {mn}, {m1}"));
                self.trap_if(&ov, TrapKind::Overflow)?;
                let t = self.tmp();
                let mnq = if op == Opcode::IdivChk {
                    "sdiv"
                } else {
                    "srem"
                };
                self.line(format!("  {t} = {mnq} {ty} {a}, {b}"));
                Ok(t)
            }
            _ => {
                let t = self.tmp();
                let mnq = if op == Opcode::UdivChk {
                    "udiv"
                } else {
                    "urem"
                };
                self.line(format!("  {t} = {mnq} {ty} {a}, {b}"));
                Ok(t)
            }
        }
    }

    /// Saturating add/sub/mul (clamps, never traps). add/sub use the
    /// native sat intrinsics; mul widens (<64) or clamps on the
    /// overflow bit (64).
    fn sat_arith(
        &mut self,
        op: Opcode,
        wa: WValue,
        wb: WValue,
        res: WValue,
    ) -> Result<String, BackendError> {
        let rty = self.f.value_ty(res);
        let ty = self.sty(res)?;
        let bits = self
            .m
            .types
            .int_bits(rty)
            .ok_or_else(|| ice("sat arith on non-int"))?;
        let (a, b) = (self.op(wa)?, self.op(wb)?);
        let (lo, hi) = self
            .m
            .types
            .int_bounds(rty)
            .ok_or_else(|| ice("bounds of non-int"))?;
        match op {
            Opcode::IaddSat | Opcode::IsubSat => {
                let base = if op == Opcode::IaddSat {
                    "sadd"
                } else {
                    "ssub"
                };
                let iname = format!("llvm.{base}.sat.{ty}");
                self.intrinsic(&format!("declare {ty} @{iname}({ty}, {ty})"));
                let t = self.tmp();
                self.line(format!("  {t} = call {ty} @{iname}({ty} {a}, {ty} {b})"));
                Ok(t)
            }
            _ if bits < 64 => {
                // Widen, multiply exactly, clamp, truncate.
                self.intrinsic("declare i64 @llvm.smax.i64(i64, i64)");
                self.intrinsic("declare i64 @llvm.smin.i64(i64, i64)");
                let xa = self.tmp();
                self.line(format!("  {xa} = sext {ty} {a} to i64"));
                let xb = self.tmp();
                self.line(format!("  {xb} = sext {ty} {b} to i64"));
                let r = self.tmp();
                self.line(format!("  {r} = mul i64 {xa}, {xb}"));
                let mx = self.tmp();
                self.line(format!(
                    "  {mx} = call i64 @llvm.smax.i64(i64 {r}, i64 {lo})"
                ));
                let mnv = self.tmp();
                self.line(format!(
                    "  {mnv} = call i64 @llvm.smin.i64(i64 {mx}, i64 {hi})"
                ));
                let t = self.tmp();
                self.line(format!("  {t} = trunc i64 {mnv} to {ty}"));
                Ok(t)
            }
            _ => {
                // i64: overflow bit picks the clamp; the product's
                // sign picks its direction.
                let iname = "llvm.smul.with.overflow.i64";
                self.intrinsic(&format!("declare {{ i64, i1 }} @{iname}(i64, i64)"));
                let pair = self.tmp();
                self.line(format!(
                    "  {pair} = call {{ i64, i1 }} @{iname}(i64 {a}, i64 {b})"
                ));
                let r = self.tmp();
                self.line(format!("  {r} = extractvalue {{ i64, i1 }} {pair}, 0"));
                let o = self.tmp();
                self.line(format!("  {o} = extractvalue {{ i64, i1 }} {pair}, 1"));
                let sa = self.tmp();
                self.line(format!("  {sa} = icmp slt i64 {a}, 0"));
                let sb = self.tmp();
                self.line(format!("  {sb} = icmp slt i64 {b}, 0"));
                let neg = self.tmp();
                self.line(format!("  {neg} = xor i1 {sa}, {sb}"));
                let clamp = self.tmp();
                self.line(format!("  {clamp} = select i1 {neg}, i64 {lo}, i64 {hi}"));
                let t = self.tmp();
                self.line(format!("  {t} = select i1 {o}, i64 {clamp}, i64 {r}"));
                Ok(t)
            }
        }
    }

    /// Lower one call, executing the callee's ABI plan (tokens erased,
    /// small aggregates in registers, big ones through memory, error
    /// unions per `[abi.err.repr]`).
    fn lower_call(
        &mut self,
        callee: &str,
        sig: SigId,
        args: &[WValue],
        results: &[WValue],
    ) -> Result<(), BackendError> {
        // Resolve: module function → `c.*` membrane → runtime shim (by
        // prefix; its WIR sig is the machine truth once tokens erase)
        // → out-of-subset wolf function by mangled symbol → refusal.
        let (symbol, conv) = if let Some(entry) = self.funcs.get(callee) {
            let conv = if entry.export { Conv::C } else { Conv::Wolf };
            (entry.symbol.clone(), conv)
        } else if let Some(sym) = abi::c_import_symbol(callee) {
            (sym.to_string(), Conv::C)
        } else if callee.starts_with("__wolf_rt_") {
            (callee.to_string(), Conv::Wolf)
        } else if self.m.funcs.values().any(|f| f.name == callee) {
            (wolf_backend::mangle(self.m, callee, sig), Conv::Wolf)
        } else {
            return Err(nyi(format!(
                "call to external `@{callee}` (hand-declared externs beyond the \
                 modelled c.* set are c10's header importer)"
            )));
        };
        let si = sig_info(self.m, sig, conv, &self.cx.opts.clone());
        // Declare the callee if this module does not define it.
        let mut extra = String::new();
        if matches!(callee, "__wolf_rt_main_err" | "__wolf_rt_os_exit") {
            extra = " cold noreturn".to_string();
        }
        let decl = format!(
            "declare {} @\"{symbol}\"({}){extra} nounwind",
            si.ret_ty(),
            si.ll_params.join(", ")
        );
        self.ensure_decl(&symbol, decl);

        self.emit_call_body(&format!("@\"{symbol}\""), &si, sig, args, results)
    }

    /// `call.ind` (s97): the callee is a VALUE, spelled as its LLVM
    /// operand — always wolf-conv (the C membrane is name-based; no fn
    /// value can name a `c.*` import, refused upstream) — through the
    /// direct call's exact marshaling (target 3's parity clause). No
    /// `declare` is emitted: there is no symbol to declare.
    fn lower_call_ind(
        &mut self,
        sig: SigId,
        args: &[WValue],
        results: &[WValue],
    ) -> Result<(), BackendError> {
        let si = sig_info(self.m, sig, Conv::Wolf, &self.cx.opts.clone());
        let callee = self.op(args[0])?;
        self.emit_call_body(&callee, &si, sig, &args[1..], results)
    }

    /// The shared back half of both call forms: out-slots, argument
    /// marshaling per the ABI plan, the call line, result unpacking.
    fn emit_call_body(
        &mut self,
        callee_ll: &str,
        si: &LSig,
        sig: SigId,
        args: &[WValue],
        results: &[WValue],
    ) -> Result<(), BackendError> {
        // Out-slots for memory-class results, in result order.
        let rdata = self.m.sigs[sig].results.clone();
        let mut sret_addrs: Vec<String> = Vec::new();
        let mut first_args: Vec<String> = Vec::new();
        for (&rty, slot) in rdata.iter().zip(si.rets.iter()) {
            if let RSlot::Sret { c, size, align, .. } = slot {
                let l = self.layout_of(rty)?;
                let s = self.new_slot(l);
                sret_addrs.push(s.clone());
                if *c {
                    first_args.push(format!("ptr sret([{size} x i8]) align {align} {s}"));
                }
            }
        }
        let mut cargs: Vec<String> = first_args;
        for (&av, slot) in args.iter().zip(si.params.iter()) {
            match slot {
                Slot::Token => {}
                Slot::Direct(ty) => cargs.push(format!("{ty} {}", self.op(av)?)),
                Slot::Split(units) => {
                    let units = units.clone();
                    let base = self.addr(av)?;
                    for u in &units {
                        let v = self.load_unit(&base, u)?;
                        cargs.push(format!("{} {v}", unit_ty(u)));
                    }
                }
                Slot::Ref => cargs.push(format!("ptr {}", self.addr(av)?)),
                Slot::CStruct(size, align) => cargs.push(format!(
                    "ptr byval([{} x i8]) align {} {}",
                    size.next_multiple_of(8),
                    (*align).max(8),
                    self.addr(av)?
                )),
            }
        }
        if !si.sret_first {
            for s in &sret_addrs {
                cargs.push(format!("ptr {s}"));
            }
        }
        let ret_ty = si.ret_ty();
        let na = self.call_noalias_suffix(args);
        let rendered = format!("call {ret_ty} {callee_ll}({}){na}", cargs.join(", "));
        let retval = if si.ret_comps.is_empty() {
            self.line(format!("  {rendered}"));
            String::new()
        } else {
            let t = self.tmp();
            self.line(format!("  {t} = {rendered}"));
            t
        };
        // Unpack components.
        let mut comp_ops: Vec<String> = Vec::new();
        match si.ret_comps.len() {
            0 => {}
            1 => comp_ops.push(retval.clone()),
            n => {
                for i in 0..n {
                    let t = self.tmp();
                    self.line(format!("  {t} = extractvalue {ret_ty} {retval}, {i}"));
                    comp_ops.push(t);
                }
            }
        }
        // Map declared results, then trailing successor tokens.
        let mut ci = 0usize;
        let mut sret_i = 0usize;
        let mut wr = results.iter();
        for slot in si.rets.iter() {
            let &rv = wr.next().ok_or_else(|| ice("call result arity"))?;
            match slot {
                RSlot::Token => {
                    self.vals.insert(rv, Repr::Token);
                }
                RSlot::Direct(_) => {
                    self.vals.insert(rv, Repr::Scalar(comp_ops[ci].clone()));
                    ci += 1;
                }
                RSlot::Split(units) => {
                    let units = units.clone();
                    let slot = self.units_slot(&units);
                    for u in &units {
                        let v = comp_ops[ci].clone();
                        ci += 1;
                        self.store_unit(&slot, u, &v)?;
                    }
                    self.vals.insert(rv, Repr::Addr(slot));
                }
                RSlot::Sret { disc_in_reg, c, .. } => {
                    if *disc_in_reg && !c {
                        ci += 1; // advisory register discriminant
                    }
                    self.vals.insert(rv, Repr::Addr(sret_addrs[sret_i].clone()));
                    sret_i += 1;
                }
            }
        }
        for &rv in wr {
            self.vals.insert(rv, Repr::Token);
        }
        Ok(())
    }

    // ---- rendering ---------------------------------------------------------

    fn render(&mut self, symbol: &str, linkage: &str, si: &LSig) -> Result<String, BackendError> {
        let mut out = String::new();
        // Def-site parameter list: names + deref facts on ptr params.
        let entry = self.f.entry().ok_or_else(|| ice("no entry"))?;
        let wparams = self.f.block_params(entry);
        let mut named: Vec<String> = Vec::new();
        let mut pi = 0usize;
        // C sret slots occupy the first LLVM params.
        let n_ll = si.ll_params.len();
        // Map WIR param index → its first LLVM param index, to attach
        // deref facts to Direct ptr params.
        let mut deref_attr: HashMap<usize, u64> = HashMap::new();
        if !self.cx.opts.strip_facts {
            let n_sret_first = if si.sret_first {
                si.rets
                    .iter()
                    .filter(|r| matches!(r, RSlot::Sret { .. }))
                    .count()
            } else {
                0
            };
            let mut next = n_sret_first;
            for (i, &wv) in wparams.iter().enumerate() {
                match &si.params[i] {
                    Slot::Token => {}
                    Slot::Direct(_) => {
                        if let Some(&n) = self.deref.get(&wv) {
                            deref_attr.insert(next, n);
                        }
                        next += 1;
                    }
                    Slot::Split(units) => next += units.len(),
                    Slot::Ref | Slot::CStruct(..) => next += 1,
                }
            }
        }
        for (k, p) in si.ll_params.iter().enumerate() {
            let extra = match deref_attr.get(&k) {
                Some(n) => format!(" dereferenceable({n})"),
                None => String::new(),
            };
            named.push(format!("{p}{extra} %p{k}"));
            pi += 1;
        }
        debug_assert_eq!(pi, n_ll);
        let _ = writeln!(
            out,
            "define {linkage}{} @\"{symbol}\"({}) nounwind {{",
            si.ret_ty(),
            named.join(", ")
        );
        // Blocks: entry first (allocas prepended), then the rest with
        // phi lines ahead of their bodies.
        let wlabel_of: HashMap<&str, WBlock> =
            self.wlabel.iter().map(|(b, l)| (l.as_str(), *b)).collect();
        for (i, b) in self.blocks.iter().enumerate() {
            let _ = writeln!(out, "{}:", b.label);
            if i == 0 {
                for a in &self.allocas {
                    let _ = writeln!(out, "{a}");
                }
            }
            if let Some(&wb) = wlabel_of.get(b.label.as_str())
                && let Some(accs) = self.phis.get(&wb)
            {
                for acc in accs {
                    if acc.incoming.is_empty() {
                        return Err(ice(format!(
                            "block {} has a param but no incoming edges",
                            b.label
                        )));
                    }
                    let inc: Vec<String> = acc
                        .incoming
                        .iter()
                        .map(|(l, v)| format!("[ {v}, %{l} ]"))
                        .collect();
                    let _ = writeln!(out, "  {} = phi {} {}", acc.name, acc.ty, inc.join(", "));
                }
            }
            for l in &b.lines {
                let _ = writeln!(out, "{l}");
            }
        }
        out.push_str("}\n");
        Ok(out)
    }
}

/// Render bytes as an LLVM `c"..."` constant.
fn bytes_const(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "zeroinitializer".to_string();
    }
    let mut s = String::with_capacity(bytes.len() + 4);
    s.push_str("c\"");
    for &b in bytes {
        match b {
            b'"' | b'\\' => {
                let _ = write!(s, "\\{b:02X}");
            }
            0x20..=0x7e => s.push(b as char),
            _ => {
                let _ = write!(s, "\\{b:02X}");
            }
        }
    }
    s.push('"');
    s
}

fn int_cc(cc: IntCc) -> &'static str {
    match cc {
        IntCc::Eq => "eq",
        IntCc::Ne => "ne",
        IntCc::Slt => "slt",
        IntCc::Sle => "sle",
        IntCc::Sgt => "sgt",
        IntCc::Sge => "sge",
        IntCc::Ult => "ult",
        IntCc::Ule => "ule",
        IntCc::Ugt => "ugt",
        IntCc::Uge => "uge",
    }
}

fn float_cc(cc: FloatCc) -> &'static str {
    // WIR float compares are ORDERED for eq/lt/le/gt/ge; `ne` is the
    // negation of `eq`, so UNORDERED (`x != x` is the NaN test).
    match cc {
        FloatCc::Eq => "oeq",
        FloatCc::Ne => "une",
        FloatCc::Lt => "olt",
        FloatCc::Le => "ole",
        FloatCc::Gt => "ogt",
        FloatCc::Ge => "oge",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trap codes are pinned to `wolf_rt`'s (the single authority;
    /// a drift here would misreport trap identities at run time).
    #[test]
    fn trap_codes_match_wolf_rt() {
        use wolf_rt::native::trap_code as tc;
        assert_eq!(trap_code(TrapKind::Overflow), tc::OVERFLOW);
        assert_eq!(trap_code(TrapKind::DivZero), tc::DIV_ZERO);
        assert_eq!(trap_code(TrapKind::Bounds), tc::BOUNDS);
        assert_eq!(trap_code(TrapKind::Assert), tc::ASSERT);
    }

    #[test]
    fn byte_constants_escape() {
        assert_eq!(bytes_const(b"hi"), "c\"hi\"");
        assert_eq!(bytes_const(b"a\"b\\c\n"), "c\"a\\22b\\5Cc\\0A\"");
        assert_eq!(bytes_const(b""), "zeroinitializer");
    }

    #[test]
    fn int_render_wraps_to_width() {
        assert_eq!(Fx::int_render(8, 127 + 1), -128);
        assert_eq!(Fx::int_render(8, -128), -128);
        assert_eq!(Fx::int_render(64, i64::MAX as i128 + 1), i64::MIN as i128);
    }
}
