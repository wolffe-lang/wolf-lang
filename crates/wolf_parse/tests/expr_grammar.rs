//! Positive coverage of the expression & statement grammar (spec §3)
//! through the typed accessor layer — precedence shapes, postfix
//! chains, `else` defaulting, `BracketApply`, regions, concurrency and
//! unsafe surface, statements, and the annex corner cases.

mod util;

use wolf_ast::{
    Arg, AsmExpr, AssignStmt, Block, CallExpr, ClosureExpr, DeferStmt, ElseExpr, FnDecl, GreenNode,
    IfExpr, MatchExpr, ParamMode, RegionBlock, SelectExpr, StringExpr, SyntaxKind, WhenExpr,
    is_expr_kind,
};

/// Parse expecting zero diagnostics.
fn clean(src: &str) -> GreenNode {
    let p = util::parse(src);
    assert!(
        p.diagnostics.is_empty(),
        "unexpected diagnostics for {src:?}: {:?}",
        p.diagnostics
    );
    p.root
}

/// Wrap an expression in a canonical fn body and return the root.
fn clean_body(expr: &str) -> GreenNode {
    clean(&format!("fn main() -> !int {{\n    {expr}\n}}\n"))
}

fn find<'a>(node: &'a GreenNode, kind: SyntaxKind, out: &mut Vec<&'a GreenNode>) {
    if node.kind == kind {
        out.push(node);
    }
    for n in node.nodes() {
        find(n, kind, out);
    }
}

fn first(root: &GreenNode, kind: SyntaxKind) -> &GreenNode {
    let mut v = Vec::new();
    find(root, kind, &mut v);
    v.first().unwrap_or_else(|| panic!("no {kind:?} in tree"))
}

fn count(root: &GreenNode, kind: SyntaxKind) -> usize {
    let mut v = Vec::new();
    find(root, kind, &mut v);
    v.len()
}

fn text(src: &str, span: wolf_span::Span) -> &str {
    &src[span.lo as usize..span.hi as usize]
}

// ----------------------------------------------------------- precedence --

#[test]
fn multiplication_binds_tighter_than_addition() {
    let src = "fn f() { let x = 1 + 2 * 3\n}\n";
    let root = clean(src);
    let outer = wolf_ast::BinExpr::cast(first(&root, SyntaxKind::BinExpr)).expect("bin");
    assert_eq!(outer.op().expect("op").kind, SyntaxKind::Plus);
    let rhs = wolf_ast::BinExpr::cast(outer.rhs().expect("rhs")).expect("bin rhs");
    assert_eq!(rhs.op().expect("op").kind, SyntaxKind::Star);
}

#[test]
fn logic_tiers_nest_and_under_or() {
    let src = "fn f() { let x = a || b && c\n}\n";
    let root = clean(src);
    let outer = wolf_ast::BinExpr::cast(first(&root, SyntaxKind::BinExpr)).expect("bin");
    assert_eq!(outer.op().expect("op").kind, SyntaxKind::PipePipe);
}

#[test]
fn cast_binds_tighter_than_arithmetic() {
    // tier 4 (`as`) vs tier 6 (`+`): (p[0] as int) + 1.
    let src = "fn f() { let x = y as int + 1\n}\n";
    let root = clean(src);
    let outer = wolf_ast::BinExpr::cast(first(&root, SyntaxKind::BinExpr)).expect("bin");
    assert_eq!(outer.op().expect("op").kind, SyntaxKind::Plus);
    assert_eq!(outer.lhs().expect("lhs").kind, SyntaxKind::CastExpr);
}

#[test]
fn prefix_binds_tighter_than_binary_looser_than_postfix() {
    // `!w.is_empty() && b` — the `!` operand is the whole method call.
    let src = "fn f() { let x = !w.is_empty() && b\n}\n";
    let root = clean(src);
    let outer = wolf_ast::BinExpr::cast(first(&root, SyntaxKind::BinExpr)).expect("bin");
    assert_eq!(outer.op().expect("op").kind, SyntaxKind::AmpAmp);
    let not = wolf_ast::PrefixExpr::cast(outer.lhs().expect("lhs")).expect("prefix");
    assert_eq!(not.operand().expect("operand").kind, SyntaxKind::CallExpr);
}

#[test]
fn spaceship_is_an_ordinary_comparison() {
    let src = "fn f() { let x = b.1 <=> a.1\n}\n";
    let root = clean(src);
    let cmp = wolf_ast::BinExpr::cast(first(&root, SyntaxKind::BinExpr)).expect("bin");
    assert_eq!(cmp.op().expect("op").kind, SyntaxKind::Spaceship);
}

#[test]
fn comparison_chaining_is_e0003() {
    let codes = util::codes("fn f() { let x = a < b < c\n}\n");
    assert_eq!(codes, ["E0003"], "exactly one chaining diagnostic");
}

#[test]
fn amp_mut_is_one_prefix_operator() {
    let src = "fn f() { let p = &mut x\n}\n";
    let root = clean(src);
    let pre = first(&root, SyntaxKind::PrefixExpr);
    assert!(pre.child_token(SyntaxKind::Amp).is_some());
    assert!(pre.child_token(SyntaxKind::MutKw).is_some());
}

// -------------------------------------------------------------- postfix --

#[test]
fn question_chains_at_max_binding_power() {
    // `fs.read_text(p)?.parse()?` — D30's poster child.
    let src = "fn f() -> !int { fs.read_text(p)?.parse()?\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::TryExpr), 2);
    // The outer node of the initializer chain is the trailing `?`.
    let body = FnDecl::cast(first(&root, SyntaxKind::FnDecl))
        .expect("fn")
        .body()
        .expect("body");
    let trailing = body.trailing_expr().expect("trailing");
    assert_eq!(trailing.kind, SyntaxKind::TryExpr);
}

#[test]
fn members_are_keyword_transparent() {
    // `take`, `move` are reserved; member position doesn't care.
    let src = "fn f() { let x = xs.take(1).move\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::MemberExpr), 2);
}

#[test]
fn int_literal_members_and_tuple_access() {
    let src = "fn f() { let d = 5.s\n    let y = pair.0\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::MemberExpr), 2);
}

#[test]
fn bracket_apply_covers_index_and_generic_application() {
    // One node kind for both ([gram.amb.brackets], D29).
    let src = "fn f() { let a = first[int](xs)\n    let b = m[\"k\"]\n}\n";
    let root = clean(src);
    let mut brackets = Vec::new();
    find(&root, SyntaxKind::BracketApply, &mut brackets);
    assert_eq!(brackets.len(), 2);
    // The generic application is also the callee of a call.
    let call = CallExpr::cast(first(&root, SyntaxKind::CallExpr)).expect("call");
    assert_eq!(
        call.callee().expect("callee").kind,
        SyntaxKind::BracketApply
    );
}

#[test]
fn bracket_args_admit_type_only_forms() {
    let src = "fn f() { let a = List[handle Node]()\n    let c = channel[region](2)\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::PrefixType), 1, "handle Node");
    assert_eq!(count(&root, SyntaxKind::RegionType), 1, "bare region");
}

#[test]
fn call_and_index_args_take_x1_mode_markers() {
    let src = "fn f() { bump(mut p.x, take b)\n    let x = pool[mut prev]\n}\n";
    let root = clean(src);
    let mut args = Vec::new();
    find(&root, SyntaxKind::Arg, &mut args);
    let modes: Vec<Option<ParamMode>> = args
        .iter()
        .map(|a| Arg::cast(a).expect("arg").mode())
        .collect();
    assert_eq!(
        modes,
        [
            Some(ParamMode::Mut),
            Some(ParamMode::Take),
            Some(ParamMode::Mut)
        ]
    );
}

#[test]
fn line_initial_bracket_never_attaches() {
    // [gram.lex.newline]: the inserted terminator after `a` breaks the
    // climb; the `[` starts a broken statement, `a` stays clean.
    let parse = util::parse("fn f() { let s = a\n    [1]\n}\n");
    assert_eq!(
        parse.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        ["E0201"]
    );
    let lets: Vec<_> = {
        let mut v = Vec::new();
        find(&parse.root, SyntaxKind::LetDecl, &mut v);
        v
    };
    assert_eq!(count(lets[0], SyntaxKind::ErrorNode), 0, "let stays clean");
    assert_eq!(count(&parse.root, SyntaxKind::BracketApply), 0);
}

// ------------------------------------------------------------- ranges --

#[test]
fn range_forms_and_end_relative_endpoints() {
    let src = "fn f() {\n    for i in 1..10 { }\n    for i in 1..=10 { }\n    let a = xs[..8]\n    let b = xs[^13..]\n    let c = xs[..^1]\n    let d = xs[..min(n, 12)]\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::RangeExpr), 6);
    assert_eq!(count(&root, SyntaxKind::FromEndExpr), 2);
}

#[test]
fn lone_from_end_index() {
    let src = "fn f() { let x = s[^1]\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::FromEndExpr), 1);
    assert_eq!(count(&root, SyntaxKind::RangeExpr), 0);
}

// ------------------------------------------------------ else defaulting --

#[test]
fn else_defaulting_forms() {
    let src = "fn f() -> !int {\n    let a = parse(s) else 0\n    let b = parse(s) else { 1 }\n    let c = parse(s) else |err| { print(\"{err}\"); 1 }\n    let d = parse(s) else |_| (0, 0)\n    let e = parse(s) else |_| return 1\n    0\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::ElseExpr), 5);
    // The jump-expression handler body parses (the recognizer couldn't).
    let mut elses = Vec::new();
    find(&root, SyntaxKind::ElseExpr, &mut elses);
    let jump = ElseExpr::cast(elses[4]).expect("else");
    assert_eq!(
        jump.fallback().expect("fallback").kind,
        SyntaxKind::ReturnExpr
    );
}

#[test]
fn else_after_if_belongs_to_if_when_viable() {
    // `} else if` / `} else {` continue the if ([gram.amb.else]).
    let src = "fn f() { let r = if x == 1 { 10 } else if x == 2 { 20 } else { 30 }\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::ElseExpr), 0);
    assert_eq!(count(&root, SyntaxKind::IfExpr), 2);
    let outer = IfExpr::cast(first(&root, SyntaxKind::IfExpr)).expect("if");
    assert_eq!(outer.else_branch().expect("else").kind, SyntaxKind::IfExpr);
}

#[test]
fn else_after_complete_expression_is_defaulting_even_after_if() {
    // Report 05's corner: `if c { a } else b else c` — the first `else`
    // cannot continue the if (no `if`/`{` follows), so both are the
    // defaulting operator, chaining right-associatively.
    let src = "fn f() { let r = if c { 1 } else b else 0\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::IfExpr), 1);
    assert_eq!(count(&root, SyntaxKind::ElseExpr), 2);
    let outer = ElseExpr::cast(first(&root, SyntaxKind::ElseExpr)).expect("else");
    assert_eq!(outer.scrutinized().expect("lhs").kind, SyntaxKind::IfExpr);
    assert_eq!(outer.fallback().expect("rhs").kind, SyntaxKind::ElseExpr);
}

#[test]
fn else_on_new_line_is_e0005() {
    let codes = util::codes("fn f() { let x = if c { 1 }\n    else { 2 }\n}\n");
    assert_eq!(codes, ["E0005"]);
}

// ------------------------------------------------------------ closures --

#[test]
fn closure_expression_body_extends_maximally() {
    // [gram.amb.closure]: the body is one else_expr, terminated only by
    // a token the expression grammar cannot consume.
    let src = "fn f() { let t = xs.sorted_by(fn(a, b) b.1 <=> a.1).take(1)\n}\n";
    let root = clean(src);
    let closure = ClosureExpr::cast(first(&root, SyntaxKind::ClosureExpr)).expect("closure");
    assert_eq!(closure.body().expect("body").kind, SyntaxKind::BinExpr);
}

#[test]
fn closure_as_non_final_argument_stops_at_comma() {
    let src = "fn f() { let m = zip(xs, fn(p) p.0 + p.1, 100)\n}\n";
    let root = clean(src);
    let call = CallExpr::cast(first(&root, SyntaxKind::CallExpr)).expect("call");
    assert_eq!(call.args().expect("args").args().count(), 3);
}

#[test]
fn closure_params_take_modes_and_ascriptions() {
    let src = "fn f() { let g = fn(mut a: int, b) { a + b }\n}\n";
    let root = clean(src);
    let closure = ClosureExpr::cast(first(&root, SyntaxKind::ClosureExpr)).expect("closure");
    let params: Vec<_> = closure.params().expect("params").params().collect();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].mode(), Some(ParamMode::Mut));
    assert!(params[0].ty().is_some());
    assert!(params[1].ty().is_none());
}

// -------------------------------------------------------- struct literals --

#[test]
fn struct_literals_and_shorthand() {
    let src = "fn f() { let p = Point { x: 0, y }\n}\n";
    let root = clean(src);
    let lit = wolf_ast::StructLit::cast(first(&root, SyntaxKind::StructLit)).expect("lit");
    let fields: Vec<_> = lit.fields().collect();
    assert_eq!(fields.len(), 2);
    assert!(fields[0].value().is_some());
    assert!(fields[1].value().is_none(), "shorthand binds from the name");
}

#[test]
fn struct_literal_in_condition_is_e0006() {
    let codes = util::codes("fn f() { if p == Point { x: 0 } { return 0 }\n}\n");
    assert_eq!(codes, ["E0006"]);
}

#[test]
fn parenthesized_struct_literal_in_condition_parses() {
    clean("fn f() -> !int { if p == (Point { x: 0 }) { 0 } else { 1 }\n}\n");
}

#[test]
fn scrutinee_brace_opens_the_block() {
    // `match err { … }` — `err {` must not become a struct literal.
    let src = "fn f() { match err {\n        A => 1,\n        B => 2,\n    }\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::StructLit), 0);
    let m = MatchExpr::cast(first(&root, SyntaxKind::MatchExpr)).expect("match");
    assert_eq!(m.arms().count(), 2);
}

// ---------------------------------------------------------- statements --

#[test]
fn assignment_and_compound_assignment_statements() {
    let src = "fn f() { x = 1\n    total += p[0] as int\n    h = h * 3 + 1\n}\n";
    let root = clean(src);
    let mut assigns = Vec::new();
    find(&root, SyntaxKind::AssignStmt, &mut assigns);
    assert_eq!(assigns.len(), 3);
    let compound = AssignStmt::cast(assigns[1]).expect("assign");
    assert_eq!(compound.op().expect("op").kind, SyntaxKind::PlusEq);
    assert_eq!(compound.value().expect("value").kind, SyntaxKind::CastExpr);
}

#[test]
fn assignment_in_expression_position_is_e0208() {
    let codes = util::codes("fn f() { let x = (y = 2)\n}\n");
    assert_eq!(codes, ["E0208"]);
}

#[test]
fn empty_statement_is_e0002() {
    let codes = util::codes("fn f() {\n    ;\n    let x = 1;\n}\n");
    assert_eq!(codes, ["E0002"]);
}

#[test]
fn semicolons_separate_single_line_blocks() {
    clean("fn f() -> !int { if x == 1 { print(\"ok\"); return 0 }\n    1\n}\n");
}

#[test]
fn leading_operator_continuation_is_e0001() {
    let parse = util::parse("fn f() { let a = 1\n        + 2\n    let b = 3\n}\n");
    assert_eq!(
        parse.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        ["E0001"]
    );
    // The later declaration still parses clean.
    let mut lets = Vec::new();
    find(&parse.root, SyntaxKind::LetDecl, &mut lets);
    assert_eq!(lets.len(), 2);
    assert_eq!(count(lets[1], SyntaxKind::ErrorNode), 0);
}

#[test]
fn trailing_operator_continuation_parses() {
    clean(
        "fn f() { let a = 1 +\n        2 +\n        3\n    let s = \"abc\".\n        upper()\n}\n",
    );
}

#[test]
fn defer_and_errdefer() {
    let src = "fn f() { defer close(x)\n    errdefer release(mut r)\n}\n";
    let root = clean(src);
    let mut defers = Vec::new();
    find(&root, SyntaxKind::DeferStmt, &mut defers);
    assert_eq!(defers.len(), 2);
    assert!(!DeferStmt::cast(defers[0]).expect("defer").is_errdefer());
    assert!(DeferStmt::cast(defers[1]).expect("defer").is_errdefer());
}

#[test]
fn assume_noalias_statement() {
    let src = "fn f() { assume noalias p, q\n}\n";
    let root = clean(src);
    let a = wolf_ast::AssumeStmt::cast(first(&root, SyntaxKind::AssumeStmt)).expect("assume");
    assert_eq!(a.exprs().count(), 2);
}

#[test]
fn nested_items_in_blocks() {
    let src = "fn f() {\n    const N = 3\n    comptime fn g(n: int) -> int { n }\n    struct P { x: int }\n    let p = P { x: g(N) }\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::ConstDecl), 1);
    assert_eq!(count(&root, SyntaxKind::FnDecl), 2);
    assert_eq!(count(&root, SyntaxKind::StructDecl), 1);
}

#[test]
fn block_trailing_expression_vs_statement() {
    let src = "fn f() -> !int { let x = 1\n    x\n}\n";
    let root = clean(src);
    let body = FnDecl::cast(first(&root, SyntaxKind::FnDecl))
        .expect("fn")
        .body()
        .expect("body");
    let trailing = body.trailing_expr().expect("trailing expression");
    assert_eq!(trailing.kind, SyntaxKind::PathExpr);
    assert_eq!(text(src, trailing.span), "x");
}

// ---------------------------------------------------- regions (X4) --------

#[test]
fn region_sugar_and_value_forms_are_distinct_kinds() {
    let src = "fn f() {\n    region tmp { build() }\n    region r: pool(Node) { build() }\n    let a = region()\n    let b = region(rc)\n    let n = in a { 21 }\n    let z = freeze a\n    let t = freeze region { 2 }\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::RegionBlock), 3);
    assert_eq!(count(&root, SyntaxKind::RegionValue), 2);
    assert_eq!(count(&root, SyntaxKind::InBlock), 1);
    assert_eq!(count(&root, SyntaxKind::FreezeExpr), 2);
    assert_eq!(count(&root, SyntaxKind::RegionStrategy), 2);
    let named = RegionBlock::cast(first(&root, SyntaxKind::RegionBlock)).expect("region");
    assert_eq!(text(src, named.name().expect("name").span), "tmp");
}

#[test]
fn move_copy_shared_prefix_operators() {
    let src = "fn f() {\n    ch.send(move r)\n    let c = copy b\n    let a = shared (Cfg { limit: 7 })\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::PrefixExpr), 3);
}

// ------------------------------------------------- concurrency surface --

#[test]
fn scope_select_spawn_when() {
    let src = "fn f() -> !int {\n    scope s {\n        s.spawn(fn() { work() })\n    }\n    let w = spawn proc worker()\n    select {\n        exit(reason) from m => { note(reason) }\n        v from ch => v,\n        timeout(1.s) => { return 1 }\n    }\n    when (a, b) { a += 1; b += 1 }\n    0\n}\n";
    let root = clean(src);
    let scope = wolf_ast::ScopeExpr::cast(first(&root, SyntaxKind::ScopeExpr)).expect("scope");
    assert_eq!(text(src, scope.name().expect("binder").span), "s");
    assert_eq!(count(&root, SyntaxKind::SpawnExpr), 1);
    let select = SelectExpr::cast(first(&root, SyntaxKind::SelectExpr)).expect("select");
    let arms: Vec<_> = select.arms().collect();
    assert_eq!(arms.len(), 3);
    assert!(!arms[0].is_timeout());
    assert!(arms[2].is_timeout());
    let when = WhenExpr::cast(first(&root, SyntaxKind::WhenExpr)).expect("when");
    assert_eq!(when.operands().count(), 2);
}

#[test]
fn when_requires_two_operands() {
    let codes = util::codes("fn f() { when (a) { a += 1 }\n}\n");
    assert_eq!(codes, ["E0201"]);
}

// ------------------------------------------------------- unsafe tier --

#[test]
fn unsafe_block_and_inline_c() {
    let src = "fn f() {\n    unsafe { p[0] = 7 }\n    unsafe c [total, n] { int i = 0; for (; i < n; i++) total += i; }\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::UnsafeBlock), 1);
    let ic = wolf_ast::InlineC::cast(first(&root, SyntaxKind::InlineC)).expect("inline c");
    assert!(ic.captures().is_some());
    assert!(ic.body().is_some(), "opaque brace-balanced body");
}

#[test]
fn asm_with_typed_operands() {
    let src = "fn f() {\n    asm {\n        \"add {t}, 35\",\n        t = inout(reg) total,\n        in(reg) n,\n    }\n}\n";
    let root = clean(src);
    let asm = AsmExpr::cast(first(&root, SyntaxKind::AsmExpr)).expect("asm");
    assert!(asm.template().is_some());
    assert_eq!(asm.operands().count(), 2);
}

#[test]
fn borrow_from_expression() {
    let src = "fn f() { let b = borrow x from r\n}\n";
    let root = clean(src);
    let b = wolf_ast::BorrowExpr::cast(first(&root, SyntaxKind::BorrowExpr)).expect("borrow");
    assert_eq!(b.borrowed().expect("borrowed").kind, SyntaxKind::PathExpr);
    assert_eq!(b.source().expect("source").kind, SyntaxKind::PathExpr);
}

// --------------------------------------------------- match + patterns --

#[test]
fn match_arms_guards_or_patterns_bindings() {
    let src = "fn f() -> int {\n    match err {\n        BadDigit(e) if e.at > 0 => 1,\n        TooShort | TooLong => 2,\n        n @ Other(_) => n.code(),\n        io.Error(_) => 3,\n        _ => { log(); 4 }\n    }\n}\n";
    let root = clean(src);
    let m = MatchExpr::cast(first(&root, SyntaxKind::MatchExpr)).expect("match");
    let arms: Vec<_> = m.arms().collect();
    assert_eq!(arms.len(), 5);
    assert!(arms[0].guard().is_some());
    assert_eq!(arms[1].pattern().expect("pat").kind, SyntaxKind::OrPat);
    assert_eq!(arms[2].pattern().expect("pat").kind, SyntaxKind::BindingPat);
    assert_eq!(arms[3].pattern().expect("pat").kind, SyntaxKind::PathPat);
    assert_eq!(
        arms[4].pattern().expect("pat").kind,
        SyntaxKind::WildcardPat
    );
}

#[test]
fn expr_bodied_arm_needs_comma_before_next_arm() {
    let parse = util::parse("fn f() { match x {\n        A => 1\n        B => 2,\n    }\n}\n");
    assert_eq!(
        parse.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        ["E0201"]
    );
}

#[test]
fn block_bodied_arm_needs_no_comma() {
    clean("fn f() { match x {\n        A => { one() }\n        B => { two() }\n    }\n}\n");
}

// ------------------------------------------------------- string mode --

#[test]
fn interpolations_parse_as_expressions() {
    let src = "fn f() { print(\"{m[\"k\"]:>8} and {\"#\".repeat(n / 10)}\")\n}\n";
    let root = clean(src);
    let s = StringExpr::cast(first(&root, SyntaxKind::StringExpr)).expect("string");
    let interps: Vec<_> = s.interps().collect();
    assert_eq!(interps.len(), 2);
    assert_eq!(
        interps[0].expr().expect("expr").kind,
        SyntaxKind::BracketApply
    );
    assert!(interps[0].format_spec().is_some());
    assert!(interps[1].format_spec().is_none());
}

#[test]
fn interpolated_width_inside_format_spec() {
    let src = "fn f() { print(\"{v:>{w}}\")\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::Interp), 2, "width re-nests");
    assert_eq!(count(&root, SyntaxKind::FormatSpec), 1);
}

#[test]
fn interp_nesting_past_eight_is_e0007() {
    // Nine nested strings: the outermost is depth 1, the innermost 9.
    let mut lit = String::from("\"x\"");
    for _ in 0..8 {
        lit = format!("\"{{{lit}}}\"");
    }
    let codes = util::codes(&format!("fn f() {{ let s = {lit}\n}}\n"));
    assert_eq!(codes, ["E0007"]);
    // Eight deep is legal.
    let mut lit = String::from("\"x\"");
    for _ in 0..7 {
        lit = format!("\"{{{lit}}}\"");
    }
    clean_body(&format!("let s = {lit}"));
}

// ----------------------------------------------------------- jumps --------

#[test]
fn jump_expressions_compose() {
    let src = "fn f() -> !int {\n    if bad { return 2 }\n    loop { break 7 }\n    while c { continue }\n    0\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::ReturnExpr), 1);
    assert_eq!(count(&root, SyntaxKind::BreakExpr), 1);
    assert_eq!(count(&root, SyntaxKind::ContinueExpr), 1);
    let brk = wolf_ast::BreakExpr::cast(first(&root, SyntaxKind::BreakExpr)).expect("break");
    assert_eq!(brk.value().expect("value").kind, SyntaxKind::LiteralExpr);
}

// -------------------------------------------------------------- misc --------

#[test]
fn tuples_grouping_and_one_tuple() {
    let src = "fn f() { let a = (1, 2)\n    let b = (x)\n    let c = (x,)\n}\n";
    let root = clean(src);
    assert_eq!(count(&root, SyntaxKind::TupleExpr), 2);
    assert_eq!(count(&root, SyntaxKind::ParenExpr), 1);
}

#[test]
fn blocks_are_expressions() {
    let src = "fn f() { let x = { let y = 1\n        y + 1 }\n}\n";
    let root = clean(src);
    // The initializer block is a real expression with statements.
    let mut blocks = Vec::new();
    find(&root, SyntaxKind::Block, &mut blocks);
    assert_eq!(blocks.len(), 2, "fn body + initializer block");
    let init = Block::cast(blocks[1]).expect("block");
    assert_eq!(
        init.trailing_expr().expect("value").kind,
        SyntaxKind::BinExpr
    );
}

#[test]
fn every_expression_kind_is_classified() {
    // is_expr_kind must cover what the parser emits in expression
    // position (spot check the corpus's richest file).
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/wordcount.lu"),
    )
    .expect("read wordcount.lu");
    let root = clean(&src);
    let mut stmts = Vec::new();
    find(&root, SyntaxKind::ExprStmt, &mut stmts);
    for s in stmts {
        let expr = wolf_ast::ExprStmt::cast(s).expect("stmt").expr();
        assert!(expr.is_some_and(|e| is_expr_kind(e.kind)));
    }
}
