//! #970/#958: failures retain their message and source coordinates.
use std::process::Command;

fn wolf() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wolf"));
    command
        .env_remove("WOLF_STD")
        .env_remove("WOLF_LD")
        .env_remove("WOLF_AS")
        .env_remove("WOLF_RT_LIB");
    command
}

#[test]
fn nominal_equality_keeps_impl_semantics_and_both_values() {
    let dir = std::env::temp_dir().join(format!("wolf-assert-nominal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("nominal_test.lu");
    // Inverting Eq, as corpus/traits/op_eq_inverting.lu: a structural
    // comparison would pass, so the diagnostic must not replace the impl.
    std::fs::write(&file, "struct P { x: int }\ntrait Eq { fn eq(self, other: Self) -> bool }\nimpl Eq for P {\n    fn eq(self, other: Self) -> bool { self.x != other.x }\n}\nfn test_failure() {\n    let a = P { x: 1 }\n    let b = P { x: 1 }\n    assert(a == b, \"the Eq impl rejected these values\")\n}\n").unwrap();
    for tier in [None, Some("--native"), Some("--release")] {
        if tier == Some("--release") && cfg!(windows) {
            eprintln!("SKIP: the release tier is not supported on Windows");
            continue;
        }
        let mut cmd = wolf();
        cmd.arg("test").arg(&file).arg("--json");
        if let Some(tier) = tier {
            cmd.arg(tier);
        }
        let out = cmd.output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{text}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let event = text
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .find(|e| e["event"] == "test")
            .unwrap();
        assert!(
            event["assertion"]["left"]
                .as_str()
                .is_some_and(|s| s.contains('1')),
            "{tier:?}: {event}"
        );
        assert!(
            event["assertion"]["right"]
                .as_str()
                .is_some_and(|s| s.contains('1')),
            "{tier:?}: {event}"
        );
        assert_eq!(
            event["assertion"]["message"], "the Eq impl rejected these values",
            "{tier:?}: {event}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn assertion_diagnostics_do_not_charge_the_programs_region_budget() {
    let dir = std::env::temp_dir().join(format!("wolf-assert-cap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("cap_test.lu");
    std::fs::write(&file, "fn test_failure() {\n    region idle(cap: 0) {\n        assert(1 == 2, \"the assertion, not a formatting allocation\")\n    }\n}\n").unwrap();
    for tier in [None, Some("--native"), Some("--release")] {
        if tier == Some("--release") && cfg!(windows) {
            eprintln!("SKIP: the release tier is not supported on Windows");
            continue;
        }
        let mut cmd = wolf();
        cmd.arg("test").arg(&file).arg("--json");
        if let Some(tier) = tier {
            cmd.arg(tier);
        }
        let out = cmd.output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{text}");
        let event = text
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .find(|e| e["event"] == "test")
            .unwrap();
        assert!(
            event["detail"]
                .as_str()
                .unwrap()
                .starts_with("trap(assert)"),
            "{tier:?}: {event}"
        );
        assert_eq!(event["assertion"]["left"], "1", "{tier:?}: {event}");
        assert_eq!(event["assertion"]["right"], "2", "{tier:?}: {event}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn compiled_tiers_refuse_conflicting_execution_flags() {
    for flags in [
        ["--native", "--release"],
        ["--native", "--schedules=2"],
        ["--release", "--no-native-fallback"],
    ] {
        let out = wolf().arg("test").args(flags).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn native_assertion_details_survive_unterminated_user_stderr() {
    let dir = std::env::temp_dir().join(format!("wolf-assert-raw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("raw_test.lu");
    std::fs::write(&file, "fn test_failure() {\n    eprint_raw(\"before failure\")\n    assert(false, \"a message with wolf-trap: overflow\")\n}\n").unwrap();
    for tier in ["--native", "--release"] {
        if tier == "--release" && cfg!(windows) {
            eprintln!("SKIP: the release tier is not supported on Windows");
            continue;
        }
        let out = wolf().arg("test").arg(tier).arg(&file).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(
            text.contains("message: a message with wolf-trap: overflow"),
            "{text}"
        );
        assert!(text.contains("raw_test.lu:3:5"), "{text}");
        assert!(text.contains("stderr: before failure"), "{text}");
        assert!(!text.contains("wolf-assert-0:"), "{text}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn string_boolean_and_float_comparands_are_json_data() {
    let dir = std::env::temp_dir().join(format!("wolf-assert-types-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("types_test.lu");
    for (condition, left, right) in [
        ("(\"é\" == \"雪\")", "é", "雪"),
        ("true == false", "true", "false"),
        ("1.25 == 2.5", "1.25", "2.5"),
    ] {
        std::fs::write(
            &file,
            format!("fn test_values() {{\n    assert({condition})\n}}\n"),
        )
        .unwrap();
        for tier in [None, Some("--native"), Some("--release")] {
            if tier == Some("--release") && cfg!(windows) {
                eprintln!("SKIP: the release tier is not supported on Windows");
                continue;
            }
            let mut cmd = wolf();
            cmd.arg("test").arg(&file).arg("--json");
            if let Some(tier) = tier {
                cmd.arg(tier);
            }
            let out = cmd.output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            assert_eq!(
                out.status.code(),
                Some(1),
                "{tier:?}: {text}\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let event = text
                .lines()
                .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                .find(|e| e["event"] == "test")
                .unwrap();
            assert_eq!(event["assertion"]["left"], left, "{tier:?}: {event}");
            assert_eq!(event["assertion"]["right"], right, "{tier:?}: {event}");
            assert!(event["assertion"]["message"].is_null(), "{event}");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn passing_output_does_not_gain_assertion_details() {
    let dir = std::env::temp_dir().join(format!("wolf-assert-pass-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("pass_test.lu");
    std::fs::write(
        &file,
        "fn test_ok() {\n    assert(true, \"do not print me\")\n    assert(1 == 1)\n}\n",
    )
    .unwrap();
    let out = wolf().arg("test").arg(&file).output().unwrap();
    assert!(out.status.success());
    let display = file.to_string_lossy().replace('\\', "/");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!(
            "test {display}::test_ok ... ok\nwolf test: 1 passed; 0 failed; 0 rejected; 0 unsupported; 0 filtered out\n"
        )
    );
    assert!(out.stderr.is_empty());
    let out = wolf().args(["test", "--json"]).arg(&file).output().unwrap();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(event.get("assertion").is_none(), "{event}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ordinary_and_freestanding_wir_keep_their_existing_trap_abi() {
    let dir = std::env::temp_dir().join(format!("wolf-assert-wir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.lu");
    let dump = dir.join("main.wir");
    std::fs::write(
        &file,
        "fn check(n: int) {\n    assert(n == 2, \"must be two\")\n}\nfn main() { check(1) }\n",
    )
    .unwrap();
    for target in [None, Some("x86_64-unknown-none")] {
        let mut cmd = wolf();
        cmd.args(["build", "--emit=wir", "--no-cache"])
            .arg(&file)
            .arg("-o")
            .arg(&dump);
        if let Some(target) = target {
            cmd.arg("--target").arg(target);
        }
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let wir = std::fs::read_to_string(&dump).unwrap();
        assert!(!wir.contains("__wolf_rt_assert_detail"), "{wir}");
        assert!(!wir.contains("__wolf_rt_strbuf"), "{wir}");
        assert!(wir.contains("  trap"), "{wir}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn equality_assertions_keep_both_evaluated_values_on_every_tier() {
    let dir = std::env::temp_dir().join(format!("wolf-assert-values-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("values_test.lu");
    std::fs::write(&file, "fn value() -> int {\n    print(\"evaluated once\")\n    11\n}\nfn assert_eq(a: int, b: int) {\n    assert(a == b, \"numbers differ\")\n}\nfn test_equality() {\n    assert_eq(value(), 13)\n}\nfn main() {\n    assert(false, \"not the selected entry\")\n}\n").unwrap();
    for tier in [None, Some("--native"), Some("--release")] {
        if tier == Some("--release") && cfg!(windows) {
            eprintln!("SKIP: the release tier is not supported on Windows");
            continue;
        }
        for json in [false, true] {
            let mut cmd = wolf();
            cmd.arg("test").arg(&file);
            if let Some(tier) = tier {
                cmd.arg(tier);
            }
            if json {
                cmd.arg("--json");
            }
            let out = cmd.output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            assert_eq!(
                out.status.code(),
                Some(1),
                "{tier:?}: {text}\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            if json {
                let event = text
                    .lines()
                    .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                    .find(|e| e["event"] == "test")
                    .unwrap();
                assert_eq!(
                    event["assertion"]["message"], "numbers differ",
                    "{tier:?}: {event}"
                );
                assert_eq!(event["assertion"]["left"], "11", "{tier:?}: {event}");
                assert_eq!(event["assertion"]["right"], "13", "{tier:?}: {event}");
                assert_eq!(event["stdout"], "evaluated once\n", "{event}");
                // Inlining can retain the helper's assertion site or its
                // caller. Both must be this source file's actual location.
                assert!(
                    ["values_test.lu:6:5", "values_test.lu:9:5"]
                        .iter()
                        .any(|site| event["assertion"]["site"]
                            .as_str()
                            .is_some_and(|actual| actual.ends_with(site))),
                    "{event}"
                );
            } else {
                let after = text.split("FAILED").nth(1).unwrap();
                assert!(after.contains("left: 11"), "{tier:?}: {text}");
                assert!(after.contains("right: 13"), "{tier:?}: {text}");
                assert_eq!(after.matches("evaluated once").count(), 1, "{text}");
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_assertions_have_messages_and_sites_even_without_nocapture() {
    let dir = std::env::temp_dir().join(format!("wolf-assert-details-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.lu");
    std::fs::write(
        &file,
        "fn main() {\n    assert(false, \"a diagnostic with \\n and \\\"quotes\\\"\")\n}\n",
    )
    .unwrap();
    for extra in [
        vec![],
        vec!["--nocapture"],
        vec!["--json"],
        vec!["--replay=1"],
        vec!["--replay=1", "--json"],
        vec!["--native"],
        vec!["--release"],
        vec!["--release", "--json"],
        vec!["--native", "--nocapture", "--json"],
        vec!["--release", "--nocapture"],
    ] {
        if extra.contains(&"--release") && cfg!(windows) {
            eprintln!("SKIP: the release tier is not supported on Windows");
            continue;
        }
        let out = wolf().arg("test").arg(&file).args(&extra).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(1),
            "{extra:?}: {text}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        if extra.contains(&"--json") {
            let event: serde_json::Value = text
                .lines()
                .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                .find(|e| e["event"] == "test")
                .unwrap();
            assert_eq!(
                event["assertion"]["message"],
                "a diagnostic with \n and \"quotes\""
            );
            assert!(
                event["assertion"]["site"]
                    .as_str()
                    .unwrap()
                    .ends_with("main.lu:2:5"),
                "{event}"
            );
        } else {
            let after = text.split("FAILED").nth(1).unwrap();
            assert!(after.contains("a diagnostic with"), "{extra:?}: {text}");
            assert!(after.contains("main.lu:2:5"), "{extra:?}: {text}");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
