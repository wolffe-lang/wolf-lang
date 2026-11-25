//! Count the retained source-description allocations, not time or process RSS.
//! A small source DAG has a constant executable shape but many debug bindings.

use std::collections::HashSet;
use std::sync::Arc;

use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};
use wolf_wir::debug::{DebugKind, DebugType};
use wolf_wir::{Build, lower_package, print_module, verify_module};

fn source(depth: usize, functions: usize, bindings: usize) -> String {
    let mut text = String::from("struct N0 { value: int }\n");
    for i in 1..=depth {
        text.push_str(&format!(
            "struct N{i} {{ left: *N{}, right: *N{} }}\n",
            i - 1,
            i - 1
        ));
    }
    for function in 0..functions {
        text.push_str(&format!("fn probe{function}(seed: *N{depth}) -> int {{\n"));
        for binding in 0..bindings {
            text.push_str(&format!("let b{binding} = seed\n"));
        }
        text.push_str("0\n}\n");
    }
    text.push_str("fn main() -> int { 0 }\n");
    text
}

fn lower(text: &str) -> Build {
    let mut loader = MemoryLoader::new("debug_memory");
    loader.add_file(&[], "main.lu", text);
    let resolved = resolve_package_with(&mut loader, &AliasTable::default(), true)
        .expect("source package resolves");
    assert!(
        resolved
            .diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "resolution diagnostics: {:?}",
        resolved.diagnostics
    );
    let checked = typecheck_package_with(&resolved.package, true);
    assert!(checked.not_yet.is_empty(), "fully typed source");
    assert!(!checked.has_errors(), "{:?}", checked.diagnostics);
    let memory = wolf_mem::check_package(&resolved.package, &checked);
    assert!(memory.not_yet.is_empty(), "fully checked memory surface");
    assert!(
        memory
            .diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "memory diagnostics: {:?}",
        memory.diagnostics
    );
    let build = lower_package(&resolved.package, &checked);
    assert!(build.not_yet.is_empty(), "{:?}", build.not_yet);
    verify_module(&build.module).expect("lowered module verifies");
    build
}

/// Require all the named bindings and their complete source types to survive.
/// Omitting metadata, or counting only a subset of the functions, cannot pass.
fn roots(build: &Build, functions: usize, bindings: usize) -> Vec<&DebugType> {
    let mut roots = Vec::new();
    for function in 0..functions {
        let name = format!("probe{function}");
        let matches: Vec<_> = build
            .module
            .funcs
            .values()
            .filter(|f| f.name == name)
            .collect();
        assert_eq!(matches.len(), 1, "exactly one {name}");
        let function = matches[0];
        assert_eq!(function.debug_vars.len(), bindings + 1, "{name}");
        assert_eq!(function.debug_vars[0].name, "seed");
        assert!(function.debug_vars[0].param);
        for (i, binding) in function.debug_vars.iter().enumerate() {
            if i > 0 {
                assert_eq!(binding.name, format!("b{}", i - 1));
                assert!(!binding.param);
            }
            let ty = binding
                .ty
                .as_ref()
                .expect("binding retains its source type");
            assert!(matches!(&ty.kind, DebugKind::Pointer(_)));
            roots.push(ty);
        }
    }
    assert_eq!(roots.len(), functions * (bindings + 1));
    for ty in &roots[1..] {
        assert_eq!(
            *ty, roots[0],
            "equivalent roots have identical descriptions"
        );
    }
    roots
}

/// Arc identity is used only to count allocations, never to define type equality.
/// Keep the Build alive throughout the walk so no address can be recycled.
fn retained_nodes(roots: &[&DebugType]) -> usize {
    let mut seen = HashSet::new();
    let mut pending = roots.to_vec();
    while let Some(ty) = pending.pop() {
        if !seen.insert(Arc::as_ptr(ty)) {
            continue;
        }
        match &ty.kind {
            DebugKind::Pointer(to) | DebugKind::Reference(to) => pending.push(to),
            DebugKind::Struct(fields) => pending.extend(fields.iter().map(|field| &field.ty)),
            DebugKind::Variants(variants) => {
                for variant in variants {
                    pending.extend(variant.members.iter().map(|member| &member.ty));
                }
            }
            DebugKind::Base(_) | DebugKind::Enum(_) | DebugKind::Opaque => {}
        }
    }
    assert!(
        !seen.is_empty(),
        "metadata cannot disappear to satisfy the budget"
    );
    seen.len()
}

fn budget(row: &str, actual: usize, limit: usize) {
    if actual > limit {
        eprintln!("FAIL {row}");
        panic!("{row}: retained {actual} description allocations, budget {limit}");
    }
    println!("PASS {row} retained_nodes={actual} budget={limit}");
}

#[test]
fn repeated_bindings_share_source_description_allocations() {
    let one = lower(&source(7, 1, 1));
    let many = lower(&source(7, 1, 64));
    assert_eq!(
        print_module(&one.module),
        print_module(&many.module),
        "extra bindings do not add executable WIR"
    );
    let one_roots = roots(&one, 1, 1);
    let many_roots = roots(&many, 1, 64);
    assert_eq!(one_roots[0], many_roots[0]);
    budget(
        "debug-description-retained-nodes",
        retained_nodes(&many_roots),
        2 * retained_nodes(&one_roots) + 32,
    );
}

#[test]
fn source_description_sharing_crosses_body_tables() {
    let one = lower(&source(7, 1, 1));
    let many = lower(&source(7, 8, 8));
    let one_roots = roots(&one, 1, 1);
    let many_roots = roots(&many, 8, 8);
    assert_eq!(one_roots[0], many_roots[0]);
    budget(
        "debug-description-unit-nodes",
        retained_nodes(&many_roots),
        2 * retained_nodes(&one_roots) + 32,
    );
}

#[test]
fn reassignments_keep_shared_source_descriptions() {
    fn reassigned(assignments: usize) -> Build {
        let mut body = String::from("var state = seed\n");
        for _ in 0..assignments {
            body.push_str("state = seed\n");
        }
        let marker = "fn probe0(seed: *N7) -> int {\n";
        let text = source(7, 1, 0);
        assert!(text.contains(marker));
        lower(&text.replacen(marker, &format!("{marker}{body}"), 1))
    }
    fn assignment_roots(build: &Build, assignments: usize) -> Vec<&DebugType> {
        let function = build
            .module
            .funcs
            .values()
            .find(|f| f.name == "probe0")
            .expect("probe0 lowered");
        assert_eq!(function.debug_vars.len(), assignments + 2);
        let mut roots = Vec::new();
        for (index, binding) in function.debug_vars.iter().enumerate() {
            assert_eq!(binding.name, if index == 0 { "seed" } else { "state" });
            assert_eq!(binding.param, index == 0);
            let ty = binding.ty.as_ref().expect("assignment retains source type");
            assert!(matches!(&ty.kind, DebugKind::Pointer(_)));
            roots.push(ty);
        }
        for ty in &roots[1..] {
            assert_eq!(*ty, roots[0], "assignment does not change the source type");
        }
        roots
    }
    let initial = reassigned(0);
    let repeated = reassigned(64);
    assert_eq!(
        print_module(&initial.module),
        print_module(&repeated.module)
    );
    let initial_roots = assignment_roots(&initial, 0);
    let repeated_roots = assignment_roots(&repeated, 64);
    assert_eq!(initial_roots[0], repeated_roots[0]);
    // add_debug_var already reuses the latest same-name/same-shape type. A new
    // annotation must not replace that sharing with a separately allocated tree.
    budget(
        "debug-description-reassignment-nodes",
        retained_nodes(&repeated_roots),
        2 * retained_nodes(&initial_roots) + 32,
    );
}

#[test]
fn source_dag_does_not_expand_into_a_description_tree() {
    for depth in [5, 7] {
        let build = lower(&source(depth, 1, 1));
        budget(
            "debug-description-dag-nodes",
            retained_nodes(&roots(&build, 1, 1)),
            4 * (depth + 1) + 32,
        );
    }
}
