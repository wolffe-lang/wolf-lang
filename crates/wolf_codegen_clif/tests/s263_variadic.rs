//! Native SysV varargs, independently witnessed by a C caller/callee.
//! Only x86-64 Linux executes these tests; no host compiler is optional there.
#![cfg(all(target_arch = "x86_64", target_os = "linux"))]

use std::process::Command;
use wolf_backend::{Backend, NullDebugSink};
use wolf_codegen_clif::{ClifBackend, compile_selected};
use wolf_wir::entity::EntityRef;
use wolf_wir::ir::{Aux, Block, FuncId, Function, Module, Param, Value};
use wolf_wir::ops::Opcode;
use wolf_wir::types::{self, RegionId, TypeData, TypeId};

fn inst(f: &mut Function, b: Block, op: Opcode, args: &[Value], ty: TypeId, aux: Aux) -> Value {
    f.append_inst(b, op, args, &[ty], aux).1[0]
}

fn export(
    m: &mut Module,
    name: &str,
    params: &[TypeId],
    result: TypeId,
    variadic: bool,
) -> (Function, Block) {
    let sig = m.make_sig(
        params.iter().copied().map(Param::val).collect(),
        vec![result],
    );
    m.sigs[sig].variadic = variadic.then_some(params.len() as u32);
    let mut f = Function::new(name, sig);
    f.export = true;
    let b = f.make_block(params);
    (f, b)
}

fn list(m: &mut Module, f: &mut Function, b: Block) -> (Value, Value, TypeId) {
    let mem = m.types.mem(RegionId::new(0));
    let token = inst(f, b, Opcode::RegionForeign, &[], mem, Aux::Int(0));
    let ap = inst(f, b, Opcode::VaStart, &[], types::PTR, Aux::None);
    (ap, token, mem)
}

fn arg(f: &mut Function, b: Block, ap: Value, token: &mut Value, mem: TypeId, ty: TypeId) -> Value {
    let out = f
        .append_inst(b, Opcode::VaArg, &[ap, *token], &[ty, mem], Aux::None)
        .1;
    *token = out[1];
    out[0]
}

fn ret(f: &mut Function, b: Block, value: Value) {
    f.append_inst(b, Opcode::Ret, &[value], &[], Aux::None);
}

fn incoming_module() -> Module {
    let mut m = Module::new();
    let (mut f, b) = export(&mut m, "w_gp", &[types::I32], types::I64, true);
    let (ap, mut token, mem) = list(&mut m, &mut f, b);
    let mut sum = inst(&mut f, b, Opcode::Iconst, &[], types::I64, Aux::Int(0));
    for _ in 0..8 {
        let v = arg(&mut f, b, ap, &mut token, mem, types::I64);
        sum = inst(
            &mut f,
            b,
            Opcode::IaddWrap,
            &[sum, v],
            types::I64,
            Aux::None,
        );
    }
    ret(&mut f, b, sum);
    m.add_func(f);

    let (mut f, b) = export(&mut m, "w_fp", &[types::F64], types::F64, true);
    let mut sum = f.block_params(b)[0];
    let (ap, mut token, mem) = list(&mut m, &mut f, b);
    for _ in 0..9 {
        let v = arg(&mut f, b, ap, &mut token, mem, types::F64);
        sum = inst(&mut f, b, Opcode::Fadd, &[sum, v], types::F64, Aux::None);
    }
    ret(&mut f, b, sum);
    m.add_func(f);

    let mut params = vec![types::I64; 8];
    params.extend([types::F64; 9]);
    let (mut f, b) = export(&mut m, "w_named", &params, types::F64, true);
    let ps = f.block_params(b);
    let (ap, mut token, mem) = list(&mut m, &mut f, b);
    let gi = arg(&mut f, b, ap, &mut token, mem, types::I64);
    let fp = arg(&mut f, b, ap, &mut token, mem, types::F64);
    let gi = inst(
        &mut f,
        b,
        Opcode::IaddWrap,
        &[ps[7], gi],
        types::I64,
        Aux::None,
    );
    let gi = inst(&mut f, b, Opcode::Sitofp, &[gi], types::F64, Aux::None);
    let sum = inst(
        &mut f,
        b,
        Opcode::Fadd,
        &[ps[16], fp],
        types::F64,
        Aux::None,
    );
    let sum = inst(&mut f, b, Opcode::Fadd, &[sum, gi], types::F64, Aux::None);
    ret(&mut f, b, sum);
    m.add_func(f);

    let pair = m.types.intern(TypeData::Agg(vec![types::I64, types::F64]));
    let doubles = m.types.intern(TypeData::Agg(vec![types::F64, types::F64]));
    let ints = m.types.intern(TypeData::Agg(vec![types::I64, types::I64]));
    for (name, fixed, tys) in [
        ("w_pair", 1, vec![pair, doubles]),
        ("w_rollback", 5, vec![ints, types::I64, pair, types::F64]),
    ] {
        let (mut f, b) = export(&mut m, name, &vec![types::I64; fixed], types::F64, true);
        let (ap, mut token, mem) = list(&mut m, &mut f, b);
        let mut sum = inst(
            &mut f,
            b,
            Opcode::Fconst,
            &[],
            types::F64,
            Aux::FloatBits(0),
        );
        for ty in tys {
            let v = arg(&mut f, b, ap, &mut token, mem, ty);
            let fields = match m.types.get(ty) {
                TypeData::Agg(fs) => fs.clone(),
                _ => vec![ty],
            };
            for (index, field) in fields.iter().enumerate() {
                let value = if fields.len() > 1 {
                    inst(
                        &mut f,
                        b,
                        Opcode::AggGet,
                        &[v],
                        *field,
                        Aux::Int(index as i64),
                    )
                } else {
                    v
                };
                let value = if *field == types::I64 {
                    inst(&mut f, b, Opcode::Sitofp, &[value], types::F64, Aux::None)
                } else {
                    value
                };
                sum = inst(
                    &mut f,
                    b,
                    Opcode::Fadd,
                    &[sum, value],
                    types::F64,
                    Aux::None,
                );
            }
        }
        ret(&mut f, b, sum);
        m.add_func(f);
    }

    let big = m.types.intern(TypeData::Agg(vec![types::I64; 3]));
    let (mut f, b) = export(&mut m, "w_sret", &[types::I64; 5], big, true);
    let ps = f.block_params(b);
    let (ap, mut token, mem) = list(&mut m, &mut f, b);
    let v = arg(&mut f, b, ap, &mut token, mem, types::I64);
    let result = inst(
        &mut f,
        b,
        Opcode::AggMake,
        &[ps[0], ps[4], v],
        big,
        Aux::None,
    );
    ret(&mut f, b, result);
    m.add_func(f);

    let (mut f, b) = export(&mut m, "w_memory", &[types::I32], types::I64, true);
    let (ap, mut token, mem) = list(&mut m, &mut f, b);
    let value = arg(&mut f, b, ap, &mut token, mem, big);
    let last = inst(&mut f, b, Opcode::AggGet, &[value], types::I64, Aux::Int(2));
    let next = arg(&mut f, b, ap, &mut token, mem, types::I64);
    let sum = inst(
        &mut f,
        b,
        Opcode::IaddWrap,
        &[last, next],
        types::I64,
        Aux::None,
    );
    ret(&mut f, b, sum);
    m.add_func(f);

    // va_copy's lowering is an ordinary four-field record copy. Advance a
    // copy before the original; each must still yield the same first value.
    let (mut f, b) = export(&mut m, "w_copy", &[types::I32], types::I64, true);
    let (ap, mut token, mem) = list(&mut m, &mut f, b);
    let copy_mem = m.types.mem(RegionId::new(1));
    let size = inst(&mut f, b, Opcode::Iconst, &[], types::I64, Aux::Int(24));
    let copy = f
        .append_inst(
            b,
            Opcode::StackAlloc,
            &[size],
            &[types::PTR, copy_mem],
            Aux::None,
        )
        .1;
    let mut copy_token = copy[1];
    for (off, ty) in [
        (0, types::I32),
        (4, types::I32),
        (8, types::PTR),
        (16, types::PTR),
    ] {
        let off = inst(&mut f, b, Opcode::Iconst, &[], types::I64, Aux::Int(off));
        let src = inst(
            &mut f,
            b,
            Opcode::PtrOff,
            &[ap, off],
            types::PTR,
            Aux::Scale(1),
        );
        let dst = inst(
            &mut f,
            b,
            Opcode::PtrOff,
            &[copy[0], off],
            types::PTR,
            Aux::Scale(1),
        );
        let v = inst(&mut f, b, Opcode::Load, &[src, token], ty, Aux::None);
        copy_token = inst(
            &mut f,
            b,
            Opcode::Store,
            &[v, dst, copy_token],
            copy_mem,
            Aux::None,
        );
    }
    let a = arg(&mut f, b, copy[0], &mut copy_token, copy_mem, types::I64);
    let bvalue = arg(&mut f, b, ap, &mut token, mem, types::I64);
    let sum = inst(
        &mut f,
        b,
        Opcode::IaddWrap,
        &[a, bvalue],
        types::I64,
        Aux::None,
    );
    inst(
        &mut f,
        b,
        Opcode::VaEnd,
        &[copy[0], copy_token],
        copy_mem,
        Aux::None,
    );
    inst(&mut f, b, Opcode::VaEnd, &[ap, token], mem, Aux::None);
    ret(&mut f, b, sum);
    m.add_func(f);

    // Nonvariadic consumers of C's live va_list use the same cursor algorithm.
    let (mut f, b) = export(&mut m, "w_consume", &[types::PTR], types::I64, false);
    let ap = f.block_params(b)[0];
    let mem = m.types.mem(RegionId::new(0));
    let mut token = inst(&mut f, b, Opcode::RegionForeign, &[], mem, Aux::Int(0));
    let a = arg(&mut f, b, ap, &mut token, mem, types::I64);
    let d = arg(&mut f, b, ap, &mut token, mem, types::F64);
    let d = inst(&mut f, b, Opcode::FtosiChk, &[d], types::I64, Aux::None);
    let sum = inst(&mut f, b, Opcode::IaddWrap, &[a, d], types::I64, Aux::None);
    ret(&mut f, b, sum);
    m.add_func(f);
    m
}

fn wrapper(
    m: &mut Module,
    name: &str,
    callee: &str,
    params: &[TypeId],
    result: TypeId,
    fixed: u32,
) -> FuncId {
    let call_sig = m.make_sig(
        params.iter().copied().map(Param::val).collect(),
        vec![result],
    );
    m.sigs[call_sig].variadic = Some(fixed);
    if callee.starts_with("c.") {
        m.add_decl(callee, call_sig);
    }
    let (mut f, b) = export(m, name, params, result, false);
    let args = f.block_params(b);
    let target = f.import_func(callee, call_sig);
    let value = inst(&mut f, b, Opcode::Call, &args, result, Aux::Callee(target));
    ret(&mut f, b, value);
    m.add_func(f)
}

fn emit(m: &Module, funcs: &[FuncId]) -> Vec<u8> {
    wolf_wir::verify_module(m).expect("variadic WIR verifies");
    let mut backend = ClifBackend::new().expect("x86-64 Linux native backend");
    compile_selected(
        &mut backend,
        m,
        funcs,
        None,
        false,
        false,
        true,
        &mut NullDebugSink,
    )
    .expect("native variadic compilation");
    Box::new(backend).finish().expect("native object").bytes
}

fn link_run(name: &str, objects: &[Vec<u8>], c: &str) {
    let dir =
        std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("s263_variadic_{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("witness.c");
    let exe = dir.join("witness");
    std::fs::write(&source, c).unwrap();
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut cmd = Command::new(cc);
    cmd.args(["-std=c17", "-Wall", "-Wextra", "-Werror", "-O2"])
        .arg(&source)
        .arg("-o")
        .arg(&exe);
    for (i, bytes) in objects.iter().enumerate() {
        let object = dir.join(format!("wolf_{i}.o"));
        std::fs::write(&object, bytes).unwrap();
        cmd.arg(object);
    }
    let out = cmd.output().expect("C compiler required on x86-64 Linux");
    assert!(
        out.status.success(),
        "C witness link: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(&exe).output().expect("run ABI witness");
    assert!(
        out.status.success(),
        "ABI witness {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

const INCOMING_C: &str = r#"
#include <stdarg.h>
#include <stdlib.h>
struct pair { long i; double d; };
struct doubles { double a, b; };
struct ints { long a, b; };
struct big { long a, b, c; };
extern long w_gp(int, ...);
extern double w_fp(double, ...);
extern double w_named(long,long,long,long,long,long,long,long,
                      double,double,double,double,double,double,double,double,double,...);
extern double w_pair(long,...);
extern double w_rollback(long,long,long,long,long,...);
extern struct big w_sret(long,long,long,long,long,...);
extern long w_consume(va_list);
extern long w_memory(int,...);
extern long w_copy(int,...);
void __wolf_rt_trap(int kind) { (void)kind; abort(); }
static long forward(int n, ...) {
    va_list a, b; va_start(a,n); va_copy(b,a);
    long x = w_consume(b), y = w_consume(a);
    va_end(a); va_end(b); return x + y;
}
int main(void) {
    if (w_gp(8,1L,2L,3L,4L,5L,6L,7L,8L) != 36) return 1;
    if (w_fp(100.,1.,2.,3.,4.,5.,6.,7.,8.,9.) != 145.) return 2;
    if (w_named(1,2,3,4,5,6,7,8,1.,2.,3.,4.,5.,6.,7.,8.,9.,10L,11.) != 38.) return 3;
    if (w_pair(0L,(struct pair){12,13.},(struct doubles){14.,15.}) != 54.) return 4;
    if (w_rollback(0L,0L,0L,0L,0L,(struct ints){1,2},3L,(struct pair){4,5.},6.) != 21.) return 5;
    struct big b = w_sret(1,2,3,4,5,6L);
    if (b.a != 1 || b.b != 5 || b.c != 6) return 6;
    if (forward(2,20L,22.) != 84) return 7;
    if (w_memory(0,(struct big){10,20,30},12L) != 42) return 8;
    if (w_copy(0,21L,99L) != 42) return 9;
    return 0;
}
"#;

#[test]
fn c_calls_native_variadic_definitions_and_list_consumer() {
    let m = incoming_module();
    let ids: Vec<_> = m.funcs.keys().collect();
    link_run("incoming", &[emit(&m, &ids)], INCOMING_C);
}

#[test]
fn native_outgoing_shims_keep_all_banks_stack_and_callsite_shapes() {
    let mut m = Module::new();
    let pair = m.types.intern(TypeData::Agg(vec![types::I64, types::F64]));
    let mut params = vec![types::I32];
    params.extend([types::I64; 8]);
    params.extend([types::F64; 10]);
    wrapper(&mut m, "w_out_many", "c.probe", &params, types::F64, 1);
    wrapper(
        &mut m,
        "w_out_small",
        "c.probe",
        &[types::I32, types::I64, types::F64],
        types::F64,
        1,
    );
    wrapper(
        &mut m,
        "w_out_pair",
        "c.pair_probe",
        &[types::I64, pair],
        types::F64,
        1,
    );
    wrapper(&mut m, "w_al_many", "c.al_probe", &params, types::I64, 1);
    wrapper(
        &mut m,
        "w_al_zero",
        "c.al_probe",
        &[types::I32],
        types::I64,
        1,
    );
    wrapper(
        &mut m,
        "w_al_named",
        "c.al_named",
        &[types::F64; 3],
        types::I64,
        1,
    );
    wrapper(&mut m, "w_stack", "c.stack_probe", &params, types::I64, 1);
    let call_sig = m.make_sig(
        vec![Param::val(types::I32), Param::val(types::F64)],
        vec![types::F64],
    );
    m.sigs[call_sig].variadic = Some(1);
    m.add_decl("c.promoted", call_sig);
    let (mut f, b) = export(
        &mut m,
        "w_promote",
        &[types::I32, types::F32],
        types::F64,
        false,
    );
    let ps = f.block_params(b);
    let fp = inst(&mut f, b, Opcode::Fpromote, &[ps[1]], types::F64, Aux::None);
    let target = f.import_func("c.promoted", call_sig);
    let value = inst(
        &mut f,
        b,
        Opcode::Call,
        &[ps[0], fp],
        types::F64,
        Aux::Callee(target),
    );
    ret(&mut f, b, value);
    m.add_func(f);
    // The same C name with two tail shapes within a single CLIF function
    // also must bypass the per-function FuncRef cache.
    let small = m.make_sig(
        vec![
            Param::val(types::I32),
            Param::val(types::I64),
            Param::val(types::F64),
        ],
        vec![types::F64],
    );
    m.sigs[small].variadic = Some(1);
    let many = m.make_sig(
        params.iter().copied().map(Param::val).collect(),
        vec![types::F64],
    );
    m.sigs[many].variadic = Some(1);
    let (mut f, b) = export(&mut m, "w_out_both", &params, types::F64, false);
    let ps = f.block_params(b);
    let one = inst(&mut f, b, Opcode::Iconst, &[], types::I32, Aux::Int(1));
    let small_target = f.import_func("c.probe", small);
    let many_target = f.import_func("c.probe", many);
    let x = inst(
        &mut f,
        b,
        Opcode::Call,
        &[one, ps[1], ps[9]],
        types::F64,
        Aux::Callee(small_target),
    );
    let y = inst(
        &mut f,
        b,
        Opcode::Call,
        &ps,
        types::F64,
        Aux::Callee(many_target),
    );
    let sum = inst(&mut f, b, Opcode::Fadd, &[x, y], types::F64, Aux::None);
    ret(&mut f, b, sum);
    m.add_func(f);
    let ids: Vec<_> = m.funcs.keys().collect();
    link_run(
        "outgoing",
        &[emit(&m, &ids)],
        r#"
#include <stdarg.h>
struct pair { long i; double d; };
extern double w_out_many(int,long,long,long,long,long,long,long,long,double,double,double,double,double,double,double,double,double,double);
extern double w_out_small(int,long,double);
extern double w_out_both(int,long,long,long,long,long,long,long,long,double,double,double,double,double,double,double,double,double,double);
extern double w_out_pair(long,struct pair);
extern double w_promote(int,float);
extern long w_al_many(int,long,long,long,long,long,long,long,long,double,double,double,double,double,double,double,double,double,double);
extern long w_al_zero(int);
extern long w_al_named(double,double,double);
extern long w_stack(int,long,long,long,long,long,long,long,long,double,double,double,double,double,double,double,double,double,double);
/* Entry-instruction witnesses: no compiler prologue can hide AL or RSP. */
__asm__(".text\n.globl al_probe\nal_probe:\nmovzbl %al,%eax\nret\n"
        ".globl al_named\nal_named:\nmovzbl %al,%eax\nret\n"
        ".globl stack_probe\nstack_probe:\nmov %rsp,%rax\nand $15,%eax\nret\n");
double probe(int count, ...) {
    va_list a; va_start(a,count); double s=0;
    int ni = count == 1 ? 1 : 8, nf = count == 1 ? 1 : 10;
    for(int i=0;i<ni;i++) s += va_arg(a,long);
    for(int i=0;i<nf;i++) s += va_arg(a,double);
    va_end(a); return s;
}
double pair_probe(long n, ...) {
    va_list a; va_start(a,n); struct pair p = va_arg(a,struct pair);
    va_end(a); return n+p.i+p.d;
}
double promoted(int n, ...) {
    va_list a; va_start(a,n); double d = va_arg(a,double);
    va_end(a); return n+d;
}
int main(void) {
    if(w_out_many(2,1,2,3,4,5,6,7,8,1.,2.,3.,4.,5.,6.,7.,8.,9.,10.) != 91.) return 1;
    if(w_out_small(1,40,2.) != 42.) return 2;
    if(w_out_pair(10,(struct pair){20,12.}) != 42.) return 3;
    if(w_promote(40,2.5f) != 42.5) return 4;
    if(w_out_both(2,1,2,3,4,5,6,7,8,1.,2.,3.,4.,5.,6.,7.,8.,9.,10.) != 93.) return 5;
    if(w_al_many(2,1,2,3,4,5,6,7,8,1.,2.,3.,4.,5.,6.,7.,8.,9.,10.) != 8) return 6;
    if(w_al_zero(0) != 0 || w_al_named(1.,2.,3.) != 3) return 7;
    if(w_stack(2,1,2,3,4,5,6,7,8,1.,2.,3.,4.,5.,6.,7.,8.,9.,10.) != 8) return 8;
    return 0;
}
"#,
    );
}

#[test]
fn variadic_exports_are_called_through_public_entries_in_same_and_other_objects() {
    let mut m = incoming_module();
    let mut params = vec![types::I32];
    params.extend([types::I64; 8]);
    let caller = wrapper(&mut m, "w_forward", "w_gp", &params, types::I64, 1);
    let ids: Vec<_> = m.funcs.keys().collect();
    let c = r#"
#include <stdlib.h>
extern long w_forward(int,long,long,long,long,long,long,long,long);
void __wolf_rt_trap(int kind) { (void)kind; abort(); }
int main(void) { return w_forward(8,1,2,3,4,5,6,7,8) == 36 ? 0 : 1; }
"#;
    link_run("same_object", &[emit(&m, &ids)], c);
    let defs: Vec<_> = ids.iter().copied().filter(|id| *id != caller).collect();
    link_run("cross_object", &[emit(&m, &defs), emit(&m, &[caller])], c);
}

/// Disassemble rather than execute a freestanding object: the AL==0 path
/// must bypass every SSE instruction. No QEMU/kernel fixture is assumed.
#[test]
fn freestanding_variadic_sse_saves_are_all_guarded_and_core_is_integer_only() {
    let mut m = Module::new();
    let (mut f, b) = export(&mut m, "w_none", &[types::I32], types::I64, true);
    let (ap, mut token, mem) = list(&mut m, &mut f, b);
    let value = arg(&mut f, b, ap, &mut token, mem, types::I64);
    inst(&mut f, b, Opcode::VaEnd, &[ap, token], mem, Aux::None);
    ret(&mut f, b, value);
    m.add_func(f);
    wrapper(
        &mut m,
        "w_none_caller",
        "w_none",
        &[types::I32, types::I64],
        types::I64,
        1,
    );
    let ids: Vec<_> = m.funcs.keys().collect();
    wolf_wir::verify_module(&m).expect("freestanding variadic WIR verifies");
    let mut backend = ClifBackend::for_target(wolf_backend::target::Target::X86_64None).unwrap();
    compile_selected(
        &mut backend,
        &m,
        &ids,
        None,
        false,
        false,
        false,
        &mut NullDebugSink,
    )
    .expect("integer-only freestanding variadics compile");
    let product = Box::new(backend).finish().expect("freestanding object");
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("s263_variadic_none");
    std::fs::create_dir_all(&dir).unwrap();
    let object = dir.join("none.o");
    std::fs::write(&object, product.bytes).unwrap();
    let tool = std::env::var("OBJDUMP").unwrap_or_else(|_| "objdump".to_string());
    let output = Command::new(tool)
        .args(["-d", "--no-show-raw-insn"])
        .arg(&object)
        .output()
        .expect("objdump required for freestanding SSE guard proof");
    assert!(
        output.status.success(),
        "objdump: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let disassembly = String::from_utf8(output.stdout).expect("objdump text");
    std::fs::write(dir.join("none.disasm"), &disassembly).unwrap();

    let mut functions: std::collections::HashMap<String, Vec<(u64, String)>> =
        std::collections::HashMap::new();
    let mut current = String::new();
    for line in disassembly.lines() {
        if let Some((_, rest)) = line.split_once('<')
            && let Some(name) = rest.strip_suffix(">:")
        {
            current = name.to_string();
            functions.entry(current.clone()).or_default();
        } else if let Some((address, text)) = line.trim().split_once(':')
            && let Ok(address) = u64::from_str_radix(address, 16)
            && !current.is_empty()
        {
            functions
                .get_mut(&current)
                .unwrap()
                .push((address, text.trim().to_string()));
        }
    }
    let entry = functions
        .get("w_none")
        .expect("public trampoline disassembled");
    let compact = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    assert_eq!(
        compact(&entry[0].1),
        "push%rbp",
        "save caller frame pointer"
    );
    assert_eq!(
        compact(&entry[1].1),
        "mov%rsp,%rbp",
        "link trampoline frame chain"
    );
    let adjust = compact(&entry[2].1);
    let amount = adjust
        .strip_prefix("sub$")
        .and_then(|s| s.strip_suffix(",%rsp"))
        .expect("frame allocation follows RBP setup");
    let frame = wolf_backend::abi::variadic::frame(&m.types, &m.sigs[m.funcs[ids[0]].sig]).unwrap();
    assert_eq!(
        u64::from_str_radix(amount.trim_start_matches("0x"), 16).unwrap(),
        u64::from(frame.frame_bytes - 8)
    );
    let end = entry.len();
    assert_eq!(
        compact(&entry[end - 3].1),
        format!("add${amount},%rsp"),
        "restore the adjusted stack"
    );
    assert_eq!(
        compact(&entry[end - 2].1),
        "pop%rbp",
        "restore caller frame pointer"
    );
    assert_eq!(
        compact(&entry[end - 1].1),
        "ret",
        "return through the original address"
    );
    let guard = entry
        .iter()
        .position(|(_, text)| {
            let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            compact == "test%al,%al"
        })
        .expect("incoming AL is tested");
    let branch = &entry[guard + 1].1;
    let mut branch_words = branch.split_whitespace();
    assert!(matches!(branch_words.next(), Some("je" | "jz")), "{branch}");
    let target = u64::from_str_radix(
        branch_words
            .next()
            .expect("skip target")
            .trim_start_matches("0x"),
        16,
    )
    .expect("hex branch target");
    let saves = &entry[guard + 2..];
    assert_eq!(
        saves
            .iter()
            .take_while(|(address, _)| *address < target)
            .count(),
        8,
        "the skip spans exactly all eight SSE saves\n{disassembly}"
    );
    for (i, (address, text)) in saves.iter().take(8).enumerate() {
        assert!(*address < target && text.starts_with("movdqu"), "{text}");
        assert!(text.contains(&format!("%xmm{i},")), "{text}");
    }
    assert_eq!(
        saves[8].0, target,
        "skip lands at the first non-SSE instruction"
    );
    let core = functions
        .get("_W.va.core.0")
        .expect("private core disassembled");
    assert!(!core.is_empty(), "non-vacuous core proof");
    let mut sse_count = 0;
    for (name, instructions) in &functions {
        for (address, text) in instructions {
            let mnemonic = text.split_whitespace().next().unwrap_or("");
            let fp_state_op = matches!(
                mnemonic,
                "ldmxcsr"
                    | "stmxcsr"
                    | "vldmxcsr"
                    | "vstmxcsr"
                    | "vzeroupper"
                    | "vzeroall"
                    | "fxsave"
                    | "fxsave64"
                    | "fxrstor"
                    | "fxrstor64"
                    | "xsave"
                    | "xsave64"
                    | "xrstor"
                    | "xrstor64"
            );
            if fp_state_op
                || text.contains("%xmm")
                || text.contains("%ymm")
                || text.contains("%zmm")
            {
                sse_count += 1;
                assert!(
                    name == "w_none" && *address > entry[guard + 1].0 && *address < target,
                    "unguarded vector instruction in {name}: {text}\n{disassembly}"
                );
            }
        }
    }
    assert_eq!(sse_count, 8, "all SSE instructions are the guarded saves");
    let outgoing = functions
        .iter()
        .find(|(name, _)| name.starts_with("_W.va.call."))
        .expect("outgoing variadic shim disassembled")
        .1;
    let first: String = outgoing[0]
        .1
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_eq!(first, "mov$0x0,%eax", "none callers supply AL=0");
}

#[test]
fn native_snprintf_promoted_tail_spills_both_banks() {
    let mut m = Module::new();
    let mut params = vec![types::PTR, types::I64, types::PTR];
    params.extend([types::I32; 7]);
    params.extend([types::F64; 9]);
    params.push(types::PTR);
    wrapper(&mut m, "w_snprintf", "c.snprintf", &params, types::I32, 3);
    let ids: Vec<_> = m.funcs.keys().collect();
    link_run(
        "snprintf",
        &[emit(&m, &ids)],
        r#"
#include <stdio.h>
#include <string.h>
#include <stddef.h>
extern int w_snprintf(char *,size_t,const char *,int,int,int,int,int,int,int,
                      double,double,double,double,double,double,double,double,double,const char *);
int main(void) {
    char actual[256], expected[256];
    const char *fmt = "%d %d %d %d %d %c %x %.2f %.2f %.2f %.2f %.2f %.2f %.2f %.2f %.2f %s";
    int a = w_snprintf(actual,sizeof actual,fmt,-128,-32768,255,65535,1,65,42,
                       1.25,2.25,3.25,4.25,5.25,6.25,7.25,8.25,9.25,"ok");
    int b = snprintf(expected,sizeof expected,fmt,-128,-32768,255,65535,1,65,42,
                     1.25,2.25,3.25,4.25,5.25,6.25,7.25,8.25,9.25,"ok");
    if (a != b || strcmp(actual,expected) != 0) return 1;
    return strcmp(actual,"-128 -32768 255 65535 1 A 2a 1.25 2.25 3.25 4.25 5.25 6.25 7.25 8.25 9.25 ok") != 0;
}
"#,
    );
}
