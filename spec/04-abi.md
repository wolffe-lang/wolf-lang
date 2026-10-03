# Wolf Language Specification — 04: ABI

Status: normative, v0 (sprint s05). Anchors `[abi.*]`. Implemented by
s29 (abi-v0), s41 (release tier), s55–s56 (Tier-F); differentially
fuzzed against platform C compilers by s49. Evidence: reports/06 §5;
per-target documents saved in `.docs/refs/articles/` (SysV AMD64 psABI,
AAPCS64, win64, Apple arm64 deltas).

---

## §1 wolf-abi-0 `[abi.native]`

- `[abi.native.unstable]` The native ABI is **versioned and unstable**:
  `wolf-abi-0`. Nothing about it is a compatibility promise; separately
  compiled wolf artifacts must agree on the version string or fail to
  link. Stability is a post-1.0 question.
- `[abi.native.layout]` Layout freedom is the point (T1): outside
  `#[repr(c)]`, field order is unspecified, niches are packed, and
  **no address identity** exists for value fields (s04 §7/O10). Programs
  observing layout via Tier-3 pointer arithmetic get target- and
  version-specific answers.
- `[abi.native.niche]` Guaranteed niches (normative, so safe wrappers
  are zero-cost — report 06 §5): an option-shaped enum over `handle T`,
  a non-null raw pointer wrapper, or `shared T` occupies exactly the
  payload's size — the null/absent case is the niche. This list may
  grow; it may not shrink.
- `[abi.native.call]` Calls may use multiple return registers, pass
  small aggregates in registers, and omit shadow space and frame
  pointers where the target permits. Callee/caller-save split is
  "allocator-tuned within a target's published contract" — each target
  backend publishes its contract file; Tier-F is the reference
  implementation.
- `[abi.native.nounwind]` There are **no unwind tables and no landing
  pads** anywhere in wolf code (D30/Perceus precondition, s04
  `[mem.shared.drop]`): every control transfer is a call, return,
  branch, or trap.
- `[abi.native.taskenv]` A spawned task's captures cross to the
  runtime as ONE pointer to a **capture record** whose layout is
  `wolf-abi-0` internal, paired with a task-entry function that reads
  it (`[conc.task.spawn]`). The record's storage is charged to the
  spawning `scope`, and the scope may not release it before
  `[conc.task.join]` completes — so the record is live for at least as
  long as the task is. Where the storage comes from is an
  implementation choice (a caller frame slot suffices for a spawn site
  reached once; a site under a loop needs one record per reach, and
  the scope's own arena is the natural home); *that* it outlives the
  join is contract. Nothing here is a stability promise —
  `[abi.native.unstable]` governs.
- `[abi.native.procenv]` A proc's arguments cross to the runtime as
  ONE pointer to an argument record of the same internal shape, paired
  with a proc-entry function that reads it (`[conc.task.root]`), plus
  the record's byte length — and the runtime **copies** the record
  before the spawn returns. A proc has no extent at its spawn site
  that outlives it: it is a failure domain under the root supervisor
  (`[conc.proc.model]`) and by design outlives the frame that spawned
  it, so the only owner that can keep its record alive is the proc's
  own frame. The copy is charged to the proc and lives until its body
  returns; the spawner's storage is free for reuse the instant it has
  the proc id back, which is what makes `spawn proc` under a loop
  sound with one record slot per site. Same stability status as the
  task record: `[abi.native.unstable]` governs.

- `[abi.native.dyn]` A trait object is a TWO-WORD pair: the data
  pointer, then the vtable pointer, laid out as an ordinary by-value
  aggregate `{ptr, ptr}`. The data half points at the erased value and
  carries exactly the region obligations the checker assigned that
  value — erasure changes dispatch, never ownership. The vtable half
  points at immutable static storage: one pointer-sized slot per
  method of the trait's dyn-safe method set, in the CANONICAL order
  sema's dyn-safety report records (the interface serializes that
  list, so a slot index is a cross-module fact — not declaration
  order). Every slot holds a function of the ERASED signature: the
  receiver crosses as the data pointer, every other parameter and the
  result exactly as the trait declares them, which dyn-safety
  guarantees are `Self`-free; a slot whose target's own convention
  differs (a by-value receiver, say) is reached through a shim of the
  erased shape, and the shim is the table's problem, never the call
  site's. Dispatch is two loads and an indirect call; nothing else is
  promised. Tables are demanded by `as dyn` cast sites
  (`[mem.dyn.unsize]`, D47): one content-interned table per
  (trait, impl) pair, synthesized with its shims when a cast first
  names the pair; the data half is the cast operand's spilled place.
  Same stability status as everything in this section:
  `[abi.native.unstable]` governs.

- `[abi.native.closure]` **Every fn value is one word: a pointer to a
  callable record whose first word is the entry.** The entry is a
  function taking the record pointer first and the declared
  parameters after; a call through a fn value is one load (the entry)
  and one indirect call with the record leading — `[abi.native.dyn]`'s
  shape with a one-slot table and no data half. A named function read
  as a value, and a capture-free closure (which lambda-lifts to a
  module function), sit behind a STATIC one-slot record —
  content-interned, so every read of the same function is the same
  pointer; the named function's slot holds a shim of the record
  convention that drops the leading pointer. A CAPTURING closure's
  record is the entry word followed by its captured values in capture
  order at 8-byte-aligned offsets (the `[abi.native.taskenv]`
  packing), allocated in the AMBIENT region of the frame that builds
  it (`[mem.region.create.3]`: the placement every container gets, so
  the value outlives its frame exactly as far as any other value built
  there does, and a `region` around a closure-building loop bounds the
  records the way it bounds a `List`). The record COPIES its captures
  at creation (`[type.fn.value]`); the mem tier's shared loan on the
  captured places is what makes that copy unobservable while the value
  lives. A binding that names a capturing closure keeps the entry
  beside the record, so a call by name skips the load. (Rewritten
  2026-09-10, s150 — wolf-lang#300: the s105 shape was a two-word pair
  that never left its frame, and chapter 4's `adjusted(340, both)` was
  refused for it. The `fn(int) -> int` a callee receives is now the
  same word whether a named function, a capture-free closure or a
  capturing one stands behind it.) Same stability status as everything
  in this section: `[abi.native.unstable]` governs.

## §2 C membranes `[abi.c]`

- `[abi.c.seams]` Exactly three ABI seams exist: `extern "c"` function
  types/imports, `export`ed wolf functions (C-callable), and
  `#[repr(c)]` (+ `packed`, `transparent`, not yet implemented) data.
  There is no fourth mechanism; everything else is `wolf-abi-0`
  internal. **The only ABI string is `"c"`**: `extern "S"` with any
  other `S` is **E0818**, naming it (K7, STATUS #31 — wolf-lang#524).
  In particular wolf has no interrupt calling convention: an interrupt
  or exception enters through an assembly trampoline that calls an
  `export fn` with the C convention (KWC F7); before the rule
  `extern "x86-interrupt" fn` compiled as an ordinary function
  returning with `ret`.
- `[abi.c.export]` `export fn f(…) { … }`, and `extern "c" fn f(…)`
  **with** a body, define a wolf function C calls: one global symbol
  named exactly `f` (no module path — an export in a child module is
  still `f`; C has no modules), defined under the target's C plan
  (`[abi.c.targets]`), and kept by every tier whether or not wolf code
  calls it. A narrow integer or `bool` result is widened to 32 bits by
  its signedness before it returns (the Apple arm64 callers C compilers
  emit rely on it). A wolf call to it crosses under the same C plan, in
  or out of its object. An export is one function: a generic or `comptime`
  `export fn`, an export read as a fn value (fn values are
  wolf-convention; C function pointers are wolf-lang#520's), a
  `mut`/`take` parameter, and a second function under the same symbol
  (another export, a package-root function, or `main`) are refused by
  name. Before kw02 (wolf-lang#513) the source spelling reached none of
  this: the symbol was mangled under the wolf convention (a
  `#[repr(c)]` struct by value from C segfaulted) and `--release`
  dropped it. Witnesses: `corpus/membrane/export_called.lu`,
  `export_child.lu`, and `c_membrane_link.rs`, where a C program calls
  21 exports on both compiling tiers and checks every value.
- `[abi.c.import]` `extern "c" fn f(…)` **without** a body declares a C
  function wolf calls: a call is a call into imported C, the raw-tier
  operation E1301 gates to `unsafe` (as `c.malloc` is,
  `[mem.unsafe.scope]`), and lowers to the plain symbol `f` under the C
  plan, with exactly the declared prototype (a variadic C function
  declared without its varargs is the author's error, as in C;
  wolf-lang#515). A narrow integer or `bool` argument is widened to 32
  bits by the caller according to its signedness, the rule C callers
  keep and clang's callees and the Apple arm64 ABI rely on. The
  program's link supplies `f` (the C library, or an object linked
  beside the wolf objects). The checked machine and the reference
  interpreter have no C membrane and refuse the call by name. `import
  c` resolves the modelled five (`malloc`, `calloc`, `free`, `memset`,
  `memcpy`); its other names wait for the header importer (the second
  half of wolf-lang#521). Witnesses: `corpus/membrane/extern_libc.lu`,
  `corpus/memory/extern_c_outside_unsafe.lu`, and `c_membrane_link.rs`,
  where wolf calls 14 C functions and two of them write through wolf
  pointers.
- `[abi.layout.c]` `#[repr(c)]` on a struct is the target psABI's C
  layout (K4, STATUS #31): fields in declaration order, each scalar at
  its natural alignment (its size), an aggregate field aligned to its
  strictest member, the size rounded up to the struct's alignment. It
  holds wherever the bytes can be observed: a local or a field (the
  native layout already places aggregates this way), and **a raw
  pointee** — `p[i]` through `p: *T` loads and stores `T`'s fields at
  their C offsets and steps `p` by the C `sizeof`, tail padding
  included, so what C reads is what wolf wrote and the reverse
  (wolf-lang#523: before kw01 the raw tier wrote a pointee packed,
  `{u8, u32, u8}` at offsets 0 1 5 where C has 0 4 8, size 12). The
  raw tier uses this layout for every struct pointee; for a struct
  without `#[repr(c)]` it is a fact of this version, not a promise
  (`[abi.native.layout]`). Padding is never read or written. The
  checked machine and the reference interpreter refuse a
  whole-aggregate raw load or store by name (they have no byte-level
  aggregate) rather than model a layout. `packed`, `align(N)`,
  `transparent` and the comptime `size_of` / `align_of` / `offset_of`
  are KWC F4's later half: E0817 and E0708 until then. Witnesses:
  `memory/raw_repr_c_layout.lu`, and `repr_c_raw_layout.rs`, which
  holds a C compiler to both directions on both compiling tiers.
- `[abi.c.types]` Only repr(c)-compatible types cross a membrane by
  value: scalars (the sized integers, `int`/`uint` as 64-bit, `byte`,
  `bool`, `f32`, `f64`), raw pointers (`*T` stands in a membrane
  signature by `[mem.unsafe.sig]`), and non-generic `#[repr(c)]`
  aggregates whose fields cross. Anything else — `str`, a container, a
  struct without `#[repr(c)]`, an error union (`[abi.err.row]`) — is a
  compile error with a fix-it naming the nearest compatible shape
  (E1201). E1201 is not built yet: until it is, the compiling tiers
  refuse such a parameter or result by name (`unsupported`, the
  construct named), never with a guessed layout.
- `[abi.c.targets]` Per-target lowering contracts, by name: SysV AMD64
  classification (linux/freebsd x86-64; the s29 implementation),
  **Apple-arm64 (macOS aarch64; implemented s59)** — AAPCS64 with the
  Apple deltas: 8 GP + 8 FP argument registers; composites ≤ 16 bytes
  whole in GP registers; HFAs (1–4 same-type float members) member-wise
  in FP registers, exempt from the 16-byte cap; larger composites
  indirect via a caller-owned copy; the `x8` indirect-result register
  not drawn from the argument eight; stack arguments packed at natural
  alignment. Register-exhaustion shapes the backend cannot express are
  refused BY SHAPE, loudly — never lowered divergently. **win64
  (windows x86-64; BRING-UP contract, s60a)** — the MSVC x64
  convention for scalars and pointers (four position-indexed
  argument slots RCX/RDX/R8/R9 ⊕ XMM0–XMM3, the caller's 32-byte
  shadow space, both executed by the backend's `WindowsFastcall`);
  every aggregate crossing by value, in either direction, is refused
  BY SHAPE until the campaign's cl.exe differential lands the MSVC
  rules (1/2/4/8-byte composites as their bits in a register, larger
  ones by pointer to a caller-owned copy, returns beyond 8 bytes via
  a hidden pointer). Still a contract to fill: AAPCS64 (linux aarch64
  — ~80% shared with the Apple plan). The s49 differential against the platform C
  compiler is each contract's acceptance test (Apple clang on macOS
  since s59).
- `[abi.c.panic]` A wolf fault reaching an `extern "c"` or `export`
  boundary **aborts the process** (after the fault report). There is no
  `c-unwind` at v1; C frames are never unwound (`[conc.cancel.c]`).

## §3 Callback and pointer rules `[abi.callback]`

- `[abi.callback.reentry]` A wolf function passed to C as a callback
  re-enters the safe-point domain on entry (`[conc.ffi.external]`); its
  ABI is the membrane ABI regardless of how C stored the pointer.
- `[abi.callback.stash]` C-held pointers obey s04 `[mem.boundary.ffi]`:
  past a call's return, C may retain only `handle`-backed or pinned
  `#[trusted]`-region pointers. The membrane type checker rejects
  signatures that would smuggle other wolf pointers into retention-shaped
  parameters only where declared (`#[retains]` annotation on the import
  — c10 mechanism; clause here so the ABI table is closed).

## §4 Error-value ABI `[abi.err]`

- `[abi.err.repr]` A `!T` return lowers to a two-value contract:
  a **discriminant** (ok/error) and a **payload** (T or the error row's
  representation), in registers where the target's contract allows,
  else via sret-style memory. Observable contract only: callers branch
  on the discriminant; there is no unwinding, no landing pads, no
  side-channel (errno-shaped) state — ever.
- `[abi.err.row]` Error rows lower to a tag + payload union whose layout
  is `wolf-abi-0` internal (rows never cross C membranes; an `export`ed
  function returning `!T` is E1201 with a fix-it to flatten).
- `[abi.err.trace]` Debug builds accrete error return traces at each `?`
  propagation and `else` observation point, into trace storage disjoint
  from the value path. Normative: debug and release differ **only in
  this metadata**, never in control flow or in the §4 value contract.
- `[abi.err.trap]` Traps (s06 vocabulary) are not errors: they do not
  use this ABI; they terminate through the fault path
  (`[abi.c.panic]` at boundaries).

## §5 Targets `[abi.target]`

- `[abi.target]` A build has one target, named by an LLVM-style triple:
  a hosted target (the host the compiler runs on, one of the
  `[abi.c.targets]` matrix) or the freestanding `x86_64-unknown-none`.
  It is a fact about the build, never the source (K1, STATUS #31):
  `wolf build --target x86_64-unknown-none`, or `target:
  "x86_64-unknown-none"` in the root `wolf.pkg` (`[pkg.manifest.schema]`;
  the flag wins); with neither, the host. `cfg(target = "…")`
  (`[gram.item.attr.cfg]`) reads the build's target, so one source file
  keeps a hosted item and a freestanding item side by side. A triple
  that is neither the host's nor `x86_64-unknown-none` is a tool error
  naming both (no tier cross-compiles to another hosted target yet).
  The checked machine and lupin run no target but the host:
  `conform-run --target x86_64-unknown-none` is `unsupported` with the
  construct `the freestanding target x86_64-unknown-none` on every rung,
  never run.
- `[abi.target.none]` On the freestanding target the compiler links
  nothing and emits objects only (`--emit=obj`, or the IR and WIR
  dumps); `bin` and `wolf run` are refused by name, as are `--checked`
  and `--profile-gen`, which are the hosted runtime's. The object has no
  `main` shim, no runtime library, no ambient-region allocation, and
  imports only the hook list (`[abi.target.none.hooks]`) and the
  program's own `extern "c"` declarations. A construct that needs more
  is refused BY NAME at the construct, on both compiling tiers alike,
  before any backend runs: `` `print` needs the hosted runtime (target
  x86_64-unknown-none) `` — and likewise `env_args`, `read_line`, a `str`
  comparison, `spawn`, channels and the rest of the runtime's surface.
  The refusal reads the lowered program before the optimizer, so a
  construct the release tier would fold away is refused there too.
- `[abi.target.entry]` On the freestanding target the entry is whatever
  `export fn` the boot code calls (`[abi.c.export]`: one global symbol
  under its own name, the C convention, kept by every tier). `main` is
  an ordinary function there, mangled like any other.
- `[abi.target.none.hooks]` A freestanding object imports at most (a)
  `wolf_trap(kind: i32, file: *u8, file_len: i64, line: i64, col: i64)`,
  the one trap hook (K8(a)): every trap of both tiers calls it with the
  kind numbered as `[conf.trap.set]`'s runtime lists it (overflow 1,
  div-zero 2, bounds 3, …) and the trap's site — the source path bytes
  and the 1-based line and column — or `(null, 0, 0, 0)` where the tier
  has no site; it must not return, and the compiler emits `ud2` after
  the call; (b) `memcpy`, `memmove`, `memset` and `memcmp` with C's
  meaning, which a tier may emit for an aggregate copy; (c) the
  program's own `extern "c"` declarations. The program supplies (a) and
  (b), in wolf (`export fn`) or assembly. On a hosted target
  `wolf_trap` is the runtime's own report-and-exit: a hosted program
  that imports it (a freestanding module's logic under a hosted test,
  say) links it as an alias of the runtime's sited reporter. The alias
  is made at the link and only for a program that imports the hook,
  and hosted code keeps calling the runtime's own reporters, so no
  other hosted program changes by a byte.
- `[abi.target.none.alloc]` No allocating construct compiles for the
  freestanding target (K8(b) = A): `List`, `Map`, `Pool`, string
  interpolation, a capturing closure, `region` and a boxed channel
  payload are each refused by name (`` `List` allocates, and target
  x86_64-unknown-none has no allocator ``). An allocator hook lifts this
  in its own lane (KWC kw12).
- `[abi.target.none.codegen]` Code generated for `x86_64-unknown-none`
  uses no red zone, no x87/MMX/SSE/AVX register and no stack protector,
  keeps frame pointers, and is position-independent under the small
  code model (K10(a) = A: it links in the -2 GiB higher half and loads
  anywhere). The release tier emits the triple
  `x86_64-unknown-none-elf`, and `noredzone`, `"frame-pointer"="all"`
  and `"target-features"="-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,+soft-float"`
  on every function; the native tier's frames never use the red zone
  and its integer code uses no vector register (proven by disassembly
  on every object the gate builds). A floating-point value of any type
  is refused by name on this target (K10(b) = A). Witness for the whole
  section: `crates/wolf_driver/tests/freestanding_target.rs`, which
  links both tiers' objects with an assembly boot stub and trap hook,
  no libc and no runtime, and runs them.

## §6 Assembly `[abi.asm]`

- `[abi.asm]` Assembly reaches a wolf program one way at this cut:
  whole routines in assembly source files the root `wolf.pkg` lists,
  linked beside the program and called through bodyless `extern "c" fn`
  declarations (K2 = C, STATUS #31: linked files now, the inline `asm`
  block later). An interrupt entry, `lgdt`/`lidt`, a context switch and a
  boot path are whole routines, so they are written this way.
- `[abi.asm.link]` `asm: ["boot/io.S", …]` in the ROOT manifest
  (`[pkg.manifest.schema]`) lists assembly sources, each path relative to
  the manifest's directory; a listed file that does not exist is refused
  naming it. The driver assembles each with the C driver the build
  already uses for its target — on `x86_64-unknown-none` the release
  tier's clang (`WOLF_CLANG`, else `clang`) with
  `--target=x86_64-unknown-none-elf`, from any host, and the result must
  be an ELF x86-64 object; on a hosted target the link's `CC` (else
  `cc`) — and hands the objects to the link, or, under `--emit=obj -o
  K.o`, writes each beside the program's object as `K.asm-<stem>.o` for
  the boot code's link (K6(b): the consuming build links a freestanding
  image). A routine is reached through a bodyless `extern "c" fn`
  declaration called inside `unsafe` (E1301, `[abi.c.import]`). An
  assembly source is not part of a package's content address
  (`[pkg.sum]`), so a DEPENDENCY's manifest that lists `asm` is refused
  (E1502): only the build's root lists assembly. A build that lists none
  links exactly as before, byte for byte.
- `[abi.asm.roster]` The listed sources are audited like `#[trusted]`
  code (`[mem.boundary.trusted]`). Their roster is the names their
  `.globl`/`.global` directives make visible, read from the text (a
  routine made global by a macro or an included file is not on it). On
  the freestanding target, a package that lists assembly calls through a
  bodyless `extern "c" fn` only into its roster or the hooks
  (`[abi.target.none.hooks]`); a call into any other name is **E1306**
  at the call, naming the routine — E1303's sibling, so a kernel's unsafe
  ring is visible in its manifest. (On a hosted target an extern may be
  the C library's, so the roster names assembly but refuses nothing.)
- `[abi.asm.machines]` The checked machine and lupin have no assembly
  membrane. A call into a listed routine is `unsupported` on the checked
  machine with the construct `` a call into assembly `NAME` `` (the
  roster is read from the `wolf.pkg` beside the entry; the verdict is the
  same `unsupported` without it, since the record is reproducible from
  the file and the flags alone); lupin refuses the call to a bodyless
  declaration by name (`` `NAME` has no body ``). A refusal is excluded
  from divergence counting and listed in the conservatism ledger
  (`[proto.record.unsupported]`). Hosted-testable logic takes its port
  writes as a parameter so it runs on all four machines.
- `[abi.asm.inline]` The `asm { "…", t = inout(reg) x }` block
  (`[gram.expr.unsafe]`): template holes, operand directions, the `reg`
  class and named registers, clobbers and `options(nomem, nostack,
  noreturn)` are reserved for KWC kw13 (LLVM inline assembly on the
  release tier, outlined to a generated assembly file on the native tier,
  which has none). **Not yet implemented**: until then every machine
  answers an `asm` block `unsupported` by name.

---

Cross-references: no-unwinding invariant `[abi.native.nounwind]` ⇄
`[conc.cancel.defer]` (cancellation uses returns) ⇄ `[conc.proc.kill]`
(kill uses region frees, not defers) — the three clauses jointly close
D30's "errors are values" story at the binary level.
