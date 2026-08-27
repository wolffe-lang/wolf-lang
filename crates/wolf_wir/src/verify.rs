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

use crate::entity::EntityRef;
use crate::facts::{DerefSize, FactKind, Just, Theorem};
use crate::ir::{Aux, Block, BlockCall, FuncId, Function, Inst, Module, Value, ValueDef};
use crate::ops::{ForeignRole, IntCc, Opcode};
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
    /// A reserved mnemonic (`sync.transfer`) used before its
    /// semantics land (c05 concurrency).
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
    /// An effect token consumed more than once ON ONE PATH (the chain
    /// is a spine per execution; s27 defer duplication legitimately
    /// consumes one value on mutually exclusive edges).
    TokenLinearity,
    /// An effect token read (by a load) at a point reachable after its
    /// consumption — use-after-free and stale-token reads are
    /// STRUCTURAL errors (s26: temporal safety by linearity).
    TokenOrder,
    /// A region with two token roots (entry param vs `region.new` /
    /// `stack.alloc`), or a minted region colliding with an existing
    /// one.
    RegionRoot,
    /// A frozen token (from `sync.freeze`) in a consuming position:
    /// stores/frees through frozen data are unrepresentable.
    FrozenToken,
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
            ErrClass::TokenOrder => "token-order",
            ErrClass::RegionRoot => "region-root",
            ErrClass::FrozenToken => "frozen-token",
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
                            "`{}` is reserved until its semantics land (c05): {}",
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
            | Opcode::UaddChk
            | Opcode::UsubChk
            | Opcode::UmulChk
            | Opcode::UdivChk
            | Opcode::UremChk
            | Opcode::Shl
            | Opcode::Lshr
            | Opcode::Ashr => {
                self.expect_counts(inst, 2, 1)?;
                if !int(args[0]) || !int(args[1]) {
                    return Err(self.type_err(inst, "integer op needs integer operands"));
                }
                self.same_tys(inst, &[args[0], args[1], results[0]])?;
            }
            Opcode::Band | Opcode::Bor | Opcode::Bxor => {
                // s104: bool operands are admitted — a bool is an
                // i8-shaped flag (the icmp equality precedent), and
                // and/or/xor on flags are exactly as meaningful as on
                // i8. The overlap guard's disjunction is `bor` of two
                // compare results. Mixed bool/int stays refused by
                // `same_tys`.
                self.expect_counts(inst, 2, 1)?;
                let bool_op = |v: Value| self.f.value_ty(v) == crate::types::BOOL;
                let both_bool = bool_op(args[0]) && bool_op(args[1]);
                if !both_bool && (!int(args[0]) || !int(args[1])) {
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
                // s88 (wolf-lang#100): `bool` operands are admitted for
                // the EQUALITY conditions only. A `bool` is an i8-shaped
                // flag with two inhabitants, so `eq`/`ne` are exactly as
                // meaningful as on `i8`; an ORDER on it would be a claim
                // the surface never makes (`<` on `bool` does not
                // typecheck), so the verifier refuses to represent one.
                let bool_op = |v: Value| self.f.value_ty(v) == crate::types::BOOL;
                let equality = matches!(data.aux, Aux::IntCc(IntCc::Eq | IntCc::Ne));
                let both_bool = bool_op(args[0]) && bool_op(args[1]);
                if both_bool && !equality {
                    return Err(self.type_err(inst, "icmp on bool operands must be `eq` or `ne`"));
                }
                // s104: pointer operands are admitted for EQUALITY and
                // the UNSIGNED orders — an address ordered as unsigned
                // is exactly the overlap-guard comparison, while a
                // signed order on an address would be a claim about a
                // sign bit no pointer has (the bool rule's own style).
                let ptr_op = |v: Value| self.f.value_ty(v) == crate::types::PTR;
                let both_ptr = ptr_op(args[0]) && ptr_op(args[1]);
                if both_ptr
                    && !matches!(
                        data.aux,
                        Aux::IntCc(
                            IntCc::Eq
                                | IntCc::Ne
                                | IntCc::Ult
                                | IntCc::Ule
                                | IntCc::Ugt
                                | IntCc::Uge
                        )
                    )
                {
                    return Err(
                        self.type_err(inst, "icmp on ptr operands must be unsigned or equality")
                    );
                }
                if !both_bool && !both_ptr && (!int(args[0]) || !int(args[1])) {
                    return Err(self.type_err(inst, "icmp needs integer or bool operands"));
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
            Opcode::Sitofp | Opcode::Uitofp => {
                // int → float (D54.4): integer source, float result.
                self.expect_counts(inst, 1, 1)?;
                if types.int_bits(self.f.value_ty(args[0])).is_none() {
                    return Err(self.type_err(inst, "int→float conversion needs an integer source"));
                }
                if !types.is_float(self.f.value_ty(results[0])) {
                    return Err(self.type_err(inst, "int→float conversion produces a float"));
                }
            }
            Opcode::FtosiChk | Opcode::FtouiChk => {
                // float → int (D54.4): float source, integer result; the
                // out-of-range/NaN trap is the backend's, not a typing
                // rule.
                self.expect_counts(inst, 1, 1)?;
                if !types.is_float(self.f.value_ty(args[0])) {
                    return Err(self.type_err(inst, "float→int conversion needs a float source"));
                }
                if types.int_bits(self.f.value_ty(results[0])).is_none() {
                    return Err(self.type_err(inst, "float→int conversion produces an integer"));
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
            Opcode::DataAddr => {
                self.expect_counts(inst, 0, 1)?;
                let Aux::Data(idx) = data.aux else {
                    return Err(self.type_err(inst, "data.addr without a data payload"));
                };
                if self.m.data.get(idx as usize).is_none() {
                    return Err(self.type_err(inst, "data.addr names a missing data declaration"));
                }
                if self.f.value_ty(results[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "data.addr result must be ptr"));
                }
            }
            Opcode::FuncAddr => {
                self.expect_counts(inst, 0, 1)?;
                let Aux::Callee(ef) = data.aux else {
                    return Err(self.type_err(inst, "func.addr without a callee"));
                };
                if !self.f.ext_funcs.contains(ef) {
                    return Err(self.type_err(inst, "func.addr of an unknown callee"));
                }
                if self.f.value_ty(results[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "func.addr result must be ptr"));
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
                // A callee sig's `mem.rF` params name FORMAL regions
                // (s26): each binds to the caller's actual region,
                // CONSISTENTLY — the substitution is checked, and the
                // successor results must come back under it.
                let mut subst: HashMap<u32, crate::types::RegionId> = HashMap::new();
                for (i, (p, &a)) in sig.params.iter().zip(&args).enumerate() {
                    let aty = self.f.value_ty(a);
                    match (self.m.types.get(p.ty), self.m.types.get(aty)) {
                        (TypeData::Mem(f), TypeData::Mem(actual)) => {
                            let f = f.as_u32();
                            let actual = *actual;
                            if let Some(&prev) = subst.get(&f)
                                && prev != actual
                            {
                                return Err(call_err(format!(
                                    "token argument {i} binds `@{name}`'s formal region r{f} to {actual}, but it is already bound to {prev}",
                                )));
                            }
                            subst.insert(f, actual);
                        }
                        _ if p.ty == aty => {}
                        _ => {
                            return Err(call_err(format!(
                                "argument {i} has type {}, `@{name}` wants {}",
                                self.ty(a),
                                self.m.types.display(p.ty)
                            )));
                        }
                    }
                }
                let mut want: Vec<crate::types::TypeId> = sig.results.clone();
                for p in &sig.params {
                    match self.m.types.get(p.ty) {
                        TypeData::Mem(f) => {
                            let actual = subst
                                .get(&f.as_u32())
                                .copied()
                                .expect("every token param was bound above");
                            // The actual token type exists — the bound
                            // argument carried it.
                            let aty = args
                                .iter()
                                .map(|&a| self.f.value_ty(a))
                                .find(|&t| {
                                    matches!(self.m.types.get(t), TypeData::Mem(r) if *r == actual)
                                })
                                .expect("bound region came from an argument");
                            want.push(aty);
                        }
                        TypeData::Io => want.push(p.ty),
                        _ => {}
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
            Opcode::CallInd => {
                // `call` minus the name (s97): operand 0 is the callee
                // ptr, the sig rides in Aux::Sig, and the sig is
                // BY-VALUE TOKEN-FREE — fn types carry no modes and no
                // region tokens, so there is no formal-region
                // substitution and no successor minting here at all.
                let Aux::Sig(sigid) = data.aux else {
                    return Err(self.type_err(inst, "call.ind without a signature"));
                };
                if !self.m.sigs.contains(sigid) {
                    return Err(self.type_err(inst, "call.ind names a missing signature"));
                }
                let sig = &self.m.sigs[sigid];
                let call_err = |msg: String| {
                    self.fail(ErrClass::CallSig, format!("{}: {msg}", self.at_inst(inst)))
                };
                let Some(&callee) = args.first() else {
                    return Err(call_err("call.ind needs a callee operand".to_string()));
                };
                if self.f.value_ty(callee) != crate::types::PTR {
                    return Err(call_err(format!(
                        "call.ind callee must be ptr, got {}",
                        self.ty(callee)
                    )));
                }
                for (i, p) in sig.params.iter().enumerate() {
                    if p.mode != crate::ir::Mode::Val {
                        return Err(call_err(format!(
                            "call.ind sigs are by-value (param {i} carries a mode; fn types carry none)"
                        )));
                    }
                    if self.m.types.is_token(p.ty) {
                        return Err(call_err(format!(
                            "call.ind sigs are token-free (param {i} is a token; fn types carry none)"
                        )));
                    }
                }
                for &t in &sig.results {
                    if self.m.types.is_token(t) {
                        return Err(call_err(
                            "call.ind sigs are token-free (a result is a token)".to_string(),
                        ));
                    }
                }
                let cargs = &args[1..];
                if cargs.len() != sig.params.len() {
                    return Err(call_err(format!(
                        "the signature takes {} argument(s), {} passed",
                        sig.params.len(),
                        cargs.len()
                    )));
                }
                for (i, (p, &a)) in sig.params.iter().zip(cargs).enumerate() {
                    if p.ty != self.f.value_ty(a) {
                        return Err(call_err(format!(
                            "argument {i} has type {}, the signature wants {}",
                            self.ty(a),
                            self.m.types.display(p.ty)
                        )));
                    }
                }
                if results.len() != sig.results.len() {
                    return Err(call_err(format!(
                        "the signature produces {} result(s), {} bound",
                        sig.results.len(),
                        results.len()
                    )));
                }
                for (i, (&w, &r)) in sig.results.iter().zip(&results).enumerate() {
                    if w != self.f.value_ty(r) {
                        return Err(call_err(format!(
                            "result {i} has type {}, the signature produces {}",
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
            // ---- the memory family (s26) ---------------------------
            Opcode::RegionNew => {
                self.expect_counts(inst, 0, 2)?;
                if self.f.value_ty(results[0]) != crate::types::PTR {
                    return Err(
                        self.type_err(inst, "region.new's first result is the arena handle (ptr)")
                    );
                }
                if !matches!(types.get(self.f.value_ty(results[1])), TypeData::Mem(_)) {
                    return Err(self.type_err(
                        inst,
                        "region.new's second result is the region's first mem token",
                    ));
                }
            }
            Opcode::RegionAlloc => {
                self.expect_counts(inst, 3, 2)?;
                if self.f.value_ty(args[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "region.alloc's handle must be ptr"));
                }
                if self.f.value_ty(args[1]) != crate::types::I64 {
                    return Err(self.type_err(inst, "region.alloc's size must be i64"));
                }
                let tok = self.f.value_ty(args[2]);
                if !matches!(types.get(tok), TypeData::Mem(_)) {
                    return Err(self.type_err(inst, "region.alloc needs a mem token operand"));
                }
                if self.f.value_ty(results[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "region.alloc's first result must be ptr"));
                }
                if self.f.value_ty(results[1]) != tok {
                    return Err(self.type_err(
                        inst,
                        "region.alloc's second result must be the successor of its mem token",
                    ));
                }
            }
            Opcode::RegionFree => {
                self.expect_counts(inst, 2, 0)?;
                if self.f.value_ty(args[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "region.free's handle must be ptr"));
                }
                if !matches!(types.get(self.f.value_ty(args[1])), TypeData::Mem(_)) {
                    return Err(self.type_err(inst, "region.free needs a mem token operand"));
                }
            }
            Opcode::RcDup | Opcode::RcDrop => {
                self.expect_counts(inst, 2, 1)?;
                if self.f.value_ty(args[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "rc ops take a ptr to the cell"));
                }
                let tok = self.f.value_ty(args[1]);
                if !matches!(types.get(tok), TypeData::Mem(_)) {
                    return Err(self.type_err(inst, "rc ops need a mem token operand"));
                }
                if self.f.value_ty(results[0]) != tok {
                    return Err(
                        self.type_err(inst, "rc ops produce the successor of their mem token")
                    );
                }
            }
            Opcode::SyncFreeze => {
                self.expect_counts(inst, 2, 1)?;
                if self.f.value_ty(args[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "sync.freeze's handle must be ptr"));
                }
                let tok = self.f.value_ty(args[1]);
                if !matches!(types.get(tok), TypeData::Mem(_)) {
                    return Err(self.type_err(inst, "sync.freeze needs a mem token operand"));
                }
                if self.f.value_ty(results[0]) != tok {
                    return Err(self.type_err(
                        inst,
                        "sync.freeze produces the frozen token of the same region",
                    ));
                }
            }
            Opcode::StackAlloc => {
                self.expect_counts(inst, 1, 2)?;
                if self.f.value_ty(args[0]) != crate::types::I64 {
                    return Err(self.type_err(inst, "stack.alloc's size must be i64"));
                }
                let ValueDef::Result(di, 0) = self.f.values[args[0]].def else {
                    return Err(self.type_err(inst, "stack.alloc's size must be an iconst"));
                };
                if self.f.insts[di].op != Opcode::Iconst {
                    return Err(self.type_err(inst, "stack.alloc's size must be an iconst"));
                }
                if self.f.value_ty(results[0]) != crate::types::PTR {
                    return Err(self.type_err(inst, "stack.alloc's first result must be ptr"));
                }
                if !matches!(types.get(self.f.value_ty(results[1])), TypeData::Mem(_)) {
                    return Err(
                        self.type_err(inst, "stack.alloc's second result is the slot's mem token")
                    );
                }
            }
            Opcode::RegionForeign => {
                self.expect_counts(inst, 0, 1)?;
                if !matches!(types.get(self.f.value_ty(results[0])), TypeData::Mem(_)) {
                    return Err(self.type_err(
                        inst,
                        "region.foreign's only result is the foreign region's mem token",
                    ));
                }
                // The role immediate is a CLOSED set (s80): it is the
                // only thing that decides whether two foreign roots may
                // alias, so an unknown one has no answer to give.
                let Aux::Int(code) = self.f.insts[inst].aux else {
                    return Err(self.type_err(
                        inst,
                        "region.foreign carries a role immediate (0 = header, 1 = buffer)",
                    ));
                };
                if ForeignRole::from_code(code).is_none() {
                    return Err(self.type_err(
                        inst,
                        "region.foreign's role must be 0 (header) or 1 (buffer)",
                    ));
                }
            }
            // ---- error unions (s27, D30) ---------------------------
            Opcode::EuMakeOk => {
                if results.len() != 1 {
                    return Err(self.type_err(inst, "eu.make.ok produces exactly one result"));
                }
                let TypeData::Eu { ok, .. } = types.get(self.f.value_ty(results[0])) else {
                    return Err(
                        self.type_err(inst, "eu.make.ok's result must be an error-union type")
                    );
                };
                match ok {
                    Some(t) => {
                        if args.len() != 1 {
                            return Err(self.type_err(
                                inst,
                                "eu.make.ok takes exactly the ok payload as its operand",
                            ));
                        }
                        if self.f.value_ty(args[0]) != *t {
                            return Err(self.type_err(
                                inst,
                                "eu.make.ok's operand must have the union's ok type",
                            ));
                        }
                    }
                    None => {
                        if !args.is_empty() {
                            return Err(self.type_err(
                                inst,
                                "eu.make.ok over a unit ok half takes no operand",
                            ));
                        }
                    }
                }
            }
            Opcode::EuMakeErr => {
                if results.len() != 1 {
                    return Err(self.type_err(inst, "eu.make.err produces exactly one result"));
                }
                let TypeData::Eu { slots, .. } = types.get(self.f.value_ty(results[0])).clone()
                else {
                    return Err(
                        self.type_err(inst, "eu.make.err's result must be an error-union type")
                    );
                };
                if args.is_empty() {
                    return Err(self.type_err(inst, "eu.make.err takes the error tag (i64) first"));
                }
                if self.f.value_ty(args[0]) != crate::types::I64 {
                    return Err(self.type_err(inst, "eu.make.err's tag operand must be i64"));
                }
                if args.len() - 1 > slots.len() {
                    return Err(self.type_err(
                        inst,
                        "eu.make.err passes more payloads than the union has slots",
                    ));
                }
                for (i, &a) in args[1..].iter().enumerate() {
                    if self.f.value_ty(a) != slots[i] {
                        return Err(self.type_err(
                            inst,
                            &format!("eu.make.err's payload {i} does not match slot {i}'s type"),
                        ));
                    }
                }
            }
            Opcode::EuIsErr => {
                self.expect_counts(inst, 1, 1)?;
                if !matches!(types.get(self.f.value_ty(args[0])), TypeData::Eu { .. }) {
                    return Err(self.type_err(inst, "eu.is_err's operand must be an error union"));
                }
                if self.f.value_ty(results[0]) != crate::types::BOOL {
                    return Err(self.type_err(inst, "eu.is_err's result must be bool"));
                }
            }
            Opcode::EuOk => {
                self.expect_counts(inst, 1, 1)?;
                let TypeData::Eu { ok, .. } = types.get(self.f.value_ty(args[0])) else {
                    return Err(self.type_err(inst, "eu.ok's operand must be an error union"));
                };
                let Some(t) = ok else {
                    return Err(self.type_err(inst, "eu.ok on a unit ok half extracts nothing"));
                };
                if self.f.value_ty(results[0]) != *t {
                    return Err(self.type_err(inst, "eu.ok's result must have the ok type"));
                }
            }
            Opcode::EuErr => {
                self.expect_counts(inst, 1, 1)?;
                let TypeData::Eu { slots, .. } = types.get(self.f.value_ty(args[0])).clone() else {
                    return Err(self.type_err(inst, "eu.err's operand must be an error union"));
                };
                match data.aux {
                    Aux::None => {
                        if self.f.value_ty(results[0]) != crate::types::I64 {
                            return Err(self.type_err(inst, "eu.err's tag result must be i64"));
                        }
                    }
                    Aux::Int(k) => {
                        let Some(&slot) = usize::try_from(k).ok().and_then(|k| slots.get(k)) else {
                            return Err(self.type_err(
                                inst,
                                &format!(
                                    "eu.err names slot {k}, but the union has {} slot(s)",
                                    slots.len()
                                ),
                            ));
                        };
                        if self.f.value_ty(results[0]) != slot {
                            return Err(self.type_err(
                                inst,
                                &format!("eu.err's result must have slot {k}'s type"),
                            ));
                        }
                    }
                    _ => {
                        return Err(self.type_err(inst, "eu.err carries no such payload"));
                    }
                }
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
                        // A value's `def` records where it was BORN, and
                        // for a block parameter that record survives the
                        // parameter's removal — the Braun trivial-φ test
                        // drops params, and a use the rewrite missed
                        // would otherwise pass a dominance test against
                        // a slot nothing occupies any more (s74, #66:
                        // exactly that reached the printer, which named
                        // the value and could not define it). So require
                        // the parameter to still BE the block's
                        // parameter at that index.
                        ValueDef::Param(db, di) => {
                            self.f.block_params(db).get(di as usize) == Some(&v) && dominates(db, b)
                        }
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

    /// The token operands `inst` CONSUMES (the chain is a spine):
    /// store's token, call's token args, the memory family's token, and
    /// token values passed along branch edges.
    fn consumed_tokens(&self, inst: Inst) -> Vec<Value> {
        let data = &self.f.insts[inst];
        let args = self.f.vpool.get(data.args);
        let is_tok = |v: Value| self.m.types.is_token(self.f.value_ty(v));
        let mut out = Vec::new();
        match data.op {
            Opcode::Store => {
                if let Some(&tok) = args.get(2) {
                    out.push(tok);
                }
            }
            Opcode::Call => out.extend(args.iter().copied().filter(|&a| is_tok(a))),
            Opcode::RegionAlloc => {
                if let Some(&tok) = args.get(2) {
                    out.push(tok);
                }
            }
            Opcode::RegionFree | Opcode::RcDup | Opcode::RcDrop | Opcode::SyncFreeze => {
                if let Some(&tok) = args.get(1) {
                    out.push(tok);
                }
            }
            _ => match data.aux {
                Aux::Jump(e) => {
                    out.extend(self.f.vpool.get(e.args).into_iter().filter(|&a| is_tok(a)));
                }
                Aux::Br(t, e) => {
                    out.extend(
                        self.f
                            .vpool
                            .get(t.args)
                            .into_iter()
                            .chain(self.f.vpool.get(e.args))
                            .filter(|&a| is_tok(a)),
                    );
                }
                _ => {}
            },
        }
        out.retain(|&v| is_tok(v));
        out
    }

    fn check_tokens(&self) -> VResult {
        // Linearity is PER PATH (s27): a token value may have several
        // consumers when they sit on mutually exclusive CFG paths —
        // defer/errdefer duplication puts the same `region.free` on
        // the normal edge AND the error edge. What stays rejected:
        // two consumers where one can REACH the other (sequential
        // double-consumption), including two in one block. Reaching
        // through the value's defining block does not count — re-
        // entering the def re-defines the value (loop-carried tokens
        // are per-iteration instances), same rule as `token-order`.
        let mut consumed: HashMap<Value, Vec<Inst>> = HashMap::new();
        for &b in &self.f.layout {
            for &inst in &self.f.blocks[b].insts {
                let mut toks = self.consumed_tokens(inst);
                // A `br` may pass one token along BOTH its edges (each
                // arm owns it; only one edge runs) — dedupe within the
                // instruction.
                toks.sort_by_key(|v| v.as_u32());
                toks.dedup();
                for tok in toks {
                    // Frozen tokens (sync.freeze results) are read-only
                    // forever: consuming one is unrepresentable
                    // mutation/free of frozen data.
                    if let ValueDef::Result(di, _) = self.f.values[tok].def
                        && self.f.insts[di].op == Opcode::SyncFreeze
                    {
                        return Err(self.fail(
                            ErrClass::FrozenToken,
                            format!(
                                "frozen token {} (from {}) used in a consuming position: {}",
                                self.canon.value(tok),
                                self.at_inst(di),
                                self.at_inst(inst)
                            ),
                        ));
                    }
                    consumed.entry(tok).or_default().push(inst);
                }
            }
        }
        for (&tok, insts) in &consumed {
            if insts.len() < 2 {
                continue;
            }
            let def_block = self.def_block_of(tok);
            for (i, &a) in insts.iter().enumerate() {
                for &b in &insts[i + 1..] {
                    let &(ab, _) = self.place.get(&a).expect("placed");
                    let &(bb, _) = self.place.get(&b).expect("placed");
                    // One block runs top to bottom: two consumers in
                    // it always both execute.
                    let sequential = ab == bb
                        || self.reaches_avoiding(ab, bb, def_block)
                        || self.reaches_avoiding(bb, ab, def_block);
                    if sequential {
                        return Err(self.fail(
                            ErrClass::TokenLinearity,
                            format!(
                                "effect token {} consumed twice on one path: by {} and by {}",
                                self.canon.value(tok),
                                self.at_inst(a),
                                self.at_inst(b)
                            ),
                        ));
                    }
                }
            }
        }
        self.check_token_order(&consumed)
    }

    /// The block a value is defined in.
    fn def_block_of(&self, v: Value) -> Block {
        match self.f.values[v].def {
            ValueDef::Param(db, _) => db,
            ValueDef::Result(di, _) => self.place.get(&di).map(|&(db, _)| db).expect("placed"),
        }
    }

    /// No read of a token value at a point reachable AFTER its
    /// consumption without passing through the value's (re)definition —
    /// this is what makes load-after-`region.free` (and any stale-token
    /// read) a structural rejection, not a runtime hope.
    fn check_token_order(&self, consumed: &HashMap<Value, Vec<Inst>>) -> VResult {
        for &b in &self.f.layout {
            for (pos, &inst) in self.f.blocks[b].insts.iter().enumerate() {
                if self.f.insts[inst].op != Opcode::Load {
                    continue;
                }
                let args = self.args(inst);
                let Some(&tok) = args.get(1) else { continue };
                let Some(consumers) = consumed.get(&tok) else {
                    continue;
                };
                let def_block = self.def_block_of(tok);
                for &consumer in consumers {
                    let &(cb, cpos) = self.place.get(&consumer).expect("placed");
                    let bad = if cb == b {
                        // Same block: a read after the consume is stale
                        // (within one block the value is never redefined).
                        cpos < pos
                    } else {
                        // The consumer's block reaches the reader's block
                        // without re-entering the token's defining block
                        // (re-entering redefines the value — loop-carried
                        // tokens are per-iteration instances).
                        self.reaches_avoiding(cb, b, def_block)
                    };
                    if bad {
                        return Err(self.fail(
                            ErrClass::TokenOrder,
                            format!(
                                "token {} is read by {} after being consumed by {} — no live token, no loads",
                                self.canon.value(tok),
                                self.at_inst(inst),
                                self.at_inst(consumer)
                            ),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Is `to` reachable from `from`'s successors along paths that
    /// never pass through `avoid`?
    fn reaches_avoiding(&self, from: Block, to: Block, avoid: Block) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut stack: Vec<Block> = successors(self.f, from)
            .into_iter()
            .filter(|&s| s != avoid)
            .collect();
        while let Some(b) = stack.pop() {
            if !seen.insert(b) {
                continue;
            }
            if b == to {
                return true;
            }
            for s in successors(self.f, b) {
                if s != avoid && !seen.contains(&s) {
                    stack.push(s);
                }
            }
        }
        false
    }

    /// One token ROOT per region: a region's token chain begins at
    /// exactly one of an entry token parameter, a `region.new`, or a
    /// `stack.alloc` — two roots would let unrelated chains forge each
    /// other's provenance.
    fn check_region_roots(&self) -> VResult {
        let mut roots: HashMap<u32, String> = HashMap::new();
        let entry = self.f.entry().expect("layout checked");
        for &v in &self.f.vpool.get(self.f.blocks[entry].params) {
            if let TypeData::Mem(r) = self.m.types.get(self.f.value_ty(v))
                && let Some(prev) = roots.insert(
                    r.as_u32(),
                    format!("entry parameter {}", self.canon.value(v)),
                )
            {
                return Err(self.fail(
                    ErrClass::RegionRoot,
                    format!("region {r} has two token roots: {prev} and another entry parameter"),
                ));
            }
        }
        for &b in &self.f.layout {
            for &inst in &self.f.blocks[b].insts {
                let op = self.f.insts[inst].op;
                // `region.foreign` roots with its ONLY result; the
                // allocating roots root with their second.
                let tok_ix = match op {
                    Opcode::RegionNew | Opcode::StackAlloc => 1,
                    Opcode::RegionForeign => 0,
                    _ => continue,
                };
                let results = self.results(inst);
                let Some(&tok) = results.get(tok_ix) else {
                    continue;
                };
                let TypeData::Mem(r) = self.m.types.get(self.f.value_ty(tok)) else {
                    continue; // ill-typed; the type check reports it
                };
                if let Some(prev) = roots.insert(r.as_u32(), self.at_inst(inst)) {
                    return Err(self.fail(
                        ErrClass::RegionRoot,
                        format!(
                            "region {r} has two token roots: {prev} and {}",
                            self.at_inst(inst)
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    // ---- facts ---------------------------------------------------------

    /// If `v` is the pointer result of an allocation op, the op and the
    /// region its token result mints.
    fn alloc_region_of(&self, v: Value) -> Option<(Inst, crate::types::RegionId)> {
        let ValueDef::Result(di, 0) = self.f.values[v].def else {
            return None;
        };
        if !matches!(
            self.f.insts[di].op,
            Opcode::RegionAlloc | Opcode::StackAlloc
        ) {
            return None;
        }
        let tok = *self.results(di).get(1)?;
        match self.m.types.get(self.f.value_ty(tok)) {
            TypeData::Mem(r) => Some((di, *r)),
            _ => None,
        }
    }

    /// The terminal base of a `ptr.off` chain: peel offsets until the
    /// value is not a `ptr.off` result.
    fn chain_base(&self, mut v: Value) -> Value {
        for _ in 0..64 {
            match self.f.values[v].def {
                ValueDef::Result(di, 0) if self.f.insts[di].op == Opcode::PtrOff => {
                    v = self.args(di)[0];
                }
                _ => return v,
            }
        }
        v
    }

    /// s104: the shape audit for a guard-justified `noalias` fact —
    /// custody, not proof. The versioner that minted the fact owns the
    /// extent arithmetic (as c04 owns a theorem tag); this check pins
    /// what one function CAN see:
    ///
    /// - the guard is `bor(ule(endA, baseB), ule(endB, baseA))` over
    ///   pointers, each end at least one `ptr.off` step off its base —
    ///   the operand-identity half (a guard over unrelated pointers
    ///   justifies nothing);
    /// - each fact subject derives from a distinct guard base by
    ///   `ptr.off` steps alone;
    /// - the subjects are defined in blocks dominated by the guard
    ///   branch's TAKEN edge, whose target has the guard block as its
    ///   only predecessor — so the fact exists only where the check
    ///   passed. A forged fact — wrong dominance, unrelated operands —
    ///   is refused: the negatives are pinned in `verify_red`.
    fn check_guarded_noalias(
        &self,
        id: crate::facts::FactId,
        a: Value,
        b: Value,
        c: Value,
    ) -> VResult {
        if self.f.value_ty(c) != crate::types::BOOL {
            return Err(self.fact_fail(ErrClass::FactJust, id, "guard must be a bool value"));
        }
        let ValueDef::Result(bi, 0) = self.f.values[c].def else {
            return Err(self.fact_fail(
                ErrClass::FactJust,
                id,
                "guard must be the result of a `bor` of two pointer compares",
            ));
        };
        if self.f.insts[bi].op != Opcode::Bor {
            return Err(self.fact_fail(
                ErrClass::FactJust,
                id,
                "guard must be the result of a `bor` of two pointer compares",
            ));
        }
        let bargs = self.args(bi);
        let mut ends_bases: Vec<(Value, Value)> = Vec::new();
        for &cv in &bargs {
            let ValueDef::Result(ci, 0) = self.f.values[cv].def else {
                return Err(self.fact_fail(
                    ErrClass::FactJust,
                    id,
                    "guard arm must be an `icmp.ule` over pointers",
                ));
            };
            let ok = self.f.insts[ci].op == Opcode::Icmp
                && matches!(self.f.insts[ci].aux, Aux::IntCc(IntCc::Ule));
            if !ok {
                return Err(self.fact_fail(
                    ErrClass::FactJust,
                    id,
                    "guard arm must be an `icmp.ule` over pointers",
                ));
            }
            let cargs = self.args(ci);
            if self.f.value_ty(cargs[0]) != crate::types::PTR {
                return Err(self.fact_fail(
                    ErrClass::FactJust,
                    id,
                    "guard arm must be an `icmp.ule` over pointers",
                ));
            }
            ends_bases.push((cargs[0], cargs[1]));
        }
        let [(end0, r0), (end1, r1)] = ends_bases[..] else {
            return Err(self.fact_fail(ErrClass::FactJust, id, "guard must have two arms"));
        };
        // Arms: ule(endA, baseB) and ule(endB, baseA) — each end must
        // actually step off its base (a zero-hop "end" guards nothing).
        let (base_a, base_b) = (r1, r0);
        if base_a == base_b
            || end0 == base_a
            || end1 == base_b
            || self.chain_base(end0) != base_a
            || self.chain_base(end1) != base_b
        {
            return Err(self.fact_fail(
                ErrClass::FactJust,
                id,
                "guard operands do not tie the compared extents to the fact subjects' bases",
            ));
        }
        // One subject per base, ptr.off-derived.
        let sides_ok = (self.derived_from(a, base_a) && self.derived_from(b, base_b))
            || (self.derived_from(a, base_b) && self.derived_from(b, base_a));
        if !sides_ok {
            return Err(self.fact_fail(
                ErrClass::FactJust,
                id,
                "guard operands do not tie the compared extents to the fact subjects' bases",
            ));
        }
        // Dominance: the unique br on the guard's taken edge.
        let mut takens: Vec<(Block, Block)> = Vec::new();
        for &bb in &self.canon.blocks {
            let Some(&term) = self.f.blocks[bb].insts.last() else {
                continue;
            };
            if self.f.insts[term].op == Opcode::Br
                && self.args(term).first() == Some(&c)
                && let Aux::Br(t, _) = self.f.insts[term].aux
            {
                takens.push((bb, t.block));
            }
        }
        let [(gb, taken)] = takens[..] else {
            return Err(self.fact_fail(
                ErrClass::FactJust,
                id,
                "guard must feed exactly one branch",
            ));
        };
        let mut preds: Vec<Block> = Vec::new();
        for &bb in &self.canon.blocks {
            for sc in successors(self.f, bb) {
                if sc == taken && !preds.contains(&bb) {
                    preds.push(bb);
                }
            }
        }
        if preds != [gb] {
            return Err(self.fact_fail(
                ErrClass::FactJust,
                id,
                "the guard's taken edge must be the target's only entry",
            ));
        }
        let def_block = |v: Value| -> Option<Block> {
            match self.f.values[v].def {
                ValueDef::Param(bb, _) => Some(bb),
                ValueDef::Result(ii, _) => self
                    .canon
                    .blocks
                    .iter()
                    .copied()
                    .find(|&bb| self.f.blocks[bb].insts.contains(&ii)),
            }
        };
        let idom = self.idoms();
        let dominates = |x: Block, mut y: Block| -> bool {
            loop {
                if y == x {
                    return true;
                }
                let Some(&up) = idom.get(&y) else {
                    return false;
                };
                if up == y {
                    return false;
                }
                y = up;
            }
        };
        for subj in [a, b] {
            let Some(db) = def_block(subj) else {
                return Err(self.fact_fail(
                    ErrClass::FactJust,
                    id,
                    "guarded subjects must be defined in reachable blocks",
                ));
            };
            if !dominates(taken, db) {
                return Err(self.fact_fail(
                    ErrClass::FactJust,
                    id,
                    "guarded subjects must be defined under the guard's taken edge",
                ));
            }
        }
        Ok(())
    }

    /// Iterative idoms over the canon RPO (Cooper–Harvey–Kennedy), for
    /// the fact checks — the dominance walk recomputes them only when
    /// a guard-justified fact is present.
    fn idoms(&self) -> HashMap<Block, Block> {
        let rpo = &self.canon.blocks;
        let mut rpo_num: HashMap<Block, usize> = HashMap::new();
        for (i, &bb) in rpo.iter().enumerate() {
            rpo_num.insert(bb, i);
        }
        let mut preds: HashMap<Block, Vec<Block>> = HashMap::new();
        for &bb in rpo {
            for sc in successors(self.f, bb) {
                preds.entry(sc).or_default().push(bb);
            }
        }
        let Some(entry) = self.f.entry() else {
            return HashMap::new();
        };
        let mut idom: HashMap<Block, Block> = HashMap::new();
        idom.insert(entry, entry);
        let mut changed = true;
        while changed {
            changed = false;
            for &bb in rpo.iter().skip(1) {
                let plist = preds.get(&bb).cloned().unwrap_or_default();
                let mut new_idom: Option<Block> = None;
                for &pp in &plist {
                    if !idom.contains_key(&pp) {
                        continue;
                    }
                    new_idom = Some(match new_idom {
                        None => pp,
                        Some(cur) => intersect(&idom, &rpo_num, cur, pp),
                    });
                }
                if let Some(ni) = new_idom
                    && idom.get(&bb) != Some(&ni)
                {
                    idom.insert(bb, ni);
                    changed = true;
                }
            }
        }
        idom
    }

    /// Is `v` equal to `base`, or derived from it through `ptr.off`
    /// steps (structural provenance inheritance)?
    fn derived_from(&self, mut v: Value, base: Value) -> bool {
        for _ in 0..64 {
            if v == base {
                return true;
            }
            match self.f.values[v].def {
                ValueDef::Result(di, 0) if self.f.insts[di].op == Opcode::PtrOff => {
                    v = self.args(di)[0];
                }
                _ => return false,
            }
        }
        false
    }

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
            if let Just::Op(v) | Just::Guard(v) = fd.just {
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
                        Just::Theorem(
                            Theorem::ExclMut | Theorem::FrozenRead | Theorem::ExclField,
                        ) => {}
                        Just::Guard(c) => {
                            self.check_guarded_noalias(id, a, b, c)?;
                        }
                        _ => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "noalias must cite a checker theorem or a guard — there is no way to state an unverified aliasing claim in safe-tier WIR",
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
                        Just::Summary(_) => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "summary justifications mint range facts only (s99)",
                            ));
                        }
                        Just::Guard(_) => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "guard justifications mint noalias facts only (s104)",
                            ));
                        }
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
                            let op = self.f.insts[di].op;
                            if !matches!(op, Opcode::RegionAlloc | Opcode::StackAlloc) {
                                return Err(self.fact_fail(
                                    ErrClass::FactDeref,
                                    id,
                                    "deref fact cites a non-allocation op (`region.alloc`/`stack.alloc`)",
                                ));
                            }
                            // The claim may not exceed the allocation:
                            // re-derive against the size operand when it
                            // is a constant.
                            let size_arg = match op {
                                Opcode::RegionAlloc => self.args(di).get(1).copied(),
                                _ => self.args(di).first().copied(),
                            };
                            let alloc_size = size_arg.and_then(|s| match self.f.values[s].def {
                                ValueDef::Result(si, 0)
                                    if self.f.insts[si].op == Opcode::Iconst =>
                                {
                                    match self.f.insts[si].aux {
                                        Aux::Int(n) => Some(n),
                                        _ => None,
                                    }
                                }
                                _ => None,
                            });
                            if let (DerefSize::Const(n), Some(k)) = (size, alloc_size)
                                && (k < 0 || n > k as u64)
                            {
                                return Err(self.fact_fail(
                                    ErrClass::FactDeref,
                                    id,
                                    "deref fact claims more bytes than the cited allocation provides",
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
                        // s99: proved by the whole-program range
                        // analysis. One function cannot re-derive a
                        // whole-program proof, so this check is
                        // SHAPE-ONLY here (integer subject, non-empty
                        // — already above); the semantic re-derivation
                        // is `midend::interproc::reverify`, run under
                        // `verify_each` in the whole-program phase.
                        Just::Summary(_) => {}
                        Just::Op(_) | Just::Guard(_) => {
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
                            let const_of = |val: Value| -> Option<i64> {
                                match self.f.values[val].def {
                                    ValueDef::Result(si, 0)
                                        if self.f.insts[si].op == Opcode::Iconst =>
                                    {
                                        match self.f.insts[si].aux {
                                            Aux::Int(n) => Some(n),
                                            _ => None,
                                        }
                                    }
                                    _ => None,
                                }
                            };
                            let (dlo, dhi) = match (self.f.insts[di].op, self.f.insts[di].aux) {
                                (Opcode::Iconst, Aux::Int(c)) => (c as i128, c as i128),
                                // Remainder postconditions (X3's
                                // claw-back): a no-trap `irem.chk` by a
                                // positive constant c lands in
                                // -(c-1)..=c-1; `urem.chk` in 0..=c-1.
                                (Opcode::IremChk, _) => {
                                    match self.args(di).get(1).and_then(|&d| const_of(d)) {
                                        Some(c) if c > 0 => (-(c as i128 - 1), c as i128 - 1),
                                        _ => (tlo, thi),
                                    }
                                }
                                (Opcode::UremChk, _) => {
                                    match self.args(di).get(1).and_then(|&d| const_of(d)) {
                                        Some(c) if c > 0 => (0, c as i128 - 1),
                                        _ => (tlo, thi),
                                    }
                                }
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
                FactKind::Region(v, r) => {
                    if !is_ptr(v) {
                        return Err(self.fact_fail(
                            ErrClass::FactType,
                            id,
                            "region subject must be a pointer",
                        ));
                    }
                    match fd.just {
                        Just::Theorem(Theorem::RegionAlloc) => {}
                        Just::Summary(_) => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "summary justifications mint range facts only (s99)",
                            ));
                        }
                        Just::Guard(_) => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "guard justifications mint noalias facts only (s104)",
                            ));
                        }
                        Just::DefOp | Just::Op(_) => {
                            // Op-derived region identity is PROVENANCE,
                            // re-derived structurally: the cited op must
                            // be an allocation minting exactly region r,
                            // and the subject must be its pointer result
                            // or a `ptr.off`-derived pointer from it —
                            // cross-region forgery is a rejection.
                            let cited = match fd.just {
                                Just::Op(c) => c,
                                _ => v,
                            };
                            let Some((di, minted)) = self.alloc_region_of(cited) else {
                                return Err(self.fact_fail(
                                    ErrClass::FactJust,
                                    id,
                                    "op-derived region facts must cite a `region.alloc`/`stack.alloc` pointer result",
                                ));
                            };
                            if minted != r {
                                return Err(self.fact_fail(
                                    ErrClass::FactJust,
                                    id,
                                    "region fact names a different region than the cited allocation mints — provenance cannot be forged in safe-tier WIR",
                                ));
                            }
                            let ptr_result = self.results(di)[0];
                            if !self.derived_from(v, ptr_result) {
                                return Err(self.fact_fail(
                                    ErrClass::FactJust,
                                    id,
                                    "region fact subject is not derived from the cited allocation's pointer",
                                ));
                            }
                        }
                        Just::Theorem(_) => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "region facts cite the region.alloc theorem or their allocating op",
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
                        Just::Op(c) => {
                            // Deep immutability by a freeze point: the
                            // cited value must be a sync.freeze result.
                            let frozen_by_op = matches!(
                                self.f.values[c].def,
                                ValueDef::Result(di, _)
                                    if self.f.insts[di].op == Opcode::SyncFreeze
                            );
                            if !frozen_by_op {
                                return Err(self.fact_fail(
                                    ErrClass::FactJust,
                                    id,
                                    "op-justified frozen facts must cite a `sync.freeze` result",
                                ));
                            }
                        }
                        _ => {
                            return Err(self.fact_fail(
                                ErrClass::FactJust,
                                id,
                                "frozen facts cite the frozen.read theorem or a `sync.freeze` point",
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
    v.check_region_roots()?;
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

    /// The invalidations a pass recorded (s42: the real pass manager
    /// audits these against the compacted function).
    pub fn invalidations(&self) -> &[(crate::facts::FactId, Invalidation)] {
        &self.invalidated
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
