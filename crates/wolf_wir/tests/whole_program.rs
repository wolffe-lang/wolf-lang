//! s43 evidence: the whole-program phase as a unit.
//!
//! - the FROZEN summary format, locked as a reviewed snapshot (c12 and
//!   s45 key on this text; the `impls=[]` slot is D42's devirt
//!   headroom and must keep printing empty),
//! - D8 dedup: 100 instantiations of one shape emit exactly one body,
//!   and the normalization hash is equivalence up to local names only
//!   (alpha-renaming never changes it; any mutation does),
//! - clusters: deterministic, balanced, and a cross-cluster inlining
//!   witness with the imported body spliced in,
//! - conc conservatism across the new boundary: a task-seam carrier is
//!   never imported.
//!
//! Every pipeline run here forces `verify_each`, so a green test is
//! also a pass-manager and fact-custody regression.

use wolf_wir::midend::summary::{Homes, SUMMARY_FORMAT_VERSION, summarize};
use wolf_wir::midend::{Options, Thresholds, cluster, dedup, optimize_whole_program};
use wolf_wir::{parse_module, print_module, print_selected, verify_module};

fn opts() -> Options {
    Options {
        verify_each: true,
        ..Options::default()
    }
}

/// Thresholds that force a multi-cluster partition on small fixtures —
/// the whole point of the knobs living in the ONE table.
fn tiny_clusters() -> Options {
    Options {
        verify_each: true,
        thresholds: Thresholds {
            cluster_target_size: 3,
            ..Thresholds::default()
        },
        ..Options::default()
    }
}

/// Two source modules: `@helper` in `a`, `@main` in `b`.
fn two_module_homes() -> Homes {
    let mut h = Homes::single();
    h.set("helper", "a");
    h.set("main", "b");
    h
}

const TWO_MODULES: &str = "fn @helper(i64) -> i64 {\n\
                           b0(%0: i64):\n  \
                           %1 = iadd.chk %0, %0\n  \
                           ret %1\n\
                           }\n\
                           \n\
                           fn @main() -> i64 {\n\
                           b0:\n  \
                           %0 = iconst.i64 21\n  \
                           %1 = call @helper(%0)\n  \
                           %2 = call @helper(%1)\n  \
                           ret %2\n\
                           }\n";

// ------------------------------------------------------- summaries ----

/// The frozen schema, byte for byte. A change here is a FORMAT change:
/// bump `SUMMARY_FORMAT_VERSION` and tell c12.
#[test]
fn summary_format_is_frozen() {
    let m = parse_module(TWO_MODULES).expect("fixture parses");
    verify_module(&m).expect("verifies");
    let s = summarize(&m, &two_module_homes());
    assert_eq!(s.version, SUMMARY_FORMAT_VERSION);
    let text = s.render();
    // The reserved slots (D42 devirt headroom, s45 hotness) print and
    // stay empty — that is the whole point of freezing them. v2 (s99)
    // adds `ret=`/`stores=`: unproven renders as `-`/`[]`, and a
    // full-type-bounds range IS unproven (normalized at the source).
    // v3 (s117) adds `refs=`: `func.addr` reference edges, empty here
    // (no function in this fixture takes an address).
    assert!(
        text.contains("impls=[]"),
        "devirt headroom present:\n{text}"
    );
    assert!(text.contains("hot=-"), "hotness slot present:\n{text}");
    // Body hashes are content, not position: they must not leak the
    // sha into the snapshot's meaning, so assert their shape and
    // redact them for the review artifact.
    for f in &s.funcs {
        assert_eq!(f.body_hash.len(), 64, "sha256 hex");
    }
    let redacted = redact_hashes(&text);
    assert!(text.contains("refs=[]"), "reference slot present:\n{text}");
    insta::assert_snapshot!("summary_format_v3", redacted);
}

/// The digest is a pure function of the rendered index — the property
/// the driver's cluster cache keys rest on.
#[test]
fn summary_digest_tracks_the_body() {
    let m = parse_module(TWO_MODULES).expect("parses");
    let a = summarize(&m, &two_module_homes()).digest();
    let b = summarize(&m, &two_module_homes()).digest();
    assert_eq!(a, b, "deterministic");
    let edited = parse_module(&TWO_MODULES.replace("iconst.i64 21", "iconst.i64 22"))
        .expect("edited parses");
    assert_ne!(
        a,
        summarize(&edited, &two_module_homes()).digest(),
        "a body edit moves the summary digest"
    );
}

/// Replace every 64-hex-digit run with `<hash>` so snapshots review
/// structure, not sha values.
fn redact_hashes(s: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.len() == 64 {
            out.push_str("<hash>");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in s.chars() {
        if c.is_ascii_hexdigit() {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

// ------------------------------------------------------------ D8 ----

/// A hundred instantiations of one shape emit exactly ONE body
/// (contract acceptance, snapshot-locked). The duplicates die at the
/// WIR level — LLVM never sees them, which is the structural immunity
/// to the IR-volume disease.
#[test]
fn a_hundred_instantiations_emit_one_body() {
    let mut src = String::new();
    for i in 0..100 {
        src.push_str(&format!(
            "fn @inst{i:02}(i64) -> i64 {{\nb0(%0: i64):\n  %1 = iadd.chk %0, %0\n  ret %1\n}}\n\n"
        ));
    }
    src.push_str("fn @main() -> i64 {\nb0:\n  %0 = iconst.i64 1\n");
    for i in 0..100 {
        src.push_str(&format!("  %{} = call @inst{i:02}(%0)\n", i + 1));
    }
    src.push_str("  ret %100\n}\n");
    let mut m = parse_module(&src).expect("fixture parses");
    verify_module(&m).expect("verifies");
    let stats = dedup::dedup(&mut m);
    assert_eq!(stats.bodies_seen, 101, "100 instantiations + main");
    assert_eq!(stats.bodies_merged, 99, "99 duplicates merged: {stats:?}");
    assert_eq!(m.funcs.len(), 2, "one shape + main survive");
    assert_eq!(
        stats.sites_retargeted, 99,
        "every retired call site retargeted: {stats:?}"
    );
    verify_module(&m).expect("dedup leaves a verified module");
    let out = print_module(&m);
    assert_eq!(
        out.matches("iadd.chk").count(),
        1,
        "exactly one body:\n{out}"
    );
    assert_eq!(out.matches("call @inst00").count(), 100);
    insta::assert_snapshot!(
        "dedup_one_shape",
        print_selected(&m, &[m.funcs.keys().next().expect("a func")])
    );
}

/// Exported and address-taken bodies keep their identity: the linker
/// symbol must exist, and two task entries must not compare equal.
#[test]
fn dedup_never_merges_pinned_bodies() {
    let src = "export fn @a(i64) -> i64 {\n\
               b0(%0: i64):\n  %1 = iadd.chk %0, %0\n  ret %1\n}\n\
               \n\
               export fn @b(i64) -> i64 {\n\
               b0(%0: i64):\n  %1 = iadd.chk %0, %0\n  ret %1\n}\n";
    let mut m = parse_module(src).expect("parses");
    let stats = dedup::dedup(&mut m);
    assert_eq!(stats.bodies_merged, 0, "exports keep their symbols");
    assert_eq!(m.funcs.len(), 2);
}

/// The I11 rule as a property over deterministic seeds: alpha-renaming
/// locals (and blocks) never changes the content hash; mutating any
/// instruction always does.
#[test]
fn normalization_is_alpha_equivalence_only() {
    for seed in 0..64u64 {
        let src = gen_body(seed);
        let m = parse_module(&src).expect("generated body parses");
        verify_module(&m).unwrap_or_else(|e| panic!("seed {seed} verifies: {e}"));
        let f = m.funcs.values().next().expect("one function");
        let base = dedup::body_hash(&m, f);

        let renamed = alpha_rename(&src);
        assert_ne!(renamed, src, "the rename is not a no-op (seed {seed})");
        let rm = parse_module(&renamed).unwrap_or_else(|e| panic!("seed {seed} renamed: {e:?}"));
        let rf = rm.funcs.values().next().expect("one function");
        assert_eq!(
            base,
            dedup::body_hash(&rm, rf),
            "alpha-renaming must not move the hash (seed {seed})"
        );

        // Mutation: one operand of one instruction changes.
        let mutated = src.replacen("iadd.chk", "isub.chk", 1);
        if mutated != src {
            let mm = parse_module(&mutated).expect("mutated parses");
            let mf = mm.funcs.values().next().expect("one function");
            assert_ne!(
                base,
                dedup::body_hash(&mm, mf),
                "an instruction change must move the hash (seed {seed})"
            );
        }
    }
}

/// Deterministic straight-line bodies over a fixed op menu — the same
/// seeded-generator discipline the round-trip property uses (no flaky
/// randomness in CI).
fn gen_body(seed: u64) -> String {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        x = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        x
    };
    let ops = ["iadd.chk", "isub.chk", "imul.chk", "band", "bor", "bxor"];
    let n = 3 + (seed % 6) as usize;
    let mut s = String::from("fn @f(i64) -> i64 {\nb0(%0: i64):\n");
    for i in 0..n {
        let op = ops[(next() % ops.len() as u64) as usize];
        let a = next() % (i as u64 + 1);
        let b = next() % (i as u64 + 1);
        s.push_str(&format!("  %{} = {op} %{a}, %{b}\n", i + 1));
    }
    s.push_str(&format!("  ret %{n}\n}}\n"));
    s
}

/// Shift every local name past the ones in use — a pure renaming, the
/// only equivalence I11 licenses.
fn alpha_rename(src: &str) -> String {
    let mut out = String::new();
    let bytes: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '%' {
            let mut j = i + 1;
            let mut num = String::new();
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                num.push(bytes[j]);
                j += 1;
            }
            if !num.is_empty() {
                let v: u32 = num.parse().expect("digits");
                out.push_str(&format!("%{}", v + 500));
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

// ------------------------------------------------------- clusters ----

/// Partitioning is a pure function of the deduped graph: same input,
/// same clusters, every time — and the members are content-derived,
/// never arena-order-derived.
#[test]
fn clustering_is_deterministic() {
    let m = parse_module(TWO_MODULES).expect("parses");
    let s = summarize(&m, &two_module_homes());
    let th = tiny_clusters().thresholds;
    let a = cluster::partition(&s, &th);
    let b = cluster::partition(&s, &th);
    assert_eq!(a, b, "cluster assignment is deterministic");
    assert!(a.len() > 1, "the tiny target splits this fixture: {a:?}");
    let members: usize = a.iter().map(|c| c.members.len()).sum();
    assert_eq!(members, s.funcs.len(), "every function lands exactly once");
}

/// The cross-cluster inlining WITNESS (contract acceptance): a hot
/// cross-module call whose callee lives in another cluster is
/// imported by the summary-driven decision and inlined — snapshot of
/// the cluster's pre-LLVM WIR.
#[test]
fn cross_cluster_inline_witness() {
    let mut m = parse_module(TWO_MODULES).expect("parses");
    let wp = optimize_whole_program(&mut m, &two_module_homes(), &tiny_clusters())
        .expect("whole-program pipeline green");
    assert!(
        wp.clusters.len() > 1,
        "the fixture partitions: {:?}",
        wp.clusters
    );
    assert!(
        wp.stats.imports >= 1,
        "the import decision brought a body across: {:?}",
        wp.clusters
    );
    assert!(
        wp.stats.opt.cross_cluster_inlined >= 1,
        "and the inliner used it: {}",
        wp.stats
    );
    assert!(
        wp.stats.opt.cross_module_inlined >= 1,
        "which is also a cross-module inline: {}",
        wp.stats
    );
    let out = print_module(&m);
    assert!(!out.contains("call @helper"), "spliced in:\n{out}");
    assert!(out.contains("iconst.i64 84"), "and folded through:\n{out}");
    insta::assert_snapshot!("cross_cluster_inline_witness", out);
}

/// The module phase's horizon is real: with one cluster per function
/// and imports switched off by an exhausted budget, nothing crosses.
#[test]
fn module_phase_does_not_inline_across_modules() {
    let mut m = parse_module(TWO_MODULES).expect("parses");
    let o = Options {
        verify_each: true,
        thresholds: Thresholds {
            cluster_target_size: 3,
            import_budget: 0,
            ..Thresholds::default()
        },
        ..Options::default()
    };
    let wp = optimize_whole_program(&mut m, &two_module_homes(), &o).expect("green");
    assert_eq!(wp.stats.imports, 0, "no import budget, no import");
    assert_eq!(wp.stats.opt.cross_cluster_inlined, 0);
    let out = print_module(&m);
    assert!(
        out.contains("call @helper"),
        "the boundary held without an import:\n{out}"
    );
}

/// One module, one cluster: the whole-program phase degenerates to the
/// s42 pipeline (plus dedup) and still verifies — the fixture/library
/// shape every unit test takes.
#[test]
fn single_module_degenerates_cleanly() {
    let mut m = parse_module(TWO_MODULES).expect("parses");
    let wp = optimize_whole_program(&mut m, &Homes::single(), &opts()).expect("green");
    assert_eq!(wp.clusters.len(), 1);
    assert_eq!(wp.stats.modules, 1);
    assert_eq!(
        wp.stats.opt.cross_module_inlined, 0,
        "one module: nothing crosses"
    );
    verify_module(&m).expect("verified");
}

// ------------------------------------------- conc conservatism ----

/// A task-seam carrier is NEVER imported across a cluster boundary
/// (s42's conc conservatism, restated at the whole-program layer): a
/// spawn edge is a call, calls are opaque, and the whole-program phase
/// does not get to forget that because it drew a partition.
#[test]
fn task_seam_carriers_are_never_imported() {
    let src = "decl @__wolf_rt_scope_spawn(ptr) -> i64\n\
               \n\
               fn @body() -> i64 {\n\
               b0:\n  \
               %0 = iconst.i64 7\n  \
               ret %0\n\
               }\n\
               \n\
               fn @spawner(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = func.addr @body()\n  \
               %2 = call @__wolf_rt_scope_spawn(%1)\n  \
               %3 = iadd.chk %0, %2\n  \
               ret %3\n\
               }\n\
               \n\
               fn @main() -> i64 {\n\
               b0:\n  \
               %0 = iconst.i64 1\n  \
               %1 = call @spawner(%0)\n  \
               ret %1\n\
               }\n";
    let m = parse_module(src).expect("fixture parses");
    verify_module(&m).expect("verifies");
    let mut homes = Homes::single();
    homes.set("spawner", "conc");
    homes.set("body", "conc");
    homes.set("main", "root");
    let s = summarize(&m, &homes);
    let spawner = s.get("spawner").expect("summarized");
    assert!(spawner.flags.task_seam, "the seam is flagged: {spawner:?}");
    assert!(
        s.get("body").expect("summarized").flags.address_taken,
        "a task entry is address-taken, so dedup pins it too"
    );
    let th = tiny_clusters().thresholds;
    let clusters = cluster::decide_imports(&cluster::partition(&s, &th), &s, &th);
    assert!(
        clusters
            .iter()
            .all(|c| !c.imports.iter().any(|i| i == "spawner")),
        "a task-seam carrier is never imported: {clusters:?}"
    );

    // s117 (#136): the reference edge is in the summary and the
    // partition honors it — the task entry lands in its spawner's
    // cluster, never in whichever bin was lightest.
    assert_eq!(
        spawner.refs,
        vec!["body".to_string()],
        "the `func.addr` reference is a summary edge: {spawner:?}"
    );
    let clusters2 = cluster::partition(&s, &th);
    let of = |name: &str| {
        clusters2
            .iter()
            .position(|c| c.members.iter().any(|m| m == name))
            .unwrap_or_else(|| panic!("`{name}` clustered"))
    };
    assert_eq!(
        of("spawner"),
        of("body"),
        "the shim travels with its spawner: {clusters2:?}"
    );

    let mut m2 = parse_module(src).expect("parses");
    let wp = optimize_whole_program(&mut m2, &homes, &tiny_clusters()).expect("green");
    let out = print_module(&m2);
    assert_eq!(
        out.matches("call @__wolf_rt_scope_spawn").count(),
        1,
        "the seam is neither duplicated nor dropped:\n{out}"
    );
    assert!(
        out.contains("call @spawner"),
        "and the carrier itself stayed put across the boundary:\n{out}"
    );
    assert!(
        out.contains("func.addr @body"),
        "the task entry survives as a link-time constant:\n{out}"
    );
    assert_eq!(
        wp.stats.dedup.bodies_merged, 0,
        "nothing merged behind the seam"
    );
}

// ------------------------------------------- s117: func.addr edges ----

/// The #136 mechanism, reduced (s117): a task shim is reached by
/// `func.addr`, not by call, so pre-fix the partitioner saw no edge
/// into it and bin-packed the shim away from its spawner — and the
/// release emitter then refused `func.addr` of a symbol outside its
/// own object, on a program the debug tier runs. The wsm01 twist this
/// fixture pins: the spawner is ALREADY over `cluster_target_size`, so
/// an ordinary affinity edge would be refused by the size cap — the
/// reference fuse must be exempt from it.
#[test]
fn func_addr_referee_travels_with_its_referrer() {
    let src = "decl @__wolf_rt_scope_spawn(ptr) -> i64\n\
               \n\
               fn @entry() -> i64 {\n\
               b0:\n  \
               %0 = iconst.i64 7\n  \
               ret %0\n\
               }\n\
               \n\
               fn @spawner(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = func.addr @entry()\n  \
               %2 = call @__wolf_rt_scope_spawn(%1)\n  \
               %3 = iadd.chk %0, %2\n  \
               %4 = iadd.chk %3, %0\n  \
               %5 = iadd.chk %4, %0\n  \
               ret %5\n\
               }\n\
               \n\
               fn @filler_a(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iadd.chk %0, %0\n  \
               %2 = iadd.chk %1, %0\n  \
               %3 = iadd.chk %2, %0\n  \
               ret %3\n\
               }\n\
               \n\
               fn @filler_b(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = imul.chk %0, %0\n  \
               %2 = imul.chk %1, %0\n  \
               %3 = imul.chk %2, %0\n  \
               ret %3\n\
               }\n";
    let m = parse_module(src).expect("fixture parses");
    verify_module(&m).expect("verifies");
    let s = summarize(&m, &Homes::single());
    let th = tiny_clusters().thresholds;
    let spawner = s.get("spawner").expect("summarized");
    assert_eq!(
        spawner.refs,
        vec!["entry".to_string()],
        "the reference edge is in the summary: {spawner:?}"
    );
    assert!(
        spawner.calls.iter().all(|c| c.callee != "entry"),
        "a reference is NOT a call edge (no site for the import \
         ranking or the inline budget): {spawner:?}"
    );
    // The wsm01 precondition holds: the spawner alone busts the cap,
    // so only a cap-exempt fuse can keep the pair together.
    assert!(
        spawner.size > th.cluster_target_size,
        "the spawner is over the target size ({} > {})",
        spawner.size,
        th.cluster_target_size
    );
    let clusters = cluster::partition(&s, &th);
    assert!(
        clusters.len() > 1,
        "the fixture partitions (no collapse): {clusters:?}"
    );
    let of = |name: &str| {
        clusters
            .iter()
            .position(|c| c.members.iter().any(|m| m == name))
            .unwrap_or_else(|| panic!("`{name}` clustered"))
    };
    assert_eq!(
        of("spawner"),
        of("entry"),
        "the shim travels with its spawner: {clusters:?}"
    );
    let members: usize = clusters.iter().map(|c| c.members.len()).sum();
    assert_eq!(members, s.funcs.len(), "every function lands exactly once");
    assert_eq!(
        clusters,
        cluster::partition(&s, &th),
        "and the partition is still deterministic"
    );
}

/// The same class, one constructor over (s105): a closure entry is a
/// `func.addr` referee exactly like a task shim — no runtime seam in
/// sight — and rides the same summary edge. The end-to-end pipeline
/// keeps the pair co-resident and the module verifies.
#[test]
fn closure_entry_rides_the_same_reference_edge() {
    let src = "fn @cls_entry(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = iadd.chk %0, %0\n  \
               ret %1\n\
               }\n\
               \n\
               fn @maker(i64) -> ptr {\n\
               b0(%0: i64):\n  \
               %1 = func.addr @cls_entry()\n  \
               %2 = iadd.chk %0, %0\n  \
               %3 = iadd.chk %2, %0\n  \
               %4 = iadd.chk %3, %0\n  \
               ret %1\n\
               }\n\
               \n\
               fn @other(i64) -> i64 {\n\
               b0(%0: i64):\n  \
               %1 = imul.chk %0, %0\n  \
               %2 = imul.chk %1, %0\n  \
               %3 = imul.chk %2, %0\n  \
               ret %3\n\
               }\n";
    let m = parse_module(src).expect("fixture parses");
    verify_module(&m).expect("verifies");
    let s = summarize(&m, &Homes::single());
    let maker = s.get("maker").expect("summarized");
    assert_eq!(maker.refs, vec!["cls_entry".to_string()], "{maker:?}");
    assert!(
        s.get("cls_entry").expect("summarized").flags.address_taken,
        "a closure entry is address-taken like a task entry"
    );
    let th = tiny_clusters().thresholds;
    let clusters = cluster::partition(&s, &th);
    assert!(clusters.len() > 1, "partitions: {clusters:?}");
    let of = |name: &str| {
        clusters
            .iter()
            .position(|c| c.members.iter().any(|m| m == name))
            .unwrap_or_else(|| panic!("`{name}` clustered"))
    };
    assert_eq!(
        of("maker"),
        of("cls_entry"),
        "the closure entry travels with its maker: {clusters:?}"
    );

    let mut m2 = parse_module(src).expect("parses");
    optimize_whole_program(&mut m2, &Homes::single(), &tiny_clusters()).expect("green");
    verify_module(&m2).expect("still verifies");
}
