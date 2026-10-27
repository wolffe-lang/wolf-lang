//! s268: do not admit token CFGs before splice can reconstruct their finals.
use wolf_wir::midend::{OptStats, Options, run_named_pass};
use wolf_wir::{parse_module, print_module, verify_module};

fn check(multi_return: bool) {
    let left = if multi_return {
        "ret %4"
    } else {
        "jmp b3(%4, %5)"
    };
    let right = if multi_return {
        "ret %7"
    } else {
        "jmp b3(%7, %8)"
    };
    let join = if multi_return {
        ""
    } else {
        "b3(%9: i64, %10: mem.r0):\n  ret %9\n"
    };
    let src = format!(
        "fn @leaf(mut ptr, bool, mem.r0) -> i64 {{
b0(%0: ptr, %1: bool, %2: mem.r0):
  br %1, b1(%2), b2(%2)
b1(%3: mem.r0):
  %4 = iconst.i64 11
  %5 = store.i64 %4, %0, %3
  {left}
b2(%6: mem.r0):
  %7 = iconst.i64 22
  %8 = store.i64 %7, %0, %6
  {right}
{join}}}
fn @caller(mut ptr, bool, mem.r1) -> i64 {{
b0(%0: ptr, %1: bool, %2: mem.r1):
  %3, %4 = call @leaf(%0, %1, %2)
  %5 = load.i64 %0, %4
  ret %5
}}"
    );
    let mut m = parse_module(&src).expect("fixture parses");
    verify_module(&m).expect("fixture verifies before");
    let opts = Options {
        verify_each: true,
        ..Options::default()
    };
    let mut stats = OptStats::default();
    for f in m.funcs.keys().collect::<Vec<_>>() {
        run_named_pass(&mut m, f, "inline", &opts, &mut stats).expect("token finals remain valid");
    }
    verify_module(&m).expect("fixture verifies after");
    assert_eq!(
        stats.inlined_calls, 0,
        "multi-block token admission requires generalized finals"
    );
    assert!(print_module(&m).contains("call @leaf"));
}

#[test]
fn single_return_token_cfg_stays_out_of_line() {
    check(false);
}

#[test]
fn multi_return_token_cfg_stays_out_of_line() {
    check(true);
}
