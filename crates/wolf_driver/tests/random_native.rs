//! s118 — the OS random source (#143), cross-lane.
//!
//! The signal_native discipline over the entropy surface: every lane
//! makes the REAL platform call (`getrandom(2)` at this pin — the
//! checked machine is a host process with the same kernel-pool access
//! as a compiled program, `[os.random.checked]`), so the witnesses
//! assert deterministic CONTRACT properties — lengths, byte range,
//! draws-differ, the trap — and never pin a byte value or sample a
//! distribution. Draws-differ is the WEAKEST honest property (two
//! equal 32-byte CSPRNG draws have probability 2^-256: equality means
//! broken, not unlucky); anything stronger is a research instrument,
//! not a test.
//!
//! Hosts the native tier refuses skip loudly at runtime (the s59
//! pattern: these tests start passing the moment a gate lifts).

use std::path::Path;
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
}

fn lane(case: &str, src: &str, flag: &str) -> Option<Obs> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join(format!("{case}.lu"));
    std::fs::write(&entry, src).expect("write fixture");
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(&entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if out.status.code() == Some(2) && flag == "--native" {
        eprintln!(
            "SKIP: environment cannot run the native lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {case}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("observation record parses");
    Some(Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    })
}

/// Differ-twice plus the edge lengths agree across lanes: the printed
/// verdict is deterministic even though the bytes are not (the draws
/// are compared to each other, never to a pin). Ten native runs pin
/// stability.
#[test]
fn draws_differ_and_edges_agree_across_lanes_ten_run_stable() {
    let src = r#"
fn main() -> !int {
    let a = os_random(32)
    let b = os_random(32)
    var range = true
    for x in a {
        if x < 0 || x > 255 { range = false }
    }
    var same = a.len == b.len
    var i = 0
    for x in b {
        if x != a[i] { same = false }
        i = i + 1
    }
    let z = os_random(0)
    let big = os_random(65536)
    print("len={a.len} range={range} same={same} z={z.len} big={big.len}")
    0
}
"#;
    let expect = "len=32 range=true same=false z=0 big=65536\n";
    let checked = lane("s118_differ", src, "--checked").expect("checked always runs");
    assert_eq!(checked.verdict, "exit(0)", "checked verdict");
    assert_eq!(checked.stdout, expect, "checked stdout");
    for _ in 0..10 {
        let Some(native) = lane("s118_differ", src, "--native") else {
            return; // environment cannot run the native lane
        };
        assert_eq!(native.verdict, checked.verdict, "cross-lane verdict");
        assert_eq!(native.stdout, checked.stdout, "cross-lane stdout");
    }
}

/// The trap witness, both lanes: a negative count is the deterministic
/// trap `assert` (`[os.random.fill]`, the caller contract) — never an
/// empty list, never a row. The OS-failure trap of `[os.random.trap]`
/// rides the SAME nonzero-rc branch in lowering; this is its honest
/// executable witness (a host whose CSPRNG fails is not a thing a test
/// can arrange).
#[test]
fn negative_count_traps_assert_both_lanes() {
    let src = r#"
fn main() -> !int {
    let b = os_random(-1)
    b.len
}
"#;
    let checked = lane("s118_neg", src, "--checked").expect("checked always runs");
    assert_eq!(checked.verdict, "trap(assert)", "checked trap");
    if let Some(native) = lane("s118_neg", src, "--native") {
        assert_eq!(native.verdict, "trap(assert)", "native trap");
    }
}
