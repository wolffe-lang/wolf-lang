//! s225 (`[os.env.unset]`, `[os.proc.exec]`) — the checked machine's
//! halves that run in-process. `env_unset` is a tombstone in the
//! machine-local overlay: `env_get` answers `missing`, `env_vars` drops
//! the name — a variable the HOST holds included — and the test
//! process's own environment is never written (it is a threaded host).
//! `os_exec` is exercised here only where it RETURNS: a successful exec
//! would replace this test binary (the replacing witnesses are
//! `crates/wolf_driver/tests/exec_lanes.rs`, out of process).

use wolf_mem::ubcheck::{self, Budget, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn run(src: &str) -> ubcheck::RunOutcome {
    let mut ml = MemoryLoader::new("execenv");
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
    assert!(tc.not_yet.is_empty(), "typechecks fully: {:?}", tc.not_yet);
    assert!(!tc.has_errors(), "typechecks clean: {:?}", tc.diagnostics);
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(mem.not_yet.is_empty(), "mem surface: {:?}", mem.not_yet);
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

/// A removed variable is unlisted, a host variable included; the host
/// process keeps its own.
#[test]
fn env_unset_drops_the_name_from_env_vars() {
    let host = "WOLF_S225_CHECKED_HOST";
    // SAFETY: set before the machine runs; no other test reads this name.
    unsafe { std::env::set_var(host, "kept") };
    assert_stdout(
        r#"
fn listed(name: str) -> int {
    var n = 0
    let want = "{name}="
    for kv in env_vars() {
        if kv.starts_with(want) { n += 1 }
    }
    n
}

fn main() -> !int {
    env_set("WOLF_S225_CHECKED_SET", "x")?
    print("set {listed("WOLF_S225_CHECKED_SET")} host {listed("WOLF_S225_CHECKED_HOST")}")
    env_unset("WOLF_S225_CHECKED_SET")?
    env_unset("WOLF_S225_CHECKED_HOST")?
    print("set {listed("WOLF_S225_CHECKED_SET")} host {listed("WOLF_S225_CHECKED_HOST")}")
    let h = env_get("WOLF_S225_CHECKED_HOST") else |_| "<missing>"
    print("host get {h}")
    0
}
"#,
        "set 1 host 1\nset 0 host 0\nhost get <missing>\n",
    );
    assert_eq!(
        std::env::var(host).as_deref(),
        Ok("kept"),
        "the threaded host's environment is not written"
    );
}

/// Exec rows that return, in-process: the shape, a missing path, a bare
/// name the handed PATH cannot find — and what the program printed before
/// them stays in the record (no refusal the machine can see coming lets
/// the buffer out).
#[test]
fn exec_rows_that_return_keep_the_record() {
    assert_stdout(
        r#"
fn row(exe: str, argv: List[str], env: List[str]) -> str {
    os_exec(exe, argv, env, List[int]()) else |e| {
        return match e {
            unsupported => "unsupported",
            invalid => "invalid",
            not_found => "not_found",
            denied => "denied",
            io => "io",
        }
    }
    "returned"
}

fn main() -> !int {
    print("first")
    print("shape {row("sh", List[str](), List[str]())}")
    print("missing {row("/nonexistent/wolf-s225", ["x"], List[str]())}")
    print("bare {row("sh", ["sh"], ["PATH=/nonexistent/wolf-s225"])}")
    print("dir {row("/", ["x"], List[str]())}")
    0
}
"#,
        "first\nshape invalid\nmissing not_found\nbare not_found\ndir denied\n",
    );
}
