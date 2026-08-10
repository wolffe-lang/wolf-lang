//! Snapshot fixtures for every s18 diagnostic (the s10 reviewed-
//! artifact rule): E1001 (whole-value and field-granular
//! use-after-move, use through a pending defer), E1002 (call-surface
//! exclusivity: prefix overlap, read-while-mut, take-while-mut),
//! E1007 (call-site mode agreement: missing, extra, wrong), E1008
//! (view-set footprint), E1009 (`mut` needs a place) — plus the
//! conforming shapes that must stay silent.

use wolf_diag::{RenderOptions, Sources, render_human};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

fn render_mem(src: &str) -> String {
    let mut ml = MemoryLoader::new("snap");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics.is_empty(),
        "snapshot inputs resolve clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty(),
        "snapshot inputs typecheck fully: {:?}",
        tc.not_yet
    );
    assert!(
        !tc.has_errors(),
        "snapshot inputs typecheck clean: {:?}",
        tc.diagnostics
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(
        mem.not_yet.is_empty(),
        "snapshot inputs stay inside the s18 surface: {:?}",
        mem.not_yet
    );
    let mut sources = Sources::new();
    for u in &res.package.files {
        sources.add(u.raw.file, u.raw.display.clone(), &u.raw.src);
    }
    let mut out = String::new();
    for d in &mem.diagnostics {
        out.push_str(&render_human(d, &sources, &RenderOptions::default()));
        out.push('\n');
    }
    if out.is_empty() {
        out.push_str("(clean)\n");
    }
    out
}

fn snap(name: &str, src: &str) {
    insta::assert_snapshot!(name, render_mem(src));
}

// ------------------------------------------------------------ E1001 ----

#[test]
fn e1001_whole_value_move() {
    snap(
        "e1001_whole_value",
        "struct Big { data: int }\n\
         fn consume(take b: Big) -> int { b.data }\n\
         fn main() -> !int {\n    \
             let b = Big { data: 2 }\n    \
             let n = consume(take b)\n    \
             let m = b.data\n    \
             n + m\n\
         }\n",
    );
}

#[test]
fn e1001_field_granular_partial_move() {
    // p.x moves; p.y stays live; p.x.n is the error; whole-p is too.
    snap(
        "e1001_partial_move",
        "struct Inner { n: int }\n\
         struct P { x: Inner, y: Inner }\n\
         fn eat(take i: Inner) -> int { i.n }\n\
         fn main() -> !int {\n    \
             var p = P { x: Inner { n: 1 }, y: Inner { n: 2 } }\n    \
             let a = eat(take p.x)\n    \
             let b = p.y.n\n    \
             let c = p.x.n\n    \
             a + b + c\n\
         }\n",
    );
}

#[test]
fn e1001_reinit_revives_and_partial_reinit_leaves_residue() {
    // Re-initializing p.x after moving all of p revives p.x only:
    // p.y is still gone, and so is whole-p use.
    snap(
        "e1001_partial_reinit_residue",
        "struct Inner { n: int }\n\
         struct P { x: Inner, y: Inner }\n\
         fn eat(take p: P) -> int { p.x.n }\n\
         fn main() -> !int {\n    \
             var p = P { x: Inner { n: 1 }, y: Inner { n: 2 } }\n    \
             let a = eat(take p)\n    \
             p.x = Inner { n: 3 }\n    \
             let b = p.x.n\n    \
             let c = p.y.n\n    \
             a + b + c\n\
         }\n",
    );
}

#[test]
fn e1001_use_in_defer_after_move() {
    // The defer's read happens at scope exit — after the move.
    snap(
        "e1001_defer_capture",
        "struct Big { data: int }\n\
         fn consume(take b: Big) -> int { b.data }\n\
         fn main() -> !int {\n    \
             let b = Big { data: 2 }\n    \
             defer consume(take b)\n    \
             let n = consume(take b)\n    \
             n\n\
         }\n",
    );
}

#[test]
fn e1001_branch_sensitive_move() {
    // Moved on one path only: the join still knows (may-analysis).
    snap(
        "e1001_branchy_move",
        "struct Big { data: int }\n\
         fn consume(take b: Big) -> int { b.data }\n\
         fn main() -> !int {\n    \
             let b = Big { data: 2 }\n    \
             var n = 0\n    \
             if b.data == 2 { n = consume(take b) } else { n = 0 }\n    \
             let m = b.data\n    \
             n + m\n\
         }\n",
    );
}

#[test]
fn moves_ok_stays_silent() {
    // Move, explicit copy, re-initialization: all conforming.
    snap(
        "clean_move_copy_reinit",
        "struct Big { data: int }\n\
         fn consume(take b: Big) -> int { b.data }\n\
         fn main() -> !int {\n    \
             var b = Big { data: 1 }\n    \
             let n = consume(take b)\n    \
             b = Big { data: 0 }\n    \
             let c = copy b\n    \
             if n == 1 && c.data == 0 { 0 } else { 1 }\n\
         }\n",
    );
}

// ------------------------------------------------------------ E1002 ----

#[test]
fn e1002_prefix_overlap_mut_mut() {
    snap(
        "e1002_prefix_mut_mut",
        "struct P { x: int, y: int }\n\
         fn two(mut a: P, mut b: int) { a.y += b; b += 1 }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2 }\n    \
             two(mut p, mut p.x)\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1002_read_while_mut() {
    // A non-Copy read argument is lent for the whole call while a
    // conflicting mut runs.
    snap(
        "e1002_read_while_mut",
        "struct P { x: int, y: int }\n\
         fn mix(mut a: P, b: P) { a.x += b.y }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2 }\n    \
             mix(mut p, p)\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1002_take_while_mut() {
    snap(
        "e1002_take_while_mut",
        "struct P { x: int, y: int }\n\
         fn grab(mut a: P, take b: P) -> int { a.x += 1; b.y }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2 }\n    \
             let n = grab(mut p, take p)\n    \
             n\n\
         }\n",
    );
}

#[test]
fn e1002_disjoint_fields_stay_silent() {
    snap(
        "clean_disjoint_mut",
        "struct P { x: int, y: int, z: int }\n\
         fn bump(mut a: int, mut b: int) { a += 1; b += 1 }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2, z: 3 }\n    \
             bump(mut p.x, mut p.y)\n    \
             bump(mut p.y, mut p.z)\n    \
             if p.x == 2 && p.y == 4 && p.z == 4 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn two_phase_copy_read_into_mut_call_stays_silent() {
    // The xs.push(xs.len) shape: the Copy read completes at argument
    // evaluation, before the mut receiver activates.
    snap(
        "clean_two_phase_shape",
        "struct V { x: int, z: int }\n\
         impl V {\n    \
             fn set_x(mut self.{x}, n: int) { self.x = n }\n\
         }\n\
         fn main() -> !int {\n    \
             var v = V { x: 1, z: 41 }\n    \
             (mut v).set_x(v.z)\n    \
             if v.x == 41 { 0 } else { 1 }\n\
         }\n",
    );
}

// ------------------------------------------------------------ E1007 ----

#[test]
fn e1007_missing_mut() {
    snap(
        "e1007_missing_mut",
        "fn bump(mut n: int) { n += 1 }\n\
         fn main() -> !int {\n    \
             var x = 1\n    \
             bump(x)\n    \
             if x == 2 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn e1007_extra_mut() {
    snap(
        "e1007_extra_mut",
        "fn look(n: int) -> int { n + 1 }\n\
         fn main() -> !int {\n    \
             var x = 1\n    \
             let y = look(mut x)\n    \
             y - 2\n\
         }\n",
    );
}

#[test]
fn e1007_take_where_mut() {
    snap(
        "e1007_take_where_mut",
        "fn bump(mut n: int) { n += 1 }\n\
         fn main() -> !int {\n    \
             var x = 1\n    \
             bump(take x)\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1007_missing_take() {
    snap(
        "e1007_missing_take",
        "struct Big { data: int }\n\
         fn consume(take b: Big) -> int { b.data }\n\
         fn main() -> !int {\n    \
             let b = Big { data: 2 }\n    \
             consume(b)\n    \
             0\n\
         }\n",
    );
}

// ------------------------------------------------------------ E1008 ----

#[test]
fn e1008_view_set_violation() {
    snap(
        "e1008_view_violation",
        "struct P { x: int, y: int, z: int }\n\
         impl P {\n    \
             fn norm(mut self.{x, y}) -> int {\n        \
                 self.z = 0\n        \
                 self.x + self.y\n    \
             }\n\
         }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2, z: 39 }\n    \
             let n = (mut p).norm()\n    \
             n - 3\n\
         }\n",
    );
}

#[test]
fn view_set_litmus_stays_silent() {
    // corpus/memory/view_set_norm.lu, the [mem.tier0.excl.3] litmus:
    // the caller uses self.z while the view is out.
    snap(
        "clean_view_set_litmus",
        "struct P { x: int, y: int, z: int }\n\
         impl P {\n    \
             fn norm(mut self.{x, y}) -> int {\n        \
                 self.x = self.x + self.y\n        \
                 self.y = 0\n        \
                 self.x\n    \
             }\n\
         }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2, z: 39 }\n    \
             let n = (mut p).norm() + p.z\n    \
             if n == 42 { 0 } else { 1 }\n\
         }\n",
    );
}

// ------------------------------------------------------------ E1009 ----

#[test]
fn e1009_mut_temporary() {
    snap(
        "e1009_mut_temporary",
        "fn bump(mut n: int) { n += 1 }\n\
         fn main() -> !int {\n    \
             bump(mut 41)\n    \
             0\n\
         }\n",
    );
}

// ----------------------------------------------------- E1004 (s19) ----

#[test]
fn e1004_param_param_store() {
    // The Cyclone equality-constraint case: storing one parameter's
    // data into another's region. Rare by measurement; no annotation
    // surface exists, so the demand reports with a restructure hint.
    snap(
        "e1004_params_independent",
        "struct Item { value: int }\n\
         struct Holder { item: Item }\n\
         fn stash(mut holder: Holder, take item: Item) {\n    \
             holder.item = item\n\
         }\n\
         fn main() -> !int {\n    \
             var h = Holder { item: Item { value: 1 } }\n    \
             let i = Item { value: 2 }\n    \
             stash(mut h, take i)\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1004_cross_region_store() {
    // Region-local data embedded into a caller-region container: the
    // [mem.region.edge] table's ❌ column, in allocation-site words.
    snap(
        "e1004_cross_region_store",
        "struct Item { value: int }\n\
         struct Holder { item: Item }\n\
         fn main() -> !int {\n    \
             var h = Holder { item: Item { value: 1 } }\n    \
             region tmp {\n        \
                 h.item = Item { value: 2 }\n    \
             }\n    \
             0\n\
         }\n",
    );
}

// ----------------------------------------------------- E1010 (s19) ----

#[test]
fn e1010_region_local_outlives_free() {
    // The sprint's headline error shape: allocated in `tmp`, freed at
    // the block end, still reachable from `out`.
    snap(
        "e1010_escape_via_binding",
        "struct Node { value: int }\n\
         fn main() -> !int {\n    \
             var out = Node { value: 0 }\n    \
             region tmp {\n        \
                 out = Node { value: 7 }\n    \
             }\n    \
             if out.value == 7 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn e1010_region_block_value_escapes() {
    // The block's own value is allocated in the dying region.
    snap(
        "e1010_escape_via_value",
        "struct Node { value: int }\n\
         fn main() -> !int {\n    \
             let n = region tmp { Node { value: 3 } }\n    \
             n.value\n\
         }\n",
    );
}

// ------------------------------------- conforming region shapes (s19) --

#[test]
fn clean_region_scratch_and_defaults() {
    // The Cyclone-posture demonstration: caller-region results, a
    // scratch region consumed in place, `in` on a region value —
    // zero region annotations, zero diagnostics.
    snap(
        "clean_region_inference",
        "struct Point { x: int, y: int }\n\
         fn make(n: int) -> Point {\n    \
             Point { x: n, y: n }\n\
         }\n\
         fn main() -> !int {\n    \
             var total = 0\n    \
             region tmp {\n        \
                 let p = Point { x: 1, y: 2 }\n        \
                 let q = make(3)\n        \
                 total = p.x + q.y\n    \
             }\n    \
             let keep = region()\n    \
             let n = in keep { 21 }\n    \
             if total + n == 24 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn clean_in_redirect_outlives_scratch() {
    // The fix ladder's "aim the allocation at a longer-lived region"
    // rung: `in dst { … }` inside the scratch block places the
    // surviving value in `dst`, which outlives `tmp` — silent.
    snap(
        "clean_in_redirect",
        "struct Node { value: int }\n\
         fn main() -> !int {\n    \
             var out = Node { value: 0 }\n    \
             let dst = region()\n    \
             region tmp {\n        \
                 in dst {\n            \
                     out = Node { value: 7 }\n        \
                 }\n    \
             }\n    \
             if out.value == 7 { 0 } else { 1 }\n\
         }\n",
    );
}

// ------------------------------------------ the region checker (s20) --

#[test]
fn e1005_move_while_open() {
    // The open window pins the handle: `move p` inside `region p { }`
    // is the transfer-of-open error ([mem.region.freeze.3]).
    snap(
        "e1005_move_while_open",
        "fn main() -> !int {\n    \
             region p {\n        \
                 let q = move p\n        \
                 0\n    \
             }\n\
         }\n",
    );
}

#[test]
fn e1005_freeze_while_open() {
    // Freezing while open would let the open window outlive the
    // immutability promise — same code, freeze spelling.
    snap(
        "e1005_freeze_while_open",
        "fn main() -> !int {\n    \
             region p {\n        \
                 let f = freeze p\n        \
                 0\n    \
             }\n\
         }\n",
    );
}

#[test]
fn e1011_open_child_of_open_owner() {
    // The multiopen antichain ([mem.region.multiopen]): p owns c
    // (iso edge via the stash), so opening c inside p's window puts
    // c's data behind two live mutable windows.
    snap(
        "e1011_ancestor_open",
        "struct Holder { child: region }\n\
         fn main() -> !int {\n    \
             let c = region()\n    \
             region p {\n        \
                 let h = Holder { child: move c }\n        \
                 let n = in h.child { 1 }\n        \
                 n\n    \
             }\n\
         }\n",
    );
}

#[test]
fn e1012_write_through_frozen() {
    // `freeze` is deep and permanent: the write reaches data a
    // freeze promoted ([mem.region.freeze.1]).
    snap(
        "e1012_write_through_frozen",
        "struct Config { limit: int }\n\
         fn main() -> !int {\n    \
             var cfg = freeze region { Config { limit: 42 } }\n    \
             cfg.limit = 7\n    \
             cfg.limit\n\
         }\n",
    );
}

#[test]
fn e1012_reopen_frozen_region() {
    // A frozen region never reopens: `in f { }` asks for a mutable
    // window on immutable-forever data.
    snap(
        "e1012_reopen_frozen",
        "fn main() -> !int {\n    \
             let r = region()\n    \
             let f = freeze r\n    \
             let n = in f { 1 }\n    \
             n\n\
         }\n",
    );
}

#[test]
fn clean_freeze_then_read_forever() {
    // The regions.lu head: build in r, freeze, read forever
    // ([mem.region.freeze.1]) — silent.
    snap(
        "clean_freeze_read",
        "struct Config { limit: int }\n\
         fn build_config() -> Config {\n    \
             Config { limit: 42 }\n\
         }\n\
         fn main() -> !int {\n    \
             let r = region(rc)\n    \
             let config = in r { build_config() }\n    \
             let frozen = freeze r\n    \
             if config.limit == 42 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn clean_frozen_return_and_imm_edge() {
    // Frozen data outlives everything and may be referenced from any
    // region ([mem.region.edge.imm]): returning it and embedding it
    // in another region's aggregate are both silent. (Both facts are
    // per-body: a frozen result arriving through a call is a plain
    // caller-region value on the other side until a signature surface
    // for `imm` results exists — the scheme-carrying interface's
    // recorded gap.)
    snap(
        "clean_frozen_imm_edge",
        "struct Config { limit: int }\n\
         struct Wrap { cfg: Config, tag: int }\n\
         fn make() -> Config {\n    \
             freeze region { Config { limit: 5 } }\n\
         }\n\
         fn main() -> !int {\n    \
             let t = freeze region { Config { limit: 5 } }\n    \
             region p {\n        \
                 let w = Wrap { cfg: t, tag: 1 }\n        \
                 if w.tag + w.cfg.limit == 6 { 0 } else { 1 }\n    \
             }\n\
         }\n",
    );
}

#[test]
fn clean_sibling_multiopen() {
    // Sibling regions co-open freely — the legal direction of the
    // antichain, pinned next to the illegal one above.
    snap(
        "clean_sibling_multiopen",
        "fn main() -> !int {\n    \
             let a = region()\n    \
             let b = region()\n    \
             var total = 0\n    \
             in a {\n        \
                 total += 1\n        \
                 in b {\n            \
                     total += 2\n        \
                 }\n    \
             }\n    \
             if total == 3 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn clean_iso_edge_then_open_after_close() {
    // The same stash as the E1011 case, opened AFTER the owner's
    // window ends: the open set is an antichain again — silent.
    snap(
        "clean_iso_open_after_close",
        "struct Holder { child: region }\n\
         fn main() -> !int {\n    \
             let c = region()\n    \
             var keep = 0\n    \
             region p {\n        \
                 keep = 1\n    \
             }\n    \
             let h = Holder { child: move c }\n    \
             let n = in h.child { 41 }\n    \
             if n + keep == 42 { 0 } else { 1 }\n\
         }\n",
    );
}
