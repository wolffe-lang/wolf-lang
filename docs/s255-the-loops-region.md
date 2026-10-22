# s255 — the loop's region (wolf-lang#839, #856, #821, #844)

Contract: wolffe-lang/wolf `sprints/compiler/s255-the-loops-region.md` at
planning `0262725`; rules `sprints/RULES.md`; ruling #112 (the
maintainer, 2026-10-22): `[mem.region.outer-write]` as the contract words
it, `List.reserve` and `List.reset`, yew's editor loop and aspen's walk as
the before/after witnesses on `s255-witness` branches never merged by this
lane. Branch point wolf-lang trunk `0cb954a8` (s254 merged); s253 (#847)
and s251 (#823) land ahead and this branch rebases over them.

This note is written in two passes. §1 (the cause) and §3 (the
prediction) were written before the first change, in the commit that
adds this file; the measurements in §5 are filled in as they are taken,
each with its evidence path on kasumi.

## 1. The cause: how E1010 decides today

Read at `0cb954a8`. Three shapes are refused, by two different paths.

### 1.1 A call with a `mut` argument inside the block (yew #839, aspen #856)

yew's witness (`notes/yw03-evidence/witness/turn-region/with-region.lu`
at yew `77c10839`) and aspen's two probes
(`notes/ap01-evidence/probes/region-per-level.lu`,
`region-first-class.lu` at aspen `476edee`) all have one shape:

```wolf
region turn {
    render(mut s, i)      // the callee pushes bytes and writes ints
}
```

`wolf_mem::lower::Lowerer::eval_call` (crates/wolf_mem/src/lower.rs,
"A real call: the callee's default scheme (Cyclone rules) says its
result — and anything it keeps — lands in *its* caller's region") mints
one `SiteKind::CallResult` site named `render(..)` in the AMBIENT region
of the call — `turn` — whenever the call has a non-`Copy`, non-scalar-only
`mut` argument (`mut_targets`), and `hold`s that site in the argument's
root local (`for place in mut_targets { self.hold(l, &call_val, e.span) }`).
`flow_kept_arguments` (s227, wolf-lang#660) then makes the same local
SHARE the sites of every other argument that can name bytes. The
callee's body is never consulted: the rule is read off the signature
(`[mem.region.escape]`, s227: "read off the call's SIGNATURE and never
the callee's body … a summary of what each callee keeps would make it
exact for a direct call to a fn in the same package, and is the debt").

At the block's closing brace `eval_region_block` calls
`sweep_region_close`, which walks `holds` for every local declared
before the block (`outer_mark`) and reports E1010 for any held site whose
region `rt.same(…, rid)` — so `s`, declared outside, "still holds a value
allocated in region `turn` when the region is freed", pointing at the
call as "allocated here, into region `turn`". That is the text in yew's
`run.txt` and aspen's note §6.8.

What the callee does is irrelevant to this path: `render` stores a
`byte` per push and two `int`s. The refusal is the signature rule's
conservatism, not a leak. (For a scalar-only argument s252's
`[mem.region.scalar]` skips the site — `scalar_muts` — which is why
`region_scalar_mut.lu` runs; a struct holding a `List` is not scalar-only.)

### 1.2 A `push` into an outer list inside the block (aspen's "even a byte push")

```wolf
region tmp { (mut keep).push(7) }
```

`push` is a builtin method whose receiver is `mut`; `eval_call` sees the
receiver place in `surface.mut_args`, so the same `CallResult` site
(`push(..)`) is minted in the ambient region and held by `keep` — the
list's GROWTH is modelled as an allocation at the push site, in the
region open at the push. At the native tier that model is wrong since
s76: `wolf_rt::list::ListHdr` carries the list's birth region and
`push_raw` grows "in the list's OWN region, not whatever is ambient now:
a region frees as a unit, so the buffer must not migrate"; the checked
machine's `list_region` charges pushes "to the region that owns the
storage, not the region open at the push site" (ubcheck.rs, s131).
The static model lags the machines by one rule: the growth belongs to the
list, and only the ELEMENT can be the block's.

### 1.3 A field store of a value built in the block (G18's original shape)

```wolf
region turn { let t = "row {i}"; s.title = t }      // or s.names = copy xs
```

`lower_assign_inner` → `flow_store`: an embedding store demands
co-location with the container's first held site
(`demand_store(val, target, …)`), and `RegionTable::unify(target, sr)`
answers `Conflict` for two rigid bindings (`Binding::Caller` for a value
built in `main`'s ambient, `Binding::Local(turn)` for the string) — E1004
"this value is allocated in region `turn`, but it is being stored into a
value living in the caller's region". `copy` does not repair it:
`eval_prefix(CopyKw)` mints the copy's `Lit` site in the AMBIENT region
(the dying one), and for a `str` carries the operand's sites instead
(ruling #35: a `str` copy shares the bytes). E1010's explain text says
exactly this ("`copy` inside the block does not help — a copy is a fresh
allocation in the *current* ambient region, which is still the dying
one").

### 1.4 Where the store is lowered (what the machines actually do)

- A `mut` parameter is `LocalBind::MutRef { ptr, region }` in wolf_wir: a
  pointer to the caller's slot; a field store through it writes the
  caller's words (a `str` field is its two-word view, a `List` field its
  header pointer). The struct carries no region: "region identity is a
  static type fact with zero runtime representation"
  (`[mem.region.create.4]`).
- `copy e` lowers to `deep_copy_in` (wolf_wir/src/lower.rs): a `List` is
  `__wolf_rt_list_copy` — "a fresh list in the AMBIENT region" — then
  each heap-reaching element copied; a `str` "stays free" (shared, not
  materialized); a `Map` whose values reach the heap refuses. The only
  materializing copy in the tree is `deep_copy_out`, written for
  `copy region { }` (s216): every `str` re-made by
  `__wolf_rt_str_repeat(s, 1)` in the ambient, "and a `List[str]`
  therefore copies element by element".
- A plain `push(v)` is `deep_copy_in(v)` (ambient) then an inline store
  into `data[len]` when there is room, else `__wolf_rt_list_push`, whose
  growth is in the header's region. So a `str` pushed from inside a block
  into an outer list stores a view of the block's bytes: refusing it is
  right, and `copy` as it stands would not change that (shared).
- The ambient region is a per-thread slot (`AMBIENT_REGION`,
  wolf_rt/src/native.rs): `region { }` and `in r { }` emit
  `__wolf_rt_region_ambient_enter(handle)` and keep the previous handle
  in `ScopeFrame::ambient_prev`, restored by `ambient_leave` on every
  exit edge. A function's entry ambient is therefore the saved
  `ambient_prev` of its outermost region scope, and the ambient at any
  binding's declaration is the `ambient_prev` of the first region scope
  opened after that binding's scope. No new runtime symbol is needed to
  name "the outer value's region" for a flat store; for a container the
  header's fifth word IS its region.

### 1.5 What the ledger sees (the checked machine)

`charge_str` charges a built `str` to `self.ambient.last()`; `mint_list`
charges the ambient and records the list's birth region in
`list_region`; a `push` charges `slot_bytes` to the BIRTH region
(`"push" =>` arm); `deep_copy` re-mints lists in the ambient; `copy_out`
(the `copy region` copy) charges every `str` reached to the ambient and
re-mints every list and map there. `free_region` disables raw-pointer
tags only: a `Value::Str` is a host string and a `Value::List` an index
into the machine's arena, so neither traps when its region dies. The
checked machine's PROOF of placement is therefore the ledger — a row
reads `region_bytes(outer)` and `region_bytes(turn)` and says which one
the copies landed in — exactly as `region_os_cwd_charged.lu` (s254) and
`region_net_read_charged.lu` (s228) prove theirs.

### 1.6 The contract's fourth write form

The contract lists "a captured `mut`". There is no such write today: a
closure BORROWS its captures (`eval_closure`: "the borrow begins where the
env is built"; `check_capture_write` refuses `push`/`pop`/`clear` on a
captured list by name — "borrow-only closures"), so the clause names the
three forms that exist (a field store or assignment, a store through a
`mut` parameter, a container store) and says why the fourth is absent.

### 1.7 What the rule as worded cannot reach — the finding

`[mem.region.outer-write]` as worded admits WRITES inside the block. Both
witnesses write only through CALLS:

- yew's turn (`src/edit/loop.lu` `loop_run`) is ten calls with `mut ed`:
  `loop_seed_probe`, `tty_wait(mut ed.tty)`, `handle_signal`,
  `loop_read_input`, `input_next(mut ed.input, …)`,
  `loop_dispatch_event`, `dispatch_tick`, `ed_timers_fire`, `ed_layout`,
  `ed_render`. A `region turn { }` around the turn is E1010 at the first
  of them under §1.1, whatever the rule says about stores.
- aspen's level (`src/walk/walk.lu` `walk_dir`) recurses
  `walk_dir(mut w, o, cp, take sub, depth + 1)` INSIDE the loop over the
  level's entries, and every byte it writes goes through `l_entry(mut w,
  …)` → `text.entry(mut w.t, …)` → `(mut t.o.buf).push(b)`. A region per
  level must contain the recursive call; E1010 at it.

So, under the rule as worded, neither loop can open its region, and the
contract's deliverable 3 ("the editor loop opening a region per turn", "a
region per directory level") is unbuildable. The rule is SOUND as worded
(§3 P2): it is insufficient. The orchestrator's brief says to stop for an
unsound rule; this is a ruling instead, asked in §3 R1 with the
recommendation implemented (RULES §9).

## 2. Inputs, verified

- wolf-lang trunk `0cb954a8` (`git rev-parse origin/trunk`, 2026-10-22):
  s254's merge. #847 (s253, head `b308dafc`) and #823 (s251, head
  `08a35795`) are OPEN; both touch `CHANGELOG.md`, `spec/anchors.json` and
  `crates/wolf_wir/tests/snapshots/lower_corpus__corpus_lowering_ledger.snap`;
  s253 also touches `ubcheck.rs` and `wolf_wir/src/lower.rs`. Rebases at
  each landing.
- Anchors 681 (`spec/anchors.json`, `version` 1); `RT_SYMBOLS` 244
  (`crates/wolf_codegen_clif/src/translate.rs`); PAIRING lupin 0.1.55 pin
  `294d626`; workspace version 0.2.34.
- Rebased onto `b308dafc` (s253 merged, 2026-10-22, the orchestrator's
  word) after the prediction was committed: s253 touched none of
  `crates/wolf_mem/src/lower.rs`, `regions.rs`, `wolf_rt/src/list.rs`,
  `map.rs`, `native.rs` (`git diff --stat 0cb954a8 b308dafc` over them
  is empty), so the `0cb954a8` dist built on kasumi stays the "before"
  compiler for every measurement. At `b308dafc` the counts are anchors
  686, `RT_SYMBOLS` 250, host table 160 (s253's), and P8's deltas are
  read from there: anchors 686 → 690 (`mem.region.outer-write`,
  `type.list.storage`, `type.list.reserve`, `type.list.reset`: one more
  than predicted, the subsection's own anchor), `RT_SYMBOLS` 250 → 251.
  s251 (#823) lands later; one more rebase then.
- yew trunk `77c10839` (`ab8bab0` is its ancestor: yw02's head); aspen
  trunk `476edee` (ap01 merged 2026-10-22). Both public.
- The four issues, read in full: #839 (yew, the editor loop, 337 B a key /
  631 B a line move), #856 (aspen, 60–158 MB against C aspen's
  2.6–18.7), #821 (a `List` replacing a field is held until the region
  ends; 670 KB per edit of a 1 MiB buffer), #844 (`reserve` and a bulk
  append). s252's PR #824 (merge `50e8d0f8`) and s254's #848 (`0cb954a8`)
  read in full. Correction to the contract: there are no `docs/s252-*`
  or s254 notes in wolf-lang (`docs/` holds `s240-the-group-signal.md`
  only); their PR bodies and spec paragraphs are the record.
- The guide ("the guide's memory paragraph") is
  `AN_AGENTS_GUIDE_TO_WRITING_GOOD_WOLF.md` §"Memory: let the regions
  work" in the PLANNING repo, not in wolf-lang; its amended paragraph is
  handed back in the report for the orchestrator to apply (this lane
  writes nothing to the planning repo).
- `List.clear` already keeps the buffer on every machine
  (`__wolf_rt_list_clear` sets `len = 0`; ubcheck `Vec::clear`), and the
  spec promises nothing about it. std's `list.extend` (wolf-std
  `std/list/list.lu:481`) is #844's bulk append, a loop of pushes.
  Nothing in the org spells `.reserve(` or `.reset(` on a `List`
  (`pool.reserve()` is `Pool`'s).
- s252's census: kasumi `~/lanes/s252/measure.py`, `m-base.tsv`,
  `m-head.tsv` (boreutils, lobo, pelt, wolf-std, yew at their 2026-10-20
  trunks; 1,013 files; `conform-run --json --phase=mem`). This lane adds
  aspen and re-pins every tree at its current trunk.
- kasumi: `/home` 31 GB free at start, load 1.1, archlinux:base-devel
  (python 3.14, gcc 16, taskset) and ubuntu:24.04 images present; the
  0.2.34 wolf and lupin 0.1.55 archives under `~/lanes/s254/tc/`; ap01's
  C aspen binary at `~/lanes/ap01/aspen/.oracle/c-aspen/aspen`; yw03's
  yew clone and sagitta oracle under `~/lanes/yw03/`.

## 3. Prediction (before the first change)

**P1 (the cause).** §1 as read: the witnesses' E1010 is the call-site
site of §1.1, the aspen byte push is §1.2, and the field-store shape is
E1004 with `copy` lowered into the dying region (§1.3). The rows written
for each shape are red at `0cb954a8` with exactly those codes: E1010 for
the call and the push rows, E1004 for the field-store row.

**P2 (soundness of the rule as worded).** Sound, with "the outer value's
region" made precise two ways, both runtime-known: a container store (a
`push`, an index or `Map` store — the key included — a `List` or `Map`
field or binding replaced) copies into the CONTAINER's birth region (the
header's region word, the one its growth already uses); any other store
copies into the region that was ambient where the outer binding was
declared (a `mut` parameter's: the function's entry ambient, which is the
caller's region). The `copy` in that position is the MATERIALIZING copy
`copy region { }` makes (`deep_copy_out`: every `str` reached re-made,
every `List`/`Map` a fresh buffer), not ruling #35's sharing copy —
otherwise a copied `str` would still view the block's bytes. Why the two
targets are sound: a container's elements live where its buffer does
(`push_str` and the growth rule already say so), and the entry ambient
of a callee outlives every `mut` parameter's storage because the caller's
own call site pins the argument to its ambient (§1.1) — the pin is what
makes the entry ambient safe. A value placed by `in r { }` and then
written from a block is still E1004 (the copy lands at the binding's
ambient, not in `r`; the checker's `demand_store` unifies and conflicts):
strict, not unsound. No byte of the block's region can be reached from an
admitted store: a scalar is its own words; a materialized copy shares
nothing with the block; a container's growth is the container's. The
"captured `mut`" form does not exist (§1.6).

**P3 (what the rule as worded reaches).** Nothing at the loop level of
either witness (§1.7). yew's statusline leaf alone (`statusline_draw` →
`statusline_build` builds `position`, `percent`, `segments`, `body` per
frame) could wrap ITS body, but `grid_puts(mut ed.grid, …)` inside it is
a call with a `mut` argument, E1010 again; aspen's `l_entry` likewise.
Under (A) alone: yew stays 337 / 631 B a key and aspen 59.9 MB.

**R1 asked (implemented as recommended): calls with a `mut` argument
inside the block.** (A) stay E1010; the witnesses are measured with their
regions pushed into whichever leaves have no `mut` call, and the numbers
do not move. (B) a `mut` argument to a parameter the callee's own
lowering proves AMBIENT-CLEAN gains no site and no shared bytes from the
call: clean when, over the callee's whole body, the parameter's root never
holds or shares a site of the callee's entry ambient (the caller's
region) and is never moved from (whole or part: `take`, `move`, a binding
that moves a field out — the alias a move makes would write the caller's
list past the summary). Under the rule's targets that is exactly: scalar
stores, container stores of scalars or of `copy`s (the container's own
region), containers replaced by `copy` (the old header's region), calls
handing the parameter or a place in it to a clean parameter, and the
mutators that store nothing (`pop`, `clear`, `reset`, `reserve`,
`truncate`); a flat field or whole store of anything else — a `copy`
included, since that copy's target is the entry ambient, which IS the
block when the call is inside one — is unclean. Computed as a by-product
of lowering each body (`holds`/`shares` of the parameter's local at the
end, plus the moved-from flag), over the package's call graph in
callee-first order with each recursive SCC iterated from the optimistic
answer to a fixpoint (aspen's `walk_dir` is self-recursive; strres's
least fixpoint is the precedent, s216). A fn value, a trait method, a C
call, a host builtin, a generic callee's unknown-typed store and a
callee outside the build are not clean. **Recommend B.** It pays s227's
named debt in the one place the wall was hit twice, read off the
callee's own lowering (no second classifier), and refuses by name: E1010
at the call names the callee's first unclean store so the port knows
what to spell. Cost: the mem tier lowers a recursive SCC's members once
more per fixpoint step (expected ≤ 2 steps anywhere in the org; measured
in §5).

**R2 asked (implemented as recommended): `reset` beside `clear`.** `clear`
keeps the buffer on every machine today but the spec promises nothing.
(A) `List.reset()` is the spelled promise — `len` to 0, the buffer and
its capacity kept, no allocation on any machine, the next pushes reuse
it — and `clear` keeps its present wording (it empties; what it keeps is
the implementation's); #821's spare-pair swap becomes `reset` then
pushes. (B) amend `clear`'s clause to promise reuse and add no name.
(C) `reset` as (A) and `clear` documented as its alias. **Recommend A**:
one name whose contract is reuse, one whose is emptiness, the maintainer's
ruling names `reset`. `reserve(n)` grows the capacity to at least `n` in
the birth region, `len` unchanged, a no-op when already there; on the
checked machine it charges the slots it adds to the birth region's ledger
once. #844's bulk append is std's `extend` already; a builtin is not
taken here (listed).

**P4 (the witnesses under A+B).**
- yew: the turn region in `loop_run` is admitted once every store through
  `mut ed` reachable from the ten calls is an admitted form. Predicted
  unclean sites the compiler will name, by kind: `str` fields of `Ed`
  written per turn (`dispatch_message`, `drawn_tab_marks`, the message
  texts) and `List[str]`/`Map[str, int]` pushes without `copy` (the grid's
  grapheme `Interner`, the cmdline's and registers' strings) — predicted
  ≤ 25 sites, each repaired by `copy` at the store or a
  `List[byte]` + `reset`. Memory rows (`ptymem.py`, 4,000 keys in the
  container under `taskset -c 0-3`): typing 337 B a key → under 48 B a
  key, line moves 631 B a key → under 48 B a key (what remains is the
  buffer's and undo's own growth, which C yew has too: 176 B a key over
  its first quarter, then flat). If a turn still allocates into `ed`'s
  region through a path the port cannot reach in this lane (a `str` field
  with no `List[byte]` twin), the numbers land between, and the note says
  which path.
- aspen: `walk_dir`'s level region is admitted once `read_level`'s arena
  stores (`w.a.infos.push(copy …)` is already a copy; `w.a.lnks`,
  `w.a.lstats`, `w.seen[ident] = true`'s key) are `copy`-spelled —
  predicted ≤ 8 sites. Peak RSS on the 100,000-file tree (`tools/mem`,
  container, `taskset -c 0-3`, `ulimit -c 0`): the listing 59.9 MB →
  under 12 MB; `-s` 76.8 → under 14; `-J` 129.1 → under 40; `--du` 134.9
  → under 40; `-pugsD` 157.6 → under 30 (the full-tree modes build the
  arena before emitting, so their floor is the arena, not a level).
  C aspen's 2.6–18.7 stays the floor.
- "Which need `reset`": yew's cursor-set and undo-compaction rebuilds
  (#821's remaining shapes, yw02 §4) and the grid's per-frame
  `frame: List[byte]` (already `clear_bytes`); aspen's `Text.pre` /
  `pre_at` level prefixes. None is needed for the per-turn region to be
  admitted; `reset` is what keeps the replaced lists from growing the
  root.

**P5 (the corpus).** No existing row is refused anew. Two rows move:
`region_scalar_list_mut.lu` (s252's stand-in: `grow(mut h)` pushes an
`int` — clean under B) moves to `run` and is rewritten over an un-copied
`str` push so it still refuses, its old shape becoming a positive row;
`region_escape_container.lu` keeps `fail(E1010)` (the whole reassignment
`keep = List[int]()` stays an escape) but reports ONE site, not two — the
push's growth is the container's now; its doc text changes. Every other
`fail(E1004)`/`fail(E1010)` region row keeps its verdict:
`region_kept_arg_*` callees store a `str` field or push a plain `str`
(unclean); `region_conflict_params`, `region_escape_local`,
`region_freeze_write`, `region_scalar_list_store`,
`region_scalar_str_escape` have no call with a clean parameter inside a
block. `cfg_dumps` snapshots: a push site's `[region]` attribution moves
for pushes onto outer lists inside blocks — predicted ≤ 3 snapshots, facts
only. The lowering ledger snapshot gains the new rows' lines.

**P6 (the ledger cost per copy).** Checked machine: a materialized `str`
charges its byte length and a copied `List` its elements' `slot_bytes`
(1 per `byte`, 8 per `int`, the view's 16 per `str` plus the bytes) to
the OUTER region; the block's region charges only the temporaries. Native
and release: the outer arena grows by the 16-byte-rounded bytes of each
`str` (via `str_repeat(s, 1)`), 40 B per list header plus its exact
buffer; `region_bytes(outer)` after N turns ≈ N × that, `region_bytes
(turn)` ≈ the temporaries, read inside the block. The row reads the
relations (`charged_outer > 0`, `charged_turn >= temporaries`,
`live_region_bytes` back to its entry reading after the loop), never the
numbers.

**P7 (lupin 0.1.55).** Cannot mirror the materializing copy: on the
copied-`str` push row it traps `region-fault` at the block's end (it
shares `copy s`'s bytes and tracks them); on the scalar-push row and the
clean-call row (the yew shape) it runs to the same answer (it tracks the
list itself and never pinned the argument — s252 measured
`region_scalar_list_mut.lu` as `exit(0)` there); `reserve` and `reset`
are unresolved methods (its E0xxx by name). Column measured, listed for
the is-lane.

**P8 (counts).** Anchors 681 → 683 (`mem.region.outer-write`,
`type.list.reserve-reset` or its spelling in spec/10); none dropped,
diffed both ways. `RT_SYMBOLS` 244 → 245 (`__wolf_rt_list_reserve`;
`reset` inlines a `len` store as `clear` does, no symbol). Host table
and prelude counts unchanged (methods are not prelude names;
`HOME_MODULES`' list gains the two names so the loader does not fetch
std's `list` for them). The mem tier's time over yew's 58 entries: within
1.1× of trunk (the SCC re-lowering is bounded to recursive members).

**P9 (the downstream census, `--phase=mem`, every `.lu` of boreutils,
lobo, pelt, wolf-std, yew, aspen at their current trunks).** No program
newly refused. Newly accepted: yew's `turn-region/with-region.lu`
(`render` is clean), aspen's `region-per-level.lu` (`level` is clean) and
`region-first-class.lu` (`level`'s `str` push lands in `keep`, a
parameter region, under `in keep { }`). Nothing else moves: no shipped
program had a refused outer write (each was worked around by hand).

**P10 (the plant).** `s255-plant` (never merged) takes the rule, the
summary and the two methods out of wolf_mem, wolf_wir, wolf_rt, sema and
ubcheck and keeps every row and gate; `-f jobs=lint-test` (`tier partial`)
goes red on: the new corpus rows (`header claims phase run but deepest
passing phase is typecheck`), `region_outer_write_lanes` (E1010/E1004 on
every run row, the RSS rows unbuilt, `reserve`/`reset` E0xxx), the
rewritten `region_scalar_list_mut.lu` (green at trunk — it is a refusal
there too; its red is the positive twin), the E1010 note test (no
`[mem.region.outer-write]` text), the lowering ledger, `spec-extract
--check` (anchors), `diag-catalog --check`.

## 4. Evidence index

Filled in as the work lands (kasumi `~/lanes/s255/`).

## 6. The guide's memory paragraph (handed back, not written here)

`AN_AGENTS_GUIDE_TO_WRITING_GOOD_WOLF.md` §"Memory: let the regions
work" lives in the planning repo; this lane writes nothing there. The
paragraph to add after "The fix is to build the value in the caller's
region … (`[mem.region.proc]`).", and the rule-7 line to amend:

> A loop whose turn builds temporaries can open a region per turn and
> still write to the long-lived values the turn updates
> (`[mem.region.outer-write]`, 0.2.35): inside the block, a store into
> a value declared outside it — a field, a `push`, an index or `Map`
> store, a store through a `mut` parameter — is fine when the stored
> value is a scalar or a literal, or is written `copy e` at the store.
> That `copy` is a deep copy made in the outer value's region (the
> list's own for a push, the binding's for a field), so nothing of the
> block survives in it; a plain `str` push or field store from the
> block is still E1010/E1004, and the report names the `copy`. A call
> inside the block may pass the long-lived value `mut` when the callee's
> parameter is ambient-clean — every write through it is one of those
> stores, or a call to another clean parameter, or `pop`/`clear`/
> `reset`/`reserve` — which the compiler proves from the callee's body;
> when it is not, the report names the callee's store that keeps the
> block's bytes (a `str` field assigned a built string is the usual
> one: keep it in a `List[byte]` the value owns instead).
>
> ```wolf
> struct Ed { frame: List[byte], names: List[str], n: int }
>
> fn turn(mut ed: Ed, i: int) {
>     let line = "turn {i}"
>     (mut ed.frame).reset()
>     for b in line.bytes() { (mut ed.frame).push(b) }
>     (mut ed.names).push(copy line)
>     ed.n += 1
> }
>
> fn main() {
>     var ed = Ed { frame: List[byte](), names: List[str](), n: 0 }
>     var i = 0
>     while i < 1000 {
>         region scratch { turn(mut ed, i) }
>         i += 1
>     }
>     print("{ed.n} {ed.names[999]}")
> }
> ```
>
> ```output
> 1000 turn 999
> ```
>
> A long-lived `List` rebuilt every turn is reset and refilled, never
> replaced: a `List` field replaced by assignment keeps its old buffer
> until the value's region ends (`xs.reset()` empties and keeps the
> buffer; `xs.reserve(n)` sizes it once for a count you know).

Rule 7 ("Regions are the allocator"): "Write no region syntax until a
loop's scratch must die per iteration; then one `region scratch { }`
does it — around the turn, even when the turn writes the loop's state
through `mut` parameters (`[mem.region.outer-write]`)."

## 5. Measured

All on kasumi (linux 7.2.8-cachyos, x86-64), `~/lanes/s255/`; the base
compiler is the `0cb954a8` dist (`base/target/dist/…/wolf`, "wolf
0.2.34+dev.0cb954a"), the head the branch's debug driver at each step
and the head dist for the witnesses.

**P1 held** (`ev/rows-trunk.txt`, `ev/witness-trunk.txt`): at the base,
every run row is refused on the checked machine, native and release —
E1010 for the scalar, str-push, struct, call, reset-shape, charged and
refused rows, E1004 for the index and field-store rows (the field row
carries both), E0403 for `reserve`/`reset` — and the three witness
programs (yew's `with-region.lu`, aspen's two probes) are E1010 at the
call ("`s`/`a` still holds a value allocated in region `turn`/`lv`");
yew's no-region control holds 200,000 turns at 8,620 KB (`maxrss`).

**The rows at the head** (`ev/rows-head.txt`, then `ev/rows-head2.txt`
after the type-parameter fix): every run row `exit(0)` with its exact
stdout on all three machines; every refusal its code; `region_escape_
container` one E1010 (was two). lupin 0.1.55's column, as P7 said:
`region-fault` on `_str_push`, `_struct`, `_field`, `_map`,
`_refused_field`, `_call_refused` and the rewritten `region_scalar_list_
mut`'s twin shape; the same answer on `_scalar`, `_index`, `_call`,
`_charged`; `exit(0)` where nothing copied is read back (`_refused`,
`region_scalar_list_mut`); `unsupported` for `reserve`, `reset` and the
reset-shape row.

**The resident set** (`region_outer_write_lanes`, `wait4`'s `ru_maxrss`,
the fixture at 100,000 and 200,000 turns): native `region` 3,660 KB →
3,660 KB, **0 B a turn**; release the same; the control without the
block 5,508 → 8,676 KB (32 B a turn) native, 5,572 → 8,620 KB (31 B a
turn) release. P4's claim for the witness shape holds exactly: the
turn's string (23 bytes, arena-rounded) is what the control keeps.

**The suites** (`t1.log`, `snap.log`): wolf_mem `mem_diagnostics` moved
seven snapshots — six gained the clause's note under an E1004 or E1010
report, and `e1010_copy_shares_str` went `(clean)`: s227's door `in st
{ o = copy d }` is a `copy` at a store into an outer value, so it
materializes into `st` — a mover P5 did not list, sound by the clause,
recorded in the spec's s227 paragraph; the sharing copy bound inside the
block (`let kept = copy d; in st { o = kept }`) keeps s227's E1010
(`e1010_copy_bound_in_block_shares_str`). 144/0 after; wolf_wir, sema,
diag green; `region_copyout_lanes` 10/0, `cwd_region_lanes` 1/0,
`small_gaps_lanes` 5/0, `region_outer_write_lanes` 17/0.

**The downstream census** (`ev/m-base.tsv`, `ev/m-head.tsv`,
`ev/m-diff.txt`; `measure.py` after s252's, aspen added; the six trees
at their 2026-10-22 trunks, 1,154 `.lu` files, `conform-run --json
--phase=mem --std-root` wolf-std's): **no program newly refused**;
movers: aspen's `region-per-level.lu` and `region-first-class.lu`
(`fail(E1010)` → `pass`, as P9 said), and yew's `witness/host-os/
main.lu` (`fail(E0301)` at resolve → `pass`: s253's `os_hostname`, in
the head and not in the `0cb954a8` base — not this lane's). yew's
`turn-region` witness cannot show in the census (its two files share a
directory and build as one module, E0302 on both sides); built alone
(`ev/witness-head.txt`) it runs: `with-region.lu` 200,000 turns at
**2,580 KB** against the no-region control's **8,744 KB**; aspen's
probes run at 3,000 and 3,960 KB. The first census run found the
regression the fix above removes: `std/map/map.lu:134` `m[k] = copy v`
over `V` — 180 programs refused at one span — because the
materializing copy refused a type parameter as `copy region { }` does;
the clause admits one (the per-instantiation lowering keeps refusing an
uncopyable part by name), and the dist's own smoke (`use std.env` on the
checked tier) caught it too.

**Counts**: anchors 686 → 690 (`mem.region.outer-write`,
`type.list.storage`, `type.list.reserve`, `type.list.reset`; key sets
diffed both ways against `b308dafc`: four added, none dropped, none
moved — `regen.log`); `RT_SYMBOLS` 250 → 251; host table 160 and the
prelude unchanged; `docs/diagnostics.md` regenerated (187 codes).

**The verdict's second reading (after the pause).** The first census
and the targeted suites were green, but the gauntlet had not run: at
`994162a1` (the rebase onto s251) `cargo test -p wolf_driver --test
region_kept_arg_lanes` was red on five tests (`gate1.log`: the callee's
push, the generic callee's push, the closure argument's captures, the
`take` argument kept, the copy of a struct sharing a region `str`), and
`conform-run` on s227's `region_kept_arg_take.lu` and
`region_kept_arg_closure.lu` answered `pass` — a soundness hole in R1 as
first implemented, not in the rule as worded. The verdict had read only
the callee's AMBIENT: a `mut` parameter was clean when its root held
no site of the entry ambient. Three things it could not see: (a)
another parameter's value — `(mut ds).push(take d)` makes `ds` keep
`d`'s bytes, whose region is the caller's block when the call sits in
one (`region_kept_arg_take`); (b) a fn value stored flat (`h.f = g`,
`region_kept_arg_closure`), whose captures the callee cannot see at all;
(c) a store under a region PARAMETER's region (`in keep { (mut
a.names).push("…") }`, aspen's `region-first-class.lu`), which the
callee cannot know outlives `a`. Before this lane, the call-site pin
covered all three; a clean verdict skips the pin, so the verdict must
cover them itself. The reading now (`49d6f681`, `6ebc243b`): a
parameter is clean when at exit its root holds or shares only its own
entry sites, sites in its OWN generalized region (its containers'
growth, the copies aimed at it) and static ones — everything else is
the caller's. For (a) the parameter pseudo-sites now RIDE into a `mut`
argument's `shares` at a call inside the callee (they were skipped as
"escaping no region in this frame", true for the escape rules, wrong
for the verdict; they are still never a kept flow to report); for (b)
a fn-typed value stored through a `mut` parameter marks it unclean by
type (`reaches_fn`); (c) falls out of the own-region reading. Probes
(`probes/p3.lu`, `p5.lu`, `p6.lu`, the dump): a `str` parameter stored
flat, or its `copy`, unclean; `push(copy d)` and a scalar push clean;
`h.f = g`, `let k = g; h.f = k`, `h.f = dflt` unclean; `h.n = 1` clean;
self- and mutual recursion clean (the fixpoint); and the dump names the
store a recursive self-call would otherwise mask (`e5`'s `probe_byte`:
its own pinned call hid `t.prefix = kept`). After the fix every
`region_kept_arg_*` and `region_kept_result_*` row is refused again
with its code, `region_conflict_params` keeps E1004,
`region_outer_write_call` and `region_scalar_list_mut` keep theirs, and
`region_str_call_index_store.lu` (s216's, #612's first witness: `st.vals
["x"] = copy v` inside the callee's own block) is a second legitimate
mover to `run` — the same door as `region_copy_shares_str` — with its
gate (`region_str_call_lanes`) and the kept-arg gate's assertion on
`region_copy_shares_str` moved to run assertions that pin lupin's
`region-fault` column. The lowering ledger carries both as `lowers`.

**P5 scored**: `region_scalar_list_mut` and `region_escape_container`
as predicted; two movers P5 did not list, both `copy`-at-an-outer-store
doors (`region_copy_shares_str`, `region_str_call_index_store`); no row
refused anew; the two rows the first verdict let through are back.
**P9 scored** (`ev/m-head3.tsv` against `m-base.tsv`, the frozen fixed
driver, 1,154 files): no program newly refused; newly accepted: aspen's
`region-per-level.lu` (as predicted) and yew's `host-os` witness (s253's
`os_hostname`, not this lane's); aspen's `region-first-class.lu` is NOT
accepted — P9 was wrong there, and rightly so: `level`'s pushes under
`in keep { }` land in a region parameter's region the callee cannot
prove outlives `a` (the caller would have to pass `a` placed in `keep`,
which no signature says yet — the E1004 note's planned surface).

**The witnesses** (yew `s255-witness`, aspen `s255-witness` at
`8fce150`, both pushed, never merged by this lane). aspen builds with
the fixed compiler unchanged (`wit-build-b9.txt`: its six one-line
edits were copies at container stores and a `reset`). yew needed the
dump three times over (`mutclean-yew-d4/d5/d6.txt`: 414 clean / 473
unclean at the pause, 628 / 267 at the build). The turn is TWO region
blocks, not one: `wait` (the poll, the signal, the burst's bytes, the
probe's tick) and `turn` (the timers, the layout, the render), with the
key dispatch between them in the session's region — a command reaches
the text engine (the undo tree, the journal, the registers, std's list
helpers), where ~200 `mut` parameters are still #821's shape, and that
port is yw05's with the dump as its list. What the two blocks needed,
all #821's shape or P4's `str` fields: the message (`Msg.text` →
`List[byte]`, set and cleared in place, `msg_text` for the tests), the
dispatch message and the drawn tab marks (`List[byte]`), the input
buffer's compaction, the probe prefix (four sites) and `pending`, the
oob queue, the grid's three tables, the gutter's sign table (and a sign's
entry copied at its index store), the cursor set's three tables (five
sites, `cset_refill`), the grapheme index (its data/motion pair at four
sites, the pending journal, the spare-pair swap retired for `reset`,
the backing stores), the text iterator's stack, the overlay signature,
the interner and the strmap (a new key `copy`d), the clipboard (two
values assigned in place, the reap list, `clip_init`/`clip_shutdown`
emptied in place rather than `c = clip_new()`) and `feed_bytes` no
longer seeding inside the read (the seeding between the blocks takes
the burst through `tty_pending`). The memory rows follow.
