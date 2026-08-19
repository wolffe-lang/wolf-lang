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

## §2 C membranes `[abi.c]`

- `[abi.c.seams]` Exactly three ABI seams exist: `extern "c"` function
  types/imports, `export`ed wolf functions (C-callable), and
  `#[repr(c)]` (+ `packed`, `transparent`) data. There is no fourth
  mechanism; everything else is `wolf-abi-0` internal.
- `[abi.c.types]` Only repr(c)-compatible types cross a membrane by
  value: scalars, raw pointers, `#[repr(c)]` aggregates. Anything else
  is a compile error with a fix-it naming the nearest compatible shape
  (E1201).
- `[abi.c.targets]` Per-target lowering contracts, by name: SysV AMD64
  classification (linux/freebsd x86-64), AAPCS64 (linux aarch64),
  Apple-arm64 deltas (macOS), win64 (windows x86-64). The s49
  differential fuzzer against the platform C compiler is each
  contract's acceptance test.
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

---

Cross-references: no-unwinding invariant `[abi.native.nounwind]` ⇄
`[conc.cancel.defer]` (cancellation uses returns) ⇄ `[conc.proc.kill]`
(kill uses region frees, not defers) — the three clauses jointly close
D30's "errors are values" story at the binary level.
