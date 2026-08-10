//! The WIR instruction set v0 — enumerated, closed, one line of
//! semantics per op. This file is the op-semantics reference until the
//! spec absorbs it (s24 acceptance).
//!
//! Design posture:
//! - X3: checked arithmetic (`*.chk`) traps on overflow in BOTH tiers;
//!   wrapping/saturating behavior is only ever spelled explicitly.
//! - Effect tokens are OPERANDS, not annotations: loads read a `mem`
//!   token, stores/calls consume tokens and produce successors.
//! - There is deliberately NO unwind/invoke-shaped terminator (D30):
//!   unwinding is unrepresentable, not just unused. Error unions lower
//!   through the `eu.*` value ops plus ordinary `br` (s27).
//! - `region.*` / `rc.*` / `sync.*` / `eu.*` mnemonics are RESERVED:
//!   they parse and print (so the text format doesn't churn) but the
//!   verifier rejects them until their semantics land (s26/s27).

/// Comparison condition for `icmp.*` (s = signed, u = unsigned order).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntCc {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
    Ult,
    Ule,
    Ugt,
    Uge,
}

impl IntCc {
    pub const ALL: [IntCc; 10] = [
        IntCc::Eq,
        IntCc::Ne,
        IntCc::Slt,
        IntCc::Sle,
        IntCc::Sgt,
        IntCc::Sge,
        IntCc::Ult,
        IntCc::Ule,
        IntCc::Ugt,
        IntCc::Uge,
    ];

    pub fn mnemonic(self) -> &'static str {
        match self {
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
}

/// Comparison condition for `fcmp.*` (ordered: NaN compares false).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FloatCc {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl FloatCc {
    pub const ALL: [FloatCc; 6] = [
        FloatCc::Eq,
        FloatCc::Ne,
        FloatCc::Lt,
        FloatCc::Le,
        FloatCc::Gt,
        FloatCc::Ge,
    ];

    pub fn mnemonic(self) -> &'static str {
        match self {
            FloatCc::Eq => "eq",
            FloatCc::Ne => "ne",
            FloatCc::Lt => "lt",
            FloatCc::Le => "le",
            FloatCc::Gt => "gt",
            FloatCc::Ge => "ge",
        }
    }
}

/// Every WIR opcode. One doc line of semantics each — this is the
/// reference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opcode {
    // ---- constants -----------------------------------------------------
    /// `%r = iconst.T N` — integer constant of type T; N must fit T.
    Iconst,
    /// `%r = fconst.T 0xBITS` — float constant, canonical form is raw
    /// IEEE-754 bits so dumps are byte-stable.
    Fconst,
    /// `%r = bconst true|false` — boolean constant.
    Bconst,

    // ---- integer arithmetic (X3: checked is the default the language
    // lowers to; wrap/sat are explicit source-level requests) ------------
    /// `%r = iadd.chk %a, %b` — signed add; TRAPS on overflow (both tiers).
    IaddChk,
    /// `%r = isub.chk %a, %b` — signed subtract; traps on overflow.
    IsubChk,
    /// `%r = imul.chk %a, %b` — signed multiply; traps on overflow.
    ImulChk,
    /// `%r = idiv.chk %a, %b` — signed divide; traps on zero and MIN/-1.
    IdivChk,
    /// `%r = irem.chk %a, %b` — signed remainder; traps on zero and MIN/-1.
    IremChk,
    /// `%r = iadd.wrap %a, %b` — two's-complement wrapping add.
    IaddWrap,
    /// `%r = isub.wrap %a, %b` — wrapping subtract.
    IsubWrap,
    /// `%r = imul.wrap %a, %b` — wrapping multiply.
    ImulWrap,
    /// `%r = iadd.sat %a, %b` — saturating add (clamps to type bounds).
    IaddSat,
    /// `%r = isub.sat %a, %b` — saturating subtract.
    IsubSat,
    /// `%r = imul.sat %a, %b` — saturating multiply.
    ImulSat,

    // ---- bit operations (total, no variants needed) --------------------
    /// `%r = band %a, %b` — bitwise and.
    Band,
    /// `%r = bor %a, %b` — bitwise or.
    Bor,
    /// `%r = bxor %a, %b` — bitwise xor.
    Bxor,
    /// `%r = shl %a, %b` — shift left; shift amount masked to bit width.
    Shl,
    /// `%r = lshr %a, %b` — logical (zero-fill) shift right; amount masked.
    Lshr,
    /// `%r = ashr %a, %b` — arithmetic (sign-fill) shift right; amount masked.
    Ashr,

    // ---- float arithmetic (IEEE-754, no traps) -------------------------
    /// `%r = fadd %a, %b` — float add.
    Fadd,
    /// `%r = fsub %a, %b` — float subtract.
    Fsub,
    /// `%r = fmul %a, %b` — float multiply.
    Fmul,
    /// `%r = fdiv %a, %b` — float divide (IEEE: /0 is ±inf/NaN, no trap).
    Fdiv,
    /// `%r = fneg %a` — float negate.
    Fneg,
    /// `%r = fma %a, %b, %c` — fused multiply-add, a*b+c with one rounding.
    Fma,

    // ---- compares ------------------------------------------------------
    /// `%r = icmp.CC %a, %b` — integer compare, result `bool`.
    Icmp,
    /// `%r = fcmp.CC %a, %b` — ordered float compare (NaN ⇒ false), `bool`.
    Fcmp,

    // ---- conversions ---------------------------------------------------
    /// `%r = sext.T %a` — sign-extend integer to the wider type T.
    Sext,
    /// `%r = zext.T %a` — zero-extend integer to the wider type T.
    Zext,
    /// `%r = itrunc.T %a` — truncate integer to the narrower type T.
    Itrunc,

    // ---- memory --------------------------------------------------------
    /// `%r = ptr.off %p, %i, S` — pointer p + i*S bytes (S a positive imm).
    PtrOff,
    /// `%r = load.T %p, %m` — load T from p; READS mem token m (not consumed).
    Load,
    /// `%m2 = store.T %v, %p, %m` — store v to p; CONSUMES m, produces m2.
    Store,

    // ---- aggregates (small, by value list) -----------------------------
    /// `%r = agg.make.T %f0, %f1, …` — build an aggregate from its fields.
    AggMake,
    /// `%r = agg.get.K %a` — extract field K (imm) from an aggregate.
    AggGet,

    // ---- calls ---------------------------------------------------------
    /// `%res…, %tok… = call @f(%args…)` — call with the mode-carrying sig;
    /// token args are consumed, and a fresh successor token is produced
    /// for each (after the declared results, in param order).
    Call,

    // ---- terminators (exactly one per block; NO unwind form, D30) ------
    /// `jmp bN(%args…)` — unconditional branch, passing block arguments.
    Jmp,
    /// `br %c, bT(%args…), bF(%args…)` — branch on bool c.
    Br,
    /// `ret %vals…` — return the signature's results.
    Ret,
    /// `trap` — abort the program (checked-arith overflow lowers here).
    Trap,

    // ---- RESERVED mnemonics (parse/print now; semantics s26/s27) -------
    /// RESERVED (s26): `%r, %m = region.new` — create a region + its token.
    RegionNew,
    /// RESERVED (s26): `%p, %m2 = region.alloc.T %r, %m` — allocate in a region.
    RegionAlloc,
    /// RESERVED (s26): `%m2 = region.free %r, %m` — free a whole region.
    RegionFree,
    /// RESERVED (s26): `%r = rc.dup %p` — shared-tier refcount increment.
    RcDup,
    /// RESERVED (s26): `rc.drop %p` — shared-tier refcount decrement.
    RcDrop,
    /// RESERVED (s26): `%q = sync.freeze %p` — freeze to deep-immutable.
    SyncFreeze,
    /// RESERVED (s26): `%q = sync.transfer %p` — region transfer to a task.
    SyncTransfer,
    /// RESERVED (s27): `%e = eu.make.ok %v` — wrap a success value in `!T`.
    EuMakeOk,
    /// RESERVED (s27): `%e = eu.make.err %t` — wrap an error tag in `!T`.
    EuMakeErr,
    /// RESERVED (s27): `%b = eu.is_err %e` — test the error bit (br on it).
    EuIsErr,
    /// RESERVED (s27): `%v = eu.ok %e` — extract the success payload.
    EuOk,
    /// RESERVED (s27): `%t = eu.err %e` — extract the error tag.
    EuErr,
}

impl Opcode {
    /// True for block terminators. Every block ends with exactly one.
    pub fn is_terminator(self) -> bool {
        matches!(self, Opcode::Jmp | Opcode::Br | Opcode::Ret | Opcode::Trap)
    }

    /// True for mnemonics reserved for s26/s27: representable in text,
    /// rejected by the verifier until their semantics land.
    pub fn is_reserved(self) -> bool {
        matches!(
            self,
            Opcode::RegionNew
                | Opcode::RegionAlloc
                | Opcode::RegionFree
                | Opcode::RcDup
                | Opcode::RcDrop
                | Opcode::SyncFreeze
                | Opcode::SyncTransfer
                | Opcode::EuMakeOk
                | Opcode::EuMakeErr
                | Opcode::EuIsErr
                | Opcode::EuOk
                | Opcode::EuErr
        )
    }

    /// The fixed part of the mnemonic (type/cc suffixes are printed by
    /// the printer from instruction data).
    pub fn base_mnemonic(self) -> &'static str {
        match self {
            Opcode::Iconst => "iconst",
            Opcode::Fconst => "fconst",
            Opcode::Bconst => "bconst",
            Opcode::IaddChk => "iadd.chk",
            Opcode::IsubChk => "isub.chk",
            Opcode::ImulChk => "imul.chk",
            Opcode::IdivChk => "idiv.chk",
            Opcode::IremChk => "irem.chk",
            Opcode::IaddWrap => "iadd.wrap",
            Opcode::IsubWrap => "isub.wrap",
            Opcode::ImulWrap => "imul.wrap",
            Opcode::IaddSat => "iadd.sat",
            Opcode::IsubSat => "isub.sat",
            Opcode::ImulSat => "imul.sat",
            Opcode::Band => "band",
            Opcode::Bor => "bor",
            Opcode::Bxor => "bxor",
            Opcode::Shl => "shl",
            Opcode::Lshr => "lshr",
            Opcode::Ashr => "ashr",
            Opcode::Fadd => "fadd",
            Opcode::Fsub => "fsub",
            Opcode::Fmul => "fmul",
            Opcode::Fdiv => "fdiv",
            Opcode::Fneg => "fneg",
            Opcode::Fma => "fma",
            Opcode::Icmp => "icmp",
            Opcode::Fcmp => "fcmp",
            Opcode::Sext => "sext",
            Opcode::Zext => "zext",
            Opcode::Itrunc => "itrunc",
            Opcode::PtrOff => "ptr.off",
            Opcode::Load => "load",
            Opcode::Store => "store",
            Opcode::AggMake => "agg.make",
            Opcode::AggGet => "agg.get",
            Opcode::Call => "call",
            Opcode::Jmp => "jmp",
            Opcode::Br => "br",
            Opcode::Ret => "ret",
            Opcode::Trap => "trap",
            Opcode::RegionNew => "region.new",
            Opcode::RegionAlloc => "region.alloc",
            Opcode::RegionFree => "region.free",
            Opcode::RcDup => "rc.dup",
            Opcode::RcDrop => "rc.drop",
            Opcode::SyncFreeze => "sync.freeze",
            Opcode::SyncTransfer => "sync.transfer",
            Opcode::EuMakeOk => "eu.make.ok",
            Opcode::EuMakeErr => "eu.make.err",
            Opcode::EuIsErr => "eu.is_err",
            Opcode::EuOk => "eu.ok",
            Opcode::EuErr => "eu.err",
        }
    }
}
