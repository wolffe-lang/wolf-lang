# s251 — closures across modules on the release tier (wolf-lang#806, #807)

Lane note. **Fable 5.1. Crawl 54.** Branch `s251` off wolf-lang trunk
`808ce11a` (0.2.34's content; rebased onto `v0.2.34` before the final
head). Contract: `sprints/compiler/s251-closures-across-modules.md`
(planning `560c78f`). §1 and §2 were written and committed before the
first line of the fix; §4 holds the numbers measured after it.

## 1. The prediction

Written before the fix, from reading `crates/wolf_wir/src/midend/`
(`dedup.rs`, `cluster.rs`, `wp.rs`, `summary.rs`), the two emitters'
`data.addr` arms and the driver's object partition, then checked
against yew's witnesses `notes/yw01-evidence/witness/closure/{a..g}` at
`808ce11a` on kasumi (§3).

1. "This object's subset" is the set of WIR functions the emitter is
   defining into ONE object: a cluster's members on the release tier
   (`wolf_wir::midend::cluster`), a module's functions on the native
   tier and under `WOLF_MIDEND=0` (`by_module` in the driver, keyed by
   each function's `src_file`). Calls and `func.addr` to a function
   outside it import the function's mangled symbol (#116, s134). A
   static callable record — the one-slot table behind a capture-free
   closure, a named function read as a value (`{f}.fv`) or a trait
   table's shim (`[abi.native.closure]`, `[abi.native.dyn]`) — is a
   `Data` whose `funcs` names its slot, defined privately in every
   object that reads it, and its slot is resolved ONLY through the
   object's own function table (`self.funcs`): the one reference to a
   module function neither backend imports.
2. Three things put a record's slot outside the object that builds the
   record, and the three witnesses are three different ones:
   (a) **dedup** merges a closure's entry away while the record still
   names it (yew's `e`); (b) the **import decision + the inliner**
   move a record's construction into another cluster while the entry
   stays where the partition put it (yew's `term.render_new_env`
   demo); (c) the **native tier's partition** files the synthetic
   `.fv` shim under the root module's object while the record is built
   in the module that read the name (yew G19, #807).
3. The fix needs no change to the partition, to the import decision or
   to the inliner: resolve an out-of-subset slot by its mangled symbol
   in both emitters, as `func.addr` has been since s134, and pin the
   names a record carries against dedup (the contract the summary has
   claimed since s98). Every program that built before keeps
   byte-identical objects — the partition, the imports and the inline
   decisions are untouched — so the release build's size and time move
   by 0 on the corpus except for programs that were refused; yew
   `ab8bab0` (which carries the workarounds) builds the same size
   within noise with both compilers; the six rows go green on both
   tiers and on the checked machine, and the differ moves no count but
   theirs.

## 2. The cause

### 2.1 The release tier's object model

`wolf build --release` runs `optimize_whole_program` (`wp.rs`): a
module phase (the s42 pipeline with the inliner's horizon at each
function's own source module), **dedup** (D8: content-identical bodies
collapse to the lexicographically smallest name), the summary (v3,
`summary.rs`), the **partition** into clusters (`cluster.rs`: `func.addr`
referees fused with their referrers by the `refs=` edges, then call
affinity under `cluster_target_size` 512, then bin packing into at most
8 clusters), the **import decision** (a cluster imports the bodies of
small callees it calls across the cut, from summaries alone), the
**cross-cluster inline round** (members ∪ imports visible), dead
function elimination and the re-key. Each cluster is one object: the
driver calls `compile_selected(.., cross_module = true)` with the
cluster's member ids, so every wolf function is `Export` under its
mangled name, and a call or a `func.addr` naming a function outside
the member set declares an import of that symbol
(`wolf_codegen_llvm/src/emit.rs` `lower_call`, `Opcode::FuncAddr`;
`wolf_codegen_clif/src/translate.rs` the same, #116).

A capture-free closure lowers to a module function `{maker}.cls{n}`
(`lower.rs`, `queue_closure`) and its VALUE is the address of a static
one-slot record: `fn_value_static` interns a `Data` with
`funcs = [entry]` and emits `data.addr` of it. A named function read as
a value is the same record over a shim `{f}.fv` built after the task
drain (`build_dyn_shim`, `src_file = None`). Both emitters define such
a record on first reference, privately, in the object being emitted
(`_W.{name}`, `Linkage::Local` / `private unnamed_addr constant`), and
fill each slot from `self.funcs.get(fname)` — refusing with `a vtable
slot @f outside this object's subset` when the name is not a member.
That refusal is the only place a module function is looked up without
the mangled-symbol fallback.

### 2.2 (a) dedup drops an entry a record still names — rows 1–4, yew `e`

`dedup::dedup` pins the identity of every function some `func.addr`
names (`pinned`), hashes every other body, merges each class into its
smallest name and `retarget`s every CALLEE table through the rename.
It never reads `m.data[*].funcs`: a closure entry or a `.fv` shim is
referenced only there, so it is not pinned, and a merge leaves the
record naming a body that no longer exists — nothing rewrites a data
table. `summarize` has flagged such names `A` (address-taken) since
s98 "so its identity is pinned (dedup)"; the summary describes a
contract the pass does not keep.

Witness `e`: `main.lu` and `lib/lib.lu` each define `lookup` and each
make `fn(k: str) lookup(k)`. The module phase inlines each `lookup`
into its closure, so `main.cls0` and `lib.tier_env.cls0` are one body;
dedup keeps `lib.tier_env.cls0` and drops `main.cls0`; nothing calls
`lib.tier_env`, so DFE removes the representative too. The final
summary holds `main` alone (`--codegen-report`: `dedup: 7 bodies -> 5
unique (2 merged, 0 site(s) retargeted)`), and `main`'s record still
names `@main.cls0`. `WOLF_MIDEND=0` (no mid-end, per-module objects)
builds and runs the same program, which is why only the release tier
refused it. Rows 1–4 (`corpus/typecheck/closure_{entry_to_module,
module_to_entry,module_to_module,captured_closure}`) each carry a
body-identical twin whose name sorts first, so the record in use is the
one that goes dark: `@main.cls0`, `@tty.getter.cls0`, `@src.run.cls0`,
`@main.cls0` (the word a capturing closure captured) at `808ce11a`.

### 2.3 (b) the import decision and the inliner relocate a record — row 6, yew's demo

A closure's entry has no call edge to its maker (the maker builds a
record; it does not call the entry) and no `refs=` edge either —
`refs` is `func.addr` only (s117) — so the partition's co-residency
rule never sees it: affinity does not keep it with its maker and bin
packing places it in whichever cluster is lightest. The maker is a
one-liner, small and single-use, exactly what `decide_imports` imports
across the cut; the cross-cluster round then inlines it into the
caller, and the record's `data.addr` is now built in the caller's
object while the entry stays in its own cluster. Row 6
(`closure_inlined_one_liner`, two clusters by ballast):
`cluster c00 members=[main,tty.pad_a,tty.pad_b]`, `cluster c01
members=[tty.render_env.cls0]`, `1 import(s); … 1 cross-cluster`, then
`a vtable slot @tty.render_env.cls0 outside this object's subset` —
yew's `--codegen-report` with `@term.render_new_env.cls0` under `fn
main`, in thirty lines. A thirty-line program without ballast is one
cluster, which is why witness `d` is green and the demo was not.

### 2.4 (c) the native partition files the `.fv` shim under the root — row 5, yew G19

`build_dyn_shim` gives the shim no source file, and the driver's
per-module partition maps `src_file = None` to module 0, the root: the
shim `tty.getv.fv` is defined in the root's object while the record
that names it is built in `tty`'s (`tty/input.lu` read `getv` from
`tty/env.lu` as a value). Only a read from the root module itself, or
a single-file program (one object), finds the shim in its own subset
(`corpus/typecheck/fn_value_import` is the root-side read). Row 5
(`fn_value_two_file_module`) is red on the native tier and under
`WOLF_MIDEND=0 --release`, green under the whole-program phase (one
cluster) — the s134 shape (`proc_cross_module`'s entry shim), one
record over.

### 2.5 The fix, and why the inliner keeps relocating

The record's slot is a link-time constant exactly as a function's
address is, so (i) both emitters resolve an out-of-subset slot through
the same mangled-symbol import `func.addr` takes (a `declare` in the
LLVM text; an `Import` declaration in the Cranelift object), covering
(b) and (c) under every partition; (ii) dedup pins every name a data
table carries, as the summary always said it did, covering (a). The
partition, the import decision and the inliner are untouched; the
contract's other option — stop relocating a closure's maker across
objects — would mean a `refs=` edge for record slots (a summary format
bump that invalidates every cluster cache) and would not even suffice:
s134 showed that co-residency at partition time is defeated the moment
a body is imported and inlined, and the import of a one-line maker is
precisely the win the inliner is there for. §4 prices it: every
previously-green object is byte-identical (nothing in the decisions
changed), the rows gain a direct symbol reference where they used to
refuse.

## 3. Evidence at `808ce11a` (kasumi, `wolf 0.2.33+dev.808ce11`)

yew's witnesses, `wolf build` and `wolf build --release`:

| dir | native | release |
|---|---|---|
| a, b, c, d, f, g | green (runs) | green (runs) |
| e | green | **refused**: `a vtable slot @main.cls0 outside this object's subset` |
| e under `WOLF_MIDEND=0` | — | green (runs) |

The six rows (each `check: run(exit=0, stdout=…)`), the same build:

| row | native | release | mechanism |
|---|---|---|---|
| `closure_entry_to_module` | green | refused `@main.cls0` | dedup (2.2) |
| `closure_module_to_entry` | green | refused `@tty.getter.cls0` | dedup |
| `closure_module_to_module` | green | refused `@src.run.cls0` | dedup |
| `closure_captured_closure` | green | refused `@main.cls0` | dedup |
| `fn_value_two_file_module` | **refused** `@tty.getv.fv` | green (one cluster); refused under `WOLF_MIDEND=0` | partition (2.4) |
| `closure_inlined_one_liner` | green | refused `@tty.render_env.cls0` (2 clusters, 1 import, 1 cross-cluster inline) | import + inline (2.3) |

## 4. Measurements

Filled in after the fix (kasumi, `taskset -c 0-3`, dist builds of
`808ce11a` and the branch head): see the PR's "Measurement" section and
the CHANGELOG entry; the table is reproduced here at the final head.

## 5. yew workarounds that can be retired (yw04)

Listed by file at yew `ab8bab0`; each is the port's answer to G19/G20
and none is needed once the pin carries this change:

- `src/term/tty.lu` — the `Env` VALUE (`env_process()`, `env_of(table)`,
  `env_getv` kept beside it, the header's idiom note at line 61 and
  the G19 note at line 227): every `getenv`-injected API of sagitta's
  can take `fn(str) -> (str, bool)` again (yew `5bd44c1` replaced the
  function value with `term.Env`; witness `e` was its shape).
- `src/term/input.lu` — `input_enable` reads `YEW_MOUSE` directly
  instead of passing `env_getv` as a value (yew `7acaa89`, G19): the
  cross-file read by name builds on both tiers now (row 5).
- `src/text/cset.lu` — `sort_items` (line 93), a local copy of
  `util.sort_stable`'s bottom-up merge written "so no function value
  crosses a module boundary (G19/G20)": `util.sort_stable(mut xs,
  item_less)` with the comparison passed as a value is the direct port
  again. The same rule shaped the yw02/yw03 name tables (a registry as
  one `match` per name where C yew holds function pointers) and the
  injected clock (`docs/MAP.md` §1: `undo.fake_mono`, `undo.fake_wall`
  as values in the owner): a table of fn values across modules builds
  on both tiers now; yw04 decides which of those the port wants back.
