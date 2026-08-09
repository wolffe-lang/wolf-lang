//! Repo automation (`cargo xtask <command>`). CI-shaped: pure, exit-code
//! driven — the s02 CI workflows are thin wrappers over these commands.

use std::collections::BTreeMap;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1);
    match cmd.as_deref() {
        Some("ci") => ci(),
        Some("deps-check") => deps_check(),
        _ => {
            eprintln!("usage: cargo xtask <ci|deps-check>");
            ExitCode::from(2)
        }
    }
}

/// fmt-check + clippy (deny warnings) + tests. The future CI entry point.
fn ci() -> ExitCode {
    let steps: &[(&str, &[&str])] = &[
        ("fmt", &["fmt", "--all", "--check"]),
        (
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("test", &["test", "--workspace"]),
        ("deps-check", &["xtask", "deps-check"]),
    ];
    for (name, args) in steps {
        eprintln!("== xtask ci: {name}");
        let ok = Command::new("cargo")
            .args(*args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("xtask ci: step `{name}` failed");
            return ExitCode::FAILURE;
        }
    }
    eprintln!("xtask ci: all steps green");
    ExitCode::SUCCESS
}

/// Enforce the locked crate dependency direction (s00): each workspace
/// crate may depend only on the workspace crates in its allowlist.
fn deps_check() -> ExitCode {
    // crate -> workspace crates it MAY depend on. wolf_driver is the top and
    // unrestricted; xtask may not depend on workspace crates at all.
    let allowed: BTreeMap<&str, Option<&[&str]>> = BTreeMap::from([
        ("wolf_span", Some(&[][..])),
        ("wolf_diag", Some(&["wolf_span"][..])),
        ("wolf_lex", Some(&["wolf_span", "wolf_diag"][..])),
        ("wolf_ast", Some(&["wolf_span"][..])),
        (
            "wolf_parse",
            Some(&["wolf_span", "wolf_diag", "wolf_lex", "wolf_ast"][..]),
        ),
        (
            "wolf_sema",
            Some(&["wolf_span", "wolf_diag", "wolf_ast", "wolf_parse"][..]),
        ),
        (
            "wolf_mem",
            Some(&["wolf_span", "wolf_diag", "wolf_ast", "wolf_sema"][..]),
        ),
        (
            "wolf_wir",
            Some(
                &[
                    "wolf_span",
                    "wolf_diag",
                    "wolf_ast",
                    "wolf_sema",
                    "wolf_mem",
                ][..],
            ),
        ),
        (
            "wolf_codegen_clif",
            Some(&["wolf_span", "wolf_diag", "wolf_wir"][..]),
        ),
        (
            "wolf_codegen_llvm",
            Some(&["wolf_span", "wolf_diag", "wolf_wir"][..]),
        ),
        // wolf_rt links into user programs: dependency-thin by law (D15).
        ("wolf_rt", Some(&["wolf_span"][..])),
        ("wolf_driver", None), // top of the graph: unrestricted
        ("xtask", Some(&[][..])),
    ]);

    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("cargo metadata failed to run");
    if !out.status.success() {
        eprintln!("deps-check: cargo metadata failed");
        return ExitCode::FAILURE;
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("cargo metadata: invalid json");

    let mut violations = 0u32;
    for pkg in meta["packages"].as_array().expect("packages") {
        let name = pkg["name"].as_str().expect("name");
        let Some(entry) = allowed.get(name) else {
            eprintln!("deps-check: crate `{name}` has no allowlist entry — add it to xtask");
            violations += 1;
            continue;
        };
        let Some(allow) = entry else { continue };
        for dep in pkg["dependencies"].as_array().expect("deps") {
            let dep_name = dep["name"].as_str().expect("dep name");
            if allowed.contains_key(dep_name) && !allow.contains(&dep_name) {
                eprintln!("deps-check: ILLEGAL EDGE {name} -> {dep_name}");
                violations += 1;
            }
        }
    }
    if violations > 0 {
        eprintln!(
            "deps-check: {violations} violation(s) — the crate graph direction is locked (s00)"
        );
        ExitCode::FAILURE
    } else {
        eprintln!("deps-check: crate graph direction ok");
        ExitCode::SUCCESS
    }
}
