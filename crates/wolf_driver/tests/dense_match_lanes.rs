//! s268: runtime correctness on all three machines, independent of timing.
mod lane_exit;
use std::process::Command;

#[test]
fn dense_dispatch_preserves_holes_defaults_widths_and_boundaries() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("s268-dense-dispatch");
    std::fs::create_dir_all(&dir).unwrap();
    let mut source = String::new();
    let cases = [
        ("dense", "int", (0..60).collect::<Vec<i64>>()),
        ("holes", "int", vec![0, 2, 4, 6, 8, 10, 12, 14]),
        ("high", "int", (i64::MAX - 7..=i64::MAX).collect()),
        ("narrow", "u8", (248..256).collect()),
    ];
    for (name, ty, arms) in &cases {
        source.push_str(&format!("fn {name}(x: {ty}) -> int {{ match x {{\n"));
        for (i, n) in arms.iter().enumerate() {
            source.push_str(&format!("{n} => {},\n", i + 10));
        }
        source.push_str("_ => -1,\n} }\n");
    }
    source.push_str("fn main() {\n");
    let mut want = String::new();
    for (name, ty, arms) in &cases {
        let mut inputs = arms.clone();
        if *ty == "u8" {
            inputs.extend([0, 1, 127, 128, 247]);
        } else {
            inputs.extend([-13, -11, -1, 0, 3, 59, 60, 4294967296, i64::MIN, i64::MAX]);
        }
        for x in inputs {
            source.push_str(&format!("{{ let r = {name}({x}); print(\"{{r}}\") }}\n"));
            let expected = arms
                .iter()
                .position(|&n| n == x)
                .map_or(-1, |i| i as i64 + 10);
            want.push_str(&format!("{expected}\n"));
        }
    }
    source.push_str("}\n");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, source).unwrap();
    assert!(
        Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .unwrap()
            .success()
    );
    for tier in ["--checked", "--native", "--release"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .arg("conform-run")
            .arg(&entry)
            .args([tier, "--json"])
            .output()
            .unwrap();
        if tier != "--checked" && lane_exit::environment_refusal(&out, tier) {
            assert!(
                std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
                "required tier {tier} unavailable"
            );
            eprintln!(
                "SKIP: s268 {tier}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            continue;
        }
        assert!(
            out.status.success(),
            "{tier}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let row: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(row["verdict"], "exit(0)", "{tier}: {row}");
        assert_eq!(row["stdout_inline"], want, "{tier}: {row}");
    }
}
