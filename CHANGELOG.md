# Changelog

## Unreleased

## 0.2.17 — 2026-09-26

THE SEVENTEENTH. 0.2.16 refused programs 0.2.14 ran (#449), and the
first release in which a `Scope` handle could be written as a parameter
hung the native tier when one was (#431). Both are fixed here, and they
go first. Beside them land the rulings the maintainer took on
2026-09-24 — four clauses (s182), the index store's compiler half
(s180, #438) and the record's file index (s181, #437) — and the pairing
moves to lupin 0.1.40, the first lupin that answers #438 and #386 as
ruled. Six lanes and 67 commits: s178, s182, s174 (the #431 gate), s179,
s180, s181.

### Read this before you bump the pin
**0.2.16's new exclusivity leg refused programs 0.2.14 and lupin run
(#449).** The `mut`-claim extent check that landed with "a container
element is a place" reported a claim that is **not in the call's
argument list** — it named a statement *preceding* the call as "inside
the outer call's extent" — and it refused a shape the exit convention
prescribes. boreutils hit it at **12 sites in 7 of its 15 utilities**
and lobo at **16 sites in 4 files** moving their pins from 0.2.14; both
shipped workarounds under this number rather than stay off 0.2.16.

The witness, `exit(0)` printing `2` on 0.2.14 and on lupin 0.1.38,
`fail(E1002)` at `phase_reached: mem` on 0.2.16 (both wolfgang lanes —
it is ahead of lowering), reproduced on the published linux x86-64
archives by digest:

```wolf
fn note(mut l: Log, m: str) { (mut l.lines).push(m) }
// ...
if true { note(mut lg, "a") } else { note(mut lg, "b") }
note(mut lg, "c")                       // E1002 at 0.2.16, naming the `else` arm
```

**The cause was not the rule, it was the walk.** The check bounded the
extent by BLOCK INDEX order, admitting every block numbered above the
call's — but block ids are handed out in allocation order, and
`eval_if` mints the join block *before* the else arm while `eval_match`
mints it before every arm. So the arms outrank the join they flow into,
and the walk read backwards into arms that had already finished. That
is why dropping the `else` compiled, why the `match` report named the
**first** arm, and why moving the `mut` argument last made the
diagnostic vanish. `[mem.tier0.excl.1]` says "at every program point",
and an arm's claim is not live at the join; the clause was right and the
check was wrong, so no spec text moves.

The extent is now the call's argument evaluation and nothing else: the
mark carries the block count, and only blocks minted since it count as
new. **s168's refusals are unweakened** — `take2(mut r.a, wipe(mut r))`
and `take2(mut xs[0], grow(mut xs))` are still E1002 on both lanes, as
is a branch spelled *inside* the argument list whose arms claim the
place, whose blocks really are minted after the mark. Both downstreams'
workarounds can be reverted under #449.

### A `Scope` handle as a parameter (s179, #431 path A)

**A `Scope` handle passed as a parameter no longer deadlocks the native
lane (#431, path A).** `fn fan_out(s: Scope, ch: channel[int]) {
s.spawn(fn() { ch.send(1) }) }`, called from `scope work { fan_out(work,
ch) }`, hung the Cranelift tier on every platform in the shipping build
— the published 0.2.16 linux archive (sha256 `84e30c05…`) included —
because the task's capture record lived in `fan_out`'s frame and
`fan_out` returns before the scope joins in `main`. The runtime now
copies that record into the scope at the spawn seam: one additive
symbol, `__wolf_rt_scope_env_copy(scope, env, len) -> env`, whose copy
the scope frees after its join. Lowering calls it only for a spawn
through a handle the spawning function did not open; a spawn into a
scope the function opened keeps its frame slot, a spawn under a loop
keeps s86's arena, and the frozen five-parameter
`__wolf_rt_scope_spawn` does not move. No lowering snapshot in the
corpus moved. On kasumi, release profile: the witness answers `1` in
under a second on `wolf run`, `conform-run --native` and `--native
--release`, where the same tree without the fix is killed at the cap;
s174's gate (`crates/wolf_driver/tests/scope_handle_param.rs`) is green
unedited. `[conc.task.scope]` is implemented as written; the spec does
not move. A task spawned **in a loop** through a handle the function
did not open is still refused by name at lowering.

*Carried from s174, the gate this fix went green under (written for the 0.2.16 cycle, landing here because 0.2.16 shipped without it):*

**#431 is not windows-only** (s174). The same program — a `Scope`
handle passed as a parameter — deadlocks the **Cranelift native tier
on linux x86-64**, measured on kasumi against trunk `2f8deb7f` under
`cargo xtask dist`, the build that ships: rc 124 at 60 s, 3/3, on both
`conform-run --native --json` and `wolf run`. The LLVM release tier
answers `exit(0)` / `1` from the same binary, and the checked lane is
an honest `unsupported` (C1 deferred).

### The rulings' prose (s182)

Four clauses landed for rulings the maintainer took on 2026-09-24. The
machines follow in this wave, and none of the four moves a corpus row's
verdict today.

- **`[gram.expr.assign]` and `[mem.region.edge.elem]` — the index store
  follows `push` (#438).** `xs[i] = v` and `m[k] = v` COPY a non-`Copy`
  value in and leave `v` live; `xs[i] = take v` MOVES it, the one store
  position an assignment admits a mode in. Accepts programs refused
  today (a use of `v` after a plain store; a `read` parameter stored
  plainly), changes no printed byte, and costs one deep copy per plain
  store of a value that reaches the heap. The ruled witnesses were
  parked in the planning repo
  (`sprints/compiler/88-the-rulings-prose/witnesses/`) for s180 on the
  wolfgang lanes (below) and is55 on lupin (lupin 0.1.40);
  `corpus/memory/index_store_copy_elem.lu` pins the `Copy` half, which
  holds on every machine already.
- **`[mem.model.place.rhs]` — a store evaluates its right-hand side
  first, then mints its place** (B123, s173's proposal 1, path B). Pins
  what all three machines already do:
  `corpus/memory/store_rhs_first_{list,map,pool}.lu`, asserted across
  the lanes by `store_rhs_first_lanes.rs`. The order of the place's
  own operands against the right-hand side is NOT ruled by it, and the
  machines differ there (#452: for `xs[idx()] = val()` the checked
  machine prints `val idx`, native, release and lupin `idx val`).
- **`[os.fs.path.domain]` — every path the host allows** (#386, s175's
  draft as written). Confinement is a scope decline by name, resolved
  not lexical, never a row. wolf serves the whole domain on both tiers
  already. lupin 0.1.38 refused `sub/../x` lexically; lupin 0.1.40
  serves it and declines a symlink that carries a path out by name
  (is55). *Corrected at the 0.2.17 cut:* this entry first said lupin
  0.1.38 also answered a write through any symlinked directory with a
  `not_found` row, a symlink defect. It had none — `lupin run` served
  both symlink witnesses at 0.1.38. The row came from `lupin
  conform-run` observing in a private root instead of the directory it
  was invoked in, where the witness's setup had made its link; a plain
  file in that directory failed the same way. 0.1.40's `conform-run`
  resolves against the working directory, as `[os.fs.path]` says. The four
  ruled witnesses and the symlink control are parked beside #438's,
  because a corpus witness names a relative path with no `..`.
- **`move xs[0]` on a `List[int]` refuses a later read of `xs[1]`
  (#446, kept as it is — option A).** A consequence of #444's fix in
  0.2.16: `move` records the move for every type now, and a container's
  elements are ONE place on the compiler (`Proj::Opaque`; element
  granularity is a declared non-target), so the refusal 0.2.14 already
  gave a non-`Copy` element reaches `Copy` ones. lupin models elements
  one by one and runs the program: a documented conservatism
  divergence, the class of `[mem.tier0.mode.read]`'s rows, pinned by
  `corpus/memory/elem_move_one_place.lu` and asserted on both sides by
  `element_move_conservatism_lanes.rs`.

### The index store follows `push` (s180, #438)

**`xs[i] = v` and `m[k] = v` copy a non-`Copy` value now, and
`xs[i] = take v` moves it** — `[mem.region.edge.elem]` and
`[gram.expr.assign]` as s182 wrote them, on the checked, native and
release lanes. A plain store through a `List`, `Map` or `Pool` index
whose right-hand side is a place (a binding, a field, an element)
leaves that place live: the static tier reads it as `xs[i] = copy v`,
the checked machine and the native lowering deep-copy it exactly as a
plain `push` does. A temporary is stored as it is — it has no other
owner. `Copy` values and `str` are free, as before.

- **Accepted now:** a use of `v` after a plain store (was E1001 on both
  wolfgang lanes) and a `read` parameter stored plainly (was E1002
  under #366 — a copy is the function's own value, not a lend).
  `corpus/memory/index_store_copies_{list,map}.lu`,
  `index_store_read_param.lu`.
- **Spelled:** `xs[i] = take v` / `m[k] = take v` parse (were E0201)
  and move; a later use is E1001 with the `copy v` fix-it.
  `m[k] = take v` on a `read` `v` is E1014, `push(take v)`'s answer.
  `take` anywhere else in an assignment is E0201 exactly as before.
  `corpus/memory/index_store_take_{list,map,read_param}.lu`.
- **No printed byte of a program accepted before changes**; no corpus
  row changes verdict. The cost is one deep copy per plain store of a
  place that reaches the heap — `take` is the old handover, spelled.
  A raw-pointer index store is unchanged. The operand order of an
  index store (#452) is untouched on every lane.
- `index_store_lanes.rs` asserts every case on checked, native, release
  and lupin. lupin 0.1.38 and 0.1.39 predate the mirror: the gate pins
  their measured answers (`PRE_MIRROR_LUPIN`) and demands the ruled ones
  from any other version, so the pairing at lupin 0.1.40 (is55's mirror)
  takes the ruled arm.

### The record names a diagnostic's file (s181, #437)

**A diagnostic whose span lies in a sibling module now says which file
it is in.** A record's spans were byte offsets with no file, so a span
from a sibling module read as an offset into the entry file — a wrong
place, silently, for any tool that renders it. `[proto.record.diag]`
gains an optional file index: the record carries a top-level `files`
table (package-relative, entry first) and each diagnostic a `file`
index into it, **only when some diagnostic lies outside the entry
file**; every other record is byte-identical to 0.2.16's (`f7a91e78`,
`0fa638b9`). `cargo xtask` validates the index all-or-nothing and
compares the file a diagnostic names (`4d204bb3`, protocol fixture
`with-file-index`, `cf36d489`).

- **Which records move: 16 per wolfgang lane**, predicted before the
  edit (`51f7bea0`) and measured — the 15 corpus entries with a
  diagnostic outside the entry file plus the new witness
  `corpus/resolve/sibling_diag` (`ff6f2667`, `fail(E0302)`). Checked
  and native are identical: the diagnostics come from the shared
  frontend. `record_file_index.rs` asserts the named file on all three
  machines (`a98f0ae2`).
- **Path keys are lexical.** `canonicalize` spelled one side `\\?\`
  on windows, so keys compare lexically and fold windows spelling
  before keying (`9111a2e0`, `69de0e66`, `cf89d3c2`).
- lupin admits both keys from 0.1.39 (is54, the lupin half); 0.1.38's
  closed schema refused them. The pairing moved to 0.1.39 in this lane
  (`48a12e6a`); s180's gate then recorded 0.1.39 as pre-mirror too
  (`30e81609`) — the version pin did its job.

### The pairing: lupin 0.1.40

`PAIRING` names **lupin 0.1.40 at pin `93a5fe5`** (wolf-interp release
397033025; the linux x86-64 archive's sha256 `509929e6…`), the first
lupin that answers #438's index store and #386's path domain as ruled.
s180's gate takes its ruled arm for 0.1.40 with no edit
(`PRE_MIRROR_LUPIN` stays 0.1.38 and 0.1.39). The ritual (#87, with
#281's control against the 0.1.39 archive, same corpus, same release
`wolf`) **moved 5 ledger counts on each tier, together**, and all five
are s180's witnesses: `index_store_copies_{list,map}.lu` SOUNDNESS →
agreement, `index_store_take_{list,map,read_param}.lu` completeness →
agreement. Checked: 389 → 394 agreements, soundness 2 → 0, hard 9 → 7;
native: 430 → 435, soundness 3 → 1, hard 12 → 10. Nothing moved below
the ledger. Native's remaining soundness finding is
`memory/unsafe_ub_uaf.lu` (`exit` vs `ub(mem.ub)`), the program's own
use-after-free read on the compiled lane, as at 0.2.16.

### Shipped, by name

Open at the cut, and none of them new in 0.2.17:

- **#452** — for `xs[idx()] = val()` the checked machine runs `val`
  before `idx`; native, release and lupin run `idx` first.
  `[mem.model.place.rhs]` pins only the place's address; the operand
  order is a ruling owed.
- **#446** — `move xs[0]` on a `List[int]` refuses a later read of
  `xs[1]` (option A, the status quo since 0.2.16's #444 fix); the
  maintainer's word is owed.
- **#455** — on the native tier a first-class `region()` created inside
  a function is not freed on return (8 × 50 MB peaks at 924 MB); the
  block form `region r { … }` frees.
- **#458** — an unannotated integer literal binding is typed by a later
  comparison on wolfgang and runs as `int`; lupin keeps the `i32`
  default and traps.
- **#457** (the test suite, not the toolchain) — `scope_handle_param.rs`
  runs whatever `target/release/wolf` exists and never rebuilds it, so a
  stale release build reds it; rebuild before a release-profile run.

## 0.2.16 — 2026-09-22

THE POINT RELEASE. The published 0.2.15 ships three silent wrong
answers on the checked machine — one of them wrong on the native lane
too — and its two machines disagree with each other on a fourth program
(#433, ruled a point release). All four are fixed here, with their
witnesses quoted so a downstream can test its own pin. Eight lanes and
140 commits land beside them; the defects go first, then the change that
alters what an existing command exits with, then the one defect this
release still ships.

### Read this before you bump the pin

**Four programs 0.2.15 gets wrong.** Each was reproduced on the
published linux x86-64 archive (`wolf` sha256 `0ede3f66…`) through
`wolf conform-run <file> --checked --json` against `--native --json`.
That command matters: `wolf run --checked` runs the native build, and
the checked machine answers only through `conform-run`, which is how one
of these was once withdrawn on a wrong reading (#432).

1. **#400 — a call through a fn-typed parameter went to a top-level fn
   of the same name**, on the checked machine, silently.
   `fn apply(le: fn(int, int) -> bool, x: int, y: int) -> bool { le(x, y) }`
   called as `apply(ge, 1, 2)`: checked printed **`true`**, native and
   lupin `false`, exit 0, no diagnostic. It crossed module boundaries —
   wolf-std's `list.sort_by` is what found it, because the library
   picks the parameter's name and a caller cannot defend against it.
   Fixed by s171; the resolver decides now, one scope further in.
2. **#391 — the checked tier's five `str` producers charged nothing to
   the ambient region.** `let s = base.repeat(64)` inside
   `region pass { … charged = region_bytes(pass) }`: checked
   `charged false 0`, native `charged true 4096`. Under
   `region idle(cap: 0)`, `base.upper()` ran to `exit(0)` on checked
   where native answered `trap(alloc-contract)`. Fixed by s171, and
   #392's `[mem.str.view]` ruling lands beside it.
3. **#432 — the two machines disagree on a program neither should have
   accepted.** `take2(mut r.a, wipe(mut r))` over
   `var r = R { a: 1, b: 2 }`: checked printed **`1007 2000`**, native
   **`8 2000`**, both `exit(0)`, no diagnostic on either. It is E1002
   on both lanes now (s168), and the divergence stands in the record as
   s168 first reported it.
4. **#444 — `move` of a `Copy`-typed place recorded no move, on every
   lane.** Found by the maintainer in wolf-book ch07 §7.2:
   `let who = move d.meta.author` followed by
   `print("{d.meta.author} wrote it")` compiled, ran and printed
   `ada wrote it` a third time — on 0.2.14, on the published 0.2.15 and
   on trunk `75e2aa8d`, checked and native alike. lupin 0.1.37 answers
   `trap(use-after-move): `d.meta.author` was moved out and is
   uninitialized here [mem.tier0.move.2]`. E1001 now, whatever the
   type (s177). This is the one that is wrong on the tiers people ship.

**A rejected program exits 2 now, and a refusal 4 — not 1**
(`[conf.exit]`, s169). wolf spelled both 1, and 1 is what a program that
RAN and returned an error out of `main` exits with; wolf-book's sample
runner carried `run(exit=1)` fences green across four pin bumps for two
programs the compiler never built. **This is the second pin bump in this
project's history that alters what an existing program observably
does**, after #385's `push` at 0.2.15 — that one changed what a program
printed; this one changes what the toolchain exits with. A harness that
reads `exit 1` as "did not compile" reads 2 and 4 now, and a program's
own `return 1` still exits 1, which is the point. lupin already answered
2 and 4 and does not move.

**One defect this release ships, by name: #431.** A function that takes
a `Scope` handle as a parameter and spawns into it —
`fn fan_out(s: Scope, ch: channel[int]) { s.spawn(fn() { ch.send(1) }) }`
called from `scope work { fan_out(work, ch) }` — never returns on the
Cranelift native tier: linux x86-64 in the shipping build (`cargo xtask
dist`, killed at 60 s, 3/3, s174 on kasumi at trunk `2f8deb7f`) and
windows. The LLVM tier (`--release`) prints `1`. The debug build of the
compiler does not show it, which is why every gauntlet was green (a
dangling stack slot; whether the dead frame is overwritten before the
task reads it depends on the profile `libwolf_rt.a` was built in). The
witness is back under a gate in PR #441, red on purpose; the fix waits on
a ruling. At 0.2.15 the program was `fail(E0301)` because `Scope` had no
spelling in a signature — 0.2.16 is the first release in which it can be
written, and it hangs. Until it is fixed: `--release`, or spawn from the
scope's own block.

**The linux archives run on glibc 2.35 and later** (#447). 0.2.15's two
linux archives were built on Ubuntu 24.04 and imported `GLIBC_2.39`, so
they did not start on 22.04 LTS at all — the most common LTS on a rented
box — and under a 24.04 loader with `libwolf_rt.a` not beside the
binary, the native tier answered exit 2 on every program behind a
correct `--version`. Both linux archives are built on 22.04 now, and the
release workflow unpacks each one on a 22.04 host and runs one program on
each tier — native, release, checked — before the release is published,
printing the highest GLIBC symbol version the binary imports. **The
floor is glibc 2.35** (Ubuntu 22.04, Debian 12, Fedora 36 and later).
lupin 0.1.38's linux archive still imports 2.39; that half of #447 is the
interpreter's.

**The release page carries one release per tag** (#226). Through 0.2.15
every dist job created the draft and tolerated "already exists", and
GitHub lets any number of drafts carry one tag name, so every tag since
v0.2.3 left three empty drafts beside the real release — 39 in all,
deleted by id at this cut. The draft is created once now, in its own
job; the archives upload to it by id; and publishing refuses unless
exactly one release carries the tag, then asserts the shape after the
flag moves. Proved on a throwaway `v0.2.16-rc.1` before this tag was
cut.

### `move` empties the place, whatever the type

**Reading a place after `move` is E1001 again — and it never was, for
any `Copy` type** (#444, found by the maintainer working through
wolf-book ch07 §7.2). The book moves one leaf out of a value tree and
says the source path is empty afterwards: nothing to free, nothing to
read. Add the line that reads it and wolf 0.2.14, the published
0.2.15 and trunk all compiled the program, ran it, and printed the
moved value — on the checked lane, the native lane and the release
lane alike, with no diagnostic. lupin traps it:
`trap(use-after-move): `d.meta.author` was moved out and is
uninitialized here [mem.tier0.move.2]`.

The mem tier lowered the `move` OPERATOR through the rule for a place
in value position, where `[mem.tier0.move.3]`'s implicit copy applies
because the program said nothing — and `str` is `Copy`. So the move
site was never recorded, and no analysis downstream could miss what
was never written down. The field path was never the problem: the
same program with a plain `var s = "ada"; let t = move s` and a read
of `s` was equally accepted, and so was the `int` twin. `move` now
records the move for every type; `[mem.model.path.disjoint]` still
decides the rest, so the book's own example — moving `d.meta.author`
and reading `d.title` and `d.meta.words` — keeps running, pinned as
`corpus/memory/move_field_siblings_ok.lu` beside the refusal witness
`corpus/memory/move_field_use_after.lu`.

**This is the third silent wrong answer found in the published
0.2.15** (after #400 and #391/#392) and the first that was wrong on
the native lane too. The corpus could not have caught it alone —
`cargo xtask corpus` runs `phase: run` entries on the native lane and
`lane-coverage` counts what the checked lane executes, not what it
answers — so the witnesses are joined by
`crates/wolf_driver/tests/move_expression_lanes.rs`, which asserts the
checked lane, the native lane and lupin agree AND agree on the right
verdict. All five of its cases were red at trunk 75e2aa8d.

### The pool has a runtime, the handle is a word, and the unsafe tier lowers

**`Pool[T]` and `handle T` compile** (#268, #31, #11). A pool is one
pointer to a generational slot arena in `wolf_rt::pool`; a handle is
ONE 64-bit word — slot index low, generation high — and deliberately
not a pointer, which is the property the type exists for. `reserve`,
`init`, `remove`, `len`, `is_empty` and `alive` lower to runtime
seams, `pool[h]` reads through a generation compare, and a stale
handle is `trap(stale-handle)` on **every** lane, release included:
`[mem.shared.handle.2]` says defined behaviour in every profile, and
that is now a WIR trap kind rather than a checked-build artifact.

**`pool[h].next = k` is one store** (#31, the book's exercise 8-7,
pending since bs04). A pool slot is a place, so the field write goes
through `resolve_place` — s168's walk — and not through a
read-modify-write of the whole node. `pool[h] = v` is a place too:
sema typed that as "assignment through this place" conservatism until
now, and `[mem.shared.handle.3]` has said the pool is the place base
since it was written.

**The slot address is re-minted at the access, and that is not a
detail.** The first shape of this work minted it once, at the place
step, on the argument that `[mem.model.place]`'s opacity would make a
second path to the pool E1002 before it could run. It does not: the
exclusivity rule s168 landed covers a nested call under a `mut`
ARGUMENT, not an assignment's right-hand side, and the sibling shape
on a `List` — `xs[0].n = grow(mut xs)` — is accepted by every lane
today and answers correctly only because `elem_addr` re-mints. Under
mint-once the pool witness printed `1` natively where checked printed
`7`, with no diagnostic on either lane. The driver test carries that
red verbatim.

**D50's `has` and `clear` land; `capacity` and iteration do not.** A
readable `capacity` is a promise about a growth policy that both lanes
would have to agree on, and pinning one belongs in a clause rather
than in two implementations that happen to match. The runtime seam
exists; the surface spelling is proposed, not taken.

**The unsafe tier lowers** (#268, #156's ch09/ch32/appx rows, s26's
deferral). Raw casts between pointer types are the identity, `p[i]`
and `p[i] = v` are one `ptr.off` and one access with no bound —
`[mem.unsafe.raw.1]` — and `assume noalias` is accepted and emits
nothing: the license is real and lowering declines to take it, because
a backend acting on an unproven `noalias` turns a wrong assertion into
a miscompile instead of the defined UB the clause already licenses.

Closing that required closing a deferral `lower_c_call` had recorded
in its own comment: the five `c.` membrane calls threaded **no** memory
token, on the ground that "the raw loads/stores an io spine would have
to order against do not lower yet". They do now, so a load through `p`
could have floated across the `c.free` that killed it. Raw C memory
rides the existing `Buffer` foreign role rather than a role of its
own — a new role would assert a disjointness nothing here can prove.

**The quarantine allocator is no longer `unimplemented!`** (D21). A
granule store over `std::alloc`, a deterministic tag stream, and the
FIFO quarantine the budget bounds: a freed granule is retagged and
poisoned, `check` answers `UseAfterFree` / `DoubleFree` /
`RegionFreed` / `OutOfBounds`, and the oldest granules are released
only under budget pressure. Nothing calls it yet — the `--checked`
profile wiring and the alloc/free backtraces are separate work, and
the module says so rather than reading as a shipped feature.

### The rulings (s175 — wolf-lang#434, #347's third clause, wolf-interp#125, #431's thread, #370's residual)

**A qualified error-set alias is the alias** (#434, wolf-std F-0138;
`[type.err.alias.qualified]`). Since s158 an alias was transparent
inside its module and **opaque from outside**: `-> int ! fs.IoErrors`
was accepted as a spelling and contributed no tags, so a `?` under it
was E0602 listing exactly the three tags the alias is defined as, and
wolf-std kept a local copy of std's alias rather than name it. The row
lowering only ever looked up a single-segment path in the current
module; it now resolves an entry the way a type path resolves — this
module's item, a `use m.Alias` binding, or `m.Alias` through the module
binding — and the deferral pre-scan, the resolver's reference table and
the W0305/W0603 tag lints see the same three forms, so `use m.Alias`
whose only use is a row is no longer E0305. A private alias named from
another module is E0304 (and still expands, so the one cause reports
once). Witnesses `corpus/rows/error_alias_qualified/` (qualified and
`use`-bound, propagating back into a spelled-out row) and
`corpus/rows/negative/error_alias_private/`; both were red on the trunk
binary at `2f8deb7f` (`fail(E0305)`, `fail(E0602)`) before the change.
lupin 0.1.37 has no typecheck tier and already ran the witness.

**`[type.list.lit.elem]` says `List[i32]`** (#347's third clause). The
sentence read "`[1, 2, 3]` is `List[int]`" while `[type.numlit.default]`
and both compiler tiers say `i32` — the spec disagreeing with itself,
found by bs49 and left standing when #347 closed. Measured, not chosen:
`let n: i32 = xs[0]` runs and `[5000000000]` is E0415 on both tiers at
trunk and at 0.2.15 (`typecheck/list_lit_elem_i32.lu`,
`list_lit_elem_unfit.lu`). No compiler change.

**`trim` takes no argument** (wolf-interp#125; `[mem.str.ws]`). lupin
0.1.37 ran `word.trim(".,;!?")` and wolf 0.2.15 refused it E0402; the
spec named one separator set and no cutset, and now says so in words.
Ruled for the spec: the extension goes from lupin, wolf changes nothing
(`strings/trim_cutset_refused.lu`). The book's ch06 capstone, which
spells the cutset, is a book lane's.

**One truth per corpus file** (#431's thread; `corpus/README.md`). No
per-platform posture exists and none is planned: a host-divergent
answer is a defect, and a header that pinned it per host would make the
gate that found it certify it.

**The keyword run is closed by name** (#370's residual;
`crates/wolf_ast/tests/keyword_block.rs`). The exhaustive ~200-arm
`match` is declined; a test over `kind.rs`'s own text asserts every
`*Kw` variant sits inside `AsKw..LParen` and nothing else does, which
is the addition mode #356 actually met and the `const` assertions
cannot see. Seen red with a planted variant on each side before it was
trusted.

Routed to the maintainer, not landed: the index-store half of #385
(wolf-lang#438) and a file index on record spans (wolf-lang#437).
Filed: the checked machine's closure refusal (#435) and its
interpolation-slice refusal (#436).

### The concurrency handles have names, and a proc talks back

**`Scope` and `Proc[T]` are prelude type names** (#316, BACKLOG B21's
ruling). `[conc.task.scope]` has said since D16 that "a function that
spawns into its caller's scope takes the handle as a parameter", and
there was no way to write that parameter: `Scope`/`Proc` were E0301
(nothing named that is in scope) and `scope`/`proc` were E0206 (a
keyword in type position), while the generic escape — `fn f[S](s: S)`
— is E0501 by D28 and always will be. Both handles now elaborate in
signature position the way `channel[T]` does, so `fn fan_out(s:
Scope)` and `fn watch(p: Proc[int])` are programs. The lowercase
keywords are *not* admitted in type position the way `region` is; that
was the other candidate and B21 decided against it.

**A proc talks back with a typed result its supervisor collects at the
join** (#110, B21's ruling; `[conc.proc.join]`). `p.join()` blocks
until the proc exits and answers `T ! {error, killed, cancelled,
fault}` — `[conc.proc.exit]`'s `normal(value)` read as a value, the
four abnormal classes read as tags. Before it, a proc could report
only its exit class and its stdout, which is why `corpus/procs.lu`
spawned every proc arg-less and why s87 had to prove its per-proc
argument copy in a `wolf_rt` test instead of a corpus program. The
join is `monitor` plus one receive in the runtime, so the two readers
decode one word and cannot drift, and a join issued after the proc has
already exited reads the tombstone and returns at once. The other
candidate shape — a channel handle as a proc parameter — is decided
against.

The two rulings are one surface: **the handle carries the completion
type** (`Proc[int]`, not a bare `Proc`), which is why #157's ch14 row
— asking for ``help: proc handles have type `Proc` `` — would have
been wrong as written. The E0206 on `fn watch(p: proc)` now names
`Proc[T]` and `Scope`, with the suggestion marked as carrying a
placeholder, because the completion type is a fact about the callee
and not a rewrite the parser can perform.

**And the first program to exercise D16's handle-as-parameter
deadlocks** (#431). `fn fan_out(s: Scope, ch)` calling `s.spawn(…)`
produced no verdict at all on windows, and read as `1` on linux
x86-64 and macOS aarch64 — the linux row was measured on a
debug-profile build, and **the section below corrects it: the shipping
build deadlocks on linux too** — a clause that has been
normative since D16 and unreachable since D16, because the parameter
had no spelling until now. s170 did not break it; s170 made it
writable, and the first thing written with it found the hole. The
program is carried in #431 rather than in the corpus: there is no
corpus posture that is true on all three hosts at once — `phase:` is
compared for EXACT equality against the deepest passing phase, and
`member: true` compiles the file into its directory's module, where
its `main` collides with every sibling (E0302, measured). The typing
half stays covered and green everywhere by `wolf_sema`'s
`conc_handles_clean_in_signature` snapshot, which checks this exact
signature pair.

**A hanging corpus entry is now a named red, not a cancelled job.**
Finding #431 took two CI cycles because the first one was *cancelled*
at the workflow's 30-minute cap with nothing in the log but a gap:
`cargo xtask lane-coverage` ran each entry with no timeout, so one
program that never returned consumed the whole budget anonymously.
It now kills an observation that exceeds 60 s, names the file and the
lane, **keeps sweeping**, and lists every hung entry at the end — one
name is a symptom, the list is the diagnosis. The red path was
exercised deliberately, with a planted non-terminating entry, before
the gate was trusted.

### The linux native lane deadlocks too, and no `cargo test` could see it

**Why every gate we had was green on it.** The defect is a dangling
stack slot, so whether it is *observable* depends on whether anything
has overwritten the dead frame before the task reads it — and that
changes with the profile `libwolf_rt.a` was built in. One host, one
commit, three builds:

| build of `wolf` | `conform-run --native` |
| --- | --- |
| `target/debug` — what every `cargo test` uses | `exit(0)`, stdout `1` — 3/3 |
| `target/release` | rc 124 at 30 s — 3/3 |
| `cargo xtask dist` — the archive that ships | rc 124 at 30 s — 3/3 |

`cargo xtask lane-coverage` runs `target/debug/wolf`, so the corpus
entry trunk removed **could not have gated this on linux either**: it
would have been green there while the shipping compiler deadlocked.
A green whose colour is decided by the build profile of the tool under
test is another shape of a green that cannot fail, and it is why the
gate that landed is structural rather than behavioural.

**Where the wait is, and why.** `main` parks in
`wolf_rt::task::scope::ScopeInner::join`
(`crates/wolf_rt/src/task/scope.rs:272`), reached from
`__wolf_rt_scope_join_free`, with all sixteen workers idle — the scope
is waiting on a child that never reports done. The cause is four lines
of `--emit=wir`:

```text
fn @fan_out(ptr, i64) {
  %3: ptr, %4: mem.r0 = stack.alloc %2
  %5 = store.i64 %1, %3, %4
  %9 = call @__wolf_rt_scope_spawn(%0, %6, %3, %7, %8, %5)
  ret
}
```

The task's capture record is a frame slot of `fan_out` —
`pack_task_env`'s straight-line arm, written at s86 on the premise
that "the scope joins before the frame dies". That premise is exactly
false for D16's handle-as-parameter: the scope joins in `main`, and
`fan_out` has already returned. With an `int` capture the task prints
an address instead of `7`, a different one every run; with a
`channel[int]` capture the same garbage is read as a channel and the
child never completes.

**The witness is back under a gate, and the fix is not in this
entry.** `crates/wolf_driver/tests/scope_handle_param.rs` asserts the
invariant on the compiler's own WIR — a function that hands
`__wolf_rt_scope_spawn` a `stack.alloc` env must be a function that
also joins that scope — beside two behavioural witnesses that refuse
to run without a release-profile `wolf`, because a debug-profile green
is not evidence about this program. A third fixture, spawning from the
frame that opened the scope, is the control: it proves the checker is
reading the defect and not merely reading `stack.alloc`. The fix needs
the record to live in the scope's arena, reached through the handle at
runtime, and the frozen five-parameter `__wolf_rt_scope_spawn` carries
no env size — it is s87's `__wolf_rt_proc_spawn_outcome` copy applied
to tasks, an ABI change, not a short one. The gate lands alone and
red, deliberately: a gated hang is worth more than a half-fix.

### A frozen value handed to two procs is one value

**`spawn proc` shares frozen data instead of moving it** (#312;
`[conc.proc.arg]`). The mem tier evaluated a spawn argument as a bare
value, which moves out of a place whatever the callee's parameter mode
is — so handing one frozen snapshot to two procs was E1001 on the
second `spawn`, while **the same two calls written as plain calls
compiled and ran**, because a `read` parameter's place is read, not
moved. `[mem.region.freeze.1]` (immutable forever, shareable across
threads) and `[conc.chan.imm]` (`imm` crosses by reference, no move)
both said the spawn surface was the wrong one. Everything that is not
frozen still moves: a proc outlives its spawner, and the move is what
makes `[abi.native.procenv]`'s per-proc copy honest. wolf-book ch16
§16.4's snapshot block prints `2 2` on the native lane now, as
`corpus/conc/freeze_proc_snapshot.lu`.

### A tool that no longer promises a reproduction it cannot make

**`wolf test --schedules=N` stopped printing "reproduce any run with
`--replay=SEED`"** (#159's X12 row). It printed that before a single
schedule had been explored, and a seed reaches the runtime scheduler's
PRNG only on the native lane (`[sched.seed]`): a `test_*` fn runs on
the checked machine, serially and seed-blind, so N runs there are one
run N times and there is nothing to replay. The banner now states only
the root seed; the copy-pasteable replay line appears where it already
was measured, on runs that genuinely explored; and the summary counts
how many tests explored under a derived seed and how many did not
(`explored`/`unexplored` in `--json`). The flag SPELLING needed no
change: spec/07 `[sched.flags]` decided `--schedules`/`--replay`/
`--chaos` in s36, `--chaos` is a kept name with a parked engine and an
owner, and the divergence #159 reports is lupin's `--seed`/
`--schedule`/`--explore`, filed there.
### A container element is a place

**`f(mut xs[i])` compiles** (#268 ×6, #156 ch28). The c06 residue's
largest row: the native pipe refused `mut` arguments beyond a local
name or a field path of one BY NAME, while the checked machine had run
them since it had places at all. Lending an element is an ADDRESS into
the container's own buffer — no spill, no writeback, and the callee's
writes land where the element already lives. The same walk serves the
write side, so `a.b.c = v` and `xs[i].f = v` compile too ("assignment
through nested places", #268's ch07 row), and the shape that motivated
all of it — the wordtree's `add(mut n.kids[i], w)`, a field of a `mut`
parameter, indexed, lent onward to a recursive call — is one line
again. `Pool[T]` and `Map[K, V]` elements stay refused by name: a pool
element is reached through a generational handle whose runtime shape is
stub 67's second lane, and a map read answers `V ! {none}`, a row
rather than storage, so there is no element address to hand out.

**And the exclusivity rule that makes it sound was one leg short**
(E1002). A `mut` argument is exclusive for the whole call, and the
arguments spelled AFTER it are evaluated inside that claim. D39 already
caught a bare read there (`f(mut a, a.x)`); a nested CALL claiming the
same place was not checked at all. With element lends landed that is
not a nicety: `take2(mut xs[0], grow(mut xs))` had the native lane mint
the element's address, the nested call grow the list and free the
buffer under it, and the program print `1` where the checked machine
printed `6` — a silent wrong answer on both lanes' watch. The same
shape over a spilled field, `take2(mut r.a, wipe(mut r))`, is a
**pre-existing** divergence on 0.2.15: checked prints `1007 2000`,
native prints `8 2000`, both exit 0, no diagnostic. Both are E1002 now,
on both lanes, and `crates/wolf_driver/tests/mut_element_place.rs` is
the cross-lane gate — nine cases, each asserting the lanes agree AND
agree on the right answer; eight of the nine go red against 0.2.15.

### Three product-pattern refusals retired

The #179/#196 residue the native pipe carried while the checked lane
ran it. Each had a corpus pin promising to advance `mem -> run` on the
day it landed, and all three advance in the commit that lands them:

- **an enum or row test inside a product** (`Box { p: Pair(a, _), .. }`,
  the deep-tree class) — a payload-carrying case value is an aggregate
  whose slot 0 is the tag, so the nested test is the same `icmp`
  against a domain built from the FIELD's own sema type, and the
  payload slots recurse back into the product walk;
- **a str literal inside a product** (`("go", n)`) — the product walk is
  a conjunction of pure tests and an inline str equality is one, the
  same `str_eq_inline` the top-level #54 chain uses;
- **an or-pattern inside a product** (`Pair(1 | 2, b)`) — and the pin's
  own name for it, "join params", turns out to have been too
  pessimistic: an alternation in a slot is a DISJUNCTION of that slot's
  tests, so each alternative's conjunction folds to one BOOL and the
  alternatives are `bor`'d. Join params are needed only when an
  alternative BINDS, which stays refused under its own name.

`grammar/match_arm_or_over_product` is still refused on BOTH lanes (the
checked machine refuses or-patterns with bindings), so it stays the
honest pin it was.

### Two design questions answered, both with a no

**`[mem.region.root]` — a region carries no root** (#155's ch16 row).
`[mem.region.intra.1]` makes every object in a region equally reachable
from every other, so none is privileged, and `[mem.region.create.4]`
gives region identity zero runtime representation, so a root would be
the one typed field in an object designed to have none. The entry point
travels beside the region as a `handle T`, which is plain data and
therefore not a cross-region edge.

**`[mem.region.imm.ret]` — `imm` is not a type qualifier** (#155's ch12
row); `-> imm List[Doc]` stays E0204. `imm` is the state of a region's
contents after `freeze`, not a property of a type, and a value-level
qualifier would be a second immutability lattice beside the one the
checker actually enforces. Returning the value out of the frozen region
already crosses by reference with no copy.

A stated no is a result; silence was not.

### The record could not say a program was fine

**`conform-run`'s default lane answered `unsupported` for every clean
program** (#150, #343, and #32 closed into the first). The word means
one thing in the protocol — `[proto.record.unsupported]`, "a construct
in this program is outside my scope", the conservatism ledger — and the
default lane used it for something else entirely: the static ladder
completed, the lowering was clean, and this lane has no execution
engine. Four repos read that record as the compiler's answer about a
program, and the misreading was already load-bearing: wolf-book's
sample runner carries the workaround in its own source ("the bare
verdict is useless here — `unsupported` is also what a perfectly good
program reports"), and wolf-lang#343 filed six clean slice lowerings as
a lowering gap on the strength of it.

**Three facts, three verdicts, from s169.** `fail(CODE)` is the
language declining a program; `unsupported` is this implementation's
coverage not reaching a construct, named in `x-unsupported-construct`;
and `pass` (`[proto.record.pass]`, new) is the ladder's clean stop —
the compiler has no complaint and this lane executed nothing.

**#343 was answered by measuring, not by lowering slices.** All five
`List` slice forms and the `str` slice lower CLEAN at 0.2.15 — no
refusal keys, empty stderr — and reach `exit(0)` on the checked and
native lanes. The verdict they drew was the verdict `print("hello")`
drew. The issue's two options (lower slices, or distinguish the two
facts) were both wrong, because the fact the witness actually hit was a
third one nobody had named.

**The comparison rule is exactly one verdict wide** (`[proto.cmp.pass]`).
`pass` against `exit`/`trap`/`ub` on a full-ladder run is not a
divergence — the passing side did not execute. `pass` against
`fail(CODE)` **is**, and that pair was invisible before: `unsupported`
on either side is excused, so a lane that accepted a program while the
counterparty rejected it compared as agreement. The amendment opens a
hole rather than widening one. `xtask lane-coverage` gates the
invariant over the whole corpus: no default-lane record may spell a
clean lowering `unsupported` at `wir` again.

### A trap's own words, and where it happened

**`trap_message`** (`[proto.record.trap]`, additive within protocol 1).
`[conf.trap.assert]` has always evaluated `assert(cond, msg)`'s second
argument on the failing path; the checked machine evaluated it and
dropped it, so the only channel a runner had for a program's own words
about its fault was `stdout_inline` — the field that carries the
program's OUTPUT. It is never compared: wording is a per-implementation
quality concern (`[proto.record.diag]`'s rule), and an implementation
the clause permits to drop the message must not become a divergence for
doing so. Beside it, `x-trap-pos` gives the trap site as `line:col`
next to `x-trap-span`'s bytes — the native tier has printed `at
<file>:<line>:<col>` since s125 and a record reader had offsets and no
source to resolve them against.

### The exit status, ruled

**`[conf.exit]`** (#32, re-opened inside #150 and closed into it). s125
ruled the TRAP status and left every other class to habit, so `wolf`
and `lupin` disagreed on the class a reader meets first. Now: a
rejection is **2** and a refusal **4** on every implementation; the
trap keeps `[conf.trap.exit]`'s per-implementation numbers, because
134 is forced by a platform convention and 2 and 4 are forced by
nothing.

wolf spelled both **1**, and 1 is what a program that RAN and returned
an error out of `main` exits with. The cost was measured, not
hypothetical: wolf-book's sample runner carried `run(exit=1)` fences
green across **four pin bumps** for two programs the compiler never
built (`book/ch15/s5`, `ch15/ex15-2`), and had to add a check that
greps this driver's closing prose to tell the two apart. wolf also
spelled "your program is illegal" and "I cannot compile this yet" the
same number; lupin already answered 2 and 4 and does not move. The
status is not injective — a program may `return 2` — and the clause
says so: the guarantee is that a rejection or refusal never exits 0 or
1, and a harness that needs the question answered exactly reads the
record, which is what the record is for.

### The warning gate reaches the repo that wanted it

**`--deny-warnings` on `conform-run`** (#49). The flag was on `build`,
`test` and `doc`, and wolf-std consumes the toolchain through
`conform-run` and the record ALONE — so the gate existed and the one
repo asking for it could not reach it. A promoted warning is a
`severity: "error"` diagnostic and the verdict is `fail(CODE)` **at the
rung whose analysis warned**, not a note appended to a `pass`. The
manifest's `lints.*` rules stay unread here, deliberately: a record has
to be reproducible from the file and the command line, because that is
all the counterparty is given. The source `#[allow]` attribute is a
different thing and still wins — it is part of the program.

**#49's second half is a different defect than it was filed as, and
the correction is recorded rather than the ask.** The `warnings` array
is NOT entry-file-only: measured at 0.2.15, a `0.0 - x` in a sibling
module lands in the entry's record as `W0402` with the sibling's span.
Why std's own forty sites never warn is `wolf_sema::wave`'s deliberate
exemption — `check_file` and `check_typed_body` both return early for
any module under `std`, with the stated reason that a user cannot act
on a warning inside a library file. That reason is right for a
consumer and exactly wrong for the library's author, which is the real
shape of the ask; it is filed with the mechanism named, because a fix
aimed at "collect from every module" would have changed nothing.

### `wolf test` tells a red suite from a broken one

**`REJECTED (does not compile)`** (#157). A file the compiler rejected
never ran, so it found nothing, so it is not a failing test — and
`wolf test` spelled both `FAILED (does not compile)` in one column
through three pins of wolf-book's ch18 ledger. The report and the
`wolf-test/0` JSON now carry a fourth status and a fourth counter; the
exit status is unchanged, since a non-passing row was always red
(`[conf.exit.test]`). A DOCTEST that does not compile stays `FAILED`,
and that is not an inconsistency: compiling is what a doctest asserts
and only a precondition for a test file.

**`wolf test` honours `--error-limit`** (#75). `build` and `run` have
capped their reports since s63; this surface printed every diagnostic a
wrecked module could produce, which is the terminal-scrolling the cap
exists to prevent.


### The checked machine answered a question wrongly, and said nothing

**A call through a fn-typed parameter reached a top-level fn of the
same name** (#400). On the checked lane only, and silently: the program
compiled, ran, and printed the wrong answer with no diagnostic anywhere,
while native and lupin were both right — so the differential could not
see it either. `fn apply(le: fn(int, int) -> bool, …) { le(x, y) }`
called the module's own `fn le` instead of its parameter, across module
boundaries too, which is how sc50 met it through `std.list.sort_by`.
A caller cannot defend against this, because the library picks the
parameter's name.

`[conf.resolve.ambient]` already ruled it — a bare name in call position
resolves at the innermost lexical binding first — and this was the same
mistake that clause was written to close, a call path deriving dispatch
from the spelled name instead of asking the resolver, one scope further
in. **No wolf-lang corpus program was affected**: all 624 entries give
byte-identical verdicts, stdout hashes and diagnostics before and after.
wolf-std's `list.sort_by` was affected, and is what found it.

### Two ledgers the checked tier was not keeping

**The five `str` producers charge the ambient region** (#391).
`[mem.region.escape]` makes `upper`, `lower`, `repeat`, `replace` and
`str_from_utf8` allocation sites "exactly as `+` is", and
`[mem.region.account.1]` charges every materialization. Native and lupin
did; the checked tier charged `+` and interpolation and none of the five.
The visible half: a `region idle(cap: 0)` block RAN all five where the
other lanes trapped `alloc-contract`. The checked tier was the wrong one.

**A `[mem.str.view]` product carries its receiver's sites** (#392) — a
clause question, ruled rather than a divergence fixed, because all three
lanes agreed on the unsound reading. A view of a region-built `str`
escaped its region clean, returning a view into freed bytes, while the
`str` itself leaving was already E1010: the same bytes, one word
narrower. Allocating nothing and pointing nowhere are different
properties — "allocates nothing" governs the account, and a view still
charges zero, so `#[noalloc]` is untouched. That shape is now E1010.
Only the receiver's sites flow, and `bytes()` is excluded because
`[mem.str.view.lend]` materializes a real copy in the very positions
that would escape. Measured before it landed across corpus, wolf-std,
lobo and boreutils — 23,722 files — **zero rows moved**. lupin 0.1.37
does not follow yet; the wolf-interp mirror is filed.

### The parser reports one wreck once (s172, #420)

**A malformed generic parameter list is one report, not one per
element — and so is a malformed paren type list.** `[]` is the generics
bracket, so a declaration keyword landing in front of an array literal
re-reads `[4, 1, 3, 2]` as a generic parameter list, and the parser said
"expected a generic parameter name" once per element: four reports for
four elements, ten for ten, a cascade that grew with the source rather
than with the damage. The nightly's blast-radius property found it at
`MUTATE_BUDGET=300` on `corpus/methods/comb_sort_enumerate_zip.lu` (six
added cascade against a structural bound of five; run 35580391507), and
once that list was capped the same property found the second shape:
deleting the inner `)` of `fn apply(le: fn(int, int) -> bool, x: int,
y: int)` made the fn-type's parameter list swallow the remaining
parameters and report a pair for each, 2n + 1. Both lists carry a
single-report latch now; recovery still runs per element, so the tree and
the stop points do not move. The per-PR gauntlet runs the property at a
budget of 3 and never generated the mutation, which is why a merged lane's
green could not see it; the property's bound stays where it was. Gates:
`crates/wolf_parse/tests/diagnostics.rs`, both witnesses.

### The pairing — lupin 0.1.38, and the pin is the released tag

`crates/wolf_driver/PAIRING` moves to **0.1.38**, which declares
**`2e4ca76`** — v0.2.15's own commit. For the first time the pin a
downstream reads off `lupin --version` is a commit on this repository's
released line: 0.1.37 declared `41695e7`, an s166 branch head kept alive
only by `refs/tags/lupin-0.1.37-conformance-pin`, and
`git merge-base --is-ancestor 41695e7 v0.2.15` answered no
(wolf-interp#127). 0.1.38 exists to make that sentence true, and it does.

Measured with `xtask differ --triage`, control (#281) against the
0.1.37 release archive, both arms over 690 files (646 entries), columns
agreement / completeness / soundness / unsupported / hard:

|  | checked | native |
|---|---|---|
| control (0.1.37) | 377/182/0/118/10 | 416/182/1/75/14 |
| main (0.1.38) | 380/182/0/117/8 | 419/182/1/74/12 |

**The interpreter bump moved 4 ledger counts on each tier, and the two
tiers moved together again.** Predicted (2–8, together): r20's
asymmetry came from the s166 method surface the checked tier declines,
and nothing in is52 or is53 touches it. The movers, all four named by the
control: `grammar/range_header_inclusive_max.lu` and
`grammar/range_value_wide_iter.lu`, which 0.1.37 could not run at all
(it materialized the range — the 28 GB RSS r20 recorded) and which are
agreements now; `memory/map_remove.lu`, unsupported → agreement (#118's
mirror); and one the prediction did not name —
`rows/error_alias_qualified/main.lu` went from a diagnostic note to a
**verdict divergence**, because wolf runs it (s175,
`[type.err.alias.qualified]`) and lupin 0.1.38 answers `fail(E0305)`:
its new item-import rule judges a `use m.Alias` whose only use is an
error row as unused. That is the interpreter's side of #434, filed as
wolf-interp#134. One file moved *below* the ledger, where no count can
see it: `typecheck/variant_bare_value.lu`, which lupin now refuses at
resolve (`exit(0)` → `fail(E0301)`) under a `fail`-pinned header, so its
class did not move. The one native soundness finding is in both arms and
predates the bump.

### The training-data permission

`LICENSE-TRAINING-DATA` — the wolf Training Data Permission, version
1.0, an additional permission under GPL-3.0 section 7 — joins `LICENSE`
at the root, and the README's license section points at it.

## 0.2.15 — 2026-09-16

Six lanes, and one of them changes what a program you already wrote
means. That goes first, because it is the first time in this project's
history that a pin bump has done it.

### Read this before you bump the pin

**`push` copies now, and `push(take x)` moves** (#385). Up to v0.2.14,
handing a non-`Copy` value to a builtin container store kept the
caller's handle alive inside the container: `(mut outs).push(xs)`
followed by `(mut xs).push(4)` printed `outs0=2 xs=2`, two writable
paths to one value with no `shared` spelled anywhere and no diagnostic.
That was `[mem.tier0.excl.1]` violated with a builtin as the callee, and
both compiler tiers agreed with each other while disagreeing with lupin,
so the differential could not see it either. At 0.2.15 the element is
**copied** in, at exactly `[mem.tier0.move.3]`'s cost, and the program
prints `outs0=1 xs=2` — which is what lupin printed all along.

**This is a semantic change, not a diagnostic change.** A program that
compiled at 0.2.14 and still compiles at 0.2.15 can print different
bytes. Nothing warns, because there is nothing wrong with the new
answer; the old one was the defect.

**What it costs, and the spelling that gives the old cost back.**
Scalars and `str` are unaffected and stay free — a value that reaches no
heap storage is its own copy, and a `str`'s bytes are immutable and
shared by the copy. The cost lands only on a plain push of an element
that reaches the heap: measured on lobo's `GenRow` shape, 160,000
pushes, **0.26 s and 136 MB against 0.06 s and 94 MB** — 4.3× wall,
+42 MB, about **1.25 µs per plain push**. A program that wants the old
zero-cost handover spells **`take`**: `push(take xs)` moves the value
and `xs` is dead after the call (E1001, with the ladder's `copy x`
fix-it). `take` is the only spelling that restores the old cost, and a
store is the one `read` position where a call site may spell a mode at
all.

**Who meets it downstream, by count.** Across wolf-std, lobo, wolf-book
and the corpus there are roughly **3,100** plain `push` sites, exactly
two spelled `copy` and none spelled `take`; the great majority push
scalars or `str` and cost nothing. The set that actually regresses is
**28 in-loop pushes** of a container or heap-shaped struct (26 unique):
**lobo, 12 sites** in its `src/main.lu` generation table, **four of them
per request**; wolf-std#44 (json's `parse_array`, and the generic-`T`
rebuilds in `sort`, `list`, `map` and `list_eq` that become copies at a
heap instantiation); and wolf-book#57 (the wordtree family and two DP
tables). wolf-interp#119 is the pairing row, and it is the good news:
**lupin 0.1.36 already agrees with both halves**, the `take` half
included, which was expected to diverge and does not.

**Beside it, and arriving at the same pin,** #366's lend rule makes a
move out of a `read` parameter lent rather than given away — a static
refusal costing nothing at run time — and that one meets **wolf-std at
70 sites** (wolf-std#39: 20 E1002 returns and 50 E1014 reader cursors in
`tls`) and **wolf-book at 1** (wolf-book#56). Two different rules, two
different site counts; bump once and meet both.

**And the reason the `copy` fix-it now works.** Native `copy x` lowered
as `x` for every heap value (#384), so **every `copy` fix-it the memory
checker ever printed was inert on the tiers people actually ship**. It
is type-directed and deep now. That is what makes #385's ruling sound:
`push`'s default copy is a real copy on every lane.

### The rest of the release

`xs.map(f)` is `list.map(xs, f)` and the ten combinators run as the std
wolf code they are, with `xs.par(f)` beside them at a measured 11.2× on
sixteen cores (s166). The checked tier gets its own 64 MiB stack and a
named `CALL_DEPTH_BUDGET`, walks a wide range instead of collecting it,
binds a raw row in argument position, and serves channels from the root
task (s161). #146 turns out to have been a **miscompile**, not an ICE —
the release tier's `noalias` guard read a stale `len` and returned 64
where 72 was correct — so lobo can drop `WOLF_MIDEND=0` at its next pin
(s162). And sixteen spec debts close, anchors 499 → 524 (s163, s166).

### The pairing — lupin 0.1.37, and the combinator set is TEN

`crates/wolf_driver/PAIRING` moves to **0.1.37**, which declares
`41695e7`. The interpreter took **two** pin moves inside one release
(`a7f517e` → `30731a6` → `41695e7`), so three interpreter lanes land
behind one tag and this is the largest interpreter bump any cut has
measured.

**The pin is preserved by a tag, and that is new.** `41695e7` is an s166
branch head; the rebase-merge that landed s166 on trunk left it reachable
from no branch and no tag, so a clone could not resolve the sha this file
names and `lupin --version` prints. It is now
**`refs/tags/lupin-0.1.37-conformance-pin`** in this repository — a tag
rather than a branch, deliberately, because it is a fixed point and not a
line of development. A later release lane that meets a PAIRING pin it
cannot check out should look for a tag of that shape before concluding
the commit is lost.

Re-pinning to a trunk sha instead was tried at this cut and **reverted**,
and the reason is a finding about the interpreter's mirrors rather than
about the pin: vendored at `12ca8acc`, lupin OOMs on
`grammar/range_value_wide_iter.lu` and `range_header_inclusive_max.lu` —
it materializes the range that s161's #381 taught the checked machine to
walk, a single 5 GB allocation reaching 28 GB RSS before the kernel
stepped in — and its corpus runner rejects s163's `run(exit=nonzero)` in
`conc/proc_link_root_death.lu` by name. Both are wolf-interp#115/#118
territory (is52), and until they are mirrored no trunk sha is a
shippable pin.

Measured with `xtask differ --triage`, control (#281) against the
0.1.36 release archive, both arms over 666 files, columns
agreement / completeness / soundness / unsupported / hard:

|  | checked | native |
|---|---|---|
| control (0.1.36) | 348/180/0/110/24 | 374/180/0/83/25 |
| main (0.1.37) | 367/177/0/113/**9** | 395/177/0/84/**10** |

**The interpreter bump moved 26 ledger counts on checked and 28 on
native** — and that asymmetry is the cut's one surprise. Every bump
since r13 has moved the two tiers together, and it was predicted to
again. It did not, by exactly two files, and the cause is on *wolf's*
side rather than the interpreter's: the checked tier declines s166's
whole method surface, so two entries that become agreements on native
stay `unsupported` on checked (unsupported +3 checked against +1
native, the same two files). A six-release regularity retires because
the language grew a surface one of its own tiers refuses.

**The set is ten, and the tenth is `sorted`.** `[type.comb.set]` names
`map`, `filter`, `fold`, `sum`, `sort_by`, `sorted_by`, **`sorted`**,
`enumerate`, `zip` and `collect` — ten std wolf functions — with `par`
an eleventh row that is the language's own builtin. `sorted` is worth
naming because it is easy to miss: it was written early, withheld while
`use std.cmp` in `std.list` darkened every importer on the native rung,
and carried in on a rebase once that cleared, so wolf-std's own
changelog headline still reads "nine functions … and `sorted` named
where it is missing" while its body records that `sorted` shipped. A
nine-row table drops it silently. Derived for ten, each probed with a
list built by `push` rather than a literal so that 0.1.36's refusal is
the method surface and not is49's list-literal clause:

| # | combinator | predicted | lupin 0.1.36 | lupin 0.1.37 | wolf native | wolf `--release` | checked |
|---|---|---|---|---|---|---|---|
| 1 | `map` | pairs | unsupported | `8 2 6 4` | `8 2 6 4` | `8 2 6 4` | unsupported |
| 2 | `filter` | pairs | unsupported | `2 4 2` | `2 4 2` | `2 4 2` | unsupported |
| 3 | `fold` | pairs | unsupported | `10` | `10` | `10` | unsupported |
| 4 | `sum` | pairs | unsupported | `10` | `10` | `10` | unsupported |
| 5 | `sort_by` | pairs | unsupported | `1 2 3` | `1 2 3` | `1 2 3` | unsupported |
| 6 | `sorted_by` | pairs | unsupported | `1 2 3 3` | `1 2 3 3` | `1 2 3 3` | unsupported |
| 7 | **`sorted`** | pairs | unsupported | `1 2 3` | `1 2 3` | `1 2 3` | unsupported |
| 8 | `enumerate` | pairs | unsupported | `2` | `2` | `2` | unsupported |
| 9 | `zip` | pairs | unsupported | `2` | `2` | `2` | unsupported |
| 10 | `collect` | pairs | unsupported | `4 2 5` | `4 2 5` | `4 2 5` | unsupported |

**Ten of ten predicted, ten of ten measured.** All ten were
`unsupported` on lupin 0.1.36 and all ten run at 0.1.37 with output
identical to both of wolf's native tiers; all ten are `unsupported` on
the checked tier, which declines the method surface by name — that is
`[conc.task.par.det]`'s posture for `par` applied to the whole set, and
it is `unsupported` rather than wrong.

**Three files break the corpus run on both arms**, cancel in the delta,
and are named here rather than compensated for. They are this release's
own new entries, unmirrored on the interpreter side and wolf-interp's
to take: `grammar/range_value_wide_iter.lu` and
`range_header_inclusive_max.lu`, where lupin materializes the range that
s161's #381 taught the checked machine to walk, and
`conc/proc_link_root_death.lu`, whose `run(exit=nonzero)` directive
(s163's #371) lupin's corpus runner does not know.

### The element edge (s167 — wolf-lang#385 ruled option 3, #387 ruled both)

**A builtin container store COPIES its element now, and `push(take x)`
moves it** (#385). `push`'s value parameter is a `read`, and a read is
a lend — so the builtin kept the lent handle inside the receiver past
the call, and the container's element and the caller's binding were one
value. Measured at v0.2.14, `(mut outs).push(xs)` then
`(mut xs).push(4)` printed `outs0=2 xs=2` on `wolf run`, on `--release`
and on the checked machine, while lupin 0.1.36 printed `outs0=1 xs=2`:
two writable paths to one value, no `shared` spelled anywhere and no
diagnostic — `[mem.tier0.excl.1]` violated with a builtin as the
callee. Both wolf-lang tiers agreed with each other and disagreed with
lupin, so the differ could not see it either.

Ruled option 3: the default is safe and the fast path is visible. A
plain element is copied in, at exactly `[mem.tier0.move.3]`'s cost for
`copy`; `push(take x)` moves it and `x` is dead after the call (E1001,
with the ladder's `copy x` fix-it). This is the one place a `read`
parameter's call site may spell a mode at all — X1 otherwise demands
the site's spelling equal the declaration (E1007) — and `take` is
admitted because a store is the one read position that KEEPS what it
was lent. A silent take (option 2) was refused for the reason the
ruling records: it would have made `push` the one place in the language
where handing a value over plainly destroys it.

**`Copy` elements and `str` are unaffected and stay free.** A value
that reaches no heap storage is its own copy, and a `str`'s bytes are
immutable and shared by the copy. The cost lands only on a plain push
of an element that reaches the heap. Measured on kasumi, same binary,
plain against `take` over 160,000 pushes of a struct holding eight
`List[str]` (lobo's `GenRow`/`Conf` shape): 0.26 s and 136 MB peak RSS
against 0.06 s and 94 MB — **4.3x wall, +42 MB, about 1.25 us of copy
per plain push**. A program that wants the old zero-cost handover
spells `take` and says so.

The spelling needed no grammar work: `call_arg ::= ('mut' | 'take')?
expr` was already in `[gram.expr]` and in the parser, and
`(mut outs).push(take xs)` parsed and typechecked at v0.2.14 — it was
refused at `mem` by E1007, the X1 mode-agreement rule, because
`push`'s value parameter declares no mode. A `ParamSig` now carries
`store`, sema declares it on `List.push`'s and `Pool.init`'s value,
and the memory checker admits a site `take` there and nowhere else.

**The index store is NOT aligned, deliberately.** `m[k] = v` and
`xs[i] = v` still MOVE their right-hand side (a later use is E1001 —
the opposite of `push`'s defect), and `m[k] = take v` does not parse at
all: the assignment grammar has no mode slot on its right (E0201, at
the parse phase, on both tiers). Aligning it would change what every
existing index store means, so the ruling on that half is open and
`[mem.region.edge.elem]` states only what is implemented.

**W1004 retires, and `copy bs` is a read position** (#387). s165's
#366 made every escape the lend analysis can PROVE a refusal in the
CALLEE (E1002/E1014), naming the problem where it can be fixed — so
W1004 either fired beside an error for one root cause (VOICE rule 5)
or, on the one shape #366 leaves legal, claimed something untrue.
`Lend::Escapes` folds into `Lend::Opaque`; the degraded lend is a
silent copy again, which is what it always compiled to. Separately,
`copy bs` is now a read position in the analysis, so
`fn keep(bs: List[byte]) -> List[byte] { copy bs }` is `Lendable` and
the call lends for free — sound only because #384 made native `copy` a
real copy. `corpus/memory/byte_view_escape.lu` drops its
`warns: W1004` header and pins the E1002 refusal alone; it stays a
static refusal, so the lane-coverage row s165's #366 moved out of the
run lanes is not restored by this change and the floors stand.

**The blast radius, measured and filed.** Across wolf-std, lobo,
wolf-book and the corpus there are roughly 3,100 plain `push` sites,
exactly two spelled `copy` and none spelled `take`; the great majority
push scalars or `str` and cost nothing. The regression set is 28
in-loop pushes of a container or heap-shaped struct (26 unique):
wolf-std#44 (json's `parse_array`, plus the generic-`T` rebuilds in
`sort`, `list`, `map` and `list_eq` that become copies at a heap
instantiation), wolf-book#57 (the wordtree family and two DP tables),
and twelve sites in lobo's `src/main.lu` generation table, four of
them per-request. wolf-interp#119 is the pairing row: **lupin 0.1.36
already agrees with both halves** — including the `take` half, which
was expected to diverge and does not.

Witnesses: `corpus/memory/push_copies_element.lu` (the issue's own
program, the `put(mut out, b)` shape one call deeper, `copy`, `take`,
and the `Copy`/`str`/struct cases) and `corpus/memory/push_take_moves.lu`
(`fail(E1001)`).

### Papercuts V (s165 — wolf-lang#366, #384; #348, #347, #352, #349; #351, #344, #370, #379)

**A move out of a `read` parameter is lent, and it may not outlive the
call** (#366, `[mem.tier0.mode.read]`). `fn f(b: List[int]) -> List[int]
{ b }` handed the caller back a second live path to its own list: at
v0.2.14 `ys=4 xs=4` on both compiler lanes, where lupin 0.1.36 prints
`ys=4 xs=3`. The measurement found three more ways in: `var c = b; (mut
c).push(9)` and `let c = b; eat(take c)` grew the caller's list from
inside the callee, and `out = b` into a `mut` parameter aliased it.
The value is lent now. Returned, sent, stored into module state or a
`mut` parameter, or carried into a `take` temporary, it is **E1002**.
Lent `mut`, taken, or written through a container step via a binding
that holds it, it is **E1014**. Refusal is limited to values that can
reach shared storage (a `List`, `Map`, `Pool`, or a type parameter). A
struct of scalars, a `str`, an `int ! {none}` fallback, a scrutinee
piece used in place, a rebinding that is read or replaced whole, a
cursor's own field writes, and every `take` parameter stay legal. The
rule is static and costs nothing at runtime. Ten corpus entries now
hand back `copy` (or `take v`); `byte_view_escape.lu` is `fail(E1002)`
and W1004's future is #387. **wolf-std meets 70 sites at its 0.2.15
pin** (wolf-std#39): 20 E1002 returns and 50 E1014 reader cursors in
`tls`.

**`copy` copies on the native tiers** (#384, `[mem.tier0.move.3]`).
Native `copy x` lowered as `x` for every heap value, so `var ys = copy
xs; (mut ys).push(2); ys[0] = 9` left `xs=2 xs0=9` under `wolf run`
and `--release` while the checked machine and lupin printed `xs=1
xs0=1`. The same was true of a `Map` and of a struct holding a `List`.
Every memory-checker fix-it that says `copy` was inert on native. A
`List`, `Map`, struct or tuple is now copied deep. Cost: one allocation
and a buffer copy per list or map reached. Shapes not modelled yet
refuse by name (a `Map` whose values reach the heap, a heap-carrying
enum payload, a `Pool`, a `shared` cell). `RT_SYMBOLS` 144 → 147 across
this entry (`map_remove`, `list_copy`, `map_copy`).

**The first hour.** A bare `Less` under `enum Ordering` in value
position is E0301 naming `Ordering.Less`, with the edit
(`[gram.expr.variant]`, #348), where it used to decline as an
out-of-context row tag. `refund(340)` under `[T: Neg]` with only `impl
Neg for int` compiles: a bound is context, so the literal takes `int`
when `i32` does not satisfy the bound (`[type.numlit.default]`, #347).
E0502 says why it names `i32` when two other integer types would. `use
cmp.Eq` above an `==` is no longer E0305 (#352). `List[(str, int)]()`
compiles on every lane (#349).

**Tooling.** `wolf fmt` breaks a struct pattern that carries a `..`
rest, ending at the rest; wolf-std's three over-width lines re-lay at
the pin (#351). `m.remove(k) -> V ! {none}` erases a key on every lane,
and the survivors keep their order (#344, D50). A `SyntaxKind`
reordering, or a lexer keyword mapped outside the keyword run, is a
compile error of `wolf_ast`/`wolf_parse` (#370; both planted breaks
went red). `wolf lsp` paints an error-set alias as `type` where it is
used: wolf-lsp's `semanticTokens-error` transcript gains exactly two
tokens at its next pin, `IoErrors` at 0-based line 13 column 22 and
`ConfigErrors` at line 15 column 27 (#379).

**Filed, not taken:** `push` of a non-`Copy` value keeps the caller's
handle on both compiler lanes (#385, a ruling). W1004 is subsumed by
the #366 refusal (#387). lupin mirrors: wolf-interp#115 (#366) and
wolf-interp#118 (#348, #344, #352).

### Methods on std data, and `par` (s166 — wolf-lang#390 ruled option 1, #154 settled)

**`xs.map(f)` IS `list.map(xs, f)`** (`[type.method]`). A method call on
std data resolves, after the type's own builtins, to the home module's
`pub fn` whose first parameter carries the receiver's type constructor —
and it is called as that free call, same instance, same code, so the two
spellings are one call by construction rather than by agreement. The
homes are a closed table: `List[T]` → `std.list`, `Map[K, V]` →
`std.map`, `str` → `std.str`, `range[int]` and `range[char]` →
`std.range`. A home module needs no `use` and binds no name; it loads
when a method call in the program names one of its `pub fn`s, under a key
no source can spell. **Both `methods on generic std data` refusals are
gone** — the two `NotYet` sites are deleted, not widened.

The refusals are the counterparty's existing codes, by name: **E0301**
where no std root is configured, naming the home module it could not
reach; **E0403** for no candidate or a first parameter the receiver does
not fit; **E0402** for arity, counted against the written arguments with
the receiver excluded; and **E0804** where a spelled receiver mode
disagrees with the candidate's first parameter, so a `mut` candidate is
`(mut xs).sort_by(less)` and the bare spelling traps `exclusivity`,
exactly as it does on a builtin. `clear` joins the builtin step on `List`
and `Map`.

**`take` is the ownership verb and never a method name**
(`[type.method.take]`, closing #154). `xs.take(n)` is E0403 with a note
naming the two slices that mean it — `xs[..n]`, or
`xs[..min(n, xs.len)]` when a short list must not trap — and `xs[n..]`
for the suffix. The word is reserved at every name position, so this is
a refusal the reader meets once and never again.

**The combinator set is ten functions plus one builtin**
(`[type.comb.set]`). `map`, `filter`, `fold`, `sum`, `sort_by`,
`sorted_by`, `sorted`, `enumerate`, `zip` and `collect` are **std wolf
code**, written by wolf-std (sc50), each with its row and its cost stated
in the clause; `par` is the language's own and the only builtin among
them (`[type.comb.builtin]`). The combinators are eager
(`[type.comb.eager]`). `collect` is one generic function over `range[T]`
rather than one per element type, because wolf has no overloading.

**What a combinator copies is now stated**, with s165's and s167's
rulings underneath it: the receiver is `read`, so a combinator never
hands the caller's own value on; `fold`'s accumulator starts as the lent
seed and outlives the activation, so the seed copies; and the elements
`filter`, `sorted_by`, `sorted`, `enumerate` and `zip` keep are `push`ed
into a fresh `List`, which under #385 copies a non-`Copy` element unless
the site spells `take`.

**`par`.** `xs.par(f)` opens a scope, splits `0..n` into
`k = min(n, W)` contiguous chunks whose lengths differ by at most one,
spawns a task per chunk, joins, and reads the slots back in index order,
so `out[i]` is `f(xs[i])` whatever the schedule — `xs.par(f)` and
`xs.map(f)` are the same `List` for every total `f` that records no
event. `W` is the worker count the runtime schedules onto, one per
logical core on the native pool. `k == 1` may run on the calling task
with no spawn and `n == 0` spawns nothing. A chunk stops at its own first
failure, the scheduler cancels its siblings, and the first failure in
schedule order becomes the `par`'s row: **a failed `par` has no value**,
so a partially filled `out` is never observable. `f` is checked as a
spawned closure's body, and a write to captured enclosing state is
**E1101** at the write with the `par` as the spawn site. Cancellation is
polled every 64 elements, because a per-element poll would cost more than
most `f`. `RT_SYMBOLS` gains `__wolf_rt_par_map`.

**The cost clause does not promise a speedup**, and says so
(`[conc.task.par.cost]`): one allocation of `n` slots — exactly `map`'s —
no copy of `xs`, `k` spawns and one join, and nothing per element beyond
the call of `f`. Because tasks run with the process root as their ambient
region and the native root arena serializes allocation behind one lock,
an `f` that allocates on every call contends there and scales worse than
one that computes; a list shorter than `W`, or an `f` cheaper than a
spawn, can make `xs.par(f)` slower than `xs.map(f)`, and both are
conforming.

**Measured at the cut**, because the lane landed no bench and the clause
promises no speedup: on kasumi (16 logical cores), a CPU-bound map over
**2,000,000** elements, compile excluded, `par` against the same program
written as a serial loop — **1.91 s serial against 0.17 s, 11.2x**, the
same on `wolf build` and on `wolf build --release`, with both tiers
printing output byte-identical to the serial version. That is one shape
on one host, not a promise: `[conc.task.par.cost]`'s sentence still
governs.

**The checked tier declines the whole surface.** Home-module method calls
are refused by name in checked execution, and `[conc.task.par.det]` says
the same of `par`: the tier refuses `scope` and closures today, so it
refuses `par` — `unsupported`, not wrong. **Every "both tiers" in this
entry means the two native tiers**, never three lanes.

Sixteen anchors are new and none is renumbered: `[type.method]` with
`home`, `resolve`, `root` and `take`; `[type.comb]` with `eager`,
`set` and `builtin`; and `[conc.task.par]` completed with `order`,
`fail`, `capture`, `chunk`, `det` and `cost`. Two new sections join
`spec/10-types.md` — §15 methods on std data and §16 the combinators.

Witnesses: `corpus/conc/par_order.lu` (100,000 elements),
`par_fail_reraises.lu` (element 70,000 fails, the row re-raises after the
join), `par_capture_write.lu` (`fail(E1101)`), the combinator and
home-module witnesses under `corpus/methods/`, the six method-surface
refusals, and `corpus/methods/wordcount_serial.lu` against
`wordcount_par.lu` — the same program with `pieces.map(count)` changed to
`pieces.par(count)`, carrying a byte-identical `check:` line. The std
bodies those fixtures resolve against are **the spec's reference
bodies, not wolf-std**: wolf-std's own modules carry the real ones, and
nothing under `corpus/methods/std/` ships.

### The checked tier (s161 — wolf-lang#381, #382; #201, #342)

**The checked machine runs on a stack it sizes itself** (#382,
`[exec.checked.budget]`). The call-depth bound was a bare `128` in the
machine's own loop; it has a name now, `CALL_DEPTH_BUDGET`, and the
machine runs its program on a thread of its own sized
`CHECKED_STACK_BYTES` — 64 MiB, the same on every host — instead of on
whatever stack its caller happened to bring. On the caller's stack it
answered on a unix main thread (8 MiB) and overflowed a windows one
(1 MiB), where three wolf-std json rows reach `CALL_DEPTH_BUDGET` with
about 1.2 MiB of host frames: one program, conforming on one host and a
crash on another, with nothing in the tier's own text deciding which.
Measured at v0.2.14 on x86-64 linux, the deepest wolf-std rows need
~1.23 MiB at the budget in a release build, and a 128-deep recursion
needs more than 8 MiB — and no more than 16 — in a debug one. A call
past 128 frames is `unsupported` (`call depth budget exhausted`), as it
was. Cost: one thread per run and 64 MiB of **reserved address space**;
resident memory is only the frames the program actually reaches. Two
alternatives are weighed and rejected in the clause: keeping the 8 MiB
unix default, which a debug build overflows at the bound, and deriving
the budget from the host's stack, which would make `unsupported` a
property of the machine rather than of the tier. A host at its thread
limit falls back to the caller's stack rather than failing the run.
Witness: `ubcheck`'s
`recursion_to_the_depth_budget_answers_on_any_callers_stack`, which
runs the machine from a 256 KiB thread.

**A wide range is walked, not collected** (#381). The checked machine
built a range's items before iterating them, so `0..int.MAX` was an
allocation the size of the range rather than a loop, and a `for` header
was normalized before it was walked. A range is walked now — the item
sequence is an iterator, never a materialized list — and a `for` header
is never normalized. An inclusive header at `int.MAX` walks to its end
where it used to die; an inclusive range **value** at `int.MAX` still
traps where it is built, which is where the overflow is. A range that
never exits is the step budget's business, not memory's. Witnesses:
`corpus/grammar/range_value_wide_iter.lu` and
`corpus/grammar/range_header_inclusive_max.lu`, both on both tiers.

**A raw row binds in argument position** (#201). A call that raises,
used directly as an argument, bound its row like a `let` on the path
that raises and left every other flow to leave the call — the checked
machine did not agree. It does now, for the row taken and not taken, an
ignored row, `?`, a handler return, and a row through a method
argument. `corpus/rows/raised_call_arg_position.lu` is the witness, one
word per lane whether the row is taken or not, and
`corpus/rows/handler_diverge_trap.lu` stops saying the checked lane
refuses it: three lanes `trap(assert)`.

**Channels without a spawn** (#342, `[conc.deadlock.trap]`). The checked
tier serves `send`, `recv`, `close` and the draining `for` from the root
task, so a channel program that never spawns runs on all three lanes
instead of being declined by name. And because the tier is one of the
deterministic modes and runs exactly one task, a blocking channel
operation on that only task cannot ever be answered: the tier reports
`trap(deadlock)` at that operation. It costs nothing to detect — with
one task the check is the block itself. The native tier is permitted not
to detect it and, at 0.2.14, waits. **lupin 0.1.36 answers the same
`trap(deadlock)` on the same four shapes.** Witnesses:
`corpus/conc/chan_root_task.lu` and `ubcheck`'s
`a_blocking_channel_operation_on_the_only_task_is_deadlock`; three
existing channel witnesses stop saying the checked lane declines channel
methods. `chan_struct_payload.lu` stays native-only, because the checked
tier still declines `scope` and `spawn`.

This entry mints no anchor and no diagnostic: every clause it touches
(`[exec.checked.budget]`, `[conc.deadlock.trap]`) already existed and is
amended in place. `RT_SYMBOLS` is unchanged by it.

**Filed, not taken here:** the checked tier's two float-cast soundness
rows (#168) and X3 on `List[int]` elements (#152), both still open.

### The mid-end (s162 — wolf-lang#146; #93 pinned; #99 and #102 measured, not taken)

**The release mid-end builds `std.x.crypto.curve25519` again** (#146).
Loop versioning's overlap guard, the runtime check that lets a
list-copying loop run under a `noalias` fact, built each buffer's
extent from the first `len` load of its header in layout order. Two
picks were wrong. A load inside an earlier loop's in-place `push`
branch does not dominate the guard, so the verifier refused the module
(`[dominance]`, the ICE lobo's release step has dodged with
`WOLF_MIDEND=0` since wsm02). A load above an in-place grow dominates
and is stale: the extent comes out short, and an empty extent passes
the guard over storage it overlaps. That second pick is a wrong
answer the verifier cannot see. It is reachable from source on the
native tiers while `copy` of a `List` still shares storage (#384).
`shape(copy c, c)` printed `sum 64 64` under `--release` and `sum 72
72` under `--native`. The extent is now built from a `len` load that
dominates the guard and has no header-moving operation between it
and the loop.

Past that ICE, `sc_muladd` hit a second one. Its guarded loop is
nested, and licm hoisted the guard's `noalias` subjects out of the
guarded region; GVN then folded a second loop's subject onto the
first's (`[fact-just]`). A guard-justified subject no longer moves.
lobo's `src/main.lu` (wolf-std 2d10219) now builds `--release` with
the mid-end on. Cost: none at runtime on either tier. A guard the old
pick would have minted from a load that is not current is not minted.
On kasumi, 400 Ed25519 sign-and-verify rounds plus X25519 run 0.553 s
with the mid-end on against 0.544 s with `WOLF_MIDEND=0`: the flag
bought no measurable time on this host. lobo's ~3.6 s handshake is
re-measured on lobo's own rig at its pin.

Witnesses: `corpus/memory/push_grow_guarded_index.lu` (#146's
reproducer), `corpus/memory/push_grow_nested_guarded_index.lu`
(`sc_muladd` without std), `exec_parity`'s
`stale_len_guard_takes_the_slow_loop_release` (WIR; trunk exited 64
where 72 is correct at -O2), and `release_overlap_guard` (source,
release against native).

**#93's accidents are pinned by code.** `let b = a` then a read of `a`
is E1001, and one `List` passed `mut` and `read` to one call is E1002.
The accident's `copy` half does not hold on the native tiers (#384).

**Measured and not taken.** #99: over 364 corpus entries, 13 bench
kernels and lobo's release build, every region the ambient gate
refuses contains a real container allocation (`__wolf_rt_list_new`).
Zero refusals come from a call that allocates nothing, so a per-callee
`allocates?` summary would unblock nothing. #102: on `word_count`,
PGO saves 10.60% of instructions (344,233,166 → 307,729,190), and
`inline_hot_bonus=64` erases all of it (344,232,982): the
self-invalidation is real. But an oracle that keeps the weights
through the knob's inline recovers exactly the knob-off count
(307,728,998), so fixing the keying buys nothing on this bench. The
kernel's wall time moves 53% with its source path's length alone
(identical IR, identical profile, #395), which is why that number was
not used.

### The spec debts (s163 — wolf-lang#364/#383, #365, #371, #319, #403, and the spec-debts stub: #181, #320, #256, #28, #302, #216, #228, #155, #309, #185)

Clauses two implementations already agreed on and no text stated, and
the few places the text said something false. No clause here moves a
behaviour it does not name; the two that do are said below, and every
amended clause states its runtime cost in the clause.

    let g: fn(proc) = 0          // E0206 at `proc` — [gram.type.start]
    sum_areas(rect, square)      // E0401 at `square` — [type.generic.bind]
    let a, b = 1, 2              // E0201 at the comma after `a` — [gram.item.let]
    //! check: run(exit=nonzero) // the class, not the number — [conf.directive.check]

**The stale code.** `[type.err.alias.cycle]` said E0515; the registry,
sema, the catalog and the witness all say E0610, and the spec was the
only outlier (#364, #383 — one bug filed twice). A sweep of all 98
codes the spec names found no second wrong number; it found
`[type.row.operand]`'s s148 status note stale (both machines answer
E0409 on either side now) and three registry explanations silent on
the alias case their code covers (E0503, E0507, E0601), all corrected.
`[type.range.accessor]` and the 0.2.14 entry below said the
interpreter had always normalized inclusive ranges; it had not
(#383's second half), and both now say so. `[type.numlit.value]` said
`let n = 0` fixes `n` as `int` while citing the rule that makes it
`i32` (#403, filed by s165): the binding is fixed where it is written,
at `i32` when nothing in the body decides it and at whatever a later
use pins — `let big = 5000000000` alone is E0415 on both tiers and
`trap(overflow)` on lupin; `take_int(big)` after it runs.

**New clauses.** `[type.generic.bind]`: a type parameter binds once
per call, left to right, E0401 at the disagreeing argument (#319).
`[os.fs.path]` and `[os.fs.remove]` (#365): a path is handed to the
host verbatim and resolves against the working directory, confinement
is never a row, and `fs_remove` on a directory is refused with the
host's row — `io` on linux, `denied` on macOS and windows, pinned per
host by a `wolf_rt` crate test. The path DOMAIN is where the machines
disagree (wolf reaches every path; lupin 0.1.36 admits only relative
paths with no `..`), so it is filed as #386 rather than chosen.
`[os.proc.spawn]` writes s111's posture down (#256): stdin null-wired,
stdout and stderr inherited — not three inherited streams, as the stub
had it, and not three null ones, as `std.process` says — and
`[os.proc.exe]` says an interpreter's `os_exe` is the interpreter.
`[gram.type.start]` names E0206 (#320). `[type.interp.spec]` is #28's
§7.4 at last, one clause: s38's executable semantics made normative,
18 specs byte-identical on `--checked`, `--native` and lupin 0.1.36,
`Show` recorded as superseded. `[os.host.sigs]` is the prelude annex
(#181): all 75 host builtins with their rows, rendered from the
checker's own table (`host_builtin_sig`, lifted out of
`call_host_stub` with no signature changed) and pinned to it both ways
by a test seen red on a planted retag.

**Ruled and written.** `[conc.chan.default]` is rendezvous by
specification (B24, #155), with a witness whose line order is causal.
`[proto.cmp.phase]` compares a trap's stdout digest when both records
carry it (B25, #216) — and `xtask differ` does, which added no row.
`[gram.item.let]`'s bare-tuple refusal points at the comma after the
first valueless binding (B25, #228): **wolfc moves**, from the
initializer list's end to lupin's locus, closing the one Diag row this
subject had. `[conc.task.order]` carries the ~196 `futex`/s a parked
wait costs the host (#302 item 2). CONTRIBUTING says a subject-line
count is advisory (#309); `chars_walk.lu`'s header is two-machine
evidence (#185).

**The corpus runner grows `run(exit=nonzero)`** (#371): any status but
0, never a trap, and never a package that did not compile — the guard
is scoped to the new word, because two `faults/` witnesses pin a
trap's dynamic meaning under a static refusal on purpose. The grammar
grew rather than `[conc.proc.root]` naming a number, since a number
would move one machine to fix a spelling. `conc/proc_link_root_death.lu`
is the witness: 121 here, 1 on lupin.

**The pairing, measured** (`xtask differ` against lupin 0.1.36, base
trunk `4b56441` vs this branch, debug builds; re-measured at each
rebase — at `8e73ac4` the base reproduced r19's ledger exactly, and the
deltas below were the same at all three bases). Plain comparison,
native: **107 -> 108** divergences on 633 -> 639 files. One closes:
`grammar/let_group_bare_tuple.lu`'s Diag row (`[374,375]` vs
`[364,365]`). Two open, both named in their clauses and both the
interpreter's to mirror or the number's to stay:
`typecheck/generic_bind_scalar.lu` (`fail(E0401)` vs `exit(0)`, lupin
checks the binding for struct arguments only — wolf-interp#103 row 2)
and `conc/proc_link_root_death.lu` (`exit(121)` vs `exit(1)`, the
status `[conc.proc.root]` leaves to each implementation). Triage,
native 369/161/0/79/22 -> 371/164/0/79/23 and checked
338/161/0/110/22 -> 339/164/0/112/22 (agreement / completeness /
soundness / unsupported / hard); predicted +5/0 and +4/0 on the first
two columns, wrong by three each way because triage files every wolf
`fail(..)` as completeness whatever the oracle answers, so the three
static agreements (`generic_bind_once`, `type_position_keyword`, and
the let-group row) land there. Unsupported and hard were exact.

## 0.2.14 — 2026-09-13

THE STRING RUNTIME AND THE PAPERCUTS IV. 0.2.14 is one subject read
from five directions — where a built `str`'s bytes live, and who is
allowed to outlive them — beside five rows where the compiler was
right about the program and wrong about the reader. It is also the
cut where the pairing catches up with the interpreter: **lupin
0.1.36**, the filesystem tier, was tagged while the stamp still said
0.1.35 (wolf-lang#372), and this cut names it. Every number below was
predicted before it was measured, and where the prediction was wrong
the entry says which way.

    fn worker(n: int, out: channel[str]) -> !int {
        out.send("built {n} here")?    // E1010: proc:worker owns these bytes
    }
    region scratch { let s = "re".repeat(2); s }   // E1010, the repeat is a site
    net_writev_head(sock, head, parts)?            // the head stays a str
    if c then a else b                             // `then` is a keyword to the editor
    fn(c: int) -> int { c }                        // one report, naming the rule

**What a reader gets.** A `str` is a citizen of the region it is built
in. Until this cut `region scratch { }` reclaimed a list's storage and
not one byte of the string work beside it (#191, owed since s40), so
string building was the one allocation `[mem.region.cap]` could not
budget — and, less visibly, the leak that made two unsound programs
look fine. Now a `str` built by interpolation charges its region
(**481.8 MB → 2.5 MB** max RSS over 20,000 iterations, the same loop
without the region block unchanged as the control), and two rules
arrive with it because freeing the bytes is what makes them
load-bearing: a spawned proc owns its ambient region
(`[mem.region.proc]`, #355 — build the bytes in the spawner and hand
them in as a parameter, which is the fix the diagnostic prescribes),
and `upper`, `lower`, `repeat`, `replace`, `str_from_utf8` and a `str`
read through a field are escape sites like any other
(`[mem.region.escape]`, #321) — E1010 on both tiers, zero rows moved
in `corpus/` and across wolf-std's 50 modules, one true positive in
lobo. `net_writev_head` hands `writev` a `str` head without
materializing a `List[byte]` for it (`[os.net.writev.head]`, #299 —
288 bytes and one list per response on lobo's head shape);
`RT_SYMBOLS` 143 → 144. Beside that, five papercuts: an editor
colours `then` and `error` (#356 — one keyword addition had opened
three holes across two subsystems, and `is_keyword` is a half-open
range now with its boundary pinned); parser recovery stops at a
contextual `error` declaration, which is why the nightly's own
`MUTATE_BUDGET=300` was RED on trunk v0.2.13 and is green here; a
type declaration with no `=` reports once, not three times (#360,
cascade 4 → 2 against a bound that does not move); a function value's
stray `-> int` gets the rule and three spellings that work instead of
a caret on the arrow (#157's ch04 row); and the plain move-out of a
`read` parameter was measured before it was ruled — it is **not**
`take` by a quieter name, it is an undeclared alias, filed as #366
(#359's premise was wrong). #298 stays open, and the entry says why.

The pairing takes **lupin 0.1.36** (`6e94436`, pin `a7f517e` — the
v0.2.12 tag), the release that gives the reference interpreter a
filesystem. `crates/wolf_driver/PAIRING` said 0.1.35 while 0.1.36 was
tagged — wolf-lang#372, filed by s160 when the pairing test went red
against a real sibling — and this stamp closes it. After this cut the
interpreter still declares v0.2.12 and the compiler names 0.1.36:
**the gap is two releases, on the compiler's side** — v0.2.13 and
v0.2.14 both go unanswered — and it is the first time the compiler has
been two ahead at tags. That is the fs tier's doing, not a lapse:
0.1.36 spent its release on an implementation the interpreter had
declined since s38, and took no new pin to do it. The bump is
**nine** ledger counts per tier — the nine files the fs tier unblocks,
each leaving `unsupported` for an agreement, predicted exactly — and
the fifteen Verdict rows s158's surface opened stay open, because the
pin has not moved.

### The pairing takes lupin 0.1.36 (#87 ritual, #281 control; closes wolf-lang#372)

wolf is now differentially tested against **lupin 0.1.36**
(`6e94436`), which declares `a7f517e` — the v0.2.12 tag — as its
conformance pin, the same pin 0.1.35 declared. PAIRING named 0.1.35,
so this stamp moves 0.1.35 -> 0.1.36, one release, and closes #372,
which s160 filed when the sibling-pairing test went red against the
real 0.1.36 archive. Both darwin archives were verified by sha256
against their release asset digests before use (0.1.36 `41f21002…`,
0.1.35 `0ea91a61…`), and the sibling-pairing test ran with its third
layer comparing rather than skipping: `LUPIN=` the 0.1.36 binary with
`WOLF_PAIRING_REQUIRE_SIBLING=1` is 10 passed, and the same command
at the 0.1.35 binary is 1 failed with the mismatch named.

**Predicted before either archive was fetched** (scratch
`r19-prediction-0136.txt`), three calls: that the bump would move
exactly the nine files is48 named, every one `unsupported ->
agreement`, on both tiers; that the fifteen Verdict rows would not
close; and that nothing would move below the ledger. Over the full
615-file corpus (580 entries), both tiers, against
`target/release/wolf` built by `cargo xtask dist` at `df00d26`, with
the control against the 0.1.35 release archive on the same tree:

                    checked                     native
    agreements      323 -> 332  (+9)            353 -> 362  (+9)
    completeness    151 -> 151   (0)            151 -> 151   (0)
    soundness         0 ->   0   (0)              0 ->   0   (0)
    unsupported     117 -> 108  (-9)             87 ->  78  (-9)
    hard             23 ->  23   (0)             23 ->  23   (0)
    coverage A      340 -> 340   (0)            372 -> 372   (0)
    coverage B      426 -> 435  (+9)            426 -> 435  (+9)
    coverage BOTH   306 -> 315  (+9)            336 -> 345  (+9)

    corpus/fs/bytes_dirs.lu                  unsupported -> agreement
    corpus/fs/error_row.lu                   unsupported -> agreement
    corpus/fs/fstat.lu                       unsupported -> agreement
    corpus/fs/open_nonblock.lu               unsupported -> agreement
    corpus/fs/roundtrip.lu                   unsupported -> agreement
    corpus/memory/byte_producers_ledger.lu   unsupported -> agreement
    corpus/net/unix_echo.lu                  unsupported -> agreement
    corpus/projects/count.lu                 unsupported -> agreement
    corpus/projects/count_dir.lu             unsupported -> agreement

    below the ledger — none.

**The bump was predicted exactly.** `THE INTERPRETER BUMP MOVED 9
LEDGER COUNT(S)` on each tier, the nine files named above and no
other, each leaving the unsupported tally for an agreement, coverage B
and BOTH up by nine, zero files below the ledger. The interpreter's
own census had said which nine (is48: out of scope 62 -> 53, "for the
next compiler release lane to predict against") and the compiler
already ran all nine on both tiers, so the open question was whether
lupin's live run under the differ would answer what its observed run
had. It did — `memory/byte_producers_ledger.lu`, the one that pins a
region-accounting relation rather than an fs one, and the
socket-binding `net/unix_echo.lu` included. That is the whole
interpreter delta of this release: the filesystem, and nothing but the
filesystem.

**The control arm, and where the prediction missed.** The control is
this tree against 0.1.35, which r18 had measured at v0.2.13 over 608
files; those 608 rows reproduce and s160's seven files are the whole
difference. Coverage was predicted exactly in all six cells (A
340/372, B 426, BOTH 306/336); the classes were not — predicted
325/149/0/116/24 checked and 355/149/0/86/24 native, measured
323/151/0/117/23 and 353/151/0/87/23. Three files, the same three on
both tiers. `memory/region_str_repeat_return.lu` and
`memory/region_str_from_utf8_return.lu` were called agreements —
lupin trapping `region-fault` where the compiler says E1010 — and are
completeness notes: lupin runs both clean, `exit(0)`, printing from
bytes the region no longer owns, which is the alternative the
prediction flagged and weighted at a third. `region_str_field_return.lu`
and `chan_payload_escape_proc.lu` trapped as called and are
agreements. And `net/writev_head_gather.lu` was called a Verdict row
— `net_writev_head` does not exist at pin `a7f517e` — and is
`unsupported` instead: lupin declines the name at resolve rather than
answering a code. So hard stays **23 = 8 Diag + 15 Verdict** on both
tiers, not 24.

**The fifteen Verdict rows do not close, and were not expected to.**
0.1.36 declares `a7f517e`, the pin 0.1.35 declared, and bs46 measured
list literals as E0201 on it before this lane ran; wolf-interp#106 is
the open mirror. The same fifteen — six `grammar/list_lit_*`, five
`grammar/range_type_*`, three `rows/error_alias_*` and
`grammar/match_nullary_variant.lu` — on both tiers; the eight Diag
rows are v0.2.12's eight, still warning parity. Zero SOUNDNESS on both
tiers, a third cut running. s160's own clauses have no lupin side at
this pin either, and the rows say which kind of none: `[mem.region.proc]`
is already an agreement by trap (lupin names `proc:worker` where the
compiler says E1010), `[mem.region.escape]`'s two new producers are
the two completeness notes above, and `[os.net.writev.head]` is the
unsupported. Filed as wolf-interp#111 so the next interpreter pin has
its rows.

### The papercuts IV (s159 — wolf-lang#356, #360, #359 ruled, #157's ch04 row; #366 filed)

THE PAPERCUTS IV — five rows, three of them filed by s157 against
itself, and one keyword addition that had opened three holes.

- **lsp** (#356): the contextual `then` and `error` reach the
  semantic-token stream. The walk's own keyword test carried a range
  `AsKw..=WhileKw` with `SelfKw` bolted past the end; `ThenKw` (s151)
  and `ErrorKw` (s158) were declared after it, matched neither that
  test nor the `Ident` test under it, and `classify_token` answered
  `None` — not a miscolouring, a hole, so an editor rendered `if c
  then a else b` with `if` and `else` coloured and `then` as plain
  text. `SyntaxKind::is_keyword` now owns the boundary, half-open at
  the first punctuation kind and pinned (`LParen - AsKw == 53`, 50
  reserved + 3 contextual), so a keyword declared in the block is a
  keyword with no second edit anywhere. Measured on
  `corpus/grammar/if_then_ident.lu`: 16 tokens → 17, the one added
  being `11:20 4 keyword "then"` — the diff wolf-lsp#11 asked for,
  against tl02's count of 16 at v0.2.12; the alias line of
  `rows/error_alias_row.lu`, 1 → 2. Both witnesses pin the WHOLE
  decoded stream, because every earlier assertion looked a token up by
  position and an absent token is a count nobody took.
- **parse** (#360): a type declaration with no `=` reports once. Past
  the missing `=` the item parser pressed on — `expected \`=\` in the
  type declaration`, then `expected a type`, then `expected a
  declaration here` — three reports on one token, the #243/#285 shape
  again. There is no alias body to parse without an `=`, so recovery
  consumes the wreck once. Measured with the run the issue asked for,
  `MUTATE_BUDGET=1000`: `comptime/assert_static.lu [swap tokens 6/7
  at 316..321]` goes from 4 cascade diagnostics against the tight
  bound of 3 to **2**, and the bound itself does not move. The witness
  pins the diagnostic CODE LIST for the reduced shape and that the
  declaration after the wreck still parses.
- **parse** (found measuring #360): recovery stops at a contextual
  `error` declaration. `error` lexes as an `Ident` and both of the
  parser's "does a declaration start here?" tests matched
  `TokenKind::Kw` only, so deleting the `}` of `error IoErrors =
  {none, parse}` made the row swallow the whole `error ConfigErrors =
  …` after it, and a `=>` in its place ran the sibling-floor skip with
  no stop at all. `at_decl_keyword` now answers through
  `at_error_item`, the parser's own reclassification test, so recovery
  and the item parser agree on what starts a declaration. This is
  #356's shape in a second subsystem — s158 added one keyword and
  opened three holes — and it is why the nightly's own
  `MUTATE_BUDGET=300` was RED on trunk v0.2.13 (reproduced with the
  grammar reverted, so not a regression of the row above); it exits 0
  now, 6 passed. #360's premise that 300 was clean was wrong.
- **mem** (#359, ruled; #366 filed): the plain move-out of a `read`
  parameter is not `take`. #359 asked for `fn f(b: List[int]) ->
  List[int] { b }` to be ruled as s157 ruled `take`, on the premise
  that it hands the caller's value away by a quieter spelling.
  Measured at v0.2.13, it does not: `let ys = f(xs)` prints `3 3` —
  the caller keeps its binding and its value — and what it got is a
  SECOND live path, `(mut ys).push(99)` then reads `ys=4 xs=4`. That
  is an undeclared alias, two writable paths with no `shared` spelling
  anywhere, and the clause it breaks is `[mem.tier0.excl.1]`, not
  `[mem.tier0.mode.read]`; folding it into E1014 would have named the
  wrong rule. The wide reading — refuse every move out of a `read`
  parameter at `emit_move` — was installed and measured before ruling:
  ONE distinct site in the corpus (`match self` in
  `typecheck/variant_value/paint/p.lu`, a false positive), 35 reports
  in wolf-std of which 23 are `self` and the rest local rebinds, and
  the three moves that actually leave an activation all in wolf-std
  (`std/cmp/cmp.lu:54`, `std/list/list.lu:68`,
  `std/option/option.lu:77`). Wrong on precision, not on cost.
  `[mem.tier0.mode.read]` says so now, and the aliasing defect is
  #366 — a cross-repo language change, not a papercut.
- **parse** (#157's ch04 row): a function value takes no return type.
  `fn(c: int) -> int { … }` reported `expected the closure body` with
  the caret on the `->` — true about where the parse stopped and
  useless to the reader, who wrote it because the ITEM spelling right
  above declares one. Now one report spanning the whole stray `-> int`,
  saying the rule — a function value's body type IS its type — and
  naming the three spellings that work, all of which the witness parses
  clean; the annotation is consumed into an `ErrorNode` so the body
  still parses and nothing cascades. `cargo xtask diag-catalog`
  regenerates with no diff, and 4456 reader-facing strings carry no
  internal id.

### The string runtime (s160 — wolf-lang#355, #321, #191, #299; #298 stays open)

Five issues that were one subject read from five directions: where a
built `str`'s bytes live, and who is allowed to outlive them. The order
mattered more than the fixes — **#355 and #321 had to land before
#191**, because #191 is what turns the escapes they refuse from leaks
into use-after-frees.

    fn worker(n: int, out: channel[str]) -> !int {
        out.send("built {n} here")?    // now E1010: proc:worker dies first
    }
    region scratch { let s = "re".repeat(2); s }   // now E1010
    region scratch { let d = Doc { .. }; d.title } // now E1010
    net_writev_head(sock, head, parts)?            // the head stays a str

**A proc owns its ambient region** `[mem.region.proc]` (#355). A
`spawn proc` entry has no caller whose region it can inherit:
`[conc.proc.1]` makes the proc its own failure domain and
`[conc.proc.kill]` step 3 bulk-frees its regions at exit. The checker
had been giving proc bodies `[mem.region.create.3]`'s plain-function
default, so a `str` a proc built and sent was accepted on both tiers
while lupin 0.1.34 trapped `region-fault` on it — a two-machine
disagreement the book caught at bs44, with the compiler on the unsound
side. A proc's PARAMETERS are untouched, which is why building the
bytes in the spawner and handing them in is the fix the diagnostic
prescribes. Cost: zero, both tiers, both directions — a static refusal,
and the repair copies the same bytes once either way.

**Two more escapes** `[mem.region.escape]` (#321). The site list now
includes every `str`-producing builtin that MATERIALIZES — `upper`,
`lower`, `repeat`, `replace`, and the free producer `str_from_utf8`
whose error row hid it — and a `str` read carries its place's sites
through a **projection**, not only out of a whole local. `str` is
`Copy`, which is why neither was ever seen: a `Copy` result never made
a call-result site, and a `Copy` field read flowed none. The second
half is conservative rather than exact (no per-field attribution, so a
literal-initialized field is refused too — #375); the sweep it was
gated on moved **0** rows in `corpus/`, **0** across wolf-std's 50
module entries, and **1** in lobo, a true positive filed as lobo#21.
`[mem.str.view]`'s other side is untouched and still allocates nothing.

**A `str` joins its region** (#191). The c09 seam `wolf_rt/src/str.rs`
had owed since s40, on a premise that stopped being true at s76:
containers learned to charge named regions and strings did not, so
`region scratch { }` reclaimed a list's storage and not one byte of the
string work beside it. Every materializing shim funnels through one
function, so one function moved — `write_owned` now dispatches through
`list::alloc_in`, exactly as `List` has since #81 — and a `List[str]`'s
element bytes charge the list's own region instead of splitting header
from contents across two arenas. **Measured** (macOS/arm64, debug
tier, standalone executables): 20,000 iterations building a ~1.2 KB
`str` by interpolation inside `region pass { }`, **481.8 MB → 2.5 MB**
max RSS, with the same loop without the region block unchanged at
481.6 MB both ways as the control. Predicted ">10x"; measured 192x.
Cost elsewhere: zero on the checked tier, and on the native tier a
materialization inside a named region gets *cheaper* by one mutex
acquisition. Two intended consequences: `[mem.region.cap]` budgets now
bind string building, and bytes that used to leak are genuinely freed —
which is what makes the two rules above load-bearing rather than
tidy.

**The head stays a `str`** `[os.net.writev.head]` (#299). A response
head is a `str`, and `[mem.str.view.lend]` materializes a `List[byte]`
for it in a `let` or a push — so entering the one-syscall gather cost a
full copy of the head, per response, for bytes already sitting in the
arena. `net_writev_head(fd, head: str, parts: List[List[byte]])` hands
`writev` the two words a `str` already is. One more `iovec` entry, no
extra syscall, no change to the drain or the rows; measured saving on
lobo's head shape **288 bytes and one list per response**.
`RT_SYMBOLS` 143 → 144.

**#298 stays open, and says so.** Its three asks — the D24 in-place
builder, `lower()`/`upper()` as views, `net_read` into the arena — are
all untouched; the contract named it the measurement, not a change.
Worse for the issue, #191 invalidated its instrument:
`tools/lobo-strings` reads RSS growth ÷ requests and its header states
the premise, "the ambient arena, which never frees (#191)". That
identity is gone, so the tool now measures retention rather than
materialization and its number is no longer comparable to 6,373. What
IS newly measurable, because #191 landed: lobo's nine-interpolation
response head charges **1,664 arena bytes for a 231-byte head — 7.2x
its own length**, undiminished, because the builder ask is untouched.

## 0.2.13 — 2026-09-12

THE LITERALS AND THE PAPERCUTS III. 0.2.13 is three surface additions
the maintainer ruled on in one day and ten filed defects, and the
pairing moves with it to **lupin 0.1.35**, cut the same night. Every
number below was predicted before it was measured, and where the
prediction was wrong the entry says which way.

    let cents = [340, 275, 100]        // a list literal
    fn width(r: range[int]) -> int { r.end - r.start }
    error IoErrors = {none, parse, io} // a transparent name for a row
    let call1 = fn(f) f(1)             // the call records its surface
    s.get(0..^1)                       // end-relative, and it resolves

**What a reader gets.** Three things that could not be spelled at all
now can. `[1, 2, 3]` is a list literal and **position** settles the
clash with indexing, so `[10, 20, 30][1]` needs no lookahead; a bare
`let xs = []` is E0419 and the message spells the annotation.
`range[int]` and `range[char]` are types in every position, with
`start` and `end` as properties and `end` **always exclusive**, which
is what makes `r.end - r.start` the count. `error IoErrors = {none,
parse, io}` names a row and is the same TYPE as its expansion both
ways, with no conversion and no coarsening; a cycle is E0610, and
`error` stays contextual, so `let error = 1` and a field named `error`
still parse. Beside them, ten places where the compiler was right
about the program and wrong about the reader: a call through an
unannotated closure parameter records its surface (the book's exercise
4-1 `compose`); `s.get(0..^1)` resolves; an `int` element read after an
index write is no longer reported moved (exercises 16-7 and 16-8 were
this, not a missing `copy`); `w.link()` compiles; `take` on a `read`
parameter is E1014; a spilled body containing a nested `fn` is ONE
E0203; **a declaration wins its own name** at a call site as
everywhere else, so a shadowing `read_line()` stops being typed
against the ambient signature and W0304 stopped lying; E0102 names the
`${` pair and `${{` is the escape; a bare dotted path is a pattern;
and no user-facing string carries a sprint, campaign or X id any more
— 70 strings across 92 sites, with `cargo xtask print-gate` holding
the line. No runtime symbol moved (`RT_SYMBOLS` 143).

The pairing takes **lupin 0.1.35** (`4f6cb2e`, pin `a7f517e` — the
v0.2.12 tag). After this cut the interpreter declares v0.2.12 and the
compiler names 0.1.35: **the gap is one release, on the compiler's
side only**, both halves named at tags for the second cut running. The
bump is small and exactly measured — **one** ledger count per tier —
and the fifteen Verdict rows s158's new surface opened stay open,
because 0.1.35 mirrors v0.2.12 and has not met those clauses.

### The pairing takes lupin 0.1.35 (#87 ritual, #281 control)

wolf is now differentially tested against **lupin 0.1.35**
(`4f6cb2e`), which declares `a7f517e` — the v0.2.12 tag — as its
conformance pin. PAIRING named 0.1.34 (pin `c9237c1`, the v0.2.11
tag), so this stamp moves 0.1.34 -> 0.1.35, one release. The darwin
release archive was verified by sha256 against the release asset
digest before it was used.

**Predicted before the archive was even fetched** (scratch
`r18-prediction-0135.txt`), three calls: that the control arm would
reproduce this tree's 0.1.34 ledger exactly; that the fifteen Verdict
rows would **not** close; and that the bump would move nought to three
counts, most likely one or two, with `interp_spec_on_union.lu` named
as the likely mover. Over the full 608-file corpus (573 entries), both
tiers, against `target/release/wolf` built by `cargo xtask dist` at
`219a483`, with the control against the 0.1.34 release archive on the
same tree:

                    checked                     native
    agreements      320 -> 320   (0)            349 -> 349   (0)
    completeness    149 -> 149   (0)            149 -> 149   (0)
    soundness         0 ->   0   (0)              0 ->   0   (0)
    unsupported     116 -> 115  (-1)             87 ->  86  (-1)
    hard             23 ->  23   (0)             23 ->  23   (0)
    coverage A      338 -> 338   (0)            369 -> 369   (0)
    coverage B      420 -> 420   (0)            420 -> 420   (0)
    coverage BOTH   305 -> 305   (0)            334 -> 334   (0)

    corpus/typecheck/interp_spec_on_union.lu   unsupported+Completeness -> Completeness   (#323)

    below the ledger — none.

**The control reproduced the earlier ledger byte for byte**, coverage
included. That matters more than it looks: this lane had already
measured the same corpus against 0.1.34 on the same tree before 0.1.35
existed, so the control arm had an independently recorded baseline to
agree with instead of being taken on faith, and it agreed in all
sixteen cells. wolf-interp's `FILED_DIVERGENCES` going 4 -> 2 in this
very release could not reach that arm, and the entry says why rather
than hoping: the list lives in lupin's own differential harness, where
it compares lupin against vendored counterparty records, and is never
consulted when lupin EXECUTES a `.lu` file — which is all the
wolf-side differ asks of it. Independently, neither retiring file has
a row in either ledger: `wordcount.lu` is `unsupported` on the
compiler at resolve and `grammar/structlit_paren.lu` is an agreement,
so no behaviour change there could have moved a count here.

**The one count.** `typecheck/interp_spec_on_union.lu` leaves the
unsupported tally on both tiers: 0.1.35 answers `fail(E0413)` at a
format spec on a `!T` hole (wolf-lang#323's mirror) where 0.1.34
declined, so the file is now a plain completeness note with both sides
naming the same code. That is the whole interpreter delta of this
release.

**The fifteen Verdict rows do not close, and that was predictable from
0.1.35's own pin.** is47 predicted they would close on the bump; they
did not, and nothing measured here is a surprise once the pin is read.
0.1.35 declares `a7f517e`, which **predates** s157 and s158, and the
whole of 0.1.34..0.1.35 is `then` joining the contextual table (#100),
a free-name walk (#45), the declared-scalar check learning `char`
(#61), two refusal texts (#158), #323's format spec, #325's
bare-receiver write lint, two filed-divergence retirements and a
re-vendor. Nothing in it implements list literals, `range[T]` as a
type, or `error` set aliases. So all fifteen stand, the same fifteen
on both tiers, and they close at the interpreter release that takes a
pin at or past this one.

**The miss.** Three to eight files were predicted to move BELOW the
ledger; **none** did. #325's bare-receiver lint changes what lupin
DIAGNOSES on `typecheck/receiver_bare_mut.lu` and
`receiver_bare_mut_param.lu` without changing the verdict the differ
reads, and a lint that does not alter a verdict is invisible to a
harness that compares verdicts. The prediction confused a diagnostic
change for an observable one.

**What stands at 0.1.35.** `checked 23 = 8 Diag + 15 Verdict`,
`native 23 = 8 Diag + 15 Verdict`. The eight Diag rows are v0.2.12's
eight, unchanged and still warning parity (lupin emits no warnings):
`binder_capitalized`, `discarded_result`, `else_arithmetic` (W0318),
`float_zero_minus`, `region_never_allocates`, `byte_view_escape` and
`unit_tail_value_discard`, plus `safety_comment_missing` on checked
and `unit_context_discard` on native. Zero SOUNDNESS on both tiers, a
second cut running. The fifteen Verdict rows are the six
`grammar/list_lit_*`, the five `grammar/range_type_*`, the three
`rows/error_alias_*` and `grammar/match_nullary_variant.lu` — fourteen
of s158's clauses and one of s157's `[gram.pat.nullary]` row.
`range_type_overflow.lu` is the one with two stories: wolf answers
`trap(overflow)` where `0..=int.MAX` normalizes past the end, which is
s158's own defect fix, and lupin never reaches the question because
the type name does not resolve.

**Two rows that are nobody's defect.** `rows/error_alias_ident.lu` is
an agreement: s158 predicted it would refuse and it does not, because
`error` is contextual, so lupin parses the program and prints `3\n4\n`
— re-measured here rather than carried from the sprint that guessed
wrong. And DIV-2026-019, `resolve/broken_sibling/entry.lu`, stays a
completeness note where the corpus pins `fail(E0202)` and lupin stops
at the first bad token with `fail(E0201)`: the spec assigns neither
code to junk recovery, so it is a hole in the spec rather than a
divergence either machine owns, and it is not written up as one.

Read against v0.2.12's ledger rather than against the control, this
cut reads `315 -> 320` agreements, `143 -> 149` completeness, `114 ->
115` unsupported and `8 -> 23` hard on the checked tier. Every one of
those moves is the COMPILER's except the single interpreter count
named above — which is precisely what the flat control licenses anyone
to say. The CI sibling step needed zero edits, the seventh re-stamp in
a row.

### The papercuts III (s157 — #311, #164, #313, #153, #60, #285, #44, #134, #162, #39, #157)

THE PAPERCUTS III — the language's ten, each a witness plus a fix.

- **sema** (#311): a call through an unannotated closure parameter
  records its call surface. `let call1 = fn(f) f(1)` types `f`
  against an inference variable; the call shapes and SOLVES that
  variable, and the site now joins the resolved surface the way the
  annotated spelling always did, so lowering stops refusing the
  program by the generic name. Every recorded call zonks at the
  body's finish, so a surface recorded against variables lands
  solved. Witness `corpus/typecheck/closure_param_call.lu` — the
  book's exercise 4-1 `compose` included.
- **strings** (#164, wolf-std F-0096): `s.get(0..^1)` resolves.
  `[mem.str.get]` says end-relative endpoints and open ends "resolve
  exactly as in `s[a..b]` before the domain question is asked"; until
  now `get`'s range was typed by the general range-value rule, which
  refuses both forms, so the recoverable spelling was `unsupported`
  at resolve on both compiler rungs while the slice ran three-lane.
  `get`'s range is now a slice-position range on both tiers, and the
  checked machine resolves its endpoints against the receiver's
  length before asking the domain question. Witness
  `corpus/strings/end_relative_get.lu`.
- **mem** (#313): an `int` element of a `List[int]` read after an
  index WRITE to the same list is no longer reported moved. Every
  container now reads its element type off the container, the way
  `Map` already did; the old lookup went through the access's own
  span, and an access in target position is a place with no recorded
  type, so one write poisoned the memoized element place and every
  later element read in the function became a move. The book's
  exercises 16-7 and 16-8 were this, not a missing `copy`. Witness
  `corpus/memory/list_elem_copy_loop.lu`; the move direction is
  untouched.
- **conc** (#153's compiler row): `w.link()` compiles. The one-arg
  spelling is `[conc.proc.link.pair]`'s own words — "`w.link()` is
  `w.link(<the calling task's proc>)`" — and the runtime has always
  taken it (partner 0, resolved to the current proc or the root
  domain); only sema refused it, `E0402`. Witness
  `corpus/conc/proc_link_root.lu`.
- **mem** (#60, ruled here): `take` spelled on a `read` parameter at
  an inner call site is E1014, the code that already refuses writes
  through one. `[mem.tier0.mode.read]` gains the sentence that makes
  "the caller retains it" a rule in both directions — move-out is the
  same immutability question as mutation and the worse answer, since
  the value the caller kept would be gone. Witness
  `corpus/memory/read_param_take.lu`, beside the write file it pairs
  with. A plain move out of a `read` parameter (`let n = b`, `return
  b`) is a wider ruling and is NOT taken here.
- **parse** (#285): a spilled function body that contains a nested
  `fn` is ONE E0203 stray-line report, not two. An unambiguous item
  keyword ended the fold on token identity alone; it now ends it only
  when its COLUMN says it is a sibling — indentation already says
  which it is, and it is the floor `item()` gives recovery. The
  nightly's budget-300 blast-radius case
  (`closure_return.lu [replace \`{\` at 1183 with \`fn\`]`, six cascade
  against the bound of five) is five, and the bound does not move.
- **resolve** (#44, wolf-std F-0047): **a declaration wins its own
  name**, at a call site as everywhere else. `spec/05` §6's new
  `[conf.resolve.ambient]` writes the order down — lexical binding,
  file imports, module items, then the ambient prelude, with
  `[conf.trap.assert]`'s intrinsics the one carve-out. The compiler
  had been answering two different questions about one name: every
  non-call reference took the declaration (which is what W0304 warns
  about), while the call path re-derived dispatch from the spelled
  name and typed a shadowing `read_line()` against the AMBIENT
  signature, on both tiers. W0304 already names the shadow. Witness
  `corpus/lints/shadow_prelude_call.lu`, three lanes.
- **lex/spec** (#134): `$` escapes nothing, and now something says
  so. `[gram.lex.str.escape]` gains the sentence — a literal `${` is
  spelled `${{`, so `"$host${{uri}}"` is the text `$host${uri}` — and
  E0102 adds a second note naming the pair whenever the unclosed
  string carries `${`, because "for a literal brace, write `{{`" sends
  a reader hunting a delimiter bug. `--explain E0102` says it too.
  Witnesses `corpus/grammar/str_dollar_brace.lu` (the refusal) and
  `corpus/strings/dollar_brace_escape.lu` (the spelling that works).
- **grammar** (#162's last row): a bare dotted path is a pattern —
  `[gram.pat.nullary]`. `Color.Green => …` and a payload-less row tag
  `none => …` parse; the parser required the parens, so a `match` over
  a closed set of names could not be spelled at all and exhaustiveness
  had nothing to be exhaustive over. Arity stays the checker's
  question (`Shape.Rgb =>` on a three-value variant is E0808; a
  missing arm is E0801 naming it). Witness
  `corpus/grammar/match_nullary_variant.lu`, both compiler tiers —
  lupin refuses it at the old rule and takes the clause as its mirror.
- **wording** (#39, #157): no sprint or campaign id in any
  user-facing string. Every conservatism refusal the ledger prints
  lost its `(c06)` / `(s39 std sync)` / `(c25 closeout)` /
  `deferred from s26` tail and kept the words that say what is
  refused — 70 distinct strings, 92 sites. `wolf add` and `wolf pkg`
  stopped calling a manifest "an s51 manifest", the publish log
  stopped citing `X7`, the LLVM banner stopped naming s41, and the
  two `str` hovers stopped citing s120/s136/D58. Witness: `wolf run`
  on a `Pool` program printed "Pool/shared constructor lowering
  (runtime shapes, c06)" and prints "(runtime shapes)".
- **wording** (#157's compiler rows): `--explain E1010` stops calling
  `freeze` and `shared` "coming in later tiers" — both ship; E0708's
  note says WHY an aggregate has no size yet instead of "not yet";
  E1101 stops suggesting `par`, which no implementation has; E1401's
  explain drops its internal decision id; `wolf add` no longer says
  the registry "arrives at c15 (X7)"; `wolf add` reports
  `capabilities: none` rather than `capabilities []`; and the
  one-operand `when` refusal points its caret at the OPERAND LIST it
  talks about instead of at the block's `{`. `wolf audit`'s capability
  findings are the verb's data and go to stdout with the tree, so
  piping keeps them — `add` and `update` keep theirs on stderr, where
  the rest of those verbs' progress lives. `cargo xtask print-gate`
  gained the ratchet: a reader-facing string in a phase, the driver,
  the package manager or the query layer that names a sprint,
  campaign or X id fails CI.

### The literals (s158 — wolf-lang#154, #24, #36)

Three surface additions the maintainer ruled on one day, landed
together because their clauses read as one section of spec/01.

    let cents = [340, 275, 100]        // a list literal
    fn width(r: range[int]) -> int { r.end - r.start }
    error IoErrors = {none, parse, io} // a transparent name for a row

**List literals** `[gram.expr.list]`. `[1, 2, 3]` builds a `List[T]`,
and **position** settles the clash with indexing: a `[` that begins a
primary opens a literal, a `[` that follows a complete expression is
the postfix form it always was, so `[10, 20, 30][1]` is a literal
indexed and no lookahead decides between them. No program that parsed
before changes shape — a leading `[` was E0201 everywhere it now opens
a literal. The element type unifies across the elements and the FIRST
element that does not fit is the error site; an expected type flows in
from an annotation, a parameter, a field or a declared return, so `[]`
is written bare in all of them. A bare `let xs = []` is **E0419**, and
the message spells the annotation for that binding. The value is a
fresh `List[T]` in the ambient region — the same one `List[T]()` plus
a push per element builds, which is also how it lowers, so no runtime
symbol moved (`RT_SYMBOLS` 143). `wolf fmt` breaks a literal exactly
like an argument list (`[gram.fmt.list]`).

**A nameable range** `[type.range]`. `range[int]` and `range[char]`
are types now, in every type position, with `start` and `end` as
properties. **`end` is exclusive, always**: `a..=b` normalizes at
construction to `b + 1`, which is what makes `r.end - r.start` the
count (corrected by s163, wolf-lang#383: this entry said the
reference interpreter had always done so; lupin 0.1.36 carried `..=`
as a flag to the loop, and agrees only since wolf-interp is49) — three of
`std.range`'s four functions were unwritable without it (wolf-std
F-0030). A range passes, returns, binds and iterates; `for` is
unchanged. The end-relative and open-sided spellings (`^n`, `a..`,
`..b`) stay subscript-position only, stated once here and cited by
`[mem.list.slice]`.

**Transparent error-set aliases** `[gram.item.error]`,
`[type.err.alias]`. `error IoErrors = {none, parse, io}` names a set
of tags, and `-> T ! IoErrors` is the same TYPE as
`-> T ! {none, parse, io}` — interchangeable both ways, no conversion,
no coarsening (D30: rows are structural, so a nominal error set would
make two rows the same two different ways). Aliases compose by naming
(`{IoErrors, closed}`), flattened by `[gram.type.row.flatten]`'s own
rule; a cycle is **E0610**, once, with the loop named. `error` is
contextual, not reserved — `let error = 1`, `error(reason)` and a
field named `error` all still parse, and `[gram.inv.kw]`'s fifty are
unchanged.

Eighteen witnesses, all eighteen byte-identical on both tiers. Nothing
in wolf-std, lobo or the book spells any of the three, and `wolf fmt
--check` over all three trees is unchanged file-for-file — except that
`wolf-std/std/range/range.lu`'s `pub fn is_empty(r: range[int])`, the
one function F-0030 said survived, now has a type that resolves.

## 0.2.12 — 2026-09-11

THE PAPERCUTS. 0.2.12 is twenty filed defects, nineteen of them
moved, and one sentence covers most of them: the compiler was right
about the program and wrong about the reader. Five came out of one
week's reading of chapter 5; the rest out of writing wolf-std, lobo
and the book against v0.2.11.

    use cmp
    if less == greater { return 1 }    // dispatches, one `use` away
    (4 as f64) == 4.0                  // true on every machine
    let r = x % 2.5                    // `fmod`, and it lowers
    n + 1                              // E0409 on a row, whichever side

**What a reader gets.** A row is E0409 on either side of an operator,
so `n + 1` and `1 + n` answer one code. A `!T` left at a unit tail is
a warning, not a return-type diagnosis. `let r = Row{…}` then
`r.cents = 5` is E0410 on both machines, where 0.2.11 ran it. A
body-less trait member parses on one line. `==` on a type a library
publishes works one `use` away — `[type.trait.op]` reads the trait
from the operand's own module and the file's imports, not the bare
name. The checked tier's casts convert (`(4 as f64) == 4.0` is true,
`1e300 as int` traps). `%` on a float is `fmod` and lowers on both
backends. A `[K, V]`-generic `set` compiles. `{m[k]:>5}` is E0413, a
typing refusal, not a silent `unsupported`. `wolf fmt` measures a
break against the tail of the line, breaks outermost first, keeps a
fallback's parens, puts one space after a closure's parameter list
and breaks a braced `if` chain as one — 33 std files and 28 lobo
files re-lay at their next pins. `str.find` is 1.6–2.0x faster on a
short needle, and no runtime symbol moved (`RT_SYMBOLS` 143).

The pairing is stamped at **lupin 0.1.34** (`b83049a`, pin `c9237c1`
— the v0.2.11 tag). The stamp moves two releases at once: 0.1.33
(is45, the one-line `if`, #85's two) and 0.1.34 (is46, the map
mirror, the operator bridge, the region `str`, four small mirrors).
After this cut the interpreter declares v0.2.11 and the compiler
names 0.1.34: **the gap is one release, on the compiler's side only,
and for the first time both halves are named at tags.** Every hard
divergence left is a warning-parity row — zero Verdict, zero
SOUNDNESS, on both tiers.

### The pairing takes lupin 0.1.34 (#87 ritual, #281 control)

wolf is now differentially tested against **lupin 0.1.34**
(`b83049a`), which declares `c9237c1` — the v0.2.11 tag — as its
conformance pin. PAIRING named 0.1.32 (pin `e0ce018`, a dev stamp
inside v0.2.10) because r16 cut before is45 tagged, so this stamp
moves 0.1.32 -> 0.1.34. The gap to this release is s154 and s156, the
compiler's side only, one release wide with both halves at tags —
which no earlier stamp could say.

**Predicted before the run: eleven counts per tier**, control 0.1.33
-> 0.1.34 (is46 alone), by name — s152's `Map` witnesses and s155's
positive operator witnesses leaving `unsupported`, `op_total_num`
leaving Verdict (the alias form parses), s153's two region rows
leaving Completeness (lupin traps `region-fault`, E1010's paired
kind), and three `fail`-pinned files leaving the unsupported count
because lupin refuses them by the compiler's code now. **Measured:
ten per tier, the same ten on both** — the eleventh,
`memory/map_std_keys.lu`, was already an agreement at 0.1.33. Over
the full 580-file corpus (545 entries), both tiers, against
`target/release/wolf` built by `cargo xtask dist` at `842a296`, with
the control against the 0.1.33 release archive on the same tree:

                    checked                     native
    agreements      308 -> 315  (+7)            335 -> 342  (+7)
    completeness    145 -> 143  (-2)            145 -> 143  (-2)
    soundness         0 ->   0   (0)              0 ->   0   (0)
    unsupported     121 -> 114  (-7)             94 ->  87  (-7)
    hard              9 ->   8  (-1)              9 ->   8  (-1)
    coverage A      318 -> 318   (0)            347 -> 347   (0)
    coverage B      414 -> 412  (-2)            414 -> 412  (-2)
    coverage BOTH   295 -> 300  (+5)            322 -> 327  (+5)

    corpus/memory/map_count.lu                 unsupported -> agreement   (wolf-interp#91)
    corpus/memory/map_int_keys.lu              unsupported -> agreement   (#91)
    corpus/traits/op_money.lu                  unsupported -> agreement   (#92)
    corpus/traits/op_ord_struct.lu             unsupported -> agreement   (#92)
    corpus/traits/op_total_num.lu              Verdict -> agreement       (#92, the alias form)
    corpus/memory/region_str_concat_return.lu  Completeness -> agreement  (#88, trap(region-fault))
    corpus/memory/region_str_concat_send.lu    Completeness -> agreement  (#88)
    corpus/traits/op_hetero_add.lu             unsupported+Completeness -> Completeness  (B fail(E0514))
    corpus/traits/op_missing_impl.lu           unsupported+Completeness -> Completeness  (B fail(E0502))
    corpus/typecheck/map_compound_absent.lu    unsupported+Completeness -> Completeness  (B fail(E0417))

    below the ledger — ten, the ten predicted by name:
    corpus/memory/map_absent_else.lu       B prints `none`, not `()`
    corpus/memory/map_char_bool_keys.lu    B's bytes changed
    corpus/traits/op_eq_inverting.lu       B consults the impl
    corpus/traits/op_eq_no_trait.lu        B exit(0) -> fail(E0301)
    corpus/traits/golden_arith.lu          B exit(0) -> fail(E0501)
    corpus/traits/golden_eq.lu             B exit(0) -> fail(E0501)
    corpus/traits/golden_missing_bound.lu  B exit(0) -> fail(E0501)
    corpus/traits/dyn_temp_refused.lu      B exit(0) -> fail(E0810)   (#98)
    corpus/typecheck/let_field_assign.lu   B exit(0) -> fail(E0410)   (#99, s154 from trunk)
    corpus/typecheck/map_struct_key.lu     B exit(1) -> fail(E0418)

The prediction's other miss is worth its sentence: coverage B was
predicted +5 and measured -2, because the seven refusals lupin now
issues by the compiler's code leave B's run rung while the five
witnesses it now runs enter it — a machine that grows stricter covers
less at run and agrees more. The whole stamp move, control 0.1.32 ->
0.1.34, measured twenty counts per tier (predicted twenty-one, the
same miss): the ten above plus is45's nine `grammar/if_then_*`
witnesses Verdict -> agreement and `typecheck/str_slice_assign.lu`
leaving the unsupported count; eleven below,
`rows/negative/row_operand_compare.lu` (E0401 -> E0409) joining the
ten.

**What stands at 0.1.34.** `checked 8 = 8 Diag`, `native 8 = 8 Diag`:
#167's warning asymmetry (`binder_capitalized`, `discarded_result`,
`float_zero_minus`, `region_never_allocates`, `byte_view_escape`, plus
`safety_comment_missing` on checked and `unit_context_discard` on
native) and s154's two new warning witnesses,
`lints/else_arithmetic.lu` (W0318) and
`typecheck/unit_tail_value_discard.lu` (W0601's widened tail) — lupin
emits no warnings. Zero Verdict rows, for the first time. Zero
SOUNDNESS on the checked tier, where 0.2.11 carried #168's two
float-cast twins: s156's #337 made the checked tier's casts trap, a
move on the compiler's side that the control cannot attribute and the
table shows as `0 -> 0`. In the completeness class the named rows now
answer the same code on both sides (`op_eq_no_trait`,
`op_missing_impl`, `op_hetero_add`, `golden_*`, `dyn_temp_refused`,
`map_compound_absent`, `map_struct_key`, `let_field_assign`,
`row_operand_compare`, `str_slice_assign`); a `fail`-pinned file never
counts. The one row that could have opened,
`traits/op_eq_imported/main.lu` (s156's #336, past the pin), stayed an
agreement: its operands are enum values and lupin keeps the structural
comparison there. #341's two seed files were fixed on trunk at
`0bb7024`, before the tag lupin pins next: `grammar/structlit_paren.lu`
stays an agreement here (on its old text 0.1.34 answers E0301 — a
Verdict row this cut would otherwise carry) and `wordcount.lu` is
`unsupported` on the compiler at resolve, invisible either way;
DIV-2026-022 and -023 close on lupin's side at its next pin, and
DIV-2026-019 stands. Two of is46's mirrors this ledger cannot see:
#94's E0408 and #95's E0301 at an unresolved bound name have sema
fixtures and no corpus file (#353). The CI sibling step needed zero
edits, the sixth re-stamp in a row.

### The papercuts (s154 — #297, #303, #314, #318, #325, #326, #329, #331, #332)

THE PAPERCUTS. Nine filed defects, each small, each one a place where
the compiler was right about the program and wrong about the reader.
Five came out of one week's reading of chapter 5.

**A row is E0409 on either side of an operator** — `[type.row.operand]`
already said so; the numeric and comparison paths fixed the operator
family from the LEFT operand and then reported a right-hand `!T` as a
mismatch against it, so `n + 1` and `1 + n` answered different codes
for one defect — a rule about position, which is the thing the clause
exists to refuse. All four families — arithmetic, comparison,
bitwise, `==`/`!=` — now report at the row, whichever side carries it,
with the clause's own reason as the second note. `==` on a row was a
conservatism refusal before this; it is a diagnostic now (#297).

**A `!T` tail in a unit context is a discard, warned** —
`[type.unit.discard]` widened (#326). `fn drop_last(mut xs: List[int])
{ (mut xs).pop() }` answered E0401 "this is `int ! {none}`, but
`drop_last` must return `()`" — a return-type diagnosis of a discard,
one line below a STATEMENT of the identical shape that has only ever
warned, and its wording sent the reader to manufacture a value nobody
wanted. s146 admitted `!()` only; the half that moves is the row,
which is the thing being discarded. The plain `int` tail keeps E0401.
W0601 names `let _ = …` at a value-carrying tail.

**W1002 stands down where a mode error contradicts it** (#325). The
lint's write scan is flat and syntactic; E0804 and E1014 are typed
refusals, and when both fired on one `mut` parameter they disagreed
about whether a line writes it — with the lint's machine-applicable
fix-it ("drop the `mut`") pointing away from the fix E0804 names, onto
E1014. The unspelled write is a write: a mode error on a use of a
parameter retires W1002 for that parameter, in the build, in the
conformance ladder, and in `wolf fix`.

**W0318 — a defaulting `else` that swallows the next term** (#329).
`x else 0 + y` is `x else (0 + y)` by `[gram.amb.else]`, both readings
type-check, and nothing said so; the maintainer's RPN first pass
defaulted every tail it wrote. The warning fires when the fallback's
leftmost term is a literal or a bare name and the term after the
operator is not a literal — `e else 0 - 1` folds to a constant and is
the `-1` sentinel this ecosystem already spells 177 times in wolf-std,
so it is left alone. W0307 is the same scar on a comparison.

**A `let` binding's fields are as immutable as the binding** (#331).
`let r = Row{…}` then `r.cents = 5` ran to exit 0 and printed on BOTH
machines — they agreed, so the differential could not see it — while
`n += 1` on a `let` was E0410 on both. That made `let` a rule about
rebinding while E0410's own note said "`let` names a value once".
`[gram.item.let]` rules the value: E0410 at the field write, with its
own message. Elements reached through an index are a different
question and stay where they were.

**A one-line body-less trait member parses** (#332). `trait Add { fn
add(self, other: Self) -> Self }` was E0201 "expected a function body"
with the caret on the trait's closing brace — a message naming a body
inside a trait whose whole point is the missing body, when what the
grammar wanted was a terminator. The multi-line form has always parsed
(the newline IS the TERM), so the refusal was a rule about layout. The
enclosing `}` closes a body-less signature now; nothing else may omit
the terminator.

**Two formatter papercuts.** A closure's expression body is separated
from its parameter list by one space whatever token it starts with:
`fn(c) (c + to / 2)`, never `fn(c)(c + to / 2)`, which parses as the
closure it is and reads as a call of `fn(c)` (#314). And **a braced
`if` chain breaks as one** (`[gram.fmt.if]`, #303): the inline
decision belongs to the chain, not to each arm. Per-arm it was taken
against the remaining width with no knowledge of the chain tail that
follows on the line, which laid one chain in two shapes and produced a
101-column line that `wolf fmt --check` accepted as a fixed point,
because a second pass reproduced it. Measured motion over wolf-std
`046bd4f`, lobo `d792290` and wolf-book `d636228`: 4 files of 1650,
all of them #303's mixed-shape chain, none of them #314's.

**`then` is in the contextual-keyword list** — `[gram.inv.ctx]` (§6.2)
enumerates them, `[gram.expr.if]` calls `then` contextual, and the two
clauses disagreed about what the list is; wolf-interp holds its table
to §6.2 in both directions by a test, so `then` could not join it
(#318).

### The papercuts II (s156 — #323, #327, #333, #334, #335, #336, #337, #339, #340, #341; #345 measured)

THE PAPERCUTS II. Eleven filings, ten of them moved. Several are the
same defect wearing different clothes: a rule that was right about the
program and could not be reached from where the reader stood — a trait
whose name the import qualified, a cast that skipped its target type,
a break measured against the wrong line.

**An operator's trait is the one REACHABLE, not the one bare** (#336).
`[type.trait.op]` resolved the operator trait by bare name, and wolf
binds whole modules — `use std.cmp` binds `cmp`, so the trait is
`cmp.Eq` and `Eq` is a name no importer can have. Importing a
library's type was therefore the very thing that made its operators
undispatchable: `==` on an `Ordering` worked inside `std/cmp/*.lu` and
was E0301 one `use` away, which is every user type any library
publishes, in the release whose headline is `==` on a library's type.
Neither half of the help could be followed — "bring `Eq` into scope"
named no spelling that reaches it, and "write `impl Eq for Ordering`"
asked for a duplicate of an impl std.cmp already ships. Three places
are consulted now, in order: the name in scope; the trait declared by
the operand type's own module, which coherence already implies and
which needs no import; then the modules this file imports. Two imports
declaring one operator trait is no answer and keeps the E0301.

**A numeric cast converts** (#337). The checked tier — the machine
`[exec.checked]` calls the one that "never guesses" — answered
`(4 as f64) == 4.0` FALSE while `a == a` was true, and the native rung
and lupin both said true. `eval_cast`'s catch-all handed the operand's
value straight back without consulting the target type, so `n as f64`
stayed an integer and `values_equal`'s variant pairs did the rest. The
float→int direction was the same hole and worse than the filing knew:
`3.7 as int` printed `3.7`, and `1e300 as int` and `nan as int` ran to
exit 0 where `[type.numlit.cast.trunc]` traps. Four corpus witnesses
answer correctly now that did not.

**`%` on a float is `fmod`, and it lowers** — `[type.float.rem]`
(#327). Ruled because it had to be ruled before it could be lowered:
the remainder of the truncated division, the sign of the dividend,
`x % 0.0` NaN, nothing traps. Three quarters of the ruling was already
in the tree and unwritten — sema typed it, the comptime folder folded
it as Rust's `f64::rem`, the checked machine refused it as "unruled" —
so a program the language accepted ran on one tier and was declined by
the other, and `[type.trait.op]`'s own `impl Rem for f64` could not be
written. `Opcode::Frem` through the whole set the ledger demands; LLVM
emits `frem`, and cranelift, which has no such instruction and whose
naive expansion is not `fmod`, calls the routine LLVM's frem lowers to.

**A trait alias's right-hand side marks its imports used** (#334). The
alias is usually the ONLY consumer of the imports it names —
`[type.trait.op.alias]`'s own worked example spans std.ops and std.cmp
— and `resolve_item` walked the members and never the alias bound, so
both imports came back E0305 with a fix-it that deleted the names three
lines below. A generic BOUND has always counted, through the very same
three lines.

**A store through a container index lends** — `[mem.region.edge.elem]`
(#333). `fn set_g[K, V](mut m: Map[K, V], k: K, v: V) { m[k] = v }` was
E1004 and `fn push_g[T](mut xs: List[T], v: T) { (mut xs).push(v) }`
was not: two operations that are one operation to a reader, needing
different source for the same generic signature, on the most ordinary
function a keyed container has. The asymmetry was never a decision —
`push`'s value parameter is a `read`, and a read is a lend. A field
store keeps its E1004, and so does a value provably outliving its
region.

**A format spec on a `!T` hole is E0413** — `[type.interp.union]`
(#323). `{m[k]:>5}` refused on both tiers and sema let it through, so a
typing question arrived as `unsupported` at run time and read as a
missing feature. The value is two values and a spec describes one:
handle the row, then format what is left.

**The break that achieves the width, outermost first** —
`[gram.fmt.break]` (#339). Every break was measured against the
construct alone, and the page is not written a construct at a time: a
parameter list ending at column 93 "fit", and the ` -> List[byte] {`
already committed to follow it ran the line to 103 — so the return
type, a two-token type application, was the only group left able to
break, and `] {` landed at the head of a line where it reads as a block
close. `fits` measures the tail of the line now. A type application is
not a break point; the receiver-dot break is a last resort, taken only
when breaking the argument lists cannot bring the line inside the
width, and then taken at every dot at once; an argument list broken
open indents one level past the line its callee name is on and closes
at that name's own column. A break that cannot achieve the width is not
taken at all. Measured re-lay against trunk: wolf-std `7582e7a` 33
files (245 orphaned receiver-dot lines to 0, breakable over-width code
lines 14 to 3); lobo `1148318` 28 files (71 to 0, 8 to 0); wolf-book
`a27b684` 0.

**A binary `else` fallback keeps the author's parens** —
`[gram.fmt.paren]` (#340). `x else (0 - 1)` printed as `x else 0 - 1`.
The meaning is the same either way, which is exactly why the parens are
the reader's; W0307 stands down when the author has "parenthesized
either reading", and the formatter erased one of the two readings that
sentence names.

**`str.find` dispatches on the needle's length** (#335). A `&str`
pattern constructs a Two-Way searcher on every call, and lobo's ws31
profile priced the family at 1.9% of a keepalive request — with the
searcher's CONSTRUCTION at 0.69% by itself — for needles of one to four
bytes. Short needles take a first-byte scan and a tail compare;
`contains` joins them. 1.6–2.0x on the find itself, measured on a
request head with a parser's six needles. No exported symbol moves.

**Two seed corpus files pinned verdicts their clauses had retired**
(#341). `wordcount.lu` taught `tally[w] += 1` — the idiom s152 retired
— and `grammar/structlit_paren.lu` spelled its point with an equality
s155 made a refusal; the compiler's own ledger could not see either,
because the phase each header named stops before the rung that
answers. wolf-interp's is46 census found them from the other side.

Not moved: **#345** was already discharged at `3a7703c` — s154's #318
added `then` to `[gram.inv.ctx]` §6.2 with its position, and §6.2 is
the only contextual-keyword inventory in the tree (verified at
`be348b9`: no second table, and the grammar carries no `contextual_kw`
production). It was measured at the v0.2.11 tag, one commit early.

## 0.2.11 — 2026-09-11

THE CHAPTER THE MAINTAINER READ. 0.2.11 is the release where the two
things a reader of chapter 5 hit in the same week stopped being
refusals: `total[T]` adds through a bound, and a `Map` answers whether
a key is there. Both were ruled the day they were met.

    fn total[T: Num](xs: List[T], zero: T) -> T {
        var acc = zero
        for x in xs {
            acc = acc + x
        }
        acc
    }

    totals[k] = (totals[k] else 0) + cents

**Operators dispatch through traits** — `[type.trait.op]`. When the
left operand is a type parameter or a user type, `+ - * / %` are
`Add.add` … `Rem.rem`, prefix `-` is `Neg.neg`, `==`/`!=` are `Eq.eq`,
the comparisons and `<=>` are `Ord.cmp`; the operator IS the trait
call, with the operands lent. Two primitives stay builtin. A bare `T`
is refused with the note the ruling asked for, "add `T: Add` to the
bound", and the "later sprint" sentence is gone from everywhere it
stood. **`Num` is an alias bound** — `trait Num = Add + Sub + Mul + Div
+ Rem + Eq + Ord` — so the chapter's function reads as above and
compiles at `int` and `f64`.

**`Map[K, V]` is typed, and an absent key is a row** — `[type.map.key]`
and `[mem.map.absent]`. Keys are `str`, `int`, `char` and `bool` this
edition (E0418 for anything else, by name); `m[k]` is `V ! {none}`,
never `()`, handled the way every row is; `m[k] op= v` is E0417 with
the two spellings that do the work in the note. std.map's `has`,
`get`, `tally` execute on both tiers, and chapter 5's count runs on a
map alone.

**The one-line `if`** — `[gram.expr.if]`: `2 => if leap then 29 else
28`, closed by a contextual `then` that is a keyword in that one
position and a member name or a binding everywhere else. Braces stay
valid everywhere; no program that parsed before changes shape, and
the formatter never converts between the two forms (`[gram.fmt.if]`).

**The closure as a value, then the payload.** Chapter 4's `let both =
fn(c) tax(discount(c))` is a `fn` value whatever it captures
(`[type.fn.value]`, `[abi.native.closure]` rewritten: every fn value
is a pointer to a callable record). A channel carries any value the
type system can move (`[conc.chan.payload]`) — chapter 12's
`channel[Doc]` runs on both tiers. Fourteen of wolf-book's one-machine
samples close by that pair.

**The checked tier's byte budget is real, and a built `str` is an
allocation site.** `[exec.checked.budget]`: steps AND bytes, each
cumulative, exhausting either is `unsupported` — the twelve-line walk
that cost 16.6 GiB is 47.8 MiB and an honest refusal. `[mem.region.escape]`:
`region scratch { "re" + "gions" }` returned from a function is E1010
on both tiers, where 0.2.10 (and lupin 0.1.31) printed freed bytes.

**Two syscalls a request, gone** (`[os.fs.open]` mode 5, `accept4`,
`TCP_NODELAY` at the first write Nagle could hold back), and
`wolf --version | head -1` exits 0 (#282).

The pairing is stamped at **lupin 0.1.32** (pin `e0ce018`, s147's
fold, twenty-two commits inside v0.2.10). The two range arms close —
`grammar/match_range.lu` and `grammar/match_range_char.lu`, 0.2.10's
two Verdict rows. What stands is what lupin has not mirrored yet, by
sprint: the one-line `if` (wolf-interp#90, is45), the key protocol and
the region `str` (wolf-interp is46, #88), the operator bridge
(wolf-interp#92, is46); the standing rows are named in the pairing
section below.

### The pairing takes lupin 0.1.32 (#87 ritual, #281 control)

wolf is now differentially tested against **lupin 0.1.32** (`c3dd607`),
which declares `e0ce018` — s147's IR-volume fold, inside v0.2.10 — as
its conformance pin. The gap to this release is named: s148, r15's
release commits, s149 through s153 and s155 are on the compiler's side
of the pin, and is45 (0.1.33, declaring v0.2.10) is in its gate run as
this is cut. 0.1.31..0.1.32 is is44: the range arm (wolf-interp#83) and
#79's method half, which moved no corpus file.

**Predicted before the run: two counts per tier** — the two exit(0)
range witnesses moving from Verdict to agreement, and one verdict move
below the ledger (`rows/match_range_empty.lu`, lupin's E0201 becoming
E0815). A `fail`-pinned file is a completeness note whatever B answers,
so `match_range_empty` and `match_range_open` could move no count.
**Measured: two per tier, the same two, and the one row below.** Over
the full 570-file corpus (536 entries), both tiers, against
`target/release/wolf` built by `cargo xtask dist` at `5289501`, with the control against the 0.1.31 release archive on the
same tree:

                    checked                     native
    agreements      290 -> 292  (+2)            320 -> 322  (+2)
    completeness    141 -> 141   (0)            141 -> 141   (0)
    soundness         2 ->   2   (0)              0 ->   0   (0)
    unsupported     123 -> 123   (0)             95 ->  95   (0)
    hard             20 ->  18  (-2)             18 ->  16  (-2)
    coverage A      311 -> 311   (0)            341 -> 341   (0)
    coverage B      396 -> 398  (+2)            396 -> 398  (+2)
    coverage BOTH   277 -> 279  (+2)            305 -> 307  (+2)

    corpus/grammar/match_range.lu        Verdict -> agreement   (wolf-interp#83)
    corpus/grammar/match_range_char.lu   Verdict -> agreement   (wolf-interp#83)

    below the ledger:
    corpus/rows/match_range_empty.lu     B fail(E0201) -> fail(E0815)

**What stands at 0.1.32.** `checked 18 = 6 Diag + 2 SOUNDNESS + 10
Verdict`, `native 16 = 6 Diag + 10 Verdict`. The Diag rows are #167's
warning asymmetry (`binder_capitalized`, `discarded_result`,
`float_zero_minus`, `region_never_allocates`, `byte_view_escape`, plus
`safety_comment_missing` on checked and `unit_context_discard` on
native) and SOUNDNESS is #168's float-cast twins — 0.2.10's rows, none
moved. The ten Verdict rows are this release's own sprints ahead of
their mirrors, `exit(0)` here and `fail(E0201)` there: the nine
positive one-line-`if` witnesses (`grammar/if_then_arm.lu`,
`if_then_block.lu`, `if_then_chain.lu`, `if_then_ident.lu`,
`if_then_let.lu`, `if_then_member.lu`, `if_then_paren_default.lu`,
`if_then_stmt.lu`, `if_then_width.lu` — lupin stops at `then`,
wolf-interp#90, is45's) and `traits/op_total_num.lu` (the alias form
`trait Num = …`, wolf-interp#92, is46's). In the completeness class,
by name: `memory/region_str_concat_return.lu` and
`region_str_concat_send.lu` (E1010 here, lupin runs to `regions` —
wolf-interp#88), `traits/op_eq_no_trait.lu` (E0301 here, lupin's
structural `==` runs it), `traits/op_missing_impl.lu` and
`op_hetero_add.lu` (E0502/E0514 here, `unsupported` there),
`typecheck/map_compound_absent.lu` (E0417 here, a run-time refusal
there) and `map_struct_key.lu` (E0418 here, exit 1 there — the
interpreter never reads a type argument), and the two wolf-interp#85
rows (`row_operand_compare`, `str_slice_assign`). Below every count,
the rows where both machines exit 0 and print differently:
`memory/map_absent_else.lu` (`none` here, `()` there),
`map_char_bool_keys.lu`, `traits/op_eq_inverting.lu` (the impl is
consulted here and not there). The CI sibling step needed zero edits,
the fifth re-stamp in a row.


### The key protocol (s152 — #11 and #154's `Map` rows ruled)

**`Map[K, V]` is typed, keyed by the four, and an absent key is a
row (#11, #154).** The count exercise in the book's chapter 5 carried a
parallel `List` for one reason: no program could ask a `Map` whether a
key was bound — std's `has`/`get`/`get_or`/`remove`/`tally` existed
and executed on neither machine (their `[K: Eq]` bound dispatched
nowhere, because wolfc never typed `Map` at all: every `Map[str,
int]()` was "a std/prelude stub without a signature"), and an
absent-key read answered `()`, the one unchecked, untyped read in the
language. The maintainer asked whether the list was there only to
check "have we seen it already" and guessed the exercise could be
pulled off with a map alone; they were right, and the ruling is two
clauses. **`[type.map.key]`** (spec/10 §10): `Map[K, V]` admits `str`,
`int`, `char` and `bool` keys this edition — the four whose equality
the language itself defines — and every other key is **E0418** where
it is spelled, in the constructor and in a signature position with the
same words; a struct key waits on derived equality, by name. `[K: Eq]`
is satisfied by the four through std.cmp's ordinary impls, so std.map's
five key functions execute on both tiers (`map_std_keys.lu` is their
bodies word for word). **`[mem.map.absent]`** (spec/02): `m[k]` is
**`V ! {none}`** — never `()`, never a zero, never a trap — handled as
every row is, and rendered as its row in a hole; `m[k] = v` on an
absent key inserts and on a bound key replaces; **`m[k] op= v` is
E0417**, E0416's sibling (the `str` index is not a place because a
`str` never changes; the `Map` index is not a place because the entry
may not exist), with a note naming the two spellings that do the work
— `m[k] = (m[k] else 0) + v`, and std's `map.tally(mut m, k)`. That
closes F-0011's filed question: the language rules the absence, std
owns the defaulting. Iteration order stays unspecified and consistent
(`pairs()` — both machines answer insertion order at this pin).

**The compiler.** `TyKind::Map(K, V)` beside `List`/`Pool`: the
constructor, the index read (a `{none}` union), the index write (a
place exactly when `V` copies, never a trap at the mem tier), `len`,
`is_empty`, `pairs() -> List[(K, V)]`, `clear`; `for (k, v) in
m.pairs()` destructures natively now (the tuple binder run inside the
loop's scope), which every tally the book writes wants. **The native
tier serves every admitted key** — `Map[str, int]`, `Map[int, int]`,
`Map[char, int]`, `Map[bool, str]` all execute — through five
`wolf_rt::map` seams (`RT_SYMBOLS` 138 → 143): a header whose first
five words ARE a `ListHdr` (so `len` is the list's load), entries laid
out as the `(K, V)` tuple's flat layout in insertion order, `get`/`set`
through one entry-shaped caller slot, `pairs` one byte copy into a
fresh list, a linear scan with `str` keys compared by the bytes they
name. The checked machine keeps the same entries as `Value::Map`.
What refuses, by name: iterating the map value itself (walk
`pairs()`), a `Map` in a hole, and — a finding, not this ruling — a
format spec on a `!T` hole (`{m[k]:>5}`) on both tiers, so
`{m[k] else 0:>5}` is the spelling (filed). **The messages**: E0417
"cannot update `totals[k]` in place: the key may be absent" with the
receiver labelled and the two fixes in the notes; E0418 "`Point`
cannot be a `Map` key" with the four named and, for a nominal, the
`str`-from-fields fix.

**Witnesses**, each `exit(0)` byte-identical on the checked and native
tiers with the stdout its header pins: `corpus/memory/map_count.lu`
(the chapter's count program on a map alone), `map_std_keys.lu`
(`has`/`get`/`tally` from std, over a local `Eq`), `map_absent_else.lu`
(the miss read with `else`, and rendered as `none`), `map_int_keys.lu`,
`map_char_bool_keys.lu`; refused: `corpus/typecheck/map_compound_absent.lu`
(E0417), `map_struct_key.lu` (E0418, at both spellings).

**Predicted, then measured.** Pairing rows under lupin 0.1.32,
predicted before the first run from the interpreter's `Value::Map`
and its `()` read, measured exactly: `map_count` refuses (`+` on `()`
and `i64`, exit 4); `map_absent_else` runs to `7 () 1 true` / `[()]`
where the compiler prints `7 0 1 true` / `[none]`; `map_int_keys`
refuses the index write through a computed key ("does not denote a
place", exit 4); `map_char_bool_keys` prints `() () ()`;
`map_compound_absent` refuses the `+=` at run time where the compiler
refuses it statically; `map_struct_key` runs to exit 1 (the interpreter
never reads a type argument). One row closes on its own:
`map_std_keys` is byte-identical — the interpreter's trait dispatch
through a bound already works. The mirror — the four-key set, the
`none` row on an absent read, the insert through a computed key — is
filed with the clause text (wolf-interp, for is46). The book's
chapter-5 rows, measured one at a time at this tree: none closes by
this move alone and every one moves from "a std/prelude stub" to a
named verdict — §5.2's block and the by-category tally stop at E0417
(`+=`), the top-2 tally at E0409 (`>` on `int ! {none}`, `[type.row.operand]`),
and "one index in the language is not checked" RUNS on both tiers and
prints `[none]` where its prose teaches `[()]`; respelled as the
clauses say (`= (… else 0) +`, `else 0` under the compare, `{… else
0:>5}`), the three tallies execute on both tiers byte-identical to
the transcripts the chapter prints. Those respellings are bs40's.
The syntax-tier fail count is unchanged (E04xx is the checker's);
the lowering ledger takes the seven (five lower, two refuse at
typecheck) and one existing line moves — `grammar/interp_fmtcolon`
reaches the mem tier now that `Map` types, and stops at the format
spec on its `!int` hole.

### The bytes and the region (s153 — #308, #310)

**The checked tier's byte budget is real now, and the view is a view
(#308).** sc43 reduced wolf-std's runner kill to twelve lines: a walk
that re-reads `rest.bytes()[0]` over a shrinking `rest` cost the
checked tier ×3.8 per doubling of the input — 1867 MiB at 8192
characters, 16.6 GiB on `cavp_sha384_long.lu` — flat on native and
lupin, under a step budget that could not see it. The allocation was
`eval_bytes_view`: since s136 the machine charged a consumed
`s.bytes()` zero (correctly, `[mem.str.view]`) and STILL materialized a
`Vec<Value>` per call and pushed it onto its list table for the rest
of the run — uncharged and retained, n + (n-1) + … values. The view now
hands its consumer the receiver's octets and mints nothing: indexing
reads one byte off the `str`'s own storage, `for b in s.bytes()` walks
the octets, the `len`/`count`/`is_empty`/`get`/`first`/`last` family
answers off them, and nothing survives the expression. Measured on the
reduction, peak resident set (`/usr/bin/time -l`, macOS aarch64):
2048/4096/8192 characters were 131/485/1867 MiB and are 10/11/11 MiB;
the CAVP row is `unsupported: step budget exhausted` at **47.8 MiB**
where it was 16.6 GiB. The other half of the same blindness: a `str`
built by `+`, `+=` or an interpolation with a hole charged nothing
either, so `s = s + s` forty times was a tebibyte the budget never saw;
builders now charge the shadow budget and the ambient region's ledger
(one byte per byte, what the native strbuf path pays), `+` charging
BEFORE it builds so the refusal precedes the allocation. **The rule,
stated — `[exec.checked.budget]`, new, in a new `exec` namespace
05-conformance.md owns:** the tier keeps two budgets, steps AND bytes,
each cumulative, and exhausting either is `unsupported`, never a
verdict; bytes are not a step cost (weighed and rejected: one ceiling
prices a 64 KiB walk and a 64 KiB allocation the same, and the finding
was that they are not). What the byte budget buys is the invariant
behind it — the machine retains on the host nothing the ledger has not
charged — so the resident set is bounded by the budget times the
machine's per-unit overhead, and a bounded host sees the refusal, not
the kill. Witnesses: `corpus/strings/bytes_view_walk.lu` (the
reduction, all three lanes) and the driver's `checked_budget` test,
which runs the reduction as a subprocess and asserts the peak under
200 MiB and flat across three doublings (`wait4`'s `ru_maxrss` on
unix, `K32GetProcessMemoryInfo`'s peak working set on windows), and
runs the doubling and asserts the honest refusal under 1.5 GiB.

**A built `str` is an allocation site (#310).** `region scratch { let s
= "re" + "gions"; s }` returned from a function printed `regions` from
freed bytes on wolf 0.2.10 and lupin 0.1.31 alike — with a W1001 saying
`scratch` never allocates — while the same block holding a `List[int]()`
was E1010 all along: the mem tier modelled `+`'s result as a site-free
copy view. **`[mem.region.escape]`, new:** a value whose site lies in a
region must not outlive it (returned, sent, stored to module state,
held outside, or the block's own value — E1010, dynamically
`region-fault`), and the sites include every built `str`: `s + u`,
`s += u` (`[type.str.concat]`) and an interpolation with a hole, each an
allocation in the ambient region of the building expression, never an
operand's. A `str` is `Copy` — the two-word view copies — but the bytes
it views live where they were built, so a whole-local `str` read now
carries its binding's sites out with it; a literal is no site, and
views allocate nothing. In the compiler: `+` on `str` and a holed
interpolation mint a `CallResult` site; `+=` on a `str` place mints one
and lands it through the same region flow `=` uses (extracted as
`flow_store`); the `+=` target is recognized by its operand, because
sema types an assignment target through `place_type` and records
nothing under its span. Witnesses `corpus/memory/region_str_concat_return.lu`
and `region_str_concat_send.lu`, `fail(E1010)` on both tiers; four
`mem_diagnostics` snapshots (block value, interpolation held outside,
append held outside, and the clean shapes — builders in the caller's
region returned, a scratch-region build consumed in place, no W1001).
Corpus motion: zero existing verdicts; wolf-std's tests at the mem
phase re-swept under the new rule (see the s153 report). lupin 0.1.32
runs both region witnesses to `regions`, exit 0 — wolf-interp#88 is the
mirror, now carrying the clause text and the send shape.


### The operator bridge (s155 — #5 closes, #176's `==` row closes)

**Operators dispatch through traits when an operand is a type
parameter or a user type (#5).** Chapter 5's `total[T](xs: List[T])
-> int { xs[0] + 1 }` was refused with "no trait covers this operator
yet (operator traits are a later sprint)"; the maintainer hit it as a
learner and asked whether the sprint should be bumped up "now that
we've encountered it in the wild — we want to attract people to the
language with friendliness". This is that sprint, ruled 2026-09-11:
**`[type.trait.op]`** (spec/10 §11). `+ - * / %` are `Add.add`
`Sub.sub` `Mul.mul` `Div.div` `Rem.rem`; prefix `-` is `Neg.neg`;
`==`/`!=` are `Eq.eq` (negated); `< <= > >=` are `Ord.cmp` read
against `Less`/`Greater` and `<=>` is the `Ordering` itself. The
operator IS the trait call — the same checking, the same dispatch
record, the same lowering as `Add.add(a, b)`, with the operands as
`read` arguments (lent, never moved: `a` serves two operators without
a `copy`). It applies when the left operand is a type parameter or a
user nominal type, never to two primitives (`int + int`, `str ==
str`, `str + char` stay builtin whatever impls are in scope).
**Homogeneous this edition**: `fn add(self, other: Self) -> Self`, no
output type; a trait of the table's name with another shape is the new
**E0514** at the operator, and `Money * int` waits on a stated need.
**The messages.** A bare `T` stays E0501 — "the bounds on `T` say
nothing about `+`" — with the note the ruling asked for, **"add `T:
Add` to the bound"**, and a machine edit that inserts it (the "later
sprint" sentence is retired everywhere it appeared: the diagnostic, the
E0501 registry prose, `golden_arith.lu`, `golden_eq.lu`). `==` on a
struct with no `Eq` in scope is **E0301** naming the trait and where
it comes from (std.cmp), not "operator traits"; a struct with the
trait in scope and no impl is **E0502** naming the trait and the
operator. `!`, `&&`, `||` and the bitwise family have no trait and say
so. Nothing is synthesized (D49): an enum without `impl Eq` is refused
like a struct.

**`Num` is an alias bound** — `[type.trait.op.alias]`, and the one
grammar move: `trait_item` gains the `=` form (`[gram.item.trait]`),
`trait Num = Add + Sub + Mul + Div + Rem + Eq + Ord`. A bound naming
it means every trait in its list, in every position a bound is read
(the body's capabilities, the instantiation's obligations — an unmet
one is E0502 naming the trait, never `Num` — and the impl search); a
cycle is E0503 once; `impl Num for T` is E0507. So the chapter's
function reads `fn total[T: Num](xs: List[T], zero: T) -> T` and
compiles at `int` and `f64`. std's half (the traits, `Neg`, the
primitive impls, `Num`) is wolf-std sc44's, filed with the exact
signatures; the witnesses carry the same declarations word for word.

**The compiler.** `Lower::resolve_bounds` expands alias bounds at
elaboration so every consumer sees traits only; `synth_bin` and
`synth_prefix` route a type-parameter or nominal left operand through
`operator_dispatch` (bound lookup or `trait_target` by name, the shape
check, the right operand against the left, an `OblOrigin::Operator`
obligation for a nominal, the `Dispatch::Trait` record at the
operator's span); WIR's `lower_bin`/`lower_prefix` read the record and
call `lower_trait_call_exprs` (the qualified-call lowering, now over
expressions instead of `Arg`s), then `operator_dispatch_result` turns
`Ord.cmp`'s tag into the comparison and negates `Eq.eq` for `!=`; the
mem tier lends the operands and types the answer as a call result;
the checked machine runs the impl body through the same
`trait_concrete`/`resolve_trait_body` path a qualified call takes.
Also on the checked machine: a payload-free variant as a bare value
(`Ordering.Less`) now evaluates (#23's member form) — every `Ord.cmp`
body needs it, and std.cmp could not run there before. Not moved: compound
assignment on a user type keeps its E0409. (`%` on `f64` had no native
lowering when this shipped, so the witness's `impl Rem for f64` spelled
the remainder by hand; s156 ruled it `fmod` and lowered it — the body
is `self % other` now.)

**Predicted before measuring, measured at this tree.** `op_total_num`
(`6` / `7.5`), `op_money` (`150` / `-150` / `50`), `op_eq_inverting`
(`false` / `true` / `true` — the inverting impl IS consulted, #176's
row), `op_ord_struct` (`true` / `false` / `true` / `less`): exit 0 on
both tiers, byte-identical. Under lupin 0.1.32, as predicted: the
alias form is `E0201: expected \`{\`, found \`=\``; `+`/`<` on a
struct are `unsupported: \`+\` is not defined on Money and Money`;
`op_eq_inverting` prints `true` / `false` / `false` — structural, the
impl never consulted — which is is46's clause (wolf-interp, filed).
The book rows that close by name are in the lane report for bs41:
§5.3's E0501 transcript (the note re-renders), §5.5's boundary
sentence ("`==` and `<=>` do not dispatch through `Eq` or `Ord`"), and
the ledger's #176 row 2.

### The one-line if (s151 — #307 ruled)

**A brace-less `if`, closed by a contextual `then` (#307).** `if leap
{ 29 } else { 28 }` in a match arm read heavier than it needed to; the
maintainer ruled a bare form, `2 => if leap then 29 else 28`, under
three rules that all bind. **`[gram.expr.if]`**:

```
if_expr ::= 'if' expr 'then'? block ('else' (if_expr | block))?
          | 'if' expr 'then' expr  ('else' (if_expr | expr))?
```

Braces stay valid everywhere and a `then` before a block is optional,
never required — no program that parsed before this clause changes
shape or meaning (rule 1). `then` is required only in the bare form,
because it is the token that closes a greedy condition (`if x -1 else
0` would swallow the `-1`; that is the work the braced form's `{` does
— rule 2). And `then` is **contextual, not reserved** — `[gram.inv.kw]`'s
fifty are unchanged: after a `.` it is a member name (std's
`Ordering.then`, called as `less.then(greater)`), as a binding it is an
identifier, and it is the keyword only after a complete `if` condition,
so `if then { … }` with a bool named `then` still parses (rule 3). The
two branches of one `if` share a form; an `else if` in a chain picks
its own. **`[gram.amb.else]`** gains its sentence: inside a bare branch
the `if`'s own `else` binds first, so `if c then f() else 0` is the
two-way `if` and a defaulting `else` there is written `(f() else 0)`;
after a braced `if`'s `}` nothing changes (`else b` with neither `if`
nor `{` following is the defaulting operator, as it always was); the
newline rule holds for the bare form too. In the compiler the lexer
never sees a keyword: the parser reclassifies `Ident("then")` as
`ThenKw` in that one position (the `self` precedent) and builds a bare
branch as a brace-less `Block` holding one trailing `ExprStmt`, so
sema, the mem tier, both lowerings and the checked machine read the
two spellings alike and changed by zero lines. **The message**: `if c
29 else 28` was a bare E0201 ("expected `{` to open the block"); it is
"expected `{` or `then` after the `if` condition" now, with a note that
names both spellings — the same note under the two other refusals,
mixed forms in one `if` (`if c then 29 else { 28 }`) and a `let` in a
bare branch.

**`[gram.fmt.if]`, new.** The formatter **never converts between the
braced and the bare form** — the author's choice stands; this is the
one place `[gram.fmt]`'s one-shape rule yields, and it yields to rule 1.
The alternative, a canonical bare form in value position, would have
rewritten 68 lines across four repos (corpus 9, wolf-std 9, lobo 39,
wolf-book 11, measured 2026-09-11) and was not taken. Inside a form it
normalizes: `then` before `{` is dropped; a bare `if` whose one line no
longer fits the width breaks to the braced form — the only conversion,
one direction, and a fixed point, because the bare form's BROKEN
rendering is the braced form (one `Group` per chain, `IfBreak`s at the
seams) and the round-trip modulus reads a bare block as `{ expr }` and
a `then` as layout. A bare chain breaks as one; a braced `else if`
inside a bare chain keeps its braces behind a shield and does not make
the head break. Parens around a defaulting `else` in a bare branch are
load-bearing and kept (`Ctx::BareBranch`). Damage inside a bare branch
is the statement's — it has no frame to repair inside — and mixed
forms pass verbatim, so the formatter never repairs a refused program.

**Witnesses**, `corpus/grammar/`, each `exit(0)` byte-identical on the
checked and native tiers with the stdout its header pins:
`if_then_let.lu`, `if_then_arm.lu` (the ruling's program),
`if_then_stmt.lu`, `if_then_chain.lu` (an `else if … then` chain and
one with a braced tail), `if_then_paren_default.lu`
(`[gram.amb.else]`), `if_then_block.lu` (`then {`, `fmt: relaid`),
`if_then_ident.lu` (`if then then 1 else 0`), `if_then_member.lu` (a
`.then(` call in a condition; the method's own body a bare `if`),
`if_then_width.lu` (the fallback, `fmt: relaid`); refused:
`if_then_missing.lu`, `if_then_mixed.lu`, `if_then_let_body.lu`,
`fail(E0201)` at parse on both tiers.

**Predicted, then measured.** Pairing rows under lupin 0.1.32:
predicted 7 (the bool-named-`then` and `.then(` witnesses were to be
byte-identical), measured **9** — every positive witness stops at
lupin's `E0201: expected `{`, found identifier `then``, because those
two use the bare form on a later line as well (`if then then 1 else
0`; the method body); the three refusals agree with lupin by code,
E0201, at another column. wolf-interp's mirror carries the clause
text. Ratchets: corpus syntax-tier fail count 23 → 26; the `fmt:
relaid` pin 2 → 4; lowering ledger +9 `lowers`, +3 pre-wir;
lane-coverage 300/331/331 (union 348, all-three 283) over floors held
at 291/318/318/335/274; IR-volume nine new paths, reported and never
red, transcribed from the linux lane; RT_SYMBOLS unchanged.
**Formatter motion over the real trees** — `wolf fmt --check` file by
file at wolf-std `046bd4f` (447 files) and lobo `d792290` (147 files),
the new binary against the 0.2.10 control: **predicted zero, measured
zero** (std's 32 non-canonical files are the same 32 under both; lobo
has none under either). Source motion, tests aside: kind +7, ast
+28/−1, parser +150/−19, fmt +179 (lower +167, canon +12), spec +58/−5
— predicted 3 / 20 / 70 / 130 / 45; the formatter ran over by the two
damage rules the prediction had not priced, the parser by the two
extra refusal messages and their note.

### The closure as a value, then the payload (s150 — #300 closes, #282 closes)

**A capturing closure is a `fn` value (#300).** Chapter 4's
`fn_as_value` — `let both = fn(c) tax(discount(c))`, then
`adjusted(340, both)` — was refused at the WIR tier as "a capturing
closure read as a value (the pair stays in its frame)": s105 lowered a
capturing closure to a two-word pair that never left the frame that
built it, and the `fn(int) -> int` a callee receives is one word. It is
still one word, and the word now means the same thing whatever stands
behind it: **`[abi.native.closure]` is rewritten** so every fn value is
a pointer to a callable record whose first word is the entry (a
function taking the record first, the declared parameters after); a
call through a fn value is one load and one indirect call, record
leading. A named function or a capture-free closure sits behind a
static one-slot record (the named function's slot holds a shim that
drops the leading pointer); a capturing closure's record is the entry
word followed by its captured values, allocated in the **ambient
region** of the frame that builds it — the placement every container
gets, so a closure returned from `rounder(5)` outlives `rounder`. Spec/10
gains **`[type.fn.value]`**: a closure is a fn value whatever it
captures, and its captures are COPIED when it is written — the value
never sees a later write to a captured place (W1102 already said so),
and while it is still needed a write to a captured `var` is E1002
(`[mem.tier0.borrow.2]`'s loan, which is what keeps copy and reference
indistinguishable). The mem tier lets a capturing closure be claimed
by a `return` or by an enclosing closure's tail, demanding each
captured value outlive the frame; a closure inside a container or a
struct literal still refuses by name. Witnesses:
`typecheck/fn_value_capturing.lu` (the chapter's program, verbatim),
`typecheck/fn_value_captured_int.lu` (passed, returned, captured by
another closure), `typecheck/fn_value_captured_var_write.lu`
(`fail(E1002)`; lupin traps `exclusivity` at the same write).

**A payload beyond one word (#268's largest family).** s39's channel
carried one machine word and E1102 refused every aggregate, which held
chapter 12 §12.1's `channel[Doc]` on one machine through two releases.
**`[conc.chan.payload]`**: a payload is any value the type system can
move — `[conc.chan.type]`'s four kinds, and a struct, enum or tuple
whose every field is a payload other than a region value (a region
crosses on its own, never inside another value: a copied handle would
be two owners). A send copies the payload into the channel, the
channel owns that copy in flight, `recv` (a `for`, a `select` arm)
hands it to the receiver. The cost, stated in the clause: a payload of
one word or less crosses in the word; anything wider — a `str` view, a
struct, a tuple, a float, a handle — crosses in a heap box the sender
fills and the receiver empties (`__wolf_rt_box_new` / `box_take`, one
allocation and two copies per message); a send that fails frees its
box before answering, and a boxed channel frees what is still in it
when it goes. A payload built in a frame-local region is E1010 at the
send, the way it would be at a return, and the message names the
receiver. Witnesses: `conc/chan_struct_payload.lu` (§12.1's shape,
byte-identical under lupin 0.1.31), `conc/chan_payload_escape.lu`
(`fail(E1010)`; lupin traps `region-fault` at the read).

Measured against wolf-book's 91 one-machine samples, one
`conform-run --native` each, prediction first: **14 close** — chapter
4's `s6`, `s7` and exercise 4-9; the whole payload family of eight
(`ch10/part-served`, `ch10/s7`, `ch12/s5`, `ch12/s6`,
`ch15/part-roster`, `ch15/part-sup`, exercises 11-5 and 14-10); and
the three `fail(E1102)` rows underneath it (`ch12/s1`, `ch12/s2`,
`ch14/s8`). Exercise 4-1 (`compose = fn(f, g) fn(x) f(g(x))`) clears
the mem tier and stops at sema: `f(g(x))` through an unannotated
closure parameter records no call surface. The `fail(E1001)` six are
the language's, not the compiler's (`ch12/s4` is a trap fence; the
rest are `copy` — the `best[T]` reading); `mut` arguments beyond local
places stays a sprint (c06's runtime shape).

**`wolf --version | head -1` (#282).** The toolchain's own text —
`--version`, `--help`, `--explain`, `--man`, `--completions` — is
written through one helper that treats EPIPE as a quiet exit 0 and any
other write failure as the failure it is (reported, exit 1);
`println!` had panicked with `failed printing to stdout: Broken pipe`
on every one-line read of the version. A test closes the pipe before
the child can write.

### Two syscalls a request, gone (s149 — #289 closes, #290 closes)

lobo's ws27 counted, with `strace -c -f` over a whole `ab` drive on
the linux runner, what a request costs the kernel on lobo and on the
pinned nginx serving the same config: keepalive **7.41 against 6.14**,
close **13.36 against 10.13**. Three of that gap were not lobo's code
and not lobo's fault — they were what the wolf runtime and the wolf fs
surface made a server do.

**The path stat (#289).** `fs_open` in read mode is `open(O_RDONLY)`,
and on unix that call PARKS on a fifo with no writer — the whole hand,
until a writer appears. A web root is operator-controlled, so lobo's
router asked `fs_is_file(path)` before every `fs_open(path)`, which on
the regular files that are every request that matters is a walk of the
name the open is about to walk again: `statx` 2.00 where nginx pays
`fstat` 1.00. spec/11 now carries **`[os.fs.open]`** — the open family
as one call under three spellings, its mode set stated (0 read, 1
write, 2 append, 3 read-write, 4 create-new) and widened by one:
**mode 5, read non-blocking**. On unix it carries `O_NONBLOCK`; a
regular file is unaffected in every respect, and a fifo or device
answers a HANDLE at once, which `[os.fs.fstat]` classifies as `kind` 2
and whose read is `eof` (no writer) or `io` (a writer with nothing to
say). So a static-file server's shape is `fs_open_mode(p, 5)` then
`fs_fstat(fd)` — two calls, no path stat, and no name a program was
handed can park it. windows has no equivalent on this path and serves
mode 5 as mode 0, by name. The clause also says what the mode is NOT:
a regular file's READ still blocks on the disk, on every unix, flag or
no flag.

**The accept posture (#290).** Two more calls per accepted connection
that nginx does not make: `ioctl(FIONBIO)`, because std's accept asks
the kernel for `SOCK_CLOEXEC` only and the runtime then wrote the
non-blocking flag itself; and `setsockopt(TCP_NODELAY)`, #254's
default, set on every stream at acceptance. Both are now paid
differently and neither is paid twice:

- Where the host has **`accept4`**, the runtime asks for the whole
  posture in the accept — `SOCK_NONBLOCK | SOCK_CLOEXEC`, one call
  where there were two. linux has it. **macOS has no `accept4` at
  all**, and the clause says so rather than pretending: there the take
  is `accept` + `ioctl(FIOCLEX)` + `ioctl(FIONBIO)`, three calls,
  measured on this box by interposing libSystem (dtruss wants SIP
  down) — and no kqueue arrangement changes it, because the cost is in
  the acquisition, not in the wait.
- **`TCP_NODELAY` is paid at the first write Nagle could hold back.**
  Nagle only delays a write that follows bytes of the same stream
  still in flight, so the FIRST write on a connection leaves at once
  whatever the option says; the runtime sets it before the next write
  that finds bytes in flight — including the continuation of a large
  drain, whose final short chunk is exactly the segment Nagle would
  sit on — and a connection whose one response is followed by a close
  pays it **never**. `net_nodelay` called by the program is still
  answered at once, either way, and `false` ends the deferral rather
  than postponing it.

**What a program can observe: nothing.** Both clauses move a COST, and
`corpus/net/accept_posture.lu` is that sentence's other half — one
write then a close arrives whole, head-then-body arrives whole and in
order, the option toggles, a forged handle is `io` — every relation
true before the change and after it. The clauses' own instruments are
beside them: `wolf_rt::net`'s crate tests read the posture and the
option BACK from the kernel at each step (so "unset after one write,
set before the second" is measured on every unix host), and
`crates/wolf_rt/tests/syscall_shape.rs` is the linux COUNT, read with
`strace` — the same instrument ws27 counted with — asserting `accept4`
exactly once with both flags, no `ioctl(FIONBIO)` on the accepted fd
ever, no `setsockopt` before the first write and exactly one before
the second, one `openat` carrying `O_NONBLOCK`, and no path stat in
front of it. The linux CI job installs `strace` and sets
`WOLF_SYSCALL_REQUIRE_STRACE`, so a runner without a tracer is a
broken job and never a quiet skip.

**PREDICTED, before any of it was measured** (ws27's cell, calls per
request on the runner): keepalive 7.41 → **6.41** against nginx's
6.14; close 13.36 → **10.34** against 10.13; per accepted connection
on the close shape, the accept side 3.10 → **1.08**. The prediction
this repo can settle by itself — the shape, not lobo's rate — is the
one `syscall_shape.rs` asserts, and macOS's 4 → 3 was predicted and
then measured exactly. lobo's own number is ws29's to report.

Witnesses: `corpus/fs/open_nonblock.lu` and
`corpus/net/accept_posture.lu`, three lanes each; the fifo itself is a
crate test (`#[cfg(unix)]` — the language has no `mkfifo`, so a corpus
witness could not build one). lupin 0.1.31 answers the net witness
byte-identically and declines the fs tier by name, which is where its
mirror is filed (wolf-interp#86): mode 5 lands there with the tier.

## 0.2.10 — 2026-09-10

THE SWITCH A READER EXPECTS, AND THE MESSAGE THAT STOPPED BLAMING THE
COMPILER. 0.2.10 is the release where `match` in statement position
became the switch a reader from any C-family language reaches for, and
where the things a reader ran into this week were answered by the
language rather than by an apology from the compiler.

    match n {
        0 => ...,
        1 | 2 => ...,
        3..=9 => ...,
        n if n % 2 == 0 => ...,
        _ => ...,
    }

**Range arms** are the clause `[gram.pat.range]`: `lo..hi` and
`lo..=hi` over integer and `char` literals, in any pattern position,
composed with `|`, guards and `@`. Both ends are literals — no
identifiers, no expressions, and no open ends (`..hi` and `lo..` are
slice spellings, and the parser now says the word "range" when it
refuses one). An empty range is E0815 at compile time. Exhaustiveness
computes no union: a `_` or binder arm is still required after range
arms, and E0801's witness names the first value past everything
covered. **The guarded arm lowers.** `n if n % 2 == 0 =>` before `_`
was refused by native lowering as conservatism — while the checked
machine and lupin ran it — and the refusal named the wrong arm. It
lowers now on both tiers; the maintainer's switch is
`grammar/match_switch.lu`, byte-identical on both machines.

**The message that stopped blaming the compiler.** `s[l-1..l] = "{t}"`
was answered "cannot compile this yet — assignment through this place
(… the conservatism ledger, not a bug in your program)". That was
backwards: a `str` never changes after it is built, and a slice of one
is a view, so there is no place on the left of that `=`, today or
ever. `[mem.str.imm]` says so, and **E0416** says it to the reader —
`cannot assign through a str slice: s is immutable` — with the three
spellings the book teaches in the note.

**A send in a loop body is a warning, not a mismatch.** `chan.send` is
typed `() ! {closed, cancelled}` now, the row `[conc.chan.close]`
always described, and the question that raised — what does a fallible
`()` mean where `()` is expected? — is answered by `[type.unit]`: the
body of a loop, an else-less `if`, a unit function are unit contexts,
and a `!()` tail there is a warned discard (W0601), never an error.
`ch.send(v)?` hands the failure to the scope and `else` handles it; the
corpus spells the first. A task closure's tail is different and the
clause says why: the scope consumes it and re-raises the row at its
exit.

**The declared type is read.** A body's tail is checked against the
declared result (`[type.fn.ret]`, E0401 at the tail), and a `!T` is
two values that no operator reads (`[type.row.operand]`, E0409 on
either side). Nothing moved in the compiler; both rules were enforced
and neither was written, and lupin 0.1.31 enforces both.

Two things about hashes and stamps, for anyone who records them. **The
interface hash is over the interface** (#292): a toolchain bump no
longer moves every module's `export_hash` — this release's own
`.wolfi` snapshot re-record moved the two `toolchain` lines and not
one hash, where 0.2.9's moved every hash in the file. And **the D57
stamp's commit is seven characters by decision** (#301): `wolf
0.2.10+dev.<seven hex>` from every clone, not whatever `git rev-parse
--short` guesses from the object count.

The pairing is stamped at **lupin 0.1.31** (pin `4c60946`, v0.2.9
itself). `strings/concat_mix_char.lu` closes — the last of 0.2.9's five
partings — and is43's witnesses moved from run to refused on the
interpreter's side. What stands parts on s147: `grammar/match_range.lu`
and `grammar/match_range_char.lu` (lupin stops at `..` in a pattern,
wolf-interp#83), with `rows/match_range_empty.lu` and
`grammar/match_range_open.lu` refused at a different site;
`rows/negative/row_operand_compare.lu` (E0409 here, E0401 there) and
`typecheck/str_slice_assign.lu` (E0416 here, `unsupported` there) are
wolf-interp#85. The four range files are byte-identical the day is44
tags.

### The pairing takes lupin 0.1.31, and the control finds its own blind spot (#304)

wolf is now differentially tested against **lupin 0.1.31** (`e9c55b4`),
which declares `4c60946` — v0.2.9 itself — as its conformance pin. That
pin advanced by exactly s145 plus r13's release commits; the gap to
trunk is s146, s147 and s148. 0.1.30..0.1.31 is is43: the declared type
gets read (a body's tail, wolf-interp#73; a bare row operand, #81), a
`char` joins a `str` (#78), a type annotation's name resolves (#79).

**Predicted before the run: four counts per tier** — `concat_mix_char`
unsupported → agreement, and is43's three witnesses each closing a
divergence. **Measured: one count per tier**, and the reason is the
finding. Over the full 534-file corpus, both tiers, against
`target/release/wolf` built by `cargo xtask dist` at `a6007fb`, with
the control against the 0.1.30 release archive on the same tree:

                    checked                     native
    agreements      280 -> 281  (+1)            307 -> 308  (+1)
    completeness    130 -> 130   (0)            130 -> 130   (0)
    soundness         2 ->   2   (0)              0 ->   0   (0)
    unsupported     115 -> 114  (-1)             90 ->  89  (-1)
    hard             10 ->  10   (0)              8 ->   8   (0)
    coverage A      289 -> 289   (0)            316 -> 316   (0)
    coverage B      384 -> 381  (-3)            384 -> 381  (-3)
    coverage BOTH   269 -> 270  (+1)            294 -> 295  (+1)

    corpus/strings/concat_mix_char.lu   unsupported -> agreement   (#278 / wolf-interp#78)

The is43 witnesses did move — 0.1.30 ran each of them (`()`, `hi`,
`4`, `small`; exit 0) and 0.1.31 refuses each by a code — and moved no
count, because a `fail`-pinned file is a completeness note whatever
the interpreter answers. Only `coverage B 384 -> 381` saw them, and
not by name: the control r14 built to catch exactly this kind of
silent delta had one class it could not see into. **Fixed in the same
release** (#304): the below-ledger list carries a verdict move beside
the stdout move it already carried, and the re-run names them,
identically on both tiers:

    corpus/typecheck/tail_declared_str.lu          B exit(0) -> fail(E0401)
    corpus/typecheck/tail_declared_union.lu        B exit(0) -> fail(E0401)
    corpus/rows/negative/row_operand_add.lu        B exit(0) -> fail(E0409)
    corpus/rows/negative/row_operand_compare.lu    B exit(0) -> fail(E0401)

The fourth was not predicted as a move at all: wolf says E0409 there
and lupin E0401 (wolf-interp#85), and a file that was never "hard"
cannot close.

**What stands at 0.1.31.** `checked 10 = 6 Diag + 2 SOUNDNESS + 2
Verdict`, `native 8 = 6 Diag + 2 Verdict`. The Diag rows are #167's
warning asymmetry (`binder_capitalized`, `discarded_result`,
`float_zero_minus`, `region_never_allocates`, `byte_view_escape`, plus
`safety_comment_missing` on checked and s146's `unit_context_discard`
— five W0601 lupin does not warn — on native); SOUNDNESS is #168's
float-cast twins. The two Verdict rows are s147's
`grammar/match_range.lu` and `grammar/match_range_char.lu`: `exit(0)`
here, `fail(E0201)` there, lupin stopping at the `..` in a pattern.
`rows/match_range_empty.lu` (E0815 vs E0201) and
`grammar/match_range_open.lu` (E0201 at the missing end vs E0201 at
the operator) sit in the completeness class beside them. All four are
wolf-interp#83, is44's, and byte-identical the day it tags.
`rows/negative/row_operand_compare.lu` and `typecheck/str_slice_assign.lu`
(E0416 here, `unsupported` there) are wolf-interp#85. No class opened
at this pin and none was deferred; the CI sibling step needed zero
edits, the fourth re-stamp in a row.

### The stamp's commit is seven characters by decision (#301)

`git rev-parse --short` picks its width from the clone's object count,
so the same revision stamped `wolf 0.2.9+dev.e0ce018` on a long-lived
checkout and `+dev.e0ce0189` from a fresh clone, and wolf-book's
byte-compared `--version` transcripts (bs36) were green everywhere a
human looked and red in the one place that clones fresh. The stamp is
D57's pin clause, and a pin whose text depends on who cloned is not a
pin. `cargo xtask dist` now asks for `--short=7` — seven is what every
published stamp has printed and what lupin's own `build.rs` pins — and
a test holds the width. Anything that reproduces the stamp by hand
must spell the same seven: wolf-book's `book.yml` computes
`WOLF_COMMIT` itself and wants `--short=7` in the same breath.

### The row is ruled where it belongs (s148 — #284 closes)

Two rules both machines enforced and no clause stated. A body's tail
was checked against the declared return type by reading
`[gram.expr.block]` together with `[gram.expr.tagident]` — a clause
about tag resolution that mentions "a fallible function's tail" inside
a list; a bare `!T` in operator position was refused by inverting
`[type.interp.union]`'s carve-out for string holes. wolf-interp is43
found the interpreter reading neither (it printed `()` under `-> str`
and exited 0 under `-> !int` with no status written; it unwrapped `!int`
at run time inside `+`), and wollf wl10 measured the gap at 56 of
2,153 generated programs. spec/10 now carries both: `[type.fn.ret]`
(§8 — the tail, `()` when the block ends in a statement, is checked
against the declared result, the ok half of a fallible one; E0401 at
the tail naming the declaration) and `[type.row.operand]` (§9 — a `!T`
is two values, never one; `?`, `else` and `match` are the whole of its
handling, and an operator on a `!T` operand is E0409 on either side).
The E0401/E0409 sites and both catalog entries cite them. is43's four
witnesses land in the shared corpus (`typecheck/tail_declared_str`,
`typecheck/tail_declared_union`, `rows/negative/row_operand_add`,
`rows/negative/row_operand_compare`) and the compiler answers all four
as pinned. **No behaviour moves**, and one of the issue's readings was
wrong: the compiler at the 0.2.9 pin answers `n <= 5` on a `!int` with
E0409, not the E0401 the issue quoted, so witness 4 pins E0409 — the
clause's code. Two gaps the clause names as follow-ups: the compiler
reports E0401 naming the other side when the row is the *right*
operand (`5 <= n`, `1 + n`), and lupin 0.1.31 reports E0401 for a
comparison on either side.

### A `str` slice is not a place, and the compiler now says so (s148 — #293 closes)

`s[l-1..l] = "{t}"` — the maintainer's chapter-3 binary-spelling
table — was answered "cannot compile this yet — assignment through
this place (… the conservatism ledger, not a bug in your program)",
which is the opposite of the truth: a `str` is immutable at every tier
and a slice is a view, so there is no place on the left of that `=`,
today or ever. spec/02 now says it directly — `[mem.str.imm]`, a
`str` never changes after it is built; an index or slice of one is not
a place — where before it was a parenthesis inside
`[mem.str.view.lend]`. **E0416** is the new sema diagnostic
(`cannot assign through a str slice: s is immutable`), for `s[a..b]`,
`s[i]` and their compound forms, with the clause in the note and the
fix the book teaches spelled out (`s = "{head}{bit}{tail}"`,
`s = head + bit + tail`, `std.strbuf` for a hot loop); lupin refuses
the same program as `unsupported: a slice expression denotes a value,
not a place`, and parity on the code is wolf-interp's half. The
conservatism arm `assignment through this place` keeps only what
sema does not type as a place yet — a tuple on the left
(destructuring assignment, unruled), a deref, a call result, a
literal, and a bracket receiver whose *read* is not served yet
(generic application, std containers beyond `List`, a `Pool` cell,
a tuple index). Witness: `corpus/typecheck/str_slice_assign.lu`,
`fail(E0416)`; two catalog snapshots.

### The interface hash is over the interface (s148 — #292 closes)

`crates/wolf_sema/src/interface.rs` hashed the compiler's own release
string (`CARGO_PKG_VERSION`) into the head of both partitions, so
every toolchain bump moved every module's `export_hash` and `pkg_hash`
on packages nobody edited — the book measured it at 0.2.8 → 0.2.9:
`tree=` and `manifest=` held, `interface=` alone moved (§25.2), and
§22.2's "if it does not move, your importers cannot tell" was true
only within one toolchain. **Ruled: the hashes are over the
interface's content** — edition, package and module path, dep export
hashes, items, impls, dyn records, trusted roster — and not over the
release string, the magic or the encoding version, which are the
file's container. The toolchain stays stamped in the `.wolfi` header
and printed by `wolf interface`, informational. A hash that moved now
means one thing on every toolchain: something you can call changed.
Correctness never rode on the old head — the `.lu-cache` rebuild key
carries the toolchain on its own (`KeyComps::env`), so a compiler
upgrade still rebuilds every object; and should the language surface
ever need to move every hash, `EDITION` (a surface revision, `v1`) is
what bumps, never a version. `wolf publish`'s `interface=` address is
taken over the same stamp-free rendering (`digest_text`). The one
`.wolfi` snapshot re-recorded once (`interface_pretty`); a new test
builds the same package under two stamps and asserts every hash and
dep hash holds while the header and `wolf interface` still show the
stamp (`version_only_bump_moves_no_hash`). `docs/modules.md` says
what a reader should take from a hash.

### The switch a reader expects: range arms (s147 — #287 ruled, #286 closes)

`match` in statement position is wolf's switch, and the one thing it
lacked against a reader's expectation was a range arm. The maintainer
ruled it (#287) and refused the alternatives — a second keyword would
carry a second grammar, a second exhaustiveness story, and reopen
fallthrough. `[gram.pat.range]` is the clause: `lo..hi` and `lo..=hi`
in any pattern position, both ends **literals of one type** — integer
literals on every integer primitive a literal arm already takes
(`byte` takes none, so no range either), `char` literals ordered by
scalar (`[type.char.order]`). No identifiers, no expressions, and no
open ends: `..hi` and `lo..` are slice spellings, and the parser
refuses them in pattern position with a note that says the word the
book's ch03 papercut row was waiting for — "range". An empty range
(`5..5`, `9..=3`) is **E0815**, decided at compile time because the
ends are literals. Ranges compose with or-patterns, guards, and
`@`-bindings; overlap is legal and the first arm wins.

**Exhaustiveness computes no union.** Integer and `char` columns stay
"infinite" — for every domain, `u8` and `char` included — so a `_` or
binder arm is still required after range arms, and this is said in
the clause rather than left to be discovered. What changed inside the
engine is the witness and the subsumption: E0801 now names the first
value past every covering range and literal (`0..10` alone witnesses
`10`, not `0`), and specialization is by containment — a row headed
by `0..10` survives specialization by `5` and by `2..=4`, so a literal
inside an earlier range is E0802 citing the range arm. An arm covered
only by the union of several earlier ranges is reported reachable:
conservative, never falsely dead.

**Both tiers lower a range arm to two comparisons** at the
discriminant's own width — the `u*` conditions for an unsigned
scrutinee, as `<` already does — joined by `band`, folded when the
discriminant is a build-time constant; the checked machine compares
scalars through the one shared `char` decoder. Inside a product
(`d @ 1..=9`, `(0..10, _)`) the same test lands on the projection.

**#286 beside it.** The maintainer's switch-shaped program — `0 =>`,
`1 | 2 =>`, `n if n % 2 == 0 =>`, `_ =>` — was refused by native
lowering as conservatism while the checked machine and lupin ran it,
and the refusal named the wrong arm ("a guard on the closing
unconditional arm" — the closing arm was the `_`). A guarded binder
arm is an unconditional pattern on a conditional arm, a
test-and-fall-through like any other: it lowers now, the guard's
failure re-entering the chain at the next arm, and the old refusal is
an invariant no caller reaches, with a message that names *this* arm
and the reason. `grammar/match_switch.lu` is the witness, byte-identical
on both machines.

**Predicted before the first edit, then measured.** Parser one
alternative, sema one constructor threaded through `specialize`,
lowering one arm per tier, four witnesses, one code. Measured over the
compiler's source (tests and snapshots aside): spec +43, ast +40,
parser +88, registry +12, checker +105, engine +45, ubcheck +29, wir
+215 — ten files, +603/−37. The parser and the native lowering ran
about twice the prediction: the literal atom became a function so the
range tail could reuse it, the open-end refusal is two sites (before
and after the operator), and the lowering's range test wanted its
product-domain twin and a signedness-aware fold the prediction had not
priced. Everything else landed within a dozen lines of its estimate.

**The pair's named divergence.** Four files —
`grammar/match_range.lu`, `grammar/match_range_char.lu`,
`rows/match_range_empty.lu`, `grammar/match_range_open.lu` — parse and
run (or refuse by the new code) here and stop at lupin 0.1.30's
`expected `=>`, found `..``. wolf-interp#83 carries the clause text
and the two parser sites; the four are byte-identical the day it
lands. Not in this sprint: `str` ranges (no order clause), float
ranges (equality only), negative endpoints (a literal arm takes no
`-` today either).

### The tail in a unit context (s146 — #275 closes, #283 closes)

`chan.send` was typed `()` on the compiler while `[conc.chan.close]`
said a send after close returns an error value. Typing it
`() ! {closed, cancelled}` — the row it always was — would have turned
every `for i in 1..=n { ch.send(i) }` and every else-less
`if ready { ch.send(1) }` into a type mismatch: 13 corpus witnesses,
26 sites, chapter 12 on every page. No clause said what a fallible
unit value *means* where `()` is expected, and s146 wrote it.
**`[type.unit.context]`** lists the unit contexts — the body of a
`for`, `while` or `loop`, the then-block of an else-less `if`, a unit
function's body and its `return` operand, a closure checked against
`fn(…) -> ()`, and any block or arm checked against one.
**`[type.unit.discard]`**: a `!()` tail in one is a discard, warned
(W0601), never a mismatch — costed against the handled reading, which
would have made *position* decide between a warning and an error, a
rule the language has no other instance of and had just retired
(#276). Only `!()` qualifies: a `!int` tail where `()` is expected is
a mismatch on the `int`. **`[type.unit.consume]`**: a closure with no
fixed result is not a unit context — `s.spawn(fn() { ch.send(v) })`
infers `fn() -> !()` and the scope consumes the value, re-raising a
failed task's row at its exit (`[conc.task.fail]`); no warning,
because that is the row reaching the one reader who can act on it.
`send`'s row joins `[conc.chan.close]`. Sema records the discard in
every unit context and the typed wave warns W0601 at each — W0601's
text now names the tail; a closure's `?`-raised row flattens into its
fallible tail (D51: chapter 10's `answer.send(words_of(t)?)` is one
`() ! {closed, cancelled, unknown}`, not a nested union); the native
lowering joins `send`'s status through `chan_status_join`. The 26
corpus sends spell `?` (one unit fn spells `else {}`); three witnesses
— `conc/chan_send_closed_row.lu` (the send side of the clause:
`late: closed`, `relayed: closed`), `typecheck/unit_context_discard.lu`
(four unit contexts, five W0601) and
`conc/spawn_tail_send_raised_row.lu` (the flattened union, re-raised
at the scope exit) — and all sixteen touched witnesses byte-identical
under lupin 0.1.30. Beside it, **#283**: the nightly's blast-radius
property admits the one designed reach-back — a line-leading `else`
withdraws the terminator above it, and the one statement it closed may
take the `else` in — asserted exactly, with the nightly's case pinned
in the ordinary gauntlet.

### The pairing takes lupin 0.1.30, and this time the control has teeth

wolf is now differentially tested against **lupin 0.1.30** (`08d787a`),
which declares `2c03ed9` as its conformance pin. That pin advanced by
exactly s144 — ten commits, `2523d87..2c03ed9` — and s144 is exactly
the three rulings the two machines were parting over: the `else` that
may start a line (#276), the lowercase `closed`/`cancelled` channel
marks (#273), and `[mem.list.pop]`'s recoverable `none` row (#274).
The remaining gap to trunk is fourteen commits and is exactly s145 plus
r13's own release commits.

**Four witnesses closed and one stayed, as predicted before the run.**
Over the full 521-file corpus, both tiers, against `target/release/wolf`
built by `cargo xtask dist` at `4c60946`:

                    checked                     native
    agreements      275 -> 279  (+4)            300 -> 304  (+4)
    completeness    123 -> 123   (0)            123 -> 123   (0)
    soundness         3 ->   2  (-1)              1 ->   0  (-1)
    unsupported     112 -> 111  (-1)             90 ->  89  (-1)
    hard             11 ->   8  (-3)              8 ->   5  (-3)
    coverage A      286 -> 286   (0)            310 -> 310   (0)
    coverage B      373 -> 376  (+3)            373 -> 376  (+3)
    coverage BOTH   265 -> 268  (+3)            287 -> 290  (+3)

Every Verdict-class divergence is gone. What is left parts on two
long-standing classes and nothing else: `checked 8 = 6 Diag + 2
SOUNDNESS`, `native 5 = 5 Diag`. The Diag rows are #167's
warning-channel asymmetry, name for name r13's set less
`safety_comment_missing` on the native tier; the two SOUNDNESS findings
are #168's float-cast twins. The third soundness finding closed.

**THE INTERPRETER BUMP MOVED FOUR COUNTS, and that is measured, not
assumed** (#281). The same corpus, the same tree, the same `wolf`
binary, run a second time against the **0.1.29 release archive**: the
control reproduced r13's table cell for cell (275/123/3/112, 11 hard;
native 300/123/1/90, 8 hard, coverage A 286/310, B 373, BOTH 265/287).
So the compiler contributed nothing and the corpus contributed nothing
— the whole delta above is the interpreter, and the ledger diff names
it, identically on both tiers:

    corpus/grammar/else_chain.lu            Verdict     -> agreement  (#276)
    corpus/grammar/else_default_newline.lu  Verdict     -> agreement  (#276)
    corpus/memory/list_pop_empty.lu         SOUNDNESS   -> agreement  (#274)
    corpus/strings/byte_view_lend.lu        unsupported -> agreement

The fourth was not predicted: `List.first` entered lupin's std subset,
so a file that had sat in the conservatism ledger now runs and agrees
byte for byte. That is the case for a control stated twice over — the
prediction was right about the three divergences and silent about the
fourth move, and only the control could tell the difference.

A **fifth** file moved where no count can see it. `compare` reads
`stdout_sha256` only when both records declare `seeded`, and no corpus
file is seeded, so an interpreter that changes what it PRINTS moves
nothing in the table:

    corpus/conc/chan_closed_row.lu   `nothing left: Closed` -> `nothing
                                     left: closed`, byte-equal with
                                     wolfgang at sha 5413b635 (#273)

That is r13's finding standing up again from the other side, and it is
why the control now reports below-ledger moves as their own list.

**One witness stays, exactly as predicted.**
`corpus/strings/concat_mix_char.lu` still reads `exit(0) vs
unsupported` — `` `+` is not defined on str and char`` — because 0.1.30's
pin sits before s145. It is a conservatism-ledger entry and not a
divergence ([proto.cmp.defined-divergence]); the mirror is
wolf-interp#78. No class opened at this pin and none was deferred.

The CI sibling step needed **zero edits** for this bump, the third
re-stamp in a row: it reads the version off `crates/wolf_driver/PAIRING`
and the digest off the release GitHub reports, and neither is written
down in the workflow.

### The control is a standing step, not a lane's good habit (#281)

`cargo xtask differ` takes **`--control <prev>`**, where `<prev>` is
either a lupin binary or the release `.tar.gz` itself — the same archive
CI's sibling step fetches. It re-runs the identical corpus with the
identical impl A against the previous interpreter, diffs the two
ledgers, and prints `THE INTERPRETER BUMP MOVED N LEDGER COUNT(S)` with
the files named, the moved cells listed, and — the part r13 had to find
by hand — the moves that live **below** the ledger, where a changed
stdout on an unseeded file reaches no count at all. Roughly ninety
seconds. The ritual's own file, `crates/wolf_driver/PAIRING`, now
carries the step and the command in its header, so the next re-stamp
reads it before it starts guessing. The diff is pure and unit-tested,
so its teeth are felt on every run of the xtask suite rather than once
per release.

## 0.2.9 — 2026-09-09

THREE THINGS A READER RAN INTO THIS WEEK, AND NONE OF THEM WAS
SOMETHING WOLF MEANT TO TEACH. A rule about where a line may begin, a
mark spelled against the language's own pact, and a `char` that could
not join a `str`. 0.2.9 is those three and the rulings behind them.

The first was layout. An aligned chain —

    if n > 0 {
        ...
    }
    else if n < 0 {
        ...
    }
    else {
        ...
    }

— was an error, E0005, because the newline after `}` inserted a
statement terminator and left the `else` orphaned. The rule was Go's,
and it was the one place in the language where layout was load-bearing.
It is retired: **a line whose first token is `else` continues the
previous statement.** The chain above compiles, and so does a
defaulting `else` on the line below the call it defaults. E0005 leaves
the catalog and its number is never reused.

**The formatter did not move.** `wolf fmt` still lays a chain as
`} else {` on one line, so the canonical shape is exactly what it was;
the aligned source above formats to it and stays there. What changed is
that the parser no longer refuses the reader who writes it the other
way before running the formatter.

The second was a name. `"12x".to_int()` answers an error row, and until
this release that row was `NotAnInt` — the one CapCase payload-free
mark on the whole builtin surface, in a language whose own lint says
otherwise. W0603 has always warned that a payload-free mark is a
lowercase word and that CapCase names a payload's type; the OS families
spell their conditions `not_found`, `utf8`, `closed`; the json builtins
already answered `parse` for exactly this condition. The row is
**`parse`** now. This is the one source-breaking change in 0.2.9 and it
announces itself: a program that still matches `NotAnInt` is refused by
name, with a note reading `the cases are: parse`. `str.to_int` is also
ruled for the first time — `[mem.str.parse]` says which text parses,
which is the row, and that nothing here ever traps — and `to_float` and
`to_bool` are ruled out of existence rather than left ambiguous.

The third was `t += cs[i]`, building a string one character at a time,
which both machines refused with E0409. **A `char` is text, not a
number.** A Unicode scalar appended to a `str` is closed under UTF-8
and has exactly one rendering, so `s + c` and `c + s` are
interpolation-append in either order and `s += c` follows. `str + int`
still refuses, and the clause now says why rather than leaving it to be
inferred: `+` is not a formatter, and an `int` has more than one
rendering — sign, radix, width — so the interpolation hole stays the
place to choose one.

Around those three, the release is mostly the language saying out loud
what it had been doing. `{x}` renders every value both machines can
show — a tuple, a struct, a `List`, an enum variant, a caught row, a
`!T`, an exit reason — and refuses by name what has no promised
rendering. `return` inside a closure returns from the closure, typed
against the closure's own result, which is how both lowering tiers and
the interpreter had always run it. `channel[T]` in a signature position
is a channel, so a worker taking its inbox as a parameter can iterate
it. `[conc.chan.close]` spells its tags `closed` and `cancelled`, and
`[mem.list.pop]` says the recoverable `List` reads answer `none` and
never fault. And the corpus IR-volume gate is per file now, so the
ratchet can tell a composition change from a regression instead of
firing on every witness that enters.

The pairing is stamped at **lupin 0.1.29** (pin `e9a17cb`). The `parse`
row is mirrored there, so `rows/to_int_parse.lu` and
`grammar/else_default.lu` are byte-identical across the two machines
for the first time. Five witnesses still part, and every one is a
ruling from this release whose mirror is filed and not yet tagged:
`grammar/else_chain.lu` and `grammar/else_default_newline.lu` (E0005
under lupin), `memory/list_pop_empty.lu` (`trap(bounds)` for `none`),
`conc/chan_closed_row.lu` (`Closed` for `closed`) and
`strings/concat_mix_char.lu` (lupin refuses `+` on `str` and `char`).
The first three close at lupin 0.1.30; the last waits on
wolf-interp#78.

### A `char` joins a `str` (s145 — #278 closes)

A reader built a string one character at a time — `t += cs[i]` in a
reverse-words loop — and both machines refused it with E0409, the
D62 note pointing at an interpolation hole. The ruling
(wolf-lang#278) is that **a `char` is text, not a number**: a
Unicode scalar appended to a `str` is closed under UTF-8 and has
exactly one rendering, so `s + c` and `c + s` are interpolation-
append in either order — `"{s}{c}"`, `"{c}{s}"`, the `char`
contributing its scalar's UTF-8 bytes — and `s += c` is `s = s + c`.
`[type.str.concat]` gains the sentence; `[type.str.concat.mix]`
drops its two `char` rows and says why the `int` rows stay: `+` is
not a formatter, and an `int` has more than one rendering (sign,
radix, width), so `str + int` and `int + str` keep E0409 and the
hole. The two sema arms admit a `char` on either side of `+` and as
the value of `+=`; lowering reuses the `{c}` hole's own path on both
tiers (`__wolf_rt_strbuf_char` natively, the scalar's bytes on the
checked machine); E0409's note now names what `+` joins, and its
catalog entry moves with it (the `str_plus_char` fixture retires).
Witness: `strings/concat_mix_char.lu` flips from `fail(E0409)` to
`run` — both orders, `+=`, two- and four-byte scalars, a `chars()`
loop — exit 0 on both tiers; `concat_mix_int.lu` and
`concat_int_str.lu` do not move. The maintainer's `reverse.lu` runs
on both tiers and prints what lupin printed for the hole spelling.
Under lupin 0.1.29 the witness is refused at its first `+`
(`unsupported: `+` is not defined on str and char`, exit 4) and stays
so until the interpreter mirrors — wolf-interp#78 names the two arms
beside `(Str, Str)` in `eval/mod.rs`; it is the one corpus file the
pair parts on for this ruling, named here so the pairing re-stamp
can count it. Predicted before the edit: eleven files, the spec one
sentence and one reason, sema two arms and a note, one helper in the
native lowering, two arms on the checked one, one witness rewritten
and its three snapshots regenerated; measured: that, with the
lowering helper factored out (`str_concat_operand`) and the three
E0409 snapshots re-rendered for the note.

### `return` inside a closure returns from the closure (s145 — #268's next family)

wolf-lang#268's inventory, by count, after s143: `return` inside a
closure (7), `fail(E0301)` (7), Pool/shared constructor lowering (6).
The first lands. Seven of the book's samples — chapter 16's receivers
among them — spell `let r = ch.recv() else |_| { return }` inside a
spawned closure; both lowering tiers and lupin already ran the
`return` as leaving the closure, and only the compiler's typing
withheld it (`NotYet`: control typing). `[type.closure.return]`
opens §6 of spec/10 and writes the meaning down: **a `return` inside
a closure returns from the closure**, its operand typed against the
closure's own result — the context's on a checked closure, the
tail's on a synthesized one (the two meet at one type; disagreeing
is E0401 against the closure), the declared one on a nested `fn` —
and the enclosing function is out of reach. Sema keeps a
closure-result frame beside s73's row frame on all three closure
paths; the mem tier, which walks a closure body inline in the
enclosing CFG, gives the body its own exit join and return depth so
the `return` runs the closure's defers and rejoins the function
instead of jumping to its exit. Witness: `typecheck/closure_return.lu`
— the four shapes, native and lupin 0.1.29 byte-identical (six
lines, exit 0); the checked machine declines closures as it always
has. Measured against the seven, one at a time with `wolf
conform-run --native`: **exercises 16-2 and 16-10 close** (`sum=42`,
`seat 4 survives`, both machines) and are the book lane's to
graduate; exercises 16-7 and 16-8 move to E1001 (`visited[c - w]`
moves out of the list — `copy`, the `best[T]` finding again); the
chapter-10 reader block and exercise 10-4 move to E0401 (`return 0`
in a unit closure — a bare `return` is the program; lupin ran them
leniently); exercise 17-9 types and compiles, and the native binary
parks at its two-rendezvous deadlock where lupin traps
`deadlock` — the runtime's deadlock-detection row, not this family.

The other two families were measured and not started, and the wave
stops here. `fail(E0301)` (7): `saturating[i32]` (chapter 3),
`Scope` (chapter 11, four times) and `Proc` (chapter 14, twice) are
names no clause and no prelude declares — lupin runs the programs
because it never reads a parameter's type annotation (a `fn f(x:
Bogus)` runs under lupin 0.1.29 and is E0301 here), so the fence
cannot stand on both machines by any compiler move: the programs
are the book's to respell, or `saturating[T]` is a clause to write
first. Pool/shared constructor lowering (6): the checked machine
serves `Pool[T]()` and lupin traps `stale-handle` on three of them;
the native tier has no runtime shape for a pool handle (c06's), a
sprint of its own, not a rung to slip into this one.

### The `else` may start a line (s144 — #276 closes)

A reader wrote an aligned `if` / `else if` / `else`, each `else`
leading its line, and both machines refused it with E0005: the
newline after `}` inserted a terminator, and the orphaned `else` was
the one place the termination rule constrained layout. The rule was
Go's. The ruling (wolf-lang#276) is that readers do not learn it: **a
line whose first token is `else` continues the previous statement.**
`[gram.lex.newline]` gains one more lookahead exception beside the
attribute one — no terminator is inserted at a newline when the next
token is `else` (trivia between does not count) — and `[gram.amb.else]`
now says the two `else`s are disjoint by binding, not by line: after
an `if`'s `}` it is the branch, after a complete expression it is the
defaulting operator, on one line or across two. The lexer looks one
token ahead; the parser's two E0005 sites are gone and E0005 is
retired from the catalog (its number is never reused).

**The formatter does not change.** `[gram.fmt.brace]` keeps `} else`
on one line and `[gram.fmt.inline]` keeps its guard-clause rule, so
`wolf fmt` re-lays what the parser now accepts: the aligned layout
formats to the canonical `} else` chain and is a fixed point from
there (`crates/wolf_fmt/tests/style.rs` pins the maintainer's layout
verbatim and what it becomes). Witnesses: `grammar/else_chain.lu`
carries the aligned layout verbatim and the `}` newline `else {`
shape; `grammar/else_default_newline.lu` pins `let v = f()` newline
`else 0`. Both are refused under lupin 0.1.29 (E0005 at the leading
`else`) and stay so until the interpreter mirrors (wolf-interp, same
wave): they are two of the five corpus files the pair parts on at the
release's pairing, and the re-stamp counts them as `Verdict`
divergences on both tiers.

One delta from the plan, found before the gauntlet: `[gram.fmt.canon]`
requires every corpus file to be formatter-canonical, and a witness
whose point is a leading `else` cannot be. The clause gains its one
exception — a header `//! fmt: relaid` declares a source layout the
parser admits and the formatter re-lays; `xtask fmt-lu` and the
formatter's stability property read past such a file (its count is
pinned at two), idempotence and round-trip still hold on it, and
`wolf fmt corpus` by hand would re-lay it, so the gate's hint now
names files. lupin's directive reader treats an unknown key as prose.
Predicted before the gauntlet: lexer one helper, parser two deletions,
corpus +1 file and 1 edited, catalog −1 code; measured: that, plus the
canon exception (directive, two gates, one clause sentence).

### The parse family is ruled, and its row is spelled `parse` (s143 — #265 closes)

`str.to_int()` was served by every implementation and ruled by none.
`[mem.str.parse]` and `[mem.str.to_int]` in spec/02 now say what the
four agreeing implementations had been doing: the family is one
method (`to_float` and `to_bool` do not exist — a float parse needs
the float text grammar ruled, which s38 never did, and a bool parse
answers nothing `s == "true"` does not); the surrounding
`[mem.str.ws]` run is ignored, exactly `trim`'s set; the value is an
optional sign and one or more ASCII digits that fit `int`, leading
zeros included; everything else is the row, never a trap — the empty
and blank text, a sign alone, an interior space, `_`, a radix prefix,
a fraction, an exponent, a non-ASCII digit, trailing text, and a
magnitude outside `int` (both machines agree since lupin 0.1.28,
wolf-interp#69; r12 brought those rows into the corpus battery).

The row is **`parse`** now, not `NotAnInt`. s142 copied the
interpreter's spelling, which made it the one CapCase payload-free
mark on the builtin surface, and the language's own pact says
otherwise: payload-free marks are lowercase words and CapCase names a
payload's type — W0603 warns a program that breaks it and lists
`parse` among its examples, the OS families spell their conditions
that way (`not_found`, `utf8`, `closed`), and the json builtins
already answer `parse` for exactly this condition. A spec that
enshrines a spelling the compiler's own lint calls a contradiction
was weighed against one wave's rename and lost. The cost, stated in
the clause: the interpreter's builtin and its two tests, the book's
chapter 1 (one printed block, two lines of prose, exercise 6-9 and its
solution — which also declares a CapCase mark W0603 flags), and here
the three implementations, their tests, the `str` completion's
signature and two witnesses. The compiler moves first: this release
raises `parse`; the mirror (wolf-interp) and the book move in the
same wave. The mirror landed inside this wave: **lupin 0.1.29 spells
`parse`**, so `rows/to_int_parse.lu` — the witness the two machines
would have parted on — is byte-identical across them at the release's
pairing (`error: parse`, stdout sha `3e419546`, both tiers), and so is
`grammar/else_default.lu`, whose `{err}` prints the same mark.
`strings/to_int.lu`, the battery, was rewritten not to spell the mark
and is byte-identical under both 0.1.28 and 0.1.29. No interface
moved.

### Three spellings a chapter was waiting on (s144 — #273, #274 ruled; #275 measured)

`[conc.chan.close]` now spells its tag: the closed error is the
payload-free row **`closed`**, and the cancellation an operation
answers at a channel's blocking point is **`cancelled`** — the
compiler's `recv` row as it has been, the OS families' spelling, and
W0603's pact (a payload-free mark is a lowercase word). The
interpreter mints `Closed`/`Cancelled` and the book's chapter 12
prints it; both follow in the same wave (#273). Witness:
`conc/chan_closed_row.lu` (native; the checked lane declines channel
methods), `nothing left: closed` here and `Closed` under lupin 0.1.29
until the mirror lands. The two verdicts are both `exit(0)`, so this
one parts only in the printed bytes — which the differential ledger
compares solely between `seeded` records and therefore never counts.
It is measured by hand at every re-stamp and named there.

`[mem.list.pop]` is the clause `grep pop spec/` could not find: the
recoverable `List` reads — `pop`, `get`, `first`, `last` — answer the
`none` row and never fault; `pop` on an empty list is `none`, not
`bounds` (`[mem.ub.defined]` rules indices, and `pop` takes none). The
compiler had answered the row since s37 on both tiers; the interpreter
trapped and the book's chapter 5 taught `pop` bounds-checked — both
move in the same wave (#274). Witness: `memory/list_pop_empty.lu`,
both tiers, `trap(bounds)` under lupin 0.1.29 until the mirror lands —
the release's one new soundness-direction finding, and the only one of
the five standing partings that reads as one.

**#275 is measured, not fixed.** `chan.send` is typed `()` where the
clause says a send after close returns an error value. Making it
`() ! {closed, cancelled}` on the compiler is one sema arm and one
wir join the `recv` path already has — and then every `ch.send(v)`
in a unit context is refused: a `for` body and an else-less `if`
check their tail against `()`, and a non-trailing statement of `!T`
is W0601. Thirteen corpus witnesses send that way, chapter 12 sends
that way on every page, and no clause says what a `!()` tail in a
unit context means. That rule is the real cost and it is a language
question, not a type fix; it is recorded on #275 for its own sprint.

### Two of the book's families stand on both machines (s143 — #268)

The book measured its samples against both machines for the first
time (wolf-book#9) and found 133 of 478 running on the interpreter
alone, 106 after r12. The two largest families by the compiler's own
refusal are gone.

**String interpolation of a non-primitive value** (10 samples, plus
the `{err}` that held `grammar/else_default.lu` at resolve). `{x}`
renders every value both machines can show the same way, and spec/10
`[type.interp.*]` now says how — the interpreter's rendering adopted
byte for byte: `()`; a tuple `(a, b)`; a struct `Doc { title:
regions, words: 900 }` (a `str` inside a composite is its bytes,
unquoted); a `List` `[1, 2]`; an enum variant qualified,
`Shape.Line(4)`; a caught row as its tag's name with its payload,
`too_short`, `BadDigit(q, 1)`; a `!T` as its ok payload or its row,
so chapter 5's `{popped}` prints without an `else`; an exit reason
`normal(1540)`, `error(Corrupt)`, `killed`, `cancelled`,
`fault(bounds)`. A format spec on a composite refuses, never silently
ignores. What has no promised rendering — a channel, a proc, a scope,
a region, a pointer, a fn, a `dyn`, the shared tier — refuses by
name (`[type.interp.none]`). Both tiers render: the checked machine
type-directs its renderer (a positional value is a tuple or a struct
by its type), the native tier walks the value's shape and reads a
tag's name through a per-module tag table synthesized after every
body has interned its tags (`wolf.tag_name`, one compare chain — a
site cannot switch over tags a later function is the first to
raise). One runtime fact moved with it: a proc's `int` result now
rides the exit reason (`__wolf_rt_task_value`, the entry shim hands it
over), so `{reason}` reads `normal(1540)` where the native tier said
`normal(0)` for every ok body. Witnesses: `strings/interp_values.lu`
(both tiers), `conc/reason_interp.lu` (native; the checked lane
refuses procs), `grammar/else_default.lu` (runs now), each
byte-identical under lupin 0.1.29. `else_default.lu` is the one of the
three that needed the interpreter to move as well: its `{err}` prints
the caught row's mark, so it parted under 0.1.28's `NotAnInt` and is
byte-equal under 0.1.29's `parse`. Predicted before the gauntlet:
sema's hole check, the checked renderer, the lowerer's print and
value-position paths, no runtime surface beyond the value stash;
measured: that, plus the tag table and the stash.

The samples, measured at this tree against lupin 0.1.28: four of the
ten close (ch05 §5.1's `{last}` block and exercise 5-1; ch14's and
ch15's opening `{reason}` blocks), and the other six now refuse or
reject for a reason that is not interpolation — ch05's `pop` on an
empty list (`none` here, `trap(bounds)` on the interpreter: `pop`'s
empty case is unruled, filed), ch10's and ch14-10's `channel[str]`
(payloads beyond one word), ch12's and ch14's `channel[Doc]` (E1102,
a static rejection), ch15's self-`link()` (E0402) and `?` in a fn
that cannot fail (E0604). The book's runner flips the four at its
next pin bump.

**The iteration protocol (for-trait wiring)** (7 samples). Not the
protocol at all: every one was a worker taking its inbox as a
PARAMETER, and `channel[T]` in a signature position — a parameter, a
field — stayed opaque where the expression `channel[T](n)` typed
`Chan`, so `for d in inbox` refused with the protocol's name. One arm
in signature elaboration types it; `conc/chan_param_for.lu` is the
witness (a plain call, a proc body, a struct field), byte-identical
under lupin 0.1.29. Predicted: one arm and no lowering change;
measured: exactly that. Five of the seven close (exercises 14-6,
14-7, 14-9; ch17's two `worker` blocks — the first prints its bug on
this machine's schedule, as the chapter says it may); ch14's shelver
and exercise 14-10 now stop at `channel[Doc]` (E1102) and
`channel[str]` (payloads beyond one word) — interpolation's leftovers,
not iteration's.

### The ratchet that can tell composition from regression (s143 — #270 closes)

`cargo xtask bench-gates` gated the corpus's IR volume as one number,
the geomean of `IR(midend) / IR(naive)` over every run-phase file the
release tier emits, ratcheted against a ceiling. Nine consecutive
re-records of that ceiling (s86 through s142) were population changes
— a witness entering at 2.3 because the mid-end inlines a twenty-call
helper — and not one was a regression; the gate fired on exactly the
event it was not meant to catch and never, in that history, on the
one it was. The gate is per file now: `bench/ir-volume.json` records
a ratio per corpus path with its two instruction counts, measured on
the linux/x86-64 lane like every gate number; an existing path whose
ratio rises past its record by more than the relative slack (5%) reds
by name with its counts; a path the record has never seen is reported
beside the table and never reds on its own; a recorded path the run
cannot emit is reported as conservatism. `cargo xtask bench-gates
--record` writes the table from the rig. The corpus geomean is still
printed on every run as the number the contract's 0.50 target is
stated against, but it is a reference line, not a verdict — its key
is `corpus_geomean_reference`, and its history in `bench/gates.json`
ends with the entry that says so. The kernel suite keeps its geomean
ceiling: the manifest fixes that population, so a move there is a
lowering change.

### The README's mark renders for everyone (found in the wave)

The logo at the top of the README was a hotlink into the private
planning repo, so every reader who was not the maintainer got a broken
image — on GitHub, on crates.io, and in the AUR page's rendered
README. The mark is vendored here now (`assets/wolf-logo.svg`) and the
tag is an absolute `raw.githubusercontent.com` URL onto this repo's
own `trunk`. Absolute on purpose: the README travels in the release
archive and is re-rendered by hosts that resolve no relative path, so
a relative `src` would break in exactly the places the hotlink already
had.

## 0.2.8 — 2026-09-09

THE FIRST CHAPTER COMPILES, AND THE SYSCALL GOES FIRST. This release
answers a learner at one end of the language and a server author at the
other.

The book's chapter 1 reaches for `row.to_int()` twice, in the cold open
and again in exercise 1-10. The compiler had refused that call since the
builtin set was drawn, so the first non-trivial string method a learner
met was also the first program that did not build. `str.to_int() -> int
! {NotAnInt}` is in the set on both tiers now, spelled the way the
reference interpreter has always spelled it, and text it cannot read is
the `NotAnInt` row rather than a trap. When a method really is outside
the set, the refusal says which method: ``this `str` method, `to_int`,
is outside the builtin set``, where it used to give a byte span and
leave the reader counting.

At the other end, a program that reads a socket the kernel has already
made ready now makes the read and waits for nothing. Every parking call
in the net family used to go to the reactor thread first (a waiter cell,
a lock handshake, a `kevent` to arm it, a `kevent` to wake it, a
condvar, two context switches) and only then make the syscall, on a
socket the program's own `net_wait` had just reported ready. lobo, the
web server written in wolf, paid that round-trip three times per
request. The same lobo source was built against the old compiler and
against this one and measured on a single macOS arm64 box in one session
six minutes apart: 53.3 µs per request before and 21.2 after, 18,766
req/s to 47,162, and the share of the serving thread parked in a condvar
fell from 40.7% to 1.1%. The `wolf-reactor` thread never started at all
in the second run, because the reactor is lazy and a serving loop that
never parks never asks for one.

Three smaller changes come with that one. `TCP_NODELAY` is on for every
TCP stream the runtime hands out, accepted or dialed, so the common
shape (answer with a head, then a body, two writes, the second small) no
longer stalls 40 ms behind the peer's delayed ACK; `net_nodelay(fd, on)`
sets it either way, and `net_writev` sends the head and the body as one
gathered write. `fs_fstat(fd)` answers `[kind, size, modified_ms]` from
one metadata read on an open handle, where a static-file server
previously paid four separate path stats for the same facts. And a
write's budget now covers its whole drain, so a `net_deadline` armed on
a stream bounds a large `net_write` to a peer that stopped reading;
before this release that drain rode a blocking syscall and ran past the
budget.

Every call that existed at 0.2.7 keeps its signature and its error rows,
so a program written against it compiles and behaves the same way here.

### The first chapter compiles (s142 — #263 closes)

`str.to_int() -> int ! {NotAnInt}` joins the builtin set on both
tiers. The book's chapter 1 reaches for it twice (`row.to_int() else
0` in the cold open and in exercise 1-10), the reference interpreter
has served it all along, and the compiler had refused it since the
set was drawn at s37 — so the first non-trivial string call a
learner meets was the first program that did not build. The row is
the one lupin 0.1.27 answers, spelled as it spells it. What the parse
accepts: the surrounding `[mem.str.ws]` run is ignored (ASCII and the
non-ASCII separators alike, so `s.to_int()` and `s.trim().to_int()`
are one value), then an optionally signed (`+`/`-`) run of ASCII
digits that fits `int`, leading zeros included. Everything else is
`NotAnInt`, never a trap — the empty and the blank text, a word, an
interior space, `_`, a fraction, a radix prefix, a non-ASCII digit, a
lone sign, trailing letters. A magnitude outside `int` is the row on
both wolf tiers: there is no `int` the text names, and checked
arithmetic never answers with a quiet wrap. The reference interpreter
at this pin parses that one input to a wider integer and prints it as
an `int`; filed as wolffe-lang/wolf-interp#69, and the corpus witness
(`strings/to_int.lu`, row for row with lupin) leaves that input to the
crate tests. `to_float`/`to_bool` are NOT added: neither the spec nor
lupin defines them, and the family is mirrored, not invented. The
spec is silent on the row — filed as #265; the clause will say what
this entry says.

And the refusal names the method. `wolf build: cannot compile this yet
— this `str` method (outside the builtin set) @155..167` made the
maintainer count bytes to learn that the span was `row.to_int()`; it
now reads ``this `str` method, `to_int`, is outside the builtin set``,
on the stderr line and in the observation record's
`x-unsupported-construct`, for every method outside the set.

### One stat on the handle (s142 — #261 closes)

`fs_fstat(fd: int) -> List[int] ! {not_found, denied, io}` —
`[kind, size, modified_ms]` from ONE metadata read on the open handle
(`[os.fs.fstat]`). lobo ws23 found a static-file request paying four
path stats (`fs_is_dir`, `fs_is_file`, `fs_size`, `fs_modified_ms`,
~0.5 µs each on macOS arm64) where nginx pays one `open` and one
`fstat`; the router's reorder took one away and the last two could not
move because nothing answered off the handle the request was about to
read. `kind` is 0 for a regular file, 1 for a directory, 2 for
anything else; `size` and `modified_ms` are `fs_size`'s and
`fs_modified_ms`'s words in their units. The rows are the path stat's;
a closed or forged handle is `io`. Every tier-1 host, every tier — the
checked machine reads the same handle metadata the runtime does. The
one host difference is `fs_open`'s and is stated in the clause rather
than papered: unix opens a directory read-only (`kind` 1 is
reachable); windows refuses the open first (`denied`), so there a
server still classifies directories by path. Witness
`corpus/fs/fstat.lu`; the directory case is the crate tests'
(`#[cfg(unix)]`). The `O_NONBLOCK` open mode #261 mentions as a
nice-to-have is not in this entry.

### The syscall goes first (s141 — #257 closes)

A ready socket is answered by the syscall alone. Every parking call
in the net family (`net_accept`, `net_read`/`net_read_bytes`,
`net_write`/`net_write_bytes`, and the new `net_writev`) now tries the
syscall before it waits, and waits only when the kernel answers that
it would block. Until now each of them parked on the `wolf-reactor`
thread first (a waiter cell, a lock handshake, a `kevent` to arm, a
`kevent` to wake, a condvar, two context switches) and only then ran
the syscall, on sockets the program's own `net_wait` had just
reported ready. lobo ws22 profiled it with `sample(1)` on macOS arm64
(one serving process, keepalive): 63 µs per request against nginx's
19 on the same box, and about 30 of the 63 was that round-trip, taken
three times per request, 37.8% of the serving thread parked in a
condvar, 8% in its own `kevent`, the syscalls themselves 12%. Now a
read on a ready socket touches no lock handshake, no condvar and no
`kevent`; a park happens on `WouldBlock` only, and the retry after
the wake goes against the budget computed when the call began, never
a fresh one (`[os.net.accept]`'s rule, now every call's).

Nothing a program can observe in the rows moves; the cost does.
Predicted first, from ws22's arithmetic: about 30 of 63 µs. Measured
the way the finding was made: lobo (trunk `023ec64`, its source
untouched) built once at the v0.2.6 pin and once at this branch
(`aec4d9f`, a dev-stamped `.wolf-bin` staged the way lobo's CI stages
one), `tools/lobo-parity` (five interleaved pairs, `ab -t 5 -c 32`
over four generators, both shapes, both cells) and the same
`sample(1)` for ten seconds at one millisecond on the N=1 serving
process under `ab -k -t 20 -c 32`, one session, same box, six minutes
apart. nomad-1, Apple M5 Pro, 18 cpus, macOS 26.4.1, 2026-09-09
01:15Z–01:21Z:

| | before (wolf 0.2.6, pin 398e5f5) | after (this branch) |
|---|---|---|
| N=1 keepalive, lobo req/s (median), µs per request | 18,766, 53.3 µs | 47,162, 21.2 µs |
| N=1 keepalive, nginx ÷ lobo, median [min, max] | 3.799x [3.612, 4.075] | 1.579x [0.943, 1.792] |
| N=1 close, nginx ÷ lobo | 2.273x [2.165, 2.687] | 1.116x [0.988, 1.324] |
| N=18 keepalive, nginx ÷ lobo | 2.446x [2.347, 2.536] | 1.597x [1.581, 1.602] |
| N=18 close, nginx ÷ lobo | 1.118x [1.072, 1.166] | 1.055x [1.045, 1.101] |
| main thread in `__psynch_cvwait` (of its samples) | 3,396 of 8,352, 40.7% | 95 of 8,450, 1.1% |
| main thread in its own `kevent` | 610, 7.3% | 0 |
| threads the process ran | main, `wolf-reactor`, `wolf-signal` | main, `wolf-signal` |

Thirty-two microseconds per request on the N=1 keepalive cell, and
the `wolf-reactor` thread never started: the reactor is lazy, and a
serving loop that never parks never asks for it. What remains at 21 µs
is the syscalls (`open` 33% of the thread, `sendto` 17%, `recvfrom`
5%) and lobo's own three stats, which are lobo's (wolffe-lang/lobo#3,
ws23). Both sets are marked REFUSED by the tool's quiet-rig rule,
load(1m) 5.05 and 5.10 before each set with two sibling lanes on the
box (the bar's rule is 3.0; the after set also saw nginx spread 1.86x
on the N=1 keepalive shape), so they are not an entry in lobo's
ledger; they are the same box, the same hour, the same load, and the
ratio between them is the number this entry claims. The bar itself
(1.10x) is not met on the keepalive shape and this entry does not say
it is; a quiet set and the linux runner's number are lobo's to take
at the next pin that carries this change (v0.2.7 shipped while this
branch was still red on the runner, and does not).

One consequence is stated because a program can build on it: a
write's budget covers the whole drain. A body larger than the send
buffer leaves in chunks with a park between them, each park against
the call's budget, so a `net_deadline` armed on a stream now bounds a
large `net_write` to a peer that stopped reading; before, the drain
rode a blocking syscall past any budget. The row is `net_write`'s `io`
(its `timeout` coarsened, as the call declares), and the bytes the
kernel took before the budget fired have been sent.

Every socket the runtime holds lives non-blocking on the three reactor
hosts (linux, macOS, windows), a listener since 0.2.6 and a stream
since now, the flag set on acquisition rather than trusted to a
kernel's inheritance rule; a tier-2 host with no reactor keeps the
blocking syscall. The checked machine answers the same rows through
its own blocking sockets and their timeouts; the reference interpreter
polls first and waits on not-yet already, so the differential is the
proof. Spec: `[os.net.io]` (new), one sentence in `[os.net.accept]`
(the accepted stream is under the same posture, its rows unchanged).
Witness: `corpus/net/syscall_first.lu`, a read on a socket `net_wait`
reported answers, and a budgeted large write to a peer that never
reads returns at the budget with the row, not a park. The runtime's
tests count this thread's parks: an accept with a connection queued,
a write into an empty buffer and a read of bytes `poll(2)` reported
park zero times.

### `TCP_NODELAY` by default, and a gathered write (s141 — #254 closes)

Nagle's algorithm is off on every TCP stream the runtime hands out,
accepted or dialed, and `net_nodelay(fd, on) -> () ! {io}` sets it
either way. lobo measured the reason on the CI runner (linux x86-64):
a server that answers with a head and then a body, two writes, the
second small, stalled behind the peer's delayed ACK, 40 ms per
request, 780 req/s against nginx's 84,493 on the same runner, a 108x
gap for a program that did nothing wrong. The posture Go, nginx and
node take is the one that makes a naive two-write server correct
without knowing this entry exists. lobo has carried `tcp_nodelay` as
`planned` in its directive table for twenty waves because nothing in
the language could honor it; now both directions can be. It is one
option, named, with a stated default and its rows, not a generic
`setsockopt`, and none is planned; the clause says what else that
shape admits (a keepalive switch) and what it does not (linger,
buffer sizes).

`net_writev(fd, parts: List[List[byte]]) -> () ! {closed, io}` sends
every part in order as one write, `writev(2)` on unix and `WSASend` on
windows, gathered by the kernel and never copied by the runtime, with
`net_write`'s rows exactly and the same posture: the syscall first, a
park on would-block against the budget computed at entry, the drain
resumed from the byte the kernel stopped at in whichever part it
stopped in. Empty parts send nothing. A head and a body now leave in
one syscall, and where two writes were two segments, one. The macOS
numbers above were taken with lobo still writing twice, so nothing in
them is this entry's; the linux number is, and it is lobo's to take.

Both are served on every tier-1 host and every tier: native, release,
and the checked machine, whose streams enter its table under the same
default and whose gather is the same std vectored write driven to
completion. The reference interpreter's mirror is filed with the
clause text as wolffe-lang/wolf-interp#67. Spec: `[os.net.nodelay]`,
`[os.net.writev]` (new). Witnesses: `corpus/net/nodelay.lu`,
`corpus/net/writev_gather.lu`.

## 0.2.7 — 2026-09-08

THE PAIRING IS CHECKED WHERE IT ROTS. `wolf --version` prints the lupin
release this compiler is differentially tested against, and that line has
one job: to be true. It stopped being true when the interpreter published
0.1.27 and `crates/wolf_driver/PAIRING` still said 0.1.26. The gate that
exists to catch exactly that did catch it, on every machine in the house
at once and on none of the six CI jobs, because the comparison needs a
lupin binary and no runner had one. Two lanes tripped over it
independently, in work that had nothing to do with the pairing. This
release re-measures the pairing against the interpreter as released, and
gives the linux CI job the pinned lupin release archive so the same
comparison runs on the runner, so the sentence `wolf --version` prints
about the reference interpreter is checked somewhere that is watching.

The tag carries two more pieces of work that a reader meets sooner than
the pairing. `wolf --help` answers a stranger now, with per-verb help, a
man page and shell completions in the archive, and three other things a
newcomer hit in their first five minutes are fixed beside it. And the
seven `[sched.*]` spec anchors are published, so a program may cite them.
Both are below.

### The one gate CI could not run (r10 — #253 closes)

The pairing test compares this build against the lupin release `PAIRING`
names, and per D57 it compares both halves of that claim: the version
always, and the declared conformance pin whenever the sibling is a release
build. It fires only where a sibling lupin binary exists. Runners had
none, so the single gate that compares the two implementations was the
single gate CI never ran, and rot in it arrived as a laptop surprise in
whatever sprint happened to be next.

A sibling checkout on the runner would be a campaign: a second repository,
its own toolchain pin, a build, and a cache to keep that build from being
paid every run. The pinned release archive is none of those things. It is
public, it is already built, it is two megabytes, and per D57 it is a
release build that prints its bare version, so both halves compare on the
runner exactly as they do beside a sibling checkout at the tag. The whole
of it is one step and two environment variables in a job that already
exists, and it cost one second of runner time on the run that proved it.

The step reads the version out of `PAIRING` rather than repeating it, so
it tracks the stamp and cannot go stale on its own; a stamp naming a
release that does not exist reds by name. The digest is never typed: it is
read off the release asset GitHub reports and checked against the bytes
that arrived. The fetch runs before the gauntlet with `LUPIN` set for the
job, so `cargo xtask ci`'s own test step is what compares, and the runner
runs the same gauntlet a laptop does rather than a second copy of one
test. `WOLF_PAIRING_REQUIRE_SIBLING` closes the way this could have been
worthless: an absent sibling is honest on a bare box and dishonest on a
runner that just downloaded one, so the test now decides which it is
looking at, and a failed download reds as a failed download instead of
arriving as a green skip.

### The pairing

The pairing moves to lupin 0.1.27 (pin `6ade878`), stamped against the
release as published rather than against its successor on the
interpreter's trunk: a release's pin is part of what that release is
(D57), and the release that exists declares `6ade878`, wolf-lang's own
v0.2.5 tag. The interpreter's trunk has since re-vendored to v0.2.6's
`398e5f5`, which is advance notice for the next stamp and not rot in this
one, exactly as the dev-sibling skip says out loud.

Re-measured bare, no `LUPIN=` override, against a clean checkout of the
`v0.1.27` tag built by itself, so D57's second half applied and version
and pin were both compared and both matched. The ritual over 508 files:
checked 269 agreements / 124 completeness notes / 2 soundness / 108
unsupported / 8 hard, native 290 / 124 / 0 / 90 / 5. Against v0.2.6's
measurement over the same 508 files (266 / 124 / 2 / 111 / 8 and 287 /
124 / 0 / 93 / 5) every row is flat but one, and that one is the same
move twice: agreements +3 and unsupported −3 on both lanes, with run-rung
coverage carrying the same fact (the interpreter executed 360 entries at
the previous stamp and executes 363 now). It is the sibling growing, not
this compiler. The three constructs the interpreter landed in its mirror
at 0.1.27 turn `unsupported` at resolve into `exit(0)` at run on its side,
so `corpus/net/reuse_port.lu`, `corpus/net/wait_readiness.lu` and
`corpus/os/cpus.lu` leave the conservatism ledger and enter agreements.

Hard divergence is flat at 8 and 5 and decomposes with nothing left over,
over the same two terms as v0.2.6: six (five) `Diag` rows are #167's
warning-channel asymmetry, the interpreter emitting no warnings at all,
and the two soundness findings are #168's float-cast twins, where wolfc's
own checked lane exits 0 and the native lanes trap. Every surviving row is
a filed issue older than this release. No class opened at this pin and
none was deferred. The interpreter's `[os.net.accept]` evaluation, landed
after the tag, moves no row here: `corpus/net/accept_race.lu` needs an
inherited listener the interpreter is never handed, so it stays
unsupported on both lanes as it was at v0.2.6.

One note for anyone whose gauntlet reds at this stamp with a pin
complaint. The suffix a lupin build prints is written by its build script,
and a build script's output is cached: a tree built once at the tag and
then moved off it can keep printing the bare version while compiling a
newer pin, which reads to this gate as a release build declaring the wrong
pin. That is the sibling's stamp being stale, not `PAIRING`. Rebuilding
the sibling with its build script forced to re-run (`touch build.rs`)
restores the `+dev` suffix and the row goes quiet.

### The stranger's five minutes (s140 — #249, #250, #251, #252 close)

`wolf --help` was one line of twenty-three pipe-separated verbs, printed
to stderr, exiting 2. Asking the compiler what it does was an error, and
the answer gave no verb's arguments. It exits 0 on stdout now, so it
pipes into a pager, and it says what wolf is, gives a first program that
can be pasted and run, points at the documentation, and states the D32
rule that a directory is a module before that rule bites. Every verb the
overview lists answers `--help` itself, both `wolf build --help` and
`wolf help build`. A wrong flag prints the same usage line `--help`
shows, because both read one table, so the two cannot drift apart.
`wolf run prog.lu --help` still belongs to the program: everything after
the entry file is the program's argv.

`cargo xtask dist` stages a man page and shell completions into the
archive the formula and the PKGBUILD unpack (`wolf.1`, `wolf.bash`,
`_wolf`, `wolf.fish`). The binary that just built emits them from the
same table the help reads, so a packager has something to install and it
describes the verbs that exist.

The compile-failure footer names the code the reader was shown (#249).
It said `wolf --explain E0201` whatever error had been reported, so a
package that failed with E0302 sent its author to a different error's
entry, on the second command a newcomer types.

A `use std.…` that misses says that no standard library is configured,
that the standard library is a separate release (wolf-std), and that
`--std-root <dir>` or `WOLF_STD` points wolf at a checkout (#251).
Reaching that miss is itself the evidence that no root is set. With a
root configured the note stays silent, and a test asserts that silence,
because the sentence would be false.

The README's links are absolute (#252). `dist` stages that file and the
packages install it to `share/doc/wolf/README.md`, where nine
repo-relative links resolved to nothing. A gauntlet check reads the
shipped file, and a second check asserts the README still links
somewhere, so an empty scan cannot pass the first one.

### The schedule is named (s139 — #246 closes)

`spec/07-schedule-points.md` is normative for the `sched` namespace, and
that namespace is registered under `[conf.anchor.ns]`. Its seven anchors
(`sched.engine`, `sched.ev1`, `sched.flags`, `sched.point.hook`,
`sched.point.set`, `sched.seed`, `sched.stable`) are published in
`spec/anchors.json`, taking it from 424 to 431. The change is additive:
nothing was renumbered or dropped, because none of the seven had ever
been published for `[conf.anchor.stable]` to pin.

Before this the document declared those anchors and no registry carried
them, so `[conf.tag.valid]` rejected every citation of them while the
scheduler implemented the clauses: `wolf_rt::task::det` carries
`[sched.seed]`'s packed seed and `[sched.stable]`'s append rule,
`::task::hooks` the kind table, `::reactor` and `::net` the
`io.arrive`/`timer.fire` appends. `corpus/test/conc_schedules_test.lu`
cites `sched.stable` in a `conforms:` tag, the first citation the
admission makes legal.

The cause was four hand-copied namespace lists that all omitted document
07. They are one `NS_OWNERS` table now, and the admission gate reads both
directions: a registered namespace must publish anchors, and a spec
document may not declare a namespace nobody registered. The second guard
scans `spec/` on disk, because the defect was a document that nothing in
the tooling listed.

## 0.2.6 — 2026-09-06

THE ACCEPT IS FAIR. A hand that loses an accept race now costs you a
budget instead of a process. Start several hands on one listening
socket (the prefork shape 0.2.5 made writable, whether they share the
address with `reuse_port` or inherit one listener from a parent) and
each arriving connection wakes more than one of them while exactly one
takes it. Until now the losers then called a *blocking* `accept(2)`
with their deadline already spent, and parked in the kernel until the
next connection arrived: microseconds on a busy server, and on a quiet
one, never. Alive at 0.0% CPU, answering no control verb, reaped by
their master as dead. Now a loser comes back inside the budget its
`net_deadline` armed when the call began, never a fresh one, so the
most a lost race can cost a serving loop is that budget, and N
processes may share one listener without one of them going silent.
Nothing in `net_accept`'s signature or its rows moved, so a program
written against 0.2.5 runs unchanged, and the shape 0.2.5 invited you
to write now survives an idle Sunday.

### A lost race is not an answer, because it carries none (s138 — #242 closes)

A listener lives non-blocking under the runtime now. The take after
a wake finds the connection or finds nothing, and a hand that finds
nothing waits again against the budget its `net_deadline` already armed.
With a budget armed, `net_accept` returns within it; without one it
parks in the runtime's wait, where kill teardown reaches it, and not in
the kernel's, where nothing does.

Nothing new appears in the rows. From the program's side a lost race is
indistinguishable from the connection never having arrived, the same
thing a single hand sees when a peer aborts between the wake and the
take. So it is not a new row (a `net_accept(l)?` with no deadline
armed would then fail on a stranger's reset, and a `timeout` before any
deadline would lie about a clock), not an empty handle, and never a
trap. A loop that multiplexes a shared listener with `net_wait` and must
keep serving its other sockets arms a short budget on the listener; that
budget is the most a lost race can cost it. The accepted stream is
blocking whatever the listener's mode (BSD kernels hand the flag down,
linux does not, and the runtime makes it one posture), so reads and
writes are as before.

Measured on macOS arm64, three hands free-for-all on one inherited
listener, `ab -n 6000 -c 32`, one connection per request: about 30,000
req/s before and after (29.7–30.8k parking, 29.7–33.1k fixed; one hand
serves 23–28k, six serve 25–30k, the loopback client being the ceiling).
Under load the fix costs nothing and buys nothing, because the next
connection unparks every loser microseconds later. It buys the quiet
server, and it deletes a workaround downstream: the ten-millisecond
accept turn lobo shipped to avoid the park cost it 11,622 req/s against
17,347, and comes out now.

Spec: `[os.net.accept]` (new), with a sentence each in `[os.net.wait]`
(readiness is not exclusivity) and `[os.proc.inherit]`. Witness:
`corpus/net/accept_race.lu`, two hands on one inherited listener, one
connection, both hands return; before this fix it reports
`loser_returned false` in twelve seconds instead of hanging. Windows
and the checked machine refuse the inherit set, so the witness is
vacuous there.

### The blast-radius bound holds (s138 — #243 closes)

A keyword where a parameter name goes is one report, not two.
`fn f(true, x: int)` said "`true` is a reserved keyword, so it cannot
name a parameter" and then, on the same token, "expected `:` and a type
after the parameter name". The second line contradicted the first and
told you nothing the first had not; now the type is asked for only when
a `:` says you meant a parameter there (`fn f(true: int)` is still one
report, the name), and a real name with no type keeps its report
(`fn f(x)` still asks for one).

That is all of #243. The nightly's blast-radius sweep (every corpus
file, three hundred single-token mutations each, at most five cascade
diagnostics for a mutation that re-keys structure) had been red since
the s137 corpus adds: turning the `=` of `let a =
net_listen_with("127.0.0.1:0", true, 16) else |e| match e {` into `fn`
drew six. Reduced to the smallest program that draws six and read one
by one, five of them are one per enclosing tier (the binding, the run
of non-parameters, the parameter, the phantom header's end, the arm
list read as its body) and the sixth was the parameter reported twice.
The recovery is fixed; the bound and its rationale do not move. The
counter-example is pinned deterministically beside #20's and #109's.

### The letters

`[conf.anchor.ns]` admits the namespaces it publishes (#239). The
clause that registers anchor namespaces named seven of them while the
spec published anchors in eleven, and had done since `diag` (s67), `ct`
(s112), `type` (s113) and `os` (s114) each went normative without an
append. At this release that was 71 published anchors outside the
clause's own letter, every one of them a clause `[conf.tag.valid]`
made a CI failure to cite. Inside this repository the gap was invisible,
because the extractor is the permissive side and admitted all eleven; it
only ever announced itself downstream, in a rig that mirrors the letter
and therefore *rejected* tags that ought to be legal: wolf-std's F-0099,
where six witnesses for `[os.net.unix]` could not name the clause they
conform to. The clause caught up, and not one anchor moved. Moving them
was never the option: `[conf.anchor.stable]` forbids renumbering a
published anchor, and the citations that would have had to move number
3,273 across nine repositories against the one paragraph an append
costs. The append is additive on #120's precedent, with the reason
written into the clause instead of a commit message, and a new
`[conf.anchor.ns.admit]` says how the next namespace is admitted: in one
change, clause and tooling together, on every implementation track. Three
tests in `cargo xtask ci` now hold the two halves equal, so the fifth
recurrence fails a gauntlet instead of a downstream rig. Anchors 423 →
424.

The pairing holds at lupin 0.1.26 (pin `982f857`), unchanged from
v0.2.5, because the interpreter published no release between the two
tags, and the first pairing in this series that needed no re-stamp at
all. Re-run at this head against the v0.1.26 release build (bare, no
`LUPIN=` override; D57's second half applies, so version and pin were
both compared and both matched). The ritual over 508 files: checked
266 agreements / 124 completeness notes / 2 soundness / 111
unsupported / 8 hard, native 287 / 124 / 0 / 93 / 5. Hard
divergence is flat against v0.2.5's 8 and 5, and decomposes with nothing
left over: six (five) `Diag` rows are #167's warning-channel asymmetry,
lupin emitting no warnings at all, and the two soundness findings are
#168's float-cast twins, where wolfc's own checked lane exits 0 and the
native lanes trap. Every surviving row is a filed issue older than this
release, and no class opened at this pin. The one corpus file this
release adds, `corpus/net/accept_race.lu`, is unsupported on both lanes
(the interpreter is handed no listener to inherit), which is where the
507 → 508 and the two `unsupported` counts moved.

## 0.2.5 — 2026-09-04

THE SERVER HAS CORES. A loop that waits costs about 37x less than a
loop that looks. Hold one connection open and idle for five seconds,
and v0.2.4 gave a single-threaded server no way to spend less than
50.2 M cycles and 1,378 involuntary context switches doing nothing
at all. With no call that waits on more than one socket, such a server
had to time-slice: a short deadline on the listener, another on every
open connection, around and around, waking up to learn that nothing had
happened. `net_wait(fds, deadline_ms)` asks the kernel about the whole
set at once and sleeps until a socket actually speaks. The same five
idle seconds now cost 1.3 M cycles and 5 switches, both figures net
of a program that only sleeps, which is the floor either way. That is
the difference between a server that burns a core to sit still and one
that costs nothing to sit still, and if you are writing a serving loop
in wolf it is the one call to reach for first.

The other half of a server is the other cores. Several processes can
hold one listening address now, with `net_listen_with(addr, reuse_port,
backlog)`, or a parent can open the listener and hand it down, with
`os_spawn_with(exe, args, inherit)` passing real descriptors and
`net_adopt_listener(fd)` picking one up on the other side. `os_cpus()`
says how many hands to start: the number of cores this process may
actually be scheduled on, which inside a container is the quota, not the
hardware. A prefork server is a program you can write in wolf today.

### A listener that hands can share (s137 — #234 and #235 close)

`net_listen_with` is `net_listen` with the two options a server needs.
`reuse_port` sets `SO_REUSEPORT` before the bind, the only place it
works, so several listeners can hold one address at once, in one
process or across many, and the old hand can leave without the port ever
going free. `backlog` is the `listen(2)` queue hint (`<= 0` asks for the
default). The handle it answers is the ordinary listener every other net
call already serves, so nothing else in the surface grew for it. A held
address answers `exists` now, instead of a bare `io`: the "already in
use" answer a server has to tell apart from everything else.

What the kernel then does with a group is the host's, and the clause
states it. All three answers were measured on the runner by the
runtime's own tests instead of quoted from a manual, and two of them
overturned the prediction:

- linux distributes accepts across the group: the prefork shape you
  expect, N processes bound to one port and N of them getting work.
- macOS hands every SYN to the newest bound socket, and falls to a
  survivor when that one closes. So `reuse_port` there buys a handover
  with no dropped connections, not a fan-out. For many hands on macOS,
  inherit the listener: one queue, every child accepting from it, the
  kernel doing the distributing.
- windows refuses `reuse_port`, and refuses an inherit set.
  `SO_REUSEADDR` there means "any process may take this port", so
  aliasing to it would hand you a hijack where you asked for a shared
  queue; and a `SOCKET` is not a small number a child can be handed by
  position.

`os_spawn_with` hands a child real descriptors, and the child receives
them as 3, 4, … in the order given. That numbering is the contract, so a
parent never learns a descriptor number and a child never has to be
told one: "the listener is 3" is true by position. The adopter asks the
kernel what the number is instead of trusting it (a stream socket, with
no peer, listening where the kernel keeps that flag), so a pipe, a
connected stream or a stranger's number is `io`, never a trap. It owns
the descriptor from then on (`net_close` closes it, a second adopt of a
live number is refused) but it owns nothing outside it: an adopted unix
listener does not unlink the socket file its parent bound. One spawn
hands at most 64. The checked machine refuses both with the construct
named, because a child of the program being interpreted would be a
child of the compiler.

Spec: `[os.net.listen.opts]`, `[os.proc.inherit]`. Witnesses:
`corpus/net/reuse_port.lu`, `corpus/net/inherit_listener.lu`. Per-host
detail in [`docs/platforms.md`](docs/platforms.md).

### The wait behind that number (s137 — #127 closes)

`net_wait(fds, deadline_ms)` answers which of a set of handles can be
read without blocking (a listener whose `net_accept` will not block, a
stream whose `net_read` will answer bytes or `closed`) as the subset,
in the order you gave. Every other blocking call in the family waits on
one socket; this is the one that waits on many, and it is what a program
that spawns no tasks has instead of a scheduler.

Measured on an idle loop holding one connection for five seconds (macOS
arm64, the shape lobo's ws16 bench runs):

| | loop passes | involuntary context switches | cycles |
|---|---|---|---|
| deadline time-slicing (25 ms + 12 ms) | 125 | 1,385 | 57.1 M |
| one `net_wait` on the whole set | 1 | 12 | 8.2 M |
| a program that only sleeps (the floor) | — | 7 | 6.9 M |

Net of the floor: 50.2 M cycles of idle work against 1.3 M, and 1,378
involuntary context switches against 5. The loop wakes when a socket
speaks, not when a timer says to look.

An empty answer is the deadline expiring with nothing ready, which is an
answer and not a failure. Readiness is level-triggered, so asking
consumes nothing and two waits answer the same until you actually read
or accept; a stream whose peer has gone is ready, and the read that
follows reports `closed`. `io` is a forged handle, or a wait nothing
could ever end. The call neither parks a task nor disturbs the
reactor's registrations, so it mixes freely with `spawn`.

Alone in this release it names no host: `poll(2)` on linux, macOS and
freebsd, `WSAPoll` on windows, and the checked machine mirrors it call
for call. Spec: `[os.net.wait]`. Witness:
`corpus/net/wait_readiness.lu`.

### `os_cpus()` (s137 — #233 closes)

A program can ask how big the machine is. `os_cpus()` answers the
count of cores this process may actually be scheduled on, always `>= 1`
when it answers.

The count is of *schedulable* cores, not *installed* ones. On linux the
number honours a cgroup cpu quota and a cpu affinity mask, so a
container given two cpus of quota on a sixty-four-core host answers 2,
where counting `processor` rows in `/proc/cpuinfo`, which is what a
program had to do before this existed, answers 64 and starts sixty-two
workers that will never get a core between them. macOS reads `hw.ncpu`,
windows `GetSystemInfo`; every tier reads the same source, so `wolf run`
and `wolf run --checked` agree on the number on one host.

A host that cannot answer gives the `io` row, and not a 1: a program
resolving `workers auto` can say it did not learn the number instead of
quietly running one worker and looking healthy. Spec: `[os.cpus]`.
Witness: `corpus/os/cpus.lu`.

### Four fixes

- A diagnostic on a long line killed the compiler (#238). When one
  diagnostic pointed twice at the same source line and the two places
  were far apart, the human renderer windowed the line around the first
  underline and then sized the second one `hi - lo` with `hi` left of
  `lo`, a `usize` underflow, so `str::repeat` asked for near-`usize::MAX`
  bytes and the process aborted. In `conform-run` that is worse than an
  ugly message: the record is written after the report, so the run
  exited 101 with an empty stdout, and every consumer that greps for
  `error[` read a clean file. wolf-std's sc35 lost a refusing row past
  two full-tree scans that way. Both ends of an underline clamp into the
  window now; a row past the cut marks the cut and keeps its label.
- `channel[bool]` compiled to a crash. A `bool` payload crossing a
  channel's one-word wire lowered to `zext.i64`, which the WIR verifier
  rejected as "integer conversion needs integer types": an internal
  compiler error on any native build. `zext` from `bool` is the bool→int
  bridge now and verifies; `sext` from `bool` still does not, because
  sign-extending a truth value means nothing.
- The runtime's own test pipes could leak into a child. On hosts
  without `pipe2` (macOS), the reactor's test pipe was not close-on-exec,
  so a descriptor nobody handed over could turn up in a spawned child,
  which matters rather more in a release where `os_spawn_with` hands
  descriptors over on purpose. It matches the posture every other
  descriptor the runtime opens already had.
- `net_wait` builds on a tier-2 host. Its readiness call reached
  through `crate::reactor`, which is gated to the three hosts with a
  poller written for them, so a target that merely `cargo check`s the
  workspace lost the module. It lives in `crate::poll` now,
  `cfg(any(unix, windows))`: the reactor owns the edge-consuming set the
  task layer parks on, and this is the level-triggered question a
  spawn-free loop asks about a borrowed set.

### The letters

The release published itself, again (#226). v0.2.4 was the first tag
where a fully-green four-host `dist` matrix moved its own release out of
draft, with the final `publish` job counting the archives on the page
before it touched the flag. If you are reading this on a published
release page that no one edited by hand, that is the mechanism holding
for a second tag.

The corpus cannot hold a long line, which is why #238 lived so long.
The renderer bug above needs a source line wider than the renderer's own
100-column window. `cargo xtask corpus` conform-runs every entry through
the human reporter, so the walk would have caught it on the first pass,
if any corpus file could carry the shape. None can, and none ever will:
`cargo xtask fmt-lu` holds every corpus file canonical under `wolf fmt`,
and the formatter reflows every over-width line it can break. Measured
while looking for a home for the witness: an `if`, a `match`, a call, an
operator chain, a nested type and a long string literal, six shapes, all
six reflowed. What survives formatting is the unbreakable single token,
which is the long f-string the ledger witnesses print, and that carries
one annotation, so the window centres on it and nothing lands past the
cut. Two of this repository's gates are in tension here, and the
resolution is that a witness needing an unformattable source writes its
own fixture: #238's lives in
`crates/wolf_driver/tests/long_line_record.rs`, which drives the binary
on both run-reaching lanes. Anyone reaching for `corpus/` to pin a
rendering shape should read this paragraph first.

The pairing moved to lupin 0.1.26 (pin `982f857`), and this time the
differential has no dark corner. v0.2.4 shipped paired with lupin 0.1.24
and named two classes of file the interpreter could not yet read: nine
that mint a `byte`, and one byte order mark whose single refusal named
itself in twenty more rows of `corpus/grammar/`, because lupin loads a
file's directory siblings as modules. Both retired at the interpreter's
next two releases, as that entry said they would. The ritual over
507 files at this pin: checked 266 agreements / 124 completeness notes
/ 2 soundness / 8 hard, native 287 / 124 / 0 / 5. Hard divergence is
down from 38 and 33, and the counts decompose with nothing left over. The six
(five) `Diag` rows are #167's warning-channel asymmetry, lupin emitting no
warnings at all; the two soundness findings are #168's float-cast twins,
where wolfc's own checked lane exits 0 and the native lanes trap. Every
surviving row is a filed issue older than this release, and no class
opened here.

## 0.2.4 — 2026-09-03

THE BYTE SHIPS. A 64 KiB read charges what it holds. wolf has a
byte-width scalar now (`byte`, an 8-bit unsigned octet, `0..=255`, one
byte of storage) and every builtin that produces or takes raw octets
speaks `List[byte]`. So `fs.read_bytes` of a 64 KiB file charges its
region 65,584 bytes natively and 65,536 on the checked machine,
where v0.2.3 charged 1,048,560 for the same octets: eight times the
width and twice again for the growth history a push-grown list carries.
Sixteen times the payload, down to one. Read a file, walk a string's
bytes, echo a socket, and the number in the ledger is now the number of
octets you actually have.

This is a breaking change, and it is one line of fix per site. The
elements of `str.bytes()`, and of `fs_read_bytes`, `fs_read_chunk`,
`net_read_bytes` and `str_from_utf8`'s input, are `byte`, not `int`.
A `byte` never adopts a numeric literal and never quietly widens: `b ==
10` is E0401, so is a literal `match` arm, so is `let n: int = b`, and
so is binding a reader's result as `List[int]`. Every note names the
spelling. Arithmetic and comparisons against integers want `as int`
(`b as int == 10`, `(b as int) + 1`); a value going back into a byte
list wants `as byte` (`out.push(n as byte)`, which truncates to the low
eight bits and never traps). A width-bearing scalar infers nothing
behind your back, and the compiler counts the sites for you: 33 in 11
corpus files here, 87 in 13 of wolf-book's, 198 in 45 of wolf-std's,
217 in 30 of lobo's. Comparisons against int literals are the majority
everywhere.

wolf speaks unix-domain sockets too. `net_listen_unix(path)` /
`net_connect_unix(path)` on linux and macOS, in the TCP pair's own
shape: the fd is an ordinary net stream, so `accept`, `read`/`write`,
the byte pair, `net_deadline` and `net_close` all serve it call for
call. Windows answers `unsupported` instead of pretending.
[`docs/platforms.md`](docs/platforms.md) is the per-host ledger.

It pairs with lupin 0.1.24 at pin `3befc3e` (D57: the pin is part of
this release's identity).

### The producers speak bytes (s136 — #231 closes)

`byte` arrived in the language at s135 and nothing produced one. All
eight octet builtins (`str.bytes()`, `str_from_utf8`, `fs_read_bytes`,
`fs_write_bytes`, `fs_read_chunk`, `fs_write_chunk`, `net_read_bytes`,
`net_write_bytes`) were declared over `List[int]`, which made the type
worse than useless downstream: wolf-std measured a reader substituted to
`List[byte]` charging 17x/18x its payload where the `List[int]`
reader it replaced charged 16x, because the substitute had to convert
elementwise against a builtin and `[mem.region.account.1]` keeps the
list it converted from charged for the region's life. The sc34 sprint
refused the substitution with that table instead of shipping the
regression, and this release is the answer.

The eight speak `List[byte]` on every tier. The native runtime mints a
byte read as one 1-byte-element buffer at exact capacity, so a producer
that knows its length has no growth history to pay. That is why the
64 KiB read costs 65,584 (one list header plus the payload) and not the
131,120 a push-grown `List[byte]` would, and the checked machine's byte
slots charge the payload exactly.
`corpus/memory/byte_producers_ledger.lu` pins the relations on every
tier (a read holds its payload; at most the payload plus a header; the
same octets as `List[int]` charge at least seven times more);
`strings/bytes_roundtrip.lu` is the round trip and `net/echo_bytes.lu`
the echo. The refusal for a `byte` used as an `int` is E0401 with the
spelling in the note (`typecheck/byte_elem_arith_fail.lu`). The
`invalid` row stays declared on the two writes: the vocabulary is
stable, and a byte element is always in range, so typed code can no
longer reach it. `os_random` keeps its `List[int]`, because
`[os.random]` says so and it is not a byte surface.

### The phantom 16x (s136 — #232 closes)

The checked machine charged 1,048,576 ledger bytes for `for b in
s.bytes()` over a 64 KiB `str`, a walk that allocates nothing and that
the native tier and lupin both charge 0 for, because it evaluated
the consumed view as the materializing fallback. The consumed positions
(`for b in s.bytes()`, `s.bytes()[i]`, `s.bytes().len` and the query
family) are the receiver's own storage on that tier too now, charged to
no region. `let bs = s.bytes()` still materializes and still charges;
that is the difference the syntax was always making.
`corpus/memory/consumed_walk_charges_nothing.lu` prints the relation on
every tier.

### Unix-domain sockets (s136 — #227 closes)

`[os.net.unix]`, the spec's first socket clause. `AF_UNIX` stream
sockets in the TCP pair's shape, and the error rows tell a host apart
from a path: `unsupported` is the host, and never a bare `io`;
`exists` is a stale socket file at bind (the operator's to remove; the
runtime never clobbers a path it did not bind), `not_found` a missing
directory or file, `denied`, and `refused` a file nobody listens on.
The runtime created the socket file, so `net_close` of the listener
unlinks it. Windows answers `unsupported`: `AF_UNIX` has existed there
since 10 1803 and the test suite measures the kernel's answer on the
runner, but `std::net` has no unix-domain surface on that host and the
runtime carries no winsock binding beyond `WSAPoll`, so
`docs/platforms.md` names the rung instead of the runtime claiming it.
`corpus/net/unix_echo.lu` is the three-lane witness, and it prints the
same stdout on every host either way: an echo where the family is
served, a named refusal where it is not. lobo's control endpoint gains
its second transport at ws16.

### The string-layout codes (s136 — D74, #230 closes)

One rule per code, which is what the catalog and the spec had stopped
agreeing about. E0103 is the delimiter rule: a `"""` shares its
line with text, opening or closing; delimiters stand alone. E0104 is
the margin rule only (a content line left of the closing delimiter's
column); the closing-delimiter case used to answer E0104 too, and the
spec sentence and the catalog disagreed about which meaning it had.
E0105 is the tab/space rule, unchanged. E0102 owns the bare `{`
in a plain string: `"hello {world"` is a string whose interpolation
never closes before its line ends, and wolfc answered E0109
("unterminated generalized literal", because the `world"` inside the
open interpolation happened to spell one), which is the wrong family;
lupin was right. And a byte order mark at the very start of a file is
stripped: tolerated, never a diagnostic, and the formatter keeps it
in place; anywhere else it is E0107. Six corpus witnesses under
`grammar/` pin the codes and spans for both machines.

### One path spelling (#222 closes)

`wolf add` and `wolf publish` printed their status paths with the host's
separator (`app\wolf.pkg` on windows) while every diagnostic path in the
same binary is slash-normalized (`--> app/main.lu:3:5`). Every path a
package verb prints (`add`, `rm`, `init`, `vendor`, `publish`, and
their error lines) goes through the diagnostics' own spelling now:
forward slashes on every host. Found by wolf-book's samples lane the
week it was first lit.

### The deadline and the closed peer (#224 closes)

`wolf conform-run --checked` killed a connected client at `net_deadline`
after the peer closed. The cause is not handle lifetime: lobo's serve
sequence hands the parsed head to `serve_request` and never drains the
socket, so the server's close is a close over unread receive data, an
RST close, and on macOS `setsockopt(SO_RCVTIMEO)` answers EINVAL on a
reset socket (linux keeps answering 0) while the reply the server wrote
is still readable. The checked machine implements deadlines with that
setsockopt pair and reported the EINVAL as an `io` row. The deadline now
arms on a reset socket (the native reactor's timer wheel never asked the
kernel and always did), the buffered reply is read, and the reset is the
ordinary `closed` row after it. `net/peer_close_after_serve.lu` pins it
on every tier; the checked twin lives in `crates/wolf_mem/tests/net.rs`.

### The letters

The release publishes itself (#226). This page has been created as a
draft since v0.2.0, which is right, since no half-uploaded release
should be public. But the un-draft was a hand step, and it is the step
that left v0.2.0 and v0.2.1 unpublished for a day (#200) and v0.2.3
sitting behind a complete, green matrix. `release.yml` gains a final
`publish` job that `needs:` the whole four-host `dist` matrix: it runs
only when every archive uploaded, it lists the assets into its own log
before it moves the flag, and it refuses (leaving the draft alone) if
fewer than four archives are on the page. A fully-green matrix
publishes itself; a red one stays a draft.

The pairing moved to lupin 0.1.24 (pin `3befc3e`), and this is the one
release where the differential is behind. The reference interpreter
released `byte` in the mirror at is35, but the type landed
at s135 and its producers at s136, both after that tag, so lupin 0.1.24
refuses `as byte` at resolve and nine corpus files go dark on it:
`fs/bytes_dirs`, `memory/byte_list_ledger`, `net/byte_roundtrip`,
`net/echo_bytes`, `net/line_reader_bytes`, `strings/byte_view_lend`,
`strings/from_utf8_border`, `typecheck/byte_casts`,
`typecheck/byte_shapes`. D74's byte order mark costs one more:
`corpus/grammar/bom_at_start.lu` opens with `ef bb bf`, wolfc strips it
and lupin 0.1.24 refuses it, and because the interpreter loads a file's
directory siblings as modules, that single refusal names itself in
twenty other rows of `corpus/grammar/`. The ritual run over 503 files:
checked 238 agreements / 2 soundness / 38 hard, native 261 / 0 / 33,
and the hard counts decompose with nothing left over: 6 and 5 are
#167's warning-channel asymmetry, 2 are #168's float-cast twins, 21 and
19 are that one BOM, 9 and 9 are the byte flip set. Two named classes,
both retiring at the interpreter's next release.

## 0.2.3 — 2026-09-02

THE ARCHIVE RETURNS. On Windows, `spawn` and scopes, `proc`, channels
and `select`, `sync`/`when`, `os.signal` and `net` deadlines now compile
and run. v0.2.2 was the first archive that built and ran your program
on that host, and it refused twenty-one corpus rows by construct name:
"windows-native serves no `spawn`/scopes (the task layer) at the s60a
bring-up". That table is retired. The windows native lane measures at
macOS parity now, 261/278/0/295/0 (checked/native/release/union/
all-three), with zero rows refused by construct name, and a stack
overflow inside a task reports in wolf's own voice,
`wolf-rt: stack overflow in task '<name>'`, exit 134, where v0.2.2
died as `0xC00000FD` with no words at all. Two things this host still
does not serve, and says so: `wolf build --release` (the LLVM tier is
s60c's), and external `reload`/`upgrade` signal delivery, which has no
Windows analog. A program's own reload path works, and wws-shaped
programs use a control channel for the rest.

linux aarch64 has its archive back. v0.2.2's release page
carried three archives, not four: the new learner-path smoke gated the
upload on a native tier that host does not serve, so the arm archive was
built and then thrown away (#213). The smoke reads the driver's
exit-code contract now, treating an exit-2 environment refusal as an
unserved host and not a broken archive, printing the refusal a learner
would see and then proving `wolf test corpus/hello.lu` runs from the
unpacked archive on the checked tier, which is how that host has served
learners since v0.2.1. Four archives at this tag. And the release page
carries this paragraph instead of a bare asset list, which is the third
letter below.

Underneath (s60b): workers are Win32 threads on the kernel's own
reserve-and-guard stacks, `CreateThread` with
`STACK_SIZE_PARAM_IS_A_RESERVATION` at `WOLF_TASK_STACK`, 8 MiB by
default: `VirtualAlloc(MEM_RESERVE)` plus the `PAGE_GUARD` page ntdll
walks down on first touch, done by the kernel instead of by hand. The
span is not ours to pool (Windows offers no thread on memory the runtime
mapped), so a worker keeps its stack for life, and idle trim is named
for s60c. `os.signal` rides `SetConsoleCtrlHandler`: Ctrl+C and the
console closing are `terminate`, Ctrl+Break is `quit`, the
`[os.signal.platform]` table, with `os_signal_raise` an in-process
loopback, because no self-targeted console event exists that would not
also reach every other process on the console. The reactor behind the
s35 interface is `WSAPoll`: `net` deadlines fire the `timeout` row,
accept/read/write park with the pool compensating, kill teardown reaches
them. Measured on the runner: 24 tasks parked on 200 ms read deadlines
resolved 24/24 `timeout` in 207 ms wall, so the deadlines fire at the
deadline and not at the poll's cost. IOCP (completion ports, the
async-fs and many-socket rung) is s60c's, behind the same seam.
[`docs/platforms.md`](docs/platforms.md) is the per-host ledger.

It pairs with lupin 0.1.23 at pin `8cda3aa` (D57: the pin is part of
this release's identity).

### The server annotates (s134)

`wolf lsp` serves `textDocument/signatureHelp`,
`textDocument/semanticTokens/full` and `/range`, and
`textDocument/inlayHint`, the three rungs s133's closeout named as
the binding table read three more ways. Nothing is a textual search.
Signature help reads the checker's call record for the innermost
call whose argument list holds the cursor (`TypedBody::calls`, keyed
by the call expression's span): the declared parameters as
`name: type` with a declared `mut`/`take` spelled, the receiver
omitted because the parentheses never spell it, the active parameter
counted by the commas before the cursor, the return type when the
callee is a declared item, and its `///` comment (the one doc model
hover and `wolf doc` read; markdown when the client lists it, plain
text otherwise); triggers on `(`, re-triggers on `,`. Semantic
tokens classify every identifier through what it bound to
(`Resolution::refs`, then `TypedBody::member_refs`): `parameter` when
the binder sits in a parameter list, `variable` otherwise (`readonly`
unless the binder is `var`'s), `function` / `type` / `variable` by an
item's kind, `namespace` for modules and std paths, `property` /
`enumMember` / `function` for fields, variants and methods, `keyword`
from the token kind, `type` for builtins and `Self`; a binder's own
token carries `declaration`; a name the compiler never bound gets no
token. The legend is closed and fixed: eight types in one order, two
modifiers. Inlay hints are the inferred type of an unascribed
`let`/`var` binder and the parameter name before a positional
argument that is not already that name, and only at calls the checker
resolved to a declaration, so a fn-typed value and a prelude name
offer none; each class switches off through
`initializationOptions.inlayHints.{types, parameterNames}` and the
client's own toggle decides whether hints show at all. Positions
honor the negotiated encoding at every span (an astral character on
the line before a token moves its UTF-16 column, not its byte). No
delta tokens: a full answer is cheap here and a delta is a promise
about identity across edits this server has no reason to make; it
answers `-32601` like every other absence. `wolf_query`'s
contract moves to v5 (additive). Eighteen transcripts were recorded
in wolf-lsp against this build (one script per rung per maintained
client profile, fackr, facsimile, nvim, vscode, helix and emacs, the
answers differing by the profile's own declarations), the forty-seven
existing ones re-recorded with the initialize answer as their only
diff, and the unknown-method probe re-targeted at what is still
absent. Latency, s57's table before and after on the same machine:
`diagnostics-after-edit` p95 110.6 → 110.8 ms (p50 107.7 → 106.6),
hover p95 0.2 → 0.1 ms, cold first diagnostics p95 4.1 → 4.4 ms. Every
class is inside its budget and the number near perception is unmoved.

### The proc leaves its module (s134 — #219 closes)

A `spawn proc` in a non-entry module now builds on the release
tier under every partition. lobo ws13 measured the gap while
adopting the region cap: its budget helper spawns a proc from a leaf
module, and `wolf build --release` answered “cannot compile this yet —
func.addr of `@work.run.task0.entry` outside this object's subset”
while `wolf run` executed the same program: #136's proc twin, one
partition over. s117's `refs=` edge keeps a spawner and its entry shim
in one cluster; the per-module partition (`WOLF_MIDEND=0`, the
measurement mode lobo's gauntlet runs in while #146 is open) never
consulted it, because the shim is synthetic, has no source file, and
rode the root module's object while the spawner sat in its own. The
debug tier had imported such a symbol across objects since #116; the
LLVM tier refused. The LLVM emitter now takes an out-of-subset
referee's address through the same mangled-symbol declaration a
cross-object call uses, a link-time constant in every object, resolved
by the linker. Only a name no module function carries is a refusal now.
Witnesses:
`corpus/conc/proc_cross_module` (ws13's thirty-line reproducer, three
lanes plus lupin, `normal=0 breach=2`) and a driver test that pins the
per-module partition itself, because a thirty-line program is one
cluster under the whole-program phase and the refusal cannot fire
there. lobo's `ws13-cap` branch builds `--release` at this commit.

A refusal names itself in the record. `wolf conform-run --json`
answered `{"verdict":"unsupported","diagnostics":[]}` on every checked
proc spawn: named on stderr, and nothing in the record a rig reads
over a pipe. The record now carries `x-unsupported-construct` and,
when the refusal has one, `x-unsupported-span` (`[proto.record.ext]`
extension keys, so they take no part in comparison and the
counterparty need not emit them) on every `unsupported` verdict at
every rung: typecheck, mem, wir, the checked machine, and the native
and release lanes. `diagnostics` stays empty, because an `unsupported`
verdict
carries no partial diagnostics and a refusal is not a fault in the
program, so it has no E-code. `proc_cap_fault_join.lu`'s checked
verdict is unchanged, `unsupported` at `mem`, now reading
`"x-unsupported-construct": "structured concurrency in checked
execution (C1 deferred)"` with the span of the spawn, because the
checked machine (`conform-run --checked`, the s23 UB machine) runs no
structured concurrency at all: `spawn`, scopes, `select` and `when`
are refused at the expression. A proc is refused where every spawn is;
running one there is the C1 sprint, not a fix. The other `--checked`,
meaning `wolf run --checked` and `wolf build --checked`, is the native
build under the checked profile (the quarantine allocator and the
checked-tier runtime hooks), which is why ws13 saw the same file run
there and refuse under `conform-run`; the two flags name two machines,
and the record now says which one declined.

### The span is the offending token (s134 — D71, #220 closes)

A parse refusal about a token now spans that token. is34's first
full three-lane diff-run found DIV-2026-020: on eight grammar
witnesses wolfc's E0201 was a zero-width point at the offending
token's start while lupin spanned the token, the same byte at a
different width, invisible to every walk that compares codes and
visible to every editor, which highlights nothing at a zero-width
range. D71 ruled the width: "expected `}`, found identifier `y`"
points at `y`, every byte of it. The parser's one primitive for a
refusal about the current token (`here()`) now answers the token's own
span, so E0201 and its siblings (E0203's "expected a struct name",
E0206's "expected a type", E0207's "expected a pattern", the missing
`=`/initializer reports) all moved together; a refusal at end of file
still lands on the zero-width `Eof` marker, and a suggestion's edit
keeps its own zero-width anchor (an insertion point is zero-width; the
primary span and the fix's span were always different things). Seven of
the eight rows are now byte-identical to lupin's: `[550,551)` the `y`,
`[534,535)`, `[415,416)`, `[581,582)`, `[669,671)` the `..`,
`[332,333)`, `[896,897)`. The eighth, `let_group_bare_tuple.lu`, was
never a width question: wolfc reads the D63 let-group and refuses at
the end of the initializer list ("this value has no name", now
`[374,375)`) while lupin refuses at the first comma (`[364,365)`), so
it stays a locus divergence for its own triage, as #220 said it
would.

Blast radius, measured before the change: wolf-lang, 37 snapshot files
(35 in `wolf_parse`, the LSP one-truth test's `[28,28]` → `[28,29]`,
and the eight `check: fail(E0201)` corpus pins unchanged, since the
walk compares codes and the directive grammar pins no spans); wolf-book, 5
diagnostic snapshots carrying 7 E0201 renderings whose carets widen
(a read-only count; the book's lane re-records at its pin bump);
wolf-lsp, 0 transcripts (none carries an E0201; the two E0202s are at
the opener and the E0203 in the two smoke transcripts already spanned
its token). `cargo xtask differ` gains a span-width class: two
rejections with the same code at the same start byte that differ only
in width now classify as `SpanWidth`, where until now they were a
`Diag` divergence spelled like a wrong locus, which is why is34 could
only carry them as a waiver (`differ::DIV_2026_020_FILES` in
wolf-interp; it retires at the pin bump that carries this, the
interpreter lane's, as #177's did).

### The letters

One archive per target, not a history (#212). `cargo xtask dist`
runs in every gauntlet, and the archive, the staged tree and the
unpacked smoke tree it writes are all named for the version, so a
version bump renamed them and left the previous set behind forever. r05
measured the cost: five runs' worth plus the fuzz target dir took the
release rig to 0 bytes free mid-release, which deadlocks a tool
harness that spools every output to a file first. `dist` now prunes
every prior artifact for its own target before it writes anything, and
names each removal, so a CI log can be audited. The second consequence
was worse than disk and closes with it: the release workflow uploads
`target/dist/*.tar.gz` by glob, so an archive left over from an older
version would have ridden a tag it did not belong to.

The release body fills itself (#214). `GET
/repos/wolffe-lang/wolf-lang/releases/tags/v0.2.2` answered with a
`body` of length 0, and so did v0.2.1: the workflow created the
release with `--title` and nothing else, so the paragraph written for a
newcomer lived only in this file, and a learner arriving from a download
link never read it. `cargo xtask release-notes <TAG>` cuts the entry for
the tag with fenced blocks transparent (the 0.2.2 entry quotes a
compiler refusal inside a fence, and a naive cut would have ended the
body two paragraphs in), and the release job passes it to `gh release
create --notes-file` and then to `gh release edit`, so the body is right
whether the workflow opened the release or someone else did. A tag with
no entry here fails the job.

The multiline, raw and generalized literals have productions (#215).
`literal` named `MULTILINE_STRING`, `RAW_STRING` and
`GENERALIZED_STRING` on one line and defined none of them; `STR_TEXT`
and `CHAR_TEXT` were cited by the productions above them and defined
nowhere. A reader working from `spec/grammar.ebnf` alone derived nothing
at all for three of the six literal forms, the same class as #198, one
literal over. It bit le05, which wired an `invalid_escape` node for
tree-sitter-wolf and had to decide whether it belongs inside a `"""`
string: the productions could not answer, so the answer had to be read
off a contrast between three prose bullets, two of which say "no
escapes" outright and the third of which says nothing about them. The
lexer was measured and the productions written from it:
`MULTI_PART ::= MULTI_TEXT | STR_ESC | '{{' | '}}' | INTERP`, the same
alternatives `STR_PART` has, because one routine scans both bodies. So
escapes and interpolation work inside a multiline, and a reader can now
derive that from the grammar. `RAW_TEXT` and `GEN_TEXT` derive
scalars and nothing else, which is where "no escapes, no interpolation"
is read from instead of the sentence beside them. Two corpus witnesses
make it measured: the escapes running inside a multiline, and the E0101
refusal of an unknown escape there, the first corpus entry anywhere to
pin one, which is how it turned up #225, a code collision 484
differential entries had never shown: lupin refuses a bad escape under
the code the catalogue spends on a multiline's opening line, in plain
strings and multilines alike.

The pairing moved to lupin 0.1.23. The sibling released while s60b
and s134 ran, which is why both waves carried `LUPIN=` overrides. The
ritual differ run over 484 corpus files found nothing new: checked 257
agreements / 2 soundness / 8 hard, native 278 / 0 / 5, every hard row a
standing named one (#167's warning-channel asymmetry, #168's float-cast
twins). One class is gone: not a single `SpanWidth` row, because
s134's D71 work made those seven E0201 spans byte-identical and this is
the pin bump at which the interpreter retires its waiver.

## 0.2.2 — 2026-09-02

THE LEARNERS' RELEASE. On Windows, this is the first archive that
compiles and runs your program. Unpack it, keep `wolf.exe` and
`wolf_rt.lib` together, and `wolf run hello.lu` produces a real
`hello.exe` and executes it: the native tier, on the host, not an
interpreter. One thing has to be installed beside it, Visual Studio
Build Tools with "Desktop development with C++". The import libraries
every Windows link needs (`kernel32.lib`, `ws2_32.lib`, the UCRT,
`msvcrt.lib`) ship with the Windows SDK and the MSVC toolset, not with
Windows, and this is the same requirement Rust's own `windows-msvc`
toolchain carries; bundling them so no install is needed at all is
s47's, and the refusal that asks for them says so. With them present
wolf finds a linker by itself, trying `WOLF_LINKER`, then `lld-link`,
then rustup's bundled `rust-lld` (a learner with a Rust toolchain
already has one), then MSVC `link.exe`, and `wolf build --verbose`
names the choice.

What the bring-up does not serve, it refuses before the link, in these
words:

```text
wolf build: cannot compile this yet — windows-native serves no
<construct> at the s60a bring-up (the runtime's task layer, io
reactor, and signal delivery are s60b's — the IOCP road);
`<symbol>` would not link (pipeline is honest through `wir`; the
conservatism ledger, not a bug in your program)
```

Twenty-one rows of the corpus take that refusal: the task layer
(`spawn` and scopes, `proc`, channels and `select`, `sync`/`when`,
`region_transfer`), `os.signal`, and `net` deadlines. `wolf build
--release` refuses in the same voice. Everything else serves: `print`,
strings, lists, json, `fs`, `os` (env, cwd, exe, exit, child
processes), `time`, `random`, regions and the region ledger, `net` in
its documented blocking posture, and the C membrane for scalars and
pointers. A trapping program prints `wolf-trap: <kind>` with its
site line and exits 134, the same number as every other native
host. Nothing is a silent stub and nothing is a link error.
[`docs/platforms.md`](docs/platforms.md) is the per-host ledger and
names s60b and s60c as the road.

Underneath (s60a): cranelift emits COFF objects under the MSVC x64
convention, the driver links them against `wolf_rt.lib` (shipped in
every windows archive since v0.2.1) and the import libraries a Rust
staticlib needs, and the C runtime's console entry calls wolf's `main`
shim. Five linker rungs were proven, and one of them turned up a real
`link.exe` bug with wolf's DWARF SECREL relocations. The 134 is not
signal arithmetic here: a trap is a call into the runtime ending in
`ExitProcess(134)` (D70), which is why no vectored handler is needed,
since no sited trap is a fault at any tier. `[abi.c.targets]` gains the
win64 bring-up contract: scalars and pointers direct, aggregates by
value refused by shape until the campaign's `cl.exe` differential.
`cargo xtask lane-coverage` measures windows on its own floor line,
259/255/0/274/0 (checked/native/release/union/all-three), checked at
full parity, native the macOS count minus the 21 refused rows, release
dark because that host's floor says so. `cargo xtask dist` now
unpacks its own archive and builds and runs `corpus/hello.lu` from it
on every host: the learner's path, mechanized, in standing CI.

It pairs with **lupin 0.1.22** at pin `2bfbe5e` (D57: the pin is part
of this release's identity).

### The server navigates (s133 — #208 closes)

`wolf lsp` serves `textDocument/definition`, `textDocument/references`
and `textDocument/rename` (with `prepareRename`), the three rungs s122
named and nobody had climbed, so F12 / Shift+F12 / F2 on a `.lu` file
did nothing in every editor. All three answer from a binding table,
never a textual search: the resolver keeps the decision it already
makes for every name (`Resolution::refs`, uses and binders alike, a
binder a ref to itself, an import's bound name a ref to what it
imports), and the checker keeps the type-dependent half
(`TypedBody::member_refs`: fields through `.`, literal and pattern
fields, variants in value/call/pattern position, methods and associated
fns). The two halves share one key, the declaration's name token span
with its `FileId`, so a cross-file answer is a lookup over the package
graph, not a scan. A name the compiler never bound answers `null`
rather than a guess, and the lexical half keeps answering when typing
stopped.

The wire shapes follow the client's declarations, read once at
`initialize`: `LocationLink[]` with `originSelectionRange` when it
declares `linkSupport`, `Location[]` otherwise; a rename's
`WorkspaceEdit` as `documentChanges` when it declares them, the
`changes` map otherwise. References come back in (file, offset) order,
the declaration only when `includeDeclaration` asks, and the negotiated
position encoding holds at every span (the utf-16 astral case is
pinned). Rename refuses with `RequestFailed` (-32803), naming the
token and the reason and never leaving a partial edit, on keywords
(`self` and `Self` included), builtin types, prelude names, std and
`import c` symbols, modules, and a new name that is not a single identifier;
`prepareRename` refuses with the same reasons before the box opens. The
D59 `//!` member marker carries no identifier, so no rename ever
touches one. `wolf_query`'s contract moves to v4 (additive). Six client
profiles were transcribed against this build. Latency, measured before
and after on the same machine: `diagnostics-after-edit` p95 105.7 →
105.7 ms, so the one number near perception is unchanged, and the three
new requests answer at ≈0.03 ms p95 in-process. Known residue: the
reachable set is the package around the entry (the v0 single-entry
model); a workspace-root model is s57's.

### The region answers, and holds (s131, s132, D68 — #187 closes)

Region accounting became readable and then became a contract. Three
tiers gained `region_bytes(r)`, a named region's byte ledger and the
count `wolf_rt` has kept since s76, now surfaced, and
`live_region_bytes()`, the process-wide live total, with
`[mem.region.account]` pinning what every tier guarantees (zero at
creation, monotone within the lifetime, stable between allocations,
wholesale disappearance at free) and leaving the units as per-tier
measured facts.

Then the cap. A region takes a creation-time byte budget (`region
r(cap: n) { … }`, or `region(cap: n)` on the value form) and a charge
that would take its ledger past the budget is the deterministic trap
`alloc-contract` at the allocating site: the existing kind, no new trap
vocabulary, because a byte budget is an allocation contract. At-cap
exactly is not a breach; the next byte is; `cap: 0` is legal and
everything breaches it. Budgets are denominated in ledger units, not
payload bytes. The clause now says so, after #203 measured a
64 KiB io chunk at ~1 MiB of ledger, so portable programs derive
budgets from measured `region_bytes` readings, which is how the
witnesses pin the boundary on all three tiers at once.

D68's point is the fault half: a breach inside a proc no longer
kills the process. The trap is contained at the proc boundary, so the
proc dies by the killed-proc sequence and no `defer` below the boundary
runs, and `[conc.proc.exit]`'s closed reason set gains the mapping,
`fault(kind)` read at the join as `is_fault()` and
`is_alloc_contract()`. Teardown is free-then-deliver, measured and then
pinned: at the join, `live_region_bytes()` has already returned to its
pre-spawn reading, so the supervisor that answers 503 on this reason
was handed the memory back first. The trapping worker parks, and its
thread and stack high-water are the measured per-breach cost.

### The comma insists everywhere (s131, s132 — D67, D69)

The separating comma is now required wherever the productions always
said it was, family by family and each with its blast radius measured
before the tightening. D67 took the pattern family: `Point { x .. }`,
`Point { x y }` and `(a b)` refuse at E0201 with a machine-applicable
"add the comma" fix (`wolf fix --apply` produces the canonical
spelling), and `..` follows a separator like one more member. D69 took
the rest of the unlicensed laxity: struct literal fields (`Point { x: 7
y: 2 }`, and the newline-separated spelling lupin always refused),
closure parameters (`fn(a b)`), and inline-C capture lists (`unsafe c
[a b]`).

Blast radius across the world, measured: zero working code. The
wolf-lang corpus and fixtures (532 files), wolf-book (1,129), wolf-std
(887), wolf-web (1,976) and lobo (1,395) already write the comma; the
sole flagged file anywhere is a fuzz-minimized broken-input formatter
fixture, whose idempotence test still holds. The multi-line literal's
trailing layout is untouched (a terminator run before `}` is the
production's own) and the separator report latches once per list and
never into a reported wreck. Only lupin and the spec's letter were ever
this strict; the compiler now agrees with both.

Two more measurements rode along: `defer` runs at scope exit, not as
the frames return (D66/#193, now with a corpus witness pinning the
loop-turn interleaving on all three lanes), and two or-pattern
divergences got witnesses. An or-pattern over product alternatives
refuses on every wolfc lane, one inside a product refuses natively
while the checked executor runs it, and lupin runs both. These are
permissive-direction divergences that were invisible until a file put
them in the differ's ledger (#196).

### The letters

A bare entry name means `.` (#206). `wolf conform-run hello.lu`
answered "the package root has no wolf source files" where
`./hello.lu` ran the program: `Path::parent()` on a bare relative name
is the empty path, not `None`, and the anchoring that fixed it lived in
one CLI parser, which is why `build`, `run` and `fmt` worked
and `conform-run`, `test`, `interface` and `doc` did not. It lives in
the loader now, root and entry anchored together so a headered entry
stays its own module's entry. One consequence is worth stating, because
it is visible in the machine record: `conform-run` reads its argument
through the same anchoring before interning it, so the record's `file`
field carries the anchored spelling and `wolf conform-run hello.lu` and
`wolf conform-run ./hello.lu` now produce byte-identical records
instead of two `FileId`s for one file. Identical programs, identical
records, whatever the command line typed. On Windows before this
release that one missing `./` stood between a learner and their first
program, because `conform-run --checked` was the only way to run one.

`STR_PART` derives escapes (#198). v0.2.1 bounded `\u{…}` at one to
six hex digits and said in prose that the bound "binds in string
literals too" while the production derived no escape at all, so a
reader working from `spec/grammar.ebnf` got the bound for `'…'` and
nothing whatsoever for `"…"`. `STR_ESC` now carries the escape set,
`UNI_ESC` sits beside it, and `CHAR_ESC ::= STR_ESC | '\' "'"`, so
"the char set is the string set plus `\'`" is read off the productions
instead of asserted next to them. Two corpus witnesses make the string
half measured: the seven-digit refusal (a shape rule, not a value one,
since `0x0000041` is `A` and it is refused before anything asks) and
its in-bounds twin at one, four and six digits.

A trap runs no defers, at the root too (#209). `[conf.trap.exit]`
ruled the proc path and was silent about the root, so nothing pinned
whether a root-domain trap flushed its pending defers. The consistent
reading is written now, that a trap is not an error value and runs no
`defer` or `errdefer` anywhere and that at the root death is immediate,
with `faults/trap_skips_root_defers.lu` as the witness. It records a
measured divergence for the interpreter's next sprint: every wolfc lane
abandons the pending root defer, lupin 0.1.22 runs it. The differ could
never have found this on its own, because on a trapping program the
interpreter's record carries no stdout, so the two machines are
verdict-identical whatever they print.

Two nondeterministic verdicts retired. The net refusal probes
dialed a just-released ephemeral port and bet that nothing took it in
between; under `cargo test`'s full parallelism that bet lost, and it
reddened a trunk gauntlet while passing 3/3 in isolation (#205). They
dial a port from outside the host's ephemeral range now, one nobody's
`bind(0)` can be handed, so one dial is the whole story. And the
bare-entry suite's linking rung builds the runtime staticlib on demand
and skips where a host cannot link, with the reason printed, instead of
reading an absent toolchain as a regression.

## 0.2.1 — 2026-09-01

THE LETTER AND THE ARCHIVE. A patch release: no new features, and
nothing runs here that did not run at v0.2.0. It carries a tag the
Windows archive can finally reach, the pattern work that landed
between the tags, and four places where the written language and the
built one had drifted apart, each one measured on the tools before a
word of it was rewritten. It pairs with lupin 0.1.20 at pin
`b80d239` (D57: the pin is part of this release's identity).

### Patterns take the product domain (s129, s130)

Struct patterns went through the whole pipe (`[gram.pat.struct]`,
#179): `Point { x, y: p, .. }` takes a struct apart by field name,
fields move field-wise so an unnamed field stays live, a pattern
without `..` must name every field (E0814, the same lean E0408 takes
at construction), and the formatter has a canonical form for them.
The lent-view slice gap closed alongside it (#184): `b[lo..hi]` over
a lent byte view resolves per `[mem.list.slice]` instead of meeting a
misattributed c06 refusal.

Match arms then took the full product domain (s130, retiring that c06
arm): tuple and struct patterns, `@`-bindings, literals at product
depth, and products nested through enum/row payloads (`Pair(a, 0)`,
`Dot(Point { x, y: 0 })`) compile on both native tiers and execute on
the checked lane, with exhaustiveness and the redundant-arm warning,
which already reasoned over products, finally carrying running
witnesses. Two shapes stay refused on the native pipe: an
enum/row test inside a product, and a `str` literal inside one.

An arm still takes the whole scrutinee when it binds a non-`Copy`
piece. The field-wise partial-move story remains a `let`-binder rule;
the boundary is pinned by an `E1001` witness and the diagnostic's
`copy` suggestion is the sanctioned idiom. Two checked-lane fixes rode
along (arm guards evaluate their condition instead of their wrapper
node, and a qualified constructor's dotted tag (`Pairs.Pair`) matches
a bare-name arm the way the native tiers compare tag ids) and one
release-tier fix: a type-blind peephole could fold a bool `bxor x, x`
to an integer constant and ICE the verifier, so boolean results now
fold to `bconst`.

### Four letters, each measured first

`defer` runs at scope exit, not as the frames return (D66, #193).
Every implementation already did: a `defer` in a loop body fires at
the end of each turn, and `[mem.shared.drop.1]` had implied it all
along, since a drop that runs "at scope exit, LIFO with defer" cannot
be LIFO with something frame-timed. `[mem.model.order]`'s frame
wording is amended and a corpus witness now pins the interleaving on
all three lanes.

`\u{…}` takes one to six hex digits (#189). The clause said so in
prose while its production said `HEX_DIGIT+`, and nothing said which
was normative. The lexer bounds it at six, so the prose was the
letter and the production amended. The bound is on the escape's
shape, not on the value it names: leading zeros count, and
`'\u{0000041}'` is refused before anything asks that it spells `A`.

Two region diagnostics stopped lying (#192). `W1001` claimed a
region "never allocates — delete the region" on blocks whose callees
allocate through it; the fact it read was the in-frame site list, and
D12 charges a callee's allocations to its caller's ambient, so the
advice was measured at +82 MB on the reporting program. It now
requires no call in the region's extent either. And `E1010` refused a
region block whose tail was a unit-typed raising call: an error union
is never `Copy`, so every raising call got a phantom ambient
allocation, which in tail position read as the block's value
outliving the region. That judgement now reads through the error row
the way the region-return judgement beside it already did: a row tag
carrying a real payload still allocates and still gets its site.

### The archive

v0.2.0's release workflow produced three of four tier-1 archives and
failed on Windows: `cargo xtask dist` hunted the unix staticlib name
on MSVC, the first tag since s59 added the guard. Fixed forward at
`10c2bf8`: the staticlib is named for its target, and the Windows
archive stages `wolf_rt.lib` against the day s60 teaches the driver to
link there. This tag proves it (#183).

## 0.2.0 — 2026-08-30

WOLFGANG TELLS THE TRUTH. v0.1.0 was an identity claim that shipped
the debug tier and named the rest not built. Nineteen days later the
rest is largely built, and this release's own contribution is saying
so precisely: `wolf --version` now names the build's commit and
never claims a release it is not (D57: a build made at the `v0.2.0`
tag prints the bare version; every other build answers
`0.2.0+dev.<commit>`), the observation record's `impl_version` carries
the same identity, and the release pairs with lupin 0.1.17 as the
reference interpreter at pin `addcd7f`, a pin that is part of this
release's identity (D57 again). Everything below landed on trunk
between the two tags, told by campaign.

### The release tier, and the M2 declaration

Campaign c09 made `wolf build --release` real: the backend emits
textual LLVM IR handed to the system clang, with no llvm-sys, no
inkwell and zero new dependencies (D33), plus `--emit=llvm-ir`, dominator-scoped
GVN, check elimination, clustering with byte-identical output at 1, 4
and 8 threads, and PGO's instrumented loop (`.wprof`, `wolf profile
show|merge`, branch weights). The campaign also found and killed a
real miscompile: a false `!noalias` pair from region-freshened
inlining let LLVM delete a load; the fix made the role the aliasing
unit, at measured zero cost. The perf campaigns that followed (c23
range facts, c24 the six mechanisms, c29 the hot header, plus the a2,
a5, b3, d2 and fmt lanes) drove the thirteen-kernel T1 suite from
0.476x to over 1.0, and on 2026-08-24, after three consecutive
nightly holds under `cargo xtask bench ritual` (geomean 1.099, 1.066,
1.080 vs naive `clang -O3`, two documented exceptions renounced by X3
and D25), M2 was declared (#89). The ritual treats a refuted gate as a
result rather than a failure; its ledger is in the repo.

### The platform matrix

The compiler came home (s59, c13): the macOS/aarch64 native tier
lights, with the Apple-arm64 C ABI (`wolf-abi-2`, differentially green
against Apple clang), Mach-O emission with dSYM and lldb parity, the
kqueue reactor, deterministic links, and all 21 linux-only test
headers flipped to runtime skips that distinguish environment from
breakage. The s127 sprint brought the release tier along: triple and
datalayout
is host-derived from clang's own emission (never hand-composed), and
macOS holds three-tier parity at full linux floors. A nine-commit
Windows sweep then hardened what the s59 flip exposed: environment
refusals became exit 2 (so Windows stopped reading every native suite
as a program refusal), fixtures learned RFC 8089 file URLs, and a
real product bug fell: `wolf add`'s git fetch now pins line-ending
config, so an `autocrlf` host no longer manufactures a false E1506.
CI runs six jobs across linux x86-64, macOS aarch64 and windows
x86-64; linux/aarch64 waits on a runner (#166).

### The toolchain

`wolf_pkg` (c11) is D33's covenant as running code: declarative
manifests with no expressions anywhere, MVS resolution, blake3
content identity, capability audit where a dependency acquiring
`exec` fails CI before the ledger moves, the transparency log's
Merkle half, `wolf publish`, `wolf vendor`, `replace`/`exclude`, and
spec §8. The C importer (c10, s46) landed as a versioned interface
artifact produced by an out-of-process libclang worker: the compiler
binary links no C frontend, and the importer names what it refuses.
`wolf doc` and script mode arrived with s53, whose rider created the
PAIRING file this release stamps.

### The claims become native

What v0.1.0 could only interpret or check now compiles. Concurrency
(c19): scope, spawn, channels and procs lower to machine code with
the defer law pinned on the release tier and a proc's argument record
copied at spawn (D14, D15, D16, D30). The io surface (c20): sixteen
fs builtins, sorted `read_dir`, and no false promise of atomic rename.
The crossing (c26) closed the checked-native split for builtins
entirely: net (with `error: timeout` firing for the first time on
any lane), JSON without an invented DOM surface, and process control
with zombie discipline. Generics (c21) monomorphize on one worklist with
D8 dedup; dispatch (c22) landed trait methods, `call.ind`, and the
two-word trait object ABI, completing the story from D28 to D47; the
value tier (c25) made regions and closures values, with capturing
closures as an (entry, env) pair through `call.ind`. The consumer
findings campaign (c17) turned wolf-std's friction into fixes,
including the byte-view lend analysis and the first code retirement
(E1015 → W1004).

### c27 — small debts

Five reopenings of the same discipline: front-end debts paid without
inventing syntax. Raw-literal decoding, `main`'s legal shapes (E0414),
scoped bottom for literal `assert(false)`, nested error rows
flattening (D51), declared-row-first widening (D52), the numeric
literal chapter `[type.numlit]` with int-adopts-float and trapping
float→int casts (D54), value-preserving `wrapping[T] as int` (D56),
and `List[mod.Type]` reading an imported type in bracket position.

### c28 — the constant-time tier

`#[consttime]` is a verified contract (D53): secret-tainted data
cannot decide a branch, index memory, pick a call target, or feed a
variable-time instruction. Each is refused fail-closed at the WIR rung
on every lane in every build mode, E1601–E1607, with spec/09 written
before the pass and assembly witnesses whose bodies contain zero
conditional branches.

### c29 — the hot header

Loop-carried header promotion, licensed only by existing proofs:
`_Wmain` on the b3 kernel gave back 8.1% of its instructions, ten
other kernels stayed byte-identical, and the campaign's real product
was a correction: the number it chased was an ops count misread as
an instruction count, and the instrument outvoted the narrative.

### c30 — signals

The program hears the signal: reception via a self-pipe trampoline,
`os_signal_listen/wait/raise` on every lane, and `os_random` over
getrandom/getentropy/BCryptGenRandom with no fallback and no seeding,
where failure traps. The `[os.signal]` and `[os.random]`
clauses wrote the platform matrix down, which pre-authorized
the macOS crossings.

### c31 — clustering

The shim travels with its spawner: summary v3 records `func.addr` as
a reachability edge, so a spawn shim or closure entry can never be
split away from the body that references it.

### c32 — codegen debts

The loop and the layout: the versioning pass routes live-outs against
the current CFG, token linearity refined to the edge target, and List
element stride rounds up to the element's alignment, witnessed on
session-shaped and mixed-width layouts.

### c33 — strings

`chars()` landed on every lane, and then the scalar got its type:
`char` is a Unicode scalar value (D58), four bytes, scalar-value
order, no arithmetic, `char as int` total and `int as char` trapping
on the surrogate gap and out of range, with `chars()` re-typed
`List[char]` and every caller migrated.

### c34 — the server

`wolf lsp` serves completion (keywords, scope names, typed members)
from a query that answers incomplete and broken buffers, measured
under the keystroke budget.

### c35 — the crashes

The compiler does not panic: str-match constant-folding builds no
orphan blocks, GVN scopes per continuation, and an integer literal
that cannot fit its type is E0415 at the front end, not a verifier
ICE. The one suspected oracle hole (#152) was ruled both-correct:
X3 has no hole at list-element provenance, only a width.

### c36 — the module explains itself

D59: directory = module by default, membership is the default and
standalone is a normative opt-out, E0301 explains both of its
situations, a silently unparseable sibling is E0202, and E0302 is
reachable from `wolf build`.

### c37 — the trap names its site

A trap's second stderr line says where (`at file:line:col`) while the
first line stays the byte-identical parsed ABI; per-site cold blocks
cost ~0.2% of .text; `[conf.trap.report]` and `[conf.trap.render]`
rule the shape, and D60 rules that the kind, not the exit status, is
the contract. The release cache learned that site coordinates are
part of a cluster's key.

### c38 — the origin

D61: the index chooses its origin, with `#![index(0|1)]` as a lexical
marker, 1-mode coupling inclusive ranges, the shift landing as one
checked subtraction on both executing lanes, and zero cost when the
marker is absent: 422 files, zero verdict flips.

### c39 — the book writes

The walls the language showed its first real user came down in one
sprint: comma-grouped binders (D63), tuple destructuring with
element-wise moves live on three lanes, `str + str` as
interpolation-append (D62) with mixes still refused, and list
slices as fresh-List copies with `for` over a slice. The acceptance
was the human's own scratch programs running unmodified on all three
tiers, byte-identical to lupin.

### The spec, the corpus, and this release's own fix

The conformance machinery hardened alongside: lane coverage became a
gated ratchet, `cargo xtask peel` reads the fail-fast ledger, and the
anchor registry now holds 403 anchors, including `gram.lex.ident`,
which the spec-extract bracket scanner had silently dropped twice by
pairing a bare `[` literal in prose with the next anchor's `]`
(F-0100, #170/#177; fixed this release, with the s126 shebang prose
as the pinned regression witness). At the tag the lane-coverage
floors stand at checked 235 / native 257 / release 257 / union 271 /
all-three 221.

v0.1.0's notes said the release tier, macros and the registry were
not built. The release tier is built and declared against its gate;
the registry protocol is built with its hosted half still waiting
(X7); macros remain the CTFE tier's future. The corpus ledger, not
this document, stays the authority on completeness.
