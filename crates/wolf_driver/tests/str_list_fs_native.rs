//! s40 acceptance — the native str/List/fs tier, cross-lane.
//!
//! Every fixture runs through `conform-run` on BOTH lanes (`--checked`
//! = the reference executor, `--native` = wir → Cranelift → cc) and
//! must produce the IDENTICAL verdict and stdout hash — the corpus's
//! cross-lane sha discipline as a pinned test, extended over the s40
//! surface: value-position interpolation (with format specs), the s37
//! str method set, region-backed Lists (push/pop/get/len/iteration,
//! `bounds` identity on OOB), and the s38 fs builtin family over D30
//! rows. Native refusals FAIL here — this file is the anti-regression
//! gate for exactly the refusal family #40 closed.
//!
//! Off-target the whole file compiles away (native codegen is
//! linux/x86-64 only at this tier).

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::path::Path;
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

struct Obs {
    verdict: String,
    stdout: String,
}

/// One conform-run lane over a fixture. `None` (with a loud SKIP)
/// only for environment failures (exit 2 from the native rung: no cc,
/// no rt staticlib); refusals are visible as `unsupported` verdicts
/// and fail the assertions below.
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

/// Run both lanes; assert identical verdict + stdout AND the expected
/// verdict/stdout. Skips (environment only) skip the whole assertion.
fn parity(case: &str, src: &str, want_verdict: &str, want_stdout: &str) {
    let checked = lane(case, src, "--checked").expect("checked lane always runs");
    assert_eq!(
        checked.verdict, want_verdict,
        "{case}: checked verdict (stdout {:?})",
        checked.stdout
    );
    assert_eq!(checked.stdout, want_stdout, "{case}: checked stdout");
    let Some(native) = lane(case, src, "--native") else {
        return;
    };
    assert_eq!(
        native.verdict, checked.verdict,
        "{case}: cross-lane verdict divergence"
    );
    assert_eq!(
        native.stdout, checked.stdout,
        "{case}: cross-lane stdout divergence"
    );
}

#[test]
fn interpolation_materializes_in_value_position() {
    parity(
        "s40_interp_value",
        r#"
fn greet(name: str) -> str {
    "hello, {name}"
}

fn main() -> !int {
    let g = greet("wolf")
    let n = 7
    let pi = 1.5
    let all = "{g} n={n:>3} f={pi} b={n == 7}"
    print(all)
    print("len={all.len}")
    0
}
"#,
        "exit(0)",
        "hello, wolf n=  7 f=1.5 b=true\nlen=30\n",
    );
}

#[test]
fn the_str_method_set_agrees() {
    parity(
        "s40_str_methods",
        r#"
fn main() -> !int {
    let s = "  the Wolf runs  "
    let t = s.trim()
    print(t.upper())
    print(t.lower())
    print("{t.len} {t.is_empty()} {s.trim_start().len} {s.trim_end().len}")
    print("{t.starts_with("the")} {t.ends_with("runs")} {t.contains("Wolf")}")
    let off = t.find("Wolf") else { return 9 }
    let roff = t.rfind("s") else { return 9 }
    print("{off} {roff} {t.count("o")}")
    let mid = t.get(4..8) else { return 9 }
    let miss = t.get(0..99) else "?"
    print("{mid} {miss}")
    let rest = t.strip_prefix("the ") else { return 9 }
    print(rest.replace("runs", "sleeps"))
    print("ab".repeat(3))
    let parts = t.split(" ")
    let ws = t.words()
    let ls = "a\nb".lines()
    let bs = "é".bytes()
    print("{parts.len} {ws.len} {ls.len} {bs.len} {bs[0]}")
    // [mem.str.empty] (s71, #56): the empty-needle family is defined
    // identically on both lanes — count 0, one whole piece, identity.
    let ep = "abc".split("")
    print("{"abc".count("")} {ep.len} {ep[0]} {"abc".replace("", "-")}")
    0
}
"#,
        "exit(0)",
        "THE WOLF RUNS\nthe wolf runs\n13 false 15 15\ntrue true true\n4 12 1\nWolf ?\nWolf sleeps\nababab\n3 3 2 2 195\n0 1 abc abc\n",
    );
}

#[test]
fn str_ordering_and_equality_agree() {
    parity(
        "s40_str_order",
        r#"
fn main() -> !int {
    let a = "wolf" < "wolves" && "z" < "é" && "a" <= "a" && "b" > "a"
    let b = "wolf" == "wolf" && "wolf" != "Wolf"
    print("{a} {b}")
    0
}
"#,
        "exit(0)",
        "true true\n",
    );
}

#[test]
fn lists_grow_read_pop_and_iterate() {
    parity(
        "s40_list_basic",
        r#"
fn main() -> !int {
    var xs = List[int]()
    var i = 0
    while i < 40 {
        (mut xs).push(i * i)
        i += 1
    }
    var total = 0
    var k = 0
    while k < xs.len {
        total += xs[k]
        k += 1
    }
    let last = (mut xs).pop() else { return 9 }
    let first = xs.first() else { return 9 }
    let got = xs.get(3) else { return 9 }
    print("{total} {last} {first} {got} {xs.is_empty()}")
    (mut xs).clear()
    let gone = (mut xs).pop() else 0 - 1
    print("{xs.len} {gone}")
    0
}
"#,
        "exit(0)",
        "20540 1521 0 9 false\n0 -1\n",
    );
}

#[test]
fn list_oob_is_the_bounds_trap_on_both_lanes() {
    parity(
        "s40_list_oob",
        r#"
fn main() -> !int {
    var xs = List[int]()
    (mut xs).push(1)
    print("{xs[2]}")
    0
}
"#,
        "trap(bounds)",
        "",
    );
}

#[test]
fn str_lists_hold_pairs() {
    parity(
        "s40_list_str",
        r#"
fn main() -> !int {
    var names = List[str]()
    (mut names).push("wolf")
    (mut names).push("pack of {3} wolves")
    print("{names.len} {names[1]} {names[0].upper()}")
    0
}
"#,
        "exit(0)",
        "2 pack of 3 wolves WOLF\n",
    );
}

#[test]
fn fs_roundtrip_and_rows_agree() {
    // The fixture writes inside the test tmpdir; both lanes see the
    // same real filesystem.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s40_fs_data");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("note.txt");
    let src = format!(
        r#"
fn main() -> !int {{
    let p = "{p}"
    fs_write_text(p, "three wolves\n")?
    let text = fs_read_text(p)?
    print("read: {{text.trim()}} exists={{fs_exists(p)}}")
    let fd = fs_open(p)?
    let head = fs_read(fd, 5)?
    fs_close(fd)?
    fs_remove(p)?
    let miss = fs_read_text(p) else |_| "gone"
    print("{{head}} {{miss}} exists={{fs_exists(p)}}")
    0
}}
"#,
        p = path.display()
    );
    parity(
        "s40_fs_roundtrip",
        &src,
        "exit(0)",
        "read: three wolves exists=true\nthree gone exists=false\n",
    );
}

#[test]
fn fs_error_rows_carry_their_tags() {
    parity(
        "s40_fs_tags",
        r#"
fn main() -> !int {
    let text = fs_read_text("target/s40-no-such-dir/absent.txt")?
    print("unreachable: {text}")
    0
}
"#,
        "exit(1)",
        "error: not_found\n",
    );
}

// ------------------------- s81 / #58: the validating byte source --

#[test]
fn str_from_utf8_validates_identically_on_both_lanes() {
    // The border post (#58). The two lanes run different code — the
    // checked machine's `String::from_utf8`, the native tier's
    // `wolf_rt::str::__wolf_rt_str_from_utf8` — so the parity here is
    // the claim that "it validates" means the SAME set on both. Every
    // refused column is one named UTF-8 failure mode, plus the two
    // elements that are not bytes at all.
    parity(
        "s81_from_utf8",
        r#"
fn decode(bs: List[int]) -> str {
    str_from_utf8(bs) else "X"
}

fn seq(a: int, b: int, c: int, dd: int, n: int) -> List[int] {
    var l = List[int]()
    if n > 0 { (mut l).push(a) }
    if n > 1 { (mut l).push(b) }
    if n > 2 { (mut l).push(c) }
    if n > 3 { (mut l).push(dd) }
    l
}

fn main() -> !int {
    let ok = decode(seq(119, 111, 108, 102, 4))
    let astral = decode(seq(240, 159, 144, 186, 4))
    let nul = decode(seq(119, 0, 102, 0, 3))
    let round = decode("wolf é".bytes())
    print("{ok} {astral} {nul.len} {round} {round.len}")
    let lone = decode(seq(128, 0, 0, 0, 1))
    let trunc = decode(seq(226, 130, 0, 0, 2))
    let overlong = decode(seq(192, 175, 0, 0, 2))
    let surrogate = decode(seq(237, 160, 128, 0, 3))
    let too_big = decode(seq(245, 128, 128, 128, 4))
    let not_a_byte = decode(seq(256, 0, 0, 0, 1))
    let negative = decode(seq(0 - 1, 0, 0, 0, 1))
    print("{lone}{trunc}{overlong}{surrogate}{too_big}{not_a_byte}{negative}")
    0
}
"#,
        "exit(0)",
        "wolf 🐺 3 wolf é 7\nXXXXXXX\n",
    );
}

#[test]
fn str_equality_agrees_across_the_inline_threshold() {
    // s81 target 1: `==` is a length guard plus an inline byte compare
    // below 64 bytes and the runtime's `memcmp` above it. Two code
    // paths, one answer — and the checked lane, which has neither,
    // is the referee.
    parity(
        "s81_str_eq",
        r#"
fn eq(a: str, b: str) -> int {
    if a == b { 1 } else { 0 }
}

fn main() -> !int {
    let short_hit = eq("wolf", "wolf")
    let short_miss = eq("wolfa", "wolfb")
    let len_miss = eq("wolf", "wolves")
    let empty = eq("", "")
    let long_a = "wolf".repeat(40)
    let long_hit = eq(long_a, "wolf".repeat(40))
    let long_miss = eq(long_a, "{"wolf".repeat(39)}wolg")
    let long_len = eq(long_a, "wolf".repeat(39))
    let ne = if "é" != "e" { 1 } else { 0 }
    print("{short_hit}{short_miss}{len_miss}{empty}{long_hit}{long_miss}{long_len}{ne}")
    0
}
"#,
        "exit(0)",
        "10011001\n",
    );
}

#[test]
fn the_runtime_symbol_table_covers_the_s40_families() {
    // Every shim lowering can emit must be declared in the codegen's
    // symbol contract (the RT_SYMBOLS drift guard).
    for family in [
        "strbuf_", "str_", "list_", "fs_", "env_", "os_", "time_",
        // The s73 conc families (scope/chan/select/sync/proc).
        "scope_", "chan_", "sync_", "when_", "proc_",
    ] {
        let n = wolf_codegen_clif::RT_SYMBOLS
            .iter()
            .filter(|(name, ..)| name.starts_with(&format!("__wolf_rt_{family}")))
            .count();
        assert!(n > 0, "no {family} symbols in RT_SYMBOLS");
    }
    // 78 at s40/s73; s75 adds the D43 line brackets; s76 the
    // ambient-region enter/leave pair; s81 the validating byte source
    // (`str_from_utf8`, wolf-lang#58).
    assert_eq!(
        wolf_codegen_clif::RT_SYMBOLS.len(),
        83,
        "RT_SYMBOLS count moved — keep the s40/s73 families in sync with wolf_rt"
    );
}
