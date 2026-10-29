//! Bounded #857 coverage: aspen's argument shape and 50,000 width/layout cases.
#[test]
fn multiline_if_argument_reaches_its_fixed_point() {
    let source = include_bytes!("regressions/idem_aspen_if_argument.lu");
    let once = wolf_fmt::format_text(source);
    assert!(
        !once.partial && !once.fell_back,
        "the aspen fixture must actually format"
    );
    assert_eq!(wolf_fmt::format_text(&once.text).text, once.text);
    for depth in 0..5 {
        for cond_len in [4, 40, 75, 95, 111] {
            for len in 1..126 {
                for form in 0..4 {
                    let cond = "c".repeat(cond_len);
                    let a = "a".repeat(len);
                    let b = "b".repeat(len + 13);
                    let expr = match form {
                        0 => format!("if {cond} then \"{a}\" else \"{b}\""),
                        1 => format!("if {cond} {{\n\"{a}\"\n}} else {{\n\"{b}\"\n}}"),
                        2 => {
                            format!("if {cond} {{\n// first arm\n\"{a}\"\n}} else {{\n\"{b}\"\n}}")
                        }
                        _ => format!(
                            "if {cond} then \"{a}\" else if other then \"{b}\" else \"last\""
                        ),
                    };
                    for call in [
                        format!("out.s(mut t.o, {expr})"),
                        format!("out.s({expr}, tail)"),
                        format!("out.s(leading, {expr}, tail)"),
                        format!("(mut out).s({expr})"),
                    ] {
                        let src = format!(
                            "fn main() {{\n{}{}\n{}\n}}\n",
                            "if guard {\n".repeat(depth),
                            call,
                            "}\n".repeat(depth)
                        );
                        let once = wolf_fmt::format_text(src.as_bytes());
                        assert!(!once.partial && !once.fell_back, "input:\n{src}");
                        let twice = wolf_fmt::format_text(&once.text);
                        assert_eq!(
                            once.text,
                            twice.text,
                            "input:\n{src}\nfirst:\n{}\nsecond:\n{}",
                            String::from_utf8_lossy(&once.text),
                            String::from_utf8_lossy(&twice.text)
                        );
                    }
                }
            }
        }
    }
}
