//! s268: the representation gate is deterministic, unlike dispatch timing.
use wolf_backend::Backend;
use wolf_codegen_clif::ClifBackend;

fn chain(ty: &str, cases: &[i64], trapping_test: bool) -> String {
    let mut text = format!("fn @dispatch({ty}) -> i64 {{\nb0(%0: {ty}):\n");
    let mut v = 1;
    for (i, n) in cases.iter().enumerate() {
        if i != 0 {
            text.push_str(&format!("b{}:\n", i * 2));
        }
        if trapping_test && i == 1 {
            text.push_str(&format!("  %{v} = iadd.chk %0, %0\n"));
            v += 1;
        }
        text.push_str(&format!("  %{v} = iconst.{ty} {n}\n  %{} = icmp.eq %0, %{v}\n  br %{}, b{}, b{}\nb{}:\n  %{} = iconst.i64 {i}\n  ret %{}\n", v+1, v+1, i*2+1, i*2+2, i*2+1, v+2, v+2));
        v += 3;
    }
    text.push_str(&format!(
        "b{}:\n  %{v} = iconst.i64 -1\n  ret %{v}\n}}\n",
        cases.len() * 2
    ));
    text
}

fn clif(text: &str) -> String {
    let m = wolf_wir::parse_module(text).expect("WIR parses");
    wolf_wir::verify_module(&m).expect("WIR verifies");
    let mut backend = ClifBackend::new().expect("native backend available");
    wolf_codegen_clif::compile_module(&mut backend, &m, None, &mut wolf_backend::NullDebugSink)
        .expect("compile");
    let text = backend
        .clif_texts()
        .iter()
        .map(|(_, s)| s.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!Box::new(backend).finish().expect("object").bytes.is_empty());
    text
}

#[test]
fn dense_integer_chain_emits_one_bounded_table() {
    let text = clif(&chain("i64", &(0..60).collect::<Vec<_>>(), false));
    assert_eq!(
        text.matches("br_table").count(),
        1,
        "dense match must not remain a chain: {text}"
    );
    assert!(
        text.contains("icmp uge"),
        "bounds check before narrowing: {text}"
    );
}

#[test]
fn dense_negative_holes_and_integer_boundaries_emit_tables() {
    for (ty, cases) in [
        ("i8", (-8..0).collect::<Vec<_>>()),
        ("i16", (-32768..-32760).collect()),
        ("i32", (2147483640..2147483648).collect()),
        ("i64", (i64::MIN..i64::MIN + 8).collect()),
        ("i64", (i64::MAX - 7..=i64::MAX).collect()),
        ("i64", vec![-12, -10, -8, -6, -4, -2, 0, 2]),
        ("i64", vec![0, 0, 1, 2, 3, 4, 5, 6, 7]),
        ("i64", vec![0, 4, 8, 12, 16, 20, 24, 31]),
    ] {
        assert_eq!(
            clif(&chain(ty, &cases, false)).matches("br_table").count(),
            1,
            "{ty} {cases:?}"
        );
    }
}

#[test]
fn sparse_short_and_trapping_chains_stay_branches() {
    for (cases, trapping) in [
        ((0..7).collect::<Vec<_>>(), false),
        (vec![0, 5, 10, 15, 20, 25, 30, 35], false),
        ((0..8).collect(), true),
        (vec![i64::MIN, 0, 1, 2, 3, 4, 5, i64::MAX], false),
        (vec![0, 0, 1, 1, 2, 2, 3, 3], false),
        (vec![0, 4, 8, 12, 16, 20, 24, 32], false),
    ] {
        assert!(
            !clif(&chain("i64", &cases, trapping)).contains("br_table"),
            "{cases:?}"
        );
    }
}

// Black-box machine execution also covers negative WIR literals (the source
// pattern grammar currently accepts positive literal arms only).
#[cfg(unix)]
#[test]
fn dense_tables_execute_every_arm_and_default_at_integer_boundaries() {
    use std::process::Command;
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("s268-dense-wir");
    std::fs::create_dir_all(&dir).unwrap();
    for (ty, cases, inputs) in [
        (
            "i8",
            (-8..0).collect::<Vec<i64>>(),
            (-128..128).collect::<Vec<i64>>(),
        ),
        (
            "i16",
            (-32768..-32760).collect(),
            vec![-32768, -32767, -32760, -1, 0, 32767],
        ),
        (
            "i32",
            (2147483640..2147483648).collect(),
            vec![i32::MIN as i64, 0, 2147483639, 2147483640, 2147483647],
        ),
        (
            "i64",
            (i64::MIN..i64::MIN + 8).collect(),
            vec![i64::MIN, i64::MIN + 8, i64::MAX, 0, 4294967296],
        ),
        (
            "i64",
            (i64::MAX - 7..=i64::MAX).collect(),
            vec![i64::MIN, i64::MAX - 8, i64::MAX, 0, 4294967296],
        ),
        (
            "i64",
            vec![-12, -10, -8, -6, -4, -2, 0, 2],
            (-15..5).collect(),
        ),
        ("i64", vec![0, 0, 1, 2, 3, 4, 5, 6, 7], (-1..10).collect()),
    ] {
        let mut text = chain(ty, &cases, false);
        let default = if ty == "i64" {
            text = text.replace(&format!("ret %{}", cases.len() * 3 + 1), "ret %4");
            cases[1]
        } else {
            -1
        };
        text.push_str("fn @main() -> i64 {\nb0:\n");
        let inputs: Vec<_> = inputs.into_iter().chain(cases.iter().copied()).collect();
        for (block, x) in inputs.iter().copied().enumerate() {
            let value = block * 4;
            let want = cases
                .iter()
                .position(|&n| n == x)
                .map_or(default, |i| i as i64);
            text.push_str(&format!("  %{value} = iconst.{ty} {x}\n  %{} = call @dispatch(%{value})\n  %{} = iconst.i64 {want}\n  %{} = icmp.eq %{}, %{}\n  br %{}, b{}, b1000\nb{}:\n", value+1, value+2, value+3, value+1, value+2, value+3, block+1, block+1));
        }
        let value = inputs.len() * 4;
        text.push_str(&format!("  %{value} = iconst.i64 0\n  ret %{value}\nb1000:\n  %{} = iconst.i64 1\n  ret %{}\n}}\n", value+1, value+1));
        let mut m = wolf_wir::parse_module(&text).unwrap();
        wolf_wir::verify_module(&m).unwrap();
        let shim = wolf_codegen_clif::add_entry_shim(&mut m).unwrap();
        let mut backend = ClifBackend::new().unwrap();
        wolf_codegen_clif::compile_module(
            &mut backend,
            &m,
            Some(shim),
            &mut wolf_backend::NullDebugSink,
        )
        .unwrap();
        let product = Box::new(backend).finish().unwrap();
        std::fs::write(dir.join("dispatch.o"), product.bytes).unwrap();
        std::fs::write(dir.join("main.c"), "void __wolf_rt_main_start(int argc, char **argv, char **envp) { (void)argc; (void)argv; (void)envp; }\n").unwrap();
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".into());
        let linked = Command::new(cc)
            .arg(dir.join("main.c"))
            .arg(dir.join("dispatch.o"))
            .arg("-o")
            .arg(dir.join("run"))
            .output()
            .unwrap();
        assert!(
            linked.status.success(),
            "link: {}",
            String::from_utf8_lossy(&linked.stderr)
        );
        assert!(
            Command::new(dir.join("run")).status().unwrap().success(),
            "{ty} {cases:?}"
        );
    }
}

#[test]
fn a_shared_literal_keeps_the_first_arm_in_the_table() {
    // The original yew witness's default reuses the second test's 1.
    // The initial conservative recognizer started its table at arm 1,
    // leaving arm 0 faster; this gate requires the complete 60-way table.
    let wir = chain("i64", &(0..60).collect::<Vec<_>>(), false).replace("ret %181", "ret %4");
    let text = clif(&wir);
    let table = text
        .lines()
        .find(|l| l.contains("br_table"))
        .expect("table");
    let targets = table.split('[').nth(1).unwrap().trim_end_matches(']');
    assert_eq!(targets.split(',').count(), 60, "{table}");
}

#[test]
fn argument_edges_stay_out_of_the_recognizer() {
    let text = chain("i64", &(0..8).collect::<Vec<_>>(), false)
        .replace("br %2, b1, b2", "br %2, b1(%0), b2")
        .replace("b1:\n", "b1(%1000: i64):\n");
    assert!(!clif(&text).contains("br_table"));
}

#[test]
fn escaping_integer_literals_are_rematerialized() {
    let text = chain("i64", &(0..8).collect::<Vec<_>>(), false).replace("ret %6", "ret %4");
    assert_eq!(clif(&text).matches("br_table").count(), 1);
}

#[test]
fn escaping_comparison_definitions_are_not_bypassed() {
    let text = chain("i64", &(0..8).collect::<Vec<_>>(), false);
    // Arm 1 consumes its test condition. It cannot be rematerialized as
    // an integer literal, so b2 must not be bypassed.
    let text = text.replace("ret %6", "br %5, b3, b4");
    assert!(!clif(&text).contains("br_table"));
}
