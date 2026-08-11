//! Rendered snapshots for the E11xx concurrency family (spec 03):
//! the capture law (E1101, [conc.task.spawn]), channel sendability
//! (E1102, [conc.chan.type]), and the `when` nesting ban (E1103,
//! [conc.when.nonest]) — plus the clean twins that pin what the conc
//! surface typing accepts. Reviewed artifacts, one per shape (D22).

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn render_types(files: &[(&[&str], &str, &str)]) -> String {
    let mut ml = MemoryLoader::new("snap");
    for (m, n, s) in files {
        ml.add_file(m, n, s);
    }
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "snapshot inputs resolve without errors: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty(),
        "fixtures check fully: {:?}",
        tc.not_yet
    );
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    // Resolve-rung warnings (the W11xx advisory tier) render alongside
    // the typecheck rung's errors — the coexistence is the artifact.
    let mut all: Vec<wolf_diag::Diagnostic> = res
        .diagnostics
        .iter()
        .chain(tc.diagnostics.iter())
        .cloned()
        .collect();
    wolf_diag::sort_diagnostics(&mut all);
    let mut out = String::new();
    for d in &all {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    if out.is_empty() {
        out.push_str("(clean)");
    }
    out
}

fn snap_one(name: &str, src: &str) {
    insta::assert_snapshot!(name, render_types(&[(&[], "main.lu", src)]));
}

// ---------------------------------------------------------- E1101 -----

/// The book's ex13-3 shape: one task, one captured `var`, one lost
/// increment. E1101 with the three-fixes teach; W1101 (the advisory
/// tier, the shape short of the error) rides along by design — the
/// corpus witness `conc/store_buffer.lu` declares both.
#[test]
fn e1101_task_capture_write() {
    snap_one(
        "e1101_task_capture_write",
        "fn main() -> !int {\n    var hits = 0\n    scope s {\n        \
         s.spawn(fn() { hits += 1 })\n    }\n    hits\n}\n",
    );
}

/// A write through a projection of a captured binding — the coverage
/// W1101 explicitly cedes to E1101 (`wave.rs`: "writes through a
/// projection: E1101's turf").
#[test]
fn e1101_projection_write() {
    snap_one(
        "e1101_projection_write",
        "struct Tally {\n    count: int,\n}\n\n\
         fn main() -> !int {\n    var t = Tally { count: 0 }\n    scope s {\n        \
         s.spawn(fn() { t.count = 1 })\n    }\n    t.count\n}\n",
    );
}

/// The sync-type carve-out and the `when`-payload rebind: writes to
/// Mutex payloads inside a `when` body are synchronized, not capture
/// writes — the clean twin ([conc.when.body], `conc/when_multi.lu`).
#[test]
fn e1101_clean_when_guarded() {
    snap_one(
        "e1101_clean_when_guarded",
        "fn main() -> !int {\n    let a = Mutex(1)\n    let b = Mutex(2)\n    scope s {\n        \
         s.spawn(fn() {\n            when (a, b) { a += 10; b += 10 }\n        })\n    }\n    \
         when (a, b) { a + b }\n}\n",
    );
}

// ---------------------------------------------------------- E1102 -----

/// A bare `List` cannot cross a channel: none of `Copy`, `imm`,
/// region, `sync` — the D14-verbs teach ([conc.chan.type]).
#[test]
fn e1102_unsendable_payload() {
    snap_one(
        "e1102_unsendable_payload",
        "fn main() -> !int {\n    let ch = channel[List[int]](1)\n    0\n}\n",
    );
}

/// The sendable shapes type clean: `Copy` scalars, `str`, region
/// values, sync types — and `channel[int]()` with no capacity is the
/// rendezvous default ([conc.chan.default]).
#[test]
fn e1102_clean_sendable_payloads() {
    snap_one(
        "e1102_clean_sendable_payloads",
        "fn main() -> !int {\n    let a = channel[int](1)\n    let b = channel[str](2)\n    \
         let c = channel[region](1)\n    let d = channel[int]()\n    a.send(1)\n    \
         let v = a.recv() else |_| { return 1 }\n    v\n}\n",
    );
}

// ---------------------------------------------------------- E1103 -----

/// A `when` lexically inside a `when` body: incremental acquisition
/// by another spelling ([conc.when.nonest]) — the merge-the-sets fix.
#[test]
fn e1103_nested_when() {
    snap_one(
        "e1103_nested_when",
        "fn main() -> !int {\n    let a = Mutex(1)\n    let b = Mutex(2)\n    \
         let c = Mutex(3)\n    let d = Mutex(4)\n    var total = 0\n    \
         when (a, b) {\n        when (c, d) {\n            total = a + b + c + d\n        }\n    }\n    \
         total\n}\n",
    );
}

/// The nesting ban is *lexical* ([conc.when.nonest]'s word): a `when`
/// inside a closure written inside a `when` body is still the
/// rejection — the closure does not launder the nesting.
#[test]
fn e1103_nested_through_closure() {
    snap_one(
        "e1103_nested_through_closure",
        "fn main() -> !int {\n    let a = Mutex(1)\n    let b = Mutex(2)\n    \
         let c = Mutex(3)\n    let d = Mutex(4)\n    \
         when (a, b) {\n        scope s {\n            s.spawn(fn() {\n                \
         when (c, d) { c + d }\n            })\n        }\n    }\n    0\n}\n",
    );
}

/// Sibling `when` blocks do not nest — sequential whole-set
/// acquisitions are the sanctioned shape (the clean twin).
#[test]
fn e1103_clean_sequential_whens() {
    snap_one(
        "e1103_clean_sequential_whens",
        "fn main() -> !int {\n    let a = Mutex(1)\n    let b = Mutex(2)\n    var total = 0\n    \
         when (a, b) { total = a + b }\n    when (b, a) { total = total + a + b }\n    total\n}\n",
    );
}
