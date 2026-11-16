//! Toolchain-only cold placement must survive optimization without becoming
//! new source syntax or changing canonical WIR when no policy is selected.

use wolf_wir::midend::{Options, dedup, optimize_whole_program, summary::Homes};
use wolf_wir::{parse_module, print_module, verify_module};

const SOURCE: &str = "fn @helper(i64) -> i64 {\n\
 b0(%0: i64):\n  %1 = iadd.chk %0, %0\n  ret %1\n}\n\
 fn @main(i64) -> i64 {\n\
 b0(%0: i64):\n  %1 = call @helper(%0)\n  ret %1\n}\n";

#[test]
fn a_selected_cold_body_remains_out_of_line_through_the_pipeline() {
    let mut module = parse_module(SOURCE).unwrap();
    for id in module.funcs.keys() {
        let function = &mut module.funcs[id];
        if function.name == "helper" {
            function.cold = true;
            function.code_home = Some("format".into());
        }
    }
    optimize_whole_program(
        &mut module,
        &Homes::single(),
        &Options {
            verify_each: true,
            ..Options::default()
        },
    )
    .unwrap();
    verify_module(&module).unwrap();
    let helper = module
        .funcs
        .values()
        .find(|f| f.name == "helper")
        .expect("the selected body is not dissolved");
    assert!(helper.cold);
    assert_eq!(helper.code_home.as_deref(), Some("format"));
    assert!(print_module(&module).contains("call @helper"));
}

#[test]
fn dedup_cannot_replace_either_side_of_a_cold_boundary() {
    let extra = "fn @hot(i64) -> i64 {\nb0(%0: i64):\n  %1 = iadd.chk %0, %0\n  ret %1\n}\n";
    let source = format!("{SOURCE}{extra}");
    let mut ordinary = parse_module(&source).unwrap();
    assert_eq!(
        dedup::dedup(&mut ordinary).bodies_merged,
        1,
        "fixture control"
    );
    let mut selected = parse_module(&source).unwrap();
    let helper = selected
        .funcs
        .iter()
        .find(|(_, f)| f.name == "helper")
        .unwrap()
        .0;
    selected.funcs[helper].cold = true;
    assert_eq!(dedup::dedup(&mut selected).bodies_merged, 0);
    assert_eq!(selected.funcs.len(), 3);
}
