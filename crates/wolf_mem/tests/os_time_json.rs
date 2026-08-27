//! s40 — the os/env, time, and json builtin tiers under checked
//! execution (the c08 capstone surface; wolf-std's std.os/std.env/
//! std.process/std.time/std.x.json build over exactly this).
//!
//! Errors are D30 rows throughout: an absent variable is `missing`,
//! an unspawnable program is `not_found`, a signal-killed child is
//! `signal`, malformed json is `parse` — each handleable with
//! `else`/`?`, never a trap. `env_set` writes the machine-local
//! overlay (the checked machine is a threaded test host; the struct
//! field documents the lane asymmetry), so these tests never pollute
//! the test process's environment.

use wolf_mem::ubcheck::{self, Budget, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

/// Statically clean ladder, then checked execution. Panics on refusal.
fn run(src: &str) -> ubcheck::RunOutcome {
    let mut ml = MemoryLoader::new("ostimejson");
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
    assert!(
        tc.not_yet.is_empty(),
        "input typechecks fully: {:?}",
        tc.not_yet
    );
    assert!(
        !tc.has_errors(),
        "input typechecks clean: {:?}",
        tc.diagnostics
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(
        mem.not_yet.is_empty(),
        "input stays inside the mem surface: {:?}",
        mem.not_yet
    );
    ubcheck::run_checked_with_input(&res.package, &tc, Budget::default(), "")
        .expect("the program is within the executable surface")
}

fn assert_stdout(src: &str, expected: &str) {
    let out = run(src);
    match out.verdict {
        Verdict::Exit(0) => {}
        other => panic!("expected exit(0), got {other:?} (stdout: {:?})", out.stdout),
    }
    assert_eq!(out.stdout, expected, "stdout");
}

// ------------------------------------------------------------ env/os --

#[test]
fn env_set_get_roundtrip_in_the_overlay() {
    assert_stdout(
        "fn main() -> !int {\n\
         env_set(\"WOLF_S40_DEN\", \"tarn\")?\n\
         let v = env_get(\"WOLF_S40_DEN\")?\n\
         print(\"got: {v}\")\n\
         0\n\
         }\n",
        "got: tarn\n",
    );
    // The overlay is machine-local: this test process never saw it.
    assert!(std::env::var("WOLF_S40_DEN").is_err());
}

#[test]
fn env_get_missing_is_a_row() {
    assert_stdout(
        "fn main() -> !int {\n\
         let v = env_get(\"WOLF_S40_ABSENT_9Q\") else |_| \"<missing>\"\n\
         print(\"{v}\")\n\
         0\n\
         }\n",
        "<missing>\n",
    );
}

#[test]
fn env_set_invalid_name_is_a_row() {
    assert_stdout(
        "fn main() -> !int {\n\
         env_set(\"A=B\", \"x\") else |_| print(\"invalid\")\n\
         env_set(\"\", \"x\") else |_| print(\"empty\")\n\
         0\n\
         }\n",
        "invalid\nempty\n",
    );
}

#[test]
fn env_args_default_is_empty() {
    // Conform-run supplies no argv — the stdin posture, mirrored.
    assert_stdout(
        "fn main() -> !int {\n\
         let args = env_args()\n\
         print(\"argc: {args.len}\")\n\
         0\n\
         }\n",
        "argc: 0\n",
    );
}

#[test]
fn env_vars_sees_the_overlay_sorted() {
    assert_stdout(
        "fn main() -> !int {\n\
         env_set(\"WOLF_S40_ZZB\", \"2\")?\n\
         env_set(\"WOLF_S40_ZZA\", \"1\")?\n\
         var hits = 0\n\
         var first = \"\"\n\
         for kv in env_vars() {\n\
         if kv.starts_with(\"WOLF_S40_ZZ\") {\n\
         if hits == 0 { first = kv }\n\
         hits = hits + 1\n\
         }\n\
         }\n\
         print(\"{hits} {first}\")\n\
         0\n\
         }\n",
        "2 WOLF_S40_ZZA=1\n",
    );
}

#[test]
fn cwd_is_nonempty() {
    assert_stdout(
        "fn main() -> !int {\n\
         let d = os_cwd()?\n\
         print(\"has_cwd: {d.len > 0}\")\n\
         0\n\
         }\n",
        "has_cwd: true\n",
    );
}

#[test]
fn os_exit_stops_with_the_code_and_skips_the_rest() {
    let out = run("fn main() -> !int {\n\
         print(\"before\")\n\
         os_exit(7)\n\
         print(\"after\")\n\
         0\n\
         }\n");
    assert!(matches!(out.verdict, Verdict::Exit(7)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "before\n");
}

#[test]
fn os_exit_masks_to_the_process_range() {
    let out = run("fn main() -> !int {\n\
         os_exit(300)\n\
         0\n\
         }\n");
    assert!(
        matches!(out.verdict, Verdict::Exit(44)), // 300 rem_euclid 256
        "{:?}",
        out.verdict
    );
}

// ----------------------------------------------------------- process --

#[test]
fn spawn_missing_program_is_not_found() {
    assert_stdout(
        "fn main() -> !int {\n\
         var argv = List[str]()\n\
         (mut argv).push(\"wolf-s40-no-such-binary-anywhere\")\n\
         os_spawn(argv) else |_| { print(\"not spawned\"); -1 }\n\
         0\n\
         }\n",
        "not spawned\n",
    );
}

#[test]
fn spawn_empty_argv_is_not_found() {
    assert_stdout(
        "fn main() -> !int {\n\
         let argv = List[str]()\n\
         os_spawn(argv) else |_| { print(\"empty\"); -1 }\n\
         0\n\
         }\n",
        "empty\n",
    );
}

#[test]
fn wait_on_a_forged_handle_is_io() {
    assert_stdout(
        "fn main() -> !int {\n\
         os_wait(99) else |_| { print(\"io\"); -1 }\n\
         os_kill(99) else |_| print(\"io2\")\n\
         0\n\
         }\n",
        "io\nio2\n",
    );
}

/// Real spawn/wait/kill against a live child — unix-gated like the
/// task layer's process scenarios (`/bin/sh` is the one portable-
/// enough fixture; windows coverage rides the std.process facade
/// sprint with its own fixture story).
#[cfg(unix)]
#[test]
fn spawn_wait_exit_code_and_double_wait() {
    assert_stdout(
        "fn main() -> !int {\n\
         var argv = List[str]()\n\
         (mut argv).push(\"/bin/sh\")\n\
         (mut argv).push(\"-c\")\n\
         (mut argv).push(\"exit 7\")\n\
         let h = os_spawn(argv)?\n\
         let code = os_wait(h)?\n\
         print(\"code: {code}\")\n\
         os_wait(h) else |_| { print(\"reaped\"); -1 }\n\
         0\n\
         }\n",
        "code: 7\nreaped\n",
    );
}

#[cfg(unix)]
#[test]
fn kill_then_wait_is_the_signal_row() {
    assert_stdout(
        "fn main() -> !int {\n\
         var argv = List[str]()\n\
         (mut argv).push(\"/bin/sh\")\n\
         (mut argv).push(\"-c\")\n\
         (mut argv).push(\"sleep 30\")\n\
         let h = os_spawn(argv)?\n\
         os_kill(h)?\n\
         os_wait(h) else |_| { print(\"signalled\"); -1 }\n\
         0\n\
         }\n",
        "signalled\n",
    );
}

// -------------------------------------------------------------- time --

#[test]
fn monotonic_now_and_sleep() {
    assert_stdout(
        "fn main() -> !int {\n\
         let a = time_now_ms()\n\
         time_sleep_ms(2)\n\
         let b = time_now_ms()\n\
         print(\"nonneg: {a >= 0}\")\n\
         print(\"advanced: {b > a}\")\n\
         0\n\
         }\n",
        "nonneg: true\nadvanced: true\n",
    );
}

#[test]
fn unix_ms_is_after_2020() {
    assert_stdout(
        "fn main() -> !int {\n\
         let w = time_unix_ms()\n\
         print(\"modern: {w > 1577836800000}\")\n\
         0\n\
         }\n",
        "modern: true\n",
    );
}

// -------------------------------------------------------------- json --

#[test]
fn json_valid_answers_rfc8259() {
    assert_stdout(
        "fn main() -> !int {\n\
         let a = json_valid(\"[1, 2, 3]\")\n\
         let b = json_valid(\"[1, 2,\")\n\
         let c = json_valid(\"01\")\n\
         print(\"{a} {b} {c}\")\n\
         0\n\
         }\n",
        "true false false\n",
    );
}

#[test]
fn json_get_walks_paths() {
    assert_stdout(
        "fn main() -> !int {\n\
         let doc = \"{{\\\"pack\\\": [{{\\\"name\\\": \\\"lupin\\\"}}, \
         {{\\\"name\\\": \\\"ainu\\\"}}], \\\"n\\\": 42}}\"\n\
         let first = json_get(doc, \"pack.0.name\")?\n\
         let n = json_get(doc, \"n\")?\n\
         print(\"{first} {n}\")\n\
         0\n\
         }\n",
        "lupin 42\n",
    );
}

#[test]
fn json_rows_parse_missing_kind() {
    assert_stdout(
        "fn main() -> !int {\n\
         json_get(\"{{\", \"x\") else |_| { print(\"parse\"); \"\" }\n\
         json_get(\"[1]\", \"9\") else |_| { print(\"missing\"); \"\" }\n\
         json_len(\"[1]\", \"0\") else |_| { print(\"kind\"); -1 }\n\
         0\n\
         }\n",
        "parse\nmissing\nkind\n",
    );
}

#[test]
fn json_type_and_len() {
    assert_stdout(
        "fn main() -> !int {\n\
         let doc = \"{{\\\"a\\\": [1, 2, 3], \\\"b\\\": null}}\"\n\
         let t = json_type(doc, \"a\")?\n\
         let u = json_type(doc, \"b\")?\n\
         let n = json_len(doc, \"a\")?\n\
         let m = json_len(doc, \"\")?\n\
         print(\"{t} {u} {n} {m}\")\n\
         0\n\
         }\n",
        "array null 3 2\n",
    );
}

#[test]
fn json_escapes_decode() {
    // The json text is `["A"]`; the decoded element is "A".
    assert_stdout(
        "fn main() -> !int {\n\
         let s = json_get(\"[\\\"\\\\u0041\\\"]\", \"0\")?\n\
         let hit = s == \"A\"\n\
         print(\"{hit} {s.len}\")\n\
         0\n\
         }\n",
        "true 1\n",
    );
}

// ------------------------------------------------------------ os_random --
//
// The OS random source (s118, #143). NOTE the property being asserted:
// two draws DIFFER — the weakest honest property a unit test can
// assert about an entropy source without becoming a statistical
// instrument (which a unit test must not be; a flaky distribution
// assertion is worse than none). Equal 32-byte draws from a working
// CSPRNG have probability 2^-256: equality means broken, not unlucky.

#[test]
fn os_random_two_draws_differ_and_are_bytes() {
    assert_stdout(
        "fn main() -> !int {\n\
         let a = os_random(32)\n\
         let b = os_random(32)\n\
         var range = true\n\
         for x in a { if x < 0 || x > 255 { range = false } }\n\
         for x in b { if x < 0 || x > 255 { range = false } }\n\
         var same = a.len == b.len\n\
         var i = 0\n\
         for x in b { if x != a[i] { same = false }\n\
         i = i + 1 }\n\
         print(\"len={a.len}/{b.len} range={range} same={same}\")\n\
         0\n\
         }\n",
        "len=32/32 range=true same=false\n",
    );
}

#[test]
fn os_random_zero_and_large_lengths() {
    // Length 0 is a valid request for no entropy (the empty list, no
    // trap); a large request (past the 256-byte per-call platform
    // caps) comes back complete — the fill loop owns the boundary.
    assert_stdout(
        "fn main() -> !int {\n\
         let z = os_random(0)\n\
         let big = os_random(65536)\n\
         var range = true\n\
         for x in big { if x < 0 || x > 255 { range = false } }\n\
         print(\"z={z.len} big={big.len} range={range}\")\n\
         0\n\
         }\n",
        "z=0 big=65536 range=true\n",
    );
}

#[test]
fn os_random_negative_count_traps_assert() {
    // n < 0 is a caller-contract violation: the deterministic trap
    // `assert` (the [mem.str.repeat] posture), ruled by
    // [os.random.fill] — never an empty list, never a row.
    let out = run("fn main() -> !int {\n\
         let b = os_random(-1)\n\
         b.len\n\
         }\n");
    match out.verdict {
        Verdict::Trap(t) => {
            assert_eq!(t.kind, "assert", "trap kind");
            assert_eq!(t.clause, "os.random.fill", "ruling clause");
        }
        other => panic!("expected trap(assert), got {other:?}"),
    }
}
