//! Braun-correctness snapshots over inline programs: loop-carried
//! values appear as block parameters with NO trivial parameters
//! remaining, sealing works on nested control shapes, and every dump
//! passes the verifier plus the print→parse→print fixpoint. Plus the
//! reducible-CFG property suite: 200 seeded random nested if/while
//! ASTs, lowered and checked for verifier-cleanliness, fixpoint, and
//! the no-trivial-params invariant (the acceptance proptest).

use wolf_wir::entity::EntityRef;
use wolf_wir::ir::{Aux, ValueDef};
use wolf_wir::{lower_package, print_module, verify_module};

use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

/// Lower one inline program (must pass every rung through `mem`).
fn lower(src: &str) -> wolf_wir::Build {
    let mut ml = MemoryLoader::new("wirlow");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input resolves clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(tc.not_yet.is_empty(), "typed fully: {:?}", tc.not_yet);
    assert!(!tc.has_errors(), "typed clean: {:?}", tc.diagnostics);
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(mem.not_yet.is_empty(), "mem surface: {:?}", mem.not_yet);
    assert!(
        mem.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "mem clean: {:?}",
        mem.diagnostics
    );
    lower_package(&res.package, &tc)
}

/// Lower, demand zero refusals, verify, fixpoint, assert no trivial
/// params; return the canonical dump.
fn dump(src: &str) -> String {
    let build = lower(src);
    assert!(
        build.not_yet.is_empty(),
        "expected full lowering, refused: {:?}",
        build.not_yet
    );
    verify_module(&build.module).expect("lowered module verifies");
    let printed = print_module(&build.module);
    let reparsed = wolf_wir::parse_module(&printed).expect("reparses");
    verify_module(&reparsed).expect("reparsed verifies");
    assert_eq!(print_module(&reparsed), printed, "fixpoint");
    assert_no_trivial_params(&build.module);
    printed
}

/// No non-entry block parameter may be trivial: for each param, the
/// incoming branch args must include at least two distinct non-self
/// values. (The Braun trivial-φ test as a global postcondition.)
fn assert_no_trivial_params(module: &wolf_wir::Module) {
    for func in module.funcs.values() {
        let entry = func.entry().expect("entry");
        // Collect incoming args per (block, param index).
        let mut incoming: std::collections::HashMap<(u32, usize), Vec<wolf_wir::Value>> =
            std::collections::HashMap::new();
        for &b in &func.layout {
            let Some(&last) = func.blocks[b].insts.last() else {
                continue;
            };
            let mut edges = Vec::new();
            match func.insts[last].aux {
                Aux::Jump(e) => edges.push(e),
                Aux::Br(t, e) => {
                    edges.push(t);
                    edges.push(e);
                }
                _ => {}
            }
            for edge in edges {
                for (i, arg) in func.vpool.get(edge.args).into_iter().enumerate() {
                    incoming
                        .entry((edge.block.as_u32(), i))
                        .or_default()
                        .push(arg);
                }
            }
        }
        for &b in &func.layout {
            if b == entry {
                continue; // signature params
            }
            for (i, &p) in func.vpool.get(func.blocks[b].params).iter().enumerate() {
                let args = incoming.get(&(b.as_u32(), i)).cloned().unwrap_or_default();
                let distinct: std::collections::BTreeSet<u32> = args
                    .iter()
                    .filter(|&&a| a != p)
                    .map(|a| a.as_u32())
                    .collect();
                assert!(
                    distinct.len() >= 2 || is_single_pred_merge(func, b),
                    "trivial block param {p:?} (block {b:?}, index {i}) in @{}:\n{}",
                    func.name,
                    print_module(module)
                );
            }
        }
    }
    // A param on a single-predecessor block (a short-circuit merge
    // whose other side diverged) carries one incoming value by
    // construction; it is not a *trivial* φ in the Braun sense unless
    // its one arg equals the value on every path — which single-pred
    // params trivially satisfy, so exempt them explicitly.
    fn is_single_pred_merge(func: &wolf_wir::Function, block: wolf_wir::ir::Block) -> bool {
        let mut preds = 0;
        for &b in &func.layout {
            let Some(&last) = func.blocks[b].insts.last() else {
                continue;
            };
            match func.insts[last].aux {
                Aux::Jump(e) => {
                    if e.block == block {
                        preds += 1;
                    }
                }
                Aux::Br(t, e) => {
                    if t.block == block {
                        preds += 1;
                    }
                    if e.block == block {
                        preds += 1;
                    }
                }
                _ => {}
            }
        }
        preds <= 1
    }
}

// -------------------------------------------------------- snapshots ----

#[test]
fn loop_carried_value_is_a_block_param() {
    // sum and i are loop-carried: both must surface as header params;
    // no trivial params may remain.
    insta::assert_snapshot!(dump(
        "fn main() -> !int {\n\
         \x20   var sum = 0\n\
         \x20   var i = 0\n\
         \x20   while i < 10 {\n\
         \x20       sum = sum + i\n\
         \x20       i = i + 1\n\
         \x20   }\n\
         \x20   sum\n\
         }\n"
    ));
}

#[test]
fn loop_invariant_var_makes_no_param() {
    // k is defined before the loop and only read inside: the Braun
    // trivial-param test must eliminate any placeholder for it.
    insta::assert_snapshot!(dump(
        "fn main() -> !int {\n\
         \x20   let k = 3\n\
         \x20   var i = 0\n\
         \x20   while i < k {\n\
         \x20       i = i + k\n\
         \x20   }\n\
         \x20   i\n\
         }\n"
    ));
}

#[test]
fn nested_if_in_loop_with_break_continue() {
    insta::assert_snapshot!(dump(
        "fn collatz_steps(n0: int) -> int {\n\
         \x20   var n = n0\n\
         \x20   var steps = 0\n\
         \x20   loop {\n\
         \x20       if n == 1 { break }\n\
         \x20       if n % 2 == 0 { n = n / 2 } else { n = 3 * n + 1 }\n\
         \x20       steps = steps + 1\n\
         \x20   }\n\
         \x20   steps\n\
         }\n\
         fn main() -> !int {\n\
         \x20   collatz_steps(6) - 8\n\
         }\n"
    ));
}

#[test]
fn if_value_rides_a_merge_param() {
    insta::assert_snapshot!(dump(
        "fn pick(c: bool, a: int, b: int) -> int {\n\
         \x20   if c { a } else { b }\n\
         }\n\
         fn main() -> !int {\n\
         \x20   pick(true, 0, 1)\n\
         }\n"
    ));
}

#[test]
fn short_circuit_and_or_not() {
    insta::assert_snapshot!(dump(
        "fn f(a: bool, b: bool, x: int) -> int {\n\
         \x20   if a && (x > 3 || !b) { 1 } else { 2 }\n\
         }\n\
         fn main() -> !int {\n\
         \x20   f(true, false, 0) - 1\n\
         }\n"
    ));
}

#[test]
fn gvn_dedups_repeated_subexpressions() {
    // x*x appears three times; the dump must contain exactly one imul.
    let out = dump(
        "fn sq3(x: int) -> int {\n\
         \x20   x * x + x * x + x * x\n\
         }\n\
         fn main() -> !int {\n\
         \x20   sq3(0)\n\
         }\n",
    );
    assert_eq!(out.matches("imul.chk").count(), 1, "{out}");
    insta::assert_snapshot!(out);
}

#[test]
fn gvn_misses_across_arms_but_dominating_entries_hit() {
    // y+1 computed in both arms may NOT be unified (neither dominates
    // the other); x+2 before the branch is reused inside both arms.
    let out = dump(
        "fn g(c: bool, x: int, y: int) -> int {\n\
         \x20   let a = x + 2\n\
         \x20   let r = if c { (x + 2) + (y + 1) } else { (y + 1) - (x + 2) }\n\
         \x20   r + a\n\
         }\n\
         fn main() -> !int {\n\
         \x20   g(true, 0, 0) - 3\n\
         }\n",
    );
    // x+2: exactly one instance, reused inside both arms AND after the
    // merge (a dominating GVN entry survives the arm scopes). y+1: two
    // instances (non-dominating arms must not share). Plus the arm's
    // own + and the trailing r + a: five iadds, one x+2.
    assert_eq!(out.matches("iadd.chk").count(), 5, "{out}");
    assert_eq!(out.matches("iconst.i64 2").count(), 1, "{out}");
    assert_eq!(out.matches("iconst.i64 1").count(), 2, "{out}");
    insta::assert_snapshot!(out);
}

/// The #67 pair, red-then-green (s74). A `match` whose discriminant is
/// narrower than i64 — here a `for` induction variable, the book's ch03
/// §15 shape — used to emit its arm tests as `icmp` against an i64
/// constant and fail `[type]` verification. `dump` verifies, so the
/// test IS the regression; the assertion pins the width so a later
/// refactor cannot quietly restore i64.
#[test]
fn match_arm_consts_take_the_discriminant_width() {
    let out = dump(
        "fn main() -> !int {\n\
         \x20   var hits = 0\n\
         \x20   for code in 0..3 {\n\
         \x20       hits += match code {\n\
         \x20           0 => 10,\n\
         \x20           1 | 2 => 20,\n\
         \x20           _ => 0,\n\
         \x20       }\n\
         \x20   }\n\
         \x20   hits - 50\n\
         }\n",
    );
    assert!(
        !out.contains("icmp.eq") || out.contains("iconst.i32"),
        "arm constants must share the i32 discriminant's width: {out}"
    );
    insta::assert_snapshot!(out);
}

/// The #67 pair's other half: a write through a `mut` parameter whose
/// type is a flat aggregate (`str` is `{ptr, i64}`) stores field by
/// field. One aggregate `store` is not a legal WIR instruction, and the
/// book's ch07 §12 `swap` produced exactly one.
#[test]
fn mut_param_aggregate_writes_are_fieldwise() {
    let out = dump(
        "fn swap(mut a: str, mut b: str) {\n\
         \x20   let t = move a\n\
         \x20   a = move b\n\
         \x20   b = t\n\
         }\n\
         fn main() -> !int {\n\
         \x20   var x = \"one\"\n\
         \x20   var y = \"two\"\n\
         \x20   swap(mut x, mut y)\n\
         \x20   0\n\
         }\n",
    );
    assert!(
        !out.contains("store.{"),
        "no aggregate store may survive lowering: {out}"
    );
    insta::assert_snapshot!(out);
}

/// #66 (s74): an INCLUSIVE `for` whose body can exit early, followed by
/// a read of a binding declared before it. Resolving that read cascades
/// the Braun trivial-φ test, and the cascade used to retire the very
/// parameter the outer removal had chosen as its replacement — the
/// outer frame kept the dead handle and the next instruction took a
/// phantom operand. `dump` verifies and reparses, which is exactly the
/// check that caught it.
#[test]
fn inclusive_for_then_later_read_keeps_live_definitions() {
    let out = dump(
        "fn main() -> !int {\n\
         \x20   let ch = channel[int](8)\n\
         \x20   for i in 1..=3 { ch.send(i) }\n\
         \x20   ch.close()\n\
         \x20   var sum = 0\n\
         \x20   for v in ch { sum += v }\n\
         \x20   sum - 6\n\
         }\n",
    );
    assert!(!out.contains("%?"), "no value may go unnamed: {out}");
    insta::assert_snapshot!(out);
}

/// s77 acceptance (wolf-lang#80): a byte walk emits **no per-element
/// call and no allocation**. `bytes()` is the receiver's own
/// `{ptr, len}` pair, so the loop body is `ptr.off` at stride 1 plus
/// `load.i8` plus `zext` — the s75 gep+load shape at the stride bytes
/// actually have. Before s77 this body was
/// `call @__wolf_rt_str_bytes` outside the loop (eight heap bytes per
/// input byte) and a `load.i64` inside.
#[test]
fn byte_walk_has_no_call_and_no_allocation() {
    let out = dump(
        "fn walk(text: str) -> int {\n\
         \x20   var n = 0\n\
         \x20   for b in text.bytes() {\n\
         \x20       n = n + b\n\
         \x20   }\n\
         \x20   n\n\
         }\n\
         fn main() -> !int {\n\
         \x20   walk(\"wolf\") - 440\n\
         }\n",
    );
    let walk = out
        .split("fn @walk")
        .nth(1)
        .expect("walk lowered")
        .split("\nfn ")
        .next()
        .expect("body");
    assert!(
        !walk.contains("call "),
        "a byte walk must emit no call: {walk}"
    );
    for alloc in ["region.alloc", "stack.alloc", "region.new"] {
        assert!(
            !walk.contains(alloc),
            "a byte walk must allocate nothing ({alloc}): {walk}"
        );
    }
    assert!(walk.contains("ptr.off"), "element address: {walk}");
    assert!(walk.contains("load.i8"), "one byte per element: {walk}");
    assert!(walk.contains("zext.i64"), "bytes widen UNSIGNED: {walk}");
    insta::assert_snapshot!(out);
}

/// s77: `<str>.bytes()[i]` is the same access with the s75 caller-side
/// bounds check (one unsigned compare, `trap.bounds` on the miss) — and
/// `s[a..b]` stops being a `__wolf_rt_str_get` call: the domain is two
/// unsigned compares plus one guarded byte probe per endpoint
/// (`[mem.str.get]`, inline), and the result is address arithmetic.
#[test]
fn byte_index_and_str_slice_emit_no_runtime_call() {
    let out = dump(
        "fn at(text: str, i: int) -> int {\n\
         \x20   text.bytes()[i]\n\
         }\n\
         fn cut(text: str, i: int) -> str {\n\
         \x20   text[i..i + 2]\n\
         }\n\
         fn main() -> !int {\n\
         \x20   at(\"wolf\", 0) - 119 + cut(\"wolf\", 0).len - 2\n\
         }\n",
    );
    for f in ["fn @at", "fn @cut"] {
        let body = out
            .split(f)
            .nth(1)
            .expect("lowered")
            .split("\nfn ")
            .next()
            .expect("body");
        assert!(!body.contains("call "), "{f} must emit no call: {body}");
        assert!(body.contains("trap.bounds"), "{f} still traps: {body}");
    }
    assert!(
        !out.contains("__wolf_rt_str_get") && !out.contains("__wolf_rt_str_bytes"),
        "neither shim is reachable from these shapes: {out}"
    );
    insta::assert_snapshot!(out);
}

/// s81 acceptance (the c09 contract): `==` and `!=` on `str` stop being
/// a call. `__wolf_rt_str_eq` was the one call left inside
/// `d2_substr_search`'s loop after s77 inlined the slice, and s77's A/B
/// priced it at 50x. What replaces it is the length guard plus the s77
/// byte view read from two bases: `ptr.off` at stride 1, `load.i8`,
/// `zext`, one `icmp` per byte — and no call on that path.
///
/// The shape here is the one the bench measures — a `str` parameter
/// against a `str` parameter, so neither length is a build-time
/// constant. That is also the ONLY shape that still names the shim: the
/// contract's long-operand route (`STR_EQ_INLINE_MAX`) is a guarded
/// branch to `__wolf_rt_str_eq`, and a dynamic length cannot fold it
/// away. So the assertion is exact rather than blanket — the compare
/// emits ONE call site, it is the memcmp route, and it is not on the
/// scan path. Constant-length operands emit none at all
/// (`str_match_dispatch_is_call_free`).
///
/// `!=` is the same shape with the two answers swapped, which is why it
/// costs exactly what `==` costs.
#[test]
fn str_equality_emits_no_call_on_the_byte_path() {
    let out = dump(
        "fn same(a: str, b: str) -> int {\n\
         \x20   if a == b { 1 } else { 0 }\n\
         }\n\
         fn differ(a: str, b: str) -> int {\n\
         \x20   if a != b { 1 } else { 0 }\n\
         }\n\
         fn main() -> !int {\n\
         \x20   same(\"wolf\", \"wolf\") - 1 + differ(\"wolf\", \"wolves\") - 1\n\
         }\n",
    );
    for f in ["fn @same", "fn @differ"] {
        let body = out
            .split(f)
            .nth(1)
            .expect("lowered")
            .split("\nfn ")
            .next()
            .expect("body");
        assert_eq!(
            body.matches("call ").count(),
            1,
            "{f} keeps only the guarded long-operand route: {body}"
        );
        assert!(
            body.contains("call @__wolf_rt_str_eq"),
            "{f}'s one call is the memcmp route: {body}"
        );
        assert!(body.contains("ptr.off"), "{f} addresses bytes: {body}");
        assert!(body.contains("load.i8"), "{f} reads one byte: {body}");
        assert!(body.contains("zext.i64"), "{f} widens UNSIGNED: {body}");
    }
    insta::assert_snapshot!(out);
}

/// s81: an operand whose length is a build-time constant — every
/// `match` arm, and every compare against a literal — folds the
/// long-operand threshold away, so the `__wolf_rt_str_eq` route is not
/// even present as dead code. A `match` over str literals is the
/// dispatch shape #54 introduced, and it is now call-free end to end.
#[test]
fn str_match_dispatch_is_call_free() {
    let out = dump(
        "fn kind(tok: str) -> int {\n\
         \x20   match tok {\n\
         \x20       \"+\" => 1,\n\
         \x20       \"-\" | \"*\" => 2,\n\
         \x20       _ => 3,\n\
         \x20   }\n\
         }\n\
         fn main() -> !int {\n\
         \x20   kind(\"+\") + kind(\"*\") + kind(\"7\") - 6\n\
         }\n",
    );
    let body = out
        .split("fn @kind")
        .nth(1)
        .expect("lowered")
        .split("\nfn ")
        .next()
        .expect("body");
    assert!(!body.contains("call "), "dispatch emits no call: {body}");
    assert!(
        !out.contains("__wolf_rt_str_eq"),
        "the shim is unreachable from a literal dispatch: {out}"
    );
    insta::assert_snapshot!(out);
}

/// s81: past the inline threshold the compare routes to
/// `__wolf_rt_str_eq`, whose body is a `memcmp` — 1.45 ns/byte inline
/// against 0.024 through the call, measured on 4 KiB operands. The
/// route is guarded, so the short path still reaches the byte loop with
/// no call executed; this pins that BOTH arms exist for a
/// dynamic-length operand.
#[test]
fn long_operands_route_to_the_memcmp_shim() {
    let out = dump(
        "fn same(a: str, b: str) -> int {\n\
         \x20   if a == b { 1 } else { 0 }\n\
         }\n\
         fn main() -> !int {\n\
         \x20   same(\"wolf\", \"wolf\") - 1\n\
         }\n",
    );
    let body = out
        .split("fn @same")
        .nth(1)
        .expect("lowered")
        .split("\nfn ")
        .next()
        .expect("body");
    assert!(
        body.contains("iconst.i64 64"),
        "the threshold is a constant in the IR: {body}"
    );
    assert!(
        body.contains("load.i8"),
        "the short path is still the byte loop: {body}"
    );
}

/// s77: a `bytes()` result that must be a first-class `List[int]`
/// value — bound, passed, returned — still MATERIALIZES through
/// `__wolf_rt_str_bytes`, bit-for-bit as before. The view is not a
/// value: it never escapes into the IR, so nothing downstream can
/// mistake it for a `str` pair or for a `List` header.
#[test]
fn a_named_bytes_result_still_materializes() {
    let out = dump(
        "fn total(l: List[int]) -> int {\n\
         \x20   var n = 0\n\
         \x20   for x in l { n = n + x }\n\
         \x20   n\n\
         }\n\
         fn main() -> !int {\n\
         \x20   let bs = \"wolf\".bytes()\n\
         \x20   total(bs) - 440\n\
         }\n",
    );
    assert!(
        out.contains("call @__wolf_rt_str_bytes"),
        "the escaping position materializes: {out}"
    );
}

/// #72 (s74): `block_order` is the walk order a single-pass backend
/// needs — every reachable definition before every use of it. The
/// `select`-in-a-loop shape is the one that broke the assumption that
/// `Function::layout` already was such an order, so it is the shape the
/// invariant is asserted on.
#[test]
fn block_order_puts_every_definition_before_its_uses() {
    use std::collections::HashSet;
    let build = lower(
        "fn main() -> !int {\n\
         \x20   let done = channel[int](0)\n\
         \x20   scope s {\n\
         \x20       s.spawn(fn() { done.send(1) })\n\
         \x20       var live = 1\n\
         \x20       while live > 0 {\n\
         \x20           select {\n\
         \x20               _ from done => { live -= 1 },\n\
         \x20           }\n\
         \x20       }\n\
         \x20   }\n\
         \x20   0\n\
         }\n",
    );
    assert!(build.not_yet.is_empty(), "lowers: {:?}", build.not_yet);
    let mut checked = 0usize;
    for func in build.module.funcs.values() {
        let mut defined: HashSet<wolf_wir::Value> = HashSet::new();
        for b in wolf_wir::block_order(func) {
            for p in func.block_params(b) {
                defined.insert(p);
            }
            for &inst in &func.blocks[b].insts {
                for a in func.vpool.get(func.insts[inst].args) {
                    assert!(
                        defined.contains(&a),
                        "@{}: {b:?} uses a value the walk has not defined yet",
                        func.name
                    );
                    checked += 1;
                }
                for r in func.vpool.get(func.insts[inst].results) {
                    defined.insert(r);
                }
            }
        }
    }
    assert!(checked > 0, "the shape has operands to check");
}

// ------------------------------------------ the reducible-CFG suite ----

/// xorshift64* — deterministic seeds, no flaky randomness in CI.
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
}

/// Generate a random wolf function body of nested if/while over three
/// always-initialized int vars — every program typechecks, mem-checks,
/// and terminates statically-analyzably (conditions read vars; bodies
/// mutate them). Loop conditions use a fuel var that only decreases,
/// so `--checked` executions of these shapes would terminate too.
fn gen_program(seed: u64) -> String {
    let mut rng = Rng::new(seed);
    let mut out =
        String::from("fn main() -> !int {\n    var a = 1\n    var b = 2\n    var fuel = 20\n");
    let depth = 0;
    gen_stmts(&mut rng, &mut out, depth, 2 + (seed % 3) as u32);
    out.push_str("    a + b\n}\n");
    out
}

fn var_name(rng: &mut Rng) -> &'static str {
    match rng.below(2) {
        0 => "a",
        _ => "b",
    }
}

fn gen_stmts(rng: &mut Rng, out: &mut String, depth: u32, n: u32) {
    for _ in 0..n {
        let pad = "    ".repeat(depth as usize + 1);
        match rng.below(if depth >= 3 { 2 } else { 4 }) {
            // Assignment: v = v op small-const (kept within checked
            // range by construction: operands stay tiny).
            0 | 1 => {
                let v = var_name(rng);
                let w = var_name(rng);
                let op = match rng.below(3) {
                    0 => "+",
                    1 => "-",
                    _ => "*",
                };
                let c = rng.below(3);
                out.push_str(&format!("{pad}{v} = ({v} {op} {w}) % 97 + {c}\n"));
            }
            // Nested if/else.
            2 => {
                let v = var_name(rng);
                let c = rng.below(50);
                out.push_str(&format!("{pad}if {v} < {c} {{\n"));
                let n1 = 1 + (rng.below(2) as u32);
                gen_stmts(rng, out, depth + 1, n1);
                out.push_str(&format!("{pad}}} else {{\n"));
                let n2 = 1 + (rng.below(2) as u32);
                gen_stmts(rng, out, depth + 1, n2);
                out.push_str(&format!("{pad}}}\n"));
            }
            // While over the fuel counter (strictly decreasing).
            _ => {
                out.push_str(&format!("{pad}while fuel > 0 {{\n"));
                out.push_str(&format!("{pad}    fuel = fuel - 1\n"));
                let n1 = 1 + (rng.below(2) as u32);
                gen_stmts(rng, out, depth + 1, n1);
                out.push_str(&format!("{pad}}}\n"));
            }
        }
    }
}

#[test]
fn reducible_cfg_property_200_seeds() {
    for seed in 0..200 {
        let src = gen_program(seed);
        let build = lower(&src);
        assert!(
            build.not_yet.is_empty(),
            "seed {seed} refused: {:?}\n{src}",
            build.not_yet
        );
        if let Err(e) = verify_module(&build.module) {
            panic!("seed {seed} fails verify: {e}\n{src}");
        }
        let printed = print_module(&build.module);
        let reparsed =
            wolf_wir::parse_module(&printed).unwrap_or_else(|e| panic!("seed {seed} reparse: {e}"));
        verify_module(&reparsed).unwrap_or_else(|e| panic!("seed {seed} reverify: {e}"));
        assert_eq!(print_module(&reparsed), printed, "seed {seed} fixpoint");
        assert_no_trivial_params(&build.module);
        // Determinism: an independent second build prints identically.
        let again = lower(&src);
        assert_eq!(
            print_module(&again.module),
            printed,
            "seed {seed} determinism"
        );
    }
}

/// Every value printed in a dump is the result of construction-time
/// SSA: sanity-check def sites exist for all values (guards against
/// orphan leaks from trivial-param removal).
#[test]
fn no_dangling_param_defs_after_removal() {
    let build = lower(
        "fn main() -> !int {\n\
         \x20   let k = 3\n\
         \x20   var i = 0\n\
         \x20   while i < k {\n\
         \x20       i = i + k\n\
         \x20   }\n\
         \x20   i\n\
         }\n",
    );
    assert!(build.not_yet.is_empty());
    for func in build.module.funcs.values() {
        for &b in &func.layout {
            for (i, &p) in func.vpool.get(func.blocks[b].params).iter().enumerate() {
                match func.values[p].def {
                    ValueDef::Param(db, di) => {
                        assert_eq!(db, b, "param def block");
                        assert_eq!(di as usize, i, "param def index");
                    }
                    other => panic!("block param with non-param def {other:?}"),
                }
            }
        }
    }
}

// ---- wolf-lang#93: the two lowering accidents, pinned -----------------
//
// s80 audited #83 and found its original shape — an opaque callee
// writing a caller's `region.foreign` storage while holding none of its
// tokens — unreachable from SOURCE for two reasons that are ACCIDENTS
// of the lowering rather than guarantees anything makes. It recorded
// both in `docs/backlog.md` and said, correctly, that either could
// evaporate for unrelated reasons and take the guarantee with it.
//
// s80's conservatism (and s83's #92 guard) mean the hazard would not
// miscompile today even if both accidents went. These tests are not
// here to catch a miscompile; they are here so that a change removing
// an accident CANNOT do it silently. If one fails, the right response
// is to read #93 and decide deliberately — not to update the test.

/// Resolve + typecheck + memory-check, returning the error codes. The
/// twin of [`lower`] for programs that are supposed to be REJECTED.
fn mem_error_codes(src: &str) -> Vec<String> {
    let mut ml = MemoryLoader::new("wirlow");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    let mem = wolf_mem::check_package(&res.package, &tc);
    res.diagnostics
        .iter()
        .chain(tc.diagnostics.iter())
        .chain(mem.diagnostics.iter())
        .filter(|d| d.severity == wolf_diag::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

/// Accident 1 (#93): a `mut List` argument SPILLS its header pointer to
/// a stack slot before the call and RELOADS it afterwards, so the
/// caller's post-call element address is a fresh SSA value and
/// `memopt`'s `(token, address)` key misses. The address, not the
/// token, is what saves the shape — and the pass comments used to
/// assert the opposite of what the code relied on.
///
/// Pinned as: some address stored before the call is loaded back after
/// it. Make the `mut` spill smarter and this stops being true.
#[test]
fn a_mut_list_argument_reloads_its_header_after_the_call() {
    let build = lower(
        "fn writer(mut out: List[int]) { out[0] = 9 }\n\
         fn main() -> !int {\n\
         \x20   var b = List[int]()\n\
         \x20   (mut b).push(5)\n\
         \x20   let before = b[0]\n\
         \x20   writer(mut b)\n\
         \x20   let after = b[0]\n\
         \x20   before + after\n\
         }\n",
    );
    assert!(
        build.not_yet.is_empty(),
        "shape lowers: {:?}",
        build.not_yet
    );
    let main = build
        .module
        .funcs
        .values()
        .find(|f| f.name.ends_with("main"))
        .expect("a main");
    let insts: Vec<_> = main
        .layout
        .iter()
        .flat_map(|&b| main.blocks[b].insts.iter().copied())
        .collect();
    let call_at = insts
        .iter()
        .position(|&i| {
            matches!(main.insts[i].aux, Aux::Callee(ef)
                if main.ext_funcs[ef].name.ends_with("writer"))
        })
        .expect("the call to @writer survives lowering");
    // store args = (value, addr, token); load args = (addr, token).
    let stored_before: Vec<_> = insts[..call_at]
        .iter()
        .filter(|&&i| main.insts[i].op == wolf_wir::Opcode::Store)
        .map(|&i| main.vpool.get(main.insts[i].args)[1])
        .collect();
    let loaded_after: Vec<_> = insts[call_at + 1..]
        .iter()
        .filter(|&&i| main.insts[i].op == wolf_wir::Opcode::Load)
        .map(|&i| main.vpool.get(main.insts[i].args)[0])
        .collect();
    assert!(
        loaded_after.iter().any(|a| stored_before.contains(a)),
        "wolf-lang#93 accident 1 is gone: nothing spilled before the \
         `mut` call is reloaded after it, so the post-call element \
         address is no longer forced fresh and #83's original shape may \
         be reachable from source. Read the issue before touching this."
    );
}

/// Accident 2 (#93): two live `List` values cannot share one buffer at
/// source level, because `let b = a` MOVES. The IR-level sharing the
/// s78 note describes is real; what keeps it out of a single frame is
/// the move checker, not anything about tokens.
///
/// Pinned as: reading the moved-from binding is a hard error. Land a
/// sharing container (or make `let b = a` an alias) and this fails.
#[test]
fn two_live_lists_cannot_share_a_buffer() {
    let codes = mem_error_codes(
        "fn main() -> !int {\n\
         \x20   var a = List[int]()\n\
         \x20   (mut a).push(5)\n\
         \x20   let b = a\n\
         \x20   let x = a[0]\n\
         \x20   let y = b[0]\n\
         \x20   x + y\n\
         }\n",
    );
    assert!(
        !codes.is_empty(),
        "wolf-lang#93 accident 2 is gone: `let b = a` no longer keeps two \
         readable paths to one `List` buffer out of a single frame, so \
         #83's original shape may be reachable from source. Read the \
         issue before touching this."
    );
}
