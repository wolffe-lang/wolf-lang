//! WIR program generation for the scoped-noalias differential fuzz
//! rig (s41 target 5, mandated by the Rust noalias saga).
//!
//! Every generated program has KNOWN-TRUE aliasing facts by
//! construction: distinct regions really are disjoint bump arenas, so
//! the scope metadata the emitter attaches is a theorem, never a hope.
//! The rig lowers each program twice — metadata-on and
//! metadata-stripped ([`crate::EmitOptions::strip_facts`]) — compiles
//! both at -O0/-O2/-O3, runs all six, and any divergence is an LLVM
//! bug or a lowering bug; either way it blocks emission of that
//! pattern until triaged.
//!
//! The permanent seed corpus rebuilds the historical miscompile shapes
//! in WIR terms (report 10 amendment 1 added the third + a stressor):
//!
//! 1. **noalias + inlining interaction** (the first Rust deactivation):
//!    a region-owning helper whose scoped memory ops get inlined into
//!    a calling loop.
//! 2. **scope-domain interaction under LICM** (the second): a
//!    loop-invariant load from one region against stores into another
//!    — hoisting is licensed exactly by our `!noalias` lists.
//! 3. **scope duplication under loop unrolling** (rust-noalias-54878's
//!    bisected root cause): a small constant-trip loop whose annotated
//!    load/store pair gets CLONED by the unroller.
//! 4. **CFG-duplication stressor**: unroll + jump-thread + inline
//!    compositions — the general class is "any pass that clones
//!    annotated instructions".
//!
//! [`random_program`] extends the corpus with seeded multi-region
//! load/store/checked-arith loops (deterministic xorshift; same seed,
//! same program).

use wolf_wir::entity::EntityRef;
use wolf_wir::ir::{Aux, Block, Function, Module, Param, Value};
use wolf_wir::ops::{IntCc, Opcode};
use wolf_wir::types::{self, RegionId, TypeId};

/// A tiny raw-arena builder (the s25 Braun builder is for lowering;
/// generated programs are simplest built explicitly).
struct Fb {
    f: Function,
    cur: Block,
}

impl Fb {
    fn new(name: &str, sig: wolf_wir::ir::SigId, param_tys: &[TypeId]) -> Fb {
        let mut f = Function::new(name, sig);
        let entry = f.make_block(param_tys);
        Fb { f, cur: entry }
    }

    fn ins(&mut self, op: Opcode, args: &[Value], tys: &[TypeId], aux: Aux) -> Vec<Value> {
        self.f.append_inst(self.cur, op, args, tys, aux).1
    }

    fn iconst(&mut self, n: i64) -> Value {
        self.ins(Opcode::Iconst, &[], &[types::I64], Aux::Int(n))[0]
    }

    fn add_chk(&mut self, a: Value, b: Value) -> Value {
        self.ins(Opcode::IaddChk, &[a, b], &[types::I64], Aux::None)[0]
    }

    fn add_wrap(&mut self, a: Value, b: Value) -> Value {
        self.ins(Opcode::IaddWrap, &[a, b], &[types::I64], Aux::None)[0]
    }

    fn band(&mut self, a: Value, b: Value) -> Value {
        self.ins(Opcode::Band, &[a, b], &[types::I64], Aux::None)[0]
    }

    fn icmp(&mut self, cc: IntCc, a: Value, b: Value) -> Value {
        self.ins(Opcode::Icmp, &[a, b], &[types::BOOL], Aux::IntCc(cc))[0]
    }

    /// `region.new` for region `r`: (handle, first token).
    fn region_new(&mut self, m: &mut Module, r: u32) -> (Value, Value) {
        let tok = m.types.mem(RegionId::new(r));
        let vs = self.ins(Opcode::RegionNew, &[], &[types::PTR, tok], Aux::None);
        (vs[0], vs[1])
    }

    /// `region.alloc`: (ptr, successor token).
    fn alloc(
        &mut self,
        m: &mut Module,
        r: u32,
        h: Value,
        size: Value,
        tok: Value,
    ) -> (Value, Value) {
        let tt = m.types.mem(RegionId::new(r));
        let vs = self.ins(
            Opcode::RegionAlloc,
            &[h, size, tok],
            &[types::PTR, tt],
            Aux::None,
        );
        (vs[0], vs[1])
    }

    /// `store.i64`: the successor token.
    fn store(&mut self, m: &mut Module, r: u32, v: Value, p: Value, tok: Value) -> Value {
        let tt = m.types.mem(RegionId::new(r));
        self.ins(Opcode::Store, &[v, p, tok], &[tt], Aux::None)[0]
    }

    fn load(&mut self, p: Value, tok: Value) -> Value {
        self.ins(Opcode::Load, &[p, tok], &[types::I64], Aux::None)[0]
    }

    fn freeze(&mut self, m: &mut Module, r: u32, h: Value, tok: Value) -> Value {
        let tt = m.types.mem(RegionId::new(r));
        self.ins(Opcode::SyncFreeze, &[h, tok], &[tt], Aux::None)[0]
    }

    fn jmp(&mut self, target: Block, args: &[Value]) {
        let edge = self.f.block_call(target, args);
        self.ins(Opcode::Jmp, &[], &[], Aux::Jump(edge));
    }

    fn br(&mut self, cond: Value, t: Block, targs: &[Value], e: Block, eargs: &[Value]) {
        let te = self.f.block_call(t, targs);
        let ee = self.f.block_call(e, eargs);
        self.ins(Opcode::Br, &[cond], &[], Aux::Br(te, ee));
    }

    fn ret(&mut self, v: Value) {
        self.ins(Opcode::Ret, &[v], &[], Aux::None);
    }

    fn block(&mut self, param_tys: &[TypeId]) -> Block {
        self.f.make_block(param_tys)
    }

    fn switch(&mut self, b: Block) {
        self.cur = b;
    }

    fn params(&self, b: Block) -> Vec<Value> {
        self.f.block_params(b)
    }
}

/// Mask a final value into the 0..=63 exit-code window (clear of the
/// runtime's trap exit code).
fn finish_main(fb: &mut Fb, v: Value) {
    let mask = fb.iconst(63);
    let r = fb.band(v, mask);
    fb.ret(r);
}

/// Shape 1 — noalias + inlining: a helper owning two disjoint regions,
/// called from a loop; -O2 inlines its scoped ops into the caller.
pub fn shape_inline_noalias() -> Module {
    let mut m = Module::new();
    let helper_sig = m.make_sig(vec![Param::val(types::I64)], vec![types::I64]);
    let main_sig = m.make_sig(vec![], vec![types::I64]);

    // helper(x): p in r0, q in r1; store x -> p, store x+1 -> q,
    // re-load p (licensed to forward across the q store), return
    // p_val*2 + q_val.
    let mut h = Fb::new("helper", helper_sig, &[types::I64]);
    let x = h.params(h.cur)[0];
    let size = h.iconst(8);
    let (h0, t0) = h.region_new(&mut m, 0);
    let (p, t0) = h.alloc(&mut m, 0, h0, size, t0);
    let (h1, t1) = h.region_new(&mut m, 1);
    let (q, t1) = h.alloc(&mut m, 1, h1, size, t1);
    let t0 = h.store(&mut m, 0, x, p, t0);
    let one = h.iconst(1);
    let x1 = h.add_wrap(x, one);
    let _t1 = h.store(&mut m, 1, x1, q, t1);
    let pv = h.load(p, t0);
    let qv = h.load(q, _t1);
    let two = h.iconst(2);
    let d = h.ins(Opcode::ImulWrap, &[pv, two], &[types::I64], Aux::None)[0];
    let s = h.add_wrap(d, qv);
    h.ret(s);
    let helper = m.add_func(h.f);
    let _ = helper;

    // main: sum helper(i) for i in 0..6.
    let mut f = Fb::new("main", main_sig, &[]);
    let zero = f.iconst(0);
    let header = f.block(&[types::I64, types::I64]); // (i, acc)
    let body = f.block(&[]);
    let exit = f.block(&[]);
    f.jmp(header, &[zero, zero]);
    f.switch(header);
    let [i, acc] = f.params(header)[..] else {
        unreachable!()
    };
    let n = f.iconst(6);
    let c = f.icmp(IntCc::Slt, i, n);
    f.br(c, body, &[], exit, &[]);
    f.switch(body);
    let callee = f.f.import_func("helper", helper_sig);
    let hv = f.ins(Opcode::Call, &[i], &[types::I64], Aux::Callee(callee))[0];
    let acc2 = f.add_wrap(acc, hv);
    let one = f.iconst(1);
    let i2 = f.add_chk(i, one);
    f.jmp(header, &[i2, acc2]);
    f.switch(exit);
    finish_main(&mut f, acc);
    m.add_func(f.f);
    m
}

/// Shape 2 — scope interaction under LICM: a loop-invariant load from
/// a (frozen) region against stores into another; hoisting is
/// licensed by the `!noalias` lists + `!invariant.load`.
pub fn shape_licm_scopes() -> Module {
    let mut m = Module::new();
    let main_sig = m.make_sig(vec![], vec![types::I64]);
    let mut f = Fb::new("main", main_sig, &[]);
    let size = f.iconst(8);
    let (h0, t0) = f.region_new(&mut m, 0);
    let (p, t0) = f.alloc(&mut m, 0, h0, size, t0);
    let seven = f.iconst(7);
    let t0 = f.store(&mut m, 0, seven, p, t0);
    // Freeze r0: loads through the frozen token are invariant.
    let ft0 = f.freeze(&mut m, 0, h0, t0);
    let (h1, t1) = f.region_new(&mut m, 1);
    let (q, t1) = f.alloc(&mut m, 1, h1, size, t1);
    let zero = f.iconst(0);
    let t1 = f.store(&mut m, 1, zero, q, t1);

    // loop i in 0..5: q <- load(q) + load(p) + i
    let header = f.block(&[types::I64, m.types.mem(RegionId::new(1))]);
    let body = f.block(&[]);
    let exit = f.block(&[]);
    f.jmp(header, &[zero, t1]);
    f.switch(header);
    let [i, t1h] = f.params(header)[..] else {
        unreachable!()
    };
    let n = f.iconst(5);
    let c = f.icmp(IntCc::Slt, i, n);
    f.br(c, body, &[], exit, &[]);
    f.switch(body);
    let pv = f.load(p, ft0); // loop-invariant, frozen, other-region
    let qv = f.load(q, t1h);
    let s1 = f.add_wrap(qv, pv);
    let s2 = f.add_wrap(s1, i);
    let t1b = f.store(&mut m, 1, s2, q, t1h);
    let one = f.iconst(1);
    let i2 = f.add_chk(i, one);
    f.jmp(header, &[i2, t1b]);
    f.switch(exit);
    let fin = f.load(q, t1h);
    finish_main(&mut f, fin);
    m.add_func(f.f);
    m
}

/// Shape 3 — scope duplication under loop UNROLLING (the bisected
/// rust-noalias-54878 root cause): a constant-trip loop whose
/// annotated store/load pair is cloned per unrolled iteration.
pub fn shape_unroll_scopes() -> Module {
    let mut m = Module::new();
    let main_sig = m.make_sig(vec![], vec![types::I64]);
    let mut f = Fb::new("main", main_sig, &[]);
    let size = f.iconst(8);
    let (h0, t0) = f.region_new(&mut m, 0);
    let (p, t0) = f.alloc(&mut m, 0, h0, size, t0);
    let (h1, t1) = f.region_new(&mut m, 1);
    let (q, t1) = f.alloc(&mut m, 1, h1, size, t1);
    let zero = f.iconst(0);
    let t0 = f.store(&mut m, 0, zero, p, t0);
    let t1 = f.store(&mut m, 1, zero, q, t1);

    // Constant trip count 4 — prime unroll bait. Per iteration:
    // p <- p + i (r0); q <- q + load(p) (r1); the scope pairs get
    // duplicated by the unroller.
    let header = f.block(&[
        types::I64,
        m.types.mem(RegionId::new(0)),
        m.types.mem(RegionId::new(1)),
    ]);
    let body = f.block(&[]);
    let exit = f.block(&[]);
    f.jmp(header, &[zero, t0, t1]);
    f.switch(header);
    let [i, t0h, t1h] = f.params(header)[..] else {
        unreachable!()
    };
    let four = f.iconst(4);
    let c = f.icmp(IntCc::Slt, i, four);
    f.br(c, body, &[], exit, &[]);
    f.switch(body);
    let pv = f.load(p, t0h);
    let pv2 = f.add_wrap(pv, i);
    let t0b = f.store(&mut m, 0, pv2, p, t0h);
    let pv3 = f.load(p, t0b);
    let qv = f.load(q, t1h);
    let qv2 = f.add_wrap(qv, pv3);
    let t1b = f.store(&mut m, 1, qv2, q, t1h);
    let one = f.iconst(1);
    let i2 = f.add_chk(i, one);
    f.jmp(header, &[i2, t0b, t1b]);
    f.switch(exit);
    let a = f.load(p, t0h);
    let b = f.load(q, t1h);
    let s = f.add_wrap(a, b);
    finish_main(&mut f, s);
    m.add_func(f.f);
    m
}

/// The CFG-duplication stressor: unroll + jump-thread + inline
/// compositions (report 10 delta 1's second half). A branchy helper
/// with region ops, called from a branchy constant-trip loop.
pub fn shape_cfg_duplication() -> Module {
    let mut m = Module::new();
    let helper_sig = m.make_sig(
        vec![Param::val(types::I64), Param::val(types::I64)],
        vec![types::I64],
    );
    let main_sig = m.make_sig(vec![], vec![types::I64]);

    // helper(x, sel): branch on sel&1; both arms store/load disjoint
    // regions with different weights — jump-threading fodder once
    // inlined into main's parity branch.
    let mut h = Fb::new("helper2", helper_sig, &[types::I64, types::I64]);
    let [x, sel] = h.params(h.cur)[..] else {
        unreachable!()
    };
    let size = h.iconst(8);
    let (h0, t0) = h.region_new(&mut m, 0);
    let (p, t0) = h.alloc(&mut m, 0, h0, size, t0);
    let (h1, t1) = h.region_new(&mut m, 1);
    let (q, t1) = h.alloc(&mut m, 1, h1, size, t1);
    let one = h.iconst(1);
    let parity = h.band(sel, one);
    let zero = h.iconst(0);
    let c = h.icmp(IntCc::Eq, parity, zero);
    let even = h.block(&[]);
    let odd = h.block(&[]);
    let join = h.block(&[types::I64]);
    h.br(c, even, &[], odd, &[]);
    h.switch(even);
    let t0e = h.store(&mut m, 0, x, p, t0);
    let _t1e = h.store(&mut m, 1, one, q, t1);
    let pv = h.load(p, t0e);
    let two = h.iconst(2);
    let ev = h.ins(Opcode::ImulWrap, &[pv, two], &[types::I64], Aux::None)[0];
    h.jmp(join, &[ev]);
    h.switch(odd);
    let t1o = h.store(&mut m, 1, x, q, t1);
    let _t0o = h.store(&mut m, 0, one, p, t0);
    let qv = h.load(q, t1o);
    let three = h.iconst(3);
    let ov = h.add_wrap(qv, three);
    h.jmp(join, &[ov]);
    h.switch(join);
    let jv = h.params(join)[0];
    h.ret(jv);
    m.add_func(h.f);

    // main: constant-trip loop over i in 0..8 with its OWN parity
    // branch selecting helper arguments — unroll makes the parity
    // constant per clone, jump-threading collapses it, inlining pulls
    // the helper's scoped ops into every clone.
    let mut f = Fb::new("main", main_sig, &[]);
    let zero = f.iconst(0);
    let header = f.block(&[types::I64, types::I64]);
    let body = f.block(&[]);
    let join = f.block(&[types::I64]);
    let exit = f.block(&[]);
    f.jmp(header, &[zero, zero]);
    f.switch(header);
    let [i, acc] = f.params(header)[..] else {
        unreachable!()
    };
    let eight = f.iconst(8);
    let c = f.icmp(IntCc::Slt, i, eight);
    f.br(c, body, &[], exit, &[]);
    f.switch(body);
    let one = f.iconst(1);
    let par = f.band(i, one);
    let z2 = f.iconst(0);
    let pc = f.icmp(IntCc::Eq, par, z2);
    let ab = f.block(&[]);
    let bb = f.block(&[]);
    f.br(pc, ab, &[], bb, &[]);
    let callee = f.f.import_func("helper2", helper_sig);
    f.switch(ab);
    let va = f.ins(Opcode::Call, &[i, i], &[types::I64], Aux::Callee(callee))[0];
    f.jmp(join, &[va]);
    f.switch(bb);
    let ip1 = f.add_wrap(i, one);
    let vb = f.ins(Opcode::Call, &[ip1, i], &[types::I64], Aux::Callee(callee))[0];
    f.jmp(join, &[vb]);
    f.switch(join);
    let jv = f.params(join)[0];
    let acc2 = f.add_wrap(acc, jv);
    let i2 = f.add_chk(i, one);
    f.jmp(header, &[i2, acc2]);
    f.switch(exit);
    finish_main(&mut f, acc);
    m.add_func(f.f);
    m
}

/// The four permanent regression shapes, named.
pub fn historical_shapes() -> Vec<(&'static str, Module)> {
    vec![
        ("inline_noalias", shape_inline_noalias()),
        ("licm_scopes", shape_licm_scopes()),
        ("unroll_scopes", shape_unroll_scopes()),
        ("cfg_duplication", shape_cfg_duplication()),
    ]
}

/// Deterministic xorshift64* PRNG.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// A seeded random program: 2–3 disjoint regions with a few slots
/// each, optional freeze of one region, and a constant-trip loop of
/// cross-region loads/stores/checked adds; result folds every slot.
pub fn random_program(seed: u64) -> Module {
    let mut rng = Rng::new(seed);
    let mut m = Module::new();
    let main_sig = m.make_sig(vec![], vec![types::I64]);
    let mut f = Fb::new("main", main_sig, &[]);

    let nregions = 2 + rng.below(2) as u32; // 2..=3
    let slots_per = 1 + rng.below(3) as usize; // 1..=3
    let size = f.iconst(8);
    // Per region: handle, current token, slot ptrs.
    let mut handles = Vec::new();
    let mut toks = Vec::new();
    let mut slots: Vec<Vec<Value>> = Vec::new();
    for r in 0..nregions {
        let (h, mut t) = f.region_new(&mut m, r);
        let mut ps = Vec::new();
        for k in 0..slots_per {
            let (p, t2) = f.alloc(&mut m, r, h, size, t);
            t = t2;
            let init = f.iconst((seed as i64).wrapping_add(k as i64) & 15);
            t = f.store(&mut m, r, init, p, t);
            ps.push(p);
        }
        handles.push(h);
        toks.push(t);
        slots.push(ps);
    }
    // Maybe freeze region 0 (its loads become invariant; it takes no
    // further stores).
    let frozen0 = rng.below(2) == 0;
    if frozen0 {
        toks[0] = f.freeze(&mut m, 0, handles[0], toks[0]);
    }

    // Loop header params: i, acc, one token per MUTABLE region. A
    // frozen token is never invalidated and never consumable, so it
    // must NOT ride block-arg lists (consuming positions) — its one
    // SSA value serves every load forever.
    let carried: Vec<usize> = (0..nregions as usize)
        .filter(|&r| !(frozen0 && r == 0))
        .collect();
    let mut header_tys = vec![types::I64, types::I64];
    for &r in &carried {
        header_tys.push(m.types.mem(RegionId::new(r as u32)));
    }
    let header = f.block(&header_tys);
    let body = f.block(&[]);
    let exit = f.block(&[]);
    let zero = f.iconst(0);
    let mut entry_args = vec![zero, zero];
    entry_args.extend(carried.iter().map(|&r| toks[r]));
    f.jmp(header, &entry_args);
    f.switch(header);
    let hp = f.params(header);
    let i = hp[0];
    let acc = hp[1];
    // Region index → its current token in the loop body.
    let mut cur_toks: Vec<Value> = toks.clone();
    for (k, &r) in carried.iter().enumerate() {
        cur_toks[r] = hp[2 + k];
    }
    let trip = f.iconst(3 + rng.below(5) as i64); // 3..=7
    let c = f.icmp(IntCc::Slt, i, trip);
    f.br(c, body, &[], exit, &[]);
    f.switch(body);
    let mut acc2 = acc;
    let nops = 2 + rng.below(4); // 2..=5 memory ops per iteration
    for _ in 0..nops {
        let r = rng.below(nregions as u64) as usize;
        let s = rng.below(slots_per as u64) as usize;
        let p = slots[r][s];
        let writable = !(frozen0 && r == 0);
        if writable && rng.below(2) == 0 {
            // store: slot <- load(slot') + i  (slot' from any region)
            let r2 = rng.below(nregions as u64) as usize;
            let s2 = rng.below(slots_per as u64) as usize;
            let v = f.load(slots[r2][s2], cur_toks[r2]);
            let v2 = f.add_wrap(v, i);
            cur_toks[r] = f.store(&mut m, r as u32, v2, p, cur_toks[r]);
        } else {
            // load-and-accumulate (checked add on small values).
            let v = f.load(p, cur_toks[r]);
            let masked = {
                let mk = f.iconst(1023);
                f.band(v, mk)
            };
            acc2 = f.add_chk(acc2, masked);
        }
    }
    let one = f.iconst(1);
    let i2 = f.add_chk(i, one);
    let mut latch_args = vec![i2, acc2];
    latch_args.extend(carried.iter().map(|&r| cur_toks[r]));
    f.jmp(header, &latch_args);
    f.switch(exit);
    // Fold every slot into the result (header-param tokens dominate
    // the exit; frozen tokens are valid everywhere).
    let mut fin = acc;
    let mut exit_toks: Vec<Value> = toks.clone();
    for (k, &r) in carried.iter().enumerate() {
        exit_toks[r] = hp[2 + k];
    }
    for (r, ps) in slots.iter().enumerate() {
        for &p in ps {
            let v = f.load(p, exit_toks[r]);
            fin = f.add_wrap(fin, v);
        }
    }
    finish_main(&mut f, fin);
    m.add_func(f.f);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_verify() {
        for (name, m) in historical_shapes() {
            wolf_wir::verify_module(&m).unwrap_or_else(|e| panic!("shape {name}: {e}"));
        }
    }

    #[test]
    fn random_programs_verify_and_are_deterministic() {
        for seed in 1..=16u64 {
            let m = random_program(seed);
            wolf_wir::verify_module(&m).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            let a = wolf_wir::print_module(&m);
            let b = wolf_wir::print_module(&random_program(seed));
            assert_eq!(a, b, "seed {seed} must be deterministic");
        }
    }
}
