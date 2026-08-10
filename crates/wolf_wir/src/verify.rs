//! The WIR verifier — the compiler's own test oracle.
//!
//! Structural SSA/block-parameter well-formedness (defs dominate uses,
//! block-argument arity/type agreement, terminator discipline, linear
//! effect tokens), type checking over the closed op set, and FACT
//! consistency: facts are semantics (D2), so a module whose facts are
//! inconsistent is REJECTED with a diagnostic naming the fact and its
//! justification. There is no way to state an unverified aliasing
//! claim in safe-tier WIR — `noalias` only accepts checker-theorem
//! citations, and op-derived facts are re-derived locally.
//!
//! Verifier failures are compiler-internal diagnostics: deterministic
//! messages in canonical textual coordinates, with the offending
//! function dumped inline.

use crate::facts::{DerefSize, FactKind, Just, Theorem};
use crate::ir::{Aux, Block, BlockCall, FuncId, Function, Inst, Module, Value, ValueDef};
use crate::ops::Opcode;
use crate::print::{Canon, canonicalize, print_function, render_fact, render_inst, successors};
use crate::types::TypeData;
use std::collections::HashMap;

/// The rejection classes. One red test per class (s24 acceptance).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrClass {
    /// Missing entry block, empty layout, or a block outside it.
    Layout,
    /// Entry block parameters disagree with the function signature.
    EntrySig,
    /// A block without exactly one trailing terminator.
    Terminator,
    /// A reserved mnemonic (`region.*`, `rc.*`, `sync.*`, `eu.*`) used
    /// before its semantics land (s26/s27).
    ReservedOp,
    /// An instruction is ill-typed.
    Type,
    /// An integer constant that does not fit its type.
    ConstRange,
    /// A call site disagrees with its callee's signature (or callee
    /// signatures conflict across the module).
    CallSig,
    /// Branch argument count disagrees with the target's parameters.
    BlockArgArity,
    /// Branch argument types disagree with the target's parameters.
    BlockArgType,
    /// A block unreachable from the entry.
    Unreachable,
    /// A value use not dominated by its definition.
    Dominance,
    /// An effect token consumed more than once (the chain is a spine).
    TokenLinearity,
    /// A fact whose operands have the wrong types (or don't exist).
    FactType,
    /// A range fact that is empty or not implied by its deriving op.
    FactRange,
    /// A noalias fact that is trivially violated (same value twice).
    FactNoalias,
    /// A deref fact citing an op that is not an allocation.
    FactDeref,
    /// A justification tag that cannot justify this fact kind.
    FactJust,
    /// A pass dropped a fact on a still-live value without a justified
    /// invalidation (D2: passes may rely on facts, never drop them).
    DroppedFact,
}

impl ErrClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrClass::Layout => "layout",
            ErrClass::EntrySig => "entry-sig",
            ErrClass::Terminator => "terminator",
            ErrClass::ReservedOp => "reserved-op",
            ErrClass::Type => "type",
            ErrClass::ConstRange => "const-range",
            ErrClass::CallSig => "call-sig",
            ErrClass::BlockArgArity => "block-arg-arity",
            ErrClass::BlockArgType => "block-arg-type",
            ErrClass::Unreachable => "unreachable-block",
            ErrClass::Dominance => "dominance",
            ErrClass::TokenLinearity => "token-linearity",
            ErrClass::FactType => "fact-type",
            ErrClass::FactRange => "fact-range",
            ErrClass::FactNoalias => "fact-noalias",
            ErrClass::FactDeref => "fact-deref",
            ErrClass::FactJust => "fact-just",
            ErrClass::DroppedFact => "dropped-fact",
        }
    }
}

/// A deterministic verifier failure: class, message in canonical
/// textual coordinates, and the offending function dumped inline.
#[derive(Clone, Debug)]
pub struct VerifyError {
    pub class: ErrClass,
    pub func: String,
    pub msg: String,
    pub dump: String,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "wir verify error [{}] in @{}: {}",
            self.class.as_str(),
            self.func,
            self.msg
        )?;
        writeln!(f, "--- offending function ---")?;
        write!(f, "{}", self.dump)
    }
}

impl std::error::Error for VerifyError {}

type VResult = Result<(), VerifyError>;

struct Verifier<'a> {
    m: &'a Module,
    f: &'a Function,
    canon: Canon,
    /// inst -> (block, position within block).
    place: HashMap<Inst, (Block, usize)>,
}

impl<'a> Verifier<'a> {
    fn fail(&self, class: ErrClass, msg: impl Into<String>) -> VerifyError {
        VerifyError {
            class,
            func: self.f.name.clone(),
            msg: msg.into(),
            dump: print_function(self.m, self.f),
        }
    }

    fn at_inst(&self, inst: Inst) -> String {
        let block = self
            .place
            .get(&inst)
            .map(|&(b, _)| self.canon.block(b))
            .unwrap_or_else(|| "b?".into());
        format!(
            "{block}: `{}`",
            render_inst(self.m, self.f, &self.canon, inst)
        )
    }

    fn ty(&self, v: Value) -> String {
        self.m.types.display(self.f.value_ty(v))
    }

    // ---- structure -----------------------------------------------------

    fn check_layout(&self) -> VResult {
        if self.f.layout.is_empty() {
            return Err(self.fail(ErrClass::Layout, "function has no blocks"));
        }
        let mut seen = std::collections::HashSet::new();
        for &b in &self.f.layout {
            if !self.f.blocks.contains(b) {
                return Err(self.fail(ErrClass::Layout, "layout names a block that does not exist"));
            }
            if !seen.insert(b) {
                return Err(self.fail(
                    ErrClass::Layout,
                    format!("block {} appears twice in the layout", self.canon.block(b)),
                ));
            }
        }
        for b in self.f.blocks.keys() {
            if !seen.contains(&b) {
                return Err(self.fail(
                    ErrClass::Layout,
                    format!("block {} is not in the layout", self.canon.block(b)),
                ));
            }
        }
        Ok(())
    }

    fn check_entry_sig(&self) -> VResult {
        let sig = &self.m.sigs[self.f.sig];
        for &t in &sig.results {
            if self.m.types.is_token(t) {
                return Err(self.fail(
                    ErrClass::Type,
                    "signature results may not be effect tokens (calls mint their own successor tokens)",
                ));
            }
        }
        let entry = self.f.entry().expect("layout checked");
        let params = self.f.vpool.get(self.f.blocks[entry].params);
        if params.len() != sig.params.len() {
            return Err(self.fail(
                ErrClass::EntrySig,
                format!(
                    "entry block has {} parameter(s), signature has {}",
                    params.len(),
                    sig.params.len()
                ),
            ));
        }
        for (i, (&v, p)) in params.iter().zip(&sig.params).enumerate() {
            if self.f.value_ty(v) != p.ty {
                return Err(self.fail(
                    ErrClass::EntrySig,
                    format!(
                        "entry parameter {i} has type {}, signature says {}",
                        self.ty(v),
                        self.m.types.display(p.ty)
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_terminators(&self) -> VResult {
        for &b in &self.f.layout {
            let insts = &self.f.blocks[b].insts;
            let Some((&last, rest)) = insts.split_last() else {
                return Err(self.fail(
                    ErrClass::Terminator,
                    format!(
                        "block {} is empty (needs a terminator)",
                        self.canon.block(b)
                    ),
                ));
            };
            if !self.f.insts[last].op.is_terminator() {
                return Err(self.fail(
                    ErrClass::Terminator,
                    format!(
                        "block {} does not end with a terminator ({})",
                        self.canon.block(b),
                        self.at_inst(last)
                    ),
                ));
            }
            for &i in rest {
                if self.f.insts[i].op.is_terminator() {
                    return Err(self.fail(
                        ErrClass::Terminator,
                        format!("terminator before the end of a block: {}", self.at_inst(i)),
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_reserved(&self) -> VResult {
        for &b in &self.f.layout {
            for &inst in &self.f.blocks[b].insts {
                let op = self.f.insts[inst].op;
                if op.is_reserved() {
                    return Err(self.fail(
                        ErrClass::ReservedOp,
                        format!(
                            "`{}` is reserved until its semantics land (s26/s27): {}",
                            op.base_mnemonic(),
                            self.at_inst(inst)
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_reachability(&self) -> VResult {
        // canonicalize() puts reachable blocks first, in RPO; anything
        // it appended past the reachable set is an error.
        let mut reachable = std::collections::HashSet::new();
        if let Some(entry) = self.f.entry() {
            let mut stack = vec![entry];
            reachable.insert(entry);
            while let Some(b) = stack.pop() {
                for s in successors(self.f, b) {
                    if self.f.blocks.contains(s) && reachable.insert(s) {
                        stack.push(s);
                    }
                }
            }
        }
        for &b in &self.f.layout {
            if !reachable.contains(&b) {
                return Err(self.fail(
                    ErrClass::Unreachable,
                    format!(
                        "block {} is unreachable from the entry",
                        self.canon.block(b)
                    ),
                ));
            }
        }
        Ok(())
    }

    // ---- types ---------------------------------------------------------

    fn results(&self, inst: Inst) -> Vec<Value> {
        self.f.vpool.get(self.f.insts[inst].results)
    }

    fn args(&self, inst: Inst) -> Vec<Value> {
        self.f.vpool.get(self.f.insts[inst].args)
    }

    fn type_err(&self, inst: Inst, msg: &str) -> VerifyError {
        self.fail(ErrClass::Type, format!("{}: {msg}", self.at_inst(inst)))
    }

    fn expect_counts(&self, inst: Inst, nargs: usize, nresults: usize) -> VResult {
        let a = self.args(inst).len();
        let r = self.results(inst).len();
        if a != nargs {
            return Err(self.type_err(inst, &format!("expected {nargs} operand(s), found {a}")));
        }
        if r != nresults {
            return Err(self.type_err(inst, &format!("expected {nresults} result(s), found {r}")));
        }
        Ok(())
    }

    fn check_edge(&self, inst: Inst, edge: BlockCall) -> VResult {
        if !self.f.blocks.contains(edge.block) {
            return Err(self.fail(
                ErrClass::Layout,
                format!(
                    "{}: branch to a block that does not exist",
                    self.at_inst(inst)
                ),
            ));
        }
        let params = self.f.vpool.get(self.f.blocks[edge.block].params);
        let args = self.f.vpool.get(edge.args);
        if params.len() != args.len() {
            return Err(self.fail(
                ErrClass::BlockArgArity,
                format!(
                    "{}: {} expects {} argument(s), {} passed",
                    self.at_inst(inst),
                    self.canon.block(edge.block),
                    params.len(),
                    args.len()
                ),
            ));
        }
        for (i, (&p, &a)) in params.iter().zip(&args).enumerate() {
            if self.f.value_ty(p) != self.f.value_ty(a) {
                return Err(self.fail(
                    ErrClass::BlockArgType,
                    format!(
                        "{}: argument {i} to {} has type {}, parameter wants {}",
                        self.at_inst(inst),
                        self.canon.block(edge.block),
                        self.ty(a),
                        self.ty(p)
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_inst_types(&self, inst: Inst) -> VResult {
        let data = &self.f.insts[inst];
        let args = self.args(inst);
        let results = self.results(inst);
        let types = &self.m.types;
        let int = |v: Value| types.is_int(self.f.value_ty(v));
        let float = |v: Value| types.is_float(self.f.value_ty(v));
        match data.op {
            Opcode::Iconst => {
                self.expect_counts(inst, 0, 1)?;
                let ty = self.f.value_ty(results[0]);
                let Some((lo, hi)) = types.int_bounds(ty) else {
                    return Err(self.type_err(inst, "iconst result must be an integer type"));
                };
                let Aux::Int(v) = data.aux else {
                    return Err(self.type_err(inst, "iconst without an integer payload"));
                };
                if (v as i128) < lo || (v as i128) > hi {
                    return Err(self.fail(
                        ErrClass::ConstRange,
                        format!(
                            "{}: constant {v} does not fit {}",
                            self.at_inst(inst),
                            types.display(ty)
                        ),
                    ));
                }
            }
            Opcode::Fconst => {
                self.expect_counts(inst, 0, 1)?;
                let ty = self.f.value_ty(results[0]);
                if !types.is_float(ty) {
                    return Err(self.type_err(inst, "fconst result must be a float type"));
                }
                if let (TypeData::F32, Aux::FloatBits(bits)) = (types.get(ty), data.aux)
                    && bits >> 32 != 0
                {
                    return Err(self.type_err(inst, "f32 constant carries more than 32 bits"));
                }
            }
            Opcode::Bconst => {
                self.expect_counts(inst, 0, 1)?;
                if self.f.value_ty(results[0]) != crate::types::BOOL {
                    return Err(self.type_err(inst, "bconst result must be bool"));
                }
            }
            Opcode::IaddChk
            | Opcode::IsubChk
            | Opcode::ImulChk
            | Opcode::IdivChk
            | Opcode::IremChk
            | Opcode::IaddWrap
            | Opcode::IsubWrap
            | Opcode::ImulWrap
            | Opcode::IaddSat
            | Opcode::IsubSat
            | Opcode::ImulSat
            | Opcode::Band
            | Opcode::Bor
            | Opcode::Bxor
            | Opcode::Shl
            | Opcode::Lshr
            | Opcode::Ashr => {
                self.expect_counts(inst, 2, 1)?;
                if !int(args[0]) || !int(args[1]) {
                    return Err(self.type_err(inst, "integer op needs integer operands"));
                }
                self.same_tys(inst, &[args[0], args[1], results[0]])?;
            }
            Opcode::Fadd | Opcode::Fsub | Opcode::Fmul | Opcode::Fdiv => {
                self.expect_counts(inst, 2, 1)?;
                if !float(args[0]) || !float(args[1]) {
                    return Err(self.type_err(inst, "float op needs float operands"));
                }
                self.same_tys(inst, &[args[0], args[1], results[0]])?;
            }
            Opcode::Fneg => {
                self.expect_counts(inst, 1, 1)?;
                if !float(args[0]) {
                    return Err(self.type_err(inst, "float op needs float operands"));
                }
                self.same_tys(inst, &[args[0], results[0]])?;
            }
            Opcode::Fma => {
                self.expect_counts(inst, 3, 1)?;
                if !float(args[0]) {
                    return Err(self.type_err(inst, "float op needs float operands"));
                }
                self.same_tys(inst, &[args[0], args[1], args[2], results[0]])?;
            }
            Opcode::Icmp => {
                self.expect_counts(inst, 2, 1)?;
                if !int(args[0]) || !int(args[1]) {
                    return Err(self.type_err(inst, "icmp needs integer operands"));
                }
                if self.f.value_ty(args[0]) != self.f.value_ty(args[1]) {
                    return Err(self.type_err(inst, "icmp operands must share a type"));
                }
                if self.f.value_ty(results[0]) != crate::types::BOOL {
                    return Err(self.type_err(inst, "compare results are bool"));
                }
            }
            Opcode::Fcmp => {
                self.expect_counts(inst, 2, 1)?;
                if !float(args[0]) || !float(args[1]) {
                    return Err(self.type_err(inst, "fcmp needs float operands"));
                }
                if self.f.value_ty(args[0]) != self.f.value_ty(args[1]) {
                    return Err(self.type_err(inst, "fcmp operands must share a type"));
                }
                if self.f.value_ty(results[0]) != crate::types::BOOL {
                    return Err(self.type_err(inst, "compare results are bool"));
                }
            }
            Opcode::Sext | Opcode::Zext | Opcode::Itrunc => {
                self.expect_counts(inst, 1, 1)?;
                let (Some(from), Some(to)) = (
                    types.int_bits(self.f.value_ty(args[0])),
                    types.int_bits(self.f.value_ty(results[0])),
                ) else {
                    return Err(self.type_err(inst, "integer conversion needs integer types"));
                };
                let ok = if data.op == Opcode::Itrunc {
                    to < from
                } else {
                    to > from
                };
                if !ok {
                    return Err(
                        self.type_err(inst, "conversion must change width in the right direction")
                    );
                }
            }
            Opcode::PtrOff => {
                self.expect_counts(inst, 2, 1)?;
                if self.f.value_ty(args[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "ptr.off base must be ptr"));
                }
                if self.f.value_ty(args[1]) != crate::types::I64 {
                    return Err(self.type_err(inst, "ptr.off index must be i64"));
                }
                if self.f.value_ty(results[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "ptr.off result must be ptr"));
                }
                if !matches!(data.aux, Aux::Scale(s) if s > 0) {
                    return Err(self.type_err(inst, "ptr.off scale must be a positive constant"));
                }
            }
            Opcode::Load => {
                self.expect_counts(inst, 2, 1)?;
                if self.f.value_ty(args[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "load address must be ptr"));
                }
                if !matches!(types.get(self.f.value_ty(args[1])), TypeData::Mem(_)) {
                    return Err(self.type_err(inst, "load needs a mem token operand"));
                }
                let rty = self.f.value_ty(results[0]);
                if types.is_token(rty) || matches!(types.get(rty), TypeData::Agg(_)) {
                    return Err(self.type_err(inst, "load result must be a scalar"));
                }
            }
            Opcode::Store => {
                self.expect_counts(inst, 3, 1)?;
                let vty = self.f.value_ty(args[0]);
                if types.is_token(vty) || matches!(types.get(vty), TypeData::Agg(_)) {
                    return Err(self.type_err(inst, "store value must be a scalar"));
                }
                if self.f.value_ty(args[1]) != crate::types::PTR {
                    return Err(self.type_err(inst, "store address must be ptr"));
                }
                if !matches!(types.get(self.f.value_ty(args[2])), TypeData::Mem(_)) {
                    return Err(self.type_err(inst, "store needs a mem token operand"));
                }
                if self.f.value_ty(results[0]) != self.f.value_ty(args[2]) {
                    return Err(
                        self.type_err(inst, "store result must be the successor of its mem token")
                    );
                }
            }
            Opcode::AggMake => {
                if results.len() != 1 {
                    return Err(self.type_err(inst, "agg.make has one result"));
                }
                let TypeData::Agg(fields) = types.get(self.f.value_ty(results[0])) else {
                    return Err(self.type_err(inst, "agg.make result must be an aggregate"));
                };
                if fields.len() != args.len() {
                    return Err(self.type_err(inst, "agg.make field count mismatch"));
                }
                for (&fty, &a) in fields.iter().zip(&args) {
                    if fty != self.f.value_ty(a) {
                        return Err(self.type_err(inst, "agg.make field type mismatch"));
                    }
                }
            }
            Opcode::AggGet => {
                self.expect_counts(inst, 1, 1)?;
                let TypeData::Agg(fields) = types.get(self.f.value_ty(args[0])) else {
                    return Err(self.type_err(inst, "agg.get needs an aggregate operand"));
                };
                let Aux::Int(k) = data.aux else {
                    return Err(self.type_err(inst, "agg.get needs a field index"));
                };
                let Some(&fty) = usize::try_from(k).ok().and_then(|k| fields.get(k)) else {
                    return Err(self.type_err(inst, "agg.get field index out of range"));
                };
                if fty != self.f.value_ty(results[0]) {
                    return Err(self.type_err(inst, "agg.get result type mismatch"));
                }
            }
            Opcode::Call => {
                let Aux::Callee(ef) = data.aux else {
                    return Err(self.type_err(inst, "call without a callee"));
                };
                if !self.f.ext_funcs.contains(ef) {
                    return Err(self.type_err(inst, "call to an unknown callee"));
                }
                let sig = &self.m.sigs[self.f.ext_funcs[ef].sig];
                let name = &self.f.ext_funcs[ef].name;
                let call_err = |msg: String| {
                    self.fail(ErrClass::CallSig, format!("{}: {msg}", self.at_inst(inst)))
                };
                for &t in &sig.results {
                    if self.m.types.is_token(t) {
                        return Err(call_err(format!(
                            "`@{name}` declares token results; calls mint their own successor tokens"
                        )));
                    }
                }
                if args.len() != sig.params.len() {
                    return Err(call_err(format!(
                        "`@{name}` takes {} argument(s), {} passed",
                        sig.params.len(),
                        args.len()
                    )));
                }
                for (i, (p, &a)) in sig.params.iter().zip(&args).enumerate() {
                    if p.ty != self.f.value_ty(a) {
                        return Err(call_err(format!(
                            "argument {i} has type {}, `@{name}` wants {}",
                            self.ty(a),
                            self.m.types.display(p.ty)
                        )));
                    }
                }
                let mut want: Vec<crate::types::TypeId> = sig.results.clone();
                for p in &sig.params {
                    if self.m.types.is_token(p.ty) {
                        want.push(p.ty);
                    }
                }
                if results.len() != want.len() {
                    return Err(call_err(format!(
                        "`@{name}` produces {} result(s) (declared results + successor tokens), {} bound",
                        want.len(),
                        results.len()
                    )));
                }
                for (i, (&w, &r)) in want.iter().zip(&results).enumerate() {
                    if w != self.f.value_ty(r) {
                        return Err(call_err(format!(
                            "result {i} has type {}, `@{name}` produces {}",
                            self.ty(r),
                            self.m.types.display(w)
                        )));
                    }
                }
            }
            Opcode::Jmp => {
                self.expect_counts(inst, 0, 0)?;
                let Aux::Jump(edge) = data.aux else {
                    return Err(self.type_err(inst, "jmp without a target"));
                };
                self.check_edge(inst, edge)?;
            }
            Opcode::Br => {
                self.expect_counts(inst, 1, 0)?;
                if self.f.value_ty(args[0]) != crate::types::BOOL {
                    return Err(self.type_err(inst, "br condition must be bool"));
                }
                let Aux::Br(t, e) = data.aux else {
                    return Err(self.type_err(inst, "br without targets"));
                };
                self.check_edge(inst, t)?;
                self.check_edge(inst, e)?;
            }
            Opcode::Ret => {
                let sig = &self.m.sigs[self.f.sig];
                if args.len() != sig.results.len() {
                    return Err(self.type_err(
                        inst,
                        &format!(
                            "ret carries {} value(s), signature returns {}",
                            args.len(),
                            sig.results.len()
                        ),
                    ));
                }
                for (i, (&want, &a)) in sig.results.iter().zip(&args).enumerate() {
                    if want != self.f.value_ty(a) {
                        return Err(self.type_err(
                            inst,
                            &format!(
                                "ret value {i} has type {}, signature returns {}",
                                self.ty(a),
                                self.m.types.display(want)
                            ),
                        ));
                    }
                }
            }
            Opcode::Trap => {
                self.expect_counts(inst, 0, 0)?;
            }
            _ => {
                debug_assert!(data.op.is_reserved(), "unhandled opcode in type check");
            }
        }
        Ok(())
    }

    fn same_tys(&self, inst: Inst, vals: &[Value]) -> VResult {
        let first = self.f.value_ty(vals[0]);
        for &v in &vals[1..] {
            if self.f.value_ty(v) != first {
                return Err(self.type_err(inst, "operand/result types must agree"));
            }
        }
        Ok(())
    }

    // ---- dominance -----------------------------------------------------

    fn check_dominance(&self) -> VResult {
        // Reachability already verified: every block is reachable, and
        // canon.blocks is an RPO.
        let rpo = &self.canon.blocks;
        let mut rpo_num: HashMap<Block, usize> = HashMap::new();
        for (i, &b) in rpo.iter().enumerate() {
            rpo_num.insert(b, i);
        }
        let mut preds: HashMap<Block, Vec<Block>> = HashMap::new();
        for &b in rpo {
            for s in successors(self.f, b) {
                preds.entry(s).or_default().push(b);
            }
        }
        let entry = self.f.entry().expect("layout checked");
        // Cooper–Harvey–Kennedy iterative idoms over RPO numbers.
        let mut idom: HashMap<Block, Block> = HashMap::new();
        idom.insert(entry, entry);
        let mut changed = true;
        while changed {
            changed = false;
            for &b in rpo.iter().skip(1) {
                let plist = preds.get(&b).cloned().unwrap_or_default();
                let mut new_idom: Option<Block> = None;
                for &p in &plist {
                    if !idom.contains_key(&p) {
                        continue;
                    }
                    new_idom = Some(match new_idom {
                        None => p,
                        Some(cur) => intersect(&idom, &rpo_num, cur, p),
                    });
                }
                if let Some(ni) = new_idom
                    && idom.get(&b) != Some(&ni)
                {
                    idom.insert(b, ni);
                    changed = true;
                }
            }
        }
        let dominates = |a: Block, b: Block| -> bool {
            let mut cur = b;
            loop {
                if cur == a {
                    return true;
                }
                let up = *idom.get(&cur).expect("reachable");
                if up == cur {
                    return false;
                }
                cur = up;
            }
        };
        // Check every use.
        for &b in rpo {
            for (pos, &inst) in self.f.blocks[b].insts.iter().enumerate() {
                let mut uses = self.args(inst);
                match self.f.insts[inst].aux {
                    Aux::Jump(e) => uses.extend(self.f.vpool.get(e.args)),
                    Aux::Br(t, e) => {
                        uses.extend(self.f.vpool.get(t.args));
                        uses.extend(self.f.vpool.get(e.args));
                    }
                    _ => {}
                }
                for v in uses {
                    let ok = match self.f.values[v].def {
                        ValueDef::Param(db, _) => dominates(db, b),
                        ValueDef::Result(di, _) => match self.place.get(&di) {
                            Some(&(db, dk)) => {
                                if db == b {
                                    dk < pos
                                } else {
                                    dominates(db, b)
                                }
                            }
                            None => false,
                        },
                    };
                    if !ok {
                        return Err(self.fail(
                            ErrClass::Dominance,
                            format!(
                                "{}: {} is not dominated by its definition",
                                self.at_inst(inst),
                                self.canon.value(v)
                            ),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    // ---- token linearity ----------------------------------------------

    fn check_tokens(&self) -> VResult {
        // Consuming positions: store's token operand, call's token
        // arguments, and token values passed along branch edges. Loads
        // READ their token — the chain stays a spine, not a DAG.
        let mut consumed: HashMap<Value, Inst> = HashMap::new();
        let mut consume = |v: Value, by: Inst| -> Option<Inst> {
            if !self.m.types.is_token(self.f.value_ty(v)) {
                return None;
            }
            consumed.insert(v, by)
        };
        for &b in &self.f.layout {
            for &inst in &self.f.blocks[b].insts {
                let data = &self.f.insts[inst];
                let args = self.f.vpool.get(data.args);
                let mut clash = None;
                match data.op {
                    Opcode::Store => {
                        if let Some(&tok) = args.get(2) {
                            clash = consume(tok, inst);
                        }
                    }
                    Opcode::Call => {
                        for &a in &args {
                            clash = clash.or(consume(a, inst));
                        }
                    }
                    _ => match data.aux {
                        Aux::Jump(e) => {
                            for a in self.f.vpool.get(e.args) {
                                clash = clash.or(consume(a, inst));
                            }
                        }
                        Aux::Br(t, e) => {
                            for a in self
                                .f
                                .vpool
                                .get(t.args)
                                .into_iter()
                                .chain(self.f.vpool.get(e.args))
                            {
                                clash = clash.or(consume(a, inst));
                            }
                        }
                        _ => {}
                    },
                }
                if let Some(first) = clash {
                    return Err(self.fail(
                        ErrClass::TokenLinearity,
                        format!(
                            "effect token consumed twice: first by {}, again by {}",
                            self.at_inst(first),
                            self.at_inst(inst)
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    // ---- facts ---------------------------------------------------------

    fn fact_fail(&self, class: ErrClass, fact: crate::facts::FactId, msg: &str) -> VerifyError {
        let rendered = render_fact(&self.canon, &self.f.facts[fact]);
        self.fail(class, format!("`{rendered}`: {msg}"))
    }

    fn check_facts(&self) -> VResult {
        for (id, fd) in self.f.facts.iter() {
            // Operands must be real values.
            let mut operands = vec![fd.kind.subject()];
            if let FactKind::Noalias(_, b) = fd.kind {
                operands.push(b);
            }
            if let FactKind::Deref(_, DerefSize::Scaled { count, .. }) = fd.kind {
                operands.push(count);
            }
            if let Just::Op(v) = fd.just {
                operands.push(v);
            }
            for v in operands {
                if !self.f.values.contains(v) {
                    return Err(self.fact_fail(
                        ErrClass::FactType,
                        id,
                        "fact references a value that does not exist",
                    ));
                }
            }
            let is_ptr = |v: Value| self.f.value_ty(v) == crate::types::PTR;
            match fd.kind {
                FactKind::Noalias(a, b) => {
                    if !is_ptr(a) || !is_ptr(b) {
                        return Err(self.fact_fail(
                            ErrClass::FactType,
                            id,
                            "noalias operands must be pointers",
                        ));
                    }
                    if a == b {
                        return Err(self.fact_fail(
                            ErrClass::FactNoalias,
                            id,
                            "a pointer always aliases itself",
                        ));
                    }
                    match fd.just {
                        Just::Theorem(Theorem::ExclMut | Theorem::FrozenRead) => {}
                        _ => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "noalias must cite a checker theorem — there is no way to state an unverified aliasing claim in safe-tier WIR",
                            ));
                        }
                    }
                }
                FactKind::Deref(v, size) => {
                    if !is_ptr(v) {
                        return Err(self.fact_fail(
                            ErrClass::FactType,
                            id,
                            "deref subject must be a pointer",
                        ));
                    }
                    if let DerefSize::Scaled { count, .. } = size
                        && !self.m.types.is_int(self.f.value_ty(count))
                    {
                        return Err(self.fact_fail(
                            ErrClass::FactType,
                            id,
                            "deref count must be an integer value",
                        ));
                    }
                    match fd.just {
                        Just::Theorem(_) => {}
                        Just::DefOp | Just::Op(_) => {
                            let cited = match fd.just {
                                Just::Op(c) => c,
                                _ => v,
                            };
                            let ValueDef::Result(di, _) = self.f.values[cited].def else {
                                return Err(self.fact_fail(
                                    ErrClass::FactDeref,
                                    id,
                                    "deref fact cites a block parameter, not an allocation op",
                                ));
                            };
                            if self.f.insts[di].op != Opcode::RegionAlloc {
                                return Err(self.fact_fail(
                                    ErrClass::FactDeref,
                                    id,
                                    "deref fact cites a non-allocation op (size checks against `region.alloc` land in s26)",
                                ));
                            }
                        }
                    }
                }
                FactKind::Range(v, lo, hi) => {
                    let Some((tlo, thi)) = self.m.types.int_bounds(self.f.value_ty(v)) else {
                        return Err(self.fact_fail(
                            ErrClass::FactType,
                            id,
                            "range subject must be an integer value",
                        ));
                    };
                    if lo > hi {
                        return Err(self.fact_fail(
                            ErrClass::FactRange,
                            id,
                            "empty range (lo > hi) is unsatisfiable",
                        ));
                    }
                    match fd.just {
                        Just::Theorem(_) => {}
                        Just::Op(_) => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "range facts cite their own defining op (`: op`) or a theorem",
                            ));
                        }
                        Just::DefOp => {
                            // Re-derive the output range from the def.
                            let ValueDef::Result(di, _) = self.f.values[v].def else {
                                return Err(self.fact_fail(
                                    ErrClass::FactJust,
                                    id,
                                    "op-justified range on a block parameter (cite a checker theorem instead)",
                                ));
                            };
                            let (dlo, dhi) = match (self.f.insts[di].op, self.f.insts[di].aux) {
                                (Opcode::Iconst, Aux::Int(c)) => (c as i128, c as i128),
                                // Checked ops cannot wrap, so their
                                // results stay within type bounds —
                                // that is all that is derivable
                                // locally without operand ranges.
                                _ => (tlo, thi),
                            };
                            if dlo < lo || dhi > hi {
                                return Err(self.fact_fail(
                                    ErrClass::FactRange,
                                    id,
                                    "range is not implied by the defining op",
                                ));
                            }
                        }
                    }
                }
                FactKind::Region(v, _) => {
                    if !is_ptr(v) {
                        return Err(self.fact_fail(
                            ErrClass::FactType,
                            id,
                            "region subject must be a pointer",
                        ));
                    }
                    match fd.just {
                        Just::Theorem(Theorem::RegionAlloc) => {}
                        _ => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "region facts cite the region.alloc theorem (op-derived forms land in s26)",
                            ));
                        }
                    }
                }
                FactKind::Frozen(v) => {
                    if !is_ptr(v) {
                        return Err(self.fact_fail(
                            ErrClass::FactType,
                            id,
                            "frozen subject must be a pointer",
                        ));
                    }
                    match fd.just {
                        Just::Theorem(Theorem::FrozenRead) => {}
                        _ => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "frozen facts cite the frozen.read theorem",
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn intersect(
    idom: &HashMap<Block, Block>,
    rpo_num: &HashMap<Block, usize>,
    mut a: Block,
    mut b: Block,
) -> Block {
    while a != b {
        while rpo_num[&a] > rpo_num[&b] {
            a = idom[&a];
        }
        while rpo_num[&b] > rpo_num[&a] {
            b = idom[&b];
        }
    }
    a
}

/// Verify one function against its module's types and signatures.
pub fn verify_function(m: &Module, f: &Function) -> VResult {
    let canon = canonicalize(f);
    let mut place: HashMap<Inst, (Block, usize)> = HashMap::new();
    for b in f.blocks.keys() {
        for (k, &inst) in f.blocks[b].insts.iter().enumerate() {
            place.insert(inst, (b, k));
        }
    }
    let v = Verifier { m, f, canon, place };
    v.check_layout()?;
    v.check_entry_sig()?;
    v.check_terminators()?;
    v.check_reserved()?;
    v.check_reachability()?;
    for &b in &f.layout {
        for &inst in &f.blocks[b].insts {
            v.check_inst_types(inst)?;
        }
    }
    v.check_dominance()?;
    v.check_tokens()?;
    v.check_facts()?;
    Ok(())
}

/// Verify a whole module: every function, plus cross-function callee
/// signature consistency.
pub fn verify_module(m: &Module) -> VResult {
    // A callee name must mean one signature everywhere.
    let mut seen: HashMap<&str, &crate::ir::SigData> = HashMap::new();
    let mut sigs_by_name: Vec<(String, crate::ir::SigData)> = Vec::new();
    for (name, sig) in &m.decls {
        sigs_by_name.push((name.clone(), m.sigs[*sig].clone()));
    }
    for f in m.funcs.values() {
        sigs_by_name.push((f.name.clone(), m.sigs[f.sig].clone()));
        for ef in f.ext_funcs.values() {
            sigs_by_name.push((ef.name.clone(), m.sigs[ef.sig].clone()));
        }
    }
    for (name, sig) in &sigs_by_name {
        match seen.get(name.as_str()) {
            None => {
                seen.insert(name.as_str(), sig);
            }
            Some(prev) if *prev == sig => {}
            Some(_) => {
                return Err(VerifyError {
                    class: ErrClass::CallSig,
                    func: name.clone(),
                    msg: format!("`@{name}` is used with two different signatures"),
                    dump: String::new(),
                });
            }
        }
    }
    for f in m.funcs.values() {
        verify_function(m, f)?;
    }
    Ok(())
}

// ------------------------------------------------ pass-manager stub -----

/// Why a pass may legitimately lose a fact. Anything else is a D2
/// violation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Invalidation {
    /// The fact's subject value no longer exists.
    ValueDeleted,
    /// The region the fact talked about has been freed.
    RegionFreed,
}

/// The context a pass runs in. Losing a fact requires an explicit,
/// justified invalidation recorded here.
#[derive(Default)]
pub struct PassCtx {
    invalidated: Vec<(crate::facts::FactId, Invalidation)>,
}

impl PassCtx {
    pub fn invalidate(&mut self, fact: crate::facts::FactId, why: Invalidation) {
        self.invalidated.push((fact, why));
    }
}

/// The s24 pass-manager contract stub (real manager in s42): snapshot
/// the fact table, run the pass, and REJECT if any pre-existing fact
/// was lost or changed on a still-live value without a justified
/// invalidation — then re-verify the whole function. This is the
/// mechanized "passes may rely on facts and may not drop them", and
/// `--verify-each-pass` keeps it on in debug compilers and all CI runs.
pub fn run_pass(
    m: &mut Module,
    func: FuncId,
    pass_name: &str,
    pass: impl FnOnce(&mut Function, &mut PassCtx),
) -> VResult {
    let before: Vec<(crate::facts::FactId, crate::facts::FactData)> = m.funcs[func]
        .facts
        .iter()
        .map(|(id, fd)| (id, *fd))
        .collect();
    let mut ctx = PassCtx::default();
    pass(&mut m.funcs[func], &mut ctx);
    let f = &m.funcs[func];
    for (id, old) in before {
        let still_there = f.facts.get(id).is_some_and(|now| *now == old);
        if still_there {
            continue;
        }
        let justified = ctx.invalidated.iter().find(|(fid, _)| *fid == id);
        let Some(&(_, why)) = justified else {
            return Err(VerifyError {
                class: ErrClass::DroppedFact,
                func: f.name.clone(),
                msg: format!(
                    "pass `{pass_name}` dropped fact {id} ({}) without a justified invalidation — facts are semantics, not metadata (D2)",
                    old.kind.keyword()
                ),
                dump: print_function(m, f),
            });
        };
        if why == Invalidation::ValueDeleted && f.values.contains(old.kind.subject()) {
            return Err(VerifyError {
                class: ErrClass::DroppedFact,
                func: f.name.clone(),
                msg: format!(
                    "pass `{pass_name}` invalidated fact {id} as value-deleted, but its subject value still exists"
                ),
                dump: print_function(m, f),
            });
        }
    }
    verify_function(m, &m.funcs[func])
}
