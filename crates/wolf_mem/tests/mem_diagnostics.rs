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
        !res.diagnostics
            .iter()
            .any(|d| d.severity == wolf_diag::Severity::Error),
        "snapshot inputs resolve without errors: {:?}",
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
             let far = in keep { make(21) }\n    \
             if total + far.x == 24 { 0 } else { 1 }\n\
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
                 let scratch = Node { value: 1 }\n        \
                 in dst {\n            \
                     out = Node { value: 6 + scratch.value }\n        \
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
        "struct Cell { v: int }\n\
         fn main() -> !int {\n    \
             let a = region()\n    \
             let b = region()\n    \
             var total = 0\n    \
             in a {\n        \
                 let one = Cell { v: 1 }\n        \
                 total += one.v\n        \
                 in b {\n            \
                     let two = Cell { v: 2 }\n            \
                     total += two.v\n        \
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
         struct Note { n: int }\n\
         fn main() -> !int {\n    \
             let c = region()\n    \
             var keep = 0\n    \
             region p {\n        \
                 let note = Note { n: 1 }\n        \
                 keep = note.n\n    \
             }\n    \
             let h = Holder { child: move c }\n    \
             let n = in h.child { 41 }\n    \
             if n + keep == 42 { 0 } else { 1 }\n\
         }\n",
    );
}

// ------------------------------------------ the shared tier (s21) --

#[test]
fn e1006_direct_strong_cycle() {
    // [mem.shared.rc.2]: a type holding `shared` of itself is the
    // smallest strong cycle — rejected at the definition, with the
    // weak/handle rewrite prescribed (shared_cycle.lu's shape).
    snap(
        "e1006_direct_strong_cycle",
        "struct S { next: shared S }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn e1006_two_type_cycle() {
    // A strong cycle through a by-value embed: `A` embeds `B`, `B`
    // holds `shared A` — the shared edge closes it and gets the
    // report.
    snap(
        "e1006_two_type_cycle",
        "struct A { b: B }\n\
         struct B { back: shared A }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn e1006_cycle_through_list() {
    // The container edge is strong: `List[shared N]` inside `N` is a
    // cycle even though no field is literally `shared N`.
    snap(
        "e1006_cycle_through_list",
        "struct N { kids: List[shared N] }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn clean_weak_backedge() {
    // The prescribed rewrite: the back-edge as `weak` breaks the
    // strong cycle ([mem.shared.rc.3]) — silent.
    snap(
        "clean_weak_backedge",
        "struct A { b: B }\n\
         struct B { back: weak A }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn clean_handle_backedge() {
    // The other prescribed rewrite: a generational `handle` back-edge
    // proves nothing until dereferenced — no strong cycle.
    snap(
        "clean_handle_backedge",
        "struct S { next: handle S }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn clean_shared_clone_drop() {
    // The conforming Tier-2 shape (shared_ok.lu): cell creation,
    // clone fan-out, weak downgrade/upgrade — statically silent; the
    // dup/drop plan lands in the facts, not in diagnostics.
    snap(
        "clean_shared_clone_drop",
        "struct Cfg { limit: int }\n\
         fn main() -> !int {\n    \
             let a = shared (Cfg { limit: 7 })\n    \
             let b = a.clone()\n    \
             let w = a.downgrade()\n    \
             let live = w.upgrade() else |_| { return 1 }\n    \
             if b.limit == 7 && live.limit == 7 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn clean_pool_two_phase() {
    // [mem.shared.handle.1]/[mem.shared.handle.3]: two-phase
    // reserve/init and checked slot access under the pool region's
    // rules — statically silent (staleness is the interpreter's
    // deterministic trap, X5's dynamic half by design).
    snap(
        "clean_pool_two_phase",
        "struct Node { value: int }\n\
         fn main() -> !int {\n    \
             region r: pool(Node) {\n        \
                 var pool = Pool[Node]()\n        \
                 let h = (mut pool).reserve()\n        \
                 (mut pool).init(h, Node { value: 41 })\n        \
                 pool[h].value + 1 - 42\n    \
             }\n\
         }\n",
    );
}

// ------------------------------------------- the unsafe tier (s22) ----

#[test]
fn e1301_raw_ops_outside_unsafe() {
    // The tier boundary: C calls, raw writes, raw reads all demand
    // the ring — E1301 per operation, never a refusal (typing is
    // permissive; the *rule* lives here).
    snap(
        "e1301_raw_outside",
        "import c \"stdlib.h\"\n\
         fn main() -> !int {\n    \
             let p = c.malloc(8) as *u8\n    \
             p[0] = 1\n    \
             let v = p[0]\n    \
             c.free(p)\n    \
             v as int\n\
         }\n",
    );
}

#[test]
fn e1301_provenance_op_and_cast_outside_unsafe() {
    // Strict-provenance ops and non-identity pointer casts are
    // ring-gated too; holding/copying the pointer itself stays free
    // (creation is not a use).
    snap(
        "e1301_prov_outside",
        "import c \"stdlib.h\"\n\
         fn main() -> !int {\n    \
             // # Safety: the allocation lives for the whole function.\n    \
             let p = unsafe { c.malloc(8) as *u8 }\n    \
             let a = p.addr() as int\n    \
             // # Safety: freed exactly once.\n    \
             unsafe { c.free(p) }\n    \
             a - a\n\
         }\n",
    );
}

#[test]
fn e1302_ptr_in_signature() {
    // [mem.unsafe.scope]: no `unsafe fn`s — a `*T` parameter, return,
    // or exported field is the boundary error.
    snap(
        "e1302_ptr_in_signature",
        "fn peek(p: *u8) -> int { 0 }\n\
         fn mint() -> *u8 { mint() }\n\
         pub struct Held { raw: *u8 }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn e1302_private_struct_field_is_allowed() {
    // The module is the audit granule: module-private data may hold
    // raw pointers (allocator internals need a home).
    snap(
        "clean_private_ptr_field",
        "struct Arena { base: *u8, len: int }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn e1304_assume_needs_pointers() {
    snap(
        "e1304_assume_malformed",
        "fn main() -> !int {\n    \
             var a = 1\n    \
             var b = 2\n    \
             // # Safety: nothing raw happens; the assume is the test.\n    \
             unsafe {\n        \
                 assume noalias a, b\n    \
             }\n    \
             a + b - 3\n\
         }\n",
    );
}

#[test]
fn e1305_door_misuse() {
    snap(
        "e1305_door_misuse",
        "fn main() -> !int {\n    \
             let x = 7\n    \
             var out = 0\n    \
             // # Safety: nothing discharged; the misuse is the test.\n    \
             unsafe {\n        \
                 let v = borrow x from x\n        \
                 out = v as int\n    \
             }\n    \
             out\n\
         }\n",
    );
}

#[test]
fn w1301_unsafe_block_without_safety_comment() {
    // Advisory, never load-bearing: the block still checks; the
    // warning asks for the invariant in writing ([mem.boundary.doc]).
    snap(
        "w1301_missing_safety",
        "import c \"stdlib.h\"\n\
         fn main() -> !int {\n    \
             var out = 0\n    \
             unsafe {\n        \
                 let p = c.malloc(8) as *u8\n        \
                 p[0] = 3\n        \
                 out = p[0] as int\n        \
                 c.free(p)\n    \
             }\n    \
             out - 3\n\
         }\n",
    );
}

#[test]
fn clean_unsafe_tier_surface() {
    // The whole s22 surface in one conforming shape: ring, C calls,
    // casts, assume, raw accesses, provenance op, both door operands
    // — statically silent; every dynamic risk is s23/is04's (P1–P6).
    snap(
        "clean_unsafe_surface",
        "import c \"stdlib.h\"\n\
         fn main() -> !int {\n    \
             let r = region()\n    \
             var out = 0\n    \
             // # Safety: p/q are distinct live allocations; b is r's own\n    \
             // base pointer, so the door's claim holds by construction.\n    \
             unsafe {\n        \
                 let p = c.malloc(8) as *u8\n        \
                 let q = c.malloc(8) as *u8\n        \
                 assume noalias p, q\n        \
                 p[0] = 1\n        \
                 q[0] = 2\n        \
                 let b = r as *u8\n        \
                 let v = borrow r from b\n        \
                 out = (p[0] + q[0] + v) as int\n        \
                 c.free(p)\n        \
                 c.free(q)\n    \
             }\n    \
             out - out\n\
         }\n",
    );
}

#[test]
fn w1001_region_never_allocates() {
    // The s68 free-`region()` smell: nothing is ever built in
    // `scratch`, its handle never leaves the frame — pure ceremony.
    snap(
        "w1001_region_never_allocates",
        "fn main() -> !int {\n    \
             var total = 0\n    \
             region scratch {\n        \
                 total = 2\n    \
             }\n    \
             total - 2\n\
         }\n",
    );
}

#[test]
fn clean_list_mut_receiver_len_and_index() {
    // #6: the List mutators take `mut self` at the call site; `xs.len`
    // is a copy read (never a move of the list); `xs[0]` bounds-checks.
    // All statically silent.
    snap(
        "clean_list_mut_receiver",
        "fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             (mut xs).push(1)\n    \
             let n = xs.len\n    \
             xs[0] + n - 2\n\
         }\n",
    );
}

// ---------------------- s72 — the mode rules get their teeth (D39/D40) --

#[test]
fn e1014_write_through_read_param_projection() {
    // D39, the callee-side half #27 found missing: a `read` parameter
    // is immutable for the whole call, projections included.
    snap(
        "e1014_projected_write",
        "struct P { x: int, y: int }\n\
         fn poke(p: P) -> int {\n    \
             p.x = 7\n    \
             p.x\n\
         }\n\
         fn main() -> !int {\n    \
             var v = P { x: 1, y: 2 }\n    \
             poke(v) - 7\n\
         }\n",
    );
}

#[test]
fn e1014_whole_reassign_and_compound() {
    // The binding itself is the caller's place: rebinding it whole and
    // compound-assigning through it are both writes.
    snap(
        "e1014_whole_and_compound",
        "struct P { x: int, y: int }\n\
         fn wipe(p: P) -> int {\n    \
             p = P { x: 0, y: 0 }\n    \
             p.y += 1\n    \
             p.y\n\
         }\n\
         fn main() -> !int {\n    \
             var v = P { x: 1, y: 2 }\n    \
             wipe(v) - 1\n\
         }\n",
    );
}

#[test]
fn e1014_mut_lend_of_read_param() {
    // Lending the binding `mut` onward is a write by proxy: the deal
    // with the caller does not transfer.
    snap(
        "e1014_mut_lend",
        "struct P { x: int, y: int }\n\
         fn bump(mut a: P) { a.x += 1 }\n\
         fn relay(p: P) -> int {\n    \
             bump(mut p)\n    \
             p.x\n\
         }\n\
         fn main() -> !int {\n    \
             var v = P { x: 1, y: 2 }\n    \
             relay(v) - 2\n\
         }\n",
    );
}

#[test]
fn e1014_read_self_method_write() {
    // `self` without a mode is `read` like any other parameter.
    snap(
        "e1014_read_self_write",
        "struct V { x: int }\n\
         impl V {\n    \
             fn peek(self) -> int {\n        \
                 self.x = 9\n        \
                 self.x\n    \
             }\n\
         }\n\
         fn main() -> !int {\n    \
             var v = V { x: 9 }\n    \
             v.peek() - 9\n\
         }\n",
    );
}

#[test]
fn read_param_reads_stay_silent() {
    // The rule rejects writes only: reads, projections read, and
    // passing the parameter onward `read` all stay silent.
    snap(
        "clean_read_param_reads",
        "struct P { x: int, y: int }\n\
         fn total(p: P) -> int { p.x + p.y }\n\
         fn relay(p: P) -> int { total(p) }\n\
         fn main() -> !int {\n    \
             var v = P { x: 1, y: 2 }\n    \
             relay(v) - 3\n\
         }\n",
    );
}

#[test]
fn e1002_copy_read_after_mut_arg() {
    // D39, the overlap rule's static half: the `Copy` read of `p.x`
    // evaluates inside the exclusive claim `mut p` already spelled —
    // f(mut a, a.x), the shape lupin traps dynamically.
    snap(
        "e1002_copy_read_after_mut",
        "struct P { x: int, y: int }\n\
         fn bump(mut a: P, n: int) { a.x += n }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2 }\n    \
             bump(mut p, p.x)\n    \
             p.x - 2\n\
         }\n",
    );
}

#[test]
fn copy_read_before_mut_stays_silent() {
    // Left-to-right order is the rule's clock: a `Copy` read finished
    // before the `mut` claim began never conflicts.
    snap(
        "clean_copy_read_before_mut",
        "struct P { x: int, y: int }\n\
         fn bump(n: int, mut a: P) { a.x += n }\n\
         fn main() -> !int {\n    \
             var p = P { x: 1, y: 2 }\n    \
             bump(p.x, mut p)\n    \
             p.x - 2\n\
         }\n",
    );
}

#[test]
fn e1013_push_while_iterating() {
    // D40, the F-0014 acceptance shape (#15): the loop's read claim
    // rejects the push — as E1013 with the collect-then-apply
    // teaching, never the old E1001 reads-as-moves accident.
    snap(
        "e1013_push_while_iterating",
        "fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             (mut xs).push(1)\n    \
             (mut xs).push(2)\n    \
             for x in xs {\n        \
                 (mut xs).push(x)\n    \
             }\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1013_move_while_iterating() {
    // Moving the container out from under the loop is the same
    // conflict; the move recovers as a read, so exactly one error
    // reports — no E1001 echo on the back edge.
    snap(
        "e1013_move_while_iterating",
        "fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             (mut xs).push(1)\n    \
             var n = 0\n    \
             for x in xs {\n        \
                 let ys = xs\n        \
                 n += x + ys.len\n    \
             }\n    \
             n - 2\n\
         }\n",
    );
}

#[test]
fn e1013_reassign_while_iterating() {
    snap(
        "e1013_reassign_while_iterating",
        "fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             (mut xs).push(1)\n    \
             for x in xs {\n        \
                 xs = List[int]()\n    \
             }\n    \
             xs.len - 1\n\
         }\n",
    );
}

#[test]
fn iterate_then_mutate_stays_silent() {
    // The claim is a read, not a move: the container is live behind
    // the walk (reads inside are fine) and after it (mutation resumes
    // the moment the loop ends).
    snap(
        "clean_iterate_then_mutate",
        "fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             (mut xs).push(1)\n    \
             (mut xs).push(2)\n    \
             var total = 0\n    \
             for x in xs {\n        \
                 total += x + xs.len - xs.len\n    \
             }\n    \
             (mut xs).push(total)\n    \
             xs.len - 3\n\
         }\n",
    );
}

// ------------------------------------------------------------ W1004 ----

#[test]
fn w1004_lent_view_returned() {
    // s89 (#86): `s.bytes()` in an argument position LENDS the string's
    // own storage, and a callee that returns the parameter keeps it
    // past the call the lend is scoped to. s92: the bytes are copied
    // and the program compiles; the diagnostic says the copy happened
    // and where the escape is (E1015 refused this through s91).
    snap(
        "w1004_lent_view_returned",
        "fn keep(bs: List[int]) -> List[int] { bs }\n\
         fn main() -> !int {\n    \
             let s = \"wolf\"\n    \
             let held = keep(s.bytes())\n    \
             held.len - 4\n\
         }\n",
    );
}

#[test]
fn w1004_lent_view_relent_into_an_escape() {
    // The escape is transitive: `relay` only passes the view on, and
    // the function it passes it to is the one that keeps it. The
    // diagnostic names the call site that lent, not the hop.
    snap(
        "w1004_lent_view_relent",
        "fn keep(bs: List[int]) -> List[int] { bs }\n\
         fn relay(bs: List[int]) -> List[int] { keep(bs) }\n\
         fn main() -> !int {\n    \
             let s = \"wolf\"\n    \
             relay(s.bytes()).len - 4\n\
         }\n",
    );
}

#[test]
fn a_read_only_lend_stays_silent() {
    // The seven consuming positions are the whole rule: a callee that
    // only reads gets the view, and nothing is reported.
    snap(
        "clean_byte_view_lend",
        "fn total(bs: List[int]) -> int {\n    \
             var n = 0\n    \
             for b in bs { n = n + b }\n    \
             n + bs.len + bs.count() + bs[0]\n\
         }\n\
         fn main() -> !int {\n    \
             let s = \"wolf\"\n    \
             total(s.bytes()) - 567\n\
         }\n",
    );
}

#[test]
fn a_bound_bytes_list_is_not_a_lend() {
    // The fix ladder: `let` materializes, so the same callee that
    // W1004 reports a copy for takes the bound list without a word.
    snap(
        "clean_bound_bytes_list",
        "fn keep(bs: List[int]) -> List[int] { bs }\n\
         fn main() -> !int {\n    \
             let s = \"wolf\"\n    \
             let bs = s.bytes()\n    \
             keep(bs).len - 4\n\
         }\n",
    );
}

#[test]
fn nested_read_iteration_stays_silent() {
    // Two read claims on one container coexist: read never excludes
    // read.
    snap(
        "clean_nested_iteration",
        "fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             (mut xs).push(1)\n    \
             var total = 0\n    \
             for x in xs {\n        \
                 for y in xs {\n            \
                     total += x + y\n        \
                 }\n    \
             }\n    \
             total - 2\n\
         }\n",
    );
}

#[test]
fn e1002_write_under_a_dyn_pair() {
    // s98 (D47, `[mem.dyn.unsize]`): `d as dyn Draw` in binding
    // position is a SHARED loan of `d`, borrower `o`, scoped by `o`'s
    // liveness (NLL, not lexical). The write to `d` lands while
    // `o.draw()` still needs the pair, so the loan engine refuses it —
    // the teeth behind "slot-vs-place is unobservable".
    snap(
        "e1002_write_under_a_dyn_pair",
        "trait Draw {\n    fn draw(self) -> int\n}\n\
         struct Dot {\n    x: int,\n}\n\
         impl Draw for Dot {\n    fn draw(self) -> int {\n        self.x\n    }\n}\n\
         fn main() -> !int {\n    \
             var d = Dot { x: 7 }\n    \
             let o = d as dyn Draw\n    \
             d = Dot { x: 9 }\n    \
             if o.draw() == 7 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn clean_write_after_a_dyn_pairs_last_use() {
    // s98's loan is NLL-scoped, not lexical: `o`'s last use passes
    // BEFORE the write to `d`, so the loan is dead and the write is
    // free (the snapshot pins zero diagnostics). The E1002 twin above
    // is the same program with the write and the use swapped.
    snap(
        "clean_write_after_dyn_last_use",
        "trait Draw {\n    fn draw(self) -> int\n}\n\
         struct Dot {\n    x: int,\n}\n\
         impl Draw for Dot {\n    fn draw(self) -> int {\n        self.x\n    }\n}\n\
         fn main() -> !int {\n    \
             var d = Dot { x: 7 }\n    \
             let o = d as dyn Draw\n    \
             let first = o.draw()\n    \
             d = Dot { x: 9 }\n    \
             if first == 7 { 0 } else { 1 }\n\
         }\n",
    );
}
