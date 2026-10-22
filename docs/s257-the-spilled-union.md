# s257 — the spilled union layout (c06's residue: payload slots of differing types)

Lane note. **Fable 5.1. Crawl 54.** Branch `s257` off wolf-lang trunk
`0477ce62`. Contract: `sprints/compiler/s257-the-spilled-union.md`.
§1 was written and committed before the first line of the change; §4
holds the numbers measured after it.

## 1. The prediction

Written before the change, from reading `crates/wolf_wir/src/lower.rs`
(the two refusal sites, `enum_value`, the three pattern-read sites,
`tagged_parts`, the handler binds, `zero_of`, the flat load/store
helpers), `types.rs`, `verify.rs`, `parse.rs`/`print.rs`,
`wolf_backend/src/{layout,abi}.rs`, both emitters' `AggMake`/`AggGet`/
`Load`/`Store` arms and `wolf_mem/src/ubcheck.rs`'s `Value::Enum`.

1. **Per slot, not one blob.** Payload slots are position-unified today
   (`{i64 tag, s0, s1, …}`); only a slot whose types disagree across
   variants or tags becomes a union, and every slot that agrees keeps
   today's bytes. So every program that lowers at `0477ce62` lowers to
   the same WIR and the same objects after the change — the ~1,100
   corpus entries, their snapshots and the differ's counts move by
   nothing but the new rows — and the error-union machinery stays
   positional: `eu.make.err`/`eu.err K`, `tagged_parts`, the prefix
   rule in `propagate_err` (`row widening with non-prefix payload
   slots`) all keep their shape, because a union slot is just another
   slot type. One blob for the whole payload would re-lay every enum
   whose variants differ in arity (`A(int)`, `B(int, int)` — green
   today) and would need a tag-switched copy at every move of the
   value. The cost of per slot is space in the rare shape
   `A(str)`/`B(int, int)` (24 bytes where a blob is 16); nothing in
   the contract is about space.
2. **The type form** is `TypeData::Union(members)`: the distinct member
   types, sorted by `TypeId` so two spellings of one set intern to one
   type; its layout is the largest member's size rounded to the
   strictest member's alignment (`wolf_backend::layout`), and its
   printed form `union{i64, {ptr, i64}}`. Two value ops, both with
   explicit result types like the `eu.*` family: `%u: union{…} =
   union.make %v` (v's type a member; the member's bytes at offset 0,
   the rest unspecified) and `%v: T = union.view %u` (T a member: the
   typed view at offset 0). A union stored in memory — a `List`
   element, a `Map` value, a `mut` spill — moves as one blob:
   `load.union`/`store.union` with the type on the annotation, since
   the typed-mnemonic suffix is a single token. Both are pure and
   removable, like `agg.*`.
3. **Both backends need the same two arms and nothing cleverer than
   what `agg.make`/`agg.get` already do.** Aggregates have no CLIF or
   LLVM first-class form in either emitter (a stack slot / an entry
   `alloca [N x i8]`), so `union.make` is a fresh slot plus
   `store_field` at offset 0 and `union.view` is `load_field` at offset
   0; `is_agg` grows the union so block params, calls, loads and stores
   copy its bytes. **The LLVM backend needs more than a bitcast**: the
   members differ in size (`i64` against `{ptr, i64}`) and an
   aggregate member has no scalar to bitcast, so the lowering is the
   alloca round trip and no `bitcast` instruction appears at all;
   SROA/mem2reg clean it under `--release`. The ABI classification
   (`abi::scalar_leaves`) treats a union as one INTEGER leaf of its
   size, so `{i64, union(8)}` still rides two registers and the
   24-byte shapes go by pointer as today.
4. **The checked machine needs no change.** `ubcheck`'s
   `Value::Enum { variant, payload }` carries the variant and its own
   typed payloads — there is no slot storage to read through the
   wrong variant, and `--checked` runs every row today. The region
   ledger and the move checker live in `wolf_mem` on sema types and
   read a payload by its variant's declared type already; the union is
   a lowering fact. Rows prove it: a `str` in a union slot escaping its
   region is E1010 on every tier, a `take` through a match and a
   `copy` behave as the uniform-slot enum does.
5. **wolf-lang#268**: its table carries this refusal on exactly one
   sample (ch27's exercise, `row payload slots with conflicting types
   across tags`); this row alone unlocks **1** sample, and nothing
   else in the inventory moves, because no other sample's first
   refusal is this one. Measured with the book's runner against the
   head's dist.
6. **The differ** (`xtask differ <wolf> <lupin 0.1.55> --native
   --triage`): 0 soundness findings and 0 hard divergences at both
   `0.2.34` and the head; the agreement count grows by the new rows'
   files and the conservatism ledger does not move, because no corpus
   file at `0477ce62` carries the refusal (none of its 1,169 files
   mentions it: it was refused at the first program the orchestrator
   wrote, not at any witness).
7. **`std.json`'s nine tests** (`tests/json/*.lu` at wolf-std
   `3bacc5f`) build and pass natively once `Value` lowers: the two
   risks named in advance are `Obj(Map[str, Value])` — a `Map` whose
   value is a 24-byte aggregate with a union slot, which rides the
   flat layout like any aggregate value — and the recursion
   `Arr(List[Value])`, which stops at `List` (a header pointer, no
   descent into the element) in `wir_ty`.

## 2. The cause

### 2.1 Position-unified slots

An enum value lowers (`lower.rs`, `wir_ty_frame`'s `ItemSig::Enum`
arm) to the aggregate `{i64 tag, s0, s1, …}` where slot `i` is shared
by every variant's `i`-th payload; a row's eu (`row_slot_tys`) has the
same slots after its ok half. Both unified a slot by equality: the
first variant's type claimed the position and a later variant with
another type at it refused, by name, at the type — before any
instruction of the body was lowered, which is why the refusal reached
every program that *declared* such an enum, used or not, and why
`std.json` (`Value`: `Bool(bool)`, `Int(int)`, `Num(f64)`,
`Str(str)`, `Arr(List[Value])`, `Obj(Map[str, Value])`, six types in
one slot) never lowered natively. The checked machine
(`ubcheck.rs`) and lupin never had the question: their enum value is
`Value::Enum { variant, payload: Vec<Value> }`, the variant and its
own typed payloads, no slot storage at all.

Everything downstream of the type was already positional and
type-agnostic: `agg.make` at the constructor (`enum_value`, unfilled
slots zeroed), `agg.get K+1` at the three pattern-read sites (the
scalar-arm bind in `enter_match_arm_inner`, the product-arm walk in
`product_arm_parts`' tag case and `product_sub`'s nested tag), the
handler bind `else |Tag(p)|` reading `eu.err K`, `tagged_parts`
splitting a caught row, `propagate_err` moving slots across a `?`
widening, and the flat load/store helpers moving an enum through a
`List`, a `Map` entry or a `mut` spill field by field. None of them
cared what a slot's type was; they needed one type per slot that
every variant could write and read.

### 2.2 The second refusal underneath: `copy` of an enum

Retiring the type refusal exposed the next one on the same rows:
`(mut xs).push(W.L([1, 2, 3]))` and every `Value` pushed into a
`List[Value]` copy the element in (`[mem.region.edge.elem]`), and
`deep_copy_in` had no enum arm — "`copy` of an enum whose payload
reaches heap storage" — nor a `Map` arm for values that reach the
heap (`Obj(Map[str, Value])`). Neither was the union's: a uniform
`enum E { A(List[int]), B(List[int]) }` in a list was refused the
same way at 0.2.34 (a `str` payload is not "heap" to the copy —
its bytes are shared by design — which is why `A(str)/B(str)` lists
built before). A recursive enum adds the real problem: the copy of
`Arr(List[Value])` copies each element, which is a `Value`, whose
`Arr` arm copies a list of `Value`… an inline rebuild unrolls until
the depth guard refuses at 16. §3.3 is the answer.

## 3. The layout

### 3.1 The type

`TypeData::Union(members)` (`types.rs`): the distinct member types of
one slot, sorted by `TypeId` and flattened, so one set interns to one
type and a one-member "union" is the member itself
(`TypeInterner::union_of`). Printed `union{i64, {ptr, i64}}`; parsed
back through `union_of`. Layout (`wolf_backend::layout`): size the
largest member's, rounded up to the strictest member alignment,
which is the slot's alignment; every member at offset 0. The ABI
(`abi::scalar_leaves`) sees one INTEGER leaf of the union's size, so
`{i64, union(8)}` still rides two registers and a 24-byte
`{i64, union{i64, {ptr, i64}}}` goes by pointer, as a 24-byte
aggregate always did.

### 3.2 The ops

Two value ops with explicit result types, pure and removable like
`agg.*`: `%u: union{…} = union.make %v` (the operand a member, or a
narrower union whose members are all members: `propagate_err`'s
widening) and `%v: T = union.view %u` (T a member). Both emitters
treat a union as they treat an aggregate — a stack slot in Cranelift,
an entry `alloca [N x i8]` in the LLVM text — so `union.make` is a
fresh slot plus `store_field` at offset 0 and `union.view` is
`load_field` at offset 0: a scalar member is one typed store or load,
an aggregate member a byte copy or a view into the slot's bytes. No
`bitcast` appears anywhere, which answers the prediction's third
question: a bitcast could not express it (the members differ in size
and an aggregate member has no scalar), and the alloca round trip is
what SROA already removes on the release tier. A union stored in
memory moves as one blob: `%r: union{…} = load.union %p, %m` /
`store.union %v, %p, %m` (the type on the annotation, since a typed
mnemonic's suffix is one token); the verifier admits a union where it
admits a scalar for `load`/`store`, and both emitters' `is_agg`
include it so the blob copies `layout_of`'s size. `simplify` folds
`union.view` of a visible `union.make` of the same member to the
member; a view of another member never folds (it reads bytes as
written).

In lowering, the two unifiers collect every type a position sees and
`union_of` them; four helpers do the rest. `into_slot(v, slot)` wraps
a payload at a write (`enum_value`, the raise, `propagate_err`'s
widening through `into_eu_slots`, `zero_of` — a union's zero is its
first member's, wrapped). `slot_view(v, T)` / `slot_view_as(v,
table, sema)` unwrap at a read: the scalar-arm bind carries the
arm's variant payload type into `enter_match_arm_inner` through
`view_binds` (a payload bound by an or-pattern over a union slot is
refused by name: two variants, two views), the two product-walk
reads view through the variant's `payload_sema_tys`, and the handler
bind reads the row's tag payload types through
`handler_slot_views`. `tagged_parts` and the fallible-row rebuild
stay positional: they move slot values between an eu and its
`{tag, slots}` mirror, whose slot types agree by construction. The
flat helpers (`flat_size`, `flat_align`, `load_flat`/`store_flat` and
their raw twins, `read_mut_ref`) treat a union as a scalar-sized
blob, with `natural_layout` mirroring the backend's rule so the blob
and `layout_of` agree byte for byte.

### 3.3 `copy` of an enum: a function per layout

`deep_copy_in` and `deep_copy_out` gain an enum arm, but not an
inline one. `enum_copy_call` queues a `PendingKind::Copy` task —
`{Enum}.copy.m{module}.t{wir type id}` (`.copyout.` for the
`copy region` rebuild), `(T) -> T` — once per layout and calls it;
the task drains after the body that asked, like a closure body
(`lower_copy_body`, under the same typed body so a generic enum's
arguments resolve), and its body is `deep_copy_enum`: `agg.get 0`,
an `emit_switch` over the variant tags, and under each variant the
payloads that reach storage read through the variant's view, copied
(`deep_copy_in` again — for a nested `Value` that is a call to the
same function, the recursion the checked machine's `copy_out` does
at run time), written back through `into_slot`, the slots a variant
does not own carried unchanged; the result through a declared
variable the arms assign. `deep_copy_in`'s `Map` arm copies values
that reach the heap entry by entry exactly as `deep_copy_out` already
walked them (`__wolf_rt_map_copy`, then each `(K, V)` entry at the
list prefix's stride). A payload-free enum or one whose payloads
reach nothing never arrives: `copy_reaches_heap` answers first.

## 4. Measurements

(written after the change)

## 5. Rulings and what is left

(written after the change)
