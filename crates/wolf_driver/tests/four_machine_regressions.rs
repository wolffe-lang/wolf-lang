//! s270: reduced compiler regressions compared against independent answers on
//! all four machines. A compiler ICE or language refusal is never a skip.
mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn assert_answer(out: &Output, row: &str, machine: &str, stdout: &str) {
    assert!(
        out.status.success(),
        "{machine} {row}: {}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let record: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{machine} {row}: {e}: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert_eq!(record["verdict"], "exit(0)", "{machine} {row}: {record}");
    assert_eq!(record["stdout_inline"], stdout, "{machine} {row}: {record}");
}

fn four_machines(row: &str, stdout: &str) {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(row);
    for machine in ["--checked", "--native", "--release"] {
        let out = Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["conform-run", "--json", machine])
            .arg(&entry)
            .output()
            .expect("run wolf");
        if machine != "--checked" && lane_exit::environment_refusal(&out, machine) {
            assert!(
                std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
                "required {machine} unavailable: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            eprintln!(
                "SKIP: {machine} {row}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            continue;
        }
        assert_answer(&out, row, machine, stdout);
    }
    let lupin = std::env::var_os("LUPIN").map(PathBuf::from).or_else(|| {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("../wolf-interp/target/release/lupin");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    });
    let Some(lupin) = lupin else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "required lupin missing"
        );
        eprintln!("SKIP: lupin {row}: set LUPIN to run the fourth machine");
        return;
    };
    let out = Command::new(lupin)
        .args(["conform-run", "--json"])
        .arg(entry)
        .output()
        .expect("run lupin");
    assert_answer(&out, row, "lupin", stdout);
}

#[test]
fn bare_enum_variant_initializes_its_unused_string_slot() {
    four_machines(
        "typecheck/enum_str_bare_choice.lu",
        "T.Text(x)\nT.Nothing\n",
    );
}

#[test]
fn mixed_error_payload_display_projects_the_active_tag() {
    four_machines("rows/mixed_payload_display.lu", "A(0)\nB(p)\nA(0)\nB(p)\n");
}
