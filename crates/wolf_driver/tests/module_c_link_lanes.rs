//! s224 (wolf-lang#620) — a module named `c` links on every tier.
//!
//! WIR names are module-path qualified (s30, #26), so a function `go` in
//! a module whose path is `c` — a child directory `c/`, or a dependency
//! under the alias `c` — is `c.go` below the checker. That is also the C
//! membrane's spelling of an imported function (`[abi.c.import]`), and
//! until s224 both backends read any `c.<C identifier>` callee outside
//! the current object as one: the native tier linked `c.go` as the C
//! symbol `go` and failed with `undefined reference to 'go'` (exit 2),
//! and release did the same once its cluster partition put the callee
//! in another object. pkg.rs's diamond met the native half and read the
//! exit 2 as an environment skip (now a failure, `lane_exit`).
//!
//! Five programs, on the checked, native and release machines and lupin:
//! t05's shape (dependencies `b` and `c`, each defining `go`), one
//! dependency named `c`, the child module `c` (`corpus/link/
//! module_c.lu`), a release build whose clusters separate `main` from a
//! large recursive `c.go` (the fixture asserts the split, so a future
//! clusterer that merges them fails here instead of passing vacuously),
//! and the one true clash — an imported C `llabs` beside a module-`c`
//! `llabs` — which lowering now refuses by name instead of the ICE it
//! was (`@c.llabs is used with two different signatures`).
//!
//! Measured at trunk `ac0ac498` and `a0169704` (hasu): the first four
//! failed the native link (the fourth also release); checked and lupin
//! answered all four. The clash was an ICE on native and release.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn record(bytes: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice(bytes).ok()
}

/// One wolfgang machine's record, or `None` for the s59 environment
/// skip. A link error or an ICE fails (`lane_exit`); an exit with no
/// record is never a skip (#550).
fn wolfgang(dir: &Path, flag: &str) -> Option<serde_json::Value> {
    let out = Command::new(wolf())
        .current_dir(dir)
        .args(["conform-run", "main.lu", flag, "--json"])
        .output()
        .expect("wolf runs");
    if let Some(r) = record(&out.stdout) {
        return Some(r);
    }
    let what = format!("wolf conform-run {flag} in {}", dir.display());
    if flag != "--checked" && lane_exit::environment_refusal(&out, &what) {
        eprintln!("SKIP {what}: {}", String::from_utf8_lossy(&out.stderr).trim());
        return None;
    }
    panic!(
        "{what} left no record (exit {:?}): {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn require_lupin() -> Option<PathBuf> {
    let l = sibling_lupin();
    if l.is_none() {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of wolf-lang#620's gate did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!("SKIP: no sibling lupin on this box (set LUPIN) — the oracle leg is absent");
    }
    l
}

fn lupin_record(l: &Path, dir: &Path) -> serde_json::Value {
    let out = Command::new(l)
        .current_dir(dir)
        .args(["conform-run", "main.lu", "--json"])
        .output()
        .expect("lupin runs");
    record(&out.stdout).unwrap_or_else(|| {
        panic!(
            "lupin left no record in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn stage(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("module_c_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("stage dir");
    dir
}

fn write(path: PathBuf, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("parent dir");
    std::fs::write(path, text).expect("fixture writes");
}

const LEG: &str = "/// One more than `x`.\npub fn go(x: int) -> int {\n    x + 1\n}\n\n\
                   /// Two more than `x`.\npub fn go2(x: int) -> int {\n    x + 2\n}\n";

fn manifest(name: &str, deps: &[&str]) -> String {
    let mut s = format!("pkg {{\n    name:    \"{name}\",\n    version: \"0.1.0\",\n");
    if !deps.is_empty() {
        s.push_str("\n    deps: {\n");
        for d in deps {
            s.push_str(&format!("        {d}: {{ path: \"{d}\" }},\n"));
        }
        s.push_str("    },\n");
    }
    s.push_str("}\n");
    s
}

/// t05's witness: two path dependencies, `b` and `c`, each `pub fn go`.
fn two_deps() -> PathBuf {
    let dir = stage("two_deps");
    write(dir.join("wolf.pkg"), &manifest("demo/app", &["b", "c"]));
    for leg in ["b", "c"] {
        write(dir.join(leg).join("wolf.pkg"), &manifest(&format!("acme/{leg}"), &[]));
        write(dir.join(leg).join(format!("{leg}.lu")), LEG);
    }
    write(
        dir.join("main.lu"),
        "use b\nuse c\n\nfn main() -> !int {\n    let one = b.go(10)\n    \
         let two = c.go(20)\n    print(\"{one} {two}\")\n    0\n}\n",
    );
    dir
}

/// One dependency, aliased `c`: the alias alone is the trigger.
fn one_dep_c() -> PathBuf {
    let dir = stage("one_dep");
    write(dir.join("wolf.pkg"), &manifest("demo/one", &["c"]));
    write(dir.join("c").join("wolf.pkg"), &manifest("acme/c", &[]));
    write(dir.join("c").join("c.lu"), LEG);
    write(
        dir.join("main.lu"),
        "use c\n\nfn main() -> !int {\n    let two = c.go2(20)\n    print(\"{two}\")\n    0\n}\n",
    );
    dir
}

/// A recursive `c.go` too large to share a cluster with `main`.
fn split_clusters(tag: &str) -> PathBuf {
    let dir = stage(&format!("split_{tag}"));
    let mut c = String::from(
        "/// A deliberately large recursive walk.\npub fn go(x: int, n: int) -> int {\n    \
         if n == 0 { return x }\n    var acc = x\n",
    );
    for k in 0..300 {
        c.push_str(&format!("    acc = (acc * 3 + {k}) % 1000003\n"));
    }
    c.push_str("    go(acc, n - 1)\n}\n\n/// Filler.\npub fn id(x: int) -> int {\n    x\n}\n");
    write(dir.join("c").join("c.lu"), &c);
    let mut m = String::from("use c\n\nfn main() -> !int {\n    var acc = 7\n");
    for k in 0..300 {
        m.push_str(&format!("    acc = (acc * 5 + {k}) % 999983\n"));
    }
    m.push_str("    let r = c.go(acc, 3)\n    print(\"{acc} {r}\")\n    0\n}\n");
    write(dir.join("main.lu"), &m);
    dir
}

/// The true clash: an imported C `llabs` and a module-`c` `llabs`.
fn clash() -> PathBuf {
    let dir = stage("clash");
    write(
        dir.join("c").join("c.lu"),
        "/// A wolf function that shares a C name.\npub fn llabs(n: i64) -> i64 {\n    n + 100\n}\n\n\
         /// Filler.\npub fn other(n: i64) -> i64 {\n    n\n}\n",
    );
    write(
        dir.join("main.lu"),
        "use c\n\nextern \"c\" fn llabs(n: i64) -> i64\n\nfn main() -> int {\n    \
         // # Safety: llabs has no preconditions.\n    let a = unsafe { llabs(-34) } as int\n    \
         let w = c.llabs(-34) as int\n    print(\"{a} {w}\")\n    0\n}\n",
    );
    dir
}

fn stdout_of(r: &serde_json::Value) -> (String, String) {
    (
        r["verdict"].as_str().unwrap_or("").to_string(),
        r["stdout_inline"].as_str().unwrap_or("").to_string(),
    )
}

#[test]
fn a_module_named_c_links_on_every_tier() {
    let cases: Vec<(&str, PathBuf, &str)> = vec![
        ("two dependencies b and c", two_deps(), "11 21\n"),
        ("one dependency aliased c", one_dep_c(), "22\n"),
        ("the release cluster split", split_clusters("run"), "259168 582955\n"),
    ];
    let lupin = require_lupin();
    let mut bad = Vec::new();
    for (what, dir, want) in &cases {
        for flag in ["--checked", "--native", "--release"] {
            if let Some(r) = wolfgang(dir, flag) {
                let (v, out) = stdout_of(&r);
                if v != "exit(0)" || out != *want {
                    bad.push(format!("{what} {flag}: {v} {out:?}, want {want:?}"));
                }
            }
        }
        if let Some(l) = &lupin {
            let (v, out) = stdout_of(&lupin_record(l, dir));
            if v != "exit(0)" || out != *want {
                bad.push(format!("{what} lupin: {v} {out:?}, want {want:?}"));
            }
        }
    }
    assert!(bad.is_empty(), "wolf-lang#620:\n{}", bad.join("\n"));
}

/// The corpus row is the child-module shape; `xtask corpus` runs it on
/// native only, so the other machines are asked here.
#[test]
fn the_child_module_row_answers_on_four_machines() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/link");
    let run = |flag: &str| {
        let out = Command::new(wolf())
            .current_dir(&dir)
            .args(["conform-run", "module_c.lu", flag, "--json"])
            .output()
            .expect("wolf runs");
        if let Some(r) = record(&out.stdout) {
            return Some(stdout_of(&r));
        }
        let what = format!("module_c.lu {flag}");
        if flag != "--checked" && lane_exit::environment_refusal(&out, &what) {
            eprintln!("SKIP {what}: {}", String::from_utf8_lossy(&out.stderr).trim());
            return None;
        }
        panic!("{what} left no record: {}", String::from_utf8_lossy(&out.stderr));
    };
    let want = ("exit(0)".to_string(), "21 22\n".to_string());
    for flag in ["--checked", "--native", "--release"] {
        if let Some(got) = run(flag) {
            assert_eq!(got, want, "module_c.lu {flag}");
        }
    }
    if let Some(l) = require_lupin() {
        let out = Command::new(l)
            .current_dir(&dir)
            .args(["conform-run", "module_c.lu", "--json"])
            .output()
            .expect("lupin runs");
        let r = record(&out.stdout).expect("lupin record");
        assert_eq!(stdout_of(&r), want, "module_c.lu lupin");
    }
}

/// The split fixture must split: `main` and `c.go` in different
/// clusters, or the release half of this gate tests nothing.
#[test]
fn the_split_fixture_puts_c_go_in_its_own_cluster() {
    let dir = split_clusters("report");
    let out = Command::new(wolf())
        .current_dir(&dir)
        .args(["build", "--release", "--codegen-report", "main.lu", "-o"])
        .arg(dir.join("rel"))
        .output()
        .expect("wolf runs");
    if out.status.code() != Some(0)
        && lane_exit::environment_refusal(&out, "wolf build --release (split)")
    {
        eprintln!("SKIP split report: {}", String::from_utf8_lossy(&out.stderr).trim());
        return;
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let owner = |f: &str| {
        text.lines()
            .filter(|l| l.starts_with("cluster "))
            .find(|l| {
                l.split("members=[")
                    .nth(1)
                    .and_then(|m| m.split(']').next())
                    .is_some_and(|m| m.split(',').any(|x| x.trim() == f))
            })
            .and_then(|l| l.split_whitespace().nth(1).map(str::to_string))
    };
    let (m, c) = (owner("main"), owner("c.go"));
    assert!(
        m.is_some() && c.is_some() && m != c,
        "the split fixture no longer separates main ({m:?}) from c.go ({c:?}):\n{text}"
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
}

/// The one program that needs both readings of `c.llabs` is refused by
/// name on native and release (an ICE before s224); the checked machine
/// and lupin have no C membrane and say so.
#[test]
fn a_c_import_beside_a_module_c_function_of_its_name_is_refused_by_name() {
    let dir = clash();
    for flag in ["--native", "--release"] {
        let Some(r) = wolfgang(&dir, flag) else { continue };
        assert_eq!(r["verdict"], "unsupported", "{flag}: {r}");
        let why = r["x-unsupported-construct"].as_str().unwrap_or("");
        assert!(
            why.contains("shares its name with a function of the module `c`")
                && why.contains("wolf-lang#620"),
            "{flag}: {r}"
        );
    }
    if let Some(r) = wolfgang(&dir, "--checked") {
        assert_eq!(r["verdict"], "unsupported", "--checked: {r}");
    }
    if let Some(l) = require_lupin() {
        assert_eq!(lupin_record(&l, &dir)["verdict"], "unsupported", "lupin");
    }
}
