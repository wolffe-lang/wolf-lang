//! One-statement bare guards retain their spelling, including past width 100.
fn formatted(source: &str) -> String {
    let outcome = wolf_fmt::format_text(source.as_bytes());
    assert!(!outcome.partial, "partial format: {source}");
    assert!(!outcome.fell_back, "formatter fallback: {source}");
    let once = String::from_utf8(outcome.text).unwrap();
    let twice = wolf_fmt::format_text(once.as_bytes());
    assert!(!twice.partial && !twice.fell_back);
    assert_eq!(twice.text, once.as_bytes(), "not idempotent: {source}");
    once
}

#[test]
fn one_statement_then_guard_stays_on_one_line_past_width() {
    for length in [1, 40, 80, 100, 180] {
        for statement in ["return answer", "break", "continue", "emit(answer)"] {
            let condition = "c".repeat(length);
            let line = format!("if {condition} then {statement}");
            let source = format!("fn f() {{\n    loop {{\n        {line}\n    }}\n}}\n");
            assert_eq!(formatted(&source), source, "guard changed shape: {line}");
        }
    }
}

#[test]
fn a_long_guard_call_does_not_introduce_braces_or_argument_breaks() {
    let line = "if ready(alpha, beta, gamma, delta, epsilon) then write_the_result(first_value, second_value, third_value)";
    let source = format!("fn f() {{\n    {line}\n}}\n");
    assert_eq!(formatted(&source), source);
}

#[test]
fn a_braced_guard_is_not_converted_to_then() {
    let source = "fn f() {\n    if ready {\n        return answer\n    }\n}\n";
    assert_eq!(formatted(source), source);
}

#[test]
fn comments_and_multiline_branches_remain_a_fixed_point() {
    for source in [
        "fn f() {\n    if ready then emit() // retain this comment\n}\n",
        "fn f() {\n    if ready /* condition */ then emit()\n}\n",
        "fn f() {\n    if ready then emit(\n        // retain this argument comment\n        answer\n    )\n}\n",
        "fn f() {\n    if ready then emit(fn() {\n        one()\n        two()\n    })\n}\n",
        "fn f() {\n    if ready then emit(\"\"\"\n        first\n        second\n        \"\"\")\n}\n",
    ] {
        formatted(source); // The formatter's self-check compares trees and comments.
    }
}
