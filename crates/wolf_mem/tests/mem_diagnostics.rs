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
fn e1301_volatile_outside_unsafe() {
    // kw07 (`[mem.unsafe.volatile]`): a volatile read or write is an
    // access through the pointer, ring-gated like `p[0]`.
    snap(
        "e1301_volatile_outside",
        "fn peek(p: *u32) -> u32 {\n    \
             p.read_volatile()\n\
         }\n\
         fn poke(p: *u32) {\n    \
             p.write_volatile(1)\n\
         }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn e1301_atomic_outside_unsafe() {
    // kw11 (`[conc.mm.atomic.raw]`, `[conc.mm.fence]`): an atomic
    // operation is an access through the pointer, ring-gated like `p[0]`;
    // a fence is ring-gated for every order but `seq_cst`.
    snap(
        "e1301_atomic_outside",
        "fn grab(p: *u32) -> u32 {\n    \
             p.atomic_swap(1, Order.acquire)\n\
         }\n\
         fn publish() {\n    \
             fence(Order.seq_cst)\n    \
             fence(Order.release)\n\
         }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn e1301_module_var_outside_unsafe() {
    // kw09 (`[mem.static.2]`, K11 = A): every read and write of a module
    // `var` is raw-tier — one E1301 per site; a `let` and a `const` read
    // freely, and a local that shadows the `var` is just a local.
    snap(
        "e1301_module_var_outside",
        "var ticks: int = 0\n\
         let STEP: int = 1\n\
         const MAX: int = 9\n\
         fn bump() {\n    \
             ticks = ticks + STEP\n\
         }\n\
         fn shadow() -> int {\n    \
             let ticks = MAX\n    \
             ticks\n\
         }\n\
         fn main() -> !int { 0 }\n",
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
    // [mem.unsafe.scope] / [mem.unsafe.sig]: no `unsafe fn`s — a `*T`
    // parameter or return on a `pub` fn or on a method, or an exported
    // field, is the boundary error (K9(b) = B, kw02).
    snap(
        "e1302_ptr_in_signature",
        "pub fn peek(p: *u8) -> int { 0 }\n\
         pub fn mint() -> *u8 { mint() }\n\
         pub struct Held { raw: *u8 }\n\
         struct Cell { n: int }\n\
         impl Cell {\n    fn at(self, p: *u8) -> int { 0 }\n}\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn e1302_private_and_membrane_signatures_are_allowed() {
    // [mem.unsafe.sig] (K9(b) = B, STATUS #31; #32 R1; wolf-lang#514):
    // a module-private fn and both sides of the C membrane take `*T`.
    snap(
        "clean_private_and_membrane_ptr_sigs",
        "fn poke(p: *u8, v: u8) { }\n\
         fn mint(p: *u8) -> *u8 { p }\n\
         export fn kx_len(s: *u8) -> i64 { 0 }\n\
         pub export fn kx_back(p: *u8) -> *u8 { p }\n\
         extern \"c\" fn kc_fill(p: *u8, n: i64)\n\
         extern \"c\" fn kc_at(p: *u8) -> *u8 { p }\n\
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
fn clean_copy_read_after_mut_arg() {
    // D39's refusal, retired by s186 (`[mem.tier0.excl.4]`, ruled
    // 2026-09-30): the `Copy` read of `p.x` in the later argument ends
    // before the claim `mut p` takes effect at call entry —
    // f(mut a, a.x) is two-phase, like the receiver's `xs.push(xs.len)`.
    snap(
        "clean_copy_read_after_mut",
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
fn e1002_nested_call_claims_inside_a_mut_arg() {
    // s168 — a nested CALL that claims the same place inside a later
    // argument conflicts: the callee receives the place exclusively,
    // and a second claim is not a read (`[mem.tier0.excl.4]`). Untested before element lends existed; with them it
    // is the difference between a diagnostic and a dangling write.
    snap(
        "e1002_nested_call_after_mut",
        "fn grow(mut xs: List[int]) -> int {\n    \
             (mut xs).push(9)\n    \
             5\n\
         }\n\
         fn take2(mut a: int, b: int) {\n    \
             a = a + b\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2]\n    \
             take2(mut xs[0], grow(mut xs))\n    \
             xs[0] - 6\n\
         }\n",
    );
}

#[test]
fn e1002_nested_call_claims_a_spilled_bases_prefix() {
    // The same rule over a SPILLED field rather than a container
    // element. This shape predates s168 and was wrong the other way:
    // both lanes accepted it, the writeback restored the stale copy
    // over the nested call's write, and the two lanes printed
    // different numbers with no diagnostic anywhere.
    snap(
        "e1002_nested_call_over_spill",
        "struct R { a: int, b: int }\n\
         fn wipe(mut r: R) -> int {\n    \
             r.a = 1000\n    \
             r.b = 2000\n    \
             7\n\
         }\n\
         fn take2(mut a: int, b: int) {\n    \
             a = a + b\n\
         }\n\
         fn main() -> !int {\n    \
             var r = R { a: 1, b: 2 }\n    \
             take2(mut r.a, wipe(mut r))\n    \
             r.a - 8\n\
         }\n",
    );
}

#[test]
fn two_literal_element_lends_stay_silent() {
    // s168's first leg, per element since eg02 (EGC's EG2): two
    // different literal indices are distinct places
    // (`[mem.model.place.elem]` 1(a)), so two `mut` element claims in
    // one call are disjoint. Through 0.2.18 this was E1002.
    snap(
        "clean_two_literal_element_lends",
        "fn add2(mut a: int, mut b: int) {\n    \
             a = a + 1\n    \
             b = b + 1\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2, 3]\n    \
             add2(mut xs[0], mut xs[1])\n    \
             xs[0] - 2\n\
         }\n",
    );
}

#[test]
fn e1002_a_run_time_index_beside_an_element_lend() {
    // Item 2 under a claim: `xs[i]` is one place with every index of
    // `xs`, so the second claim conflicts; the note says why in the
    // clause's words rather than calling two elements a prefix.
    snap(
        "e1002_elem_run_time_index_lend",
        "fn add2(mut a: int, mut b: int) {\n    \
             a = a + 1\n    \
             b = b + 1\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2, 3]\n    \
             var i = 1\n    \
             add2(mut xs[0], mut xs[i])\n    \
             xs[0] - 2\n\
         }\n",
    );
}

#[test]
fn offsets_and_loop_indices_stay_silent_in_one_call() {
    // EG3 (eg03), `[mem.model.place.elem]` item 4: R1 (`xs[i]` and
    // `xs[i + 1]`, `xs[i + 1]` and `xs[i + 2]`) and R2 (a `1..3` index
    // the body never writes against `0`; a `0..2` one against `2`)
    // prove the claims of one call distinct.
    snap(
        "clean_elem_r1_r2_one_call",
        "fn add2(mut a: int, mut b: int) {\n    \
             a = a + 1\n    \
             b = b + 1\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2, 3, 4]\n    \
             var i = 0\n    \
             add2(mut xs[i], mut xs[i + 1])\n    \
             add2(mut xs[i + 1], mut xs[i + 2])\n    \
             for j in 1..3 {\n        \
                 add2(mut xs[0], mut xs[j])\n    \
             }\n    \
             for j in 0..2 {\n        \
                 add2(mut xs[j], mut xs[2])\n    \
             }\n    \
             xs[0] - xs[0]\n\
         }\n",
    );
}

#[test]
fn e1002_an_index_written_between_the_claims() {
    // EG3 (eg03): R1 holds only while the call keeps `i` unwritten. A
    // block argument between the claims writes it, so `xs[i]` and
    // `xs[i + 1]` stay one place — and at run time they are one element.
    snap(
        "e1002_elem_offset_rebound",
        "fn add3(mut a: int, n: int, mut b: int) {\n    \
             a = a + n\n    \
             b = b + 1\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2, 3]\n    \
             var i = 1\n    \
             add3(mut xs[i], { i = i - 1; 0 }, mut xs[i + 1])\n    \
             xs[0] - 1\n\
         }\n",
    );
}

#[test]
fn e1002_a_loop_index_the_body_writes() {
    // EG3 (eg03): R2 holds only for a loop index the body never
    // assigns; `i = 0` puts it back on the literal.
    snap(
        "e1002_elem_induction_written",
        "fn add2(mut a: int, mut b: int) {\n    \
             a = a + 1\n    \
             b = b + 1\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2, 3]\n    \
             for i in 1..3 {\n        \
                 i = 0\n        \
                 add2(mut xs[0], mut xs[i])\n    \
             }\n    \
             xs[0] - 1\n\
         }\n",
    );
}

#[test]
fn a_member_read_under_an_element_claim_stays_silent() {
    // 1(c) under a claim (eg02b, wolf-lang#472 ruled A): an element and
    // a member of its container are distinct places whatever the index
    // is, so a member read beside a `mut` element claim — after it
    // (D39's order-sensitive half), before it, through a run-time
    // index, one level down, inside a nested call — conflicts with
    // nothing. The whole container against its element still does
    // (the next fixture).
    snap(
        "clean_elem_member_read_under_claim",
        "fn bump(mut a: int, n: int) {\n    \
             a = a + n\n\
         }\n\
         fn pre(n: int, mut a: int) {\n    \
             a = a + n\n\
         }\n\
         fn id(n: int) -> int { n }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2, 3]\n    \
             var i = 1\n    \
             var g = [[1, 2], [3]]\n    \
             bump(mut xs[0], xs.len)\n    \
             pre(xs.len, mut xs[1])\n    \
             bump(mut xs[i], xs.len)\n    \
             bump(mut g[0][1], g[0].len)\n    \
             bump(mut g[1][0], g.len)\n    \
             bump(mut xs[2], id(xs.len))\n    \
             xs[0] - 4\n\
         }\n",
    );
}

#[test]
fn e1002_the_whole_container_read_under_an_element_claim() {
    // 1(c) is about a member, not the container: `both(mut xs[0], xs)`
    // lends all of `xs`, a prefix of the claimed `xs[0]` (item 2's last
    // sentence).
    snap(
        "e1002_elem_claim_whole_read",
        "fn both(mut a: int, ys: List[int]) {\n    \
             a = a + ys.len\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2, 3]\n    \
             both(mut xs[0], xs)\n    \
             xs[0] - 4\n\
         }\n",
    );
}

#[test]
fn e1002_a_nested_call_claims_a_run_time_index() {
    // s168's nested-call leg under item 2: `grow(mut xs[i])` inside the
    // claim on `xs[0]` may reallocate the very element lent.
    snap(
        "e1002_elem_nested_run_time_index",
        "fn grow(mut ys: List[int]) -> int {\n    \
             (mut ys).push(9)\n    \
             5\n\
         }\n\
         fn put(mut a: List[int], n: int) {\n    \
             (mut a).push(n)\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [[1], [2]]\n    \
             var i = 1\n    \
             put(mut xs[0], grow(mut xs[i]))\n    \
             xs[0].len - 2\n\
         }\n",
    );
}

#[test]
fn element_claims_per_leg_stay_silent() {
    // EG2 on every claim leg at once, each over two different
    // literals: D39's read inside a claim, s168's nested call, `mut`
    // against `take`, iteration of one element while another changes.
    snap(
        "clean_elem_claims_per_leg",
        "fn bump(mut a: int, n: int) {\n    \
             a = a + n\n\
         }\n\
         fn grow(mut ys: List[int]) -> int {\n    \
             (mut ys).push(9)\n    \
             5\n\
         }\n\
         fn put(mut a: List[int], n: int) {\n    \
             (mut a).push(n)\n\
         }\n\
         fn eat(mut a: List[int], take b: List[int]) {\n    \
             (mut a).push(b.len)\n\
         }\n\
         fn main() -> !int {\n    \
             var ns = [1, 2]\n    \
             bump(mut ns[0], ns[1])\n    \
             var xs = [[1], [2], [3]]\n    \
             put(mut xs[0], grow(mut xs[1]))\n    \
             eat(mut xs[0], take xs[2])\n    \
             for x in xs[0] {\n        \
                 (mut xs[1]).push(x)\n    \
             }\n    \
             ns[0] + xs[1].len - 10\n\
         }\n",
    );
}

#[test]
fn nested_call_on_a_disjoint_place_stays_silent() {
    // The guard on the s168 rule: disjoint bases and disjoint FIELDS
    // still pass. A rule that rejected these would be a new way to
    // refuse correct programs.
    snap(
        "clean_nested_call_disjoint",
        "struct R { a: int, b: int }\n\
         fn add(mut n: int, k: int) {\n    \
             n = n + k\n\
         }\n\
         fn side(mut m: int) -> int {\n    \
             m = m + 1\n    \
             5\n\
         }\n\
         fn main() -> !int {\n    \
             var r = R { a: 1, b: 2 }\n    \
             add(mut r.a, side(mut r.b))\n    \
             r.a - 6\n\
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

// -------------------- the byte-view lend, after W1004 retired (#387) ----

#[test]
fn w1004_lent_view_returned() {
    // s89 (#86): `s.bytes()` in an argument position LENDS the string's
    // own storage, and a callee that returns the parameter keeps it
    // past the call the lend is scoped to. s92: the bytes are copied
    // and the program compiles; the diagnostic says the copy happened
    // and where the escape is (E1015 refused this through s91). s165
    // (#366): the callee returning its `read` parameter is E1002 in
    // its own right now. s167 (#387) retired W1004, so the snapshot
    // carries the refusal alone and the degraded lend is a silent copy
    // again — which is what it compiled to all along.
    snap(
        "w1004_lent_view_returned",
        "fn keep(bs: List[byte]) -> List[byte] { bs }\n\
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
    // diagnostic names the call site that lent, not the hop. s165
    // (#366): `keep` is E1002; `relay` hands back `keep`'s RESULT, a
    // fresh value, and is not.
    snap(
        "w1004_lent_view_relent",
        "fn keep(bs: List[byte]) -> List[byte] { bs }\n\
         fn relay(bs: List[byte]) -> List[byte] { keep(bs) }\n\
         fn main() -> !int {\n    \
             let s = \"wolf\"\n    \
             relay(s.bytes()).len - 4\n\
         }\n",
    );
}

#[test]
fn e1002_read_param_returned() {
    // s165 (#366): the plain move-out. The caller keeps `b`, so the
    // returned value would be a second live path to it.
    snap(
        "e1002_read_param_returned",
        "fn f(b: List[int]) -> List[int] { b }\n\
         fn main() -> !int {\n    \
             var xs = List[int]()\n    \
             let ys = f(xs)\n    \
             ys.len + xs.len\n\
         }\n",
    );
}

#[test]
fn e1002_read_param_carried_out() {
    // s165 (#366): the value escapes wherever it is carried — a struct
    // literal, a rebinding, an element of a generic list — and the
    // report names the move where the `copy` belongs.
    snap(
        "e1002_read_param_carried_out",
        "struct Box { items: List[int] }\n\
         fn wrap(b: List[int]) -> Box { Box { items: b } }\n\
         fn rebind(b: List[int]) -> List[int] {\n    \
             let c = b\n    \
             c\n\
         }\n\
         fn first[T](xs: List[T]) -> T { xs[0] }\n\
         fn replace(mut out: List[int], b: List[int]) { out = b }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn e1014_read_param_through_a_rebinding() {
    // s165 (#366): a write or a `take` through a binding that holds the
    // `read` parameter's value is the write or give-away through the
    // parameter.
    snap(
        "e1014_read_param_through_a_rebinding",
        "fn eat(take v: List[int]) -> int { v.len }\n\
         fn write(b: List[int]) -> int {\n    \
             var c = b\n    \
             (mut c).push(9)\n    \
             c.len\n\
         }\n\
         fn give(b: List[int]) -> int {\n    \
             let c = b\n    \
             eat(take c)\n\
         }\n\
         struct Rd { b: List[int], pos: int }\n\
         fn bump(mut r: Rd) { r.pos = r.pos + 1 }\n\
         fn poke(bs: List[int]) -> int {\n    \
             var r = Rd { b: bs, pos: 0 }\n    \
             r.b[0] = 5\n    \
             bump(mut r)\n    \
             r.pos\n\
         }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn read_param_moves_that_stay_legal() {
    // s165 (#366): no alias, or no escape — a struct of scalars, a
    // `str`, an `int ! {none}` fallback, a scrutinee piece read in
    // place, a rebinding replaced whole, a `take` parameter, a `copy`,
    // and a cursor around the lent handle written through FIELD steps.
    snap(
        "clean_read_param_moves",
        "struct Point { x: int, y: int }\n\
         struct Named { name: str, items: List[int] }\n\
         fn same(p: Point) -> Point { p }\n\
         fn label(n: Named) -> str { n.name }\n\
         fn or(v: int ! {none}, d: int) -> int { v else d }\n\
         fn count(n: Named) -> int {\n    \
             match n {\n        \
                 Named { name, items } => items.len,\n    \
             }\n\
         }\n\
         fn widest(key: List[int]) -> int {\n    \
             var key0 = key\n    \
             if key0.len > 2 { key0 = List[int]() }\n    \
             key0.len\n\
         }\n\
         fn keep(take b: List[int]) -> List[int] {\n    \
             var c = b\n    \
             (mut c).push(3)\n    \
             c\n\
         }\n\
         fn fresh(b: List[int]) -> List[int] { copy b }\n\
         struct Rd { b: List[int], pos: int }\n\
         fn scan(bs: List[int]) -> int {\n    \
             var r = Rd { b: bs, pos: 0 }\n    \
             r.pos = 1\n    \
             r.pos += r.b.len\n    \
             r.pos\n\
         }\n\
         fn main() -> !int { 0 }\n",
    );
}

#[test]
fn a_read_only_lend_stays_silent() {
    // The seven consuming positions are the whole rule: a callee that
    // only reads gets the view, and nothing is reported.
    snap(
        "clean_byte_view_lend",
        "fn total(bs: List[byte]) -> int {\n    \
             var n = 0\n    \
             for b in bs { n = n + b }\n    \
             n + bs.len + bs.count() + bs[0] as int\n\
         }\n\
         fn main() -> !int {\n    \
             let s = \"wolf\"\n    \
             total(s.bytes()) - 567\n\
         }\n",
    );
}

#[test]
fn a_bound_bytes_list_is_not_a_lend() {
    // The fix ladder: `let` materializes, so the callee that would
    // otherwise force a copy at the lend takes the bound list without
    // a word.
    // s165 (#366): `keep` hands back `copy bs` — returning the `read`
    // parameter itself is E1002.
    snap(
        "clean_bound_bytes_list",
        "fn keep(bs: List[byte]) -> List[byte] { copy bs }\n\
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

#[test]
fn e1001_tuple_destructure_moves_elements() {
    // s128 (#173): `let (x, _) = p` moves p.0 ONLY — p.1 stays
    // usable, whole-p and p.0 reuse are the error.
    snap(
        "e1001_tuple_destructure",
        "struct Inner { n: int }\n\
         fn main() -> !int {\n    \
             var p = (Inner { n: 1 }, Inner { n: 2 })\n    \
             let (x, _) = p\n    \
             let b = p.1.n\n    \
             let c = p.0.n\n    \
             x.n + b + c\n\
         }\n",
    );
}

// ------------------------------- built strs are sites (s153, #310) --

#[test]
fn e1010_region_built_str_is_the_block_value() {
    // wolf-lang#310's shape: `+` inside `scratch` allocates there
    // ([mem.region.escape]); the block's value is the freed bytes.
    // Through s152 this was silent — `regions` printed from freed
    // storage, with a W1001 saying the region never allocates.
    snap(
        "e1010_str_concat_block_value",
        "fn build() -> str {\n    \
             region scratch {\n        \
                 let s = \"re\" + \"gions\"\n        \
                 s\n    \
             }\n\
         }\n\
         fn main() -> !int {\n    \
             print(\"{build()}\")\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1010_region_built_str_by_interpolation_held_outside() {
    // `+` is defined as `"{s}{u}"` ([type.str.concat]), so a hole makes
    // an interpolation the same site: held by an outer binding when
    // the region frees.
    snap(
        "e1010_str_interp_escape_via_binding",
        "fn main() -> !int {\n    \
             var keep = \"\"\n    \
             region scratch {\n        \
                 let n = 7\n        \
                 keep = \"n={n}\"\n    \
             }\n    \
             if keep.len == 3 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn e1010_str_from_a_materializing_builtin() {
    // s160 (wolf-lang#321, `[mem.region.escape]`): `repeat` builds
    // fresh bytes in the ambient region — a site, exactly as `+` is.
    // `str` is `Copy`, so the call-result rule (non-`Copy` returns
    // only) never saw it.
    snap(
        "e1010_str_repeat_block_value",
        "fn build() -> str {\n    \
             region scratch {\n        \
                 let s = \"re\".repeat(2)\n        \
                 s\n    \
             }\n\
         }\n\
         fn main() -> !int {\n    \
             print(\"{build()}\")\n    \
             0\n\
         }\n",
    );
}

#[test]
fn clean_str_from_a_view_builtin_is_no_site() {
    // The other side of `[mem.str.view]`: `trim` is a subslice of the
    // receiver, whose bytes are a static literal's — no allocation, no
    // site, and nothing to escape.
    snap(
        "clean_str_trim_block_value",
        "fn build() -> str {\n    \
             region scratch {\n        \
                 let s = \"  re  \".trim()\n        \
                 s\n    \
             }\n\
         }\n\
         fn main() -> !int {\n    \
             print(\"{build()}\")\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1010_str_field_read_out_of_a_region_local_struct() {
    // s160 (wolf-lang#321): returning `d` was E1010 all along;
    // returning `d.title` was not, because a `Copy` field read flowed
    // no site. Right for an `int`, wrong for a `str` whose bytes live
    // in the parent's region.
    snap(
        "e1010_str_field_escape",
        "struct Doc { title: str, words: int }\n\
         fn build() -> str {\n    \
             region scratch {\n        \
                 let d = Doc { title: \"re\" + \"gions\", words: 1 }\n        \
                 let t = d.title\n        \
                 t\n    \
             }\n\
         }\n\
         fn main() -> !int {\n    \
             print(\"{build()}\")\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1010_str_built_in_a_proc_and_sent() {
    // s160 (wolf-lang#355, `[mem.region.proc]`): a `spawn proc` entry
    // has no caller region to inherit — `[conc.proc.1]` makes the proc
    // its own failure domain and `[conc.proc.kill]` step 3 bulk-frees
    // its regions at exit — so the interpolation is a site in
    // `proc:worker`, and sending it out is the same escape a
    // region-block-built payload is. The fix ladder is the proc's own:
    // build it in the spawner and hand it in.
    snap(
        "e1010_str_built_in_proc_sent",
        "fn worker(n: int, out: channel[str]) -> !int {\n    \
             out.send(\"built {n} here\")?\n    \
             0\n\
         }\n\
         fn main() -> !int {\n    \
             let out = channel[str](4)\n    \
             let c = spawn proc worker(7, out)\n    \
             out.close()\n    \
             for line in out { print(line) }\n    \
             0\n\
         }\n",
    );
}

#[test]
fn clean_str_built_in_the_spawner_and_handed_to_a_proc() {
    // The accepted twin: a parameter's region is the SPAWNER's, which
    // outlives the proc, so the bytes never lived in `proc:worker`.
    snap(
        "clean_str_param_sent_from_proc",
        "fn worker(msg: str, out: channel[str]) -> !int {\n    \
             out.send(msg)?\n    \
             0\n\
         }\n\
         fn main() -> !int {\n    \
             let n = 7\n    \
             let msg = \"built {n} here\"\n    \
             let out = channel[str](4)\n    \
             let c = spawn proc worker(msg, out)\n    \
             out.close()\n    \
             for line in out { print(line) }\n    \
             0\n\
         }\n",
    );
}

#[test]
fn e1010_region_built_str_by_append_held_outside() {
    // `keep += u` is `keep = keep + u`: the fresh allocation lands in
    // the ambient region — `scratch` — and `keep` is declared outside.
    snap(
        "e1010_str_append_escape_via_binding",
        "fn main() -> !int {\n    \
             var keep = \"re\"\n    \
             region scratch {\n        \
                 keep += \"gions\"\n    \
             }\n    \
             if keep.len == 7 { 0 } else { 1 }\n\
         }\n",
    );
}

#[test]
fn built_strs_in_the_ambient_region_stay_silent() {
    // The conforming shapes: a str built in the caller's region and
    // returned (the default effect binds it there); a str built inside
    // a scratch region and consumed there; and the region is not
    // "never allocates" any more — no W1001 — because it did.
    snap(
        "clean_str_builders",
        "fn greet(name: str) -> str {\n    \
             var s = \"hello, \" + name\n    \
             s += \"!\"\n    \
             \"{s} ({s.len})\"\n\
         }\n\
         fn main() -> !int {\n    \
             var total = 0\n    \
             region scratch {\n        \
                 let line = greet(\"wolf\") + \"\\n\"\n        \
                 total = total + line.len\n    \
             }\n    \
             if total == 17 { 0 } else { 1 }\n\
         }\n",
    );
}

// ------------------------------------------ eg01: element places ----
//
// `[mem.model.place.elem]` for moves: distinct literal indices are
// distinct places (1(a)/(b)), an element is not its container's header
// (1(c)), a store revives a moved element only when it surely denotes
// it (item 3, wolf-lang#460), and R3: `xs[i] = v` revives a moved
// `xs[i]` only while `i` is unwritten since the move.

#[test]
fn clean_distinct_literals_and_the_header_after_an_element_move() {
    snap(
        "clean_elem_literals_and_header",
        "fn main() -> !int {\n    \
             var xs = List[List[int]]()\n    \
             (mut xs).push([1])\n    \
             (mut xs).push([2, 3])\n    \
             let a = move xs[0]\n    \
             a.len + xs[1].len + xs[0x1].len + xs.len\n\
         }\n",
    );
}

#[test]
fn e1001_a_run_time_index_may_be_the_moved_element() {
    // Item 2: `xs[i]` against the moved `xs[0]` — the note names the
    // element rule, not a prefix.
    snap(
        "e1001_elem_may_alias",
        "fn main() -> !int {\n    \
             var xs = List[List[int]]()\n    \
             (mut xs).push([1])\n    \
             (mut xs).push([2, 3])\n    \
             var i = 1\n    \
             let a = move xs[0]\n    \
             a.len + xs[i].len\n\
         }\n",
    );
}

#[test]
fn clean_a_store_revives_the_literal_it_names_and_the_whole_revives_all() {
    snap(
        "clean_elem_literal_store_revives",
        "fn main() -> !int {\n    \
             var xs = List[List[int]]()\n    \
             (mut xs).push([1])\n    \
             (mut xs).push([2])\n    \
             let a = move xs[0]\n    \
             xs[0] = [7, 8]\n    \
             let b = move xs[1]\n    \
             xs = [[9]]\n    \
             a.len + b.len + xs[0].len + xs[1].len\n\
         }\n",
    );
}

#[test]
fn e1001_a_whole_move_is_not_revived_by_one_element_store() {
    // Before eg01 the residue of a whole-container move re-initialized
    // at one element was empty (every element overlapped the collapsed
    // store), so `xs[1]` read the moved list.
    snap(
        "e1001_elem_whole_move_one_store",
        "fn main() -> !int {\n    \
             var xs = List[List[int]]()\n    \
             (mut xs).push([1])\n    \
             (mut xs).push([2])\n    \
             let ys = move xs\n    \
             xs[0] = [5]\n    \
             ys.len + xs[1].len\n\
         }\n",
    );
}

#[test]
fn clean_r3_same_local_index_revives() {
    snap(
        "clean_elem_r3_same_index",
        "fn main() -> !int {\n    \
             var xs = List[List[int]]()\n    \
             (mut xs).push([1])\n    \
             (mut xs).push([2, 3])\n    \
             var i = 1\n    \
             var t = move xs[i]\n    \
             (mut t).push(4)\n    \
             let n = i + 1\n    \
             xs[i] = take t\n    \
             xs[0].len + xs[i].len + n\n\
         }\n",
    );
}

#[test]
fn e1001_r3_does_not_hold_after_the_index_is_written() {
    // Assignment, compound assignment, a `mut` lend and a write on one
    // branch each blur `xs[i]`: the store may name another element.
    for (name, between) in [
        ("e1001_elem_r3_assigned", "i = 0"),
        ("e1001_elem_r3_compound", "i += 0"),
        ("e1001_elem_r3_mut_lent", "bump(mut i)"),
        ("e1001_elem_r3_one_branch", "if t.len > 5 { i = 0 }"),
    ] {
        let src = format!(
            "fn bump(mut k: int) {{\n    k = k + 0\n}}\n\
             fn main() -> !int {{\n    \
                 var xs = List[List[int]]()\n    \
                 (mut xs).push([1])\n    \
                 (mut xs).push([2, 3])\n    \
                 var i = 1\n    \
                 var t = move xs[i]\n    \
                 {between}\n    \
                 xs[i] = take t\n    \
                 xs[1].len\n\
             }}\n"
        );
        insta::assert_snapshot!(name, render_mem(&src));
    }
}

#[test]
fn e1001_r3_does_not_carry_across_loop_iterations() {
    // The loop variable is re-bound every iteration: a store in a later
    // iteration names a later element, so the one moved earlier stays
    // moved at the loop's exit.
    snap(
        "e1001_elem_r3_loop",
        "fn main() -> !int {\n    \
             var xs = List[List[int]]()\n    \
             (mut xs).push([1])\n    \
             (mut xs).push([2])\n    \
             var keep = List[int]()\n    \
             for i in 0..2 {\n        \
                 if i == 0 {\n            \
                     keep = move xs[i]\n        \
                 } else {\n            \
                     xs[i] = [9]\n        \
                 }\n    \
             }\n    \
             keep.len + xs[0].len\n\
         }\n",
    );
}

#[test]
fn clean_r3_a_str_key_local_revives() {
    // eg01b: R3 is stated over any `Copy` local (the maintainer's
    // ruling), so the store through the same unwritten `str` key
    // revives the value read out of it — eg01 refused this (E1001).
    snap(
        "clean_elem_r3_str_key",
        "fn main() -> !int {\n    \
             var m = Map[str, List[int]]()\n    \
             m[\"a\"] = [1]\n    \
             let k = \"a\"\n    \
             var v = m[k] else List[int]()\n    \
             (mut v).push(2)\n    \
             m[k] = take v\n    \
             let w = m[\"a\"] else List[int]()\n    \
             w.len\n\
         }\n",
    );
}

#[test]
fn e1001_r3_over_a_copy_key_does_not_hold_after_the_key_is_written() {
    // eg01b's blur twins: a `str`, `char` or `bool` key written between
    // the read-out and the store blurs `m[k]` exactly as an integer
    // index does — the store may name another key.
    for (name, ty, init, lit, between) in [
        (
            "e1001_elem_r3_str_key_assigned",
            "str",
            "\"a\"",
            "\"a\"",
            "k = \"b\"",
        ),
        (
            "e1001_elem_r3_char_key_assigned",
            "char",
            "'a'",
            "'a'",
            "k = 'b'",
        ),
        (
            "e1001_elem_r3_bool_key_mut_lent",
            "bool",
            "true",
            "true",
            "flip(mut k)",
        ),
    ] {
        let src = format!(
            "fn flip(mut b: bool) {{\n    b = !b\n}}\n\
             fn main() -> !int {{\n    \
                 var m = Map[{ty}, List[int]]()\n    \
                 m[{lit}] = [1]\n    \
                 var k = {init}\n    \
                 var v = m[k] else List[int]()\n    \
                 (mut v).push(2)\n    \
                 {between}\n    \
                 m[k] = take v\n    \
                 let w = m[{lit}] else List[int]()\n    \
                 w.len\n\
             }}\n"
        );
        insta::assert_snapshot!(name, render_mem(&src));
    }
}

#[test]
fn clean_r3_a_pool_handle_local_revives() {
    // eg01b: a handle is a `Copy` value, so `move p[h]` … `p[h] = take
    // t` with `h` unwritten revives the element.
    snap(
        "clean_elem_r3_pool_handle",
        "struct Node { tags: List[int] }\n\
         fn main() -> !int {\n    \
             region r: pool(Node) {\n        \
                 var p = Pool[Node]()\n        \
                 let h = (mut p).reserve()\n        \
                 (mut p).init(h, Node { tags: [1] })\n        \
                 var t = move p[h]\n        \
                 (mut t.tags).push(2)\n        \
                 p[h] = take t\n        \
                 p[h].tags.len\n    \
             }\n\
         }\n",
    );
}

// ------------------------- s184: `mut` parameters at return (#464) ----
//
// `[mem.tier0.mode.mut]`: a `mut` parameter is initialized at every
// return of the callee. Each refused shape compiled before s184, and
// native handed the caller the moved buffer (wolf-lang#464).

#[test]
fn e1001_mut_param_moveout_at_return() {
    for (name, params, body) in [
        (
            "e1001_mut_param_moveout_whole",
            "mut xs: List[int]",
            "var t = move xs\n    (mut t).push(9)",
        ),
        (
            "e1001_mut_param_moveout_field",
            "mut s: S",
            "var t = move s.tags\n    (mut t).push(9)",
        ),
        (
            "e1001_mut_param_moveout_elem",
            "mut xss: List[List[int]]",
            "var t = move xss[0]\n    (mut t).push(9)",
        ),
        (
            "e1001_mut_param_moveout_map",
            "mut m: Map[str, List[int]], k: str",
            "var v = m[k] else List[int]()\n    (mut v).push(9)",
        ),
        (
            "e1001_mut_param_moveout_one_path",
            "mut xs: List[int], c: bool",
            "var t = move xs\n    (mut t).push(9)\n    if c {\n        xs = t\n    }",
        ),
        (
            "e1001_mut_param_moveout_take_onward",
            "mut xs: List[int]",
            "let n = sink(take xs)",
        ),
        (
            "e1001_mut_param_moveout_partial_restore",
            "mut s: S",
            "var t = move s\n    s.name = \"b\"\n    let n = sink(take t.tags)",
        ),
    ] {
        let src = format!(
            "struct S {{ name: str, tags: List[int] }}\n\
             fn sink(take xs: List[int]) -> int {{\n    xs.len\n}}\n\
             fn f({params}) {{\n    {body}\n}}\n"
        );
        snap(name, &src);
    }
}

/// Every return, the `?` error edge included: the store back sits after
/// a `?` that can leave first.
#[test]
fn e1001_mut_param_moveout_on_the_error_edge() {
    snap(
        "e1001_mut_param_moveout_error_edge",
        "fn get(n: int) -> int ! {empty} {\n    if n == 0 {\n        return empty\n    }\n    n\n}\n\
         fn f(mut xs: List[int], n: int) -> int ! {empty} {\n    \
             var t = move xs\n    \
             let v = get(n)?\n    \
             (mut t).push(v)\n    \
             xs = t\n    \
             v\n}\n",
    );
}

/// An early `return` before the store back.
#[test]
fn e1001_mut_param_moveout_on_an_early_return() {
    snap(
        "e1001_mut_param_moveout_early_return",
        "fn f(mut xs: List[int], n: int) -> int {\n    \
             var t = move xs\n    \
             if n == 0 {\n        return 0\n    }\n    \
             xs = t\n    \
             n\n}\n",
    );
}

/// A use of the moved parameter in the body already reports the move:
/// one root cause, one diagnostic — no second report at the return.
#[test]
fn e1001_mut_param_used_after_move_reports_once() {
    snap(
        "e1001_mut_param_used_after_move_once",
        "fn f(mut xs: List[int]) -> int {\n    \
             var t = move xs\n    \
             (mut t).push(9)\n    \
             xs.len\n}\n",
    );
}

/// The store back revives every shape, and a `defer` store runs on
/// every return; a `take` parameter and a local may leave moved-out.
#[test]
fn clean_mut_param_stored_back_before_every_return() {
    snap(
        "clean_mut_param_stored_back",
        "struct S { name: str, tags: List[int] }\n\
         fn sink(take xs: List[int]) -> int {\n    xs.len\n}\n\
         fn whole(mut xs: List[int]) {\n    var t = move xs\n    (mut t).push(9)\n    xs = t\n}\n\
         fn field(mut s: S) {\n    var t = move s.tags\n    (mut t).push(9)\n    s.tags = t\n}\n\
         fn elem(mut xss: List[List[int]]) {\n    var t = move xss[0]\n    (mut t).push(9)\n    xss[0] = t\n}\n\
         fn map(mut m: Map[str, List[int]], k: str) {\n    var v = m[k] else List[int]()\n    (mut v).push(9)\n    m[k] = take v\n}\n\
         fn both_paths(mut xs: List[int], c: bool) {\n    var t = move xs\n    if c {\n        xs = t\n    } else {\n        xs = List[int]()\n    }\n}\n\
         fn taken(take xs: List[int]) -> int {\n    sink(take xs)\n}\n\
         fn local() -> int {\n    let xs = [1]\n    sink(take xs)\n}\n",
    );
}

#[test]
fn header_methods_beside_a_moved_or_claimed_element_stay_silent() {
    // s185 (wolf-lang#474, wolffe-lang/wolf-interp#149, ruled
    // 2026-09-30): `len`, `count` and `is_empty` read only a builtin
    // container's header — the place `xs.len` denotes — so after an
    // element moves out (a `List` element, one level down, a `Map`
    // value read out) and beside a `mut` element claim they conflict
    // with nothing. Every other method reads the whole receiver (the
    // next fixture).
    snap(
        "clean_header_methods_beside_an_element",
        "fn bump(mut a: int, n: int) {\n    \
             a = a + n\n\
         }\n\
         fn flag(mut a: int, e: bool) {\n    \
             if e { a = 0 } else { a = a + 1 }\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [[1], [2, 3]]\n    \
             let a = move xs[0]\n    \
             var g = [[[1], [2]], [[3]]]\n    \
             let b = move g[0][1]\n    \
             var m = Map[str, List[int]]()\n    \
             m[\"a\"] = [1]\n    \
             let c = m[\"a\"] else List[int]()\n    \
             var ys = [1, 2, 3]\n    \
             bump(mut ys[0], ys.count())\n    \
             flag(mut ys[1], ys.is_empty())\n    \
             let n = xs.count() + g[0].count() + g.count() + m.count() + m.len()\n    \
             if xs.is_empty() || g.is_empty() || m.is_empty() { 1 } else { n - a.len - b.len - c.len - 5 }\n\
         }\n",
    );
}

#[test]
fn e1001_whole_read_methods_after_a_moved_part() {
    // The other half of the ruling: `push`, `get`, a slice and an impl
    // method's `self` (even one NAMED `count`) take the whole receiver,
    // a prefix of the moved part. And a whole move still reaches the
    // header: `count()` after `move ys` is E1001 at `ys.len`.
    snap(
        "e1001_whole_read_methods_after_a_moved_part",
        "struct P { x: List[int], y: int }\n\
         impl P {\n    \
             fn count(self) -> int { self.y }\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [[1], [2, 3], [4]]\n    \
             let a = move xs[0]\n    \
             (mut xs).push([5])\n    \
             let b = xs.get(1) else [9]\n    \
             let s = xs[1..3]\n    \
             var p = P { x: [1], y: 2 }\n    \
             let c = move p.x\n    \
             let d = p.count()\n    \
             var ys = [1]\n    \
             let zs = move ys\n    \
             let e = ys.count()\n    \
             a.len + b.len + s.len + c.len + d + zs.len + e\n\
         }\n",
    );
}

// ------------------------------------------ s186: wolf-lang#476 --

#[test]
fn e1002_writes_and_moves_inside_a_later_arguments_claim() {
    // s186 (wolf-lang#476, `[mem.tier0.excl.4]`): arguments are
    // two-phase, so a later argument may read the claimed place but may
    // not write it (a store, a compound store) or move it. One report
    // per access.
    snap(
        "e1002_writes_and_moves_inside_a_later_arguments_claim",
        "fn bump(mut a: int, n: int) {\n    \
             a = a + n\n\
         }\n\
         fn grow(mut ys: List[int], n: int) {\n    \
             (mut ys).push(n)\n\
         }\n\
         fn main() -> !int {\n    \
             var a = 1\n    \
             bump(mut a, {\n        \
                 a = 5\n        \
                 1\n    \
             })\n    \
             bump(mut a, {\n        \
                 a += 2\n        \
                 1\n    \
             })\n    \
             var xs = [1, 2]\n    \
             grow(mut xs, {\n        \
                 var t = move xs\n        \
                 xs = [9]\n        \
                 t.len\n    \
             })\n    \
             a\n\
         }\n",
    );
}

#[test]
fn clean_two_phase_reads_of_a_claimed_place() {
    // s186 (ruled 2026-09-30, `[mem.tier0.excl.4]`): every read a later
    // argument makes of the claimed place ends before the claim takes
    // effect — a direct `Copy` read (D39's E1002 through 0.2.19), a
    // member read, a read lend one call down, a whole-reading receiver,
    // the header under a WHOLE claim, an operand, a `Map`, and string
    // interpolation of a field.
    snap(
        "clean_two_phase_reads_of_a_claimed_place",
        "struct Fl { store: str, n: int }\n\
         fn bump(mut a: int, n: int) {\n    \
             a = a + n\n\
         }\n\
         fn grow(mut ys: List[int], n: int) {\n    \
             (mut ys).push(n)\n\
         }\n\
         fn total(ys: List[int]) -> int {\n    \
             ys.len\n\
         }\n\
         fn msize(m: Map[str, int]) -> int {\n    \
             m.len\n\
         }\n\
         fn mput(mut m: Map[str, int], n: int) {\n    \
             m[\"z\"] = n\n\
         }\n\
         fn fail(mut fl: Fl, msg: str) -> int {\n    \
             fl.n = fl.n + 1\n    \
             msg.len\n\
         }\n\
         fn main() -> !int {\n    \
             var a = 1\n    \
             bump(mut a, a)\n    \
             var xs = [1, 2, 3]\n    \
             grow(mut xs, xs.len)\n    \
             bump(mut xs[0], total(xs))\n    \
             bump(mut xs[0], xs.get(1) else 9)\n    \
             grow(mut xs, xs.count())\n    \
             bump(mut a, a + 1)\n    \
             var m = Map[str, int]()\n    \
             mput(mut m, msize(m))\n    \
             var fl = Fl { store: \"/var\", n: 0 }\n    \
             let k = fail(mut fl, \"under {fl.store}\")\n    \
             a + xs.len + m.len + k\n\
         }\n",
    );
}

#[test]
fn clean_disjoint_and_earlier_reads_beside_a_claim() {
    // What the #476 scan must leave alone: a different element, the
    // header beside an ELEMENT claim (#474), a read before the claim
    // began, a disjoint field, and a closure's body (it does not run
    // where it is written — its capture is the access, and a closure
    // capturing nothing claimed is clean).
    snap(
        "clean_disjoint_and_earlier_reads_beside_a_claim",
        "struct R { xs: List[int], k: int }\n\
         fn bump(mut a: int, n: int) {\n    \
             a = a + n\n\
         }\n\
         fn bump2(n: int, mut a: int) {\n    \
             a = a + n\n\
         }\n\
         fn id(n: int) -> int {\n    \
             n\n\
         }\n\
         fn total(ys: List[int]) -> int {\n    \
             ys.len\n\
         }\n\
         fn apply(mut a: int, f: fn(int) -> int) {\n    \
             a = f(a)\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2, 3]\n    \
             bump(mut xs[0], id(xs[1]))\n    \
             bump(mut xs[0], id(xs.count()))\n    \
             bump2(total(xs), mut xs[0])\n    \
             var r = R { xs: [4], k: 0 }\n    \
             bump(mut r.k, total(r.xs))\n    \
             apply(mut xs[1], fn(n: int) n + 1)\n    \
             xs[0] + r.k\n\
         }\n",
    );
}

// ------------------------------------------ s186: wolf-lang#466 --

#[test]
fn nested_fn_parameters_carry_their_modes() {
    // s186 (wolf-lang#466): a nested fn's `mut` parameter is spelled
    // `mut` at the call (E1007 when omitted), checked at the nested
    // fn's own return (#464's E1001), and a parameter without a mode
    // is `read` — E1014 to write, lent when returned (s165's E1002).
    snap(
        "nested_fn_parameters_carry_their_modes",
        "fn main() -> !int {\n    \
             fn push9(mut xs: List[int]) {\n        \
                 (mut xs).push(9)\n    \
             }\n    \
             fn drain(mut xs: List[int]) {\n        \
                 var t = move xs\n        \
                 (mut t).push(9)\n    \
             }\n    \
             fn write(xs: List[int]) {\n        \
                 (mut xs).push(9)\n    \
             }\n    \
             fn back(xs: List[int]) -> List[int] {\n        \
                 xs\n    \
             }\n    \
             var xs = [1]\n    \
             push9(mut xs)\n    \
             push9(xs)\n    \
             drain(mut xs)\n    \
             write(xs)\n    \
             let ys = back(xs)\n    \
             xs.len + ys.len\n\
         }\n",
    );
}

// ------------------------------------------ s192: wolf-lang#487 --

#[test]
fn e1002_a_mut_receiver_written_moved_or_claimed_in_its_own_arguments() {
    // s192 (wolf-lang#487, `[mem.tier0.excl.4]`): a `mut` receiver is
    // the call's first argument, so its own arguments may not write it,
    // move it, or claim it again — directly, through an element or a
    // field receiver, through a prefix of the claimed place, or through
    // a view set's own field. One report per access, naming the
    // receiver.
    snap(
        "e1002_a_mut_receiver_written_moved_or_claimed_in_its_own_arguments",
        "struct Out { bytes: List[int], class_off: List[int] }\n\
         struct V { x: int, y: int, z: int }\n\
         impl V {\n    \
             fn set_x(mut self.{x}, n: int) {\n        \
                 self.x = self.x + n\n    \
             }\n\
         }\n\
         fn drain(mut ys: List[int]) -> int {\n    \
             ys.len\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2]\n    \
             (mut xs).push({\n        \
                 xs = [9]\n        \
                 5\n    \
             })\n    \
             (mut xs).push({\n        \
                 var t = move xs\n        \
                 xs = [9]\n        \
                 t.len\n    \
             })\n    \
             (mut xs).push({\n        \
                 (mut xs).push(7)\n        \
                 5\n    \
             })\n    \
             (mut xs).push(drain(mut xs))\n    \
             var ys = [[1], [2]]\n    \
             (mut ys[0]).push({\n        \
                 ys[0] = [7]\n        \
                 5\n    \
             })\n    \
             (mut ys[0]).push({\n        \
                 ys = [[7]]\n        \
                 5\n    \
             })\n    \
             var out = Out { bytes: [7], class_off: [1] }\n    \
             (mut out.class_off).push({\n        \
                 out.class_off = [9]\n        \
                 5\n    \
             })\n    \
             (mut out.class_off).push({\n        \
                 out = Out { bytes: [], class_off: [9] }\n        \
                 5\n    \
             })\n    \
             var p = V { x: 1, y: 2, z: 3 }\n    \
             (mut p).set_x({\n        \
                 p.x = 50\n        \
                 1\n    \
             })\n    \
             xs.len + p.x\n\
         }\n",
    );
}

#[test]
fn clean_reads_and_disjoint_writes_beside_a_mut_receiver() {
    // What s192's receiver seeding must leave alone: header and whole
    // reads, a block writing only its own local, bu12's sibling-field
    // read (`tr.lu:528`), a sibling-field write, a different literal
    // element written, a write after the call, a view set's other field
    // written, and a closure made and called inside the argument (its
    // body does not run where it is written; its capture is a read).
    snap(
        "clean_reads_and_disjoint_writes_beside_a_mut_receiver",
        "struct Out { bytes: List[int], class_off: List[int] }\n\
         struct V { x: int, y: int, z: int }\n\
         impl V {\n    \
             fn set_x(mut self.{x}, n: int) {\n        \
                 self.x = self.x + n\n    \
             }\n\
         }\n\
         fn total(ys: List[int]) -> int {\n    \
             ys.len\n\
         }\n\
         fn main() -> !int {\n    \
             var xs = [1, 2]\n    \
             (mut xs).push(xs.len)\n    \
             (mut xs).push(total(xs))\n    \
             (mut xs).push({\n        \
                 var t = xs.len\n        \
                 t = t + xs[0]\n        \
                 t\n    \
             })\n    \
             (mut xs).push({\n        \
                 let f = fn() xs.len\n        \
                 f()\n    \
             })\n    \
             var out = Out { bytes: [7, 8], class_off: List[int]() }\n    \
             (mut out.class_off).push(out.bytes.len)\n    \
             (mut out.class_off).push({\n        \
                 out.bytes = [4]\n        \
                 1\n    \
             })\n    \
             var ys = [[1], [2]]\n    \
             (mut ys[0]).push({\n        \
                 ys[1] = [7]\n        \
                 ys[1].len\n    \
             })\n    \
             xs = [9]\n    \
             (mut xs).push(xs[0])\n    \
             var p = V { x: 1, y: 2, z: 3 }\n    \
             (mut p).set_x({\n        \
                 p.z = 9\n        \
                 p.x\n    \
             })\n    \
             xs.len + p.x\n\
         }\n",
    );
}
