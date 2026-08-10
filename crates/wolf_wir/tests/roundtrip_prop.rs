//! Round-trip as a PROPERTY over generated modules (deterministic
//! seeds — no flaky randomness in CI): every generated module
//! verifies, `print → parse → print` is a byte-identical fixpoint, and
//! the re-parsed module verifies too. Plus parser robustness: random
//! garbage and mutated canonical text must never panic (the same
//! surface the s01 fuzz scaffold's `wir_parse` target hammers).

use wolf_wir::types::{BOOL, I64, PTR};
use wolf_wir::{
    Aux, DerefSize, FactData, FactKind, Function, IntCc, Just, Mode, Module, Opcode, Param,
    RegionId, Theorem, Value, entity::EntityRef,
};

/// xorshift64* — tiny, deterministic, good enough for structure fuzz.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn flip(&mut self) -> bool {
        self.next() & 1 == 0
    }
}

/// Generate a verifier-green module: entry block with consts, a store
/// chain (linear tokens), facts over the pointer params, then a
/// forward chain of blocks with random arithmetic, loads, and
/// conditional branches; the last block returns.
fn gen_module(seed: u64) -> Module {
    let mut rng = Rng::new(seed);
    let mut m = Module::new();
    let mem0 = m.types.mem(RegionId::new(0));
    let sig = m.make_sig(
        vec![
            Param {
                ty: PTR,
                mode: Mode::Mut,
            },
            Param {
                ty: PTR,
                mode: Mode::Read,
            },
            Param::val(I64),
            Param::val(mem0),
        ],
        vec![I64],
    );
    let mut f = Function::new(format!("gen{seed}"), sig);
    let entry = f.make_block(&[PTR, PTR, I64, mem0]);
    let ep = f.block_params(entry);
    let (p, q, n, m0) = (ep[0], ep[1], ep[2], ep[3]);

    // Entry: integer constants (some with exact op-justified ranges).
    let mut ints: Vec<Value> = vec![n];
    for _ in 0..rng.below(4) {
        let c = rng.below(200) as i64 - 100;
        let (_, r) = f.append_inst(entry, Opcode::Iconst, &[], &[I64], Aux::Int(c));
        ints.push(r[0]);
        if rng.flip() {
            let slack = rng.below(8) as i128;
            f.add_fact(FactData::new(
                FactKind::Range(r[0], c as i128 - slack, c as i128 + slack),
                Just::DefOp,
            ));
        }
    }
    // Entry: a store chain — each store consumes the previous token.
    let mut token = m0;
    for _ in 0..rng.below(3) {
        let idx = ints[rng.below(ints.len() as u64) as usize];
        let (_, addr) = f.append_inst(entry, Opcode::PtrOff, &[p, idx], &[PTR], Aux::Scale(8));
        let v = ints[rng.below(ints.len() as u64) as usize];
        let (_, t2) = f.append_inst(
            entry,
            Opcode::Store,
            &[v, addr[0], token],
            &[mem0],
            Aux::None,
        );
        token = t2[0];
    }
    // Facts over the pointer params (theorem-justified).
    if rng.flip() {
        f.add_fact(FactData::new(
            FactKind::Noalias(p, q),
            Just::Theorem(Theorem::ExclMut),
        ));
    }
    if rng.flip() {
        f.add_fact(FactData::new(
            FactKind::Deref(p, DerefSize::Scaled { elem: 8, count: n }),
            Just::Theorem(Theorem::ExclMut),
        ));
    }
    if rng.flip() {
        f.add_fact(FactData::new(
            FactKind::Frozen(q),
            Just::Theorem(Theorem::FrozenRead),
        ));
    }
    if rng.flip() {
        f.add_fact(FactData::new(
            FactKind::Region(p, RegionId::new(0)),
            Just::Theorem(Theorem::RegionAlloc),
        ));
    }

    // A forward chain of paramless blocks; every block is reachable
    // through its predecessor's then-edge.
    let extra = rng.below(4) as usize;
    let mut chain = vec![entry];
    for _ in 0..extra {
        chain.push(f.make_block(&[]));
    }
    for k in 0..chain.len() {
        let b = chain[k];
        // Body (skip for entry, already filled).
        let mut avail = ints.clone();
        if k > 0 {
            for _ in 0..rng.below(4) {
                let a = avail[rng.below(avail.len() as u64) as usize];
                let c = avail[rng.below(avail.len() as u64) as usize];
                let op = match rng.below(4) {
                    0 => Opcode::IaddChk,
                    1 => Opcode::IsubWrap,
                    2 => Opcode::ImulSat,
                    _ => Opcode::Bxor,
                };
                let (_, r) = f.append_inst(b, op, &[a, c], &[I64], Aux::None);
                avail.push(r[0]);
            }
            if rng.flip() {
                let idx = avail[rng.below(avail.len() as u64) as usize];
                let (_, addr) = f.append_inst(b, Opcode::PtrOff, &[q, idx], &[PTR], Aux::Scale(8));
                let (_, r) = f.append_inst(b, Opcode::Load, &[addr[0], token], &[I64], Aux::None);
                avail.push(r[0]);
            }
        }
        // Terminator.
        if k + 1 == chain.len() {
            let rv = avail[rng.below(avail.len() as u64) as usize];
            f.append_inst(b, Opcode::Ret, &[rv], &[], Aux::None);
        } else {
            let next = chain[k + 1];
            if rng.flip() {
                let edge = f.block_call(next, &[]);
                f.append_inst(b, Opcode::Jmp, &[], &[], Aux::Jump(edge));
            } else {
                let a = avail[rng.below(avail.len() as u64) as usize];
                let c = avail[rng.below(avail.len() as u64) as usize];
                let (_, cond) =
                    f.append_inst(b, Opcode::Icmp, &[a, c], &[BOOL], Aux::IntCc(IntCc::Slt));
                // Else-edge goes anywhere forward (the then-edge keeps
                // the chain reachable).
                let j = k + 1 + rng.below((chain.len() - k - 1) as u64) as usize;
                let t = f.block_call(next, &[]);
                let e = f.block_call(chain[j], &[]);
                f.append_inst(b, Opcode::Br, &[cond[0]], &[], Aux::Br(t, e));
            }
        }
    }
    m.add_func(f);
    m
}

#[test]
fn generated_modules_verify_and_round_trip() {
    for seed in 0..200 {
        let m = gen_module(seed);
        wolf_wir::verify_module(&m)
            .unwrap_or_else(|e| panic!("seed {seed}: generated module must verify:\n{e}"));
        let p1 = wolf_wir::print_module(&m);
        let m2 = wolf_wir::parse_module(&p1)
            .unwrap_or_else(|e| panic!("seed {seed}: canonical text must parse: {e}\n{p1}"));
        let p2 = wolf_wir::print_module(&m2);
        assert_eq!(p1, p2, "seed {seed}: print→parse→print must be a fixpoint");
        wolf_wir::verify_module(&m2)
            .unwrap_or_else(|e| panic!("seed {seed}: re-parsed module must verify:\n{e}"));
    }
}

#[test]
fn generation_is_deterministic() {
    for seed in [0, 1, 17, 199] {
        assert_eq!(
            wolf_wir::print_module(&gen_module(seed)),
            wolf_wir::print_module(&gen_module(seed)),
        );
    }
}

#[test]
fn parser_never_panics_on_garbage() {
    for seed in 0..100 {
        let mut rng = Rng::new(0xDEAD_0000 + seed);
        let len = rng.below(400) as usize;
        let bytes: String = (0..len)
            .map(|_| {
                // Printable-biased soup with plenty of grammar chars.
                let pool = b"fn decl @%(){}:,=->..=; \n\tiadd.chk b0 i64 fact noalias 0x12x%";
                pool[rng.below(pool.len() as u64) as usize] as char
            })
            .collect();
        let _ = wolf_wir::parse_module(&bytes); // must return, never panic
    }
}

#[test]
fn parser_never_panics_on_mutated_canonical_text() {
    let base = wolf_wir::print_module(&gen_module(42));
    for seed in 0..100 {
        let mut rng = Rng::new(0xBEEF_0000 + seed);
        let mut text: Vec<u8> = base.clone().into_bytes();
        for _ in 0..1 + rng.below(4) {
            if text.is_empty() {
                break;
            }
            let i = rng.below(text.len() as u64) as usize;
            match rng.below(3) {
                0 => {
                    text[i] = b' ' + (rng.below(94) as u8);
                }
                1 => {
                    text.remove(i);
                }
                _ => {
                    text.insert(i, b' ' + (rng.below(94) as u8));
                }
            }
        }
        if let Ok(s) = String::from_utf8(text) {
            let _ = wolf_wir::parse_module(&s); // must return, never panic
        }
    }
}
