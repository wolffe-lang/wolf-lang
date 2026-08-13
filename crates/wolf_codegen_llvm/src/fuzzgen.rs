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
//! 5. **Loaded-pointer scopes** (s78, wolf-lang#82): the accessed
//!    pointer is READ OUT OF another region's memory — the container
//!    shape, where a wrong scope is a wrong answer about the very
//!    pointer #82 is about.
//! 6. **Duplicated foreign roots** (s80, wolf-lang#83): two
//!    `region.foreign` roots of one role over one piece of storage —
//!    what inlining a container-touching callee produces. This one is
//!    not a historical LLVM bug but OUR OWN, found by s80 and fixed in
//!    the same commit: it miscompiled.
//! 7. **Foreign storage across a non-inlining call** (s80): an opaque
//!    callee writes the caller's foreign memory holding none of its
//!    tokens. Deliberately too large to inline — the shapes that hid
//!    this hazard hid it by inlining first.
//!
//! 8. **The call-site `!noalias` fact** (s83, wolf-lang#92): the fact
//!    s78 declined, in the half that has a theorem — a call carries
//!    `!noalias` over the regions it provably cannot reach. Two shapes,
//!    because one alone proves nothing. `call_escaped_pointer` is the
//!    guard: the caller hands a RAW POINTER across and the callee
//!    writes through it, so the region must NOT be listed and the
//!    reload must happen. `call_noalias_local` is the claim: a second
//!    region that never leaves the frame, whose load may forward across
//!    the same call. Drop the fact and 8b goes quiet; widen it by one
//!    region and 8a fails.
//!
//! Shapes 6, 7 and 8 are mid-end findings, so the rig runs them through
//! [`wolf_wir::midend`] as well as straight to the emitter: a lane that
//! never optimizes cannot witness an optimizer bug.
//!
//! [`random_program`] extends the corpus with seeded multi-region
//! load/store/checked-arith loops (deterministic xorshift; same seed,
//! same program).

use wolf_wir::entity::EntityRef;
use wolf_wir::ir::{Aux, Block, Function, Module, Param, Value};
use wolf_wir::ops::{ForeignRole, IntCc, Opcode};
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

    /// `load.ptr`: a POINTER read out of region memory (s78 — the
    /// container-header shape, where the accessed pointer is a loaded
    /// value and not an allocation result).
    fn load_ptr(&mut self, p: Value, tok: Value) -> Value {
        self.ins(Opcode::Load, &[p, tok], &[types::PTR], Aux::None)[0]
    }

    /// `ptr.off p, i, scale`.
    fn ptr_off(&mut self, p: Value, i: Value, scale: u64) -> Value {
        self.ins(Opcode::PtrOff, &[p, i], &[types::PTR], Aux::Scale(scale))[0]
    }

    /// `%m: mem.rN = region.foreign ROLE` — a root over runtime-owned
    /// storage (s75), carrying its role (s80).
    fn region_foreign(&mut self, m: &mut Module, r: u32, role: ForeignRole) -> Value {
        let tok = m.types.mem(RegionId::new(r));
        self.ins(Opcode::RegionForeign, &[], &[tok], Aux::Int(role.code()))[0]
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

/// Shape 5 — the LOADED-POINTER shape (s78, wolf-lang#82): the
/// accessed pointer is not an allocation result but a value read out of
/// another region's memory, which is how every container access looks
/// after s75 (`data` field in the header region, elements in the buffer
/// region). The two regions really are disjoint bump arenas, so the
/// scope pair is a theorem; what it licenses is exactly the motion the
/// container shape wants — an element store cannot clobber the header,
/// so the header load hoists over it, while the header's OWN store (in
/// the same region, same scope) still blocks it.
///
/// This is the shape #82 is about, and it is here because target 2 of
/// s78 says every fact the emitter attaches gets fuzzed like the old
/// ones: the scopes on these loads and stores are the ones a wrong
/// answer would miscompile.
pub fn shape_loaded_pointer_scopes() -> Module {
    let mut m = Module::new();
    let main_sig = m.make_sig(vec![], vec![types::I64]);
    let mut f = Fb::new("main", main_sig, &[]);
    let hdr_size = f.iconst(16);
    let buf_size = f.iconst(32);
    let eight = f.iconst(8);
    let zero = f.iconst(0);
    let one = f.iconst(1);
    // r0: the "header" region, holding a pointer and a counter.
    let (h0, t0) = f.region_new(&mut m, 0);
    let (hdr, t0) = f.alloc(&mut m, 0, h0, hdr_size, t0);
    // r1: the "buffer" region, holding four elements.
    let (h1, t1) = f.region_new(&mut m, 1);
    let (buf, t1) = f.alloc(&mut m, 1, h1, buf_size, t1);
    // header.data = buf (a pointer INTO r1 living in r0), counter = 0.
    let t0 = f.store(&mut m, 0, buf, hdr, t0);
    let cnt = f.ptr_off(hdr, one, 8);
    let t0 = f.store(&mut m, 0, zero, cnt, t0);
    let seven = f.iconst(7);
    let t1 = f.store(&mut m, 1, seven, buf, t1);

    // loop i in 0..4: read the pointer out of r0, touch element i of
    // r1 through it, then bump the r0 counter.
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
    let data = f.load_ptr(hdr, t0h); // the container-header load
    let elem = f.ptr_off(data, i, 8);
    let ev = f.load(elem, t1h);
    let ev2 = f.add_wrap(ev, i);
    let t1b = f.store(&mut m, 1, ev2, elem, t1h);
    let cv = f.load(cnt, t0h);
    let cv2 = f.add_wrap(cv, one);
    let t0b = f.store(&mut m, 0, cv2, cnt, t0h);
    let i2 = f.add_chk(i, one);
    f.jmp(header, &[i2, t0b, t1b]);
    f.switch(exit);
    // Fold the counter and every element (through the loaded pointer
    // again — the exit's tokens are the header params').
    let mut fin = f.load(cnt, t0h);
    let data = f.load_ptr(hdr, t0h);
    for k in 0..4i64 {
        let ki = f.iconst(k);
        let e = f.ptr_off(data, ki, 8);
        let v = f.load(e, t1h);
        fin = f.add_wrap(fin, v);
    }
    let _ = eight;
    finish_main(&mut f, fin);
    m.add_func(f.f);
    m
}

/// Shape 6 — TWO foreign roots of one role over ONE piece of storage
/// (s80, wolf-lang#83). This is the state the inliner produces the
/// moment a container-touching callee is spliced in: the caller has its
/// own `region.foreign` root and the callee's freshened one, and both
/// name the runtime's element buffers.
///
/// It was a MISCOMPILE, not a hazard. One `!alias.scope` per region id
/// declared the two roots `!noalias`, and LLVM cashed it — the load
/// through root B forwarded across the store through root A, printing a
/// stale value. The mid-end had the same hole one level down: `memopt`
/// keys availability on the token version, and a store on A's chain
/// versions nothing on B's.
///
/// The third root here is a HEADER-role root, and it is load-bearing in
/// the other direction: the fix must NOT collapse every foreign region
/// into one scope, because header/buffer separation is a theorem (D46)
/// and s75's whole win rests on it. This shape passes only if same-role
/// roots alias and different-role roots do not.
///
/// Storage comes from a `region.new` arena and is then touched ONLY
/// through foreign tokens, so the program makes no claim it cannot back
/// — the arena's own scope never appears on an access.
pub fn shape_foreign_dup_roots() -> Module {
    let mut m = Module::new();
    let main_sig = m.make_sig(vec![], vec![types::I64]);
    let mut f = Fb::new("main", main_sig, &[]);
    // r0 headers, r1 + r2 buffers: r1 is "the caller's" root, r2 is the
    // one an inline would have minted. Same bytes, both of them.
    let fh = f.region_foreign(&mut m, 0, ForeignRole::Header);
    let fb_a = f.region_foreign(&mut m, 1, ForeignRole::Buffer);
    let fb_b = f.region_foreign(&mut m, 2, ForeignRole::Buffer);
    let size = f.iconst(8);
    let (h, t) = f.region_new(&mut m, 3);
    let (hdr, t) = f.alloc(&mut m, 3, h, size, t);
    let (buf, _t) = f.alloc(&mut m, 3, h, size, t);
    let three = f.iconst(3);
    let five = f.iconst(5);
    let nine = f.iconst(9);
    let fh1 = f.store(&mut m, 0, three, hdr, fh);
    let fb_a1 = f.store(&mut m, 1, five, buf, fb_a);
    // Through the OTHER buffer root: must see 5.
    let x = f.load(buf, fb_b);
    let _fb_a2 = f.store(&mut m, 1, nine, buf, fb_a1);
    // Same address, same token as `x` — and yet it must RELOAD, because
    // the store in between wrote this very memory through a root whose
    // chain this token is not on.
    let y = f.load(buf, fb_b);
    // The header read is the control: nothing wrote it, and the
    // header/buffer disjointness is still claimed.
    let hv = f.load(hdr, fh1);
    let s1 = f.add_wrap(x, y);
    let s2 = f.add_wrap(s1, hv);
    finish_main(&mut f, s2); // 5 + 9 + 3 = 17
    m.add_func(f.f);
    m
}

/// Shape 6b — [`shape_foreign_dup_roots`] with the two accesses reached
/// through addresses LLVM cannot prove equal (s80). This is the half
/// that indicts the METADATA: with one SSA address, basic AA answers
/// MustAlias and never consults `!alias.scope` at all, which is exactly
/// why the original source witness had to index by two opaque-but-equal
/// values before the miscompile showed itself.
///
/// `__wolf_rt_test_opaque` is the identity, in the RT stub's own
/// translation unit. Both indices are 0, so both addresses are element
/// 0 of the same buffer; nothing in the IR says so.
pub fn shape_foreign_dup_roots_opaque_index() -> Module {
    let mut m = Module::new();
    let main_sig = m.make_sig(vec![], vec![types::I64]);
    let opaque_sig = m.make_sig(vec![Param::val(types::I64)], vec![types::I64]);
    let mut f = Fb::new("main", main_sig, &[]);
    let fb_a = f.region_foreign(&mut m, 0, ForeignRole::Buffer);
    let fb_b = f.region_foreign(&mut m, 1, ForeignRole::Buffer);
    let size = f.iconst(32);
    let (h, t) = f.region_new(&mut m, 2);
    let (buf, _t) = f.alloc(&mut m, 2, h, size, t);
    let zero = f.iconst(0);
    let opaque = f.f.import_func("__wolf_rt_test_opaque", opaque_sig);
    let i = f.ins(Opcode::Call, &[zero], &[types::I64], Aux::Callee(opaque))[0];
    let j = f.ins(Opcode::Call, &[zero], &[types::I64], Aux::Callee(opaque))[0];
    let pa = f.ptr_off(buf, i, 8);
    let pb = f.ptr_off(buf, j, 8);
    let five = f.iconst(5);
    let nine = f.iconst(9);
    let fa1 = f.store(&mut m, 0, five, pa, fb_a);
    let x = f.load(pb, fb_b); // 5
    let _fa2 = f.store(&mut m, 0, nine, pa, fa1);
    let y = f.load(pb, fb_b); // 9 — the load the false !noalias killed
    let s = f.add_wrap(x, y);
    finish_main(&mut f, s); // 14
    m.add_func(f.f);
    m
}

/// Shape 7 — a NON-INLINING callee that writes the caller's foreign
/// storage while holding none of the caller's tokens (s80,
/// wolf-lang#83). This is `@stencil` in the kernel suite, stripped to
/// its bones and made large enough that the inliner declines it: the
/// shapes that hid this hazard hid it by inlining first, so the witness
/// exists precisely to stop them.
///
/// The callee mints its OWN buffer-role root and stores through it. The
/// caller's foreign token is not an argument, is not consumed, and is
/// not re-minted — so on the mid-end's headline claim ("a call that
/// does not consume a region's token cannot touch that region") the
/// caller's load may forward across the call. It may not: the claim is
/// a theorem for `region.new`/`stack.alloc`/parameter regions and false
/// for this one.
pub fn shape_foreign_cross_call() -> Module {
    let mut m = Module::new();
    let writer_sig = m.make_sig(vec![Param::val(types::PTR)], vec![types::I64]);
    let main_sig = m.make_sig(vec![], vec![types::I64]);

    // writer(p): store 9 through its own foreign root, then pad well
    // past `inline_single_use` (96) plus every bonus, so the decision
    // is "too big" rather than "we got lucky".
    let mut w = Fb::new("writer", writer_sig, &[types::PTR]);
    let p = w.params(w.cur)[0];
    let wfb = w.region_foreign(&mut m, 0, ForeignRole::Buffer);
    let seed = w.load(p, wfb);
    let acc = inline_proof_padding(&mut w, seed);
    let nine = w.iconst(9);
    let _t = w.store(&mut m, 0, nine, p, wfb);
    w.ret(acc);
    m.add_func(w.f);

    let mut f = Fb::new("main", main_sig, &[]);
    let fb = f.region_foreign(&mut m, 0, ForeignRole::Buffer);
    let size = f.iconst(8);
    let (h, t) = f.region_new(&mut m, 1);
    let (buf, _t) = f.alloc(&mut m, 1, h, size, t);
    let five = f.iconst(5);
    let fb1 = f.store(&mut m, 0, five, buf, fb);
    let a = f.load(buf, fb1);
    let callee = f.f.import_func("writer", writer_sig);
    f.ins(Opcode::Call, &[buf], &[types::I64], Aux::Callee(callee));
    // Same address, same token, and the call consumed nothing — the
    // load must still reload. It reads 9.
    let b = f.load(buf, fb1);
    let eight = f.iconst(8);
    let sa = f.ins(Opcode::ImulWrap, &[a, eight], &[types::I64], Aux::None)[0];
    let s = f.add_wrap(sa, b);
    finish_main(&mut f, s); // 5*8 + 9 = 49
    m.add_func(f.f);
    m
}

/// Shape 7b — [`shape_foreign_cross_call`] with the call inside a LOOP
/// (s80). This is `licm`'s half of the same claim: the loop's foreign
/// token is defined outside it and never consumed inside, so on the
/// pass's own test the load is loop-invariant and hoists to the
/// preheader — over a call that writes the very bytes it reads. The
/// token being loop-invariant is not the same as the memory being
/// loop-invariant, and for a foreign root the two come apart.
///
/// The buffer holds a counter the callee bumps; the loop reads it each
/// iteration and sums. A hoisted load sums the same value four times.
pub fn shape_foreign_licm_call() -> Module {
    let mut m = Module::new();
    let bump_sig = m.make_sig(vec![Param::val(types::PTR)], vec![types::I64]);
    let main_sig = m.make_sig(vec![], vec![types::I64]);

    // bump(p): *p += 1 through its own foreign root, padded past every
    // inline budget so the call survives into the loop.
    let mut w = Fb::new("bump", bump_sig, &[types::PTR]);
    let p = w.params(w.cur)[0];
    let wfb = w.region_foreign(&mut m, 0, ForeignRole::Buffer);
    let cur = w.load(p, wfb);
    let one = w.iconst(1);
    let next = w.add_wrap(cur, one);
    let t = w.store(&mut m, 0, next, p, wfb);
    let seed = w.load(p, t);
    let acc = inline_proof_padding(&mut w, seed);
    w.ret(acc);
    m.add_func(w.f);

    let mut f = Fb::new("main", main_sig, &[]);
    let fb = f.region_foreign(&mut m, 0, ForeignRole::Buffer);
    let size = f.iconst(8);
    let (h, t) = f.region_new(&mut m, 1);
    let (buf, _t) = f.alloc(&mut m, 1, h, size, t);
    let zero = f.iconst(0);
    let fb1 = f.store(&mut m, 0, zero, buf, fb);
    let header = f.block(&[types::I64, types::I64]); // (i, acc)
    let body = f.block(&[]);
    let exit = f.block(&[]);
    f.jmp(header, &[zero, zero]);
    f.switch(header);
    let [i, acc] = f.params(header)[..] else {
        unreachable!()
    };
    let four = f.iconst(4);
    let c = f.icmp(IntCc::Slt, i, four);
    f.br(c, body, &[], exit, &[]);
    f.switch(body);
    // Address and token both defined outside the loop — and the value
    // still changes every iteration, because the call writes it.
    let v = f.load(buf, fb1);
    let acc2 = f.add_wrap(acc, v);
    let callee = f.f.import_func("bump", bump_sig);
    f.ins(Opcode::Call, &[buf], &[types::I64], Aux::Callee(callee));
    let one = f.iconst(1);
    let i2 = f.add_chk(i, one);
    f.jmp(header, &[i2, acc2]);
    f.switch(exit);
    finish_main(&mut f, acc); // 0 + 1 + 2 + 3 = 6, not 0
    m.add_func(f.f);
    m
}

/// Padding that survives the callee's own optimization, so a witness
/// callee stays too big to inline. The inliner sizes a callee AFTER
/// simplify folds and DCE, so a constant chain would vanish and the
/// shape would inline after all: seed on a load with nothing to forward
/// from, chain on it, and return the result so nothing is dead.
fn inline_proof_padding(w: &mut Fb, seed: Value) -> Value {
    let mut acc = seed;
    for k in 1..120i64 {
        let c = w.iconst(k);
        acc = w.ins(Opcode::Bxor, &[acc, c], &[types::I64], Aux::None)[0];
        acc = w.add_wrap(acc, c);
    }
    acc
}

/// Shape 8a — the call-site `!noalias` fact's ESCAPE guard (s83,
/// wolf-lang#92). The mirror image of shape 7: there the callee reached
/// the caller's storage by minting a foreign root; here it reaches an
/// ordinary `region.new` region through a RAW POINTER the caller handed
/// it. The caller's token is not an argument and is not versioned, so
/// both `memopt`'s "no token ⇒ no effect" and the emitter's call-site
/// `!noalias` would license forwarding the pre-call load across it.
///
/// Neither may. `dse_dying_regions` has refused to touch an escaped
/// region since s42; `rle_and_forward` did not, and the emitter would
/// have handed the same false claim to LLVM. The load must reload and
/// read 9, so the answer is 5*8 + 9 = 49; a stale forward gives 45.
pub fn shape_call_escaped_pointer() -> Module {
    let mut m = Module::new();
    let writer_sig = m.make_sig(vec![Param::val(types::PTR)], vec![types::I64]);
    let main_sig = m.make_sig(vec![], vec![types::I64]);

    // writer(p): *p = 9 through its own foreign root — an opaque callee
    // that writes memory it was only ever handed the address of.
    let mut w = Fb::new("writer", writer_sig, &[types::PTR]);
    let p = w.params(w.cur)[0];
    let wfb = w.region_foreign(&mut m, 0, ForeignRole::Buffer);
    let seed = w.load(p, wfb);
    let acc = inline_proof_padding(&mut w, seed);
    let nine = w.iconst(9);
    let _t = w.store(&mut m, 0, nine, p, wfb);
    w.ret(acc);
    m.add_func(w.f);

    let mut f = Fb::new("main", main_sig, &[]);
    let size = f.iconst(8);
    let (h, t) = f.region_new(&mut m, 0);
    let (buf, t1) = f.alloc(&mut m, 0, h, size, t);
    let five = f.iconst(5);
    let t2 = f.store(&mut m, 0, five, buf, t1);
    let a = f.load(buf, t2);
    let callee = f.f.import_func("writer", writer_sig);
    f.ins(Opcode::Call, &[buf], &[types::I64], Aux::Callee(callee));
    // Same address, same token, and the call took no token — but the
    // ADDRESS went across, which is the whole point.
    let b = f.load(buf, t2);
    let eight = f.iconst(8);
    let sa = f.ins(Opcode::ImulWrap, &[a, eight], &[types::I64], Aux::None)[0];
    let s = f.add_wrap(sa, b);
    finish_main(&mut f, s); // 5*8 + 9 = 49
    m.add_func(f.f);
    m
}

/// Shape 8b — the call-site `!noalias` fact where it is TRUE (s83).
///
/// Two local regions. `r1`'s pointer crosses to an opaque callee that
/// writes it; `r0`'s never leaves the frame, its handle only ever
/// allocates, and its token is not among the call's arguments — so
/// nothing the callee can do reaches `r0`, and the call carries `r0`'s
/// scope in `!noalias`. The load of `r0` across the call may forward;
/// the load of `r1` may not. 5 + 5 + 9 = 19, and a lane that got either
/// half wrong reads 5+9+9=23 or 5+5+7=17.
///
/// This is the shape that would go quiet if the fact were dropped and
/// LOUD if it were widened to a region the callee can reach, which is
/// what makes it worth keeping around.
pub fn shape_call_noalias_local() -> Module {
    let mut m = Module::new();
    let writer_sig = m.make_sig(vec![Param::val(types::PTR)], vec![types::I64]);
    let main_sig = m.make_sig(vec![], vec![types::I64]);

    let mut w = Fb::new("writer", writer_sig, &[types::PTR]);
    let p = w.params(w.cur)[0];
    let wfb = w.region_foreign(&mut m, 0, ForeignRole::Buffer);
    let seed = w.load(p, wfb);
    let acc = inline_proof_padding(&mut w, seed);
    let nine = w.iconst(9);
    let _t = w.store(&mut m, 0, nine, p, wfb);
    w.ret(acc);
    m.add_func(w.f);

    let mut f = Fb::new("main", main_sig, &[]);
    let size = f.iconst(8);
    let (h0, t0) = f.region_new(&mut m, 0);
    let (p0, t0a) = f.alloc(&mut m, 0, h0, size, t0);
    let (h1, t1) = f.region_new(&mut m, 1);
    let (p1, t1a) = f.alloc(&mut m, 1, h1, size, t1);
    let five = f.iconst(5);
    let seven = f.iconst(7);
    let t0b = f.store(&mut m, 0, five, p0, t0a);
    let t1b = f.store(&mut m, 1, seven, p1, t1a);
    let a = f.load(p0, t0b);
    let callee = f.f.import_func("writer", writer_sig);
    f.ins(Opcode::Call, &[p1], &[types::I64], Aux::Callee(callee));
    let b = f.load(p0, t0b); // unreachable by the callee: may forward
    let c = f.load(p1, t1b); // the callee wrote it: must reload → 9
    let ab = f.add_wrap(a, b);
    let s = f.add_wrap(ab, c);
    finish_main(&mut f, s); // 5 + 5 + 9 = 19
    m.add_func(f.f);
    m
}

/// The eleven permanent regression shapes, named.
pub fn historical_shapes() -> Vec<(&'static str, Module)> {
    vec![
        ("inline_noalias", shape_inline_noalias()),
        ("licm_scopes", shape_licm_scopes()),
        ("unroll_scopes", shape_unroll_scopes()),
        ("cfg_duplication", shape_cfg_duplication()),
        ("loaded_pointer", shape_loaded_pointer_scopes()),
        ("foreign_dup_roots", shape_foreign_dup_roots()),
        (
            "foreign_dup_roots_opaque",
            shape_foreign_dup_roots_opaque_index(),
        ),
        ("foreign_cross_call", shape_foreign_cross_call()),
        ("foreign_licm_call", shape_foreign_licm_call()),
        ("call_escaped_pointer", shape_call_escaped_pointer()),
        ("call_noalias_local", shape_call_noalias_local()),
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
///
/// Some accesses go INDIRECT (s78): region `r`'s slot 0 has its address
/// parked in a handle slot living in the NEXT region, and an access may
/// read the address back out of that handle before using it. The
/// address is the same either way, so the program's value is unchanged
/// — what changes is that the accessed pointer is a loaded value whose
/// scope came from the emitter's token reasoning rather than from an
/// allocation site.
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
    // Address slots (s78): region r's slot 0 has its ADDRESS parked in
    // the next region's memory, so a later access can read the pointer
    // back out instead of naming the allocation. Set up before any
    // freeze — a frozen region takes no further stores.
    let mut addr_slot: Vec<Value> = Vec::new();
    for (r, ps) in slots.iter().enumerate() {
        let hr = (r + 1) % nregions as usize;
        let (ap, t2) = f.alloc(&mut m, hr as u32, handles[hr], size, toks[hr]);
        toks[hr] = f.store(&mut m, hr as u32, ps[0], ap, t2);
        addr_slot.push(ap);
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
        // Slot 0 is reachable two ways: by name, or by reading its
        // address out of the next region's memory. Same address, same
        // program value — a different provenance for the emitter.
        let p = if s == 0 && rng.below(2) == 0 {
            let hr = (r + 1) % nregions as usize;
            f.load_ptr(addr_slot[r], cur_toks[hr])
        } else {
            slots[r][s]
        };
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
