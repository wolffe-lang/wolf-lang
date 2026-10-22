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

(written after the change)

## 3. The layout

(written after the change)

## 4. Measurements

(written after the change)

## 5. Rulings and what is left

(written after the change)
