//! s194 (#496): every fact the printer can emit re-parses to the same
//! fact, and every numeric immediate the printer writes beside another
//! token reads back as itself.
//!
//! The fuzz target `wir_parse` found `fact deref %q 00x%n`: it parses as
//! a scaled deref of element size 0, verifies, and printed as `0x%2`,
//! which the lexer read as a hex prefix with no digits. The canonical
//! form `{elem}x{count}` is the one format in the printer that writes a
//! number directly before a letter; `{lo}..={hi}` is the one that writes
//! a number directly before punctuation. Both are covered here at their
//! edge values, with every justification, whether or not the verifier
//! accepts the module: the printer emits facts for unverified modules
//! too, and a fact's text does not depend on the verdict.

use wolf_wir::types::{F32, F64, I8, I64, PTR};
use wolf_wir::{
    Aux, Block, DerefSize, FactData, FactKind, Function, Just, Mode, Module, Opcode, Param,
    RegionId, Theorem, Value, entity::EntityRef,
};

/// The libFuzzer artifact from #496 (sha1 `c255f8ab…`), kept in the
/// seed corpus so fuzz smoke replays it on every run.
const CRASH_C255F8AB: &str =
    include_str!("../../../fuzz/corpus/wir_parse/deref-elem0-c255f8ab.wir");

/// `fuzz/fuzz_targets/wir_parse.rs`, step for step.
fn fuzz_steps(src: &str) {
    let Ok(module) = wolf_wir::parse_module(src) else {
        return;
    };
    let p1 = wolf_wir::print_module(&module);
    if wolf_wir::verify_module(&module).is_ok() {
        let reparsed = wolf_wir::parse_module(&p1)
            .expect("canonical output of a verified module must re-parse");
        let p2 = wolf_wir::print_module(&reparsed);
        assert_eq!(p1, p2, "print→parse→print must reach a fixpoint");
    } else {
        let _ = wolf_wir::parse_module(&p1);
    }
}

#[test]
fn wir_parse_crash_c255f8ab_round_trips() {
    let m = wolf_wir::parse_module(CRASH_C255F8AB).expect("the artifact parses");
    // The target takes its re-parse arm only for a verified module; the
    // artifact must reach it, or this test would pass without testing.
    wolf_wir::verify_module(&m).expect("the artifact verifies");
    let f = m.funcs.values().next().expect("one function");
    assert!(
        f.facts.values().any(|fd| matches!(
            fd.kind,
            FactKind::Deref(_, DerefSize::Scaled { elem: 0, .. })
        )),
        "the artifact carries a scaled deref of element size 0"
    );
    let printed = wolf_wir::print_module(&m);
    assert!(
        printed.contains("fact deref %1 0x%2 : frozen.read"),
        "the canonical spelling is unchanged:\n{printed}"
    );
    fuzz_steps(CRASH_C255F8AB);
}

/// `0x` is a hex prefix only when a hex digit follows; a `0x` with no
/// digit was a lexing error everywhere before, so nothing that parsed
/// before parses differently.
#[test]
fn hex_prefix_needs_a_digit() {
    let base = |size: &str| {
        format!(
            "fn @f(mut ptr, i64) {{\n  fact deref %p {size} : excl.mut\nb0(%p: ptr, %n: i64):\n  ret\n}}\n"
        )
    };
    let deref_of = |src: &str| {
        let m = wolf_wir::parse_module(src).unwrap_or_else(|e| panic!("{src}: {e}"));
        let f = m.funcs.values().next().expect("one function");
        match f.facts.values().next().expect("one fact").kind {
            FactKind::Deref(_, size) => size,
            k => panic!("not a deref: {k:?}"),
        }
    };
    let n = Value::new(1);
    assert_eq!(
        deref_of(&base("0x%n")),
        DerefSize::Scaled { elem: 0, count: n }
    );
    assert_eq!(
        deref_of(&base("00x%n")),
        DerefSize::Scaled { elem: 0, count: n }
    );
    assert_eq!(deref_of(&base("0x10")), DerefSize::Const(16));
    assert_eq!(deref_of(&base("0x0")), DerefSize::Const(0));
    // A bare `0x` reads as `0` then a stray `x`: still refused.
    for bad in ["0x", "0xg%n"] {
        let src = base(bad);
        assert!(
            wolf_wir::parse_module(&src).is_err(),
            "`{bad}` must not parse"
        );
    }
    let iconst = |lex: &str| {
        wolf_wir::parse_module(&format!(
            "fn @g() -> i64 {{\nb0:\n  %k = iconst.i64 {lex}\n  ret %k\n}}\n"
        ))
    };
    assert!(iconst("0x").is_err(), "`iconst 0x` stays an error");
    assert!(iconst("0x1f").is_ok());
}

/// The skeleton every case shares: `(mut ptr, read ptr, i64, mem.r0)`,
/// one block, an `iconst` the range facts can cite, `ret`. Values are
/// minted in canonical order, so the original and the re-parsed module
/// number them alike and their facts compare as `FactData`.
struct Skel {
    m: Module,
    f: Function,
    p: Value,
    q: Value,
    n: Value,
    k: Value,
}

fn skeleton() -> Skel {
    skeleton_with(&|_, _, _| {})
}

/// [`skeleton`] with `extra` appended to the entry before its `ret`.
fn skeleton_with(extra: &dyn Fn(&mut Function, Block, Value)) -> Skel {
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
    let mut f = Function::new("edge", sig);
    let entry = f.make_block(&[PTR, PTR, I64, mem0]);
    let ps = f.block_params(entry);
    let (_, r) = f.append_inst(entry, Opcode::Iconst, &[], &[I64], Aux::Int(41));
    let k = r[0];
    extra(&mut f, entry, ps[0]);
    f.append_inst(entry, Opcode::Ret, &[k], &[], Aux::None);
    Skel {
        m,
        f,
        p: ps[0],
        q: ps[1],
        n: ps[2],
        k,
    }
}

const U64_EDGES: [u64; 8] = [0, 1, 8, 9, 10, 16, 255, u64::MAX];

fn kinds(s: &Skel) -> Vec<FactKind> {
    let mut out = vec![FactKind::Noalias(s.p, s.q), FactKind::Frozen(s.q)];
    for c in U64_EDGES {
        out.push(FactKind::Deref(s.p, DerefSize::Const(c)));
        out.push(FactKind::Deref(
            s.p,
            DerefSize::Scaled {
                elem: c,
                count: s.n,
            },
        ));
    }
    for (lo, hi) in [
        (0, 0),
        (0, 41),
        (41, 41),
        (-1, 0),
        (-41, -1),
        (-5, 5),
        (i64::MIN as i128, i64::MAX as i128),
        (u64::MAX as i128, u64::MAX as i128 + 1),
        (i128::MIN, i128::MAX),
    ] {
        out.push(FactKind::Range(s.k, lo, hi));
    }
    for r in [0, 1, 9, 10, u32::MAX] {
        out.push(FactKind::Region(s.p, RegionId::new(r)));
    }
    out
}

fn justs(s: &Skel) -> Vec<Just> {
    let mut out: Vec<Just> = Theorem::ALL.iter().map(|&t| Just::Theorem(t)).collect();
    out.extend([Just::DefOp, Just::Op(s.p), Just::Op(s.k), Just::Guard(s.n)]);
    for d in [
        0,
        u64::MAX,
        0x0123_4567_89ab_cdef,
        0x1111_1111_1111_1111,
        0xaaaa_aaaa_aaaa_aaaa,
    ] {
        out.push(Just::Summary(d));
    }
    out
}

fn sorted_facts(f: &Function) -> Vec<String> {
    let mut v: Vec<String> = f
        .facts
        .values()
        .map(|fd| format!("{:?} {:?}", fd.kind, fd.just))
        .collect();
    v.sort();
    v
}

/// Every fact kind × every justification × the edge values: the
/// printed fact re-parses to the same `FactData`, and print → parse →
/// print is stable. All failures are collected and reported together,
/// so a red run names every edge at once.
#[test]
fn every_printed_fact_reparses_to_itself() {
    let probe = skeleton();
    let (kinds, justs) = (kinds(&probe), justs(&probe));
    let mut failures: Vec<String> = Vec::new();
    let (mut cases, mut verified, mut verified_elem0) = (0usize, 0usize, 0usize);
    for &kind in &kinds {
        for &just in &justs {
            cases += 1;
            let mut s = skeleton();
            s.f.add_fact(FactData::new(kind, just));
            let want = sorted_facts(&s.f);
            s.m.add_func(s.f);
            if wolf_wir::verify_module(&s.m).is_ok() {
                verified += 1;
                if matches!(kind, FactKind::Deref(_, DerefSize::Scaled { elem: 0, .. })) {
                    verified_elem0 += 1;
                }
            }
            let p1 = wolf_wir::print_module(&s.m);
            let line = p1
                .lines()
                .find(|l| l.trim_start().starts_with("fact "))
                .unwrap_or("?")
                .trim()
                .to_string();
            let m2 = match wolf_wir::parse_module(&p1) {
                Ok(m2) => m2,
                Err(e) => {
                    failures.push(format!("`{line}`: does not re-parse: {e}"));
                    continue;
                }
            };
            let f2 = m2.funcs.values().next().expect("one function");
            let got = sorted_facts(f2);
            if got != want {
                failures.push(format!(
                    "`{line}`: re-parses as {got:?}, printed from {want:?}"
                ));
            }
            let p2 = wolf_wir::print_module(&m2);
            if p1 != p2 {
                failures.push(format!("`{line}`: print→parse→print moved:\n{p1}\n{p2}"));
            }
        }
    }
    eprintln!(
        "fact edges: {cases} cases, {verified} verify ({verified_elem0} scaled with elem 0), {} fail",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {cases} facts do not round-trip:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Not vacuous: the verifier accepts a share of the cases, among them
    // the shape #496 found, so the fuzz target's re-parse arm is covered.
    assert!(verified > 0 && verified_elem0 > 0);
}

/// The numeric immediates the printer writes after a mnemonic, at their
/// edges: `iconst` (decimal, signed), `fconst` (`0x` and 8 or 16 hex
/// digits), `ptr.off`'s scale. Each re-parses to the same `Aux`.
#[test]
fn every_printed_immediate_reparses_to_itself() {
    let mut failures: Vec<String> = Vec::new();
    let mut cases = 0usize;
    let mut check = |build: &dyn Fn(&mut Function, Block, Value)| {
        cases += 1;
        let mut s = skeleton_with(build);
        let entry = s.f.layout[0];
        let want: Vec<Aux> = s.f.blocks[entry]
            .insts
            .iter()
            .map(|&i| s.f.insts[i].aux)
            .collect();
        s.m.add_func(s.f);
        let p1 = wolf_wir::print_module(&s.m);
        let m2 = match wolf_wir::parse_module(&p1) {
            Ok(m2) => m2,
            Err(e) => {
                failures.push(format!("does not re-parse: {e}\n{p1}"));
                return;
            }
        };
        let f2 = m2.funcs.values().next().expect("one function");
        let e2 = f2.layout[0];
        let got: Vec<Aux> = f2.blocks[e2]
            .insts
            .iter()
            .map(|&i| f2.insts[i].aux)
            .collect();
        if got != want {
            failures.push(format!("re-parses as {got:?}, printed from {want:?}\n{p1}"));
        }
        let p2 = wolf_wir::print_module(&m2);
        if p1 != p2 {
            failures.push(format!("print→parse→print moved:\n{p1}\n{p2}"));
        }
    };
    for c in [i64::MIN, -10, -1, 0, 1, 9, 10, i64::MAX] {
        check(&|f, b, _| {
            f.append_inst(b, Opcode::Iconst, &[], &[I64], Aux::Int(c));
        });
    }
    for c in [i8::MIN as i64, 0, i8::MAX as i64] {
        check(&|f, b, _| {
            f.append_inst(b, Opcode::Iconst, &[], &[I8], Aux::Int(c));
        });
    }
    for bits in [0, 1, 0x8000_0000_0000_0000, 1.5f64.to_bits(), u64::MAX] {
        check(&|f, b, _| {
            f.append_inst(b, Opcode::Fconst, &[], &[F64], Aux::FloatBits(bits));
        });
    }
    for bits in [0u64, 1, 0x8000_0000, 1.5f32.to_bits() as u64, 0xffff_ffff] {
        check(&|f, b, _| {
            f.append_inst(b, Opcode::Fconst, &[], &[F32], Aux::FloatBits(bits));
        });
    }
    for scale in U64_EDGES {
        check(&|f, b, p| {
            let (_, z) = f.append_inst(b, Opcode::Iconst, &[], &[I64], Aux::Int(0));
            f.append_inst(b, Opcode::PtrOff, &[p, z[0]], &[PTR], Aux::Scale(scale));
        });
    }
    eprintln!("immediate edges: {cases} cases, {} fail", failures.len());
    assert!(
        failures.is_empty(),
        "{} of {cases} immediates do not round-trip:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
