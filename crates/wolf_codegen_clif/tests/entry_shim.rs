//! s267 (wolf-lang#962, `[abi.target.main]`): the entry shim is the C
//! `int main(int argc, char **argv, char **envp)`, and its first call
//! hands exactly those three values to the runtime (`__wolf_rt_main_start`)
//! in place of the niladic `__wolf_rt_stack_start()` it made before, so
//! the shim's instruction count does not move. The shape is the backends'
//! shared input (the driver builds it once, before choosing a tier), so it
//! is pinned here on every host; the executed rows are
//! `wolf_driver/tests/static_entry.rs` (linux) and `exec_smoke.rs`.

use wolf_codegen_clif::{C_MAIN_PARAMS, MAIN_START, add_entry_shim};
use wolf_wir::ir::Aux;
use wolf_wir::ops::Opcode;
use wolf_wir::types;

/// The shim's callees, in order, and the arguments of the first.
fn shim(text: &str) -> (Vec<String>, bool, usize) {
    let mut m = wolf_wir::parse_module(text).expect("wir parses");
    let id = add_entry_shim(&mut m).expect("entry shim");
    let f = &m.funcs[id];
    let sd = &m.sigs[f.sig];
    let params: Vec<_> = sd.params.iter().map(|p| p.ty).collect();
    assert_eq!(params, C_MAIN_PARAMS, "the shim's parameters are main's three");
    assert_eq!(sd.results, vec![types::I32], "the shim returns the C exit code");
    let entry = f.entry().expect("an entry block");
    let entry_params = f.block_params(entry);
    assert_eq!(entry_params.len(), 3);
    let mut callees = Vec::new();
    let mut first_args_are_params = false;
    let mut insts = 0;
    for &b in &f.layout {
        for &i in &f.blocks[b].insts {
            insts += 1;
            let d = &f.insts[i];
            if let (Opcode::Call, Aux::Callee(ef)) = (d.op, &d.aux) {
                let ext = &f.ext_funcs[*ef];
                if callees.is_empty() {
                    first_args_are_params = f.vpool.get(d.args) == entry_params;
                    let start = &m.sigs[ext.sig];
                    let tys: Vec<_> = start.params.iter().map(|p| p.ty).collect();
                    assert_eq!(tys, C_MAIN_PARAMS, "the start call takes main's three");
                    assert!(start.results.is_empty());
                }
                callees.push(ext.name.clone());
            }
        }
    }
    (callees, first_args_are_params, insts)
}

#[test]
fn a_unit_main_hands_mains_arguments_to_the_runtime_first() {
    let (callees, args, insts) = shim("fn @main() {\nb0:\n  ret\n}\n");
    assert_eq!(callees, [MAIN_START, "main"]);
    assert!(args, "the start call's arguments are the shim's own parameters");
    // The start call, wolf's main, the exit code's constant, the return:
    // what the niladic shim had, one call for one call.
    assert_eq!(insts, 4);
}

#[test]
fn an_int_main_hands_mains_arguments_to_the_runtime_first() {
    let (callees, args, insts) = shim("fn @main() -> i64 {\nb0:\n  %r = iconst.i64 7\n  ret %r\n}\n");
    assert_eq!(callees, [MAIN_START, "main"]);
    assert!(args);
    // The start call, wolf's main, the truncation, the return.
    assert_eq!(insts, 4);
    assert!(!callees.iter().any(|c| c == "__wolf_rt_stack_start"));
}
