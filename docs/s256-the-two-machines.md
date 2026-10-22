# s256 — a stack overflow by name, and a read parameter's nested field (wolf-lang#866, #867)

Lane note. **GPT-6-Astra (Codex) to `e28275d3`, then Fable 5.1 from
`e8a02b5b`. Crawl 54.** Branch `s256` off wolf-lang trunk `0477ce62`
(0.2.34's content). Contract:
`sprints/compiler/s256-the-two-machines.md`. The prediction was
committed first (`adf32227`, now `docs/s256-evidence/prediction.txt`);
the witnesses before the fixes (`3f8a0910`); the spec row by its own
commit (`900bdcf0`); the control failures (`a49ddaa2`) before the
handler (`75279748`) and the move markers (`df5fff6e`, `42a8da6d`).
The evidence is under `docs/s256-evidence/`; the rows were measured on
kasumi, in `archlinux:base-devel` containers with `ulimit -c 0`,
`ulimit -s 8192` and `--cpuset-cpus 0-3`, `SigBlk`/`SigIgn` printed
before every fault.

## 1. The handler (#866) and the thread sites

**What the control does.** At `9da7bdc` (0.2.34) deep recursion on the
main thread ends as a bare signal death: exit 139, no `wolf-trap` line,
on both compiled tiers (`rows-control.log`). A task's thread already
had a guard reporter (s246's pooled-stack guard list) and ended 134
with the legacy text `wolf-rt: stack overflow in task '…'`, but no
`wolf-trap:` line and no trap kind: the program died by a name the
spec's trap table did not hold.

**The design** (`crates/wolf_rt/src/task/stack.rs`). The reporter is
one process-wide `SIGSEGV`/`SIGBUS` handler (`SA_SIGINFO|SA_ONSTACK`,
installed once, idempotent), and the guard range it compares the fault
address against is **thread-local** (`CURRENT_GUARD`), not the old
append-only process-wide list: a wild pointer into another worker's
guard page is not this thread overflowing. A thread registers its
bounds at entry:

- a **pooled task stack** (`TaskStack`, the mmap'd stack with its own
  guard page) registers `[base, base+GUARD)` when a task is placed on
  it;
- a **system-provided stack** (`enter_thread`) asks the host: linux
  `pthread_getattr_np` + `pthread_attr_getstack` /
  `pthread_attr_getguardsize` (the main thread's attributes carry its
  `RLIMIT_STACK` range; the guard is below the low address, at least a
  page); macOS `pthread_get_stackaddr_np` (the HIGH address) and
  `pthread_get_stacksize_np`, the guard taken as the page below
  `top - size`. The prediction's "macOS gives the high address" held;
  no other macOS difference was needed.
- An **alternate signal stack** is installed per thread only when the
  thread has none (`sigaltstack` query, `SS_DISABLE`); a Rust
  `std::thread`'s own altstack is reused, never owned; one the runtime
  maps is unmapped when the thread leaves (`AltStack`'s `Drop`
  disables it first).

In the handler: the fault address in the current thread's guard range
is `stack-overflow`; the report is written with `write(2)` from a fixed
buffer — `wolf-trap: stack-overflow`, then `  at pc=0x…` with the
instruction address read from the kernel's `ucontext` (`REG_RIP` /
`pc` / `__ss.__pc`), then the legacy task line where a task owns the
thread — and the process exits 134 (`[trap.exit]`'s discipline,
`trap_code::STACK_OVERFLOW = 13`, `trap_kind_name` → `stack-overflow`).
Anything else is re-raised with the prior disposition, so s246's sent
`SEGMENT_FAULT` rows keep their shape, and an ignored sent signal
survives the installation (the `SIG_IGN` read into `sent_flag`).
Nothing in the handler allocates, takes the loader lock, unwinds,
reads DWARF or a file: the site is a PC only (R-s256-3).
`install_trap_containment` returns early for `STACK_OVERFLOW`:
exhausted frames never run a proc's containment protocol
(`[conf.trap.stack]`, the amendment to `[conc.proc.fault]`).

**The thread sites.** The prediction counted five production
thread-creation sites in `wolf_rt` (the pooled pthread worker, the unix
`std::thread` fallback worker when the stack reservation cannot be
mapped, the Windows worker, the signal drain, the reactor), with procs
and the compensation threads sharing the worker seam. That count held.
`enter_runtime_thread()` (`task/mod.rs`, a no-op off linux/macOS) is
called from:

| site | file | how it registers |
|---|---|---|
| the main thread | `native.rs` `__wolf_rt_stack_start` (the new RT symbol, emitted before user `main`) | `enter_thread` |
| the pooled worker | `task/pool.rs` (`mod os`) | `enter_runtime_thread` at thread start; the task stack's own guard when a task is placed |
| the fallback worker | `task/stack.rs` (`std::thread` when the reservation fails) | `enter_thread` — the `WOLF_TASK_STACK=4 EiB` row |
| the signal drain | `signal.rs` `reader_main` | `enter_thread` |
| the reactor | `reactor.rs` `reactor_main` | `enter_thread` |

The Windows worker keeps its legacy text (`docs/platforms.md`). No
pool or background thread starts just to install the reporter: a
program that never spawns pays one `sigaction` pair, one `sigaltstack`
and one `pthread_getattr_np` at startup (`7b2861ce` measured it).

## 2. The spec reading for #867, both consequences

The witness (`corpus/memory/read_param_indexed_caller.lu`, yew's
`notes/yw04-evidence/witness/read-field-move/` adapted): `probe(ed: Ed,
b: int)` binds `let tb = ed.bufs[b].tb` from a `read` parameter and
hands it on; `main` calls `probe` twice and then reads
`ed.bufs[0].tb.bytes.len` itself. Expected `1 1 1`.

**Before reading s252's record**, the prediction read
`[mem.tier0.mode.read]` as: a move out of a `read` view is the callee's
business; the caller's value is preserved across the call. **After**
(s252 left no `docs/s252-*.md`; its record is the E1001 commit and the
spec amendment), the rule is explicit on both halves: a later read of
the moved path **in the same activation** is E1001 at the compiler
(the single-field case s252 gave the fix-it to), and **a second call
must see the caller's value intact**. The witness only ever reads the
path again across calls. So the spec says the binding is legal and the
native/release behaviour (`1 1 1`) is the right one, and it is the
checked machine that was wrong: it wrote `Value::Moved` into the
caller's arena (the shared `List` storage), so the next activation —
and `main` — saw a tombstone.

**The two consequences.** Had the spec said "a move": the sema check
would extend E1001 from one field to a path through an index, with
s252's fix-it, and yew's `let tb = ed.bufs[b].tb` would be refused —
yew would read the field in place (as its fork already does in
`edit.cmdparse`) or `copy` it (a deep copy of the `List[byte]`, never
a free lend; there is no `lend` keyword). The spec said "a read through
the path", so (R-s256-2, built): the checked machine keeps the move
inside the activation. `Frame` gains `read_roots` (the values bound to
mode-less parameters) and `read_moves` (a tombstone list of
`(storage, path-from-that-storage)`); a partial move whose last
container crossing belongs to a read view (`lent_move`/`lent_storage`,
walking the read roots' reachable containers once, iteratively,
cycle-safe) is recorded there instead of written to the arena; a
`read_place` whose path matches a tombstone traps `use-after-move`
(`mem.tier0.move.2`) as before; owned and `mut` places are still
emptied in the arena. Aliases and different source spans name one
place (`same_step` ignores spans). The regression found on the way:
an **empty projection** — `count()`, `is_empty()`, a nested header
read — reads the container's header, not an element, and must not
match a tombstone (`f3c10897`, pinned by
`borrowed_header_methods_do_not_read_moved_elements`). yew keeps its
binding; nothing in its code needs to change for #867.

R-s256-1: the checked machine's 128-frame budget stays `unsupported`
(`call depth budget exhausted`) — an instrument limit, not a program
trap — and the native event is `stack-overflow`, host-dependent, never
128. Two names for two events, written into `[conf.trap.stack]`.

## 3. The rows

Every row was seen red first (`a49ddaa2` is the control's record).

**#866, the control at `9da7bdc`** (`rows-control.log`, six depths
50k–300k, three of them the contract's "three argument shapes" above
the limit): main-thread recursion, native: 50k lives, 100k+ exit 139,
no report; release: 150k lives, 200k+ exit 139; a task's thread,
native: 50k lives, 100k+ exit 134 with the legacy line and no
`wolf-trap`; release: 150k lives, 200k+ the same. **At the head**
(`rows-handler.log`): the same depths, the same boundaries, every death
`wolf-trap: stack-overflow` / `  at pc=0x…` / exit 134, on both tiers
and both threads. The fallback worker (`WOLF_TASK_STACK=4 EiB`, so
`mmap` fails and the task runs on a `std::thread`): control 139 on
both tiers, head 134 with the trap line (`fallback.log`). The trap
depth per tier, with an 8 MiB stack: native between 50k and 100k
frames of `walk`, release between 150k and 200k — as predicted.

The gate: `crates/wolf_driver/tests/two_machines.rs` —
`stack_overflow_is_named_on_both_tiers_and_threads` builds
`corpus/faults/stack_overflow.lu` (main) and
`fixtures/two_machines/task.lu` (a task) on native and release and
runs each at 200k/250k/300k in the container (macOS: on the disposable
CI runner only, `ulimit -s 8192`), asserting exit 134 and the exact
`wolf-trap: stack-overflow` line; a compile error is never a skip
(`lane_exit::environment_refusal` + the release-tier text). The corpus
program's record is `check: pass` at `phase: wir` (its execution is
the gate's, in isolation), conforms `conf.trap.stack, conf.trap.exit`.
s246's `signal_gaps_lanes` rows did not move (`parent.lu`'s
`overflow-ignored` now prints the trap line and still exits 134;
`sent-segv` is still signal 11).

**#867** (`read-rows.log`, five variants at the control and the head):
`direct` and `alias` (the witness and a rebound root) trap
`use-after-move` at the control, `1 1 1` at the head; `repeat` and
`twice` (a reread in the same activation) are E1001 at both — the
compiler's half is untouched; `copy` is `1 1 1` at both. The gate:
`crates/wolf_mem/tests/read_views.rs`, six checked rows (survives,
rebound root survives, still moves inside its activation, spans name
one place, a copied container is not a read view, header methods do
not read moved elements), and `two_machines::
indexed_read_keeps_the_callers_field` runs the corpus program on all
three tiers through `conform-run --json` and asserts `exit(0)` and
`1 1 1`. lupin 0.1.55 prints `1 1 1` (the differ agrees).

## 4. The gauntlet

Measured on kasumi (linux/x86-64, CachyOS, podman 6.1.3, the
`archlinux:base-devel` image of 2026-09-21) under `taskset -c 4-7`, the
width CI has, at `05d915d9` (the code-final head; the commits after it
are this note). The hosts' load was 17–27 across the run (`uptime` in
every log under `~/lanes/s256/ev/`).

**`cargo xtask ci`** (`ev/gauntlet5.log`, exit 0, 15:01–15:49 UTC):
`fmt`, `fmt-fixpoint`, `clippy`, `rt-none`, `test` (323 test binaries,
0 failed), `fuzz-check`, `deps-check`, `corpus`, `abi-check`,
`debug-check`, `midend-rate`, `bench-gates`, `spec-extract`,
`conformance`, `print-gate`, `diag-catalog`, `doc-catalog`, `fmt-lu`,
`fmt-fuzz`, `audit-surface`, `dist-smoke`, `differ-self`,
`lane-coverage` — "all steps green". `differ-self`: 1,190 files, 0
divergences, 21 in the conservatism ledger, 3 forward pins (s251 at
1,169 files had the same 21/3). `lane-coverage`: floors held
(221/242/242/256/207); 380 static rejections every lane enforces.
`cfg_dumps` unchanged; the lowering ledger +2 rows (the two corpus
programs); the lex/parse declaration snapshots +2 files; anchors +1
(`conf.trap.stack`), none removed.

**The SKIP census** (`RUST_TEST_NOCAPTURE=1`): 7 lines, all the host's
— 4 × `lldb not installed (required on the macOS CI lane)`, 2 × `core
(on): this host's core_pattern pipes cores to a program`, 1 × `the clang
drift witness runs on macOS/aarch64 hosts only`. None in the new rows:
the container was required (`WOLF_*_REQUIRE_CONTAINER=1`) and ran.

**Two reds met on the way, neither a row of this lane's design:**

1. `str_list_fs_native::the_runtime_symbol_table_covers_the_s40_families`
   counts `RT_SYMBOLS`; the new `__wolf_rt_stack_start` made it 251.
   Fixed in `e724c563` (the symbol named in the guard's list). The
   first plant (39789568459) and gauntlet2 both stopped here before
   the new rows ran, so neither was a proof.
2. `signal_gaps_lanes::a_group_send_of_a_fault_the_sender_ignores`
   (s246's row) was red on kasumi with `error: denied` from the checked
   machine — and red the same way at the control 0.2.34, green on
   trunk's CI under docker. The mechanism: `record_in` ran the checked
   machine as the LAST command of the container's `sh -c`, and bash
   5.3.15 (the image's `sh`) execs that command in place; under
   podman/crun the container's pid 1 is a session leader, so
   `os_set_pgid(0)` is EPERM before any row is reached (under docker's
   runc pid 1 is not a session leader, hence trunk's green). `e667a2ad`
   ends the script with `exit $?`, which keeps the machine a child, as
   the compiled rows already were (`; echo rc=$?`). No assertion moved.

**`bench-gates`, the ir-volume ratchet.** The entry shim now calls
`__wolf_rt_stack_start` before `main` in every program, one instruction
on each side of every file's ratio; 36 small files crossed the 5 %
slack (7/17 → 8/18 and the like) and the record had not been
re-recorded since s156 (353 paths recorded, 694 measured). `05d915d9`
re-records the table from kasumi's `cargo xtask bench-gates --record`,
held against CI run 39794860445's own lines at `c9bf10ac`: every path
the gate printed there (36 regressed by name, 336 new, 1 improved)
carries the same counts, the population is the same; the provenance
is in the record's `recorded` field. CI at `05d915d9` (run
39800849317): `694 recorded, 694 measured, 0 new, 0 unmeasured, 0
improved, 0 regressed`.

**The differ** (`ev/differ.log`, `target/debug/xtask differ A
B --control <lupin 0.1.55 archive>`, 1,190 files; the lupin pin is
`294d626`, the archive's printed build pin — the contract's `e642f26`
is not what the archive prints):

| A | lane | divergences | unsupported | forward pins | A ran / B ran / both |
|---|---|---|---|---|---|
| head `05d915d9` | checked | 234 | 101 | 3 | 723 / 908 / 695 of 1,129 |
| head `05d915d9` | native | 248 | 102 | 3 | 728 / 908 / 694 |
| control 0.2.34 `9da7bdc` | checked | 236 | 101 | 3 | 723 / 908 / 695 |

The control arm (the same A against the same interpreter) moved 0
files below the ledger in every pass. Head vs control on the checked
lane: `memory/read_param_indexed_caller` leaves the divergence set
(the control traps `use-after-move`, the head and lupin print `1 1 1`
— #867's row); `memory/distinct_copy_scalar` and
`memory/region_scalar_mut` also leave it and `memory/region_os_cwd_return`
enters it, all three trunk's since 0.2.34 (s252's rows, which 0.2.34
still refuses; s254's `fail(E1010)` row, which lupin 0.1.55 accepts).
Nothing else moved.

**The plant** (ruling #104 R2): `s256-plant` at `4c2c3cc5` = the head
`c9bf10ac` + `55477a8e` (the trap line renamed `wolf-plant:`, the
read-view move written to the caller's arena), `workflow_dispatch -f
jobs=lint-test`, run 39796666337, red in `lint + test + corpus` on
exactly `two_machines::stack_overflow_is_named_on_both_tiers_and_threads`
(exit 134 with the planted line, the trap-line assertion at
`two_machines.rs:143`) and
`two_machines::indexed_read_keeps_the_callers_field`
(`trap(use-after-move)` against `exit(0)`, `:38`); `0 passed; 2
failed`; nothing else red.

**CI.** `tier pr` at `05d915d9` (run 39800849317) printed `xtask ci:
all steps green` at 15:45:32 and was then cut by the job's 45-minute
`timeout-minutes` at 15:46:20: that runner spent 8 minutes before
`fmt` (trunk's and the earlier PR run's: 1–2) and 22.8 in `test`
(trunk's 18.4); the branch's own suites cost 5.9 s (`two_machines`)
and 0.0 s (`read_views`). The run at the final head is the one the PR
names.

## 5. Rulings

- **R-s256-1** (asked; recommendation implemented): the native
  `trap(stack-overflow)` and the checked machine's `unsupported` at
  its 128-frame budget are two events with two names; no depth
  equivalence is claimed. `[conf.trap.stack]` says so.
- **R-s256-2** (asked; recommendation implemented):
  `[mem.tier0.mode.read]` preserves the caller's value across the
  call; a move out of a read view empties the path inside the callee's
  activation only. Fix the checked machine, not the compiler; yew's
  binding stays legal.
- **R-s256-3** (asked; recommendation implemented): the overflow site
  is the fault PC, printed async-signal-safely; a source/function
  lookup (a sorted table of function ranges the emitter writes, read
  with no loader lock) is the follow-up to file.
