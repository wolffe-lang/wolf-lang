//! s263: SysV variable arguments in both directions, including whole-
//! aggregate rollback. These are API-level C oracles, not compiler or
//! libc implementation sources. Execute the witnesses on Linux x86-64.

use wolf_backend::{Backend, BackendError, Linkage, NullDebugSink};
use wolf_codegen_llvm::{EmitOptions, LlvmBackend, ReleaseTarget};
use wolf_wir::entity::EntityRef;
use wolf_wir::ir::{Aux, Function, Module, Param};
use wolf_wir::ops::Opcode;
use wolf_wir::types::{self, RegionId, TypeData, TypeId};

#[path = "../../wolf_backend/tests/support/variadic_guard.rs"]
mod variadic_guard;

fn emit(m: &Module, target: ReleaseTarget) -> Result<String, BackendError> {
    wolf_wir::verify_module(m).expect("valid emitter input");
    let mut backend = LlvmBackend::with_options(EmitOptions {
        target: Some(target),
        ..EmitOptions::default()
    })?;
    for (id, f) in m.funcs.iter() {
        backend.declare_function(m, id, &f.name, &f.name, f.sig, Linkage::Export)?;
    }
    for (id, f) in m.funcs.iter() {
        backend.define_function(m, id, f, &mut NullDebugSink)?;
    }
    Ok(backend.module_ir())
}

/// Return every extracted value in a C record, rather than checking
/// only the last cursor position. The hidden sret consumes a GP slot.
fn reader(m: &mut Module, name: &str, fixed: &[TypeId], tail: &[TypeId]) {
    let mem = m.types.mem(RegionId::new(0));
    let output = m.types.intern(TypeData::Agg(tail.to_vec()));
    let mut params: Vec<_> = fixed.iter().copied().map(Param::val).collect();
    params.push(Param::val(mem));
    let sig = m.make_sig(params, vec![output]);
    m.sigs[sig].variadic = Some(fixed.len() as u32);
    let mut f = Function::new(name, sig);
    f.export = true;
    let mut entry_tys = fixed.to_vec();
    entry_tys.push(mem);
    let b = f.make_block(&entry_tys);
    let mut token = *f.block_params(b).last().unwrap();
    let (_, ap) = f.append_inst(b, Opcode::VaStart, &[], &[types::PTR], Aux::None);
    let mut values = Vec::new();
    for &ty in tail {
        let (_, out) = f.append_inst(b, Opcode::VaArg, &[ap[0], token], &[ty, mem], Aux::None);
        values.push(out[0]);
        token = out[1];
    }
    f.append_inst(b, Opcode::VaEnd, &[ap[0], token], &[mem], Aux::None);
    let (_, out) = f.append_inst(b, Opcode::AggMake, &values, &[output], Aux::None);
    f.append_inst(b, Opcode::Ret, &[out[0]], &[], Aux::None);
    m.add_func(f);
}

fn readers() -> Module {
    let mut m = Module::new();
    let mixed = m.types.intern(TypeData::Agg(vec![types::F64, types::I64]));
    let pair = m.types.intern(TypeData::Agg(vec![types::F64, types::F64]));
    let big = m.types.intern(TypeData::Agg(vec![types::I64; 3]));
    reader(&mut m, "read_gp", &[types::I32], &[types::I64; 8]);
    reader(&mut m, "read_fp", &[types::F64], &[types::F64; 10]);
    reader(
        &mut m,
        "read_mixed",
        &[types::I32],
        &[mixed, types::I64, types::I64, types::I64, mixed, types::F64],
    );
    reader(
        &mut m,
        "read_sse_rollback",
        &[types::F64; 7],
        &[pair, types::I64, types::F64],
    );
    let mut named = vec![types::I64; 8];
    named.extend([types::F64; 9]);
    reader(
        &mut m,
        "read_named_stack",
        &named,
        &[types::I64, types::F64, types::PTR, big],
    );
    m
}

#[test]
fn definitions_have_ellipsis_noinline_and_psabi_cursor_paths() {
    let ir = emit(&readers(), ReleaseTarget::LinuxX64).expect("emit readers");
    for line in ir.lines().filter(|l| l.starts_with("define ")) {
        assert!(line.contains(", ...)"), "{line}");
        assert!(line.contains(" noinline"), "{line}");
    }
    assert!(ir.contains("declare void @llvm.va_start(ptr)"));
    assert!(ir.contains("alloca [24 x i8], align 8"));
    assert!(ir.contains("va.reg"));
    assert!(ir.contains("va.stack"));
    assert!(ir.contains("icmp ule i32"));
    assert!(
        !ir.contains(" = va_arg "),
        "aggregates use the explicit psABI algorithm"
    );
}

#[test]
fn each_started_cursor_has_an_explicit_matching_end() {
    let ir = emit(&readers(), ReleaseTarget::LinuxX64).expect("emit paired cursors");
    assert!(ir.contains("declare void @llvm.va_end(ptr)"));
    let mut functions = 0;
    for body in ir.split("define ").skip(1) {
        functions += 1;
        let starts: Vec<_> = body
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("call void @llvm.va_start("))
            .collect();
        let ends: Vec<_> = body
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("call void @llvm.va_end("))
            .collect();
        assert_eq!(starts.len(), 1, "{body}");
        assert_eq!(ends.len(), 1, "{body}");
        assert_eq!(starts[0].replace("llvm.va_start", "llvm.va_end"), ends[0]);
    }
    assert_eq!(functions, 5);
}

#[test]
fn variadic_definition_refuses_other_targets_by_name() {
    let target = ReleaseTarget::MacosArm64;
    let err = emit(&readers(), target).expect_err("variadics must refuse this target");
    let text = err.to_string();
    assert!(text.contains(target.triple()), "{text}");
    assert!(text.contains("E0826"), "{text}");
}

/// A separate, ordinary consumer accepts a live C va_list pointer.
fn consumer() -> Module {
    let mut m = Module::new();
    let mem = m.types.mem(RegionId::new(0));
    let sig = m.make_sig(
        vec![Param::val(types::PTR), Param::val(mem)],
        vec![types::I64],
    );
    let mut f = Function::new("consume_list", sig);
    f.export = true;
    let b = f.make_block(&[types::PTR, mem]);
    let p = f.block_params(b);
    let (_, value) = f.append_inst(b, Opcode::VaArg, &p, &[types::I64, mem], Aux::None);
    f.append_inst(b, Opcode::Ret, &[value[0]], &[], Aux::None);
    m.add_func(f);

    // va_copy copies all four fields through scalar memory operations,
    // matching lowering's load_flat/store_flat (WIR has no aggregate load).
    let copy_mem = m.types.mem(RegionId::new(1));
    let mut f = Function::new("consume_copy", sig);
    f.export = true;
    let b = f.make_block(&[types::PTR, mem]);
    let p = f.block_params(b);
    let (_, size) = f.append_inst(b, Opcode::Iconst, &[], &[types::I64], Aux::Int(24));
    let (_, local) = f.append_inst(
        b,
        Opcode::StackAlloc,
        &size,
        &[types::PTR, copy_mem],
        Aux::None,
    );
    let mut copied = local[1];
    for (offset, ty) in [
        (0, types::I32),
        (4, types::I32),
        (8, types::PTR),
        (16, types::PTR),
    ] {
        let (_, index) = f.append_inst(b, Opcode::Iconst, &[], &[types::I64], Aux::Int(offset));
        let (_, src) = f.append_inst(
            b,
            Opcode::PtrOff,
            &[p[0], index[0]],
            &[types::PTR],
            Aux::Scale(1),
        );
        let (_, dst) = f.append_inst(
            b,
            Opcode::PtrOff,
            &[local[0], index[0]],
            &[types::PTR],
            Aux::Scale(1),
        );
        let (_, field) = f.append_inst(b, Opcode::Load, &[src[0], p[1]], &[ty], Aux::None);
        let (_, stored) = f.append_inst(
            b,
            Opcode::Store,
            &[field[0], dst[0], copied],
            &[copy_mem],
            Aux::None,
        );
        copied = stored[0];
    }
    let (_, value) = f.append_inst(
        b,
        Opcode::VaArg,
        &[local[0], copied],
        &[types::I64, copy_mem],
        Aux::None,
    );
    f.append_inst(
        b,
        Opcode::VaEnd,
        &[local[0], value[1]],
        &[copy_mem],
        Aux::None,
    );
    f.append_inst(b, Opcode::Ret, &[value[0]], &[], Aux::None);
    m.add_func(f);
    m
}

#[test]
fn ordinary_va_list_consumer_is_target_gated_without_a_prologue() {
    let ir = emit(&consumer(), ReleaseTarget::LinuxX64).expect("emit consumer");
    assert!(!ir.contains("llvm.va_start"));
    assert!(!ir.contains(" noinline"));
    let err = emit(&consumer(), ReleaseTarget::MacosArm64).expect_err("foreign list layout");
    assert!(err.to_string().contains("arm64-apple-macosx"));
}

fn callers() -> Module {
    let mut m = Module::new();
    let sig = m.make_sig(vec![], vec![types::I32]);
    let mut f = Function::new("call_oracle", sig);
    f.export = true;
    let b = f.make_block(&[]);
    let mut answers = Vec::new();
    for tag_value in 0..3 {
        let wide = tag_value != 0;
        let (_, tag) = f.append_inst(b, Opcode::Iconst, &[], &[types::I32], Aux::Int(tag_value));
        let (_, fixed) = f.append_inst(
            b,
            Opcode::Fconst,
            &[],
            &[types::F64],
            Aux::FloatBits(2.5f64.to_bits()),
        );
        let mut args = vec![tag[0], fixed[0]];
        let mut tys = vec![types::I32, types::F64];
        if wide {
            for i in 0..8 {
                let (_, v) =
                    f.append_inst(b, Opcode::Iconst, &[], &[types::I64], Aux::Int(100 + i));
                args.push(v[0]);
                tys.push(types::I64);
            }
            if tag_value == 1 {
                for i in 0..9 {
                    let (_, v) = f.append_inst(
                        b,
                        Opcode::Fconst,
                        &[],
                        &[types::F64],
                        Aux::FloatBits((20.5 + f64::from(i)).to_bits()),
                    );
                    args.push(v[0]);
                    tys.push(types::F64);
                }
            } else {
                // Overflow GP scalars must not force an SSE-only
                // aggregate onto the stack: its own bank still fits.
                let pair = m.types.intern(TypeData::Agg(vec![types::F64; 2]));
                let (_, a) = f.append_inst(
                    b,
                    Opcode::Fconst,
                    &[],
                    &[types::F64],
                    Aux::FloatBits(1.5f64.to_bits()),
                );
                let (_, z) = f.append_inst(
                    b,
                    Opcode::Fconst,
                    &[],
                    &[types::F64],
                    Aux::FloatBits(7.5f64.to_bits()),
                );
                let (_, v) = f.append_inst(b, Opcode::AggMake, &[a[0], z[0]], &[pair], Aux::None);
                args.push(v[0]);
                tys.push(pair);
            }
        } else {
            let (_, small) = f.append_inst(
                b,
                Opcode::Fconst,
                &[],
                &[types::F32],
                Aux::FloatBits(u64::from(1.5f32.to_bits())),
            );
            let (_, promoted) =
                f.append_inst(b, Opcode::Fpromote, &small, &[types::F64], Aux::None);
            let (_, integer) = f.append_inst(b, Opcode::Iconst, &[], &[types::I64], Aux::Int(7));
            args.extend([promoted[0], integer[0]]);
            tys.extend([types::F64, types::I64]);
        }
        let cs = m.make_sig(tys.into_iter().map(Param::val).collect(), vec![types::I32]);
        m.sigs[cs].variadic = Some(2);
        let callee = f.import_func("c.oracle", cs);
        let (_, answer) = f.append_inst(b, Opcode::Call, &args, &[types::I32], Aux::Callee(callee));
        answers.push(answer[0]);
    }
    let (_, partial) = f.append_inst(b, Opcode::IaddWrap, &answers[..2], &[types::I32], Aux::None);
    let (_, sum) = f.append_inst(
        b,
        Opcode::IaddWrap,
        &[partial[0], answers[2]],
        &[types::I32],
        Aux::None,
    );
    f.append_inst(b, Opcode::Ret, &sum, &[], Aux::None);
    m.add_func(f);
    m
}

#[test]
fn differing_call_tails_share_the_fixed_prototype() {
    let ir = emit(&callers(), ReleaseTarget::LinuxX64).expect("emit callers");
    assert_eq!(
        ir.matches("declare i32 @\"oracle\"(i32, double, ...) nounwind")
            .count(),
        1
    );
    assert_eq!(
        ir.matches("call i32 (i32, double, ...) @\"oracle\"")
            .count(),
        3
    );
    assert!(ir.contains("fpext float"));
}

#[test]
fn variadic_call_refuses_other_targets_by_name() {
    let target = ReleaseTarget::MacosArm64;
    let err = emit(&callers(), target).expect_err("unsupported variadic call");
    assert!(err.to_string().contains(target.triple()), "{err}");
}

/// Run each witness at both optimization levels: the optimized case
/// also exercises LLVM's variadic prologue/inlining boundary.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_c(tag: &str, ir: &str, c: &str) {
    use std::process::Command;
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(tag);
    std::fs::create_dir_all(&dir).unwrap();
    let ll = dir.join("input.ll");
    let source = dir.join("oracle.c");
    std::fs::write(&ll, ir).unwrap();
    std::fs::write(&source, c).unwrap();
    let clang = std::env::var("WOLF_CLANG").unwrap_or_else(|_| "clang".to_string());
    for opt in ["-O0", "-O2"] {
        let exe = dir.join(&opt[1..]);
        let out = Command::new(&clang)
            .args(["-std=c17", opt])
            .arg(&ll)
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("clang");
        assert!(
            out.status.success(),
            "{opt}: {}\n{ir}",
            String::from_utf8_lossy(&out.stderr)
        );
        let out = Command::new(exe).output().expect("execute ABI witness");
        assert!(
            out.status.success(),
            "{opt}: {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn c_calls_variadic_definitions_across_both_register_banks_and_stack() {
    let ir = emit(&readers(), ReleaseTarget::LinuxX64).unwrap();
    run_c(
        "s263_llvm_readers",
        &ir,
        r#"
struct gp { long v[8]; };
struct fp { double v[10]; };
struct mixed { double f; long i; };
struct pair { double a,b; };
struct big { long a,b,c; };
struct mixout { struct mixed a; long b,c,d; struct mixed e; double f; };
struct sseout { struct pair a; long b; double c; };
struct namedout { long a; double b; void *c; struct big d; };
extern struct gp read_gp(int,...);
extern struct fp read_fp(double,...);
extern struct mixout read_mixed(int,...);
extern struct sseout read_sse_rollback(double,double,double,double,double,double,double,...);
extern struct namedout read_named_stack(long,long,long,long,long,long,long,long,double,double,double,double,double,double,double,double,double,...);
int main(void) {
    struct gp g = read_gp(0,11L,12L,13L,14L,15L,16L,17L,18L);
    for(int i=0;i<8;i++) if(g.v[i] != 11+i) return 10+i;
    struct fp f = read_fp(99.0,0.5,1.5,2.5,3.5,4.5,5.5,6.5,7.5,8.5,9.5);
    for(int i=0;i<10;i++) if(f.v[i] != 0.5+i) return 30+i;
    struct mixout m = read_mixed(0,(struct mixed){2.5,12},20L,21L,22L,(struct mixed){3.5,13},4.5);
    if(m.a.f!=2.5 || m.a.i!=12 || m.b!=20 || m.c!=21 || m.d!=22 || m.e.f!=3.5 || m.e.i!=13 || m.f!=4.5) return 50;
    struct sseout s = read_sse_rollback(1.,2.,3.,4.,5.,6.,7.,(struct pair){8.5,9.5},23L,10.5);
    if(s.a.a!=8.5 || s.a.b!=9.5 || s.b!=23 || s.c!=10.5) return 51;
    int marker;
    struct namedout n = read_named_stack(1L,2L,3L,4L,5L,6L,7L,8L,1.,2.,3.,4.,5.,6.,7.,8.,9.,24L,11.5,(void*)&marker,(struct big){31,32,33});
    if(n.a!=24 || n.b!=11.5 || n.c!=&marker || n.d.a!=31 || n.d.b!=32 || n.d.c!=33) return 52;
    return 0;
}
"#,
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn wolf_calls_c_with_promoted_float_fixed_sse_and_different_tails() {
    let ir = emit(&callers(), ReleaseTarget::LinuxX64).unwrap();
    run_c(
        "s263_llvm_callers",
        &ir,
        r#"
#include <stdarg.h>
struct pair { double a,b; };
int oracle(int tag, double fixed, ...) {
    va_list ap;
    va_start(ap, fixed);
    int bad = fixed != 2.5;
    if (!tag) {
        bad |= va_arg(ap,double) != 1.5;
        bad |= va_arg(ap,long) != 7;
    } else {
        for(int i=0;i<8;i++) bad |= va_arg(ap,long) != 100+i;
        if (tag == 1) {
            for(int i=0;i<9;i++) bad |= va_arg(ap,double) != 20.5+i;
        } else {
            struct pair p=va_arg(ap,struct pair);
            bad |= p.a!=1.5 || p.b!=7.5;
        }
    }
    va_end(ap);
    return bad;
}
extern int call_oracle(void);
int main(void) { return call_oracle(); }
"#,
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn ordinary_consumer_accepts_a_c_created_list_and_advances_it() {
    let ir = emit(&consumer(), ReleaseTarget::LinuxX64).unwrap();
    run_c(
        "s263_llvm_consumer",
        &ir,
        r#"
#include <stdarg.h>
extern long consume_list(va_list);
extern long consume_copy(va_list);
static int check(int n, ...) {
    va_list ap;
    va_start(ap,n);
    long copy=consume_copy(ap);
    long a=consume_list(ap), b=consume_list(ap);
    va_end(ap);
    return copy!=41 || a!=41 || b!=42;
}
int main(void) { return check(2,41L,42L); }
"#,
    );
}

/// A list escapes to an external C consumer, so its SSE save slots
/// cannot be optimized away even though the Wolf body is integer-only.
fn freestanding_readers() -> Module {
    let mut m = Module::new();
    reader(&mut m, "none_read", &[types::I32], &[types::I64; 8]);
    let mem = m.types.mem(RegionId::new(0));
    let sig = m.make_sig(
        vec![Param::val(types::I32), Param::val(mem)],
        vec![types::I64],
    );
    m.sigs[sig].variadic = Some(1);
    let callee_sig = m.make_sig(
        vec![Param::val(types::PTR), Param::val(mem)],
        vec![types::I64],
    );
    let mut f = Function::new("none_forward", sig);
    f.export = true;
    let b = f.make_block(&[types::I32, mem]);
    let token = f.block_params(b)[1];
    let (_, ap) = f.append_inst(b, Opcode::VaStart, &[], &[types::PTR], Aux::None);
    let callee = f.import_func("c.inspect_cursor", callee_sig);
    let (_, value) = f.append_inst(
        b,
        Opcode::Call,
        &[ap[0], token],
        &[types::I64, mem],
        Aux::Callee(callee),
    );
    f.append_inst(b, Opcode::VaEnd, &[ap[0], value[1]], &[mem], Aux::None);
    f.append_inst(b, Opcode::Ret, &[value[0]], &[], Aux::None);
    m.add_func(f);

    let sig = m.make_sig(vec![Param::val(types::I64)], vec![types::I64]);
    let mut f = Function::new("none_plain", sig);
    f.export = true;
    let b = f.make_block(&[types::I64]);
    let p = f.block_params(b);
    f.append_inst(b, Opcode::Ret, &p, &[], Aux::None);
    m.add_func(f);
    m
}

#[test]
fn freestanding_only_variadic_definitions_enable_abi_sse_saves() {
    let ir = emit(&freestanding_readers(), ReleaseTarget::X86_64None).unwrap();
    for name in ["none_read", "none_forward"] {
        let line = ir
            .lines()
            .find(|line| line.starts_with("define ") && line.contains(name))
            .unwrap();
        assert!(line.contains(" noimplicitfloat"), "{line}");
        assert!(line.contains(" noredzone"), "{line}");
        assert!(line.contains(" noinline"), "{line}");
        assert!(line.contains("\"frame-pointer\"=\"all\""), "{line}");
        assert!(line.contains("+sse,+sse2"), "{line}");
        assert!(line.contains("-avx,-avx2,-soft-float"), "{line}");
    }
    let plain = ir
        .lines()
        .find(|line| line.starts_with("define ") && line.contains("none_plain"))
        .unwrap();
    assert!(plain.ends_with(" noredzone \"frame-pointer\"=\"all\" \"target-features\"=\"-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,+soft-float\" {"), "{plain}");
    assert!(
        !plain.contains("noimplicitfloat"),
        "ordinary definition attributes must remain unchanged"
    );
}

/// Reuse the same incoming-AL proof as the driver's acceptance gate.
/// These fixtures additionally pin the expected save counts: the list
/// escaping to C needs all eight FP slots; the plain function needs none.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn check_none_disassembly(text: &str) {
    variadic_guard::check(text).unwrap_or_else(|why| panic!("{why}:\n{text}"));
    for name in ["none_plain", "none_read", "none_forward"] {
        let mut selected = String::new();
        let mut collecting = false;
        for line in text.lines() {
            if line.trim_end().ends_with(">:") {
                collecting = line.trim_end().ends_with(&format!("<{name}>:"));
            }
            if collecting {
                selected.push_str(line);
                selected.push('\n');
            }
        }
        let saves =
            variadic_guard::check(&selected).unwrap_or_else(|why| panic!("{name}: {why}:\n{text}"));
        match name {
            "none_plain" => assert_eq!(saves, 0, "ordinary function acquired SSE:\n{text}"),
            "none_forward" => assert_eq!(saves, 8, "escaping list lost SSE saves:\n{text}"),
            _ => assert!(matches!(saves, 0 | 8), "partial ABI save block:\n{text}"),
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn freestanding_sse_is_confined_to_the_al_guarded_save_block() {
    use std::process::Command;
    let ir = emit(&freestanding_readers(), ReleaseTarget::X86_64None).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("s263_llvm_none_saves");
    std::fs::create_dir_all(&dir).unwrap();
    let ll = dir.join("input.ll");
    std::fs::write(&ll, &ir).unwrap();
    let clang = std::env::var("WOLF_CLANG").unwrap_or_else(|_| "clang".to_string());
    let objdump = std::env::var("WOLF_OBJDUMP").unwrap_or_else(|_| "objdump".to_string());
    for opt in ["-O0", "-O2"] {
        let object = dir.join(format!("{}.o", &opt[1..]));
        let out = Command::new(&clang)
            .args(["--target=x86_64-unknown-none-elf", opt, "-c"])
            .arg(&ll)
            .arg("-o")
            .arg(&object)
            .output()
            .expect("clang");
        assert!(
            out.status.success(),
            "{opt}: {}\n{ir}",
            String::from_utf8_lossy(&out.stderr)
        );
        let out = Command::new(&objdump)
            .args(["-d", "--no-show-raw-insn"])
            .arg(&object)
            .output()
            .expect("objdump");
        assert!(
            out.status.success(),
            "objdump: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        std::fs::write(dir.join(format!("{}.objdump", &opt[1..])), &text).unwrap();
        check_none_disassembly(&text);
    }
}
