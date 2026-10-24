//! Copy elision for a large aggregate lent from an exclusive address.

use std::collections::HashMap;
use wolf_wir::ir::{Function, Module, Value};

/// Aggregate SSA values that may use their source address for one read call.
pub fn borrowed_aggregates(_m: &Module, _f: &Function) -> HashMap<Value, Value> {
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(before: &str, after: &str) -> String {
        format!(
            "decl @reader({{i64, i64, i64}}) -> i64
             fn @reader({{i64, i64, i64}}) -> i64 {{
             b0(%0: {{i64, i64, i64}}):
               %1 = agg.get %0, 0
               ret %1
             }}
             fn @caller(mut ptr, mem.r0) -> i64 {{
             b0(%0: ptr, %1: mem.r0):
               %2 = iconst.i64 8
               %3 = ptr.off %0, %2, 1
               %4 = iconst.i64 16
               %5 = ptr.off %0, %4, 1
               %6 = load.i64 %0, %1
               %7 = load.i64 %3, %1
               %8 = load.i64 %5, %1
               %9 = agg.make %6, %7, %8
               {before}
               %10 = call @reader(%9)
               {after}
               ret %10
             }}"
        )
    }

    fn count(src: &str) -> usize {
        let m = wolf_wir::parse_module(src).expect("parse");
        wolf_wir::verify_module(&m).expect("verify");
        let f = m.funcs.values().find(|f| f.name == "caller").unwrap();
        borrowed_aggregates(&m, f).len()
    }

    #[test]
    fn lends_the_exclusive_address_until_the_read_call_returns() {
        assert_eq!(count(&fixture("", "%11 = store.i64 %10, %0, %1")), 1);
    }

    #[test]
    fn keeps_a_snapshot_used_again_after_the_call() {
        assert_eq!(count(&fixture("", "%11 = agg.get %9, 1")), 0);
    }

    #[test]
    fn keeps_a_snapshot_across_an_intervening_write() {
        assert_eq!(count(&fixture("%11 = store.i64 %2, %0, %1", "")), 0);
    }

    #[test]
    fn keeps_a_snapshot_without_exclusive_provenance() {
        let src = fixture("", "").replace("@caller(mut ptr", "@caller(ptr");
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn keeps_reordered_fields() {
        let src = fixture("", "").replace("agg.make %6, %7, %8", "agg.make %8, %7, %6");
        assert_eq!(count(&src), 0);
    }

    #[test]
    fn does_not_change_take_or_external_calls() {
        let src = fixture("", "").replace("@reader({i64", "@reader(take {i64");
        assert_eq!(count(&src), 0);
        let src = fixture("", "").replace("call @reader", "call @external");
        let src = format!("decl @external({{i64, i64, i64}}) -> i64\n{src}");
        assert_eq!(count(&src), 0);
    }
}
