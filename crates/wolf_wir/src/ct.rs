//! The constant-time taint verifier (c28, spec/09 `[ct.taint]`).
//!
//! For every function carrying a [`crate::ir::CtContract`], a forward
//! dataflow over the SSA values decides what is SECRET — the contract's
//! secret parameters and everything derived from them — and then the
//! sink pass refuses, fail-closed, every place a secret would become
//! control flow, a memory address, a call target, or a variable-time
//! instruction. Taint rides VALUES, never facts or Aux metadata
//! (`[ct.taint.prop]`): a range fact about a key byte is knowledge, not
//! program data. Effect tokens (`mem.rN`, `io`) never carry taint —
//! they sequence memory, they do not hold it.
//!
//! Two kinds of secrecy per value:
//! - **value taint** — the bits themselves are secret (a key byte, a
//!   comparison of key bytes, a pointer offset by a secret index);
//! - **contents secrecy** — the value is a pointer into storage that
//!   may hold secret data, while the pointer itself (an allocator
//!   artifact) is public. Contents secrecy lives on ROOTS — entry
//!   pointers and pointer-minting instructions — and v1's granularity
//!   is the whole object (`[ct.taint.source]`): any load reached
//!   through a secret root yields a secret value, lengths included.
//!   The spine/leaf refinement is named residue.
//!
//! The verifier runs on constructed WIR and again on the final
//! pre-emission form (`[ct.taint.verify]`), so a mid-end transform
//! that introduces a violation is refused, not audited pass by pass.
//! Functions without the contract are never walked: the tier is free
//! for every program that does not ask for it.

use crate::ir::{Aux, Function, Inst, Module, Value};
use crate::ops::{Opcode, TrapKind};
use std::collections::HashMap;
use wolf_diag::{Diagnostic, codes};
use wolf_span::{FileId, Span};

/// One refusal class of `[ct.taint.sink]` — each its own E-code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CtSink {
    /// E1601 — `br` on a secret condition.
    Branch,
    /// E1602 — load/store address (or its guarding bounds check)
    /// derived from a secret.
    MemAddr,
    /// E1603 — `call.ind` target derived from a secret.
    CallTarget,
    /// E1604 — division/remainder with a secret operand.
    DivRem,
    /// E1605 — a secret argument crossing the membrane.
    Membrane,
    /// E1606 — checked arithmetic on a secret operand.
    CheckedArith,
}

impl CtSink {
    pub fn code(self) -> wolf_diag::Code {
        match self {
            CtSink::Branch => codes::E1601,
            CtSink::MemAddr => codes::E1602,
            CtSink::CallTarget => codes::E1603,
            CtSink::DivRem => codes::E1604,
            CtSink::Membrane => codes::E1605,
            CtSink::CheckedArith => codes::E1606,
        }
    }

    pub fn code_str(self) -> &'static str {
        match self {
            CtSink::Branch => "E1601",
            CtSink::MemAddr => "E1602",
            CtSink::CallTarget => "E1603",
            CtSink::DivRem => "E1604",
            CtSink::Membrane => "E1605",
            CtSink::CheckedArith => "E1606",
        }
    }
}

/// One refusal, with everything the driver needs to render it.
#[derive(Clone, Debug)]
pub struct CtViolation {
    pub sink: CtSink,
    /// The consttime function refused.
    pub func: String,
    /// The offending instruction's source span, when lowering
    /// recorded one (spans are debug aux and may thin post-mid-end).
    pub span: Option<Span>,
    /// The specific finding, in message position.
    pub detail: String,
}

impl CtViolation {
    /// The rendered diagnostic (`[ct.taint.sink]`). Deterministic —
    /// the corpus and the snapshot suite both pin it.
    pub fn diagnostic(&self, fallback: Span) -> Diagnostic {
        let d = Diagnostic::error(
            self.sink.code(),
            self.span.unwrap_or(fallback),
            format!("in consttime fn `{}`: {}", self.func, self.detail),
        );
        match self.sink {
            CtSink::CheckedArith => d.with_note(
                "checked arithmetic branches on its operands' values to trap — spell the \
                 kernel's arithmetic in `wrapping[T]`, which is branch-free",
            ),
            CtSink::Membrane => d.with_note(
                "the constant-time contract stops at unverified code — mark the callee \
                 `#[consttime]`, or keep the secret's work inside this function",
            ),
            CtSink::DivRem => d.with_note(
                "hardware division latency depends on operand values; there is no \
                 constant-time spelling of `/` or `%` — restructure with masks, shifts, \
                 or multiply-based reduction",
            ),
            _ => d,
        }
    }
}

/// Verify every consttime function in `m` (`[ct.taint]`). Returns the
/// refusals in deterministic order: functions in module order, then
/// instructions in layout order, at most one refusal per instruction.
/// Empty means the module keeps its constant-time promises at this
/// form of the program.
pub fn check_module(m: &Module) -> Vec<CtViolation> {
    let by_name: HashMap<&str, &Function> =
        m.funcs.values().map(|f| (f.name.as_str(), f)).collect();
    let mut out = Vec::new();
    for f in m.funcs.values() {
        if f.consttime.is_some() {
            check_function(m, f, &by_name, &mut out);
        }
    }
    out
}

/// Per-value dataflow state. Monotone: bits only ever turn on, so the
/// fixpoint terminates.
struct St {
    /// Value taint: the bits are secret.
    taint: Vec<bool>,
    /// Root set per value: which storage this (pointer) value may
    /// point into. Sorted, deduplicated.
    roots: Vec<Vec<u32>>,
    /// Contents secrecy per root.
    root_secret: Vec<bool>,
}

impl St {
    fn tainted(&self, v: Value) -> bool {
        self.taint[index(v)]
    }
    /// Value or contents secret — what "carries a secret" means for a
    /// call argument ([ct.taint.membrane]).
    fn carries_secret(&self, v: Value) -> bool {
        self.taint[index(v)]
            || self.roots[index(v)]
                .iter()
                .any(|&r| self.root_secret[r as usize])
    }
    fn contents_secret(&self, v: Value) -> bool {
        self.roots[index(v)]
            .iter()
            .any(|&r| self.root_secret[r as usize])
    }
}

fn index(v: Value) -> usize {
    use crate::entity::EntityRef;
    v.index()
}

fn merge_roots(dst: &mut Vec<u32>, src: &[u32]) -> bool {
    let mut changed = false;
    for &r in src {
        if let Err(at) = dst.binary_search(&r) {
            dst.insert(at, r);
            changed = true;
        }
    }
    changed
}

fn check_function(
    m: &Module,
    f: &Function,
    by_name: &HashMap<&str, &Function>,
    out: &mut Vec<CtViolation>,
) {
    let ct = f.consttime.as_ref().expect("checked by caller");
    let nvals = f.values.len();
    let mut st = St {
        taint: vec![false; nvals],
        roots: vec![Vec::new(); nvals],
        root_secret: Vec::new(),
    };
    let mut next_root = 0u32;
    let mut mint = |secret: bool, root_secret: &mut Vec<bool>| -> u32 {
        let id = next_root;
        next_root += 1;
        root_secret.push(secret);
        id
    };
    let is_token = |v: Value| m.types.is_token(f.value_ty(v));
    let is_ptr = |v: Value| matches!(m.types.get(f.value_ty(v)), crate::types::TypeData::Ptr);

    // ---- sources ([ct.taint.source]) ---------------------------------
    // Entry params in signature order; the contract's indices name the
    // secret ones. A secret scalar taints its value; a secret pointer
    // gets a secret root (its contents), staying value-public.
    let entry = f.layout[0];
    let entry_params: Vec<Value> = f.vpool.get(f.blocks[entry].params);
    for (i, &v) in entry_params.iter().enumerate() {
        if is_token(v) {
            continue;
        }
        let secret = ct.secret_params.contains(&(i as u16));
        if is_ptr(v) {
            let r = mint(secret, &mut st.root_secret);
            st.roots[index(v)].push(r);
        } else if secret {
            st.taint[index(v)] = true;
        }
    }
    // Root-minting instructions get ONE root each, reused across
    // fixpoint iterations (ids must be stable).
    let mut inst_root: HashMap<Inst, u32> = HashMap::new();
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            let data = &f.insts[inst];
            let mints = matches!(
                data.op,
                Opcode::RegionNew
                    | Opcode::RegionAlloc
                    | Opcode::StackAlloc
                    | Opcode::DataAddr
                    | Opcode::Load
                    | Opcode::Call
                    | Opcode::CallInd
            );
            if mints {
                let any_ptr_result = f
                    .vpool
                    .get(data.results)
                    .iter()
                    .any(|&r| is_ptr(r) && !is_token(r));
                if any_ptr_result {
                    inst_root.insert(inst, mint(false, &mut st.root_secret));
                }
            }
        }
    }

    // ---- propagation to fixpoint ([ct.taint.prop]) -------------------
    let mut changed = true;
    while changed {
        changed = false;
        for &b in &f.layout {
            for &inst in &f.blocks[b].insts {
                changed |= transfer(m, f, by_name, &mut st, &inst_root, inst);
            }
        }
    }

    // ---- sinks ([ct.taint.sink]), deterministic order ----------------
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            if let Some(v) = sink(m, f, by_name, &st, inst) {
                out.push(v);
            }
        }
    }
}

/// One instruction's dataflow transfer. Returns whether anything grew.
fn transfer(
    m: &Module,
    f: &Function,
    by_name: &HashMap<&str, &Function>,
    st: &mut St,
    inst_root: &HashMap<Inst, u32>,
    inst: Inst,
) -> bool {
    let data = &f.insts[inst];
    let args: Vec<Value> = f.vpool.get(data.args);
    let results: Vec<Value> = f.vpool.get(data.results);
    let is_token = |v: Value| m.types.is_token(f.value_ty(v));
    let mut changed = false;

    let arg_taint = args
        .iter()
        .filter(|&&a| !is_token(a))
        .any(|&a| st.taint[index(a)]);
    let arg_roots: Vec<u32> = {
        let mut all = Vec::new();
        for &a in &args {
            if !is_token(a) {
                for &r in &st.roots[index(a)] {
                    if let Err(at) = all.binary_search(&r) {
                        all.insert(at, r);
                    }
                }
            }
        }
        all
    };
    let roots_secret = arg_roots.iter().any(|&r| st.root_secret[r as usize]);

    match data.op {
        // Terminators: propagate branch arguments into block params.
        Opcode::Jmp => {
            if let Aux::Jump(bc) = data.aux {
                changed |= flow_edge(f, st, &bc);
            }
        }
        Opcode::Br => {
            if let Aux::Br(t, e) = data.aux {
                changed |= flow_edge(f, st, &t);
                changed |= flow_edge(f, st, &e);
            }
        }
        Opcode::Ret | Opcode::Trap => {}
        // Stores mark the stored-into roots secret when the stored
        // value carries a secret (by value or by contents).
        Opcode::Store => {
            let val = args[0];
            let addr = args[1];
            if st.carries_secret(val) {
                let addr_roots = st.roots[index(addr)].clone();
                for r in addr_roots {
                    if !st.root_secret[r as usize] {
                        st.root_secret[r as usize] = true;
                        changed = true;
                    }
                }
            }
        }
        // Loads: the result is secret iff the address reaches secret
        // storage (or the address itself is tainted — refused at the
        // sink pass, tainted here for completeness). A loaded pointer
        // points into unknown storage: it gets this load's own root,
        // secret iff the source storage was.
        Opcode::Load => {
            let addr = args[0];
            let src_secret = st.contents_secret(addr) || st.taint[index(addr)];
            let r = results[0];
            if src_secret && !st.taint[index(r)] {
                st.taint[index(r)] = true;
                changed = true;
            }
            if let Some(&root) = inst_root.get(&inst) {
                changed |= merge_roots(&mut st.roots[index(r)], &[root]);
                if src_secret && !st.root_secret[root as usize] {
                    st.root_secret[root as usize] = true;
                    changed = true;
                }
            }
        }
        // Calls: per the callee's contract ([ct.taint.prop]). A
        // consttime callee's result and writable arguments become
        // secret iff any argument carried a secret. A non-consttime
        // callee with secret arguments is refused at the sink pass;
        // with clean arguments its results are clean. Pointer results
        // get this call's root either way.
        Opcode::Call | Opcode::CallInd => {
            let callee_ct = match data.op {
                Opcode::Call => match data.aux {
                    Aux::Callee(ef) => by_name
                        .get(f.ext_funcs[ef].name.as_str())
                        .and_then(|cf| cf.consttime.as_ref()),
                    _ => None,
                },
                _ => None,
            };
            let secret_in = args
                .iter()
                .filter(|&&a| !is_token(a))
                .any(|&a| st.carries_secret(a));
            if callee_ct.is_some() && secret_in {
                for &r in &results {
                    if !is_token(r) && !st.taint[index(r)] {
                        st.taint[index(r)] = true;
                        changed = true;
                    }
                }
                // Writable (pointer) arguments may now hold secrets.
                let ptr_arg_roots: Vec<u32> = args
                    .iter()
                    .filter(|&&a| !is_token(a))
                    .flat_map(|&a| st.roots[index(a)].to_vec())
                    .collect();
                for r in ptr_arg_roots {
                    if !st.root_secret[r as usize] {
                        st.root_secret[r as usize] = true;
                        changed = true;
                    }
                }
            }
            if let Some(&root) = inst_root.get(&inst) {
                for &r in &results {
                    if !is_token(r) {
                        changed |= merge_roots(&mut st.roots[index(r)], &[root]);
                    }
                }
            }
        }
        // Root-minting non-call ops: results carry their own root.
        Opcode::RegionNew | Opcode::RegionAlloc | Opcode::StackAlloc | Opcode::DataAddr => {
            if let Some(&root) = inst_root.get(&inst) {
                for &r in &results {
                    if !is_token(r) {
                        changed |= merge_roots(&mut st.roots[index(r)], &[root]);
                    }
                }
            }
        }
        // Everything else: value ops. Result taint joins the operands'
        // value taints; result roots join the operands' root sets
        // (derived pointers — ptr.off, aggregates, error unions).
        _ => {
            for &r in &results {
                if is_token(r) {
                    continue;
                }
                if arg_taint && !st.taint[index(r)] {
                    st.taint[index(r)] = true;
                    changed = true;
                }
                changed |= merge_roots(&mut st.roots[index(r)], &arg_roots);
                // A pointer extracted from secret-rooted storage-borne
                // aggregates stays contents-secret through the root
                // set; nothing else to do here.
                let _ = roots_secret;
            }
        }
    }
    changed
}

/// Propagate one branch edge's arguments into the target block params.
fn flow_edge(f: &Function, st: &mut St, bc: &crate::ir::BlockCall) -> bool {
    let args: Vec<Value> = f.vpool.get(bc.args);
    let params: Vec<Value> = f.vpool.get(f.blocks[bc.block].params);
    let mut changed = false;
    for (&a, &p) in args.iter().zip(params.iter()) {
        if st.taint[index(a)] && !st.taint[index(p)] {
            st.taint[index(p)] = true;
            changed = true;
        }
        let src = st.roots[index(a)].clone();
        changed |= merge_roots(&mut st.roots[index(p)], &src);
    }
    changed
}

/// The sink pass for one instruction ([ct.taint.sink]). At most one
/// refusal per instruction — the most specific class wins.
fn sink(
    m: &Module,
    f: &Function,
    by_name: &HashMap<&str, &Function>,
    st: &St,
    inst: Inst,
) -> Option<CtViolation> {
    let data = &f.insts[inst];
    let args: Vec<Value> = f.vpool.get(data.args);
    let is_token = |v: Value| m.types.is_token(f.value_ty(v));
    let at = |sink: CtSink, detail: String| -> Option<CtViolation> {
        let span = f
            .srcspan(inst)
            .zip(f.src_file)
            .map(|(s, file)| Span::new(FileId::from_index(file as usize), s.lo, s.hi));
        Some(CtViolation {
            sink,
            func: f.name.clone(),
            span,
            detail,
        })
    };

    match data.op {
        Opcode::Br => {
            let cond = args[0];
            if st.tainted(cond) {
                // A bounds guard protecting a secret-indexed access is
                // the INDEX's sin: classify with the access (E1602).
                if bounds_guard(f, inst) {
                    return at(
                        CtSink::MemAddr,
                        "a container index derives from a secret (its bounds check would \
                         branch on the secret)"
                            .to_string(),
                    );
                }
                return at(
                    CtSink::Branch,
                    "this branch condition derives from a secret".to_string(),
                );
            }
            None
        }
        Opcode::Load => {
            let addr = args[0];
            if st.tainted(addr) {
                return at(
                    CtSink::MemAddr,
                    "this load's address derives from a secret".to_string(),
                );
            }
            None
        }
        Opcode::Store => {
            let addr = args[1];
            if st.tainted(addr) {
                return at(
                    CtSink::MemAddr,
                    "this store's address derives from a secret".to_string(),
                );
            }
            None
        }
        Opcode::IdivChk | Opcode::UdivChk => {
            if args.iter().any(|&a| st.tainted(a)) {
                return at(
                    CtSink::DivRem,
                    "division with a secret operand is variable-time on real hardware".to_string(),
                );
            }
            None
        }
        Opcode::IremChk | Opcode::UremChk => {
            if args.iter().any(|&a| st.tainted(a)) {
                return at(
                    CtSink::DivRem,
                    "remainder with a secret operand is variable-time on real hardware".to_string(),
                );
            }
            None
        }
        Opcode::IaddChk
        | Opcode::IsubChk
        | Opcode::ImulChk
        | Opcode::UaddChk
        | Opcode::UsubChk
        | Opcode::UmulChk => {
            if args.iter().any(|&a| st.tainted(a)) {
                return at(
                    CtSink::CheckedArith,
                    format!(
                        "checked `{}` on a secret operand would trap on a secret-dependent \
                         path",
                        data.op.base_mnemonic()
                    ),
                );
            }
            None
        }
        Opcode::CallInd => {
            let callee = args[0];
            if st.carries_secret(callee) {
                return at(
                    CtSink::CallTarget,
                    "this indirect call's target derives from a secret".to_string(),
                );
            }
            if args
                .iter()
                .skip(1)
                .any(|&a| !is_token(a) && st.carries_secret(a))
            {
                return at(
                    CtSink::Membrane,
                    "a secret argument reaches an indirect callee whose contract is unknowable"
                        .to_string(),
                );
            }
            None
        }
        Opcode::Call => {
            let Aux::Callee(ef) = data.aux else {
                return None;
            };
            let name = f.ext_funcs[ef].name.clone();
            let callee = by_name.get(name.as_str());
            match callee.and_then(|cf| cf.consttime.as_ref()) {
                Some(cct) => {
                    // Secret into a PUBLIC parameter of a consttime
                    // callee: the exemption is a license to branch, so
                    // this is a membrane crossing too.
                    for (i, &a) in args.iter().enumerate() {
                        if is_token(a) || !st.carries_secret(a) {
                            continue;
                        }
                        if !cct.secret_params.contains(&(i as u16)) {
                            return at(
                                CtSink::Membrane,
                                format!(
                                    "a secret argument reaches PUBLIC parameter {i} of \
                                     consttime fn `{name}`"
                                ),
                            );
                        }
                    }
                    None
                }
                None => {
                    if args.iter().any(|&a| !is_token(a) && st.carries_secret(a)) {
                        return at(
                            CtSink::Membrane,
                            format!(
                                "a secret argument crosses into `{name}`, which is not \
                                 consttime"
                            ),
                        );
                    }
                    None
                }
            }
        }
        // Allocation sized by a secret crosses into the allocator.
        Opcode::RegionAlloc => {
            let size = args[1];
            if st.tainted(size) {
                return at(
                    CtSink::Membrane,
                    "an allocation sized by a secret crosses into the allocator".to_string(),
                );
            }
            None
        }
        _ => None,
    }
}

/// `br %c, then, else` whose false arm is a bare `trap.bounds` block —
/// the bounds-guard shape lowering emits for container indexing (the
/// same recognition the range pass uses to count eliminable checks).
fn bounds_guard(f: &Function, inst: Inst) -> bool {
    let Aux::Br(_, e) = f.insts[inst].aux else {
        return false;
    };
    let insts = &f.blocks[e.block].insts;
    match insts.first() {
        Some(&i) => matches!(
            (f.insts[i].op, f.insts[i].aux),
            (Opcode::Trap, Aux::Trap(TrapKind::Bounds))
        ),
        None => false,
    }
}
