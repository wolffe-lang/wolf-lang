# Changelog

## Unreleased

- Long build/run/test compilations show live unit and phase progress on a
  terminal. Non-interactive builds remain silent for 30 seconds without
  output, then emit a heartbeat every 60 seconds and a completion line.
  `WOLF_STATUS_AFTER` overrides the initial idle deadline. `-q`/`--quiet`
  disables the reporter without hiding diagnostics. `--timings` prints
  scoped elapsed contributions; `--timings=json:<file>` writes versioned
  timing records. See [the CLI page](docs/cli.md) for the schema and the
  distinction between wall time and overlapping contributions.

## 0.2.36 — 2026-10-26

C variadics work in both directions on hosted Linux x86-64 and the
freestanding x86-64 target, with `VaList` and the four `va_*` operations.
The invocation name and byte-exact arguments are available through
`env_arg0` and `env_args_bytes`. Static musl links receive those arguments
too: the generated C entry now hands argc, argv and envp to the runtime.
Objects and runtime must come from this release together.

This release pairs with **lupin 0.1.58**, spec pin `a5378e3`, and bundles
**wolf-std at `78b9d3c`**: the thirteen corrected ledger cells, new process
and filesystem wrappers, in-place List helpers and `fs.read_into`.
Lupin's new vendored corpus resolves the stale three-region-row snapshot;
its separate List-element escape limitation remains tracked at
wolf-interp#232.

Exclusive, exact-layout, single-use internal aggregate read lends can
forward the owner's address instead of physically snapshotting it
(s264). Value semantics and all conservative copy boundaries remain;
there is no new memory rule or ABI class.

The four host archives now have a fifth companion asset,
`wolf-rt-0.2.36-x86_64-unknown-linux-musl.tar.gz`, containing the musl
runtime, not a musl compiler executable. The musl link bundle is
`link-bundle-wolf-0.2.36` on the ligature release page. Its staged archive
`ld02-musl-link-bundle.tar.gz` has SHA-256
`dd09895a13d3ea4056db73d3e974f025a732391c38044094ea6fc8ed69fb54cc`;
its `libwolf_rt.a` has SHA-256
`3d6f821ae70ebb40d106f31c6a95ee46c0c3626d9bf230f43c1f0b8c7c26eb24`.
The archive's digest and the separately built runtime archive's digest
are distinct identities; use each artifact's own published digest.

s267 — the static entry (#962).

- **A static musl link reads its arguments** (#962): a wolf program linked
  statically against musl (ligature's link bundle, by `ld.lld` or GNU `ld`)
  read `argc=0`, an empty `env_arg0()` and no `env_args_bytes()` on both
  tiers, while the same object linked against glibc read every argument.
  The entry the compiler emits is now `int main(int argc, char **argv,
  char **envp)` and its first call hands the three to the runtime
  (`__wolf_rt_main_start`, `[abi.target.main]`), so the argv readers answer
  what the program was started with on every unix C library. glibc links
  answer byte for byte as before, Windows keeps the host's own arguments,
  and `env_vars()`, right under musl already, still reads the live
  environment. The call replaces the entry's `__wolf_rt_stack_start()`,
  so the entry has the instructions it had. The published link bundle's
  runtime is 0.2.35's: a new bundle carries the fixed `libwolf_rt.a`.

- **Choose a cc-shaped linker on native Linux**: `--linker PATH` overrides
  nonempty `WOLF_LD`, then the unchanged platform link path. Selected-tool
  errors and malformed or foreign ELF output are final, without cc fallback.
  Paths with spaces are passed intact. Assembly and release LLVM code
  generation remain separate; Windows, Mach-O and foreign-target support
  are not expanded.

- **Choose the assembler for root-manifest assembly sources** (#937):
  `--assembler PATH` overrides `WOLF_AS`, then the existing target-specific
  C driver. An explicitly selected tool receives the as-shaped argv and is
  never silently replaced after a failure. Freestanding ELF verification
  remains, and explicitly selected Linux assemblers also have their output
  checked. The linker and the release backend's LLVM path are unchanged.

s266 — the small ones (#932, #936, #923, #935, #925).

- **An `else` fallback that swallowed a range says so.** `rest[rest.find(":=") else 0 + 2..]` reads as the fallback `0 + 2..` (`[gram.amb.else]`), an open-ended range the compiler declines; the refusal used to name the slicing ledger and call it "not a bug in your program". It now names "an open-ended or end-relative range as an `else` fallback (slicing)" and adds the note "the `else` fallback extends to the end of the expression; parenthesise `(… else 0)`"; `conform-run` records the note as `x-unsupported-note` (#932, R-s266-1 A).
- **Every spawn and a failed `os_exec` carry the host's number in `os_error()` on every machine** (#936): `ENOENT`, `E2BIG`, `ENOEXEC`, `EACCES`, and on linux `ETXTBSY`, so a shell can say `Text file busy` as dash does (macOS runs a file held open for writing, so it has no such refusal). Two machine-made refusals answered 0 until now and carry the number `execvp(3)` reports: `os_exec`'s own `PATH` search on every machine (R-s266-3 A), and the checked machine's look at a name holding `/` before its exec. `[os.fs.error]` says so.
- **A non-UTF-8 argument through a terminal traps by index on macOS** (#923): 0.2.35 aborted in Rust's `env::args` (SIGABRT); s261's argv reader, in this release, traps `argument-utf8` with `argument N is not UTF-8; read it with env_args_bytes()`, exit 134, as `[os.env.argv]` says (R-s266-2 A). A pty witness runs it on both native tiers.
- **The release publishes the musl runtime**: `wolf-rt-<version>-x86_64-unknown-linux-musl.tar.gz` (`libwolf_rt.a` for `x86_64-unknown-linux-musl`, with its licenses) beside the four host archives, both digests in the release notes (#935). `cargo xtask dist-rt` builds it with PAX's `tools/mkwolf` recipe, and reproduces the PAX image's v0.2.35 runtime byte for byte; a `readback` job hashes every asset of the draft twice and holds the musl digests to the bytes before `publish`; `workflow_dispatch` runs the whole release dry (no tag, nothing uploaded). `libwolf_rt_none.a` is the same bytes on the musl host, so it ships once.
- **The process and signal gates pin to the lane's own CPUs** (#925): `WOLF_TEST_CPUS` (a `taskset -c` list) or the test process's affinity, the first four or the first one of it, `taskset` from `PATH`; labels name the list used, and a request that cannot fit a named allocation fails by name. No gate spells `taskset -c 0-3` or `/usr/bin/taskset` any more.

s265 — the start-up regression (wolf-lang#918).

- **Programs start as fast as on 0.2.34 again.** 0.2.35's entry asked for the main thread's stack bounds before `main` (`pthread_getattr_np`, which reads `/proc/self/maps` through stdio), installed the overflow reporter and unmapped its altstack at exit: a release-tier hello world paid 17 more system calls and about 37 us a start on kasumi (+16 %), every program alike. The bounds are now resolved by the handler on the main thread's first fault, read with `open`/`read`/`close` alone (R-s265-1 B); the altstack lives as long as the process; each fault signal takes one `sigaction`. A hello world now pays 5 calls and about 5 us over 0.2.34. A main-thread overflow still reports `wolf-trap: stack-overflow`, its PC and exit 134 on native and release (`[conf.trap.stack]`).
- **bench-gates has a start-up row**: the system calls of a release-tier `corpus/hello.lu` minus its C twin's (`bench/startup/hello.c`), held at `bench/startup.json` (12); the wall time beside it is printed, report-only.

s263 — C variadics in both directions (#515).

- **Five prelude additions:** `VaList`, `va_start`, `va_arg`, `va_copy`,
  and `va_end`. Declarations shadowing these names may now warn (W0304).
- Hand-declared C calls and exported variadic definitions work on Linux
  x86-64 and `x86_64-unknown-none`, on native and release. Unnamed narrow
  integers promote to `i32`, and `f32` promotes to `f64`; register overflow,
  C-created lists, independent cursor copies and nested forwarding follow
  the SysV ABI. C's decayed `va_list` parameter is spelled `*VaList`.
- Other targets report E0826; the checked machine refuses both directions
  by name. The header importer remains limited to its modelled call set.
- Freestanding nonvariadic code remains SSE-free. Variadic entry saves are
  guarded by incoming AL (or a proven unchanged register copy); a zero
  vector count executes no SSE. The ordinary floating-value ban remains.
- Formatting a malformed first item no longer repeats comments from the
  already-emitted module header.

s261 — the invocation name and byte-exact arguments (#899, #406).

- **Two prelude additions:** `env_arg0() -> str` returns the invocation name, not the resolved executable; `env_args_bytes() -> List[List[byte]]` returns every argument with argv[0] first. Existing `env_args()` still returns argv[1..]. Local declarations with either new prelude name may now warn (W0304).
- Text argv readers report `trap(argument-utf8)` with the OS argument index and a recommendation to use the byte reader, instead of Rust's unwrap panic. The normal native trap status is 134. `env_get` and `env_vars` keep their existing UTF-8 behavior.
- `conform-run --checked` accepts repeated `--argv-hex=HEX`, one byte argument per occurrence, argv[0] first. The freestanding target refuses both new calls by name because it has no host argv.


s260 — the three wrong answers t06 replayed (wolf-lang#642, #682, #683).

- **A copy whose write fails keeps what it read** (`[os.fs.copy]`, #642): the read/write rung writes what the sink takes and puts the source back by the shortfall, so after any `io` the source stands exactly past the bytes written (`cat f > /dev/full` left it at 17 with nothing to retry); the checked machine does the same.
- **No spawn runs a file without `#!` under `/bin/sh`** (`[os.proc.exec]`, #682): a spawn with a `pre_exec` hook (a descriptor map, a job, an inherit set, an ignored `BROKEN_PIPE`) execs the program itself instead of the C library's `execvp`, and off glibc so does every other spawn (macOS's `posix_spawn` falls back too); every spawn and `os_exec` answer `io` with `ENOEXEC` in `os_error()`, on every machine.
- **A killed proc runs no further user code** (`[conc.proc.kill]` step 1, #683): a compiled task parked at any runtime wait (net, channels, `select`, `when`, the signal waits, `time_sleep_ms`, a scope's or a proc's join) leaves at the park — its frames discarded, the entry answering cancelled — instead of running its `else` arm and the code after; `time_sleep_ms` in a task is a sleep the kill ends; `os_wait` leaves when the host returns (`docs/s260-the-three-wrong-answers.md`).

## 0.2.35 — 2026-10-23

Mixed-payload enums and rows now compile natively and on the release
tier. This is the release for `std.json`, and for an enum such as
`enum Value { Int(int), Text(str) }` that the compiler previously
refused because its payload slots disagreed. Each conflicting slot is
a union, sized for its largest member and aligned for its strictest
member; the active variant writes and reads its own typed view.
Uniform slots keep their old layout. Copying an enum whose payload
reaches the heap uses a per-layout, tag-switched copy function;
recursive enums copy recursively, and maps of such values copy their
entries. The checked machine and lupin already ran these programs
(`[abi.native.enum]`, s257).

A member read or method call on `!T` is now E0409 at the base, not
"cannot compile this yet". `rest.len` and `rest.starts_with("4")` on a
`str ! {none}` must handle the row: the diagnostic names `rest?.len`
and `(rest else "").len`. The method check precedes impl lookup,
including a blanket impl. This closes the refusal that sent a caller
looking for a missing builtin implementation when its operand was an
unhandled row (`[type.row.operand]`, s258).

### A region per loop turn

A region block can write a scalar or an explicit materializing `copy`
into an outer value without keeping that block's data. The copy is
made in the outer value's region, including fresh string bytes and
container buffers; container growth uses the container's own region.
A call passing `mut` is admitted when the callee is ambient-clean,
proved callee-first over the build and to a fixpoint for recursion.
Other keeping stores stay E1010/E1004, with the store and repair named.
`wolf conform-run --dump=mut-clean` prints each parameter's verdict
and first keeping store (`[mem.region.outer-write]`, s255).

`List.reserve(n)` grows capacity without changing length or elements;
`List.reset()` empties the list and keeps its buffer. A long-lived
list can therefore be rebuilt inside a per-turn region by resetting
and pushing, rather than replacing its header and retaining another
buffer until the outer region ends. `clear` makes no buffer-retention
promise. An outer store's materializing copy pays a deep copy in the
outer region; programs without that store pay nothing for this rule.

### Faults and portability

Physical stack overflow now reports `wolf-trap: stack-overflow`, the
fault PC and exit 134 on the main thread and runtime threads, on
native and release. Unix uses one process-wide SIGSEGV/SIGBUS handler,
per-thread guard bounds and alternate stacks; the report performs no
allocation, symbol lookup or unwinding. The checked machine's bounded
recursion refusal is a different event, not a promise to reach the
same depth. A move through a checked `read` view now empties the
callee's path only, preserving the caller's container as both compiled
tiers do (`[conf.trap.stack]`, `[mem.tier0.mode.read]`, s256).

The musl portability patch is applied in this release (#886). The
checked machine and runtime make the C locale with `newlocale(0, …)`
rather than a libc-specific `LC_ALL_MASK` binding. Retired pthread
handles use an opaque, ownership-carrying wrapper that moves to one
joiner, never a pointer to dereference. CI's Linux lint job checks
the workspace for `x86_64-unknown-linux-musl`; no libc feature change
was needed. This is a compile-portability fix, not a new compiler
archive: the four existing archive targets remain, and #887 tracks
the separate musl archive.

### Refusals name their source

A spanned conservatism refusal now shows the source file, line,
column and caret, rather than file-relative offsets detached from
their file. JSON retains its existing fields and adds
`x-unsupported-file`, `x-unsupported-line` and `x-unsupported-col`;
backend refusals with no source identity do not invent one. An
unresolved container argument such as `Map[str, Child]()` is E0301 at
`Child`, with a note to import and qualify the type. Tuple and rigid
generic arguments that already worked keep working; remaining
declined bracket shapes are named individually (s259).

### The rest of the cycle

Cross-module closure and function-value records now resolve their
out-of-object vtable slots by the same imported symbol a function
address uses, on both backends. Deduplication pins the functions those
records name. This fixes the release-tier refusals for a closure made
in one module and called from another, including an inlined maker;
partitioning and the inliner are unchanged (s251).

An adapter of a Copy scalar is Copy; an adapter of a string, container
or struct still moves. Module state of `wrapping[T]` is static data.
A value whose type reaches no storage names no region, so scalar-only
structs, enums and rows can leave a region or survive its freeze.
Storage-holding values retain their region rules. The diagnostic for
`~x` names integer `!x`; a brace on the next line after a control
header gets the joining edit; and a read-parameter reuse names the
parameter and the two ways to read twice (s252).

`wolf test --nocapture` exposes both output streams, passing SKIPs
included. A native fallback is named on its row and counted in the
summary; JSON adds the refusal in `native` and a native count.
`--no-native-fallback` makes a checked-only suite explicit. Under the
test runner the checked machine refuses `os_exe()` by name, so a main
test that re-executes itself runs natively as its own executable.
Exit status is unchanged. `wolf fmt` is a fixed point on member calls
whose arguments break across lines, with `fmt-fixpoint` in the gauntlet
(s250).

The process tier adds core-limit reads and changes, a no-core exit,
an inherited descriptor adopted into the handle table, and host OS
and architecture names. The C library's reserved signal numbers answer
`invalid` before a host call. The filesystem tier adds access checks,
locks, a handle's stat record, FIFOs, non-blocking mode and partial
pipe writes; open mode bit 16 refuses a final symlink, and bit 62 in a
wait descriptor asks for write readiness. `os_cwd()` allocates in the
ambient region, so keeping it past that region is E1010 (s253, s254).

| prelude surface | 0.2.34 | 0.2.35 |
|---|---:|---:|
| functions | 154 | 167 |
| W0304 names | 186 | 199 |
| all names | 194 | 207 |
| spec anchors | 672 | 693 |

The new function names are `fs_access`, `fs_adopt`, `fs_fstat_record`,
`fs_lock`, `fs_mkfifo`, `fs_set_nonblock`, `fs_unlock`, `fs_write_some`,
`os_arch`, `os_core_limit`, `os_core_off`, `os_name` and
`os_set_core_limit`. No prelude name was removed.

### The bundled std and pairing

The archive bundles wolf-std
`2386522eb059cca5a84108562c2c0a0e72345d34` (sc70), replacing
`e036bec318ad81ecd5a025612bc2c2adb17527f3`. It includes sc69's editor,
process, sync and path wrappers and sc70's local civil time, TZif and
POSIX rule reader, conversion and C-locale formatting in `std.time`.
The staged `std/STD-REV` names that exact commit; no std ledger cell
was edited for this compiler release.

| bundled-std measurement | result |
|---|---|
| std-test entry files | 524 |
| changed ledger cells | 13; named below, not silently flipped |
| doc-examples | 533 blocks, 1,599 lane observations, GREEN |
| std.json on native / release | 9 / 9 run their directives, matching outputs |
| named Linux host skips | 2, directory_accounts on checked and native |

The fixed std's aggregate `std-test` is RED on the cells below, not a
claimed green run. The native JSON graduations belong to sc72 after
this release; the other interpreter movements are findings to review
at its paired pin, not waivers added here.

| file under tests/ | lane | ledger at 2386522e | observed |
|---|---|---|---|
| fs/sync_rows.lu | lupin | unsupported | run |
| json/navigation.lu | native | unsupported | run |
| json/navigation_rows.lu | native | unsupported | run |
| json/number_posture_and_depth.lu | native | unsupported | run |
| json/parse_misses.lu | native | unsupported | run |
| json/parse_shapes.lu | native | unsupported | run |
| json/pretty_and_escape.lu | native | unsupported | run |
| json/stringify_shapes.lu | native | unsupported | run |
| json/unescape_shapes.lu | native | unsupported | run |
| json/values_and_accessors.lu | native | unsupported | run |
| term/mode_bits_set.lu | lupin | divergent(stdout) | unsupported |
| term/restore_rows.lu | lupin | unsupported | run |
| time/monotonic_ns.lu | lupin | unsupported | run |

The first isolated-container run also reported a directive mismatch on
`process/join_own_group.lu` on checked and native: the inherited process
group was outside the PID namespace, so its own group read as zero.
The published compiler and the candidate gave the same result there;
both passed the unchanged directive when the container established its
own session. Those original reds and the session-controlled retakes are
retained, not ledger changes. `os/users/directory_accounts.lu` prints
its named skip where the directory service is absent; the interpreter
refuses the user-id call by name instead.

The paired interpreter is lupin **0.1.56**, with its spec/corpus pin
unchanged at `294d626`. It serves real OS entropy even under `--seed`,
mirrors the scalar-region and wrapping rules, and resolves the earlier
editor/storage names. It still refuses canonical paths outside its
served tree and the host operations it cannot implement. Its process
and filesystem gaps from this cycle, outer-region materializing
copies, the two List methods and the row-member rule remain named
lag, measured before carrying any version-keyed entry. The six
outer-copy SOUNDNESS-class rows are wolf-interp#289, carried only for
published lupin 0.1.55 and 0.1.56 records; unlisted releases must mirror
the materializing copy. is81 takes that debt with #287's row-member
mirror. The full
published-0.2.34 compiler control and the published-0.1.55 interpreter
control are retained; movement is not attributed to the new pairing
by assumption.

### Downstream at 0.2.35

| repository | next step |
|---|---|
| wolf-std | sc72 re-pin; #92 in-place helpers on reset/reserve; #93 JSON header and the measured native ledger graduations; #94; #91 README |
| wolf-interp | is81: #289 outer-region materializing copies and List.reserve/reset, with #287 row-member E0409 |
| pelt | update the wolf, lupin and std pins |
| lope | update the pins |
| aspen | update the pins and adopt std.time where appropriate |
| boreutils | update the pins; measure the published pair on both tiers |
| lobo | update the pins |
| yew | yw06 pin; #40 prompt-menu refill can use the new region/list rules |
| wolf-book | #92 graduate ch27/ex27-3; update the release pins |
| wolf-web | #68 retire the carried portability patch at lupin 0.1.56 |
| wolf-lsp | absorb the measured prelude delta |
| tree-sitter-wolf | absorb the measured prelude delta |
| pax | next pins lane; consume #887's compiler archive only when published |
| muzzle | update the pins |

### Changes recorded before the release pairing

- s259 (#889): conservatism refusals show the source file, line, column and caret; JSON records retain their fields and add the refusal's location.
- s259 (#890): an unresolved container type argument is E0301 at the name, not a refusal; remaining declined bracket shapes are named individually.

s258 — a member read on a `!T` is E0409 (wolf-lang#879). `rest.len`
and `rest.starts_with("4")` on a `str ! {none}` were refused as "cannot
compile this yet" (members on this type, methods on builtin types; exit
4); they are E0409 at the base now (`[type.row.operand]`: a member read
and a method call read their base as an operator reads its operand),
with the note naming `rest?.len` and `(rest else "").len`. The method
refusal comes before any impl is consulted, a blanket one included.
Rows: `rows/negative/row_operand_member.lu`, `row_operand_method.lu`,
`rows/row_member_handled.lu`; `row_member_lanes.rs` holds the checked
machine, native and release to them. lupin 0.1.55 runs both witnesses
(pinned pre-mirror; wolf-interp owes the mirror). A `shared` cell of a
`!T` still answers its own refusal (`docs/s258-row-member.md`).

s255 — the loop's region (wolf-lang#839, #856, #821, #844; ruling #112,
the maintainer: "if we've hit the same wall twice in real projects,
maybe it's time for the language adjustment"). The wall, met by yew's
editor loop and aspen's directory walk: a `region` block refused every
write into a value living outside it, so a loop whose turn builds
temporaries could not open a region per turn when the turn also wrote
to something long-lived. One clause, two `List` methods, two witnesses.

- **A write into an outer value keeps nothing of the block when the
  stored value names no region or is written `copy`**
  (`[mem.region.outer-write]`). Inside a `region`/`in` block, a field
  store, a store through a `mut` parameter, a `push`, an index store or
  a `Map` store into a value declared outside the block is admitted when
  the stored value is a scalar or a literal, or is written `copy e` at
  the store — and that `copy` is the materializing copy `copy region
  { }` makes (every `str` re-made, every container a fresh buffer),
  made in the OUTER value's region: a container's own (the region its
  header remembers and its growth uses) for a `push`, an index or `Map`
  store (the key included) or a `List`/`Map` replaced; the region
  ambient where the binding was declared for any other field (a
  parameter's: the caller's region). A container's growth inside the
  block is its own region's too ("even a byte push" was E1010 before).
  A call inside the block passing a `mut` argument gives it the
  block's region unless the callee's parameter is **ambient-clean** —
  proven by the callee's own lowering: scalar stores, container stores
  of scalars or `copy`s, containers replaced by `copy`, calls to clean
  parameters and the storing-nothing mutators only (a part moved out
  and given back is judged by what comes back); computed callee-first
  over the build with a fixpoint per recursive component (aspen's
  `walk_dir`). A parameter is judged against its OWN region: another
  argument's value (a `take` struct pushed, a fn value stored) or a
  region parameter's region is not the parameter's and keeps it unclean
  — the first verdict read only the callee's ambient and let s227's
  `region_kept_arg_take` and `region_kept_arg_closure` through, which
  `cargo xtask corpus` at the head caught. `wolf conform-run --dump=mut-clean` prints every
  parameter's verdict with its first keeping store. Everything else stays
  E1010/E1004, and the report names the store and the one-word repair,
  or the callee's keeping store. The checked machine charges the copies
  to the outer region's ledger (`region_outer_write_charged.lu`); the
  turn region runs flat at 100,000 and 200,000 turns on both native
  tiers (`region_outer_write_lanes`). Cost: a `copy` at an outer store
  pays one deep copy in the outer region where it used to share a
  `str`'s bytes; nothing for a program without such a write.
- **`List.reserve(n)` and `List.reset()`** (`[type.list.storage]`,
  #844, #821): `reserve` grows the capacity to at least `n` in the
  list's own region, the length and elements untouched, a no-op when
  the capacity is there; `reset` empties the list and KEEPS its buffer
  (`clear` promises nothing of it), so a long-lived list is rebuilt by
  `reset` then pushes — yew's spare-pair swap as one method — and with
  the clause the rebuild runs inside a per-turn block. The rule the
  names state: a `List` field replaced by assignment holds its old
  storage until its region ends. `RT_SYMBOLS` 251
  (`__wolf_rt_list_reserve`); host table and prelude unchanged.
- **Corpus**: `region_outer_write_{scalar,str_push,struct,field,map,
  index,call,reset,charged}.lu` run on every machine; `_refused.lu`
  (a plain `str` push, E1010), `_refused_field.lu` (a plain field store,
  E1004), `_call_refused.lu` (an unclean callee, E1010 naming its
  store); `list_reserve.lu`, `list_reset.lu`. `region_scalar_list_mut.lu`
  holds an unclean callee now (its `int`-push shape, refused by the
  signature rule alone, is `region_outer_write_call.lu`'s run row);
  `region_escape_container.lu` reports one site (the reassigned header),
  not two. Two refusals are run rows now, each a `copy` at a store into
  an outer value that the clause makes the materializing copy:
  `region_copy_shares_str.lu` (s227's door, `in st { o = copy d }`) and
  `region_str_call_index_store.lu` (s216's, #612's first witness,
  `st.vals["x"] = copy v` inside the callee's own block); lupin's
  column on both is `region-fault`. No other row moved verdict.
- **lupin 0.1.55** shares `copy`'s bytes: `region-fault` where a copied
  `str` is read back after its block (`_str_push`, `_struct`, `_field`,
  `_map`); the same answers on the scalar, index, clean-call and ledger
  rows; `reserve` and `reset` unresolved (`unsupported`). wolf-interp
  owes the clause and the two methods.
- **Measured** (s255's note, `docs/s255-the-loops-region.md`): the
  downstream census at `--phase=mem` over boreutils, lobo, pelt,
  wolf-std, yew and aspen; yew's editor loop and aspen's walk before and
  after on their `s255-witness` branches.

s257 — the spilled union (wolf-lang#268 row 1, c06's residue). An enum
whose variants carry payloads of different types, and a row whose tags
do, compile on every tier.

- **A payload slot whose types disagree is a union** (`[abi.native.enum]`):
  `enum V { Int(int), Text(str) }`, `error E = {Parse(int), Io(str)}`
  and `std.json`'s `Value` (null/bool/int/float/str/list/map, recursive)
  build and run natively and on the release tier. The slot's storage is
  the largest member at the strictest alignment; each variant writes
  and reads its own member through a typed view at the slot's start.
  Per slot: a slot every variant agrees on keeps the layout it had, so
  every program that built before builds to the same objects. Before,
  every compiled tier refused by name ("enum payload slots with
  conflicting types across variants", "row payload slots with
  conflicting types across tags", `unsupported`); the checked machine
  and lupin ran such programs all along. WIR gains `union{A, B}`,
  `union.make`, `union.view` and the blob forms `load.union`/
  `store.union`; row widening at `?` moves a payload into the wider
  row's union instead of refusing a non-prefix. Rows:
  `typecheck/enum_union_{two,three,tree}.lu`,
  `rows/row_union_payloads.lu`, `memory/enum_union_{take,copy,str_escape}.lu`.

s252 — the small gaps (wolf-lang#800, #813, #814, #816, #822, #801; yew
`docs/MAP.md` §5 G11, G18, G24, G25, G28 and the fuzz fork's findings).
Three behaviour changes, each a refusal removed with the spec's clause
saying why, and the rest decided by design with the spec's sentence, a
better report and a row.

- **An adapter of a `Copy` scalar is `Copy`** (#813,
  `[mem.tier0.move.3]`): `type ByteOff = distinct int` copies on what
  would otherwise move, as `int` does, and so does an adapter of any
  integer type, `wrapping[T]`, a float, `bool`, `char` or `byte`. An
  adapter of `str`, a container or a struct still moves. Before, `let x
  = a as int` then `let y = a as int` was E1001 and nothing could opt
  in.
- **Module state of `wrapping[T]` is static data** (#814,
  `[mem.static.3]`): `const PRIME: wrapping[u64] =
  11400714819323198485`, and a `let` or `var` of a wrapping type, build
  on every machine; a `const` folds with the type's wrap. Before, the
  package was refused by name ("not static data", `unsupported`), read
  or not.
- **A value whose type reaches no storage names no region** (#816,
  #801, #822's first finding; `[mem.region.scalar]`, anchors 673): a
  scalar, unit, an adapter of one, or a tuple, struct, enum or row of
  such. Built inside a region and held outside, returned, stored into
  another parameter's region, passed `mut` inside a region block (a
  one-`int` RNG state), bound by a `for` over a region's list of them,
  or read out of a frozen region, it ties itself to no region: no
  E1010, E1004 or E1012 is about it. A struct with a `str` or `List`
  field keeps every rule. And `copy e` of a non-place (a call result) is
  the fresh ambient value its lowering always made, so `m.s = copy
  g(mut t)` — the `copy` E1004's note asks for — builds. Measured over
  boreutils, lobo, pelt, wolf-std and yew (1,013 files, trunk against
  head at `--phase=mem`): two verdicts moved, yew's two witnesses; no
  diagnostic or warning moved anywhere else. Three old rows that had
  used a scalar-only struct as a stand-in for region data hold a `List`
  now.
- **By design, with the report saying so**: `~x` (#800) is E0107 —
  wolf's bitwise not is `!` on an integer (`[type.int.not]`, ruling #51;
  ruling #99 R1 withdrawn), and the report offers `!x` as a
  machine-applicable edit and the parser reads on without a second
  error; `let tb = run.ec.tb` then `run.ec.tb` again on a `read`
  parameter (#822) is E1001, whose report now names the parameter and
  the two ways to read twice; a `{` on the line after an `if`, `while`,
  `for` or `loop` header (#822) is E0201 (`[gram.lex.newline]`,
  `[gram.fmt.brace]`), whose report labels the brace and joins it to
  the header with a machine-applicable edit; a local `let clock_ms`
  (#822) is W0304 (`[conf.resolve.ambient]`); a counter inferred
  `wrapping[u64]` from a shift count never ends a `>= 0` countdown
  (#822, `[type.numlit.value]`: annotate it and cast at the use).
  `wolf --explain` carries each.
- **lupin** (0.1.55, the pairing, and 0.1.54 alike): owes `[mem.static.3]`'s `wrapping[T]` (it answers
  `unsupported`) and `[mem.region.scalar]` (it traps `region-fault` on
  the literal, binding, enum, `for`, `take`-store and frozen-write
  shapes); diverges on the countdown (it accepts an `int` count on a
  wrapping shift, where the three compiling machines say E0401).
  `small_gaps_lanes.rs` asserts its column.

s250 — `wolf test` tells the truth (wolf-lang#817, #810, #809; yew
yw02 §3, G22, G23, G29). Where a test ran and what it printed, said by
the runner; and `wolf fmt` a fixed point, with a gate.

- **The native fallback is named and counted** (#817). A `main` test
  the checked machine refuses (its 20M-step budget, a construct outside
  its surface) runs once natively, as since s73 — and now its row says
  so: `ok [native: step budget exhausted]`, the refusal verbatim; the
  summary counts it (`N passed, M of them native`, the line unchanged
  when none is); a stderr note says what a native pass is. `--json`
  keeps `detail: "exit(0) [native]"` and gains `native` (the refusal) on
  the event and a `native` count in the summary. **The exit status is
  unchanged**: this is reporting, not a new failure. `--no-native-fallback`
  makes the refusal the verdict (`unsupported`, exit 1) for a suite that
  wants the checked machine or nothing. yw02 read ten of twenty files
  green that the checked machine never finished; each now reads
  `[native: step budget exhausted]`.
- **`--nocapture`** (#810; RULES.md §5, #571). A test's standard output
  and standard error are shown as the test writes them, a passing test's
  `SKIP:` lines included: the checked machine echoes each write, a
  native run's pipes are pumped, and each stream goes to its own (under
  `--json` both to stderr, and every test event carries `stdout` and
  `stderr`, a passing one too). 0.2.34 answers `unknown flag`.
- **The child role: `os_exe()` under `wolf test`** (#810, yew G23,
  `[os.proc.exe]`). On the checked machine it named the `wolf` binary,
  so a test re-spawning the program under test started the compiler
  ("`--child` is not a wolf command"). Under `wolf test` the checked
  machine now refuses it by name; a `main` test then runs natively,
  where `os_exe()` is the test's own executable — spawn it with an
  argument of your own and read the child's status — and its row says
  `[native: …]`. A `test_*` fn, which runs only on the checked machine,
  is `unsupported`. Outside `wolf test` (`conform-run --checked`, `wolf
  run --checked`) nothing changes. Doctests run with the same posture.
- **`wolf fmt` is a fixed point on a member call over a block** (#809,
  yew G22, `[gram.fmt.break]` (2)). A call like `log.put(if k { a } else
  { b })` or `t.g(c, if k == "…" then 1 else 2)` whose argument breaks
  multiline (an inline block that no longer fits, a bare `if` broken to
  braces) kept `t.g(` on the first pass; the second read the braces' hard
  newlines as forcing the receiver-dot break and laid `t.` / `g(`. A hard
  break inside an argument list is now that list's break: the receiver
  dot stays a last resort. Over every `.lu` file of the org's thirteen other
  repositories (3,602, kasumi), the first pass is byte-identical to
  0.2.33's (0.2.34's formatter is 0.2.33's) on every one; eight pax
  kernel files that 0.2.33 did not
  bring to a fixed point (`kernel/pci/pci.lu`'s `log.put(if …)`, and
  seven more) now converge in one pass; yew's `src/term_render_test.lu`
  at 688613c (the issue's file) too.
- **`cargo xtask fmt-fixpoint`**, the gauntlet's second step: `wolf fmt`
  twice over every corpus file, the std at STD-PIN and the formatter's
  regression corpus (1,247 files at this pin: 1,170, 54 and 23), the
  second pass changing nothing; any extra path is formatted too.
  `fmt-lu` asks whether the corpus is canonical; this asks whether
  canonical is where the formatter stops.

s254 — the fs gaps and the pipe write (wolf-lang#820, #818, #834; yew
yw02 G30, G32; pelt sh22). What an editor's save path, journal and
clipboard met as absences, and a shell's working directory kept in the
root; the spellings beyond the contract are rulings asked at audit
(`[os.fs.gaps.note]` R1–R8), each recommendation what is built.

- **`fs_access(path, mode) -> () ! {denied, invalid, io, not_found,
  unsupported}`** (#820, `[os.fs.access]`; R1): `access(2)` with the real
  ids — 0 exists, 4 read, 2 write, 1 execute; `EROFS` is `denied`;
  windows `unsupported`.
- **Bit 16 of the open mode, `O_NOFOLLOW`** (#820,
  `[os.fs.open.nofollow]`; R2): `fs_open_mode` and `fs_open_perm` refuse
  a FINAL symbolic link `invalid` (`ELOOP` in `os_error()`) and follow a
  middle one; windows refuses the bit.
- **`fs_lock(fd, kind) -> () ! {busy, invalid, io, unsupported}`** and
  **`fs_unlock(fd) -> () ! {io, unsupported}`** (#820, `[os.fs.lock]`;
  R3): an advisory lock on the open file (`flock`; `LockFileEx` on
  windows, mandatory there), 1 shared, 2 exclusive, tried once — `busy`
  held through another open, in this process or another.
- **`fs_fstat_record(fd) -> List[int] ! {io}`** (#820,
  `[os.fs.fstat.record]`; R4): `fs_stat`'s twenty words from a handle —
  its identity (`dev`, `ino`) with no path walked.
- **`fs_mkfifo(path, perm) -> () ! {denied, exists, invalid, io,
  not_found, unsupported}`** (#820, `[os.fs.mkfifo]`): `mkfifo(3)`; a
  file system that makes no fifo (`EPERM`, `ENOSYS`, `EOPNOTSUPP`) and
  windows `unsupported`.
- **`fs_set_nonblock(fd, on) -> () ! {invalid, io, unsupported}`** and
  **`fs_write_some(fd, bytes) -> int ! {busy, invalid, io}`** (#818,
  `[os.fs.nonblock]`; R5, R7): `O_NONBLOCK` on a handle (0..2 `invalid`,
  windows `unsupported`), and one write that answers the count taken —
  `busy` (`EAGAIN`) when a non-blocking handle takes nothing.
- **`os_signal_wait_fds` waits for write readiness** (#818,
  `[os.signal.wait.write]`; R6): an element `h + (1 << 62)` asks whether
  `h` takes a write (`POLLOUT`; a reader gone is ready). yew's clipboard
  witness, which parked for ever at 0.2.34, now takes the pipe's 65536
  bytes, answers `busy`, waits out a 25 ms deadline and gives up.
- **`os_cwd()`'s answer lands in the region it is asked in** (#834,
  `[mem.region.escape]`): it went to the process root, so a loop asking
  it inside `region { }` kept the name every turn — 32 B a call for a
  28-byte directory on both tiers, 0 now (pelt sh22 read 48 B for 39).
  `os_cwd` is a site first (#374's order), so its answer returned out of
  the region it was asked in is E1010; the checked machine charges it to
  the region.

s253 — the process calls (wolf-lang#811, #812, #815, #819; boreutils
bu28, yew yw02 G26 and G31, pax's sigwolf). Five calls and one refusal
by name; the spellings beyond the contract are rulings asked at audit
(`[os.process.calls.note]` R1–R7), each recommendation what is built.

- **`os_core_limit() -> (int, int) ! {unsupported}`**,
  **`os_set_core_limit(soft, hard) -> () ! {denied, invalid,
  unsupported}`** and **`os_core_off() -> () ! {unsupported}`** (#811,
  `[os.proc.core]`; R1, R2): the core limit read (-1 unlimited; read
  first, the limit the process was born with) and set (the shape first:
  each -1 or 0 and up, the soft at most the hard; `denied` for a raise
  the host refuses), and an end that dumps no core on any host or
  pattern — on linux the "dumpable" attribute 0, GNU `timeout`'s own call
  (measured), the one switch a pattern that pipes cores obeys (core(5):
  the limit is not enforced there), set back by `exec`; elsewhere the
  soft limit 0. `[os.proc.status]` now says what a core in a status means
  when the child's limit is 0 (ruling #88 R3): a dump the host piped to a
  program, which may keep nothing.
- **The C library's own signals are refused by name** (#812,
  `[os.signal.host.reserved]`; R3): the handler on 33 a wolf program
  holds is glibc's, installed when a process starts its second thread
  (nptl(7); measured black-box on glibc 2.39 and 2.44, none on musl) —
  the runtime installs nothing on 32–34. `os_signal_ignore_one`,
  `os_signal_default_one` and `os_exit_signal` answer **`invalid`** for
  the C library's numbers (linux: 32 up to `os_signal_realtime()`'s
  first; glibc's 32 and 33, musl's 32 to 34) on every machine; the sends
  and `os_signal_text` take them as before.
- **`fs_adopt(fd) -> int ! {invalid, io, unsupported}`** (#815,
  `[os.proc.adopt]`; R4): a descriptor this process inherited, 3 or
  above, made an fs handle that owns it and closes it at `fs_close`,
  marked close-on-exec on adoption. What may be adopted is open and not
  close-on-exec; every descriptor the runtime makes is close-on-exec, so
  its own and one adopted before answer `io`; 0–2 and numbers past i32
  are `invalid`; windows `unsupported`.
- **`os_name() -> str`** and **`os_arch() -> str`** (#819, `[os.uname]`;
  R5): the OS and the architecture the program runs on, as the kernel
  answers (`uname(2)`, lowercased; Darwin is `macos`, `arm64` is
  `aarch64`, `amd64` is `x86_64`) — `linux`, `macos`, `freebsd`, `pax` on
  PAX; windows `windows` and the build's architecture.
- Charges (R6): the three reads `env`, the limit's set and `os_core_off`
  `exec`, `fs_adopt` `fs`; all six refused at comptime. The checked
  machine answers each on the `wolf` process it runs in (R7).
- spec/11 gains `[os.proc.core]`, `[os.proc.adopt]`,
  `[os.signal.host.reserved]`, §11 `[os.uname]` and
  `[os.process.calls.note]` (anchors 686 with s252's and s254's); the host table is 160 lines with s254's seven.

### Read this before you bump the pin

- **`wolf test`'s rows and summary** (s250): a native pass reads `ok
  [native: …]` and the summary `N passed, M of them native; …`; a tool
  reading the human output for `N passed;` sees the count only when a
  test fell back, and `--json` is the stable surface (`native`, new
  keys only). Under `wolf test` the checked machine refuses `os_exe()`:
  a `test_*` fn that called it is now `unsupported`, a `main` test runs
  natively. Exit statuses are unchanged.
- **`wolf fmt`** (s250): a member call over an argument that breaks
  across lines keeps `t.g(` where 0.2.34 sometimes laid `t.` / `g(` on a
  second pass; no file in the org moved on its first pass.
- **Seven new prelude names**: `fs_access`, `fs_lock`, `fs_unlock`,
  `fs_fstat_record`, `fs_mkfifo`, `fs_set_nonblock`, `fs_write_some` (a
  function of one of these names now shadows a builtin, W0304). The
  `busy` mark is answered by `fs_lock` and `fs_write_some` beside
  `os_fork`.
- **An open mode with bit 16 set is no longer `invalid`** on unix: it
  opens without following a final link. Any other bit above the mode
  stays `invalid`; windows refuses bit 16.
- **An element of `os_signal_wait_fds`'s list with bit 62 set is no
  longer `io`**: it waits for the handle to take a write.
- **`os_cwd()` asked inside a region and kept past it is E1010.** Its
  bytes now live in that region; make the call outside the region, or
  copy the answer out with `copy region { }`. Measured before it landed:
  pelt, boreutils, yew and lobo (341 builds, the published 0.2.34 against the head) answer with the same verdicts and diagnostics — no site newly refused.
- **Six new prelude names** (s253): `os_core_limit`, `os_set_core_limit`,
  `os_core_off`, `fs_adopt`, `os_name`, `os_arch` (a function of one of
  these names now shadows a builtin, W0304).
- **`os_signal_ignore_one(-32)`, `(-33)` and `os_signal_default_one` of
  them answer `invalid`** (s253) under glibc where 0.2.33 and 0.2.34 answered
  `io` (glibc's `sigaction` refused them); under musl so does `-34`.
- **`os_exit_signal(-32)` and `(-33)` return `invalid`** (s253) where 0.2.33 and
  0.2.34 exited 160 and 161 (glibc never delivered them; measured on both
  tiers, with a thread and without).

### Downstream

- **yew** (s250): the guard and invalid-cell self-spawns' loud SKIP on
  the checked machine (`self_reexecutes`) retires at this pin — those
  files run natively under `wolf test`, their rows say so; `tools/
  check-test` can add `--no-native-fallback` for a checked-only lane and
  `--nocapture` for its SKIP count.
- **wolf-interp** (s250): lupin has no test runner; nothing to mirror in
  the language. If lupin's test surface grows, it mirrors `--nocapture`,
  the `native`-key shape (lupin has no native tier: its refusal is the
  verdict, as `--no-native-fallback`'s), and `os_exe()` refused by name
  under test.
- wolf-interp: resolve the seven names and the open's bit 16 (0.1.55
  refuses the bit as a mode outside its set), the wait's bit 62, and
  `os_cwd`'s place in the region — or refuse each by name.
- wolf-std: wrappers (`fs.access`, `fs.lock`/`fs.unlock`,
  `fs.stat_handle`, `fs.mkfifo`, `fs.set_nonblock`, `fs.write_some`, a
  `signal.WAIT_WRITE`), and `fs.open`'s no-follow flag.
- yew: at its 0.2.35 pin, retire `directory_writable`, the `fs_lstat`
  before the open, the path stat after it, the `mkfifo` utility, and the
  blocking clipboard write (G30, G32).
- pax: `flock` and `mknodat` of a fifo are the two absences the runtime
  answers `unsupported` (px lane).
- **wolf-std** (s253): wrappers for the six (`process.core_limit`,
  `process.set_core_limit`, `process.core_off`, `fs.adopt`, `os.name`,
  `os.arch`, or the std's spellings), and `signal.host(n)` refusing the
  C library's numbers by name (ruling #85's spelling).
- **wolf-interp** (s253): lupin 0.1.55 resolves none of the six names; it owes
  the shape rows and then `getrlimit`/`setrlimit` (std has neither: a
  refusal by name), `os_core_off` refused by name, `fs_adopt` refused by
  name (no `fcntl` in std), `os_name` and `os_arch` from
  `std::env::consts` or refused by name; the reserved numbers' `invalid`
  rows where its one-meaning calls answer the shape.
- **boreutils** (s253): `timeout` may call `os_core_off()` before it ends itself
  by its command's signal, retiring the three `design` rows `core:` in
  `tests/timeout/cases.toml` (GNU's own call, measured); the `-s 33` row
  stays by design (glibc's handler, `[os.signal.host.reserved]`).
- **yew** (s253): `fs_adopt` for the torture child's ready descriptor (G26) and
  `os_name`/`os_arch` for the clipboard's choice (G31), at its 0.2.35 pin.
- **pax** (s253): sigwolf's `L:` rows may read `os_core_limit`; a px lane to
  confirm `os_name` answers `pax` there, `os_core_limit` `(0, -1)`, the
  set `denied` (`prlimit64`'s -EPERM) and `os_core_off` `ok`.

s251 — closures across modules on the release tier (wolf-lang#806,
#807; yew G19/G20). The cause is written before the fix
(`docs/s251-closures-across-modules.md`); the clause is
`[abi.native.closure.object]`.

- **What was refused.** On the release tier of a multi-module program a
  capture-free closure made in one module and called from another could
  refuse `a vtable slot @main.cls0 outside this object's subset`; a
  one-line module function that makes a closure and calls another with
  it could refuse the same after the inliner moved it into the entry;
  and on the native tier a function of one file of a module read as a
  value in another file of it refused `@term.env_getv.fv` the same way.
  The value of a capture-free closure, of a named function read as a
  value and of a trait object's dispatch is a STATIC one-slot record
  (`[abi.native.closure]`, `[abi.native.dyn]`), and its slot was the one
  reference to a module function the emitters resolved only inside the
  object being emitted — a cluster's members on the release tier, a
  module's functions on the native tier — where a call and a
  `func.addr` had imported the mangled symbol since #116 and s134.
  Three things put a slot outside the object that builds the record:
  **dedup** merged a closure's entry into a body-identical twin while
  the record still named it (it pinned `func.addr` referees and never
  read the data table, though the summary had claimed since s98 that it
  did); the **import decision and the inliner** brought a small maker
  across a cluster cut, so the record is built in an object that does
  not own the entry; and the native tier's **per-module partition**
  filed the synthetic `.fv` shim under the root module's object.
- **The fix.** Both emitters resolve an out-of-subset slot through the
  mangled-symbol import `func.addr` takes (a `declare` in the LLVM
  text, an `Import` in the Cranelift object), and dedup pins every name
  a record carries. The partition, the import decision and the inliner
  are untouched — the inliner keeps relocating a closure's maker and
  the slot is placed where every caller can reach it, by symbol — so
  every previously-green object is byte-identical, and the release
  build's size and time on the corpus move only where a program was
  refused before. The native tier's semantics are the oracle: every row
  prints the native tier's output on the release tier and the checked
  machine.
- **Witnesses.** Six corpus rows under `corpus/typecheck/`
  (`closure_entry_to_module`, `closure_module_to_entry`,
  `closure_module_to_module`, `closure_captured_closure`,
  `fn_value_two_file_module`, `closure_inlined_one_liner`), each
  refused at `808ce11a` by the mechanism it pins; yew's seven witnesses
  green on both tiers; `wolf_driver/tests/closure_cross_module.rs` (the
  six under clusters, under `WOLF_MIDEND=0`, on native and on the
  checked machine) and the dedup pin in `wolf_wir/tests/dedup_instances.rs`.
  Anchors 672 → 673. yew can retire `term.Env` (`src/term/tty.lu`),
  the direct `YEW_MOUSE` read (`src/term/input.lu`), the local
  `sort_items` (`src/text/cset.lu`) and the one-`match`-per-registry
  name tables at its next pin.

## 0.2.34 — 2026-10-20

**What this release changes: the fork, and what an editor asks of the
runtime.** s247 gives wolf a fork whose child keeps running wolf code
(ruling #90; its shape ruling #95): `os_fork`, for pelt's subshells
(sh20), which fork from 0.2.34 as dash's do. The fork's rule is
**`busy`**: it answers `busy`, before the host is asked, while any task
is live, from a task or a proc handler, or under the deterministic
scheduler, so a fork never copies a lock another thread holds. **The
checked machine answers `unsupported`** (ruling #95 R4 C), so a program's
in-process path (pelt's) runs there to its end; windows answers
`unsupported`. s248 builds the calls yew asks for (ruling #93): the
terminal restored at every exit the runtime owns
(`os_term_restore_at_exit`), one wait over the terminal, its jobs'
pipes, its signals and a deadline (`os_signal_wait_fds`), the atomic
save's syncs (`fs_sync`, `fs_sync_dir`), the canonical path
(`fs_realpath`) and a nanosecond clock (`os_clock_ns`); then the mode
int's three new bits and `wolf test`'s discovery. What a running program
can see change:

- **A mode int without bits 5–7 now writes the usual setting** —
  `ICRNL` and `OPOST` on, `ISTRIP` off — where 0.2.33 left each as found;
  0 in each bit is the usual setting (`RAW_CR` 32, `RAW_OUTPUT` 64,
  `STRIP8` 128; #802, ruling #97 R7), so on a usual terminal every
  released mode int reads and sets what it did.
- **`wolf test` runs a test file's own `test_*`** (#799): it read the
  first file of the package built for the test file, so a test file that
  a plain file sorted before ran its `main` instead.
- **A program that names `os_signal_wait_fds` makes the signal hub and
  the wake pipe before `main`** (ruling #80), as one that names
  `os_signal_wait_fd` does; one that calls `os_term_restore_at_exit`
  holds one more descriptor from the call.
- **The bundled std moves to wolf-std `e036bec`** (from `cceda63`): sc66's
  wrappers for 0.2.33's five calls (ruling #94), sc67's `net.Addr` and
  the accounts' documented limit, and sc68's lupin lines at the 0.1.55
  pin.

THE THIRTY-FOURTH. Two compiler lanes land, s247 (wolf-lang#803; rulings
#90, #95) and s248 (wolf-lang#804: #794, #795, #796, #797, #798, #799,
#802; rulings #93, #97). **The pairing moves to lupin 0.1.55** (is77:
lupin learns 0.2.33's five calls and refuses the signal family by name;
is78: wolf-interp's CI tiers); lupin 0.1.55 does not resolve 0.2.34's
seven new names (wolf-interp#268, #269), which is expected. `STD-PIN`
moves to wolf-std `e036bec`. 97 commits since 0.2.33 (s247 29, s248 68),
then r39's release commits.

### Read this before you bump the pin

- **Seven new prelude names**: `os_fork` (s247) and
  `os_term_restore_at_exit`, `os_signal_wait_fds`, `fs_sync`,
  `fs_sync_dir`, `fs_realpath`, `os_clock_ns` (s248) — a function of one
  of these names now shadows a builtin (W0304, fatal under
  `--deny-warnings`: rename yours) — and **one new mark**, `busy`.
- **A test file's own `test_*` now run** under `wolf test`, never a
  plain file's that sorted before it: a directory where one did may run
  different tests.
- **`[os.term.mode]`'s bits 5–7 read 0 as the usual setting.** A mode
  with bit 5, 6 or 7 set is no longer `invalid`: it clears `ICRNL` or
  `OPOST`, or sets `ISTRIP`. A mode without them now WRITES the usual
  setting where 0.2.33 left each as found: on a terminal set otherwise
  (`stty -opost`), a hand-built mode puts it back.
- **`os_fork` answers `busy` while any task is live**, from a task or a
  proc handler, and under the deterministic scheduler: fork before the
  first spawn, or after every task has joined. The child is one thread,
  the caller's; its threads come at its first use.
- **The pairing is lupin 0.1.55.** A downstream that keys a pin on the
  lupin version meets 0.1.55's answers: 0.2.33's five calls resolve
  (the shape rows, the core siblings answered, the rest refused by
  name), the listen, wait and raise resolve and are refused by name,
  and `os_signal_text(0)` is refused by name where 0.1.54 answered
  `invalid`.
- **The bundled std is wolf-std `e036bec`**, whose new names (below) are
  std functions, not prelude names.

### The bundled std: wolf-std `e036bec` (from `cceda63`)

`STD-PIN` moves from `cceda63` (0.2.33's) to **`e036bec`**: sc66
(wolf-std#78, ruling #94) on wolf 0.2.33, sc67 (wolf-std#4, #68; ruling
#99 R2), then sc68 (wolf-std#83: #80, #76) at the lupin 0.1.55 pin.
Under `std/` only `process/process.lu` and `signal/signal.lu` (sc66),
`net/net.lu` and `os/users/users.lu` (sc67) change; sc68 changes nothing
there; `std/prelude/prelude.lu` does not change (the std adds no prelude
name).

- **`std.signal`**: `ignore_one(m)`, `reset_one(m)` (one meaning's
  disposition, over `os_signal_ignore_one` / `os_signal_default_one`).
- **`std.process`**: `exit_signal(m)` (over `os_exit_signal`); the core
  read beside `Status`, never inside it: `wait_status_core`,
  `status_from_core`, `dumped_core`, `poll_core() -> CoreChange !
  {io}` with `CoreChange { change, core }`.
- **`std.net`** (sc67, wolf-std#4): `Addr { version: int, text: str }`,
  the value a resolver will answer (an address and its IP version); no
  builtin resolves a name without dialling it at this release
  (wolf-lang#217), so std ships the value and names the builtin it waits
  on. `connect`'s doc says its resolve has no deadline.
- **`std.os.users`** (sc67, wolf-std#68, ruling #99 R2): no signature or
  answer changes; the header and every reader's doc say what the two
  files cannot see (macOS's directory service, LDAP, NIS, SSSD, systemd's
  dynamic users) and what that costs `id`, `whoami`, `chown`, `chgrp`
  and `groups`.
- **sc68**: at the lupin 0.1.55 pin, the five lines sc66 marked to flip
  the day a lupin that answers them is pinned: `tests/signal/text_zero.lu`'s
  lupin column (`divergent(stdout)` → `unsupported`, `text(0)` refused by
  name; wolf-std#76), `signal/one_rows.lu`'s and
  `process/poll_core_none.lu`'s (`unsupported` → `run`), and the
  doc-example gate's two `LUPIN_MIRROR_LAG` entries (`signal.ignore_one`,
  `signal.reset_one`; wolf-interp#261) retired; and five windows cells
  (`text_zero`, `one_host`, `one_fault`, `exit_signal_lives`,
  `wait_core_rows`) that run on lupin 0.1.55 there, where the host's row
  comes before lupin's refusal by name. `std/` is unchanged by sc68.
- **Measured under this release** (kasumi, linux x86-64, the
  release-head archive with the published lupin 0.1.55 beside `wolf`, no
  `WOLF_STD`): wolf-std `e036bec`'s 497 tests on lupin, the checked
  machine and native, and its 519 doc examples: **all green** (1,557
  lane runs). The release machine equals native on 496 of the 497 with
  no std configured; the one (`list/sort_by_tier.lu`) is refused by name
  on release, as at 0.2.33. Against the published 0.2.33 on the same
  std, 0 records moved on any tier.
- **Not wrapped yet** (the sc lane after this release): `process.fork()`;
  `term.Mode`'s three fields (`raw_cr`, `raw_output`, `strip8`) and a
  `raw` that sets `raw_cr` and `raw_output`, as C yew's raw mode is;
  wrappers for `os_term_restore_at_exit`, `os_signal_wait_fds`,
  `fs.sync`, `fs.sync_dir`, `fs.realpath` and `time.now_ns`.

**New prelude names:** `os_fork`, `os_term_restore_at_exit`,
`os_signal_wait_fds`, `fs_sync`, `fs_sync_dir`, `fs_realpath`,
`os_clock_ns`, all functions at `[os.host.sigs]`, and the mark `busy`
(`cargo xtask prelude-diff v0.2.33 HEAD --markdown`, after `git fetch
--tags`). No downstream declares or calls any of the seven at its trunk
(wolf-std, boreutils, lobo, pelt, pax, muzzle, wolf-book, wolf-lsp, yew,
lope; `.lu` files; the control word `os_signal_wait_fd` is found in
wolf-std, pelt and yew, so the search sees).

**Capabilities:** `os_fork` and `os_signal_wait_fds` are `exec` (with
`os_spawn` and `os_signal_wait_fd`); `fs_sync`, `fs_sync_dir` and
`fs_realpath` are `fs`; `os_term_restore_at_exit` (stdio's `io`) and
`os_clock_ns` (the clock, with `time_now_ms`) are charged nothing in a
manifest; every one is refused at comptime, and the freestanding target
refuses each by name. **Bytes:** `RT_SYMBOLS` 229 → 237; the host table
140 → 147; the prelude 147 → 154 functions, 179 → 186 W0304 names; spec
anchors 664 → 672 (`[os.proc.fork]`, `[os.proc.fork.note]`,
`[os.term.restore]`, `[os.signal.wait.fds]`, `[os.fs.sync]`,
`[os.fs.realpath]`, `[os.clock.ns]`, `[os.editor.calls.note]`).

**Known:** `region_copyout_lanes::a_loop_keeping_a_scalar_is_flat` can
exceed its RSS bound under load (#652, #731, open), a test flake.
`conform-run --native`/`--release` give the program no terminal on
standard input (#766). Carried from 0.2.33, open: #790, #791, #792.
Filed since 0.2.33, open: by yew's lanes, #800 (no bitwise not), #801,
#806–#810, #813–#822; by lobo's ws67, #805; by boreutils' bu28, #811
(no undumpable or core-limit call), #812. lupin 0.1.55 lags 0.2.34's
seven names (wolf-interp#268, #269).

**Rulings:** #90 (the fork, reversing #69 R1), #95 (`[os.proc.fork.note]`:
R1–R3 and R5–R8 as built, R4 C), #93 (yew's G1–G4: R1 C, R2 A, R3–R5),
#97 (`[os.editor.calls.note]` R1–R8 as built). #94 is wolf-std's
spellings; #96 (lope) and #98 (pelt's hook) change no compiler line.

**What each downstream does to bump** (to 0.2.34 / lupin 0.1.55):

- **wolf-std:** the pin; then the wrappers for the restore, the wait over
  many descriptors, `fs.sync`, `fs.sync_dir`, `fs.realpath`,
  `time.now_ns`, `term.Mode`'s three fields (and `raw`) and
  `process.fork()`. The lupin column at 0.1.55 is sc68's, already in the
  bundled std (wolf-std#76, #80).
- **pelt** (sh20): subshells, `( … ) &`, `{ …; } &`, command
  substitutions and the pipeline stages pelt runs itself fork, as dash's
  do; the toy probe `crates/wolf_driver/tests/fixtures/fork/subshell.lu`
  is the shape (traps back to default in the child, XCU 2.12); the pin.
  The checked lane keeps running in process (`unsupported`, ruling #95 R4
  C).
- **boreutils, muzzle:** the pin.
- **lobo:** the pin (ws68 is at 0.2.33).
- **yew** (yw02's pin bump): G3 adopted (the atomic save's syncs), the
  restore at exit, the wait over the tty, its jobs and its signals with
  the 25 ms deadline (the `VTIME` interim retires), the raw bits, and its
  tests named `test_*`.
- **lope:** `fs_sync` for the store's temp-and-rename (ruling #96 D9).
- **pax:** px54 with lope; the forking pelt on PAX (px17 serves `fork`
  since 2026-10-07).
- **wolf-lsp and tree-sitter-wolf:** the pin and the seven prelude names
  (the vendored prelude 187 → 194).
- **wolf-book, then the site:** the book pins 0.2.34 / 0.1.55: the fork
  (a note in the subshell chapter) and the editor's calls; then wolf-web
  pins the pair and the book, and deploys.
- **wolf-interp:** lupin's mirror of s248's six names and the mode's
  bits 5–7 (wolf-interp#268), and `os_fork` refused by name (#269).
- **Homebrew and the AUR:** wolf 0.2.34 with the std at `e036bec`, lupin
  0.1.55.

### The pairing: lupin 0.1.55 (from 0.1.54)

`PAIRING` names **lupin 0.1.55 at pin `294d626`** (the linux x86-64
archive's sha256 `1152abc0…`; its records report `commit e642f26`). is77 is lupin's half of 0.2.33 (the five
calls, `text(0)`, the core siblings, the listen, wait and raise refused by
name); lupin's halves of s247 and s248 are wolf-interp#268 and #269.

- **Carried to 0.1.55, dropped 0, widened 0**: `checked_tasks_lanes`
  (#227, #230), `chan_sent_arg_lanes` and `region_kept_arg_lanes` (#232),
  `attr_closed_set_lanes`' control, `atomic_lanes`' race check, and
  s248's two pre-mirror pins, `editor_calls_lanes` (now a list, 0.1.54
  and 0.1.55) and `editor_wait_lanes` (`("0.1.55", "e642f26")`, whose
  race fixture 0.1.55 stops at the listen, refused by name). Nothing for
  `signal_gaps_lanes`, `fork_lanes` or `shift_lanes`. Against the
  published 0.1.55, 92 gate targets (the 91 that name lupin and
  `stdfd_lanes`) pass 861 of 861; the plant (the same head against the
  published 0.1.54) is red on exactly `pairing` (every carried list still
  names 0.1.54).
- **The ritual against the published 0.2.33** (the same 1155-file
  corpus, the same lupin 0.1.55, this release's archive `wolf` against
  0.2.33's): **no count moved on either tier.** checked: 793 agreements,
  241 completeness notes, 95 unsupported, SOUNDNESS 0, hard 19 on both;
  native: 778, 241, 96, SOUNDNESS 14, hard 33 on both. s247 and s248 add
  no corpus file (their witnesses are the gates'). Below the ledger only
  the three SOUNDNESS rows that read freed memory
  (`conc/atomic_ub_uaf.lu`, `memory/unsafe_ub_uaf.lu`,
  `memory/volatile_ub_uaf.lu`) printed other exit statuses, as at 0.2.32
  and 0.2.33.

### The fork (s247)

s247 — the fork (ruling #90, reversing ruling #69 R1 from B to A): wolf
gets a user-visible `os_fork` whose child keeps running wolf code, sound
beside the runtime's own threads. The surface and the protocol were
asked (`[os.proc.fork.note]` R1–R8) and ruled by ruling #95: R1–R3 and
R5–R8 as recommended, R4 C (the checked machine answers `unsupported`).

- **`os_fork() -> (int, int) ! {busy, io, unsupported}`**
  (`[os.proc.fork]`, R1 A). The parent gets `(pid, h)`: the child's
  process id and its handle in the child table, which `os_wait_status`,
  `os_wait_status_core`, `os_wait_poll`, `os_wait_poll_core`, `os_wait`,
  `os_proc_pid` and `os_kill` serve as they serve a spawn's; the child
  gets `(0, -1)`. The child is one thread, the caller's, with the
  program's memory as it was; every descriptor shared, offsets included;
  dispositions and the mask kept, nothing pending; its own signal hub
  (the inherited pipe ends were the parent's) and its own readiness set
  (linux shares an epoll set across a fork; macOS hands a child no
  kqueue); an empty pool and empty wait tables (the parent's children are
  not the child's: `io`, as `ECHILD`). Its threads come at its first use
  (R3 A): the drain thread at its first wait or listen, the reactor at its
  first park, the workers at its first spawn.
- **`busy`, the protocol's rule** (R2 A, R7 A). A fork from a task or a
  proc handler, or while any task is live anywhere (queued, running,
  parked), or under the deterministic scheduler, answers `busy` before the
  host is asked. Otherwise every thread the runtime made is idle, and the
  fork brings each to rest where it holds no lock (the pool's workers
  parked, retiring ones joined; the hub's drain thread and the reactor's
  thread claimed in their kernel wait without a wake), so the child never
  inherits a lock a vanished thread held. `io` is the host's refusal
  (`EAGAIN`, `ENOMEM`), its number in `os_error()`; a refusal of the
  child's own hub pipes ends the child with exit 71 (ruling #80's rule).
- **Machines** (R4 C, ruling #95; R5 A, R6, R8). native and release on
  linux and macOS; `unsupported` on windows and any host without the task
  layer; **the checked machine answers `unsupported`** on every host, so a
  program's in-process path (pelt's subshells) runs there; lupin 0.1.54 and
  0.1.55 do not resolve the name (its refusal by name is wolf-interp#269;
  a newer lupin is held to a refusal by name); charged
  `exec`, refused at comptime; the freestanding target refuses it by name.
- **The cost** (kasumi, a container, release runtime, medians of nine):
  with a live signal hub, 1,000 fork-and-end loops take 0.119 s (dash's
  `x=$(true)` 0.083 s, C's fork/_exit/wait 0.073 s); fork, exec `true`,
  wait 0.272 s (`os_spawn_fds` 0.179 s, dash's `x=$(/bin/true)` 0.236 s,
  C's fork/exec 0.238 s). In the parent: two mask calls and the fork; in
  the child: up to eight descriptors closed and the hub's three pipes
  made, no thread.
- spec/11 gains `[os.proc.fork]` and `[os.proc.fork.note]` (anchors 666);
  the host table is 141 lines.

### What the editor asks of the runtime (s248)

s248 — what the editor asks of the runtime (ruling #93; yew
`docs/MAP.md` §5 G1–G4, G8, G10; yw01's W1). Six calls, three mode bits
and one `wolf test` fix; the spellings beyond the contract are rulings
asked at audit (`[os.editor.calls.note]` R1–R8), each recommendation
what is built.

- **`os_term_restore_at_exit(fd, mode, bytes: List[byte]) -> () !
  {invalid, io, unsupported}`** (#794, `[os.term.restore]`; ruling #93 R1
  C, R1 asked for the `io` row): the runtime sets the registered mode
  and writes the bytes on every exit it owns — a return from `main`,
  `os_exit`, an error from `main`, a trap (after its report), the stack
  guard's report, `os_exit_signal` (before its raise), the proc root's
  death — in the runtime, never in a handler of the program's. One
  registration a process, the last wins; a death by a signal nobody
  listened for restores nothing. The checked machine restores at its
  own end (R8).
- **`os_signal_wait_fds(fds: List[int], set, deadline_ms) -> (int, int)
  ! {invalid, io, unsupported}`** (#795, `[os.signal.wait.fds]`; ruling
  #93 R2 A): one wait over several descriptors, a signal set and a
  deadline — `(-1, m)` a meaning (it wins), `(i, 0)` the lowest ready
  descriptor, `(-1, 0)` the deadline; 0 polls, -1 waits for ever, below
  -1 `invalid`; windows `unsupported`. `os_signal_wait_fd` is its
  one-descriptor case, its rows unchanged.
- **`fs_sync(fd) -> () ! {io}`** and **`fs_sync_dir(path) -> () !
  {denied, invalid, io, not_found, unsupported}`** (#796, `[os.fs.sync]`):
  the atomic save's syncs (`fsync`; macOS `F_FULLFSYNC`; windows
  `FlushFileBuffers`, and no directory sync there: `unsupported`). The
  save's exclusive create is the one wolf has, open mode 4 and
  `fs_open_perm(path, 4, perm)` (R3: no `fs_create_new`).
- **`fs_realpath(path) -> str ! {io, not_found}`** (#797,
  `[os.fs.realpath]`): the canonical path, as `realpath(3)` resolves it.
- **`os_clock_ns() -> int`** (#798, `[os.clock.ns]`): the monotonic
  clock in nanoseconds, from `time_now_ms`'s anchor.
- **The mode int's bits 5–7** (#802, `[os.term.mode]`; R7): `RAW_CR`
  (32, `ICRNL` cleared), `RAW_OUTPUT` (64, `OPOST` cleared), `STRIP8`
  (128, `ISTRIP` set). 0 in each is the usual setting, so on a usual
  terminal every released mode int reads and sets what it did.
- **`wolf test` runs each test file's own `test_*`** (#799; R6): it read
  the first file of the package built for the test file, so a test file
  that a plain file sorted before ran its `main` instead (yw00's
  `z_test.lu`), or a helper of that file named `test_*`.


## 0.2.33 — 2026-10-19

**What this release changes: five new calls and the std that wraps
0.2.31's.** s246 closes the signal gaps 0.2.32 left (ruling #88): a
program can ignore or default ONE meaning in the sending shape, a
send-only meaning or a host number `-N` (`os_signal_ignore_one`,
`os_signal_default_one`), end itself by a signal
(`os_exit_signal`), and learn that a child dumped core
(`os_wait_status_core`, `os_wait_poll_core`). **No released status
changes** (ruling #88 R3 = D): `os_wait_status` and `os_wait_poll`
keep answering `-N` for a child ended by signal `N`, core or not; a core
is `-(256 + N)` only from the two `_core` siblings. What a running
program can see change:

- **`os_signal_text(0)` answers words** (glibc's `Unknown signal 0`)
  where it answered `invalid` (#787).
- **A SEGV or BUS another process sends ends the program** under the
  default disposition (#784). A program that had run a task used to live
  through the first one (the runtime's stack guard took it for its own);
  a real fault is still the guard's (the overflow report, exit 134).
- **A program that names `os_signal_wait_fd` starts with two more
  descriptors**, one wake pipe shared by every descriptor wait, made
  before `main` after the hub (#781); a host that refuses them refuses
  the start (one stderr line, exit 71), as for the hub.
- **The bundled std moves to wolf-std `cceda63`** (from `d37c911`):
  sc64's wrappers for 0.2.31's calls (ruling #85) and sc65's two rows.

THE THIRTY-THIRD. One compiler lane lands, s246 (wolf-lang#789: #780,
#781, #783, #784, #785, #787, #788), with no lupin half: lupin 0.1.54
does not resolve the five new names (wolf-interp#261, #262, #263), and
the pairing stays 0.1.54. `STD-PIN` moves to wolf-std `cceda63`. 37
commits since 0.2.32, all s246's, then r38's release commits.

### Read this before you bump the pin

- **Five new prelude names**: `os_signal_ignore_one`,
  `os_signal_default_one`, `os_exit_signal`, `os_wait_status_core`,
  `os_wait_poll_core` (a function of one of these names now shadows a
  builtin, W0304, fatal under `--deny-warnings`: rename yours). No
  released status changes: a core is reported only by the two `_core`
  siblings, so code that compares a status with `-N` (pelt's `$?`,
  boreutils' `timeout`) reads what it read at 0.2.32.
- **`os_signal_text(0)` answers words** where it answered `invalid`
  (windows: `unsupported`; lupin 0.1.54 still `invalid`). A program
  that tested `text(0)` for `invalid` must accept either (wolf-std's
  own row did, and is fixed at `cceda63`, sc65).
- **A SEGV sent to a program ends it** (the host's default) unless the
  program ignores `SEGMENT_FAULT` with `os_signal_ignore_one`; a program
  that had run a task used to live through the first one.
- **A program that names `os_signal_wait_fd` starts with two more
  descriptors** (the shared wake pipe) and makes none per wait; a host
  that refuses them refuses the start (one stderr line, exit 71), as for
  the hub.
- **The bundled std is wolf-std `cceda63`**, whose new names (below)
  are std functions, not prelude names: a program that `use`s
  `std.process` or `std.signal` and declares one of them itself keeps
  its own.

### The bundled std: wolf-std `cceda63` (from `d37c911`)

`STD-PIN` moves from `d37c911` (0.2.31 and 0.2.32's) to **`cceda63`**:
sc64 (wolf-std#71, ruling #85) on wolf 0.2.32, then sc65 (wolf-std#75).
Under `std/` only `process/process.lu` and `signal/signal.lu` change;
`std/prelude/prelude.lu` does not (the std adds no prelude name).

- **`std.process`**: `Status { value }` with `status_from`,
  `is_exited`, `is_signalled`, `is_stopped`, `exit_code`, `signal_of`,
  `stopped_by`, `shell_code` (S1; `256 + N` is a stop); `poll() ->
  Change ! {io}`, `Change { changed, child, status, live }` over
  `os_wait_poll` (S2); `Group`, `group_of(leader)`, `own_group()`,
  `lead_group()`, `join_group(g)` (#79 R1, G2, G3).
- **`std.signal`**: `send_group`, `send_all`, `last`, `realtime()` with
  `Realtime { first, last }` (G4), `text`, `host(n)` (= `-n`, G5).
- **sc65**: `signal/group_rows.lu`'s `text_zero` row and `signal.text`'s
  doc example are true at both 0.2.32 and 0.2.33 (`dcf3e89`'s were
  red under this release on exactly those two rows, s246's #787; no
  function changed).
- **Measured under this release** (kasumi, linux x86-64, the
  release-head archive with lupin 0.1.54 beside `wolf`, no
  `WOLF_STD`): wolf-std `cceda63`'s 484 tests on lupin, the checked
  machine and native, and its 511 doc examples: **all green**. The
  release machine equals native on 483 of the 484 with no std
  configured; the one (`list/sort_by_tier.lu`) is refused by name on
  release, as at 0.2.32. Against the published 0.2.32 on the same std,
  0 records moved on any tier.
- **Not wrapped yet** (wolf-std#74): the five new calls. A program
  calls them as builtins until wolf-std's next lane.

**New prelude names:** `os_signal_ignore_one`, `os_signal_default_one`,
`os_exit_signal`, `os_wait_status_core`, `os_wait_poll_core`, all
functions at `[os.host.sigs]` (`cargo xtask prelude-diff v0.2.32 HEAD
--markdown`, after `git fetch --tags`, run at the release head).

**Capabilities:** all five are `exec` (the set calls' and the wait's
category) and refused at comptime; the freestanding target refuses each
by name. **Bytes:** `RT_SYMBOLS` 223 → 229 (the five and
`__wolf_rt_signal_wait_fd_start`); the host table 135 → 140; the
prelude 142 → 147 functions, 174 → 179 W0304 names; spec anchors 661 →
664 (`[os.signal.disp.one]`, `[os.signal.exit]`,
`[os.signal.gaps.note]`).

**Known:** `region_copyout_lanes::a_loop_keeping_a_scalar_is_flat` can
exceed its RSS bound under load (#652, #731, open), a test flake.
`conform-run --native`/`--release` give the program no terminal on
standard input (#766, open). Filed by s246, open: on macOS a SEGV
another process sends to a program that ignores it is survived only
once (#790); the checked machine's `os_signal_wait_fd` still makes a
pipe per wait (#791); suspected, `available_parallelism`/`os_cpus` may
open cgroup `cpu.max` on a low descriptor (#792). lupin 0.1.54 lags the
five names and `text(0)` (wolf-interp#261, #262, #263).

**Rulings:** #88 (R1 A, R2 A, R3 D; `[os.signal.gaps.note]`). #86 (group
1 stays refused) and #87 (boreutils' self-appending filters) change no
compiler line.

**What each downstream does to bump** (to 0.2.33 / lupin 0.1.54):

- **wolf-std:** the pin; then the wrappers for the five new calls
  (wolf-std#74): `signal.ignore` and `signal.default` of one meaning
  over `os_signal_ignore_one` / `os_signal_default_one` (taking
  `signal.Realtime` and `signal.host(n)`), an exit by signal over
  `os_exit_signal`, and a core accessor over the `_core` siblings
  (`process.Status`'s released accessors unchanged).
- **boreutils:** the toolchain digests; then four refusals retire
  (boreutils#41): `timeout -s SEGV` / `-s RTMIN` / `-s` a host number
  without `--foreground` (`os_signal_ignore_one`), `timeout`'s 139 where
  GNU dies of its command's fault (`os_exit_signal`), the missing
  "(core dumped)" (`os_wait_poll_core`), and killprobe's signal-0 rows
  (`os_signal_text(0)`). The 7 `kill 0` cases stay (ruling #86); #37
  stays a recorded difference (ruling #87).
- **pelt:** the toolchain digests. Nothing is owed for status decoding:
  the released wait and poll still answer `-N` (ruling #88 R3 D), so `$?`
  reads as at 0.2.32.
- **lobo, muzzle, pax:** the pin.
- **wolf-lsp and tree-sitter-wolf:** the pin and the five prelude names
  (the vendored prelude 182 → 187).
- **wolf-book, then the site:** the book pins 0.2.33 / 0.1.54; §34.5's
  waiver sentence (the shipped std has no wrappers for the group,
  numbering and poll calls) comes out, since `cceda63` has them; the new
  calls (§34.5: one meaning's disposition, the exit by signal, the core
  siblings, `text(0)`); then wolf-web pins the pair and the book, and
  deploys.
- **Homebrew and the AUR:** wolf 0.2.33 with the std at `cceda63`
  (this release's tap change); lupin unchanged at 0.1.54.

### The pairing: lupin 0.1.54 (unchanged)

`PAIRING` still names **lupin 0.1.54 at pin `294d626`** (the linux
x86-64 archive's sha256 `175df238…`). s246 has no lupin half; lupin's
mirror of the five names is wolf-interp#261–#263.

- **Dropped 0, widened 0, carried every pin unchanged**: each
  version-keyed list already names 0.1.54, or is keyed by an older
  release's commit (`shift_lanes`); s246's `signal_gaps_lanes` pins
  0.1.54's measured answers (`PRE_MIRROR_LUPIN`) and holds any other
  lupin to the mirror. Against the published 0.1.54, 89 gate targets (the
  88 that name lupin, `signal_gaps_lanes` among them, and `stdfd_lanes`)
  pass 837 of 837. The plant (the same head against the published 0.1.53)
  is red on exactly 0.2.31's four dropped cases and the two
  `signal_gaps_lanes` witnesses pinned to 0.1.54's answers.
- **The ritual against the published 0.2.32** (the same 1155-file
  corpus, the same lupin 0.1.54, this release's archive `wolf` against
  0.2.32's): **one completeness note fewer on each tier and nothing
  else moved**: `corpus/os/signal_one_rows.lu`, s246's own fixture, which
  0.2.32 cannot compile (E0301: the new names) and this release runs.
  checked: 792 agreements, 96 unsupported, SOUNDNESS 0, hard 19 on both.
  native: 777 agreements, 97 unsupported, SOUNDNESS 14, hard 33 on both;
  the three SOUNDNESS rows that read freed memory
  (`conc/atomic_ub_uaf.lu`, `memory/unsafe_ub_uaf.lu`,
  `memory/volatile_ub_uaf.lu`) printed other exit statuses, as at 0.2.32:
  0.2.32 itself gives five statuses in five runs of each.

### The signal gaps 0.2.32 left (s246)

s246 — the signal gaps 0.2.32 left (wolf-lang#780, #781, #783, #784,
#785, #787, #788). Three of them add surface, ruled by the maintainer
(ruling #88, `[os.signal.gaps.note]`): R1 A and R2 A as built, R3 D (a
core through new sibling calls; the released statuses unchanged).

- **#783: one meaning's disposition, in the sending shape**
  (`[os.signal.disp.one]`, R1). `os_signal_ignore_one(meaning) -> () !
  {invalid, io, unsupported}` and `os_signal_default_one(meaning)` take
  one bit of the table's 28 or a host number `-N`, so a program can
  ignore what it is about to send its own group (GNU `timeout -s SEGV`,
  `-s RTMIN`); 0, `KILL` and `STOP` are `invalid`, windows
  `unsupported`, the host's refusal `io`. The set calls and their shape
  rule are unchanged. The checked machine refuses `SEGMENT_FAULT` and
  `BUS_ERROR` by name; lupin 0.1.54 resolves neither name.
- **#784: ending by a signal** (`[os.signal.exit]`, R2).
  `os_exit_signal(meaning) -> () ! {invalid, unsupported}` sets the
  signal back to the default, unblocks it and raises it on the calling
  thread, and exits 128 + N if the process lives on; it returns only a
  shape refusal (or windows' `unsupported`). The runtime's stack guard
  now passes a SEGV or BUS a process SENT (`si_code`) to the program's
  own disposition — the default ends it, `os_signal_ignore_one` keeps it
  alive — while a real fault stays the guard's (the overflow report,
  exit 134). Until now a program that had run a task lived through the
  first SEGV it was sent. The guard's handler is installed on `SIGBUS`
  on linux too (macOS only before). macOS gives a sent SEGV a fault's
  siginfo (measured), so there the runtime takes a delivery at address 0
  as sent when the program announced a send to itself (its own id, its
  own group); one from another process ends it under the default and is
  lived through once under an ignore. `os_signal_raise` is unchanged.
  The checked machine refuses the call by name.
- **#785: a core, through new siblings** (`[os.proc.status]`,
  `[os.proc.poll]`, ruling #88 R3 D). `os_wait_status_core(h) -> int !
  {io}` and `os_wait_poll_core() -> (int, int) ! {io}` are the released
  wait and poll with a child that dumped core (`WCOREDUMP`) answered
  `-(256 + N)`; `os_wait_status` and `os_wait_poll` keep answering `-N`,
  core or not. The checked machine answers both; lupin 0.1.54 resolves
  neither.
- **#787: signal 0 has words** (`[os.signal.text]`).
  `os_signal_text(0)` is `strsignal(0)`'s, glibc's `Unknown signal 0`
  (GNU `kill -t 0` and `kill -L` row 0), where it was `invalid`;
  `unsupported` on windows. lupin 0.1.54 still answers `invalid`.
- **#781: a descriptor wait makes no descriptor while the program runs**
  (`[os.signal.wait.fd]`, `[os.signal.hub]`). The descriptor waits
  share one wake pipe, made before `main` after the hub in a program that
  names `os_signal_wait_fd` (ruling #80's rule;
  `__wolf_rt_signal_wait_fd_start`), where each wait made its own and the
  host could put an end of it on a closed 0, 1 or 2 until the runtime
  moved it: beside such waits, with 1 closed, another task's
  `fs_fstat(1)` answered ok 760,739 times in 2 s at 0.2.32 (kasumi), 0
  now.
- **#780: the write gate blocks on every host.** `checked_net_write`'s
  four write fixtures go on writing 1 MiB at a time after the 8 MiB
  write until their reader has run (its one byte back), and print how
  far they wrote, so windows' loopback (which took 8 MiB whole) fills
  too; tests only.
- **#788: spec/03 `[conc.task.order]`** no longer says `os_signal_wait`
  makes ~196 futex calls a second: an idle one made 0 system calls in 8
  s at 0.2.32 (measured, both tiers).

## 0.2.32 — 2026-10-18

**Why this release: a 0.2.31 regression (#778).** At 0.2.31 a program
on the dev tier (`wolf build`, `wolf run`, `conform-run --native`) that
imports `std.signal` or `std.net` (every boreutils utility imports
`std.signal`) and is started with descriptor 0, 1 or 2 closed found that
descriptor open before `main`: the runtime made its signal hub (or its
readiness set) at start for every function the build held, and the host
put the hub's pipes on the lowest free numbers. A program run `prog >&-`
saw `fs_fstat(1)` answer ok and wrote its output into the runtime's
pipe instead of failing as it should (boreutils' `cat` and `wc` failed
five dev-tier difftest cases, which held boreutils at 0.2.30). On
**either tier**, a program that *reached* `os_signal_listen`,
`os_signal_wait`, `os_signal_wait_fd` or a parking net call with 0, 1
or 2 closed had the same fault (`wolf build --release` too, at 0.2.31).
Now (s245, `[os.signal.hub]`, `[os.net.reactor]`):

- **The runtime's own descriptors never take 0, 1 or 2.** The hub's
  pipes, `os_signal_wait_fd`'s wake pipe and the readiness set's
  descriptors move to 3 or above when the host places one on a closed
  standard descriptor, and that descriptor is closed again: a standard
  descriptor the program was started without stays closed on both
  tiers.
- **The hub and the set are made where they are reached, on both
  tiers.** The entry shim decides from the functions reachable from
  `main` and the exports (the release tier's walk), so an import alone
  makes nothing: no pipe, no set, no extra thread. Ruling #80 stands
  where a call is reached (made before `main`; a refusal is exit 71).

THE THIRTY-SECOND, a point release. Two compiler lanes land, neither
with a lupin half: the checked machine's `net_wait` hands over (s244,
#720) and s245's descriptor fix with the checked machine's big writes
(#778, #777). The pairing stays lupin 0.1.54 and the bundled std stays
wolf-std `d37c911`. 22 commits since 0.2.31 (s244 6, s245 16), then
r37's release commits.

### Read this before you bump the pin

**A closed standard descriptor stays closed (s245, #778).** A program
started with 0, 1 or 2 closed now finds it closed (`fs_fstat` answers
`io`, a write to it fails) on both tiers, whatever it imports. A
dev-tier program that imports `std.signal` or `std.net` but reaches no
call that makes the hub or can park no longer makes either at start: it
carries no pipe, no readiness set and no drain thread, and it can no
longer exit 71 at start for them (a program that reaches such a call
can, as at 0.2.31). Windows is unchanged (no descriptor here: the hub is
a queue, the set a wake socket).

**The checked machine's waits and writes hand over (s244, s245; only
`conform-run --checked`).** `net_wait` beside other tasks polls a
millisecond at a time and lets them run, so a loop waiting on tasks it
spawned (the book's ex33-7) answers instead of hanging; a `net_wait`
with deadline 0 asks once. The four net writes do the same when the
socket is full, so an 8 MiB write to a spawned reader answers in about
0.1 s instead of hanging. A deadline still decides each answer; with no
other task each is the blocking call it was.

**The bundled std is unchanged: wolf-std `d37c911`**, as 0.2.31 shipped.

- **Measured under this release** (kasumi, linux x86-64, the
  release-head archive with lupin 0.1.54 beside `wolf`, no `WOLF_STD`):
  wolf-std `d37c911`'s 475 tests on lupin, the checked machine and
  native, and its 497 doc examples: **all green**. The release machine
  equals native on 474 of the 475 with no std configured; the one
  (`list/sort_by_tier.lu`) is refused by name on release, as at 0.2.31.
  Against the published 0.2.31 on the same std, 0 records moved on any
  tier.

**New prelude names:** none (`cargo xtask prelude-diff v0.2.31 HEAD
--markdown`, after `git fetch --tags`, run at the release head).

**Capabilities:** unchanged. **Bytes:** `libwolf_rt.a` gains the move
off 0–2 (no new exported symbol); a dev-tier program that names but
does not reach a hub or park call loses its start call. Host table,
prelude and spec anchors (661) unchanged.

**Known:** `region_copyout_lanes::a_loop_keeping_a_scalar_is_flat` can
exceed its RSS bound under load (#652, open), a test flake.
`conform-run --native`/`--release` give the program no terminal on
standard input (#766, open). Filed by s245, open: the checked machine's
write gate never blocks on windows (#780); a thread can take a low
descriptor in the window between `pipe` and the move off 0–2 (#781).

**Rulings:** none new; ruling #80 stands as s245 applies it.

**What each downstream does to bump.** 0.2.31's list, now at 0.2.32
(0.2.31's section below has each line in full):

- **wolf-std, first:** pins to 0.2.32 / 0.1.54; `process.poll` over
  `os_wait_poll` (`256 + N` a stop) and the group-call wrappers, as
  0.2.31's list says.
- **boreutils:** the toolchain digests, now unblocked: its pin stayed
  at 0.2.30 because 0.2.31's dev tier failed five `>&-` cases (#778);
  boreutils `792ad31`'s difftest against this release's candidate
  archive (lupin 0.1.54 beside it) passes 6383, fails 0, skips 262 on
  both tiers, as s245 measured its branch build. Then `kill`'s groups
  and `timeout`, as 0.2.31's list says.
- **pelt:** the toolchain digests; job control on the poll.
- **lobo:** the toolchain digests; lobo#92 (retire the descriptor
  workaround).
- **wolf-book, then the site:** the book pins 0.2.32 / 0.1.54; **ex33-7's
  checked record** changes from killed to `answered 4 questions on 2
  connections` (#720, s244); the TONE.md waiver sentence in chapter 34
  comes out; then wolf-web pins the pair and the book, and deploys.
- **wolf-lsp and tree-sitter-wolf:** the pin (no new prelude name since
  0.2.31; 0.2.31's seven if they have not taken them).
- **muzzle:** the pin.
- **pax:** the pin (it builds from the archive).
- **Homebrew and the AUR:** 0.2.32 / 0.1.54 (this release's tap
  change); the AUR moves from 0.2.30 / 0.1.53 straight to 0.2.32 /
  0.1.54.

### The pairing: lupin 0.1.54 (unchanged)

`PAIRING` still names **lupin 0.1.54 at pin `294d626`** (wolf-interp
release 415937354; the linux x86-64 archive's sha256 `175df238…`). The
spec pin is unchanged (this compiler's v0.2.24 tag). Neither s244 nor
s245 has a lupin half: lupin already handed over in `net_wait` and the
writes (the checked machine now does as lupin does), and the descriptor
fix is the compiled runtime's.

- **Dropped 0, widened 0, carried every pin unchanged**: each
  version-keyed list already names 0.1.54 (`checked_tasks_lanes`,
  `region_kept_arg_lanes`, `chan_sent_arg_lanes`, attr_closed_set's
  control, the racy counter's check) or is keyed by an older release's
  commit (`shift_lanes`). Against the published 0.1.54, 88 gate targets
  (the 87 that name lupin, s244's `checked_net_wait` and s245's
  `checked_net_write` among them, and s245's `stdfd_lanes`) pass 831 of
  831. The plant (the same head against the published 0.1.53) is red on
  exactly 0.2.31's four dropped cases.
- **The ritual against the published 0.2.31** (the same 1154-file
  corpus, the same lupin 0.1.54, this release's archive `wolf` against
  0.2.31's): **0 ledger counts moved** on either tier. checked: 792
  agreements, 95 unsupported, SOUNDNESS 0, hard 19 on both, the two
  passes byte-identical. native: 777 agreements, 96 unsupported,
  SOUNDNESS 14, hard 33 on both; three SOUNDNESS rows changed their
  printed exit status (`conc/atomic_ub_uaf.lu`,
  `memory/unsafe_ub_uaf.lu`, `memory/volatile_ub_uaf.lu`): each reads
  freed memory, and 0.2.31 itself gives a different status on each of
  five runs.

### The checked machine's `net_wait` hands over (s244, #720)

On the checked machine `net_wait` was one host poll for its whole
deadline on the only thread that runs the tasks, so a loop waiting on
connections from tasks it had spawned never let those tasks run: the
book's ex33-7 (a door that spawns two callers, then loops on
`net_wait([door] + open, 2000)`) was killed at 120 s and 200 s under
`conform-run --checked` on three hosts, polling `poll(…, 2000) = 0`
every two seconds with no `connect`. lupin and native answer `answered
4 questions on 2 connections`. Now, beside other tasks, `net_wait`
polls the set for at most a millisecond at a time (never past its
deadline) and hands the baton on between polls, as the socket reads
already did; the deadline still decides the empty answer; a deadline of
0 asks once and hands nothing on, as lupin's does; with no other task it
is the one blocking poll it was (`[exec.checked.task]`). ex33-7 under
`--checked` now answers lupin's line in 0.06 s. Witnesses
(`checked_net_wait.rs`): the reduced door loop, ex33-7 verbatim and the
deadline rows, the checked machine held to lupin and native under a
30 s kill.

### A closed standard descriptor stays closed; the dev tier makes the hub and the set only where they are reached (s245, #778)

A 0.2.31 regression. On the dev tier a program that imported
`std.signal`, and so every boreutils utility, made the signal hub (two
pipes) before `main`, and the host put its pipes on the lowest free
descriptors: a program started `prog >&-` found descriptor 1 open, a pipe
of the runtime's, `fs_fstat(1)` answered ok and its output went into the
pipe. The release tier, and 0.2.30 on either, left 1 closed (boreutils'
`cat` and `wc` with `>&-` failed five dev-tier difftest cases at 0.2.31).
Two faults, both fixed (`[os.signal.hub]`, `[os.net.reactor]`):

- **The runtime's own descriptors never take 0, 1 or 2.** The hub's two
  pipes, `os_signal_wait_fd`'s wake pipe and the readiness set's
  descriptors (epoll and its eventfd on linux, the kqueue on macOS) are
  moved to 3 or above when the host places one on a closed standard
  descriptor, and that descriptor is closed again. A program that
  reaches `os_signal_listen` with 0, 1 or 2 closed now finds it closed on
  both tiers (at 0.2.31 the release tier put the hub there too, for a
  program that reached the listen). The readiness set did the same for a
  program that reached a parking net call, on both tiers.
- **"Names a call" is what the program reaches, on both tiers.** The
  entry shim decides whether to make the hub and the readiness set from
  the functions reachable from `main` and the exports (the release
  tier's dead-function walk), not every function the build holds. A
  dev-tier program that imports `std.signal` or `std.net` and reaches no
  call that makes the hub or can park now carries no pipe, no set and no
  extra thread, as on the release tier. Ruling #80 stands: where such a
  call is reached the hub (or the set) is made before `main`, and a
  refusal is exit 71.

Windows has no descriptor here (the hub is a queue, the set a wake
socket) and is unchanged: a program started with a null standard handle
reads that stream closed and the other two open, before and after
(measured on the dev tier; windows has no release tier).

### The checked machine's big write no longer hangs beside its reader (s245, #777)

On the checked machine (`conform-run --checked`) `net_write`,
`net_write_bytes`, `net_writev` and `net_writev_head` were one blocking
write of everything on the thread that runs the tasks, so a write larger
than the socket's buffers to a task of the same program that would read
it waited for ever (an 8 MiB write to a spawned reader was killed at
30 s; lupin and native answer in about 0.1 s). Beside other tasks a write
now parks as a read does (s244's construction for `net_wait`): it polls
for room a millisecond at a time, sends what the socket takes, and lets
the other tasks run between polls. An armed `net_deadline` still ends a
write no byte of which can go for that long, with the same `io` row; with
no other task the write is the blocking call it was
(`[exec.checked.task]`).

## 0.2.31 — 2026-10-17

**Three things change what a running program does. Read them before you
bump.**

- **A shift on a plain integer is arithmetic on its value** (s237,
  `[type.int.shift]`, ruling #77 A). `x << n` is `x * 2^n` and traps
  `overflow` when that leaves the type; `x >> n` is the floor of `x /
  2^n`; a negative count traps. On 0.2.30 native and release computed
  the masked machine shift on plain types (`255 << 1` at `u8` printed
  254, `1 << 8` printed 1, `255 >> 8` printed 255): a program that
  shifts a set bit out, shifts by the width or more, or by a negative
  count now traps `overflow` (or, for `>>` past the width, answers 0 or
  -1). `wrapping[T]` keeps the old bits.
- **A program the runtime cannot set up does not start: exit 71** (s239,
  s243; ruling #80's rule). A program that names a call which can park
  in the reactor (`net_accept`, the net reads and writes) makes its
  readiness set before `main`, and one that names `os_signal_listen`,
  `os_signal_wait` or `os_signal_wait_fd` makes its signal hub there.
  When the host refuses either (a descriptor limit already reached at
  start), the program writes one line to standard error, `wolf: the
  runtime could not make its readiness set (…); the program did not
  start` (or `… its signal hub …`), and exits **71** (`EX_OSERR`), before
  any line of `main` runs. On 0.2.30 the same program started, ran, and
  ended with SIGABRT (134) at its first park or listen. A program that
  names none of these calls is unchanged.
- **The poll of the children** (s242, `[os.proc.poll]`, ruling #84):
  `os_wait_poll() -> (int, int) ! {io}` never blocks and reports a stop
  as **`256 + N`**, beside an exit code (0..255) and an end by signal
  (`-N`). And **`os_signal_wait` no longer wakes on a timer**: outside a
  scope that can be killed it parked on a 5 ms tick at 0.2.30 (about 195
  context switches a second, idle); now an idle wait makes none.

THE THIRTY-FIRST. Six compiler lanes land, four with a lupin half, and
the pairing moves to lupin 0.1.54, which mirrors them: the shift rule and
home-module methods on the checked machine (s237, #736, #745, ruling #77
A); the readiness set made at start and `os_error()` for the net family
(s239, #750, #751, ruling #80); the signal hub made at start and two test
races closed (s243, #763, #765); the group send, the caller's group, host
signal numbers and a signal's words (s241, #739, #740, #741, #762, ruling
#79); the job wait (s242, #680, ruling #84); `parked_tasks`' race (s238,
#733). s240's group-signal note (docs and probes), CI's std checkout at
`.dist-std/` (tl22) and the windows legs (tl23). The bundled standard
library moves to wolf-std `d37c911`. 153 commits since 0.2.30
(s238 5, s239 23, s237 40, tl22 3, s241 24, s240 12, tl23 3, s243 17,
s242 24, is76's compiler half 2: `shift_lanes` keys its lupin pin by
version and release commit), then r36's release commits.

### Read this before you bump the pin

**The shift rule (s237, ruling #77 A).** Upgrading from 0.2.29 or 0.2.30:

1. A shift on a plain integer now traps `overflow` where it used to
   wrap: a set bit shifted out by `<<` (`1 << 63` on `int` included), a
   `<<` of a non-zero value by the width or more, and a negative count
   either way. A `>>` by the width or more now answers 0 (-1 for a
   negative value) instead of the masked shift.
2. Code that wants the old bits says so with `wrapping[T]` (`(x as
   wrapping[u64] << n as wrapping[u64]) as u64` with `x`, `n: u64`;
   both operands of a shift have one type), where the count masks to
   the width and the bits that leave are dropped, as before.
3. Measured downstream before the ruling (s237's census, the same
   programs built by this compiler and by 0.2.29's trunk): wolf-std (100
   shift sites, all `wrapping`), lobo (95, all `wrapping`), boreutils
   (7, one plain), pelt (13, eleven plain) and pax (288, all plain) gave
   no new trap, no changed output and no changed exit status in their
   own suites. pax's variable-count shifts — signal bits, bitmap bits,
   retransmit backoff — are where a count of 0 or one past the width
   would now trap instead of misbehaving quietly.
4. The release tier's code changes beyond the shifts themselves,
   through inlining (a checked shift is larger); behaviour does not.

The constant evaluator now computes what the run time does (`const Z:
u8 = 255 >> 8` is 0; `const K: u8 = 200 << 1` stays E0706). The checked
machine answers a plain shift and a home-module method call (`m.keys()`)
it refused by name before (#736, #745).

**Exit 71 at start (s239, s243).** A program that names a parking net
call, or `os_signal_listen`, `os_signal_wait` or `os_signal_wait_fd`,
makes the runtime's readiness set or signal hub before `main`; when the
host refuses, the program does not start: one stderr line and exit 71,
no `wolf-trap:` line, not 134. A supervisor that reads 134 as "ran out
of descriptors mid-flight" now sees 71 at once, before the program's
first line. The cost, only in a program that names such a call: two
descriptors and one thread for the readiness set and four descriptors
and one thread for the hub on linux, from the start; about 13–22 µs of
start-up for the set. No park now asserts on a host resource: a wait
that finds no set answers its call's `io`.

**`os_wait_poll`'s status (s242, ruling #84).** `(h, status)` for the
first child of the program's that ended or stopped since it was last
looked at: **0..255** an exit code, **`-N`** an end by host signal N
(`os_wait_status`'s spelling), **`256 + N`** a stop by host signal N
(`STOP`, `SUSPEND`, `BG_READ`, `BG_WRITE`: 19–22 on linux; 17, 18, 21,
22 on macOS). An end reaps and closes the handle; a stop is reported
once and the child stays. **A continue is not reported** (512 stays
free), and **macOS sends no `CHILD` for a continue** (measured; linux
does). Nothing to report is `(-1, n)`, `n` the children still live, so
`(-1, 0)` is "none left". One `waitpid(pid, WNOHANG | WUNTRACED)` per
live child, never `waitpid(-1)`: a child the program did not start is
never reaped. windows reports ends and `(-1, n)`, never a stop. The
wait a signal cuts short is a listen for `CHILD` and the trapped set,
the poll, and `os_signal_wait` (or `os_signal_wait_fd`).

**`os_signal_wait` no longer wakes on a timer (s242).** Outside a scope
that can be killed it parked on a 5 ms tick, so an idle wait made about
195 context switches a second at 0.2.30; now it makes none (0 in 5 s on
native, release and checked, measured by s242). It still returns only
for a signal in its set. The checked machine's `os_signal_wait` with no other task alive now waits on
the host for a set a listen armed, instead of refusing by name.

**A negative meaning is now a host signal (s241, ruling #79 R4).**
`os_signal_send(pid, -N)` was `invalid` and is now a send of host signal
N, as are `-N` to `os_signal_send_group`, `os_signal_send_all` and
`os_signal_text`; a program that relied on a negative meaning being
refused must check it first.

**The bundled std is wolf-std `d37c911`** (sc62: std at 0.2.30 / lupin
0.1.53 with s236's ids and owners wrappers and the terminal's size and
characters; sc62b: the descriptor-and-signal wait as `signal.wait_fd`,
ruling #82; sc63: its ledger records that lupin 0.1.54 runs
`x/crypto/sha2/wycheproof_hkdf256_full.lu` to the end, and it vendors
lupin 0.1.54; its `std/` tree is `f50fe73`'s byte for byte), where
0.2.30 shipped `fd6bcdc`. New: `env.uid`, `euid`,
`gid`, `egid`, `groups`; `fs.set_owner`, `set_link_owner`,
`set_file_owner`, `owner_keep`; `process.set_ids` (whole or the process
ends, ruling #78); `std.os.users` (a pure reader of `passwd(5)` and
`group(5)` lines); `term.size` and `Size`; `term.chars`, `Chars`,
`chars_from`, `char_disabled`; `signal.wait_fd(f, set)`. It has **no
wrappers for this release's seven builtins** (s241's six and s242's
poll): call them directly; they are in the prelude.

- **Measured under this release** (kasumi, linux x86-64, the
  release-head archive with lupin 0.1.54 beside `wolf`, no `WOLF_STD`):
  wolf-std `d37c911`'s 475 tests on lupin, the checked machine and
  native, and its 497 doc examples: **all green**. The release machine
  equals native on 474 of the 475 with no std configured; the one
  (`list/sort_by_tier.lu`) is refused by name on release, as at 0.2.30.
  Against the published 0.2.30 on the same std, 0 records moved on any
  tier: the shift rule moves no std row (its 100 shift sites are all
  `wrapping`).

**New prelude names** (`cargo xtask prelude-diff v0.2.30 HEAD
--markdown`, after `git fetch --tags`, run at the release head):

> **New prelude names.** HEAD adds `os_signal_send_group`,
> `os_signal_send_all`, `os_set_pgid`, `os_signal_last`,
> `os_signal_realtime`, `os_signal_text`, and `os_wait_poll` (all seven:
> function, `[os.host.sigs]`). A module that declares a function, type
> or binding named any of them now draws W0304 (it shadows the ambient
> name), which `--deny-warnings` makes fatal: rename yours.

A grep for the seven in wolf-std `d37c911f`, boreutils `23ea8dd2`, lobo
`d3f4a385`, pelt `06223a89`, pax `5c36560f`, muzzle `da22e9e9`, the book
`95d68ad0` and wolf-lsp `eb907a0d` found **no declaration and no use**.
s237, s238, s239, s243, tl22 and tl23 add no prelude name.

**Capabilities.** The three sends and `os_set_pgid` charge `exec`;
`os_signal_last`, `os_signal_realtime` and `os_signal_text` are
uncharged; `os_wait_poll` charges `exec` with `os_wait_status` (it
reaps). All seven are refused at comptime. A package that calls a
charged one without the declaration is E1504.

**Every hosted binary's bytes may move:** `libwolf_rt.a` gains the start
shims and s241's and s242's calls (`RT_SYMBOLS` 214 → 223); a program
that names a park or a hub call gains its start call; the shift rule
changes every plain shift's code. Host table 128 → 135 lines, prelude
135 → 142 functions, spec anchors 648 → 661 (13 added, none dropped).

**Known:** `region_copyout_lanes::a_loop_keeping_a_scalar_is_flat` can
exceed its RSS bound under load (#652, open), a test flake.
`parked_tasks`' witness race (#733) is fixed (s238), and two more tests
of the same shape (#765, s243). `conform-run --native`/`--release` give
the program no terminal on standard input (#766, open).

**For contributors.** CI's std checkout moved to `.dist-std/<pin>/`
(tl22, gitignored; the std a build uses is byte-identical); windows'
build and test runs as two legs, `tests` and `rung`, and caches are
saved only from trunk (tl23).

**Rulings made, each as implemented:** #77 (s237, A: a shift on a plain
integer is arithmetic on its value); #79 (s241, R1–R9: two group-send
builtins, `os_set_pgid` on the caller, rows as the host answers, host
numbers by −N with no new bits, `os_signal_text` in the C locale, the
capabilities, windows `unsupported`, lupin refuses by name, the lane
order); #80 (s239, R1–R6: the readiness set before `main`, exit 71, a
park finding none answers `io`, `os_signal_wait_fd` answers `io`, the
net family sets `os_error()`; s243 applies the same rule to the signal
hub); #84 (s242, R1–R9: `os_wait_poll`, the program's own children,
`256 + N`, continues not reported, `(-1, n)`, the interruptible wait
composed, `exec`, windows ends only, lupin `(-1, 0)` then refused by
name); #82 (wolf-std's terminal wrappers, `signal.wait_fd`).

**What each downstream does to bump.**

- **wolf-std, first:** pins to 0.2.31 / 0.1.54; then `process.poll` over
  `os_wait_poll` with a status type that reads `256 + N` as a stop
  (and `-N`, 0..255), and std wrappers for the group calls
  (`os_signal_send_group`, `os_signal_send_all`, `os_set_pgid`,
  `os_signal_last`, `os_signal_realtime`, `os_signal_text`; ruling #79
  R1's `Group` type). A later compiler release bumps `STD-PIN` to carry
  them.
- **pelt:** the toolchain digests; job control on the poll: listen for
  `CHILD` and the trapped set, `os_wait_poll`, `signal.wait` (or
  `signal.wait_fd` at the prompt); a stopped job's `$?` is `128 + (s −
  256)`, as dash reports it; `kill %job` through
  `os_signal_send_group`; the shell's own group by `os_set_pgid(0)`.
  Its eleven plain shifts gave no new trap (s237's census).
- **lobo:** the toolchain digests; **lobo#92**: retire the descriptor
  workaround (one parked read at start, ws64): the reactor is made at
  start now, so a server at its descriptor limit either starts with its
  set or exits 71 before serving.
- **boreutils:** the toolchain digests; `kill` reaches groups
  (`kill -- -PGID`, `kill 0`, `kill -1`) through
  `os_signal_send_group`/`os_signal_send_all`, `kill -l` and `-L`
  through `os_signal_last`, `os_signal_text` and `-N`; then `timeout`
  (`os_set_pgid(0)`, the group send).
- **wolf-book, then the site:** the book pins 0.2.31 / 0.1.54; the TONE.md
  waiver sentence in chapter 34 ("the wrappers ship with the next
  release") comes out, since this release's std carries them; the
  shift rule, exit 71 and the poll get their pages. Then wolf-web pins
  the pair and the book, and deploys.
- **wolf-lsp and tree-sitter-wolf:** the pin, and the seven names in
  their vendored `prelude.json`.
- **muzzle:** the pin; it uses none of the new names.
- **pax:** the pin (it builds from the archive); its 288 plain shifts
  gave no new trap in its suites (s237's census), and the seven new
  builtins are hosted: the freestanding target refuses each by name.
- **Homebrew and the AUR:** the std beside the binary moves to `d37c911`
  (this release's tap change).

### The pairing: lupin 0.1.54

`PAIRING` names **lupin 0.1.54 at pin `294d626`** (wolf-interp release
415937354; the linux x86-64 archive's sha256 `175df238…`). The spec
pin is unchanged (this compiler's v0.2.24 tag). 0.1.54 carries the lupin
halves of s241 and s242 and is76, which mirrors s237's shift rule
(wolf-interp#251) and s239's `os_error()` for the net family (#250);
s238, s239's start, s240, s243, tl22 and tl23 have none.

- **Dropped 4 cases in four gates**, each green on the published 0.1.54
  unpinned: `group_lanes` 1 (s241's mirror), `job_wait_lanes` 1 (s242's),
  `reactor_lanes` 1 (#751's lupin half; its two entries, 0.1.52 and
  0.1.53) and `pairing` 1. Each pre-mirror list stays, empty, as the
  gate's machinery.
- **Carried, measured.** To 0.1.54 (each answers as 0.1.53 on the pinned
  rows; the issues are open): `checked_tasks_lanes`' two partings
  (wolf-interp#227, #230), `region_kept_arg_lanes`' seven rows and
  `chan_sent_arg_lanes`' channel-payload rows (both wolf-interp#232),
  attr_closed_set's control (lupin still declines its `comptime fn`); the
  racy counter's `trap(race)` check names 0.1.54 too. `shift_lanes`'
  list stays keyed by version and release commit (0.1.52 at `025428b`,
  0.1.53 at `7896edd`); the published 0.1.54 matches neither and answers
  the rule on every count, past 127 included.
- **Widened 0.** The plant (these lists against the published 0.1.53)
  is red on exactly the 4 dropped cases.
- **The ritual** (#87, with #281's control against the 0.1.53 archive,
  the same corpus and the same release `wolf`, 1154 files):
  - checked: **moved 2 ledger counts**. Agreements 785 → 787,
    unsupported 96 → 94; SOUNDNESS 0 and hard 19 unchanged.
  - native: **moved 2**. Agreements 769 → 771, unsupported 98 → 96;
    SOUNDNESS 14 and hard 33 unchanged.
  - The same two files on both, `os/group_rows.lu` (s241) and
    `os/job_poll_rows.lu` (s242), from unsupported to agreement.
  - **One file moved below the ledger**, on both: `comptime/shift_overflow.lu`,
    whose pin is `fail(E0706)`; lupin answered `exit(0)` printing 400 and
    now answers `fail(E0706)` (is76's initialiser check). A
    `fail`-pinned file is a completeness note whatever lupin answers, so
    no count moved.

### The job wait: a poll of the children that reports stops, and a signal wait with no timer (s242, #680; ruling #84)

A shell could wait for one child to end and nothing else: `os_wait_status`
blocks through a stop, nothing answered "has any child changed?" without
blocking, and a `wait` could not be cut short by a trapped signal (#680).
The surface was ruled by the maintainer, 2026-10-17 (ruling #84,
`[os.proc.wait.note]` R1–R9), each as recommended; on the checked
machine, native and release:

- **`os_wait_poll() -> (int, int) ! {io}`** (`[os.proc.poll]`) — never
  blocks: `(h, status)` for the first child of the program's that ended
  or stopped since it was last looked at — an exit code (0..255), `-N`
  for an end by host signal N (`os_wait_status`'s), or **`256 + N` for a
  stop** by host signal N (`STOP`, `SUSPEND`, `BG_READ`, `BG_WRITE`:
  19–22 on linux, 17, 18, 21, 22 on macOS). An end reaps and closes the
  handle; a stop is reported once and the child stays; a continue is not
  reported. Nothing to report: `(-1, n)`, `n` the children still live.
  One `waitpid(pid, WNOHANG | WUNTRACED)` per live child, never
  `waitpid(-1)`. `exec`, refused at comptime; windows answers ends and
  never a stop.
- **The wait a signal cuts short** is the poll with `os_signal_wait` (or
  `os_signal_wait_fd`) on `CHILD` and the trapped set: `CHILD` comes for
  a stop and an end. **`os_signal_wait` no longer ticks**: outside a
  scope that can be killed it parked on a 5 ms timer (about 195 context
  switches a second, measured at 0.2.30); now an idle wait makes none
  (0 in 5 s on native, release and checked, measured). The checked
  machine's `os_signal_wait` with no other task alive now waits on the
  host for a set a listen armed, instead of refusing by name.

lupin answers the poll with no child, `(-1, 0)`, and refuses it by name
once a child is live (its half: wolf-interp `s242`). Host table 134 → 135
lines, prelude 141 → 142 functions, RT_SYMBOLS 222 → 223, anchors 659 →
661 (after s243's hub symbol and anchor). Witnesses (`job_wait_lanes.rs`): a job stopped by each of the four,
continued and ended, and a `wait` cut short by `TERMINATE`, in a
throwaway container with no init on linux (podman's init blocks the stop
signals and its children inherit the block) and on CI's runner on macOS;
an idle wait's context switches counted from outside.

### The signal hub made before `main`; two test races closed (s243, #763, #765; ruling #80's rule)

A program at its descriptor limit no longer ends at its first
`os_signal_listen` or `os_signal_wait`. Until now the first of those
made the signal hub (two pipes and a drain thread) and asserted that the
host gave it: at the limit the process ended with SIGABRT (134), `the
signal hub could not be made`, after its own lines had run. Now, by
ruling #80's rule (`[os.signal.hub]`):

- A program that names `os_signal_listen`, `os_signal_wait` or
  `os_signal_wait_fd` makes the hub **before `main`** (the entry shim,
  after the readiness set). A listen, a raise and a wait at a
  32-descriptor limit answer ok, ok and the meaning on native and
  release, as the checked machine always did. A program that names none
  carries no pipe and no thread (D15); one that does carries four
  descriptors and one thread on linux from its start.
- **When the host refuses the hub the program does not start**: one
  stderr line, `wolf: the runtime could not make its signal hub (…); the
  program did not start`, and exit **71**, as for the readiness set.
- A caller of the runtime that did not start the hub (no compiled
  program) gets `io` from `listen` and `wait` instead of an abort.
- `os_signal_ignore`, `default`, `poll` and `raise` never made the hub
  on unix and are unchanged. lupin has no signal family.

Tests: `release_determinism` and `bench_hatches` stage each build's
source in a directory of their own, as `parked_tasks` does since s238
(#765). Two tests rewrote one witness file while the other built it (a
guard that does the same thing faster was red 8 of 10 at trunk: the
build read an empty file, `no main`), and every hatch build read the
sources the other tests were writing. Neither was ever seen red in CI.

### A signal to a process group and to every process; this process into a group; host signal numbers and a signal's words (s241, #739, #740, #741, #762; ruling #79)

`os_signal_send` reaches one process, so `kill 0`, `kill -1`, `kill --
-PGID` and a shell's `kill %job` could not be written, and a job's own
children (a script's `sleep`, a `make`'s compilers) outlived a signal to
its leader. Six host builtins, each on the checked machine, native and
release, as ruled (#79):

- **`os_signal_send_group(pgid, meaning) -> () ! {denied, invalid, io,
  not_found, unsupported}`** (`[os.signal.send.group]`) — `kill(-pgid)`:
  every member of the group this process may signal. `pgid` must be
  above **1** (`kill`'s -1 is every process, not group 1, so a group id
  computed wrong can never become a broadcast). `kill 0` is
  `os_signal_send_group(os_pgid()?, m)`.
- **`os_signal_send_all(meaning) -> () ! {denied, invalid, io,
  not_found, unsupported}`** (`[os.signal.send.all]`) — `kill(-1)`.
- **`os_set_pgid(pgid) -> () ! {denied, invalid, io, unsupported}`**
  (`[os.term.group]`) — the CALLING process into a group, 0 to lead one of
  its own (what an interactive shell does at start and GNU `timeout`
  before its command); a session leader, a group of another session or
  none is `denied`.
- **Host numbers `-N`** (`[os.signal.host]`): `os_signal_send`,
  `os_signal_send_group`, `os_signal_send_all` and `os_signal_text` take
  `-N` for the host's signal N, `1 <= N <= os_signal_last()` — the
  real-time and host-only signals (linux `STKFLT`, `IO`, `PWR`, 32–64;
  macOS `EMT`, `IO`, `INFO`) with no portability promise and **no new
  meaning bit**. `-N` is `os_wait_status`'s spelling of "ended by N".
  **`os_signal_last() -> int ! {unsupported}`** (64 linux, 31 macOS) and
  **`os_signal_realtime() -> (int, int) ! {unsupported}`** ((34, 64)
  under glibc, (35, 64) under musl, (0, 0) on macOS).
- **`os_signal_text(meaning) -> str ! {invalid, unsupported}`**
  (`[os.signal.text]`) — the C library's `strsignal` words, read in the C
  locale whatever `LANG` says ("Killed" under glibc, "Killed: 9" on
  macOS).

The rows are the host's, kept rather than evened out (R3): a group some of
whose members this process may signal is `ok` and reaches only those; a
group whose members have all ended unreaped is `ok` on linux and `denied`
on macOS; a linux `send_all` is `ok` when it finds only processes it may
not signal and `not_found` when it finds none. The sends and `os_set_pgid`
charge `exec`; `os_signal_last`, `os_signal_realtime` and
`os_signal_text` are uncharged; all six are refused at comptime. Windows
answers `unsupported` after the shape; lupin answers the shape rows and
refuses each well-formed call by name (its half: wolf-interp `s241`).
**Upgrade note:** `os_signal_send(pid, -N)` was `invalid` and is now a
send of host signal N; a program that relied on a negative meaning being
refused must check it. Host table 128 → 134 lines, prelude 135 → 141
functions, RT_SYMBOLS 215 → 221, anchors 652 → 658. Witnesses
(`group_lanes.rs`): the live sends and the move run inside a throwaway
`--init` container on linux, `send_all` only there; on macOS on CI's
runner, a program signalling only the groups it started.

### `parked_tasks` no longer races itself (s238, #733)

The gate failed in about one CI test job in five (22 jobs between
2026-10-12 and 10-14, linux and macOS, trunk too; "the witness starts:
NotFound", or dsymutil finding `waitwolf_native/w` gone). Two of its
tests built the same witness into one directory, and each build began by
deleting it, so one test removed the other's program while that one was
linking or starting it. Each build now has a directory of its own, and
`two_builds_of_the_witness_at_once_each_keep_their_own` holds that. On
kasumi under `taskset -c 0-3` with four busy loops the old gate was red 8
of 30 times and the fixed one 0 of 30, interleaved run by run. No runtime
or compiler change; no assertion, timeout or retry changed.

### A process out of descriptors no longer ends at its first park; a failed net call names its reason (s239, #750, #751; ruling #80)

The native runtime made its readiness set (epoll and an `eventfd` on
linux, a `kqueue` on macOS) at the first call that parked, and asserted
that the host gave it. A program that had used its descriptors up
before it first waited for a socket ended there, SIGABRT, after its own
lines had run; a client could end lobo 0.1.2 and 0.1.3 that way
(lobo#90). `os_signal_wait_fd` did the same with its wake pipe.

- **The set is made before `main`** (`[os.net.reactor]`), in a program
  that names a call which can park — `net_accept`, the reads, the
  writes. The entry shim calls `__wolf_rt_reactor_start` first
  (RT_SYMBOLS 215). A program that names none makes no set and no
  reactor thread, as before. Its cost where it is made: two descriptors
  and one thread from the start (linux), about 13 µs of a 450 µs start
  natively and 23 µs released.
- **When the host refuses it, the program does not start**: one line on
  stderr, `wolf: the runtime could not make its readiness set (Too many
  open files (os error 24)); the program did not start`, exit status
  **71** (`EX_OSERR`). Not a trap: no `wolf-trap:` line, not 134.
- **No park asserts on a host resource.** A wait that finds no set
  answers its call's `io` and asks again next time; `os_signal_wait_fd`
  answers `io` when it cannot have its descriptor and wake pipe; the
  checked machine's descriptor wait no longer remembers a refused pipe
  (it answered `io` forever after one refusal).
- **The net family sets `os_error()`** (`[os.net.error]`, #751), as the
  fs family does: a refused accept at the limit is `io` with 24
  (EMFILE) or 23 (ENFILE) beside it; a fired budget and an orderly close
  read 0. Native, release and the checked machine; lupin's half is
  wolf-interp's.
- **Measured** (kasumi, a 32-descriptor limit on the program alone): the
  witness (`fixtures/reactor/park_at_limit.lu`) ends with SIGABRT at
  `reactor.rs:139` on 0.2.29 and trunk, native and release, and answers
  the budget's `timeout` now; the checked machine and lupin 0.1.52
  always answered. lobo 0.1.4 with its workaround removed ends at the
  8th of 30 slow downloads at trunk and serves on now, 12 of 12
  downloads whole. Made at the first park instead, with a refusal `io`
  there (the other design measured), the same lobo stays up but cuts 2
  of 12 downloads it had begun: ruling #80 (R1–R6,
  `[os.net.reactor.note]`) takes the start.
- **Downstream**: lobo's workaround (one parked read at start, ws64) is
  no longer needed from this release; it costs nothing to keep.

### The checked machine answers shifts on plain integers and home-module methods (s237, #736, #745; ruling #77)

Two ordinary shapes had no record on the reference machine: `x << n` on
a plain integer ("this operator in checked execution") and `m.keys()`,
a method that resolves to the receiver's home module ("home-module
method calls in checked execution"). The second kept pelt off the
checked machine entirely.

- **A shift on a plain integer is arithmetic on its value**
  (`[type.int.shift]`, ruled by the maintainer 2026-10-14, ruling #77,
  option A; the alternatives weighed are a note in the clause). `x << n` is `x * 2^n` and
  traps `overflow` when that leaves the type; `x >> n` is the floor of
  `x / 2^n`; a negative count traps. A count at or past the width is no
  case of its own (`255 >> 8` at `u8` is 0, `0 << 8` is 0, `1 << 8`
  traps). `wrapping[T]` keeps the masked, bit-dropping shift it has
  always had, and the clause now says so.
- **This changes compiled programs. Read it before you bump.** The spec
  had no clause for a shift, and the machines did not agree: measured
  on 250 programs (ten types by twenty-five edges), native and release
  at 0.2.29 computed `wrapping[T]`'s answer on plain types and parted
  from lupin 0.1.52 on 155. `255 << 1` at `u8` printed 254, `1 << 8`
  printed 1, `255 >> 8` printed 255, `1 << 63` at `int` printed `MIN`,
  `8 >> -1` printed 0; lupin trapped or answered the value each time.
  All four machines agree on the 250 now. A program that shifts a set
  bit out of a plain integer, or shifts by the width or more, or by a
  negative count, traps `overflow` where it wrapped (or, for `>>` past
  the width, answers 0 or -1).
- **Upgrading from 0.2.29 or 0.2.30, read this.**
  1. A shift on a plain integer now traps `overflow` where it used to
     wrap: a set bit shifted out by `<<` (`1 << 63` on `int`
     included), a `<<` of a non-zero value by the width or more, and
     a negative count either way. A `>>` by the width or more now
     answers 0 (-1 for a negative value) instead of the masked shift.
  2. Code that wants the old bits says so with `wrapping[T]`
     (`(x as wrapping[u64] << n as wrapping[u64]) as u64` with `x`,
     `n: u64`; both operands of a shift have one type), where the
     count masks to the width and the bits that leave are dropped, as
     before.
  3. Measured downstream before the ruling, the same programs built by
     this compiler and by 0.2.29's trunk (s237's census): wolf-std
     (100 shift sites, all `wrapping`), lobo (95, all `wrapping`),
     boreutils (7, one plain), pelt (13, eleven plain) and pax (288,
     all plain) gave no new trap, no changed output and no changed
     exit status in their own suites (std-test and doc-examples; pelt's
     difftest and sessions; boreutils' 6,252 differential cases with
     killtest and roottest in containers; pax's core, net, TCP and UDP
     suites booted under QEMU). pax's variable-count shifts — signal
     bits, bitmap bits, retransmit backoff — are where a count of 0 or
     one past the width would now trap instead of misbehaving quietly.
  4. The release tier's code changes beyond the shifts themselves,
     through inlining (a checked shift is larger); behaviour does not.
- **The constant evaluator computes what the run time does.** It
  faulted on every count at or past the width (E0706) while the
  compiled `let` computed an answer; `const Z: u8 = 255 >> 8` is 0 now,
  and a `wrapping[T]` count masks at comptime as at run time. `const
  K: u8 = 200 << 1` stays E0706.
- **An ICE fixed on the way:** `x >> n` at `wrapping[u8]`, `[u16]` or
  `[u32]` with both operands known and a masked count of 0 folded to a
  constant the verifier refused ("lowered WIR failed verification",
  `iconst.i8 255`), on native and release.
- **A home-module method call runs on the checked machine** (#745): it
  is the free call with the receiver first (`[type.method.resolve]`),
  through the one path a free call takes. pelt at `00e15ca` then runs:
  of its 960 differential cases fed on standard input, none meets a
  refusal that names a construct. 930 run to an exit status, 7 end at
  the machine's step or call-depth budget, 22 leave no record (in 18
  the shell's `exec` replaces the machine's own process; in 4 the
  shell dies of a signal it sent itself) and 1 passed 120 s.
- **Measured:** `shift_lanes` (the rule written a second time; ten
  types, every edge, checked, native, release and lupin 0.1.52) and
  `home_method_lanes` (method and free spellings over a fixture std
  tree); eight corpus rows. `shr_range_lanes` row 2, which pinned the
  shift refusal since s214, asserts the answer on four machines.
  lupin 0.1.52 and 0.1.53 trap on a count of 128 or more where the
  rule answers (pinned by version and release commit, measured on both
  archives) and do not range-check a constant initialiser; lupin 0.1.54
  answers both (wolf-interp is76, #251).

## 0.2.30 — 2026-10-15

**wolf 0.2.29's native and release tiers panic on seven constructs in a
straight-line function. This release fixes it (s755, #755); upgrade.**
A function with no `if`, loop, call or index that declares a local,
then uses one of these, then reads the local, made `wolf build`, `wolf
run` and `conform-run --native`/`--release` panic ("use of a variable
with no definition on some path", `wolf_wir/src/build.rs`) on 0.2.29:

- `==`, `!=` and `<` (and `<=`, `>`, `>=`) on two `str`s;
- a narrowing or sign-changing integer `as` (`x as i8`, `x as u64`, `n
  as int` with `n: u64`);
- `int as char`;
- a signed `wrapping` `/` or `%` whose divisor is not a constant.

The checked machine and lupin always answered these programs. The fix
changes nothing in a program 0.2.29 already compiled (s233's new
refusals, below, are a separate change); the seven rows of `corpus/typecheck/straight_line_*` panic on the 0.2.29 archive
and run on every machine here (`straight_line_lanes`).

THE THIRTIETH. Four compiler lanes land, two with a lupin half, and the
pairing moves to lupin 0.1.53, which mirrors them: the crash fix above
(s755); a function that may leave a proc running is held to the frame
rule at every call (s233, #678, ruling #71); what a line editor asks of
the terminal (s234, #719, ruling #73); who a process runs as, a file's
owner, and the permanent drop to another user (s236, #633, #701, #742,
ruling #78). CI runs in tiers (tl21). The bundled standard library
moves to wolf-std `fd6bcdc`. 89 commits since 0.2.29 (s233 21, s234 29,
s236 25, s755 8, tl21 6), then r35's release commits.

### Read this before you bump the pin

**The crash fix (s755)** is the reason for this release: any program
built for native or release on 0.2.29 may hit it, and the fix changes
nothing else a program can see.

**Some programs that compiled on 0.2.29 are refused now (s233, E1010).**
A function that spawns a proc whose arguments can name a parameter's
bytes (a `str`, a fn value) or its caller's region must meet that proc
on every path out — `p.join()`, an exit received from `p.monitor()`, or
`p` handed to a function that does so (a kill is not a join). One that
does not leaves the proc running, and every call to it that hands it
bytes from a region block, a region value made in the frame or the
proc's own region is E1010 — on 0.2.29, 17 of the 25 such shapes
measured printed the bytes the next region reused on native and
release. **The fix-it:** join the proc (or receive its
exit) on every path before the function returns, or build the argument
outside the region. Measured downstream at the memory check: 0 of 1,383
records moved; lobo compiles unchanged.

**The terminal mode has two more bits (s234, ruling #73 R4):** `FLOW` (8,
`IXON`) and `EXTENDED` (16, `IEXTEN`). A mode read with `os_term_mode`
and set back still round-trips. **A mode built by hand without bits 3
and 4 now turns flow control and the host's extended input processing
off**, where 0.2.29 left them as found; a program that changes only the
bits it means to is unaffected. (0.2.29 answers a mode carrying bit 3 or
4 with `invalid`.) **The bundled std's `std.term` keeps them:** at
wolf-std `fd6bcdc` (sc61) its `Mode` gains `flow` and `extended`, `pack`
and `unpack` carry bits 3 and 4, so `term.set_mode(tty, term.mode(tty)?)`
round-trips exactly, and `term.raw` clears both, as `cfmakeraw(3)` does
(so Ctrl-S, Ctrl-Q, Ctrl-V and Ctrl-O reach a raw-mode program; sc61's
ruling R1 is owed, implemented as recommended). A `Mode` struct literal
must now name the two fields; no package outside wolf-std builds one.

**`os_set_ids` is whole or the process ends (s236, ruling #78 R4).** It
returns with every id as asked or every id as it was: a refusal before
any change returns its row; a refusal after one is put back, read back
and then returned; if the put-back fails, the ids read back are not the
ones asked for, or an old id can still be taken, the process writes one
line to standard error and aborts. linux serves it; macOS and freebsd
answer `unsupported`; windows answers `unsupported` for all eight new
calls. Set a file's owner before its mode: the host clears set-user-id
and set-group-id when an owner changes.

**The bundled std is wolf-std `fd6bcdc`** (sc60: std at 0.2.29 / lupin
0.1.52; then sc61: `std.term` keeps `flow` and `extended`), where 0.2.29
shipped `d7c2968`. `std.signal` names all twenty-eight meanings (the
twenty listenable and the eight send-only: `signal.interrupt()` …
`signal.bad_syscall()`, `continued()` for `CONTINUE`), with
`signal.send`, `signal.number`, `signal.ignored`, `env.pid` and
`env.ppid` (ruling #74); `tests/signal/rows.lu`, 0.2.29's one red std
row, moved its outside bit to 2^62. It has **no wrappers for this
release's eleven builtins**: s236's eight (wolf-std#65 adds them and
needs this release) and s234's three (`os_term_size`, `os_term_chars`,
`os_signal_wait_fd`; owed at the next std pin). Call them directly;
they are in the prelude.

- **Measured under this release** (kasumi, linux x86-64, the
  release-head archive with lupin 0.1.53 beside `wolf`, no `WOLF_STD`):
  wolf-std `fd6bcdc`'s 460 tests on lupin, the checked machine and
  native, and its 481 doc examples: **all green**. The release machine
  equals native on 459 of the 460 with no std configured; the one
  (`list/sort_by_tier.lu`) is refused by name on release, as at 0.2.29.
  Against the published 0.2.29 on the same std, 0 records moved on any
  tier.

**New prelude names** (`cargo xtask prelude-diff v0.2.29 HEAD
--markdown`, after `git fetch --tags`, run at the release head):

> **New prelude names.** HEAD adds `fs_set_owner`, `fs_set_owner_fd`,
> `os_uid`, `os_euid`, `os_gid`, `os_egid`, `os_groups`, `os_set_ids`,
> `os_term_size`, `os_term_chars`, and `os_signal_wait_fd` (all eleven:
> function, `[os.host.sigs]`). A module that declares a function, type
> or binding named any of them now draws W0304 (it shadows the ambient
> name), which `--deny-warnings` makes fatal: rename yours.

A grep for the eleven in wolf-std `fd6bcdcb`, boreutils `9819e449`, lobo
`570e2a95`, pelt `00e15ca4`, pax `c81d464f`, muzzle `44387d08`, the book
`0a5fecc0` and wolf-lsp `097bbcf0` found **no declaration**; one use, in
boreutils' `notes/bu21/census/missing/euid/p.lu` (a probe for the
missing builtin, not built). s233 and s755 add no prelude name.

**Capabilities.** `os_term_size` and `os_term_chars` are `io`
(uncharged, as `os_term_mode`); `os_signal_wait_fd` and `os_set_ids`
charge `exec`; the five id reads charge `env`; `fs_set_owner` and
`fs_set_owner_fd` charge `fs`. A package that calls one without the
declaration is E1504.

**Every hosted binary's bytes may move:** `libwolf_rt.a` gains s234's and
s236's shims (`RT_SYMBOLS` 214), and s755 changes how a straight-line
function's locals are lowered. No handle number, error row or exit
reason moves.

**Known:** the `parked_tasks` witness build can fail on macOS
(`dsymutil`, then a missing `waitwolf_native/w`; #733, open), and
`region_copyout_lanes::a_loop_keeping_a_scalar_is_flat` can exceed its
RSS bound under load (#652, open); both are test flakes, not program
behaviour. The plain-integer shift rule (`[type.int.shift]`, ruling #77
A; #748) is **not** in this release: it lands in 0.2.31.

**For contributors: CI runs in tiers (tl21).** A pull request's push runs
`tier pr` (linux lint, test and corpus, the fuzz smoke); trunk, the
nightly (02:07 UTC), a dispatch and the `full-matrix` label run `tier
full` (adding macOS, windows and the freebsd cross-check); a prose-only
change runs `tier noop`. **A PR merges only with a green `tier full` at
its head**: open it without the label, push, then add `full-matrix` last
(CONTRIBUTING).

**Rulings made, each as implemented:** #71 (s233, `[conc.proc.arg]`'s
must-join extension, A); #73 (s234, R1–R6: `os_term_size`, `RESIZE`
unchanged, `os_signal_wait_fd`, `os_term_chars` with `FLOW` and
`EXTENDED`, the capabilities, lupin refuses the three by name); #78
(s236, R1–R10 and the std spellings S1–S4: the four id reads and
`os_groups`, `fs_set_owner`/`fs_set_owner_fd`, one `os_set_ids`, whole
or fatal, the read-back and the take-back check, the capabilities,
macOS, windows, the checked machine, lupin).

**Corrections to 0.2.29's notes (#749).** Its head counted 56 commits
since 0.2.28 (the three lanes); `git rev-list --count v0.2.28..v0.2.29`
is **70**: the 56 lane commits and r34's 14 release commits. And its
muzzle line called muzzle#3 open: it was merged on 2026-10-12, before
0.2.29 was tagged (muzzle moved to 0.2.29 in mz2). `docs/platforms.md`
still has no windows rows for 0.2.29's signal and pid builtins (#749
stays open for that).

**What each downstream does to bump.**

- **wolf-std, first:** pins to 0.2.30 / 0.1.53; then **#65** (s236's
  wrappers: `env.uid`, `euid`, `gid`, `egid`, `groups`, `fs.set_owner`,
  `process.set_ids`; a draft that is red at the old pin by construction)
  and wrappers for s234's `os_term_size`, `os_term_chars` and
  `os_signal_wait_fd`. (`std.term`'s `flow` and `extended` are already in
  this release's std, sc61.) A later compiler release bumps `STD-PIN` to
  carry them.
- **pelt:** the toolchain digests; the line editor adopts the terminal
  surface (#719, W05): `os_term_size` and `RESIZE` instead of the
  cursor-position report, `os_signal_wait_fd` instead of `VTIME 1` and a
  poll, `os_term_chars` for the user's keys, `FLOW`/`EXTENDED` off in
  raw mode. A hand-built mode there must now name bits 3 and 4 to keep
  `IXON`/`IEXTEN`.
- **boreutils:** the toolchain digests; the six identity tools of
  boreutils#32 — `id`, `whoami`, `groups`, `logname`, `chown`, `chgrp` —
  can be written on `os_uid` … `os_groups` and `fs_set_owner` (names for
  ids come from the account files; `logname` also needs the login
  name). Remove the staged `.wolf-bin/std` before unpacking its own std
  pin.
- **lobo:** the toolchain digests; the privilege drop after binding a
  low port (lobo#52) can be written on `os_set_ids`; `os_euid() == 0`
  replaces reading `/proc/self/status`.
- **wolf-book, then the site:** the book pins 0.2.30 / 0.1.53; its
  signals page is still missing (wolf-book#79: the pages and how a
  reader spells a meaning, now that `std.signal` names all
  twenty-eight). Then wolf-web pins the pair and the book, and deploys.
- **pax:** the pin (it builds from the archive). The eleven new builtins
  are hosted: the freestanding target refuses each by name.
- **wolf-lsp and tree-sitter-wolf:** the pin, and the eleven names in
  their vendored `prelude.json` (prelude functions do not paint as
  builtins, ruling #77).
- **muzzle:** the pin; it uses none of the new names.
- **Homebrew and the AUR:** the std beside the binary moves to `fd6bcdc`
  (this release's tap change).

### The pairing: lupin 0.1.53

`PAIRING` names **lupin 0.1.53 at pin `294d626`** (wolf-interp release
413375414; the linux x86-64 archive's sha256 `5db55332…`). The spec
pin is unchanged (this compiler's v0.2.24 tag). 0.1.53 carries the lupin
halves of s234 and s236; s233 and s755 have none.

- **Dropped 7 cases in three gates**, each green on the published 0.1.53
  unpinned: `ids_lanes` 2 and `term_editor_lanes` 4 (s236's and s234's
  mirrors; each pre-mirror list stays, empty, as the gate's machinery)
  and `pairing` 1.
- **Carried to 0.1.53, measured** (each answers as 0.1.52 on the pinned
  rows; the issues are open): `checked_tasks_lanes`' two partings
  (wolf-interp#227, #230), `region_kept_arg_lanes`' seven `List` and
  `Map`-key rows and `chan_sent_arg_lanes`' channel-payload rows (both
  wolf-interp#232), attr_closed_set's control (lupin still declines its
  `comptime fn`); the racy counter's `trap(race)` check names 0.1.53 too;
  `int_truth_lanes`' `PIN_INT_NOT` names 0.1.48 alone and is inert.
- **Widened 0.** The plant (the 0.1.53 lists against the published
  0.1.52) is red on exactly those 7 cases.
- **The ritual** (#87, with #281's control against the 0.1.52 archive,
  the same corpus and the same release `wolf`, 1141 files):
  - checked: **moved 2 ledger counts**. Agreements 773 → 775,
    unsupported 96 → 94; SOUNDNESS 0 and hard 19 unchanged.
  - native: **moved 2**. Agreements 758 → 760, unsupported 97 → 95;
    SOUNDNESS 14 and hard 33 unchanged.
  - The same two files on both, `os/ids_rows.lu` (s236) and
    `os/term_editor_rows.lu` (s234), from unsupported to agreement.
    Nothing moved below the ledger.

### A function that may leave a proc running is asked at every call (s233, #678; ruling #71)

`fn helper(s: str) { let _w = spawn proc child(s, 0) }` called from
`region pass { helper(s) }` compiled: `helper` returned while `child`
slept, the block freed `s`'s bytes, and native and release printed the
chunk the next region reused (the checked machine the stale bytes).
s229 refused the spawn when its argument was built in the spawning
frame's own region block; a parameter's bytes passed, because they
outlive the frame — and the proc may outlive it too.

- **The must-join check** (`[conc.proc.arg]`, `wolf_mem::procjoin`). A
  spawn whose arguments can name a parameter's bytes, or the caller's
  region's, must be met on every path out of the function: `p.join()`,
  an exit received from `p.monitor()`, or `p` handed to a function that
  does so. A kill is not a join (#683). An `int` or `bool` set from a
  literal is followed across an `if`, so lobo's `budget.run_small`
  (`cls = 4` on the timeout arm, then `if cls == 4 { end_body(w) }`)
  is read path by path.
- **A function that misses one leaves a proc running**, and every call
  to it is E1010 at the argument whose bytes live in a region block, a
  region value made in the frame or the proc's own region — or at the
  call, when the function builds the bytes in its caller's region. A
  caller handing on a parameter leaves the proc running in turn
  (computed once per package; a call through a trait bound answers for
  every impl). A closure that may return with such a proc still
  running, and such a function taken as a fn value, are refused where
  they are written.
- **Measured:** 25 refused shapes and 12 sound ones on checked, native,
  release and lupin 0.1.52 (`proc_must_join_lanes`); 17 of the refused
  printed the reused chunk on native and release before. Downstream at
  the memory check, 0 of 1,383 records moved; lobo compiles unchanged,
  and the same lobo with `end_body(w)` replaced by `w.kill()` is
  refused at its three region blocks. Still refused though sound:
  s229's proc joined inside the block that built its argument, a join
  through a value the dataflow does not carry, a handle joined out of a
  list.

### What a line editor asks of the terminal: its size, a wait a signal ends, its special characters (s234, #719; ruling #73)

pelt's line editor (sh11) worked around four gaps (#719, W05): it asked
the terminal for its width with a cursor-position report before every
prompt (0.3 s on a terminal that never answers), redrew a wrapped line
at the old width after a resize until the next prompt, read keys with
`VTIME 1` and polled for signals between reads — ten wake-ups a second
at an idle prompt and up to 0.1 s before a `kill -INT` acted — and could
not learn the user's erase, kill or interrupt characters, or turn flow
control and the host's extended input processing off (so Ctrl-S and
Ctrl-Q stayed flow control, and on macOS Ctrl-V and Ctrl-O never reached
the program).

- **`os_term_size(fd) -> (int, int) ! {io, unsupported}`**
  (`[os.term.size]`): the window's rows and columns as the terminal
  holds them (`TIOCGWINSZ`); 0 for a size it was never told.
- **A resize is s232's `RESIZE`** (bit 16384, `SIGWINCH`). #719 asked for
  it at bit 512, which s232 gave to `USER`; nothing changes here, and a
  set with a bit outside the listenable meanings is already `io`.
- **`os_signal_wait_fd(fd, set) -> int ! {io, unsupported}`**
  (`[os.signal.wait.fd]`): one wait that a key and a signal both end — it
  answers the meaning delivered (taken off the queue, first when both
  are ready), or 0 when the descriptor has a byte, the end of input or a
  hang-up, and reads nothing. It is `poll(2)` on the descriptor and a
  wake pipe the signal hub writes when it queues a meaning: an idle wait
  wakes for nothing, and a `kill -INT` is answered in about 0.2 ms
  (measured on linux, every tier).
- **`os_term_chars(fd) -> List[int] ! {io, unsupported}`**
  (`[os.term.chars]`): erase, kill, interrupt, quit, suspend, end of
  file, word erase, literal next, start, stop, reprint, discard, in that
  order, -1 for one that is disabled.
- **The mode gains `FLOW` (8, `IXON`) and `EXTENDED` (16, `IEXTEN`).** A
  mode read and set again still round-trips. **The one change of
  meaning:** a mode built by hand without these two bits now turns
  `IXON` and `IEXTEN` off where it used to leave them as found; a
  program that changes only the bits it means to is
  unaffected. Bit 3 was `invalid` before; bit 5 (32) is the first that
  still is.
- Every machine: native and release through `wolf_rt`, the checked
  machine on the host's own terminal (its wait is the host's `poll(2)`
  on the descriptor and a pipe its handler writes), and the freestanding
  target refuses each by name. lupin refuses all three by name after
  their shape rows (no `unsafe`, and std has no `ioctl`, `tcgetattr` or
  `poll`). Windows: `unsupported`, as every `[os.term]` call.
- Capabilities: the size and the characters are `io` (uncharged, as
  `os_term_mode`); the wait is `exec` (as `os_signal_wait`). Host table
  120, RT_SYMBOLS 206.
- **Ruled** (#73, `[os.term.editor.note]` R1–R6), each as implemented.

### Who a process runs as, a file's owner, and the drop (s236, #633, #701, #742; ruling #78)

A wolf program could not tell whether it ran as root (lobo read
`/proc/self/status`, and on macOS the exit code of a spawned `id`),
could not change a file's owner (`chown`, `chgrp`, `cp -p` and
`install -o` could not be written), and could not stop being the
super-user (`su`, `login`, a server after it binds its low port). No
machine resolved a name for any of it.

- **`os_uid()`, `os_euid()`, `os_gid()`, `os_egid()` `-> int !
  {unsupported}`** and **`os_groups() -> List[int] ! {io,
  unsupported}`** (`[os.proc.ids]`): the real and effective user and
  group ids, and the supplementary groups as `getgroups` answers them,
  in the host's order (whether the effective group is in the list is
  the host's). "Am I the super-user?" is `os_euid()? == 0`.
- **`fs_set_owner(path, uid, gid, follow) -> () ! {denied, invalid, io,
  not_found, unsupported}`** and **`fs_set_owner_fd(fd, uid, gid) -> ()
  ! {denied, invalid, io, unsupported}`** (`[os.fs.owner]`): `chown` by
  path — `-1` leaves an id alone, `follow` false changes a link itself —
  and on a handle. An id outside `-1` and `0..=4294967294` is `invalid`
  before the host is asked, and so is an id the host has no mapping for
  (a user namespace). The host clears set-user-id and set-group-id when
  an owner changes, so set the owner first and the mode after.
- **`os_set_ids(uid, gid, groups) -> () ! {denied, invalid, io,
  unsupported}`** (`[os.proc.ids.set]`): this process becomes that user
  for good — real, effective and saved ids, and exactly those
  supplementary groups — in the one order that works: the groups, the
  group ids, the user ids. The other order leaves the old group id and
  the old groups behind (measured as root in a container: `uid 1000`,
  `gid 0`, groups `[0, 4, 27]`), and a program cannot write it with this
  call. **The call returns with every id as asked or every id as it
  was, and never in between:** a refusal before any change returns its
  row; a refusal after one is put back and read back first; and if the
  put-back fails, the ids read back are not the ones asked for, or an
  old id can still be taken, the process ends — one line on standard
  error and an abort. Every thread of the process changes together.
- **Hosts.** linux serves all eight. macOS and freebsd serve the reads
  and the owner calls and answer `unsupported` for `os_set_ids` (macOS
  has no way to read a drop back). windows answers `unsupported` for
  all eight, after the shape rows. The freestanding target refuses each
  by name.
- **Machines.** checked, native and release make the same host calls in
  the same order; the checked machine sets the real ids of the `wolf`
  process the program runs as, as `os_umask` sets the real mask. lupin
  serves the two owner calls and refuses the five reads and
  `os_set_ids` by name (wolf-interp's s236, released in lupin 0.1.53;
  0.1.52 resolved none of the eight).
- **Capabilities.** The five reads charge `env`, the owner calls `fs`,
  `os_set_ids` `exec`.
- **Witnessed** (`ids_lanes`, checked, native, release and lupin): the
  ids held to `id` and the groups to the host's own list; on a host,
  only calls that change nothing and the refusals a user gets. The drop
  and the owner changes run as the super-user inside a throwaway
  `--network none` container with read-only mounts (docker in CI, where
  a missing engine is a failure; podman on a developer's box, or a loud
  skip): the drop read back in every thread and by a child `id`, the
  way back `denied`, a drop refused at its last step (`--cap-drop
  SETUID`) put back whole, and a user's `chgrp` to a group it is in and
  to one it is not.
- **Counts:** the host table 120 → 128 lines, `RT_SYMBOLS` 206 → 214,
  the prelude 127 → 135 functions and 159 → 167 W0304 names, anchors
  644 → 648. A program that defines a top-level `os_uid`, `os_euid`,
  `os_gid`, `os_egid`, `os_groups`, `os_set_ids`, `fs_set_owner` or
  `fs_set_owner_fd` now gets W0304; none of boreutils, pelt, lobo, pax,
  wolf-std or muzzle does.
- **Ruled** (#78, `[os.proc.ids.note]` R1–R10 and the std spellings
  S1–S4, each as implemented): the spellings (`fs_set_owner`, where #701 and px38 say
  `fs_chown`), one call or three for setting ids, what a failed drop
  does, the check, the capabilities, macOS, windows, the checked
  machine, lupin. **Not done:** `fs_access` (#633's second ask), a
  temporary change of the effective id, names for ids (wolf-std's
  `std.os.users` reads the account files).

### A local survives a branch inside an expression in a straight-line function (s755, #755)

wolf 0.2.29 panicked on native and release ("use of a variable with no
definition on some path") compiling a function with no `if`, loop,
call or index that declared a local, then used one of these, then read
the local:

- a narrowing or sign-changing integer `as` (`x as i8`, `x as u64`,
  `n as int` with `n: u64`, `b as i8` with `b: u8`);
- `int as char`;
- a signed `wrapping` `/` or `%` whose divisor is not a constant;
- `==`, `!=`, `<`, `<=`, `>` or `>=` on two `str`s.

Such a function is lowered as one block, and its locals were kept for
that block alone; each construct above branches inside the expression
(to its trap, or a comparison loop), which ended the block under the
local. The definitions now outlive the block. The checked machine and
lupin always answered these programs; no program that compiled before
compiles differently. Measured: the seven rows of
`corpus/typecheck/straight_line_*` panic on the 0.2.29 archive and run
on every machine now (`straight_line_lanes`).
## 0.2.29 — 2026-10-12

THE TWENTY-NINTH. Three compiler lanes land, one with a lupin half, and
the pairing moves to lupin 0.1.52, which mirrors it: a program may park
a task per connection, with no cap and no cpu while parked (s231,
#713); the shell's signals, every signal POSIX names, what was ignored
at start, a poll that never trails a delivery, this process's id and a
send to any pid (s232, ruling #70); the language server finds the
standard library (s235, #724). The bundled standard library moves to
wolf-std `d7c2968`. 56 commits since 0.2.28 (s231 17, s232 25, s235 14),
then r34's release commits; s235 merged while the release lane ran and
was taken into it.

### Read this before you bump the pin

**The signal set has twenty meanings (s232, ruling #70).** `512` is
`USER` (`SIGUSR1`) now, and bits 9..19 are the eleven new meanings a
program may listen for; bits 20..27 may only be sent. Every call that
takes a set (`os_signal_listen`, `wait`, `poll`, `ignore`, `default`)
answers `io` for an empty set or one holding any bit that is not a
listenable meaning, and changes nothing: a listen that dropped such a
bit and answered ok, and a wait or poll that ignored it, now refuse. A
program that used `512` as "a bit outside the meanings" gets ok where it
got `io` (s219's `job_rows` did; it moved to `KILL`'s bit). A child now
keeps an ignored `BROKEN_PIPE`.

**A signal bit is permanent (`[os.signal.set.stable]`, new):** a
published meaning keeps its bit forever and a new one takes the next
unused bit; name meanings, never literal bits or host numbers
(`os_signal_number` is the one way to a host number).

**The bundled std's `std.signal` predates the twenty meanings.** wolf-std
`d7c2968` was written against 0.2.28: `std.signal` names the nine
meanings (`reload()` … `child()`), its header says a bit outside the
nine is `io`, and it has no function for the eleven new meanings and no
wrapper for the five new builtins. Its calls hand any bit straight to
the builtins, so a program that passes it `512` gets `USER`'s answer.
Until wolf-std catches up, call the new builtins directly (they are in
the prelude) and write the new bits as numbers.

**The bundled std is wolf-std `d7c2968`** (sc59: std at 0.2.28 / lupin
0.1.51), where 0.2.28 shipped `98e1b7b`. It carries s230's wrappers,
which 0.2.28's bundled std lacked: `fs.set_mode`, `create_private`,
`create_with_mode`, `create_dir_mode`, `symlink`, `link`, `fs.Time`,
`set_times`, `set_link_times`, `truncate` and `env.umask`; and
`tests/ledger.toml` records the checked machine running
`mem/budget/breach_is_a_row.lu` (0.2.28's one red row). On windows,
`fs.truncate` on a handle opened for append fails (#723): open it for
writing.

- **Measured under this release** (kasumi, linux x86-64, the
  release-head archive with lupin 0.1.52 beside `wolf`, no `WOLF_STD`):
  wolf-std `d7c2968`'s 449 tests on lupin, the checked machine and
  native, and its 477 doc examples, compile and answer as under 0.2.28
  except **one test, `tests/signal/rows.lu`**: its `signal.reset(512)`
  expects `io` (512 was "a bit outside the meanings") and now gets ok on
  every compiler tier, because 512 is `USER`; lupin 0.1.52 answers the
  file `unsupported`, refusing `os_signal_default` by name once the
  shape row passes (ruling #70 R9). `cargo xtask std-test` reads the row
  as red until wolf-std's directive moves. Nothing in it answers a wrong
  value. The release machine equals native on 448 of the 449 with no
  std configured; the one (`list/sort_by_tier.lu`) is refused by name
  on release, as at 0.2.28.

**The worker pool grows a worker per parked task (s231, #713).** A
program that parks more tasks than eight a core (sixteen on one or two
cpus) in `net_read`, `net_accept`, a channel `recv`, a `when` or a
sleep no longer stops: each parked task holds a thread (about 15 KiB on
linux x86-64) and costs no cpu. The host's thread limit is the last
word; a thread it refuses is one line on stderr, never an abort. A
server that holds a task per connection now holds as many threads, so
the host's limits (`ulimit -u`, `kernel.threads-max`) bound it, where
the cap stopped it first. `os_signal_wait` still polls.

**`wolf lsp` finds std as `wolf build` does (s235, #724).** An editor
that ran `wolf lsp` saw a false E0301 ("no standard library is
configured") on every std method outside the builtins. The server now
reads, in order: `wolf lsp --std-root <dir>` (new), `WOLF_STD`, a `std`
path dependency in the opened file's `wolf.pkg`, then the `std/`
installed beside `wolf`; a freestanding manifest target reads none, and
a `WOLF_STD` that is not a directory exits 2 with build's message.
Rename now refuses a std or dependency declaration by name. A tool
embedding `wolf_query` meets contract v6 (`QueryHost::with_roots`).
**Known:** the `parked_tasks` witness build can fail on macOS
(`dsymutil`, then a missing `waitwolf_native/w`; #733, open).

**Every hosted binary's bytes move** (`libwolf_rt.a`: s231's pool,
parked registry and waits; s232's signal table, self-pipe poll and five
shims). No handle number, error row or exit reason moves.

**New prelude names** (`cargo xtask prelude-diff v0.2.28 HEAD
--markdown`, after `git fetch --tags`, run at the release head):

> **New prelude names.** HEAD adds `os_signal_ignored`,
> `os_signal_send`, `os_signal_number`, `os_pid`, and `os_ppid` (all
> five: function, `[os.host.sigs]`). A module that declares a function,
> type or binding named any of the five now draws W0304 (it shadows the
> ambient name), which `--deny-warnings` makes fatal: rename yours.

A grep for any of the five in wolf-std `d7c2968c`, boreutils
`2ab36513`, lobo `944edc1b`, pelt `96828ac4`, pax `6316c51c`, muzzle
`7c95300e` and the book `b845e077` found **no declaration and no use**.
s231 and s235 add no prelude name.

**Capabilities (ruling #70 R8).** `os_signal_send` charges `exec`;
`os_pid`, `os_ppid` and `os_signal_ignored` charge `env`;
`os_signal_number` charges nothing. A package that calls one without
the declaration is E1504.

**Spellings ruled, each as implemented:** #70 (s232, R1–R9: every
signal POSIX names, by meaning; one shape rule; `os_signal_ignored`; a
poll that settles and reads the self-pipe; `os_pid`, `os_ppid`;
`os_signal_send` to one pid; `os_signal_number`; the capabilities;
lupin serves what the standard library reaches and refuses the rest by
name).

**What each downstream does to bump.**

- **wolf-std, first:** pins to 0.2.29 / 0.1.52 in `vendor/tools.toml`;
  `std.signal` learns the twenty meanings (eleven more functions; the
  header's "nine"), `tests/signal/rows.lu` moves its outside bit off
  `512` to one that is not listenable (s232 moved `job_rows` to `KILL`'s
  1048576), and wrappers for `os_pid`, `os_ppid`, `os_signal_send`,
  `os_signal_number` and `os_signal_ignored`. A later compiler release
  bumps `STD-PIN` to carry them.
- **wolf-book, then the site:** the book pins 0.2.29 / 0.1.52 (it is at
  0.2.28 / 0.1.51); no sample uses the five new names. Then wolf-web
  pins the pair and the book, and deploys.
- **boreutils:** the toolchain digests (remove the staged
  `.wolf-bin/std` before unpacking its own std pin, as at 0.2.27).
  `kill` can be written on `os_signal_send` and `os_signal_number`.
- **lobo:** the toolchain digests; a server holding a task per
  connection no longer stops past eight a core (s231).
- **pelt:** the toolchain digests; `trap` for every signal, `$$` and
  `$PPID` (`os_pid`, `os_ppid`), `kill` (`os_signal_send`), the set it
  was started ignoring (`os_signal_ignored`); the `settle` barrier can
  go (s232 measured twelve more pending cases passing).
- **wolf-lsp:** the pin; the server it drives now loads std (hover,
  definition into the std file, member completion of home-module
  methods), and `wolf lsp --std-root` is available to its clients.
- **pax:** the pin. The five new builtins are hosted: the freestanding
  target refuses each by name.
- **muzzle:** `capabilities: [ffi]` in `src/wolf.pkg`, owed since 0.2.27
  (muzzle#3 is open for it).
- **Homebrew and the AUR:** the std beside the binary moves to
  `d7c2968` (this release's tap change).

### The pairing: lupin 0.1.52

`PAIRING` names **lupin 0.1.52 at pin `294d626`** (wolf-interp release
410845419; the linux x86-64 archive's sha256 `f5194b74…`). The spec pin
is unchanged (this compiler's v0.2.24 tag). 0.1.52 carries the lupin
half of s232; s231 and s235 have none.

- **Dropped 6 cases in two gates**, each green on the published 0.1.52
  unpinned: `signal_shell_lanes` 5 (s232's mirror; its pre-mirror list
  stays, empty, as the gate's machinery) and `pairing` 1.
- **Carried to 0.1.52, measured** (each answers as 0.1.51 on the pinned
  rows; the issues are open): `checked_tasks_lanes`' two partings
  (wolf-interp#227, #230), `region_kept_arg_lanes`' seven `List` and
  `Map`-key rows and `chan_sent_arg_lanes`' channel-payload rows (both
  wolf-interp#232), attr_closed_set's control (lupin still declines its
  `comptime fn`); the racy counter's `trap(race)` check names 0.1.52 too;
  `int_truth_lanes`' `PIN_INT_NOT` names 0.1.48 alone and is inert.
- **Widened 0.** The plant (the 0.1.52 lists against the published
  0.1.51) is red on exactly those 6 cases.
- **The ritual** (#87, with #281's control against the 0.1.51 archive,
  the same corpus and the same release `wolf`, 1130 files):
  - checked: **moved 1 ledger count**. Agreements 764 → 765,
    unsupported 94 → 93; SOUNDNESS 0 and hard 19 unchanged.
  - native: **moved 1**. Agreements 749 → 750, unsupported 95 → 94;
    SOUNDNESS 14 and hard 33 unchanged.
  - The same file on both, s232's row `os/signal_shell_rows.lu`, from
    unsupported to agreement. Nothing moved below the ledger.

### The language server finds the standard library (s235, #724)

`wolf lsp` never loaded std: in an editor every method of a std data
type outside the builtins (`Map.keys`, `Map.has`, …) was a false
E0301, "no standard library is configured", on files `wolf build`
compiles clean — nine of them on pelt's `sh/` package at 0.2.28.

- **The server finds std as `build` does**, by the same functions:
  `wolf lsp --std-root <dir>`, then `WOLF_STD` (a root that is not a
  directory exits 2 with build's message), then a `std` path dependency
  in the opened file's `wolf.pkg`, then the `std/` installed beside
  `wolf`; a freestanding manifest target reads no default. A
  `wolf.pkg`'s dependency roots reach the server too.
- **Hover on a std method** shows its module, signature and doc comment;
  **definition** opens the std file itself (a `file://` path, not the
  `std://` display); **member completion** on a `Map`, `List`, `str` or
  range lists its home module's methods once the package calls one;
  **rename** refuses a std or dependency declaration by name.
- `wolf_query`'s contract is v6: `QueryHost::with_roots` takes the
  resolver; `QueryHost::new` is the std-less host it always was.

### A program may park a task per connection (s231, #713)

A task parked in a wait — `net_read`, `net_accept`, a channel `recv` —
holds its thread, and the pool adds a worker beside it. That stopped at
eight workers a core (sixteen on one or two cpus): a program that
parked more tasks at once stopped, the task that would have woken them
queued behind them. #713's witness (`waitwolf loop 0 N`, 2N + 1 parked
tasks) stopped at N = 8 on one cpu, 16 on four and 64 on sixteen.

- **No cap.** The pool holds a worker for every parked task; the host's
  thread limit is the last word, and a thread it refuses is one line on
  stderr, never an abort. The witness answers at N = 64 on one cpu and
  N = 1000 on four and sixteen, native and release
  (`parked_tasks.rs`); 10,000 tasks parked in `recv`, `net_read` or
  `net_accept` are held and released.
- **A parked task costs no cpu.** Every parked wait used to wake each
  5 ms to look for a cancellation (~200 wakeups a second a task; with
  10,000 parked, more than the machine). A cancellation now wakes the
  waits; one task parked 8 s in `net_accept` makes 8 `futex` calls in
  its whole run, from 1,741. Idle extra workers that cannot retire
  sleep instead of waking every 50 ms. `os_signal_wait` still polls.
- **About 15 KiB a parked task** on linux x86-64 (19.6 in `net_read`),
  and task stacks take no transparent huge pages, which made one cost
  8 KiB or 2 MiB by luck.
- **A dial holds no lock.** `net_connect` held the process's socket
  table across a blocking connect, so a full listen backlog stopped
  every socket call, the draining `net_accept` included. It dials
  outside the lock now. `time_sleep_ms`, `net_wait` and the dial tell
  the pool they wait, as every other wait does.
- `[conc.task.order]`'s cost paragraph is restated with these numbers.
  lupin and the checked machine answer the witness at N = 64 as the
  compiled tiers do.
### The shell's signals: every signal, what was ignored, a poll that never trails, a pid (s232, #679, #681, #691, #141, #423; ruling #70)

A POSIX shell written in wolf (pelt) could trap five signals; a listen
of any other bit answered ok and armed nothing, so `trap … USR1` let
SIGUSR1 kill the shell. It could not tell which signals it was started
ignoring, could poll 0 for a signal already delivered, and could not
name its own id or signal a process it had not spawned. Spellings are
ruled as implemented (#70, `[os.signal.shell.note]` R1–R9).

- **Every signal POSIX names has a bit (`[os.signal.set]`, #679,
  #423).** Eleven more a program may listen for, ignore, put back,
  wait for, poll and raise: `USER` (`SIGUSR1`, 512), `ALARM`,
  `BROKEN_PIPE`, `ABORT`, `CONTINUE`, `RESIZE` (`SIGWINCH`), `URGENT`,
  `VIRTUAL_ALARM`, `PROFILE_ALARM`, `CPU_LIMIT`, `FILE_LIMIT` (bits
  9..19). Eight a program may only send and ask the number of: `KILL`,
  `STOP` and the six fault signals (bits 20..27). A program that
  ignores `BROKEN_PIPE` sees a write to a closed pipe as its `io` row
  instead of dying, and a program that listens for it is told.
- **One shape rule.** `os_signal_listen`, `wait`, `poll`, `ignore` and
  `default` refuse an empty set, or one holding any bit that is not a
  listenable meaning, with `io` and change nothing (a listen dropped
  such a bit and answered ok; a wait or poll refused only a set with no
  listenable bit). `os_signal_raise` takes one listenable meaning.
- **`os_signal_ignored() -> int ! {unsupported}`
  (`[os.signal.ignored]`, #681)** answers the meanings ignored now —
  read at start, what the program was started ignoring (XCU `trap`,
  `nohup`). The checked machine answers `BROKEN_PIPE` from what the
  `wolf` binary was started with, recorded in a start-up constructor
  before Rust's runtime ignores it.
- **A poll never trails a delivery (`[os.signal.poll]`, #691).** A
  poll, and a wait's first look, let pending listened signals be
  delivered on the calling thread, wait out a trampoline running
  elsewhere, and read the self-pipe themselves under the drain thread's
  lock; `os_signal_raise` sends to the calling thread (`raise(3)`), so
  it is delivered when the call returns. The issue's witness — a poll
  straight after a raise — missed 2,000 of 2,000 on native and release;
  a child that signals its parent before the parent reaps it missed a
  few in 2,000 under `taskset -c 0-3`. Both are 0 in every cell now.
  pelt's `settle` barrier can go.
- **`os_pid() -> int` and `os_ppid() -> int ! {unsupported}`
  (`[os.proc.self]`, #141)** — `$$` and `$PPID` on every host.
  `os_proc_pid(h)` stays a child's id by handle.
- **`os_signal_send(pid, meaning) -> () ! {denied, invalid, io,
  not_found, unsupported}` (`[os.signal.send]`, #141)** — `kill(2)` for
  a pid above 0 and one meaning of the 28, or 0 to ask whether one
  could be sent. A group send stays with job control.
- **`os_signal_number(meaning) -> int ! {invalid, unsupported}`
  (`[os.signal.number]`)** — the host's number, for `kill -l`,
  `trap … 13` and `128 + N` (`USER` is 10 on linux, 30 on macOS).
- **A child keeps an ignored `BROKEN_PIPE`.** The runtimes start
  children through Rust's `Command`, which set `SIGPIPE` back to the
  default in every child; a native child of a program that ignored it
  died of it. The child now sets it ignored again before it runs, only
  when the program ignores it. The checked machine's spawns and exec
  follow the program's disposition, not `wolf`'s.
- **Capabilities (R8).** `os_signal_send` charges `exec`; `os_pid`,
  `os_ppid` and `os_signal_ignored` charge `env`; `os_signal_number`
  charges nothing.
- **lupin (R9).** lupin serves `os_pid`, `os_ppid` and
  `os_signal_number`, answers every shape row, and refuses
  `os_signal_send` and `os_signal_ignored` by name; it still never
  listens. lupin 0.1.52 (the 0.2.29 pairing) carries the mirror; the
  pre-mirror pin on 0.1.51 in `signal_shell_lanes` is dropped.
- **Counts.** The host table is 117 lines (from 112), `RT_SYMBOLS` 203
  (from 198), the prelude 124 functions and 156 W0304 names.

**Read this before you bump.** A program that passed a set with a bit
outside the meanings to `os_signal_wait` or `os_signal_poll` (masked
off before) now gets `io`. `512` is `USER` now: a program that used it
as a "bit outside the meanings" (s219's `job_rows` did) gets ok where
it got `io`. wolf-std's `std.signal`, if it validates against the nine
meanings, needs the twenty.

## 0.2.28 — 2026-10-11

THE TWENTY-EIGHTH, the soundness release. **0.2.27 shipped with two
known soundness holes (#660, #665); both are fixed here, and users of
0.2.27 should move.** Six compiler lanes land with it, one with a lupin
half, and the pairing moves to lupin 0.1.51, which mirrors it: the
checked machine runs tasks (s226, ruling #63); the calls that change a
file, eight new builtins (s230, ruling #67); a long-running server
stops growing in the runtime, and a `List` slice is one copy (s228,
ruling #65); a call that keeps or sends what it is handed is refused
when its bytes die first (s227, ruling #64; s229, ruling #66). The
bundled standard library moves to wolf-std `98e1b7b`. 120 commits since
0.2.27 (s228 25, s227 22, s229 20, s230 24, s226 29), every lane merged
before the release lane (r33) began, then r33's release commits.

### Soundness: a program could read freed region bytes on every compiler tier (#660, #665)

**Affected: 0.2.27 and earlier.** Two holes, both named as known in
0.2.27's notes, both through a call: the callee keeps or sends an
argument, and the call site charged that to nothing.

- **#660, a callee keeps a string built in an enclosing region block.**
  `region pass { let s = "built {n}"; in st { keep_it(mut l, s) } }`,
  with `l` living in `st`: the callee may keep `s` in `l` (a `copy`
  shares the bytes; a `push` or an index store lends them), and `l`
  outlives `pass`. It built on every compiler tier: native and release
  then printed whatever the next region wrote into the freed chunk, the
  checked machine printed stale bytes, and lupin traps `region-fault`.
  The same hole reached two shapes with no call at all: `in st { out =
  copy d }` (a `copy` of an aggregate shares its strings), and a
  region-built `str` stored as a key in an outer `Map` — which needed
  no `in` either, and then answered `m[k] else 0` with `0` for the key
  just stored.
- **#665, a callee sends its argument on a channel it was handed.**
  `fn fwd(ch: channel[str], s: str) { ch.send(s) else { } }` called with
  a string built in a region block: the direct `ch.send(s)` was always
  E1010, the send through the helper was not, and the receiver read the
  freed bytes on native and release and stale bytes on the checked
  machine. No `in` and no `mut` argument were needed. **#678's direct
  half** is the same hole without a channel: a `spawn proc` argument
  built in a region block, read by the proc after the block freed it.

**The fix is a compile-time refusal (E1010), not a run-time check.** No
accepted program's code changes and nothing costs time at run time; a
program that compiled on 0.2.27 may now be refused. Each rule reads the
call's signature, never the callee's body (rulings #64 R1 and #66 R1):
a `mut` argument (the receiver too) or a non-`Copy` result shares the
bytes of every other argument that can reach it by mode and type; a call
that hands its callee a channel able to carry bytes, and every `spawn
proc` argument, must outlive the frame, as a sent payload must. So the
refusals include some programs that ran correctly — a region-built
`str` handed under `in r { }` to a callee that only reads it beside a
`mut` argument that could have held it; any helper handed a
`channel[str]` inside a region block, whatever it sends; a proc joined
inside the block that built its argument.

**The fix-it.** E1010 names the callee, the argument and the region or
channel, and offers the edit that is sound at the call for a `str`:
`"{s}"`, an interpolation inside the `in` block, which builds the bytes
where the callee allocates. Otherwise: hand the callee a scalar
(`s.len`), make the call outside the region block, send from the proc
entry itself rather than a helper it calls, hand a function the channel
rather than a closure that captured it, and pass a spawned proc its
bytes as parameters of a function that joins it before returning.

**Measured downstream before each fix landed** (`conform-run
--phase=mem --json`, verdict and every diagnostic's code and span, the
compiler before against after, linux x86-64 and macOS arm64, over
wolf-std, boreutils, lobo, pelt, pax, muzzle and the book): s227, 1,276
records (1,156 files and lobo's 120 staged tests), **0 rows moved**;
s229, 1,287 records (1,165 files and lobo's 122 staged tests), **0 rows
moved**. Each time, lobo, boreutils' 28 utilities and pelt built
byte-identical binaries on native and release, on both hosts (60 of
60); in s227's census pax's 28 kernels built 56 of 56 identical objects
(linux).

**Known, not fixed here.** #678's other half (#678 stays open for
it): a function that spawns a proc with its own parameter and returns
without joining it is legal in the function and invisible at its call;
it needs a must-join rule (ruling #66 R4 left it to a lane of its own).
The checked machine has no dynamic `region-fault` for safe-tier values
(#667), so an escape the compiler missed would run there too. lupin 0.1.51 still runs a freed
region's bytes reached through a `List` element, a `Map` key or a
channel payload read inside an interpolation to `exit(0)`
(wolf-interp#232); the compiler refuses all of them.

### Read this before you bump the pin

**New refusals (E1010).** A program that compiled on 0.2.27 can be
refused by this release in four shapes, each a soundness fix (above and
in the lanes' sections below), each with the edits above:

- under `in r { f(mut outer, s) }`, an argument whose bytes die before
  `outer` does (s227, #660);
- `in st { out = copy d }` and `in st { xs[i] = d }` where `d` reaches a
  region-built `str`, and a region-built `str` stored as a key in a
  `Map` that outlives its block, with or without `in` (s227);
- a call that hands its callee a channel able to carry bytes, from a
  region block, under `in r { }` for a region made in the same frame,
  or in a `spawn proc` entry; and a `spawn proc` argument that can name
  region bytes, channel or no channel (s229, #665, #678's direct half);
- a `net_read` result returned out of the region it was read in (s228,
  #374's net half).

None of them moved a row in the downstream censuses taken as each
landed (above; s228's census: boreutils, lobo and its 28 serve tests,
pelt, wolf-std's 433 test files and the book's 357 samples and
exercises, the same verdicts and diagnostics).

**Eight new builtins, and what windows answers (s230, ruling #67).**
`fs_set_mode`, `fs_open_perm`, `fs_create_dir_mode`, `fs_symlink`,
`fs_link`, `fs_set_times`, `fs_truncate` and `os_umask`: `chmod`,
creating a file or directory with its permission bits, `ln -s`, `ln`,
`touch`, a length, and the process's creation mask. On windows
`fs_set_mode`, `fs_open_perm`, `fs_create_dir_mode`, `fs_symlink` and
`os_umask` answer `unsupported` by name (no permission bits, no mask, no
unprivileged symbolic link); `fs_link`, `fs_set_times` and
`fs_truncate` are served. The seven `fs_*` calls charge `fs` and
`os_umask` charges `env` (E1504 without the declaration). lupin 0.1.51
mirrors seven and refuses `os_umask`, and `fs_set_times` on a link
itself, by name (ruling #67 R10). **Not run per lane on windows** by the
lane (CI green only), nor on linux aarch64.

**The bundled std is wolf-std `98e1b7b`** (sc58: std at 0.2.27 / lupin
0.1.50, wolf-lang#693), where 0.2.27 shipped `87ba162`. It carries
what 0.2.27's bundled std lacked: `std.signal`, `std.term` and process
jobs (s219), `fs.stat`, `lstat`, `read_link` and `entries` (s218),
`env.unset` and `process.exec` (s225), and json's last-wins reading in
its header and tests (s224, ruling #61); the red row 0.2.27 shipped
(`tests/x/json/dom_typed_reads.lu`) is green. **It does not have this
release's wrappers**: `fs.set_mode`, `create_private`,
`create_with_mode`, `create_dir_mode`, `symlink`, `link`, `fs.Time`,
`set_times`, `set_link_times`, `truncate` and `env.umask` (wolf-std#62)
need this compiler and merge after it. Until a release bumps `STD-PIN`
again, call the builtins directly (they are in the prelude), or point
`WOLF_STD`, `--std-root` or a `wolf.pkg` `std` dependency at a wolf-std
that has them.

- **Measured under this release** (kasumi, linux x86-64, the
  release-head archive with lupin 0.1.51 beside `wolf`, no `WOLF_STD`):
  wolf-std `98e1b7b`'s 442 tests on lupin, the checked machine and
  native, and its 464 doc examples, compile and answer as under 0.2.27
  except **one test, `tests/mem/budget/breach_is_a_row.lu`, which the
  checked machine now runs** (s226) to the exit and bytes its directive
  names, where wolf-std's ledger still records the checked machine
  declining it; `cargo xtask std-test` reads that advancement as red
  until the ledger moves (wolf-std, after this release). Nothing in it
  answers a wrong value. The release machine equals native on 441 of
  the 442 with no std configured; the one (`list/sort_by_tier.lu`) is
  refused by name on release, as at 0.2.27.

**The checked machine runs tasks (s226, ruling #63).** `wolf
conform-run --checked` answers `scope`, `spawn`, `spawn proc`, `select`,
`when`, closures and `par` where it answered `unsupported` since s23;
`--seed=N` selects its one schedule and the record says `"seeded":
true`; a data race is `trap(race)`. A tool that read a checked record's
`unsupported` as "concurrency, skip it" now gets a verdict. `wolf run
--checked` is still the native build. No compiled binary changes for
this.

**Every hosted binary's bytes move** (`libwolf_rt.a`: s228's handle
tables, monitor and pool; s230's eight shims; `__wolf_rt_list_slice`).
No handle number, error row or exit reason moves. A `List` slice's
region charge is the slice's own bytes, where it counted the abandoned
doubling buffers: a slice that fit a checked `cap:` budget now fits
natively too.

**For contributors:** the linux CI job's limit is 45 minutes, from 30
(s226; the gauntlet had grown to 29 m 42 s).

**A correction to 0.2.27's notes (#695).** They said nothing hosted
downstream declares C. pax's `user/play` does (`extern "c" fn
play_kd`), and at 0.2.27 it is E1504 without `capabilities: [ffi]`;
pax added the line in `0cc43be`. Re-taken over every downstream
manifest at this release (wolf-std `98e1b7bc`, boreutils `abc59f5b`,
lobo `d6bd15b2`, pelt `84a02dc1`, pax `1af462d5`, muzzle `7c95300e`, the
book `2504a0fc`): the packages whose sources declare C are pax's
`kernel` and `tests/mkw.d/kernel` and muzzle's `src` (freestanding), and
pax's `user/play` (hosted). pax's three carry `ffi`; **muzzle's
`src/wolf.pkg` still does not**, so muzzle's build stays E1504 (0.2.27's
notes name the line) until it adds it.

**New prelude names** (`cargo xtask prelude-diff v0.2.27 HEAD
--markdown`, after `git fetch --tags`, run at the release head):

> **New prelude names.** HEAD adds `fs_set_mode`, `fs_open_perm`,
> `fs_create_dir_mode`, `fs_symlink`, `fs_link`, `fs_set_times`,
> `fs_truncate`, and `os_umask` (all eight: function,
> `[os.host.sigs]`). A module that declares a function, type or binding
> named any of the eight now draws W0304 (it shadows the ambient name),
> which `--deny-warnings` makes fatal: rename yours.

A grep for a declaration (`fn`, `let`, `var`, `const`, `type`,
`struct`, `enum`) of any of the eight in wolf-std `98e1b7bc`, boreutils
`abc59f5b`, lobo `d6bd15b2`, pelt `84a02dc1`, pax `1af462d5`, muzzle
`7c95300e` and the book `2504a0fc` found **none**. The only calls are
boreutils' bu21 probes for the then-missing builtins
(`notes/bu21/census/missing/`), which are notes and are not built. s226,
s227, s228 and s229 add no prelude name.

**Spellings ruled, each as implemented:** #63 (s226, R1–R8, A
throughout: the seed, `trap(race)`, a UB finding inside a proc ends the
run, the rendezvous back-edge, the deadlock through a closure, a task's
ambient region and module state, exit 121 when the root domain is
killed, `par` regions on checked first), #64 (s227, R1–R6), #65 (s228:
a finished proc keeps its exit reason for now, #671; the parked
worker's frames by option A as their own lane, #670), #66 (s229, R1–R6:
refuse now; a copying send stays the alternative), #67 (s230, R1–R10,
the umask the real process mask on the checked machine).

**What each downstream does to bump.**

- **wolf-std, first:** pins to 0.2.28 / 0.1.51 in `vendor/tools.toml`;
  rebase and merge #62 (s230's wrappers, red by design until this pin);
  move `tests/ledger.toml`'s checked column for
  `mem/budget/breach_is_a_row.lu` to `run` (s226). A later compiler
  release bumps `STD-PIN` to carry #62.
- **wolf-book, then the site:** the book pins 0.2.28 / 0.1.51 (the book
  and lupp.us are still at 0.2.26 / 0.1.49); ex16-7 and ex16-8, the two
  concurrency samples the checked machine declined, can run on it; no
  sample moved in the E1010 censuses. Then wolf-web pins the pair and
  the book, and deploys.
- **boreutils:** the toolchain digests (remove the staged
  `.wolf-bin/std` before unpacking its own std pin, as at 0.2.27).
  `chmod`, `touch`, `ln`, `mkdir -m` and `cp -p` can be written on the
  new builtins (bu21 left them out for want of each).
- **lobo:** the toolchain digests; `conn.sock_read` can go back to
  `net_read`, which now allocates in the region it is called in (s228).
- **pelt:** the toolchain digests; a `umask` built-in can be served by
  `os_umask`.
- **pax:** the pin; `user/play` already carries `ffi`. The eight new
  builtins are hosted: the freestanding target refuses each by name.
- **muzzle:** `capabilities: [ffi]` in `src/wolf.pkg`, owed since 0.2.27.
- **Homebrew and the AUR:** the std beside the binary moves to
  `98e1b7b` (this release's tap change).

### The pairing: lupin 0.1.51

`PAIRING` names **lupin 0.1.51 at pin `294d626`** (wolf-interp release
409991647; the linux x86-64 archive's sha256 `d8f62dd1…`). The spec pin
is unchanged (this compiler's v0.2.24 tag). 0.1.51 carries the lupin
half of s230; s226, s227, s228 and s229 have none.

- **Dropped 9 cases in two gates**, each green on the published 0.1.51
  unpinned: `fs_change_lanes` 8 (s230's mirror; its pre-mirror list
  stays, empty, as the gate's machinery) and `pairing` 1.
- **Carried to 0.1.51, measured** (each answers as 0.1.50 on the pinned
  rows; the issues are open): `checked_tasks_lanes`' two partings
  (wolf-interp#227, #230), `region_kept_arg_lanes`' seven `List` and
  `Map`-key rows and `chan_sent_arg_lanes`' channel-payload rows (both
  wolf-interp#232), attr_closed_set's control (lupin still declines its
  `comptime fn`); the racy counter's `trap(race)` check names 0.1.51 too;
  `int_truth_lanes`' `PIN_INT_NOT` names 0.1.48 alone and is inert.
- **Widened 0.** The plant (the 0.1.51 lists against the published
  0.1.50) is red on exactly those 9 cases.
- **The ritual** (#87, with #281's control against the 0.1.50 archive,
  the same corpus and the same release `wolf`, 1129 files):
  - checked: **moved 5 ledger counts**. Agreements 759 → 764,
    unsupported 98 → 93; SOUNDNESS 0 and hard 19 unchanged.
  - native: **moved 5**. Agreements 744 → 749, unsupported 99 → 94;
    SOUNDNESS 14 and hard 33 unchanged.
  - The same five files on both, s230's rows: `fs/change_rows.lu`,
    `fs/hard_link.lu`, `fs/mode_symlink.lu`, `fs/set_times.lu` and
    `fs/truncate.lu`, from unsupported to agreement. Nothing moved
    below the ledger.

### The checked machine runs tasks (s226, C1)

`wolf conform-run --checked` (the checked machine; `wolf run --checked`
is the native build) refused `scope`, `spawn`, `spawn proc`, `select`,
`when`, closures and `xs.par(f)` by name since s23 — "structured
concurrency in checked execution (C1 deferred)". It runs them now, so
the concurrency tier is checked by three machines and lupin instead of
two. No compiled binary changes: the runtime and both backends are
untouched.

- **Tasks, one at a time, under a deterministic scheduler
  (`[exec.checked.task]`, `[exec.checked.sched]`).** The scheduler is
  lupin's: a task runs until it blocks and is never preempted, a spawn
  leaves the spawner running, and with no seed tasks run in spawn
  order. `--seed=N` selects one schedule (the ready-task and
  `select`-arm choices; a seed with bit 62 set is a packed schedule),
  equal seeds are equal runs, and a seeded checked record now says
  `"seeded": true` (it said `false` whatever the flag). It explores
  nothing: one seed is one interleaving. Procs, `link`, `monitor`,
  `kill`, `cancel`, `join`, `timeout` arms on a virtual clock, `when`
  and `Mutex` run as `[conc.*]` says; a `par` worker allocates in a
  region of its own that is transferred to the `par`'s region at the
  join (ruling #30, the checked machine first).
- **A data race is `trap(race)` (`[exec.checked.race]`,
  `[conc.mm.race.3]`).** Vector clocks over `[conc.mm.hb]`'s edges,
  checked on every raw access, pool slot and module `var`:
  `corpus/conc/atomic_race_plain.lu` is `trap(race)` at the span lupin
  reports, where the compiled tiers print whatever four racing writers
  left (119370 and 400000 on one run). Two tasks that only wait on
  each other are `trap(deadlock)` with the roster on stderr.
- **What moved.** The checked lane executes 671 of the corpus's 1076
  entries at run, from 624, and all three lanes together 663, from 619
  (`cargo xtask lane-coverage`). The rows that were `unsupported` there
  — the `corpus/conc/` tier, `procs.lu`, `net/spawn_accept.lu`,
  `os/signal_supervisor.lu`, the closure rows — reach lupin's verdict
  and bytes; `freeze region { … }` and `1.s`-style durations run there
  too. Still declined there by name: a nested `fn`
  (`typecheck/closure_return.lu`).
- **Two measured partings from lupin 0.1.49, 0.1.50 and 0.1.51**,
  pinned by version in `checked_tasks_lanes`: lupin adds no rendezvous back-edge
  (`[conc.mm.hb.chan]`: "the receive also happens-before the send
  returns"), so a program ordered only by it is `trap(race)` on lupin
  and runs here (wolf-interp#227); and a `when` reached through a
  closure that captured a held mutex is `trap(deadlock)`
  (`[conc.deadlock.self]`) here and `trap(exclusivity)` on lupin
  (wolf-interp#230). Three more differences sit outside the corpus:
  a UB finding inside a proc ends the run here and is the exit reason
  `error` on lupin (wolf-interp#228); a task writes the program's one
  module state here and a private copy on lupin, and allocates in the
  root region here and its spawner's on lupin (wolf-interp#229).
- **For the book:** ex16-7 and ex16-8, the two concurrency samples the
  checked machine declined, can run on it at the pin that carries
  this.

### The calls that change a file: modes, links, times, a length, the umask (s230, #346, #698, #699, #700, #702, #704, #688)

**Read this before you bump the pin.** Eight new prelude names, all
`w0304: true` in `spec/prelude.json`: `fs_set_mode`, `fs_open_perm`,
`fs_create_dir_mode`, `fs_symlink`, `fs_link`, `fs_set_times`,
`fs_truncate` and `os_umask`. A module that declares one draws W0304,
which `--deny-warnings` makes fatal: rename yours. A grep for a
declaration of any of the eight found none in wolf-std (`98e1b7b`),
boreutils (`abc59f5`), pelt (`b32bebc`), lobo (`b256d2e`), pax
(`0cc43be`) or the book (`2504a0f`). Nothing that existed changed:
`fs_open_mode` and `fs_create_dir` keep their arity and still ask for
`0o666` and `0o777`. Every hosted binary moves bytes (`libwolf_rt.a`
gained the eight shims). **Each spelling and shape was a ruling owed
and is ruled as implemented** (ruling #67; `[os.fs.change.note]`, R1
to R10).

s218 let a program read what `ls -l` shows; these let it write the same
words — what `chmod`, `touch`, `ln`, `mkdir -m`, `cp -p` and a shell's
`umask` are made of (boreutils' bu21 left them out for want of each).
Every call judges its arguments first (`invalid`, the same on every
host), then says so by name where the host has no such operation
(`unsupported`), then makes the host's one call.

- **`fs_set_mode(path, mode) -> () ! {denied, invalid, io, not_found,
  unsupported}` (`[os.fs.mode]`).** `chmod`: the permission, set-id and
  sticky bits become exactly `mode` (`0..=0o7777`, `fs_stat`'s word 4),
  following a final link.
- **`fs_open_perm(path, open, mode) -> int` and
  `fs_create_dir_mode(path, mode) -> ()`, each `! {denied, exists,
  invalid, io, not_found, unsupported}` (`[os.fs.create.mode]`).**
  `fs_open_mode` and `fs_create_dir` with the permission bits the
  creation uses (`0..=0o777`, less the umask), so a private file is
  never readable for an instant (#346) and a copied program is
  executable from the first.
- **`fs_symlink(target, path) -> () ! {denied, exists, invalid, io,
  not_found, unsupported}` (`[os.fs.symlink]`).** The target stored
  verbatim, as `fs_read_link` reads it back; an empty target is
  `invalid`.
- **`fs_link(existing, path) -> () ! {cross_device, denied, exists,
  io, not_found}` (`[os.fs.link]`).** A second name. It never replaces
  (the atomic publish `fs_rename` is not) and never follows a final
  link in `existing`.
- **`fs_set_times(path, atime_s, atime_ns, mtime_s, mtime_ns, follow)
  -> () ! {denied, invalid, io, not_found}` (`[os.fs.times]`).** One
  `utimensat`. A nanosecond word is a count below a second, or `-1`
  for the host's clock now, or `-2` to leave that time alone; `follow`
  false sets a link's own times.
- **`fs_truncate(fd, len) -> () ! {invalid, io}`
  (`[os.fs.truncate]`).** The file behind a handle becomes exactly
  `len` bytes; the offset does not move.
- **`os_umask(mask) -> int ! {invalid, unsupported}`
  (`[os.fs.umask]`).** Sets the process's file mode creation mask and
  answers the one it replaced, POSIX's only form. The mask is the
  process's: every task's, and every child's.
- **windows**, by name: `fs_set_mode`, `fs_open_perm`,
  `fs_create_dir_mode`, `fs_symlink` and `os_umask` are `unsupported`
  (it has no permission bits, no mask, and no unprivileged symbolic
  link); `fs_link`, `fs_set_times` and `fs_truncate` are served.
- **Capabilities:** the seven `fs_*` calls charge `fs`; `os_umask`
  charges `env`, as `os_chdir` does. A package that reaches one must
  declare it (E1504).
- **Every machine.** native and release call the runtime; the checked
  machine makes the same host calls — and sets the process's real mask,
  where it keeps the working directory to itself (`[os.fs.umask]` says
  why); the freestanding target refuses each by name; lupin mirrors
  seven and refuses `os_umask`, and a link's own times, by name
  (wolf-interp s230). `os_error()` carries the host's number after each
  new `fs_*` refusal.
- **An octal or binary literal lowers (#688).** `0o755` and `0b101` ran
  on the checked machine and lupin and were refused by native and
  release ("this literal shape in WIR lowering", exit 4): the two
  literal readers in the lowering knew `0x` alone. A mode can be
  written the way everyone writes one. Witness:
  `corpus/typecheck/numlit_radix.lu`.
- Witnesses: `crates/wolf_driver/tests/fs_change_lanes.rs` — six corpus
  rows on every host and five unix fixtures (modes under a known mask,
  both links, a link's own times, `denied`, the mask), on four
  machines, each followed by the test's own reading of the host;
  `wolf_rt`'s crate tests, the umask's in a test binary of their own.
  Not done, each left to its issue: owner changes (#701), holes and
  reflinks (#703), fifos and device nodes (#705), a rename that does
  not replace (#706), calls relative to an open directory (#707).

### A long-running server stops growing in the runtime (s228, #661, #662, #374)

**Read this before you bump the pin.** Three places where the runtime
kept memory for the life of the process, and one compile-time rule that
had to come with the third. Every hosted binary moves bytes
(`libwolf_rt.a` changed); no handle number, error row or exit reason
moves; one program shape is newly refused (a `net_read` result leaving
the region it was read in, E1010), and no downstream source has it.

- **A closed handle's storage is reused (#661).** `fs_open` and the net
  calls kept one table slot per open for ever — 4 bytes a file, 56 a
  socket: five million open/close cycles took a process from 6 MB to
  22 MB, a million connections to 112 MB. The tables are now keyed by
  the live handle numbers. **The numbers themselves are unchanged**:
  they still only go up and are never reused (`[os.fs.std]`), which is
  also the use-after-close argument — a closed number is absent from
  the table and can never be handed out again, so a call on it answers
  `io` as before and cannot reach the handle whose storage it now
  shares. After: 0 bytes a cycle for both. The cost is the lookup: a
  slot found by the number's low bits and one comparison, 72 more user
  instructions per `fs_open` + `fs_close` than the vector's index (345
  to 417, linux x86-64) and 103 more per socket's life, with no change
  in wall time — both cycles are their syscalls.
- **A finished proc leaves its reason word, and nothing else (#662).**
  A spawned and monitored proc kept about 277 bytes: the monitor's
  channel, handed to compiled code as a raw pointer that nothing ever
  freed (260 bytes), and a 16-byte registry tombstone. The monitor is
  now a table handle whose channel is dropped once its delivery has
  been received — still an ordinary `channel[ExitReason]` to the
  program, an empty one afterwards — and the registry keeps one word
  per finished proc, one word per 512 when they ended alike. A million
  procs: 286 MB before; 0.1 MB after when they end alike (a server's
  per-request procs), 8 MB when each ends with a value of its own. That
  last word cannot go without a rule the language does not have: a
  `Proc[T]` is a copyable id, and `[conc.proc.join]` lets it be joined
  after the exit any number of times (#671).
- **Contained traps no longer stop the pool (#662).** A trap contained
  at a proc boundary parks its worker thread for good (D68). The parked
  thread also kept its pool slot, the pool has eight slots a core, and
  after that many `fault` exits nothing could run a task: the process
  stopped answering (a 2-cpu host: 16 traps). The parked worker now
  gives its slot and its queued work back. The thread itself remains,
  14.5 kB each, until D68's mechanism changes (#670).
- **`net_read` allocates in the region it is called in (#374, the net
  half).** It copied into the process root whatever region was open, so
  a server that read each request inside its own region gave none of
  it back. The mem tier names `net_read` a `[mem.region.escape]` site
  first (a read returned out of its region is E1010,
  `corpus/memory/region_net_read_return.lu`), then the runtime puts
  the bytes in the ambient region; the checked machine charges the same
  ledger. Census, trunk compiler against head: boreutils (28), lobo and
  its 28 serve tests, pelt, wolf-std (433 test files) and the book (357
  samples and exercises) build with the same verdicts and diagnostics.
  lobo's `conn.sock_read` (which read bytes and decoded them to route
  around this) can go back to `net_read`: with it removed, lobo's
  memory gate reads 4 kB over 4,000 requests at this branch and
  17,112 kB without the change (trunk `a027b643`). `fs_read_text`, `fs_read`, `read_line`, the
  directory, environment and json producers still use the root (#374
  stays open). lupin charges the read to its region already and
  does not yet refuse the escape (wolf-interp#234).
- **The gate.** `soak_residue` (wolf_driver) builds one program on the
  release tier and runs a million opens, a million connections and two
  million procs, reading the process's RSS from outside: 0 kB, 0 kB,
  96 kB and 7,124 kB of growth (8.1 bytes a proc on the leg where
  every proc returns its own value), against 3,516, 98,436, 251,256
  and 251,540 kB at trunk. `handle_residue` (wolf_rt) counts host bytes
  outstanding around the same cycles at the shims.

### A `List` slice is one copy (s228, #663)

`xs[a..b]` on a `List` was lowered as a loop of runtime `push` calls,
one per element, into a list that grew by doubling: 12.2 µs for a
4,096-byte `List[byte]` slice on the release tier (linux x86-64), where
a UTF-8 decode of twice those bytes takes 0.19 µs. It is now one
runtime call, `__wolf_rt_list_slice`: the bounds check the caller
already made, one allocation at exact capacity, one copy — 0.06 µs for
the same slice, for every element type (a slice is only lowered for
flat elements, and a flat element's copy is its bytes). The value is
the list the loop built; what a program can observe is the region
charge, which stops counting the abandoned doubling buffers: 4,144
bytes for that slice where it was 8,240, so a slice that fits a
`cap:` budget on the checked machine (4,096) no longer breaches it
natively. The freestanding runtime archive defines the symbol too, so
a kernel that slices a `List` links as before.

### A call keeps what it is handed: `in r { f(mut outer, s) }` is refused when `s` dies first (s227, #660, ruling #64)

**Read this before you bump the pin: a soundness fix that refuses
programs 0.2.26 built.** Under `in r { … }` a call aims what the callee
allocates at `r`, but an argument's bytes stay in the region that built
them, and a callee may keep an argument in a `mut` one: `l.last = copy
s` shares a `str`'s bytes, a `push` or an index store lends them. So

```wolf
region pass {
    let s = "built in pass, {n} uses"
    in st { keep_it(mut l, s) }     // `l` lives in `st`
}
```

built on every tier and left `l` naming freed bytes: native and release
printed the chunk the next region reused, the checked machine stale
bytes, and lupin traps `region-fault`. It is E1010 now, at `s`, naming
the callee, both arguments and the region, with the edit that is sound
at the call for a `str` (`"{s}"`: an interpolation inside the `in` block
materializes the bytes where the callee allocates). The rule reads the
call's signature, never the callee's body: every `mut` argument (the
`mut` receiver included) shares the bytes of every other argument that
can reach it — a `take` or `mut` argument always; a `read` one when its
type can name bytes a read can share (a `str`, a fn value and its
captures, a type parameter, or an aggregate reaching one) and the `mut`
argument's type can too. A non-`Copy` result and a declared call's `str`
result share the `read` arguments' bytes the same way; a builtin
container method keeps only what it stores (`push`'s value) and answers
from its receiver. Two doors that need no call close with it: `in st {
out = copy d }` and `in st { xs[i] = d }` — a `copy` of a struct, list
or map shares every `str` the original reaches (`[mem.tier0.move.3]`).
And one that needs neither a call nor `in`: **a `Map` store keeps its
key.** `region pass { let s = "built {n}"; m[s] = 1 }`, with `m` declared
outside the block, was accepted, and native and release then answered
`m[k] else 0` with `0` for the key just stored (its bytes freed and
reused): a silent wrong answer. It is E1010 at the store.

What is **not** refused: a string built inside the `in` block, a scalar
(`s.len`), a literal, a string built where the `mut` argument lives, a
`List[byte]` (the callee can only copy it), a `mut` argument that can
name no bytes (an `int`, a struct of scalars, a `List[int]`), a builtin
that keeps nothing of its argument (`(mut m).remove(key)`), and
anything without `in` that built before — there the call already pinned
its `mut` arguments to the current region. Shared bytes are never
"placed": a list built in a region block that holds strings from a
longer-lived region still goes into a struct built in the block.

What is refused that would have run correctly: under `in r { }`, a
region-built `str` handed to a callee that only reads it, beside a `mut`
argument that could have held it. The repair is the `"{s}"` edit or a
scalar argument. **Measured** against trunk `a027b643` at the memory
check: wolf-std, boreutils, lobo `c9eff15`, pelt, pax, muzzle and the
book — 1,156 files and lobo's 120 staged tests — move zero rows, and
lobo, boreutils' 28 utilities and pelt build byte-identical binaries on
both tiers. No run-time cost on any tier: a refusal.

A callee that sends its argument on a channel it was handed (#665, the
same read of freed bytes, no `in` needed) is s229's, below. lupin 0.1.51
(as 0.1.50 and 0.1.49) runs the `List` and `Map`-key forms of this to
`exit(0)` on stale bytes (wolf-interp#232) and traps a sound `copy` of a
scalar struct under `in` (wolf-interp#233). Witnesses: `corpus/memory/region_kept_*.lu`,
`region_copy_shares_str.lu`, `region_copy_index_store.lu`; gate
`region_kept_arg_lanes` (checked, native, release, lupin).

### A call sends what it is handed: `fwd(ch, s)` is refused where `ch.send(s)` was (s229, #665, #678, ruling #66)

**Read this before you bump the pin: a soundness fix that refuses
programs 0.2.27 built.** A sent payload must outlive the frame that
sends it (`[conc.chan.payload]`), and the compiler asked that of the
builtin `send` alone. One line of helper around the send and nothing
was asked:

```wolf
fn fwd(ch: channel[str], s: str) { ch.send(s) else { } }

region pass {
    let s = "built in pass, {n} uses"
    fwd(ch, s)                      // E1010 now; `ch.send(s)` always was
}
```

built on every tier, and the receiver read `s` after `pass` was freed:
native and release printed the chunk the next region reused, the
checked machine stale bytes. It is E1010 now, at `s`, naming the
callee, the argument and the channel. The rule reads the call's
signature, never the callee's body. At a call that hands its callee a
channel able to carry bytes — an argument or receiver whose type
reaches a `channel[str]` or a channel of structs, tuples or enums
reaching a `str`, through any field or container; a type parameter or
a `dyn` value counts — two things must outlive the frame, as a sent
payload must:

- **every argument that can name region bytes** (a `str`, or a list,
  map, struct, tuple or enum reaching one, by any mode);
- **the call's own site.** A callee allocates in its caller's current
  region and may send what it builds there, so `region pass { emit(ch,
  n) }` — `emit` sending `"built {n}"` — read freed bytes with no
  argument carrying them. A call that hands such a channel from inside
  a region block, under `in r { }` for a region made in the same
  frame, or in a `spawn proc` entry is refused at the call.

**A `spawn proc` argument is demanded the same way, channel or no
channel (#678).** The proc outlives the frame that spawned it, and a
`str` argument crosses as its view, so `region pass { let s = "built
{n}"; spawn proc child(s) }` left `child` reading freed bytes. It is
E1010 at `s` now. A function handed the bytes as parameters that spawns
and joins before it returns is accepted, and is the repair. **A closure that
captured such a channel** sends what it is called with, wherever it is
called: calling it is a call under both demands, and handing it to a
function, returning it, sending it or storing it in module state is
E1010 there (a `scope`'s `spawn` is not a handing-over).

**What to change.** Make the call where the bytes outlive the frame:
outside the region block, or under `in r { }` for a region the
function was handed — there the diagnostic offers `"{s}"`, which builds
the string in `r`. Send from a proc entry itself, not from a helper it
calls. Hand a function the channel, never a closure that captured it.

**What it costs.** Nothing at run time; no accepted program's code
changes. It refuses some programs that would have run correctly, and
says so: a helper that only reads the argument beside a channel it
sends something else on; any helper handed a `channel[str]` inside a
region block or a proc entry, whatever it sends; a channel made, used
and drained inside one region block through a helper; a proc joined
inside the region block that built its argument. A `channel[int]`
and every channel of scalars is untouched, in any region. Measured
before it landed, the compiler before against after at the memory
check: wolf-std, boreutils, lobo, pelt, pax, muzzle and the book
(1,165 files and lobo's 122 staged tests) — zero rows moved; lobo has
no channel that carries bytes, and its two spawns with request-lived
strings pass them as parameters and join.

Still open: a function that spawns a proc with its own parameter and
returns without joining it — legal in the function, invisible at its
call, and it needs a must-join rule this change does not have (#678's
helper half; ruling #66 R4 leaves it to a lane of its own). lupin 0.1.51,
as 0.1.50, runs the shapes whose receive is read inside an
interpolation (wolf-interp#232) and traps the rest. A run-time
alternative — a send that copies the bytes out, about 40 ns a message —
was weighed; ruling #66 refuses now (R2).
Gate `chan_sent_arg_lanes`: 42 rows on checked, native, release and
lupin.

## 0.2.27 — 2026-10-11

THE TWENTY-SEVENTH, and the first release that ships its standard
library: every archive carries wolf-std as `std/` beside the `wolf`
binary, and that copy is the default std root (s204, ruling #59). Nine
compiler lanes land with it, six of them with a lupin half, and the
pairing moves to lupin 0.1.50, which mirrors all six. A program can
survive Ctrl-C, run a job in its own process group and own the terminal
(s219, ruling #57); become another program and remove a variable (s225,
ruling #62); read the full stat record, a link and a typed listing
(s218, ruling #60). **Three P0s are fixed (#551, #538, #553):** unsigned
and wrapping integers print and compute as the values their types hold
(s220). A repeated json name is last-wins (s224, ruling #61); a C
declaration needs the `ffi` capability (s221, ruling #55); a kernel's
threads keep their own ambient region (s223, ruling #58); the root arena
takes no lock per allocation (s222). 184 commits since 0.2.26 (s220 18,
s219 29, s221 20, s222 15, s225 32, s223 13, s224 23, s218 16, s204 18),
every lane merged before the release lane (r32) began, then r32's
release commits.

### Read this before you bump the pin

**The archive carries std, and it is the default (s204, ruling #59).**
`std/` sits beside `wolf` in all four archives with `std/STD-REV` naming
its commit. With nothing configured, `use std.env` resolves there, where
every machine answered E0301 through 0.2.26. Every configured source
still wins, in this order: `--std-root`, `WOLF_STD`, a `std` path
dependency in `wolf.pkg`, then the copy beside the binary. A `wolf`
built from source with plain `cargo build` has nothing beside it and
keeps the prelude stub.

- **The bundled std is wolf-std `87ba162`** (sc57: std as it stood for
  wolf 0.2.26 / lupin 0.1.49). It does **not** have the wrappers for
  this release's new builtins, because those wolf-std changes need this
  compiler and merge after it: `std.signal`, `std.term` and process
  jobs (wolf-std#56, s219), `std.fs`'s `stat`, `lstat`, `read_link` and
  `entries` (wolf-std#57, s218), `std.env.unset` and `std.process.exec`
  (wolf-std#60, s225). Until a release bumps `STD-PIN`, call the
  builtins directly (they are in the prelude), or point `WOLF_STD`,
  `--std-root` or a `wolf.pkg` `std` dependency at a wolf-std that has
  them; any of the three overrides the bundled copy.
- **Measured under this release** (kasumi, linux x86-64, the
  release-head archive with lupin 0.1.50 beside `wolf`, no `WOLF_STD`):
  wolf-std `87ba162`'s
  433 tests on lupin, the checked machine and native, and its 457 doc
  examples, answer as under 0.2.26 except **one test,
  `tests/x/json/dom_typed_reads.lu`, which exits 33**: its row 33 pins
  the first-wins json reading this release replaces (below). The
  release machine equals native on 432 of the 433 with no std
  configured; the one (`list/sort_by_tier.lu`) is refused by name on
  release as it was at 0.2.26. So the bundled std compiles and runs,
  and one sentence in it is stale: the header of `std/x/json/json.lu`
  still says a repeated key answers its first occurrence. The code
  calls the builtins and is last-wins. wolf-std#59 corrects the header
  and the row.
- **If you stage your own std beside a staged `wolf`** (boreutils',
  pelt's and lobo's `.wolf-bin/std`): the archive now puts a `std/`
  there first. A fetch that copies the whole archive into `.wolf-bin/`
  and then unpacks its own std pin over it is left with a mix whenever
  the two commits differ in a file one of them lacks. Remove
  `.wolf-bin/std` before staging yours, or stage none and read the
  archive's `std/STD-REV` as the pin. A `WOLF_STD` you export still
  wins either way.
- **A freestanding build reads no default std.** `--target
  x86_64-unknown-none` (or a manifest `target`) sees only a root you
  name: the shipped std is the hosted library (with it beside the
  binary, 27 of pax's 29 kernels stopped on a `str` comparison its home
  modules lower).
- **lupin's archive ships no std** (ruling #59 B(1)). A `lupin` unpacked
  beside this `wolf` reads the same `std/`; a `lupin` installed alone
  (Homebrew's, the AUR's) still answers `unsupported` for `use std.…`
  with nothing set. `LUPIN_STD` or `--std-root` serves it.
- **Packagers** install `std/` beside the real binary: Homebrew's
  `libexec/std`, the AUR's `/usr/lib/wolf-lang/std`. A source build
  that must not fetch sets `WOLF_DIST_STD_SRC` to a wolf-std checkout
  at `STD-PIN`'s commit; `cargo xtask dist` refuses any other commit.

**Freestanding downstreams (pax, muzzle): one line each, before the
build (s221, ruling #55).** A package that declares C — a bodyless
`extern "c" fn` or an `extern "c" let` — must list `ffi` in its
manifest's `capabilities`, called or not. Without it every one of pax's
29 kernels and muzzle's `src/muzzle.lu` is E1504 on native and release.
The files: `pax/kernel/wolf.pkg`, `pax/tests/mkw.d/kernel/wolf.pkg`,
`muzzle/src/wolf.pkg`. With the line their objects are the same as
0.2.26 built them. Nothing hosted downstream declares C this way.

**New prelude names** (`cargo xtask prelude-diff v0.2.26 HEAD
--markdown`, after `git fetch --tags`, run at the release head):

> **New prelude names.** HEAD adds `fs_stat`, `fs_lstat`,
> `fs_read_link`, `fs_read_dir_entries`, `env_unset`, `os_exec`,
> `os_signal_ignore`, `os_signal_default`, `os_signal_poll`,
> `os_spawn_job`, `os_proc_pid`, `os_wait_status`, `os_pgid`,
> `os_term_foreground`, `os_term_set_foreground`, `os_term_mode`, and
> `os_term_set_mode` (all seventeen: function, `[os.host.sigs]`). A
> module that declares a function, type or binding named any of the
> seventeen now draws W0304 (it shadows the ambient name), which
> `--deny-warnings` makes fatal: rename yours.

A grep for a declaration (`fn`, `let`, `var`, `const`, `type`,
`struct`, `enum`) of any of the seventeen in wolf-std `87ba1620`,
boreutils `5da892e8`, lobo `d1bf1356`, pelt `3e7516c9`, pax `bc130e46`,
muzzle `7c95300e` and the book `2504a0fc` found **none**. No downstream
calls any of them yet; the one mention is a comment in wolf-std's
`std/env/env.lu` ("There is no `env_unset` builtin"), which wolf-std#60
retires.

**Output that moves.**

- *Unsigned 64-bit values print as themselves (s220).* A `u64`, `uint`
  or `wrapping[u64]` above `i64::MAX` printed negative in a spec-less
  hole on native and release (`u64::MAX` was `-1`); it prints
  `18446744073709551615`. `-x` on a plain unsigned type traps
  `overflow` for any `x` but 0, where native and release printed a
  wrapped value. Wrapping `/` and `%` compile. A program that worked
  around the printing (boreutils' `bore.u64` halves) still runs.
- *json (s224, ruling #61).* `json_get`, `json_type`, `json_len` and
  paths read a repeated object name last-wins, one member per name:
  `json_get("{\"a\": 1, \"a\": 2}", "a")` was `1` and is `2`, and
  `json_len` of that object was `2` and is `1`.
- *A checked child sees `env_set` (s225).* A child the checked machine
  spawns is handed the environment with the program's `env_set` and
  `env_unset` applied, as native and release always did.
- *Every hosted binary's bytes move, and nothing else it prints does.*
  `libwolf_rt.a` changed (the root arena, the json kernel, the exec
  module). The lanes' censuses (984 builds a side) found the compiler's
  own output byte-identical when both sides link one runtime.
  `libwolf_rt_none.a` gains two symbols (`wolf_rt_ambient_get`,
  `wolf_rt_ambient_set`); a freestanding object that calls neither is
  unchanged.

**A module or alias named `c` now links (s224, #620)**, and a program
that needs both an imported C `f` and a module-`c` function `f` is
refused by name.

**Spellings ruled, each as implemented:** #55 (`ffi` at a C
declaration), #57 (s219's eleven names, R1–R7; R8 = A, lupin refuses the
terminal half by name), #58 (`wolf_rt_ambient_get`/`_set`, option A),
#59 (the archive's `std/`, A–D), #60 (s218's four names; a non-UTF-8
name is listable, not yet stat'able by name), #61 (json: Map
semantics), #62 (`env_unset`, the execve-shaped `os_exec`, R1–R5).

**Known issues: two soundness holes this release shares with 0.2.26.**
Neither is new in 0.2.27 and neither is fixed in it. Both let a
compiled program read freed bytes with no diagnostic, and both go
through a call: the callee stores or sends an argument, and the call
site does not charge that to the other arguments.

- **wolf-lang#660: a callee keeps a string built in an enclosing region
  block.** Inside a region block, under `in r { f(mut outer, s) }` with
  `r` a longer-lived region, the callee may keep `s` (through
  `copy s`, which shares the bytes) in the `mut` argument, which
  outlives the block. No E1010 on any compiler machine. Native and
  release then read the freed bytes (the witness prints another
  region's reuse of them); the checked machine prints the stale bytes
  and reports nothing; lupin traps `region-fault`. Until the fix, hand
  such a call scalars, or a string built inside the `in` block
  (`"{s}"`), as lobo does. The fix is written (PR #666, s227), held for
  a ruling, and ships in the next release.
- **wolf-lang#665: a callee sends its argument on a channel it was
  handed.** `fn fwd(ch: channel[str], s: str) { ch.send(s) else { } }`
  called with a string built in a region block: the direct send is
  E1010, the send through the helper is not refused, and the receiver
  reads freed bytes on native and release and stale bytes on the
  checked machine. No `in` and no `mut` argument are needed. lupin does
  not trap here either. Not yet fixed; send directly, where the
  compiler checks the payload, until it is.

**What each downstream does to bump.**

- **wolf-std:** pins to 0.2.27 / 0.1.50 in `vendor/tools.toml`; rebase
  and merge #56, #57, #59, #60 (each red by design until this pin);
  `tests/x/json/dom_typed_reads.lu` goes green with #59. A later
  compiler release bumps `STD-PIN` to carry them.
- **boreutils, pelt, lobo:** the toolchain digests; remove the staged
  `.wolf-bin/std` before unpacking their own std pin (above); no
  manifest change. boreutils can now write `ls -l`, `stat`, `readlink`
  and `env -u` on the new builtins and retire `bore.u64`'s halves; pelt
  can take `os_exec`, `env_unset`, the signal poll and job spawn.
- **pax, muzzle:** `capabilities: [ffi]` in the three manifests named
  above, in the same change as the pin. pax may save and restore the
  ambient slot in its switch (`wolf_rt_ambient_get`/`_set`) and then
  lift its rule against a `region` held across a yield. No std reaches
  a kernel unless it names one.
- **wolf-book:** builds wolf from source, so no std is beside its
  binary and no transcript moves for that reason; a sample that prints
  a 64-bit unsigned value above `i64::MAX`, or reads a repeated json
  name, prints the new bytes.
- **Homebrew and the AUR:** install `std/` beside the binary (done in
  this release's tap change).

### The pairing: lupin 0.1.50

`PAIRING` names **lupin 0.1.50 at pin `294d626`** (wolf-interp release
409303515; the linux x86-64 archive's sha256 `e2867a35…`). The spec pin
is unchanged (this compiler's v0.2.24 tag). 0.1.50 carries the lupin
halves of s220, s219, s225, s224, s218 and s204.

- **Dropped 13 cases in five gates**, each green on the published
  0.1.50 unpinned: `exec_lanes` 5 (s225), `fs_stat_lanes` 4 (s218),
  `int_truth_lanes` 2 (s220's wide wrapping product, in the rows and in
  the sweep), `ctrl_c_lanes` 1 (s219) and `json_dup_lanes` 1 (s224, its
  seven rows); and `pairing` 1. The emptied lists stay, empty, as the
  gates' machinery; `json_dup_lanes` loses its 0.1.49 table outright.
- **Kept.** attr_closed_set's control, carried to 0.1.50 measured (lupin
  still declines its `comptime fn`, #174); the racy counter's
  `trap(race)` check names 0.1.50 too; `int_truth_lanes`' `PIN_INT_NOT`,
  which names 0.1.48 alone and is inert against this pairing.
- **Widened 0.** The plant (the 0.1.50 lists against the published
  0.1.49) is red on exactly those 14 cases.
- **The ritual** (#87, with #281's control against the 0.1.49 archive,
  the same corpus and the same release `wolf`, 1089 files):
  - checked: **moved 4 ledger counts**. Agreements 683 → 687,
    unsupported 138 → 135, SOUNDNESS 1 → 0, hard 17 → 16.
  - native: **moved 4**. Agreements 710 → 714, unsupported 94 → 91,
    SOUNDNESS 15 → 14, hard 34 → 33.
  - The same four files on both: `os/env_unset.lu`, `os/exec_rows.lu`
    and `os/job_rows.lu` go from unsupported to agreement, and
    `typecheck/int_truth_wrap_mul_wide.lu` from SOUNDNESS to agreement
    (0.1.49 trapped where the compiler machines answered). Seven files
    moved below the ledger on each, the seven `json/dup_*.lu` rows,
    whose lupin stdout is now the last-wins answer.
- **lupin and the shipped std.** A lupin beside this `wolf` reads the
  archive's `std/` as its last std root; lupin's own archive ships none.

### The standard library ships with the compiler (s204, ruling #29, #415)

**Read this before you bump the pin.** The release archive now carries
wolf-std as `std/` beside the `wolf` binary, at the commit
`crates/wolf_driver/STD-PIN` names (`87ba162…`, std at wolf 0.2.26 /
lupin 0.1.49), with `std/STD-REV` recording it and wolf-std's `LICENSE`
and `LICENSE-EXCEPTION` beside the tree. That directory is the default
std root: after `brew install wolf` or an unpacked archive, `use std.env`
and `xs.map(f)` work with no `WOLF_STD` and no `--std-root`, where every
machine answered E0301 before. Every configured source still wins, in
this order: `--std-root`, `WOLF_STD`, a `std` path dependency in
`wolf.pkg`, then the copy beside the binary. A `wolf` built from source
(`target/<profile>/wolf`) has nothing beside it and keeps the prelude
stub, so the test suite and the corpus answer as before. A freestanding
build (`--target x86_64-unknown-none`, or a manifest `target`) reads no
default: the shipped std is the hosted library, and with it beside the
binary 27 of pax's 29 kernels stopped on a `str` comparison its home
modules lower; only an explicit root reaches a kernel. `cargo xtask
dist` fetches the pinned commit (or takes a checkout named by
`WOLF_DIST_STD_SRC`, refused unless it is at the pin), and its smoke runs
a `use std.env` program from the unpacked archive with `WOLF_STD` unset.
Packagers: install `std/` beside the binary with the rest (Homebrew's
`libexec`, the AUR's `/usr/lib/wolf-lang`). The E0301 stub note now says
where the shipped std lives.

### A repeated json name is last-wins; a module named `c` links (s224, #124, #620)

**Read this before you bump the pin.** Two fixes, one runtime rebuild.
Every hosted binary moves bytes, because `libwolf_rt.a` changed (the
json kernel); none changes behaviour except a program that reads a
json object with a repeated name.

- **A repeated json object name is last-wins (#124, ruling B22,
  `[os.json.dup]`).** `json_get`, `json_type`, `json_len` and every path
  segment now read an object as one member per name, at the position
  where the name first appears, holding its LAST value, as
  `std.json.parse`, JavaScript, Python and serde do.
  `json_get("{\"a\": 1, \"a\": 2}", "a")` was `1` and is `2`; `json_len`
  of that object was `2` and is `1`; a nested object `json_get`
  re-renders carries the name once (`{"a":1,"b":2,"a":3}` renders
  `{"a":3,"b":2}`). The root's own text is unchanged. All four machines
  answered first-wins before (the checked machine's `wolf_mem::json`,
  native and release's `wolf_rt::json`, and lupin); lupin's half is
  wolf-interp s224, in lupin 0.1.50; `json_dup_lanes` pinned lupin
  0.1.49 to its measured first-wins answers until then (dropped at
  this pairing).
  wolf-std's `tests/x/json/dom_typed_reads.lu` pinned first-wins (rows
  33 and 34); its s224 PR flips them and waits for this release.
- **A module named `c` links on every tier (#620).** WIR spells a
  function `go` of a module whose path is `c` (a child directory `c/`,
  or a dependency under the alias `c`) as `c.go`, which is also how the
  C membrane spells an imported C function. Both backends read such a
  callee outside the current object as C, so the native tier linked it
  as the plain symbol `go` and failed (`undefined reference to 'go'`,
  exit 2), and release did the same whenever its cluster partition
  separated caller and callee. A function the program defines is now
  never a C import. No symbol moved: wolf functions were already
  mangled by their module path, and two packages that each define `go`
  were never the problem (t05's witness happened to name one of them
  `c`). The one program that needs both readings — an imported C `f`
  beside a module-`c` function `f` — is refused by name instead of the
  ICE it was.
- **A link error fails a gate, never skips it (#620).**
  `lane_exit::environment_refusal` now fails an exit 2 whose stderr
  names an undefined or duplicate symbol, as it already failed an ICE;
  pkg.rs's diamond had been reading `undefined reference to 'go'` as an
  environment skip. A missing `cc` or runtime library still skips.

**Downstream** (boreutils `5da892e`, lobo `ebace85`, pelt `3e7516c`,
wolf-std `87ba162`, pax `ef4c3ca`; 984 builds a side, native and
release): with one runtime on both sides all 1968 are byte-identical
(stripped), so the compiler change moves nothing. With each side's own
runtime, 834 hosted binaries move (every one that links
`libwolf_rt.a`: the runtime's code layout shifted) and the 58 pax
objects do not. Run, the 399 native and 399 release wolf-std test
binaries answer as before except `x/json/dom_typed_reads`, which now
exits 33: its row 33 is the first-wins pin above.

### A C declaration carries the `ffi` capability, as `import c` does (s221, #619, ruling #55)

**Read this before you bump the pin if your package declares C.** A
bodyless `extern "c" fn` (a C function wolf calls, `[abi.c.import]`)
and an `extern "c" let` (a link-time symbol, `[abi.link.extern]`) now
charge `ffi` at the declaration, called or not, exactly as `import c`
is charged at its line (`[pkg.caps]` (1a)). Until now a `caps=[]`
package could call `getpid` through `extern "c" fn` and run, and `wolf
audit --ci` said `effective: []`. A package that declares C without
`capabilities: [ffi]` is now E1504 on native and release, naming the
declaration and its first call (or that nothing calls it), and `wolf
audit --ci` exits 1 with the reason ("declares `extern "c" fn getpid`
at app/main.lu:4:15, first called at app/main.lu:7:22"). An `extern "c"
fn` with a body and an `export fn` are wolf code that C calls and charge
nothing; an inline `asm` block still compiles nowhere, so it charges
nothing yet (kw13's). The checked machine and lupin never reached C
through these declarations (both refuse the call by name) and do not
move.

- **Downstream (census at boreutils `5da892e`, lobo `ebace85`, wolf-std
  `87ba162`, pelt `3e7516c`, pax `ef4c3ca`, muzzle `7c95300`, wolf-book
  `2504a0f`; trunk vs this change, native and release).** Every
  boreutils utility (28), lobo, pelt, wolf-std's 433 test files and the
  book's 6 sample packages build byte-identical with identical
  diagnostics, and every manifest's `wolf audit` prints the same bytes.
  **pax and muzzle declare C and need one line each:** all 29 pax
  kernels (27 under `kernel/`, 2 under `tests/mkw.d/kernel/`) and
  muzzle's `src/muzzle.lu` are E1504 until `capabilities: [ffi]` is in
  `pax/kernel/wolf.pkg`, `pax/tests/mkw.d/kernel/wolf.pkg` and
  `muzzle/src/wolf.pkg`; with it their objects are the same as trunk's
  (release byte for byte; native with debug info stripped, since the
  declared copy builds from another directory).
- **In this repository** three gates' packages call their listed
  assembly through bodyless externs, so they declare `ffi`: kw05's
  `asm_link.rs` manifests, kw10's
  `tests/fixtures/freestanding_interrupt/wolf.pkg`, and s214's #600
  witness in `shr_range_lanes.rs`.
- `wolf audit` unites a directory's standalone entries (pax's
  `kmain_*.lu`) by the reason a reader sees, so a declaration several
  entries share is counted once.
- Witness: `crates/wolf_driver/tests/cap_ffi.rs` (nine rows: a
  `caps=[]` root and dependency, an `extern "c" let`, an unused
  declaration, a pax-shaped freestanding kernel per entry and in the
  audit, and the honest twins).

### A kernel's threads each keep their own ambient region (s223, #611)

On the freestanding target (`x86_64-unknown-none`), the runtime kept
one ambient-region slot for the whole program. A kernel that switched
threads while one of them was inside a `region` sent the next thread's
allocations into a region that thread never entered, and that
region's exit freed them under it. pax measured `region ra 0 bytes,
region rb 1008 bytes` on both tiers, BIOS and UEFI. The freestanding
runtime now exports `wolf_rt_ambient_get() -> *u8` and
`wolf_rt_ambient_set(p: *u8)` (`[abi.target.none.ambient]`,
`[abi.target.none.hooks]` (e)). A scheduler stores `get()` in the
outgoing thread's record and calls `set()` with the incoming thread's
saved value, or null for a thread that has not run yet. In a package
that lists assembly, the roster admits both as hooks. A call to either
links `libwolf_rt_none.a`, which still imports only `wolf_alloc`,
`wolf_free` and `wolf_trap`, so a kernel that calls the pair supplies
the allocator hooks. **What moves:** `libwolf_rt_none.a` gains the two
symbols. No hosted runtime byte changes, and a freestanding object that
never calls the pair is unchanged. pax can lift its rule against
holding a `region` across a yield once it saves the slot in its switch.
Witness: `crates/wolf_driver/tests/freestanding_threads.rs`. With the
pair, the List lands in its own thread's region on both tiers. Without
it (the control), the List lands in the other thread's region.

### Unsigned and wrapping integers print and compute as the values their types hold (s220, #551, #538, #553)

**Read this before you bump the pin.** Programs that print a `u64`,
`uint` or `wrapping[u64]` value above `i64::MAX` print different bytes
on native and release, and that is the fix: `u64::MAX` printed `-1` in
a spec-less hole (`"{x}"`, `print`, a `str` built from it) and now
prints `18446744073709551615`, as lupin always has and as `{x:x}`
already did. A program that worked around it (boreutils' `bore.u64`
halves, `nproc`'s spelled-out `umax_tenth`) still runs and can retire
the workaround. **No downstream build changed:** boreutils `50d8907`,
lobo `a728fab`, pelt `1c1d0f7`, wolf-std `0f74ec5` (its 423 test
files) and pax `f197e40` (29 kernels, freestanding objects) built with
trunk and with this change, 964 pairs a tier on native and release,
are byte-identical with identical diagnostics, and the checked machine
answers all 423 wolf-std test files as before (none of them prints a
64-bit unsigned value, divides or negates a wrapping value, or holds a
negative narrow one).

- **Printing (#551, #538, `[type.interp.value]`).** Native and release
  set the runtime's unsigned bit only beside a format spec; every
  64-bit unsigned hole now carries it. The checked machine printed
  `Value::Int` signed whatever the type and its format-spec path said
  `unsigned: false` (`{d:x}` of `u64::MAX` was `-1`).
- **The checked machine holds `u64`'s whole range (#551).** A `u64`/
  `uint` is held as its bit pattern, as the compiled tiers' registers
  hold it: the literal takes the whole range (it was `unsupported`),
  arithmetic is `u64`'s (it trapped `overflow` past `i64::MAX`), `<`
  `>` `<=` `>=` and `/` `%` read it unsigned, `!` answers (it was
  refused by name), and casts keep it (`wrapping[u64]` above
  `i64::MAX` into `u64` was refused by name since kw03). kw03's
  `narrow_cast_lanes` carve-out of 20 checked rows is gone.
- **Signed narrow wrapping values are negative on the checked machine
  (#553, #602).** It held every `wrapping[iN]` result as its unsigned
  mask, so `200 as wrapping[i8]` printed `200`, `5 - 11` at
  `wrapping[i16]` printed `65530`, and both ordered above zero; native,
  release and lupin print `-56` and `-6`.
- **Wrapping division (#538, `[type.wrap.div]`, new).** `/` and `%` on
  a `wrapping[T]` divide the values: native and release refused them
  ("no idiv.wrap op"), the checked machine divided a `wrapping[u64]`
  as signed (`u64::MAX / 10` was `0`). `MIN / -1` wraps to `MIN`,
  `MIN % -1` is `0`, a zero divisor traps `div-zero`. No new WIR
  opcode: unsigned is `udiv.chk`, signed branches around `idiv.chk`
  when the divisor may be -1.
- **Negation, found on the way.** `-x` at a wrapping type wraps (native
  and release trapped `-MIN`; `-(5 as wrapping[u8])` is 251), and at a
  plain unsigned type is `0 - x` there, trapping `overflow` for any `x`
  but 0: `-x` on a `u8` printed `251` on native and release and `-5`
  on the checked machine, where lupin traps.
- **lupin** (wolf-interp s220): a `wrapping[u64]` product past 2^127
  trapped `overflow` (its `i128` multiply); it now keeps the low bits.
  Found by the sweep; `int_truth_lanes` pinned 0.1.48 and 0.1.49 by
  version and commit until 0.1.50 carried it (dropped at this pairing).
- **Witnesses:** eighteen `corpus/{typecheck,faults}/int_truth_*.lu`
  rows and `crates/wolf_driver/tests/int_truth_lanes.rs`, which holds
  them on four machines and sweeps seeded random values of all
  eighteen integer types (printed, ordered, combined) against an
  `i128` reference.

### Ctrl-C: a program that survives SIGINT, process groups and the terminal (s219, #622)

**Read this before you bump the pin.** Eleven new prelude names, all
`w0304: true` in `spec/prelude.json`: `os_signal_ignore`,
`os_signal_default`, `os_signal_poll`, `os_spawn_job`, `os_proc_pid`,
`os_wait_status`, `os_pgid`, `os_term_foreground`,
`os_term_set_foreground`, `os_term_mode` and `os_term_set_mode`. A
module that declares one draws W0304 (fatal under `--deny-warnings`). No
existing builtin's row changed. **Every spelling below was a ruling
owed** (`[os.term.note]` R1–R8) and is ruled as implemented (ruling
#57, R8 = A).

- **Five new meanings** (`[os.signal.set]`): `INTERRUPT` 16 (`SIGINT`,
  Ctrl-C), `SUSPEND` 32 (`SIGTSTP`), `BG_READ` 64 (`SIGTTIN`),
  `BG_WRITE` 128 (`SIGTTOU`), `CHILD` 256 (`SIGCHLD`). `os_signal_listen`
  takes them on unix; on windows a listen for one is `io`.
- **A disposition: default, ignored or listened** (`[os.signal.disp]`).
  `os_signal_ignore(set)` and `os_signal_default(set)` beside
  `os_signal_listen(set)`; the last call wins; a child starts as `exec`
  leaves it, so an ignored meaning stays ignored there and a listened
  one is the default. No wolf code ever runs in a handler.
- **`os_signal_poll(set) -> int ! {io}`** (`[os.signal.poll]`) takes a
  delivered meaning without waiting, 0 when none has come: how a shell
  learns after its read returns that Ctrl-C was typed.
- **`os_spawn_job(exe, args, map, group, tty, defaults)`**
  (`[os.proc.job]`): `os_spawn_fds` plus, in the child before `exec`, a
  process group (-1 stay, 0 lead, > 0 join), the terminal's foreground
  handed to it (`tcsetpgrp`, `SIGTTOU` blocked for the call) and the
  meanings in `defaults` set back to default; the parent repeats the
  group and the handoff.
- **`os_proc_pid(h)` and `os_wait_status(h)`** (`[os.proc.status]`): a
  child's pid, and how it ended — the exit code, or `-N` for a death by
  signal N, so a shell can say `128 + N` (130 for Ctrl-C).
- **The terminal** (`[os.term]`, `[os.term.mode]`): `os_pgid()`,
  `os_term_foreground(fd)`, `os_term_set_foreground(fd, pgid)`, and the
  mode as one int — canonical 1, echo 2, signals 4, `VMIN` in bits
  8..15, `VTIME` in 16..23 — read by `os_term_mode(fd)` and set by
  `os_term_set_mode(fd, mode)`; every other termios field is left as
  found.
- **Every machine.** Native and release share `wolf_rt` (`signal.rs`,
  `os.rs`, the new `term.rs`). The checked machine now arms the host for
  real when it listens (a handler that only sets an atomic bit) and uses
  the host's `sigaction` for ignore and default, so a checked program
  survives Ctrl-C exactly as a native one does. lupin mirrors the group
  spawn, the pid, the status, the poll and every shape row, and refuses
  the dispositions and the terminal by name (no `unsafe`, no std call;
  R8). windows: `unsupported` by name for the dispositions, the job
  options and the terminal.
- **Witnesses** (`crates/wolf_driver/tests/ctrl_c_lanes.rs`): five rows
  through a pseudo-terminal — an ignored INTERRUPT survives Ctrl-C, a
  listened one is polled as 16, a child group handed the terminal dies
  of Ctrl-C (-2) while its parent lives and takes the terminal back, raw
  mode reads one key and restores — on checked, native and release (and
  under `taskset -c 0-3`), with each program's SigBlk/SigIgn printed;
  a control that dies of Ctrl-C; and `status.lu` with no terminal on
  every machine, lupin included. pelt (a throwaway build, not pushed)
  matches dash on its `ctrl_c_prompt` and `ctrl_c_child` sessions with
  four calls added: a listen, a poll after the read, `os_wait_status`,
  a poll after the wait.

### The root arena takes no lock per allocation; reads and gathered writes stop allocating (s222, #635, #421)

No program's output, verdict or diagnostics change: this is the native
runtime (`libwolf_rt.a`), so every native and release binary's bytes
move and nothing it prints does.

- **The root arena bumps a per-thread window (#635, #421).** Where an
  allocation lands with no `region` open — every list header, list
  growth and materialized `str` in such code — was one bump window
  behind one `Mutex`, taken on EVERY allocation. Each thread now bumps
  its own window over the same never-freed chunk list and takes the
  lock only to refill one (once per 64 KiB, or for one allocation of
  16 KiB or more); a thread's unused tail is parked when it exits and
  adopted by the next thread. The root's meaning is unchanged (process
  lifetime, readable from every thread; the design note and its
  memory-safety argument are `wolf_rt::str`'s module doc), so no rule
  moves on the checked machine or in lupin. `[conc.task.par.cost]`'s
  sentence that the root "serializes allocation behind one lock" is
  rewritten. bs50's word counter (#421: 72 pieces of ~380k characters,
  a `Map[str, int]` and a `lower()` per word) on kasumi (16 cpus):
  `par` 0.77–0.79 s with 6.3–7.3 s of system time and 178,911 futex
  calls before, 0.03–0.04 s and 2,556 after, against 0.18–0.21 s
  serial; under `taskset -c 0-3`, 0.63–0.75 s before and 0.05–0.07 s
  after.
- **Reads stop zeroing a buffer per call (#635).** `net_read`,
  `net_read_bytes`, `fs_read`, `fs_read_chunk` and `fs_read_at` read
  into one buffer a thread keeps (zero-filled only when it grows,
  never past the 1 MiB clamp) and copy the bytes read out of it, where
  each call used to allocate `max` zeroed bytes and free them.
- **Gathered writes and interpolations stop asking the host allocator
  (#635).** `net_writev` and `net_writev_head` build their `IoSlice`
  run on the stack for up to 16 parts (two malloc/free pairs a call
  before); an interpolation's buffer comes from a per-thread pool and
  keeps its capacity (a `Box`, a `realloc` per doubling and a free per
  interpolation before), and an integer hole formats in place. #635
  named the `realloc` as `net_writev_head`'s; measured, it was the
  interpolation buffer's.
- **Measured on lobo** (trunk `ebace85`, its release build, one hand,
  `perf stat -e instructions:u`, kasumi, median of three): user
  instructions a request keepalive 19,532 → 17,179 (−12.0 %), close
  20,847 → 18,182 (−12.8 %); the arena alone −4.1 % / −5.8 %, the
  reads −3.6 % / −2.9 %, the writes −4.9 % / −4.7 %.
- **Witnesses:** `wolf_rt`'s `str::tests` (a warmed thread allocates
  while the arena's lock is held; a dead thread's bytes survive later
  threads' churn; concurrent allocations never overlap),
  `crates/wolf_rt/tests/read_write_alloc.rs` (a counting global
  allocator: zero zeroed allocations and at most one host call per 50
  reads, gathers or interpolations once warm), and
  `crates/wolf_driver/tests/root_arena_threads_lanes.rs` (`par` and
  scoped-task strings read back on the spawner on native, release and
  lupin; the checked machine refuses both shapes by name).

### Exec and env unset: a program can become another, and remove a variable (s225, #534)

**Read this before you bump the pin.** Two new prelude names, both
`w0304: true` in `spec/prelude.json`: `os_exec` and `env_unset`. A
module that declares one draws W0304 (fatal under `--deny-warnings`);
no downstream does (boreutils, lobo, pelt, wolf-std, pax at their
trunks). No existing builtin's row changed. **One checked-machine
behaviour moved:** a child the checked machine spawns is now handed the
environment with the machine's overlay applied, so a variable the
program `env_set` reaches its children there as it always did on
native and release (before, a checked child saw only the host's
environment). **No compiler output changed:** boreutils `5da892e`,
lobo `ebace85`, pelt `3e7516c`, wolf-std `87ba162` (433 test files) and
pax `ef4c3ca` (29 kernels, freestanding objects), 1968 builds on native
and release, are byte-identical (debug info and build-id stripped)
with identical diagnostics and exit codes when trunk and this change
link one runtime archive; 1826 build and 142 refuse identically. Linked
with each side's own archive every linked binary differs, because a new
module in `wolf_rt` moves rustc's code-generation units (the freestanding
objects, which link no runtime, do not); that is layout, not behaviour.

- **`os_exec(exe, argv, env, map)` (`[os.proc.exec]`, new).** Replace
  the running program: the pid stays, the image goes. `argv` is the
  whole vector (argv[0] included), `env` the new program's whole
  environment as `NAME=VALUE` entries (`env_vars()` passed through
  keeps the current one), `map` `os_spawn_fds`'s pairs with an unmapped
  0, 1 and 2 left as this process's own; everything else the runtime
  opened is close-on-exec. A bare name is searched along the `PATH` of
  the handed `env`, or `/usr/bin:/bin`. Returns only with a row, in
  order: `invalid` (shape), `unsupported` (windows), `io` (a source),
  `not_found`/`denied`/`io` (the program); a failed exec puts back
  every descriptor the map moved. Result type unit, as `os_exit`'s.
  The checked machine replaces the `wolf` process itself, its print
  buffers written out, its working directory entered and `SIGPIPE`
  defaulted first; under `conform-run` no record follows a successful
  exec. Capability `exec`.
- **`env_unset(name)` (`[os.env.unset]`, new).** Remove a variable;
  absent is not an error; `env_set`'s names are `invalid`. Native and
  release write the real environment; the checked machine keeps a
  tombstone in its overlay. Capability `env`.
- **Every spelling was a ruling owed**, and is ruled as implemented
  (ruling #62, R1–R5):
  `env_unset` beside `env_set` (no `os_env_set`: `env_set` exists),
  `os_exec`'s execve shape, the handed environment's `PATH`, and the
  record-less checked exec.
- **lupin** (wolf-interp s225) mirrors both; its exec is std's
  `CommandExt::exec` for descriptors 0..2 and refuses a map above 2 or
  a close by name. `exec_lanes` pinned 0.1.49 (`f516a5f`) and its
  trunk (`51cb491`) as pre-mirror by version and commit until 0.1.50
  (dropped at this pairing).
- **Witnesses:** `corpus/os/exec_rows.lu`, `corpus/os/env_unset.lu`,
  two comptime rows, and `crates/wolf_driver/tests/exec_lanes.rs`: the
  new image's pid is the pid the harness started, argv[0] read from the
  kernel, SIGPIPE not ignored in the image, a file mapped AS descriptor
  5, an opened unmapped file closed, a failed exec putting descriptor 0
  back, and a child that no longer sees a removed variable — checked,
  native, release (and under `taskset -c 0-3`) and lupin. #534's
  signal-number half is s219's `os_wait_status`.

### The full stat record, `lstat`, `readlink`, and a listing as the host keeps it (s218, #625, #626, #536)

**Read this before you bump the pin.** Four new prelude names, all
`w0304: true` in `spec/prelude.json`: `fs_stat`, `fs_lstat`,
`fs_read_link` and `fs_read_dir_entries`. A module that declares one
draws W0304, which `--deny-warnings` makes fatal: rename yours. A grep for
a declaration of any of the four found none in wolf-std (`0f74ec5`),
boreutils (`50d8907`), lobo (`a728fab`), pelt (`dd22a86`), pax
(`396c5fb`) or the book (`fde77b8`). Nothing that existed changed:
`fs_read_dir` still sorts and still answers `utf8`, `fs_fstat` still
answers three words, and every path call still follows a link.

- **`fs_stat(path)` and `fs_lstat(path) -> List[int] ! {denied, io,
  not_found}` (`[os.fs.stat]`).** One record from one host call: kind
  (file, directory, other, symbolic link, fifo, socket, character and
  block device), size, modified_ms, a `have` mask, mode bits, link
  count, uid, gid, blocks, device, inode, rdev, and the access,
  modification, status-change and birth times to the nanosecond. The
  first three words are `fs_fstat`'s. `fs_lstat` answers a link itself,
  so a program can at last tell a link from its target, and a dangling
  link from a missing name. A word the host does not keep is 0 with its
  `have` bit clear: windows refuses mode, link count, owner, blocks,
  identity and ctime by name; linux answers a birth time where the
  filesystem keeps one. Two paths name one file when their device and
  inode agree (#536's `pwd -L` question).
- **`fs_read_link(path) -> List[byte] ! {denied, invalid, io,
  not_found}` (`[os.fs.readlink]`).** A link's target, as bytes, as
  stored; `invalid` when the path is not a link.
- **`fs_read_dir_entries(path) -> List[List[byte]] ! {denied, io,
  not_found}` (`[os.fs.readdir]`).** A directory in the host's own
  order, unsorted, each entry one byte list: its kind, then its name's
  bytes. A name that is not UTF-8 is listed like any other, so one odd
  name can no longer fail a listing, and a walk knows a directory (or a
  link) without a stat per entry. `ls -U` prints this order.
- Every machine: the checked machine reads the same metadata the runtime
  does, word for word; native and release call the runtime; lupin
  mirrors all four (wolf-interp s218). A path is still a `str`, so a
  name that is not UTF-8 can be listed but not yet stat'd by name; the
  record from an open handle stays `fs_fstat`'s three words. Witnesses:
  `crates/wolf_driver/tests/fs_stat_lanes.rs` (every word against
  `stat(1)` on linux).

## 0.2.26 — 2026-10-09

THE TWENTY-SIXTH. Five compiler lanes, each with its lupin half, land
together, and the pairing moves to lupin 0.1.49, which mirrors all of
them. A program's standard streams are bytes; a child's descriptors,
pipes and the working directory are the program's to set; `-> never`
and `!` on integers are in the language (rulings #50, #51); a loop can
keep each turn's result and free the rest (`copy region`, ruling #56);
a package's capabilities are what its code reaches (rulings #53, #54).
**One P0 is fixed (#618):** a call's `str` result held past a region
read freed bytes on checked, native and release. Five lanes, 149
commits integrated by r31 one at a time (s200 19, s213 42, s215 44,
s216 29, s217 15, each count including r31's integration commits),
then r31's release commits.

### Read this before you bump the pin

**New prelude names** (`cargo xtask prelude-diff v0.2.25 HEAD
--markdown`, after `git fetch --tags`, run at the release head):

> **New prelude names.** HEAD adds `fs_copy_chunk`, `os_spawn_fds`,
> `os_pipe`, `os_chdir`, `os_isatty`, `os_error`, `os_error_text`,
> `bytes_find`, `bytes_count`, and `never` (the nine functions at
> `[os.host.sigs]`; `never`: builtin_type, `[type.fn.never]`). A module
> that declares a function, type or binding named any of the nine
> functions now draws W0304 (it shadows the ambient name), which
> `--deny-warnings` makes fatal: rename yours. `never` draws no W0304.
>
> **New builtin marks:** `none` (`[os.host.sigs]`).

A grep for a declaration (`fn`, `let`, `var`, `const`, `type`,
`struct`, `enum`) of any of the ten in wolf-std `0f74ec5a`, boreutils
`50d89074`, lobo `a728fab7`, pax `396c5fbf`, the book `fde77b8d`, pelt
`dd22a86c` and muzzle `7c95300e` found **none**. No downstream calls any
of the nine functions yet; `os_isatty` appears only in two comments
(boreutils `src/ls.lu`, pelt `src/sh/interact.lu`) that wait for it.

**Packages that built are now refused (s217).** A package whose own
files call a host builtin in the fs, net, env or exec family, or that
imports a std module whose code does (`std.process` → `exec`, `std.os`
→ `env`), must declare the capability. The fix is the word in
`capabilities: [ … ]`; the E1504 names it. The census at wolf-std
`2f389a7`, boreutils `2f15585`, lobo `9167cb5`, pax `94364ad`, pelt
`f0014da` and the book `d0213d0` found **no build that changes**: only
lobo, pax's kernel and the book's `samples/pkg` have a manifest beside
the files they build, and none of them reaches an undeclared capability.

**`wolf audit` prints more.** Under `effective:` there is now a reason
line per capability, so a transcript with a non-empty set gains lines.
The book has five: `pkg/acquired` in ch24 (two), solutions.md and
exercises ch24 (`net: regex — declared (nothing in its code reaches
it)`), and appendix E's `pkg/promote` (`` fs: local/tally (root) —
declared; calls `fs_write_text` … ``). Transcripts with `effective: []`
are byte-identical.

**`wolf audit --ci` refuses where it passed.** It fails on an undeclared
capability with no build, and on a manifest whose directory holds no
wolf source. lobo is that case: `wolf.pkg` sits at the repository root
and the program builds from `./src/main.lu`, so the manifest governs no
build. Nothing in lobo runs `wolf audit` today. If lobo's manifest did
govern `src/`, it would need `capabilities: [env, exec, fs, net]`.

**A program that compiled may now be refused (#618, s216).** A call to a
fn returning `str` is an allocation site in the region it is called in,
so holding its result past that region is E1010 where it used to build
and read freed bytes. A fn whose every result is a string literal is
exempt. s216 measured boreutils, lobo, wolf-std's tests and pelt: the
same verdicts and byte-identical binaries. The way to keep a region
block's result is `copy region { … }` (`[mem.region.copyout]`).

**Spellings ruled.** `-> never` (ruling #50) and `!` on an integer as
its bitwise complement (ruling #51) were implemented ahead of their
rulings in s213 and are ruled A as implemented; `copy region { … }` is
ruling #56 = A. ruling #53 keeps stdio, the clock and random free of any
capability; ruling #54 charges `os_chdir` to `env` and `os_pipe` to
`exec` (`os_isatty` stays free).

### The pairing: lupin 0.1.49

`PAIRING` names **lupin 0.1.49 at pin `294d626`** (wolf-interp release
408028965; the linux x86-64 archive's sha256 `84911a35…`). The spec pin
is unchanged (this compiler's v0.2.24 tag). 0.1.49 carries is74
(volatile access and the raw-pointer atomics, wolf-interp#185, #194),
is75 (the CI slot budget) and the lupin halves of s200, s213, s215 and
s216.

- **Dropped 45 cases in eight gates**, each green on the published
  0.1.49 unpinned: `atomic_lanes` 9 (#194), `volatile_lanes` 5 (#185),
  `byte_surface_lanes` 10 (s200), `papercut_lanes` 5 (s213, #200),
  `raw_align_lanes` 2 (#205, answered by s213's struct pointee),
  `proc_fd_lanes` 9 (s215), `region_copyout_lanes` 4 (s216), and
  `pairing` 1. The emptied lists stay, empty, as the gates' machinery.
- **Kept.** attr_closed_set's control, carried to 0.1.49 measured (lupin
  still declines its `comptime fn`, #174); the racy counter's
  `trap(race)` check names 0.1.49 too (DIV-2026-028).
- **Corrected, not widened (#606).** On the volatile and atomic UB rows
  lupin is held to the verdict, the row and the clause, without the
  checked machine's E1401: a `ub` record carries no diagnostic
  (`[proto.record.verdict]`), as `raw_align_lanes` already held it.
- **Widened 0.** The plant (the 0.1.49 lists against the published
  0.1.48) is red on exactly the 45 dropped cases.
- **The ritual** (#87, with #281's control against the 0.1.48 archive,
  the same corpus and the same release `wolf`, 1057 files):
  - checked: **moved 39 ledger counts**. Agreements 625 → 657, hard
    22 → 16, SOUNDNESS 3 → 0 (s216's three copy-out rows, where 0.1.48
    trapped, now agree).
  - native: **moved 40**. Agreements 658 → 685, hard 33 → 33,
    SOUNDNESS 11 → 14: the three copy-out rows leave it, and six UB rows
    lupin now reaches join the class every UB row holds on native, which
    compiles undefined behaviour without a check (`atomic_ub_misaligned`,
    `atomic_ub_uaf`, `volatile_ub_misaligned`, `volatile_ub_uaf`,
    `raw_ub_misaligned_repr_c_field`, `…_field_store`).
  - Every move is on the five lanes' rows or is74's, toward agreement,
    a completeness note, or that UB class. Four checked files and three
    native moved below the ledger (fail-pinned rows whose lupin answer
    moved to the pinned code, and `atomic_counter` on checked).

### The byte surface: bytes on descriptors 0, 1 and 2, the byte scan, the copy the host makes, and the host's number (s200, #405, #411, #417, #407)

**Read this before you bump the pin.** Five new prelude names, all
`w0304: true` in `spec/prelude.json`: `fs_copy_chunk`, `os_error`,
`os_error_text`, `bytes_find` and `bytes_count`. A module that declares
one draws W0304, which `--deny-warnings` makes fatal: rename yours. A
grep for a declaration of any of the five (`fn`, `let`, `var`, `const`)
found none in wolf-std (`2f389a7`), boreutils (`2f15585`), lobo
(`b4975f7`) or the book. **Four calls that answered `io` now answer
data**: `fs_read`, `fs_read_chunk`, `fs_write` and `fs_write_chunk` on
0, 1 and 2. A program that relied on `io` there to detect "not a
handle" now reads its standard input or writes its standard output.
No row changed anywhere.

- **Bytes on the standard streams, through the descriptors themselves
  (#405).** The four byte and text calls serve 0, 1 and 2
  (`[os.fs.std]`): a read of 0 starts at the offset the process shares
  with whoever started it and moves it, a pipe, socket or terminal is
  read as what it is, and a write to 1 or 2 lands in program order with
  `print` (the native tiers write the descriptor under `print`'s lock).
  A closed descriptor is `io` (`EBADF`), never a silent success and
  never a write into whatever file took the number. Bytes `read_line`
  read ahead come first. `fs_close` on 0..2 stays `io`. The checked
  machine reads descriptor 0 (or a test's input buffer) and writes 1
  and 2 into its capture, now kept as bytes and decoded once at the
  end, as the driver decodes a native child's stdout. This retires
  the `/dev/stdin`/`/dev/stdout` reopen boreutils uses.
- **`bytes_find(xs, b, from) -> int ! {none}` and `bytes_count(xs, b) ->
  int` (#411, `[mem.list.bytes]`).** `str`'s `find` and `count` for a
  `List[byte]`. On native and release each is one call into a runtime
  loop in the target's baseline vector instructions (SSE2 on x86-64,
  NEON on aarch64; scalar elsewhere): counting the newlines of 256 MiB
  costs about 12 ms of CPU above the read on kasumi, against about
  100 ms for a wolf `for b in chunk` loop (`bench/byte-scan/`). `from`
  outside `0..len` is `none`. Free builtins, because
  `List`'s `count` method is its length.
- **`fs_copy_chunk(src, dst, max) -> int ! {eof, io}` (#417,
  `[os.fs.copy]`).** `fs_read_chunk` then `fs_write_chunk` fused: at
  most `max` bytes from `src`'s offset to `dst`'s, the count answered,
  `eof` at the end. linux tries `copy_file_range`, `sendfile`, `splice`
  and then a read/write loop, remembering the rung each pair settled
  on; macOS, freebsd and windows run the loop, by name. A table handle
  is duplicated for the call, so a copy that parks never holds the
  handle table's lock. A socket destination stays open on #417.
- **`os_error() -> int` and `os_error_text(code) -> str` (#407,
  `[os.fs.error]`).** The host's error number for the task's most
  recent fallible fs call (`errno`; `GetLastError` on windows), 0 when
  it succeeded or failed before the host was asked, and the host's
  words for a number ("No space left on device"). The number travels
  BESIDE the row, never on it: a payload on `io` breaks W0603's pact
  and every `io` arm downstream. The word is per task. The net and os
  families do not set it yet.
- The host table is 83 lines. Witnesses: `corpus/fs/std_write_bytes.lu`,
  `corpus/fs/copy_chunk.lu`, `corpus/fs/os_error.lu`,
  `corpus/memory/bytes_scan.lu`, the fixtures under
  `crates/wolf_driver/tests/fixtures/byte_surface/`, and
  `byte_surface_lanes.rs`, which runs them on checked, native, release
  and lupin with standard input a file, a pipe and a shared offset, a
  read-only stdout (`EBADF`) and `/dev/full` (`ENOSPC`). It pins lupin
  0.1.47 and 0.1.48 by version and release commit until wolf-interp's
  mirror ships. The micro-benchmark is `bench/byte-scan/`.

### The kernel's papercuts (s213, #572, #575, #577, #579)

Two of the four were spellings the maintainer ruled on 2026-10-07:
`-> never` (#572, ruling #50 = A) and `!` on integers (#575, ruling #51
= A), each as s213 implemented it.

- **`m.K` reads another module's `pub const` (#579).** A `pub const`,
  `pub let` or `pub var` read through its module's name after `use m`
  is the item, exactly as the bare name inside its own module
  (`[mem.static.4]`). Native and release refused it ("a member access
  without a recorded type"), the checked machine refused it ("module
  items in checked execution"); lupin already ran the `const` and `let`
  reads. A `var` so named is raw-tier, read or written (`m.V = x`,
  `m.V op= x`) only inside `unsafe`: E1301 outside it, which the
  compiling machines never reached before.
- **`p[i].f = v` stores a field of a raw element (#577).** Plain,
  compound (`p[i].f += 1`), nested (`p[0].inner.y = 5`) and through a
  dereference (`(*p).f = v`), at the field's `[abi.layout.c]` offset,
  touching no other byte (`[mem.unsafe.raw.5]`). Every compiling machine
  refused it ("assignment through this place shape"), outside `unsafe`
  too, where it is now E1301. The checked machine asks row L4 of the
  ELEMENT, as s209's read does: a packed struct's `u64` at offset 2 is
  defined, a plain `#[repr(c)]` element at a misaligned address is L4
  whichever field is named. The checked machine now also reads
  `(*p).f` and nested paths. A whole-aggregate raw store (`p[0] =
  Pair { … }`) is still refused by the checked machine (unchanged).
- **`!` on an integer is its bitwise complement (#575, ruling #51 = A).**
  `x & !0xfff`, `(x + 0xfff) & !0xfff`, `e & !bit` and `const MASK:
  u32 = !0xfff` work at the operand's own width (`[type.int.not]`); a
  `byte` widens to `int` first (`[type.byte.op]`). It was E0409. The
  checked machine now also runs `&`, `|` and `^` on plain (checked)
  integers — total over every range it holds — where it refused them;
  shifts stay refused there. A `u64`/`uint` complement lies past the
  checked machine's range and is refused by name, as such a literal is.
- **`-> never` declares a function that never returns (#572, ruling
  #50 = A).** `never` is a built-in type name, written as a return type
  (`[type.fn.never]`); a call to such a function is bottom, so `bad =>
  die("…")` fits an arm that wants an `int`, and a bodyless
  `extern "c" fn abort() -> never` crosses the membrane as `void`. A
  `-> never` body that can reach its end, or holds a `return`, is
  E0401. The bottom type now renders as `never` (it rendered as `!`,
  which no diagnostic snapshot printed). `never` is exempt from W0304,
  as `range` is: wolf-std's tests bind a local named `never`.
- **New prelude name:** `never` (`builtin_type`, `w0304: false`).
- lupin 0.1.47 and 0.1.48 part on the `var`, field-store, complement
  and `never` rows; its half is wolf-interp#200, pinned by version and
  release commit in `papercut_lanes.rs`. Anchors 595 → 599. Found on
  the way and filed, not fixed here: **#601** (a silent wrong answer on
  native and release: a raw-pointer or module-`var` read after a call
  that writes it returns the pre-call value; since fixed by s214 in
  0.2.25) and #602 (the checked machine prints a
  negative narrow signed `wrapping` value unsigned).

### A child's descriptors, a pipe, the working directory, isatty (s215)

A wolf program can now build a shell's plumbing from argv alone: a
pipeline, a redirection, `2>&1`, `N<file`, `cd`. Four host builtins
(with s200's five, the host table is 87 lines, `RT_SYMBOLS` 173,
the prelude 94 functions):

- **`os_spawn_fds(exe, args, map) -> int ! {denied, invalid, io,
  not_found, unsupported}`** (`[os.proc.fds]`). `map` is a flat
  `List[int]` of `(child fd, source)` pairs: a source is a handle of
  this process (0..2 its standard streams, 3 and up an fs handle such as
  a pipe end) or `-1` for "closed in the child". Unmapped descriptors
  keep `[os.proc.spawn]`'s posture (0 the null device, 1 and 2
  inherited, nothing else open). Refusals come in a fixed order and
  none makes a child: the map's shape (`invalid`), the host (windows
  answers `unsupported` to a non-empty map), the sources (`io`), then the
  program. `os_spawn_with`'s listener set stays as it is: its sources are
  net handles, a separate numbering space. `exec` at comptime.
- **`os_pipe() -> (int, int) ! {io}`** (`[os.proc.pipe]`): the read and
  write ends as fs handles, close-on-exec, served by every handle call.
- **`os_chdir(path) -> () ! {denied, io, not_found}`** (`[os.fs.chdir]`,
  wolf-std F-0067's ask): the write twin of `os_cwd`. The checked machine
  keeps a machine-local directory, as it keeps `env_set`'s overlay, and
  every fs path, unix-socket path, `os_cwd` and spawn follows it.
- **`os_isatty(fd) -> bool ! {io}`** (`[os.fs.isatty]`): closed or forged
  is `io`, never `false`.

At comptime `os_spawn_fds` and `os_pipe` are `exec`, `os_chdir` is `env`
and `os_isatty` is `io` (ruling #54; s217 charges capabilities from these). lupin 0.1.47
resolves none of the names (pinned pre-mirror in `proc_fd_lanes.rs`);
its mirror serves descriptors 0..2 and refuses a target above 2 or a
close by name. Witnesses: `corpus/os/{pipe_round_trip,chdir_relative,
spawn_fds_rows}.lu`, `corpus/comptime/sandbox_{chdir,pipe,isatty}.lu`
(E0701), `sandbox_spawn_fds.lu` (a forward pin: the comptime engine
cannot build its `List` arguments, so `crates/wolf_sema/tests/sandbox_s215.rs`
pins its `exec` entry), and
the unix gate `crates/wolf_driver/tests/proc_fd_lanes.rs` (a two-stage
pipeline, `2>&1` onto a pipe, an explicit fd 5, no leaks, a child in the
moved directory, `isatty` under a pipe and under `script(1)`).

### A loop can keep each turn's result and free the rest: `copy region { … }` (s216, #612; ruling #56 = A)

- **`copy region name? { … }`** (`[mem.region.copyout]`). The block runs
  as `region { … }` does; at its closing brace its VALUE is deep-copied
  into the region it was entered from (every `str` it reaches
  materialized there, every `List` and `Map` a fresh buffer), and then
  the block's region is freed. A shell's command loop, a server's
  request loop or a kernel's event loop computes a turn inside the block
  and keeps only the result. #612's witness (twenty strings and a list
  per turn, one string kept in a map) peaks at 27.8 MB at 20,000 turns
  and 53.1 MB at 40,000 with no region, and at 2.9 MB and 3.6 MB with the
  prefix, on native and release alike; what still grows is the kept
  string itself (about 32 bytes a turn here), because the region holding
  the loop's state (the process root) frees nothing. Keeping a scalar,
  the loop is flat. Applied to pelt's simple-command expansion in a
  throwaway copy: 163 MB (empty directory) or 490 MB (24 files) at
  20,000 turns down to 34 MB.
- **Every other escape is still E1010.** Only the block's own value at
  its `}` is copied: an outer binding, a `return`, module state or a
  channel holding something built in the block is refused as before, and
  a value with no independent copy (a region, a fn value, a cell, a
  channel or other handle, a pool, a raw pointer, a trait object) is
  E1010 at the block, naming the part. A raw pointer into the freed
  region read afterwards is row P4 on the checked machine.
- **Cost:** one deep copy of the value per execution, in time and in
  bytes in the enclosing region; nothing for a program without the
  prefix. On native and release a `Map` is rebuilt entry by entry; an
  enum or error-row payload holding a string or a container is refused
  by name there (`unsupported`), as for a plain `copy`.
- **The spelling: ruling #56 = A** (the maintainer, 2026-10-07), as
  implemented (no new token or keyword); the alternatives (a materializing
  `copy v in outer`, a per-iteration loop region) and their costs are in
  the clause.

### A silent wrong answer is fixed: a call's `str` result left its region (s216, #618)

- A call to a declared fn returning `str` carried no allocation site, so
  `region scratch { out = build(7) }` handed `out` freed bytes on
  checked, native and release (lupin traps `region-fault`); so did
  #612's own `st.vals["x"] = copy v` witness, which #612 reported as
  E1010 but which built at 0.2.24. Such a call is now a site in the
  ambient region and carries its arguments' and receiver's sites
  (`[mem.region.escape]`); a builtin container method's `str` element
  (`xs.last()`) carries the receiver's.
- **A program that compiled is now refused** where it held such a call's
  result past the region the call was made in. A fn whose every result
  is a string literal is exempt (its bytes are static), which keeps
  boreutils' `cat` (`why = bore.io_error()` inside `region pass`)
  building. Measured, trunk against head on both tiers: boreutils' 27
  utilities, lobo, wolf-std's 423 test files and pelt build with the
  same verdicts and byte-identical binaries. Still refused though lupin
  runs it: a fn returning a parameter's bytes, called inside a region
  and held past it.

### A package's capabilities are what its code reaches (s217, #615)

I13 counted imports only. A dependency that declared `caps=[]` and
called `fs_read_text` directly, with no `use std.fs`, built and ran, and
`wolf audit` printed `effective: []`. `wolf audit --ci` also passed on an
undeclared `use std.net`, which only the build refused. Now a package
reaches a capability three ways, and E1504 refuses any it does not
declare:

- an import the facade rule names (`std.net`, `std.fs`, `std.env`,
  `import c`), as before;
- a prelude host builtin named anywhere in the package's own files. The
  sandbox table (`ctfe::intrinsics::host_stub`) gives its capability:
  the fs, net, env and exec families carry `fs`, `net`, `env`, `exec`.
  Stdio, the clock and randomness carry nothing, because the manifest
  has no word for them. A fn the package declares under a builtin's name
  is its own;
- a std module it imports, for what that module's code reaches, closed
  over the std modules it imports in turn. Until now a std module's
  imports were charged to the root package, so `use std.process` asked
  for `net` and never for `exec`.

The E1504 for a builtin names it and points at its call site. `wolf
audit` resolves the code itself, with no build and no `wolf.sum`. It
prints `effective` as every declared capability united with every
reached one, then one reason line per capability and package (`` fs: pad —
UNDECLARED: calls `fs_read_text` at pkg://pad/pad.lu:4:16 ``). `--ci`
refuses an undeclared capability, and refuses when it cannot read the
code. A directory of standalone entries (`//! member: false`) is
audited entry by entry. `wolf tree` is unchanged. Witnesses:
`crates/wolf_driver/tests/cap_reach.rs`.

## 0.2.25 — 2026-10-07

THE TWENTY-FIFTH. Three silent wrong answers on the compiled tiers are
fixed (s214: wolf-lang#598, #600, #601), and the pairing moves to lupin
0.1.48, which carries this compiler's v0.2.24 spec and corpus. lupin is
no longer two releases behind, and sixteen gate pins drop. One lane, 39
commits (s214), then r30's 10: 49 since v0.2.24.

### Read this before you bump the pin

**New prelude names:** none (`cargo xtask prelude-diff v0.2.24 HEAD
--markdown`, after `git fetch --tags`, run at the release head). There
is nothing to grep downstream.

**Three silent wrong answers are fixed (s214).** A program that printed
a wrong value on native or release now prints the value the checked
machine and lupin always printed.
- **#598.** On native and release, a store to a module `var`, to
  `extern "c" let` storage or through any raw pointer was forwarded
  across a call to a function that wrote it. The caller read its own
  stale store: checked and lupin printed 99, both compiled tiers 0.
- **#601.** The same family on loads: a raw-pointer or module-`var`
  read after a call that writes the memory returned the pre-call value
  (the builder's GVN joined foreign-region loads across calls).
  Every call now clobbers that memory. A `List` lent to the function
  stays frozen for the call, as `[mem.tier0.mode.read]` says, so its
  loads keep their reuse. The print shims are inert calls and keep it
  too. `[mem.static.2]` now says it on every machine: no value read or
  stored before a call stands for that memory after it.
- **#600.** On release, a branch on the top half of an unsigned shift
  (`a >> 63 == 1`, `cr2 >> 39 == 0x1ffff80`) was deleted, because
  rangeopt bounded `x >> s` by the signed maximum. It now uses the
  width's unsigned maximum.

Downstream binaries gain reloads after calls. No downstream test changed
its answer.

**One gate was overridden for this release (ruling #52).** The
compile-track bench gate (D5) went red on one counter,
`wir/compile/rust/wir_forward_hits`: −3.5%, 57 → 55 on the corpus,
deterministic. The two lost hits are the forwards across a writing call
that #598 was, so restoring them would restore the bug. The runtime cost
is within noise (boreutils CPU −2.8% to +3.9%). The maintainer ruled a
one-time exception: s214 merged over that one step, and the next
nightly re-baselines. It is not a precedent.

### The pairing: lupin 0.1.48

`PAIRING` names **lupin 0.1.48 at pin `294d626`** (wolf-interp release
405340127; the linux x86-64 archive's sha256 `81cfd77a…`). Its spec pin
is this compiler's v0.2.24 tag. 0.1.48 carries is72 (`lupin
--trace-places`, a JSON-lines trace of every place's state after each
statement) and is73 (the re-pin blockers: module initializers, `extern
"c" let`, `#[section]`, `packed`, `align(N)` and the layout queries,
wolf-interp#190, #188). The re-pin filed one divergence, DIV-2026-028:
kw11's `conc/atomic_race_plain.lu` is `check: pass`, and lupin's
`trap(race)` is the detection `[conc.mm.race.3]` permits
(wolf-lang#603).

- **Dropped 16.** These rows now answer as the machines do on 0.1.48,
  measured unpinned:
  - `static_lanes`, 7 (#190): E0705 on both initializer rows (0.1.47
    overflowed its stack on the cycle), E1301, E0821, and by-name
    refusals for a `List` in module state, a link-time symbol and
    section placement;
  - `repr_layout_lanes`, 9 (#188): the layout queries' numbers, E0708,
    E0819, E0820, and the packed and aligned rows.
- **Kept.** Each is carried to 0.1.48 with its 0.1.47 verdict, measured
  row by row and naming its open issue:
  - attr_closed_set's control, 1 (lupin declines its `comptime fn`);
  - volatile, 6 (#185);
  - kw11's atomic rows, 9, plus the racy counter's `trap(race)` (#194).
- **Re-keyed.** `raw_align_lanes`' repr(c) element keeps its verdict
  (`unsupported`, "has no member `b`") and now names wolf-interp#205: a
  raw pointer to a `#[repr(c)]` struct still reads its element as bytes.
- **Kept, verdict changed** (ruled at r30). `raw_align_lanes`' packed
  field was `fail(E0817)` while `packed` was refused (#188). On 0.1.48
  `packed` is admitted, and the struct pointee is declined by name:
  `unsupported`, "has no member `limit`" (#205).
- **Widened 0.** The plant (the 0.1.48 lists against the published
  0.1.47) is red on exactly the sixteen dropped cases.
- **The ritual** (#87, with #281's control against the 0.1.47 archive,
  the same corpus and the same release `wolf`):
  - checked: **moved 8 ledger counts**. Agreements 605 → 607, hard
    20 → 16, SOUNDNESS 0 → 0 (1007 files).
  - native: **moved 11**. Agreements 642 → 644, hard 34 → 27,
    SOUNDNESS 8 → 8.
  - Every move is on is73's rows or #205's, toward agreement or a
    by-name decline. Nine checked files and six native moved below the
    ledger: fail-pinned rows whose lupin answer moved to the pinned code
    (E0705, E0819, E0820, E0821, E1301), which reaches no count by
    design.

### Stale stores and loads across calls, and the shifted range (s214, #598, #601, #600)

- The builder clobbers foreign storage at every call and `call.ind`:
  module `var`s, `extern "c" let` storage and raw-pointer memory. No
  store is forwarded and no load reused across one, in a loop body
  either. Without a call, and for memory the fn owns, forwarding is
  unchanged.
- A `List` lent to the function is frozen for the call: its `len`, its
  data pointer and its elements are reused across calls. The print shims
  are inert calls.
- rangeopt bounds a logical shift by the width's unsigned maximum.
- New corpus rows (#598: the witness, a read-call-read, a writer two
  calls deep and through a fn value, a callee writing through a raw
  pointer; #600: top-half branches on `wrapping[u64]`/`wrapping[u8]` and
  on a plain `u64`/`u8` from a C allocation). New gates:
  `stale_global_lanes` (four machines, C writing `extern let` storage
  and calling back, a freestanding kernel) and `shr_range_lanes` (four
  machines and the issue's hosted witness behind listed assembly).
  Plants were seen red in CI for #598 and #600, then reverted.

## 0.2.24 — 2026-10-06

THE TWENTY-FOURTH. A kernel can share a word between cpus and can
allocate. Atomics and fences work on raw pointers (`[conc.mm.atomic.*]`,
`[conc.mm.fence]`, E1308, E1309). On `x86_64-unknown-none`, `List`,
`Map`, interpolation, closures and `region` compile against an allocator
the program supplies, and every archive ships the freestanding runtime
`libwolf_rt_none.a` beside `wolf`. Ruling #36 makes a misaligned raw
access UB row L4. A module string holds its value; it printed its own
source on every compiler machine. `wolf prelude --json` publishes the
names a program gets without an import, and `cargo xtask prelude-diff`
lists the ones each release adds. The pairing moves to lupin 0.1.47,
which mirrors row L4; its spec pin stays v0.2.22 (below). Six lanes and
129 commits (s210, s209, s212, s211, kw12, kw11), then r29's 10: 139
since v0.2.23.

### Read this before you bump the pin

**New prelude names** (`cargo xtask prelude-diff v0.2.23 v0.2.24 --markdown`,
after `git fetch --tags`; run at the release head as `v0.2.23 HEAD`):

> **New prelude names.** HEAD adds `fence` (`fence`: function, `[conc.mm.fence]`). A module that declares a function, type or binding named `fence` now draws W0304 (it shadows the ambient name), which `--deny-warnings` makes fatal: rename yours. (`cargo xtask prelude-diff v0.2.23 HEAD`)

`fence` has `w0304: true` in `spec/prelude.json`. A grep for a
declaration named `fence` (`fn`, `let`, `var`, `const`, `struct`,
`enum`, `type`, `union`, `mut`) found none in wolf-std (`2f389a7`),
boreutils (`2f15585`), lobo (`b4975f7`) or the book (`76143be`). lobo's
19 uses of the word are comments and strings.

**A program that parsed is now refused (s211, #589).** A keyword member
name continued onto a new line at the item's own column is refused. In
`let A: int = B + .` followed by `let B: int = A * 2`, the dangling `.`
used to take the next line's `let` as a member name and swallowed `B`.
A declaration keyword that starts its line at or left of the item's
column is now always a sibling, never a member. A member on the same
line, or on an indented continuation line, parses as before.

**A silent wrong answer is fixed (s210, #585).** A module-level `let` or
`const` string literal printed its source on checked, native and
release: `"""` strings kept their quotes and indentation, raw strings
kept their fence, and `\u{…}`, `\x…` and `{{`/`}}` printed raw. It now
holds the same bytes as the same literal inside a fn. A program that
printed such a string prints different bytes on 0.2.24.

**Programs that compiled are now refused.** An atomic on a pointee that
is not a fixed-width integer is E1308. An order the operation does not
admit is E1309. E1301 covers atomics and every fence except
`fence(Order.seq_cst)` outside `unsafe`.

**Programs that were refused now compile.** `List`, `Map`,
interpolation, capturing closures and `region` on
`x86_64-unknown-none`, given `wolf_alloc`/`wolf_free` (kw12). The
atomic methods and `fence` (kw11). Hosted programs that use none of
this build byte-for-byte as before.

**Packagers:** the archive has a new member, `libwolf_rt_none.a`. It
installs beside `wolf` and `libwolf_rt.a`. `cargo xtask dist` builds it
only with the `x86_64-unknown-none` target installed, and skips it
loudly otherwise.

### The pairing: lupin 0.1.47

`PAIRING` names **lupin 0.1.47 at pin `8e36bc1`** (wolf-interp release
404283632; the linux x86-64 archive's sha256 `0ddc4ff3…`). Its spec pin
is still this compiler's v0.2.22 tag, two releases behind. The v0.2.23
re-pin was measured and held back: on the v0.2.23 corpus lupin
overflows its stack on `memory/static_init_cycle.lu` (wolf-interp#190),
cannot parse `extern "c" let` (#190), answers E0817 on six layout rows
(#188), and its fuzz test finds a two-diagnostic record (#196). 0.1.47
carries s209's mirror of ruling #36 (row L4, wolf-interp#192) and
nothing else.

- **Dropped 6**: `raw_align_lanes`' L4 pins, one test over six rows. On
  0.1.47 every L4 row answers `ub(mem.ub)`, `x-ub-row` L4, `x-ub-clause`
  `mem.unsafe.raw.4`, at the checked machine's span. **A gate
  correction rides with the drop:** s209's gate (`befed047`) held lupin
  to the checked machine's E1401 diagnostic, but a lupin `ub` record
  carries none (only a `fail` carries one; `int_ptr_lanes` already
  holds lupin to verdict and row). lupin is now held to verdict, row
  and clause.
- **Kept 34**, each carried to 0.1.47 with its 0.1.46 verdict, measured
  row by row, each naming its open issue:
  - attr_closed_set's control, 1 (lupin declines its `comptime fn`);
  - volatile, 6 (#185);
  - repr_layout, 9 (#188);
  - static, 7 (#190);
  - raw_align's repr(c) element and packed field, 2 (#188);
  - kw11's atomic rows, 9, plus the racy counter's `trap(race)` (#194).
- **Widened 0.** The plant (the 0.1.47 lists against the published
  0.1.46) is red only on the L4 test.
- **The ritual** (#87, with #281's control against the 0.1.46 archive,
  the same corpus and the same release `wolf`) **moved 6 ledger counts
  on each tier**, all on `memory/raw_ub_misaligned_*` (u16/u32/u64,
  read and write):
  - checked: `Soundness → agreement`, agreements 595 → 601, hard
    26 → 20;
  - native: `agreement → SOUNDNESS`, agreements 643 → 637, SOUNDNESS
    2 → 8, hard 28 → 34. This is the row's design: native runs the
    misaligned access (`compiled: None` in the gate; O12 licenses it),
    and lupin now names it UB, as the checked machine does.
  - Nothing moved below the ledger. Native's standing SOUNDNESS pair
    is `unsafe_ub_uaf.lu` and kw11's racy counter.

### Atomics and fences on raw pointers (kw11, ruling K5 = A)

- **A kernel can share a word between cpus.** On a `*T` with a
  fixed-width integer pointee, inside `unsafe`: `p.atomic_load(o)`,
  `p.atomic_store(v, o)`, `p.atomic_swap(v, o)`, `p.atomic_add`,
  `atomic_sub`, `atomic_and`, `atomic_or`, `atomic_xor` (each the old
  value, the arithmetic wrapping at the width) and
  `p.atomic_cas(expected, new, success, failure) -> (T, bool)`; and the
  builtin `fence(o)` (`[conc.mm.atomic.raw]`, `[conc.mm.fence]`). The
  order is a mark of the closed builtin enum `Order` — `Order.relaxed`,
  `.acquire`, `.release`, `.acq_rel`, `.seq_cst` — written at the call
  (`[conc.mm.atomic.order]`).
- **Native runs every order as `seq_cst`** (Cranelift's atomics are
  sequentially consistent); **release passes each order to LLVM**. Each
  call is its instruction in place, hosted and freestanding, with
  nothing imported: `atomic_disasm.rs` reads every operation, width and
  order on x86-64 (`lock xadd`, `lock cmpxchg`, `xchg`, `mfence`) and
  aarch64. Four tasks' `atomic_add`s and CAS loops count exactly on 1,
  4 and all cpus (`atomic_witness.rs`).
- **The checked machine runs every order as `seq_cst`**, one task (it
  runs no task concurrency); the weaker-order outcomes it cannot
  produce are named in `[conc.mm.atomic.raw.5]`. A misaligned atomic
  address is row L4.
- New codes: **E1308** (an atomic operation on a pointee that is not a
  fixed-width integer: `int`, `uint`, `byte`, `bool`, …) and **E1309**
  (an order the operation does not admit — a release load, an acquire
  store, a CAS failure order stronger than its success order or a
  release one, `fence(Order.relaxed)`, an operand that is not a mark,
  `Order` as a value). E1301 names atomics and every fence but
  `fence(Order.seq_cst)`. **New prelude names:** `fence` (`cargo xtask
  prelude-diff v0.2.23 HEAD`); a module that declares its own `fence`
  now draws W0304. `Order` is no prelude name (an order operand is a
  mark), so a program's own `Order` type is untouched.
- lupin 0.1.46 answers `unsupported`; its half is wolf-interp#194,
  pinned by version in `atomic_lanes.rs`. The safe `std.sync`
  `Atomic[T]` is a later layer. Anchors 587 → 595.

### A kernel can allocate: the allocator hook (kw12, K8(b) = B)

- **On `x86_64-unknown-none`, `List`, `Map`, string interpolation, a
  capturing closure and `region` now compile**, on both tiers, with
  their hosted meaning (`[abi.target.none.alloc]`). They were refused
  by name ("`List` allocates, and target x86_64-unknown-none has no
  allocator").
- **The program supplies the allocator**: `wolf_alloc(size, align) ->
  *u8` and `wolf_free(p, size, align)`, in wolf or assembly
  (`[abi.target.none.hooks]` (d)). The runtime always asks for 16-byte
  alignment. It frees a region's blocks when the region ends, and an
  interpolation's buffer when it finishes. It never frees the process
  root. A null from the hook traps `alloc-contract`.
- **The freestanding runtime is `libwolf_rt_none.a`**: the `no_std`
  build of the region runtime (crate `wolf_rt_none`). It shares the
  hosted list and map source. It ships beside `wolf` in every archive,
  and a build that allocates writes it beside its object as
  `K.rt-none.a`. Its members leave only `wolf_alloc`, `wolf_free` and
  `wolf_trap` undefined; its C memory functions are weak.
  `cargo xtask rt-none` builds it. A toolchain without the
  x86_64-unknown-none target (`rustup target add x86_64-unknown-none`)
  skips loudly, and CI requires it (`WOLF_RT_NONE_REQUIRE=1`).
- **Still refused by name**: `Pool`, `freeze`, boxed channel payloads,
  `spawn`, `spawn proc`, channels, `par`, and float holes.
- **Hosted programs do not move.** `libwolf_rt.a` is untouched, and
  downstream binaries are byte-identical.
- **Witness**: `freestanding_alloc.rs`. One report is built through
  the hosted runtime and through a kernel over a bump allocator written
  in wolf, linked `-nostdlib`. The bytes are equal on both tiers.

### The prelude's names, as data (s212, ruling #39 = B, #586)

- **`wolf prelude [--json]`** prints every name a program uses without an
  import: 123 names (17 `builtin_type`, 8 `type`, 84 `function`, 8
  `intrinsic`, 6 `provisional`) and the 17 builtin marks, the row tags
  the host builtins declare. Each name comes with its kind, the spec
  anchor that defines it, and whether a module that declares it draws
  W0304. The rows are the checker's own tables, which resolution and
  W0304 read, so the list cannot drift. The schema is `wolf-prelude/0`
  (`docs/prelude-json.md`). `spec/prelude.json` is the same output,
  committed beside `anchors.json`, and a test reds when the two part.
  30 names have no defining clause yet (the print family, `min`, `zip`,
  four reflection intrinsics, the six stand-ins, and the numeric scalars,
  `bool` and `wrapping`). Their anchor is `null`, so the gap is visible.
- **`cargo xtask prelude-diff <tagA> <tagB> [--markdown]`** lists the
  names a release adds and removes. It reads `spec/prelude.json` at each
  tag, or the tag's `prelude.rs` tables before that file existed.
  `--markdown` prints the "Read this before you bump the pin" paragraph.
  The two releases that went unannounced: `v0.2.22..v0.2.23` adds
  `align_of` and `offset_of`. `size_of` is not new; 0.2.15 already
  warned on a module's `fn size_of`. `v0.2.15..v0.2.16` adds `Scope` and
  `Proc`.
- No compiled program changes; the tables gained columns, and no name
  was added or removed.

### The nightly's blast-radius property is green again (s211, #589)

- **A dangling `.` no longer swallows the next declaration.** A `.` at
  the end of a module-level initializer took the next line's `let` as a
  keyword member name (`let A: int = B + .` over `let B: int = A * 2`),
  so `B` vanished into `A`'s initializer. A declaration keyword that
  leads its line at or left of the item's column is now a sibling, never
  a member, as `fn`/`struct`/`pub` already were across a line break. A
  keyword member continued onto a new line at the item's own column
  (`x.\nlet`) is refused where it was accepted; members on the same
  line, and indented continuation lines, parse as before.
- **Recovery stops at a sibling's attribute, not only its keyword.**
  Under an unclosed `{`, a column-0 `#[…]` was eaten by the wreck above
  it and the decorated item lost its start (found at budget 1000 on
  `grammar/attr_section.lu`).
- The property holds at budget 300 and 1000 over the whole corpus; no
  bound moved. Three mutations pinned in `SWEEP_1000`. Hosted binaries
  are byte-identical.

### A misaligned raw access is UB row L4 (s209, ruling #36 = A, #574)

- **The machines disagreed, and release could miscompile.** A `*u16`,
  `*u32` or `*u64` read or write at an address that is not a multiple of
  the pointee's alignment (`p.with_addr(p.addr() + 1)`, then `q[0]`)
  ran as defined on the checked machine and lupin, while release emitted
  `load i32 … align 4` (UB to LLVM) and native marked the access
  aligned. It is now §7 row **L4** (`[mem.unsafe.raw.4]`; licenses O12:
  every ordinary raw access at its natural alignment, no check, no
  split). The checked machine reports it as it reports L3, before the
  provenance rows and only for a pointer with an allocation; a dangling
  pointer stays L2. `[mem.unsafe.raw.1]` names it.
- **Native and release do not move.** No backend file changed; hosted
  binaries are byte-identical. `raw_align_ir.rs` holds the release IR to
  `align 2/4/8` on every raw access and `align 1` on a packed field.
- **Packed fields stay defined.** Through a raw element it is the
  struct's alignment that is asked: 1 for `#[repr(c, packed)]`, so a
  `u64` field at offset 2 is an ordinary read (`[abi.layout.packed]`);
  a plain `#[repr(c)]` element at a misaligned address is L4. The
  checked machine now reads a scalar field through a raw element
  (`g[i].f`) at the clause layout; it refused that read by name before.
- Read misaligned data byte by byte, or through a packed struct.
  lupin 0.1.46 still runs the access as defined; its half is
  wolf-interp#192, which 0.1.47 ships (the pairing, above). Anchors
  586 → 587.

### Module strings hold their value, a parallel build cannot share a scratch directory, and the pool no longer starves (s210, #585, #583, #570, #584)

- **A silent wrong answer is fixed (#585).** A module `let` or `const`
  initialized by a string literal printed the literal's source on all
  three compiler machines. A `"""` string kept its quotes and indentation,
  `r"a\nb"` kept its fence, and `\u{41}`, `\x42` and `{{`/`}}` printed
  raw. The comptime engine decoded literals with its own unquoter, which
  knew none of them. It now cooks every literal through the lowering's
  cooker (`strlit::cook`), so a module string and the same literal in a
  fn are the same bytes, and a comptime fn that returns a `"""` string
  folds to its value. lupin was right throughout. Corpus row
  `memory/static_str_literals.lu`; `strlit_parity` holds the two cookers
  together. The untyped module `let` that wolf-book ch06 uses is still
  declined by name.
- **The release tier's scratch directory is per unit (#583).** Units
  compile in parallel, and two could share `wolf-llvm-<pid>-<nanos>`,
  which made an ICE on macOS. Each finish now owns a directory named by
  pid and a process sequence, and creation steps over any existing name.
  `parallel_finish` drives 16 units for 64 rounds at a barrier.
- **The runtime pool no longer strands a spawned task (#570).** An idle
  extra worker retired while the pool's running count, which includes
  blocked workers, exceeded the target. Then nothing ran the next task.
  An extra now retires only out of surplus unblocked workers, by CAS.
  `pool_retire` was red at 0.2.23 on 7 of 9 runs across 1, 4 and 16 cpus.
- **One fault, one record (#584).** A failed comptime `assert` under a
  module `const` reported E0710 twice. The module-state loop now skips an
  initializer whose call site already faulted.

## 0.2.23 — 2026-10-04

THE TWENTY-THIRD. A wolf kernel can now own its machine. Integers and
pointers convert both ways, and `*p` reads and writes a raw pointer
(`[mem.prov.expose]`). Foreign memory is the platform's on the
freestanding target. `p.read_volatile()` and `p.write_volatile(v)` are
one access each, never elided (E1307). `#[repr(c, packed)]` and
`#[repr(c, align(N))]` lay out as C does, and `size_of`, `align_of` and
`offset_of` answer at comptime (E0819, E0820). Module `const`, `let` and
`var` compile as image data, a `var` is raw-tier, and
`#[section(".name")]` and `extern "c" let` place code and name linker
symbols (E0705, E0821). `[abi.interrupt]` says how an interrupt reaches
an `export fn` through an assembly trampoline. Ruling #34 makes an
else-less `if` at a fallible fn's tail discard its raise, warned, on
every machine. The pairing moves to lupin 0.1.46, whose spec pin moves to
v0.2.22: it closes eight of the lupin gaps the PAX lanes filed. Six lanes
and 187 commits (kw06, kw07, s208, kw08, kw09, kw10), then r28's 13: 200
since v0.2.22.

### Read this before you bump the pin

**Three programs gave a wrong answer with no diagnostic on 0.2.22.**
Each now answers the ruled bytes on its lane:

- `fn g(c: bool) -> () ! {bad} { if c { boom() } }` handed `boom`'s raise
  to `g`'s caller on every machine, with no W0601 (ruling #34). It is the
  warned discard now, and `g` returns `()`. An else-if chain with no
  final `else` at that tail split the machines (native and release
  discarded, checked raised); all discard now.
- The checked machine read a signed raw pointee unsigned: a `*i32`
  holding -6 read 4294967290 (#561).
- The checked machine gave `*T as u8` the whole address under a `u8`
  (`300 as *u8 as u8` exited 44). It traps `overflow` now.

**Programs that compiled are now refused.** Every read or write of a
module `var` outside `unsafe` is E1301. A module initializer that reads a
`var`, calls run-time code or needs its own value is E0705. A volatile
access through a pointee that is not one fixed-width integer or `byte`
is E1307. A packed field that is lent is E0819. A representation that
cannot be laid out is E0820. A non-pointer `extern "c" let` is E0821.
A bare tag as the then-block's tail of an else-less `if` at a fallible
fn's tail (`if c { bad }`) is `unsupported` by name; write `return bad`.

**Programs that were refused now compile.** `int as *T`, `*T as int`,
`addr`, `with_addr`, `expose`, `with_exposed` and `is_null` on native and
release, and prefix `*p` on all three tiers (it was `unsupported` or
refused at resolve). Module `const`, `let` and `var`, which were
`unsupported` everywhere. `size_of(T)` for a `#[repr(c)]` struct, which
was E0708. `extern "c" let`, which was E0201 at parse. The volatile
methods. Hosted programs that use none of this build byte-for-byte as
before.

### The pairing: lupin 0.1.46

`PAIRING` names **lupin 0.1.46 at pin `8e36bc1`** (wolf-interp release
403040421; the linux x86-64 archive's sha256 `d13a0379…`), whose spec pin
is this compiler's v0.2.22 tag. It carries is70 (wolffe-lang/wolf-interp
#174, #175, #178, #179's `errdefer` half, #180, #181, #182, #184) and
s208's mirror of ruling #34 (#179's `if` half). The ritual (#87, with
#281's control against the 0.1.45 archive, same corpus, same release
`wolf`) **moved 11 ledger counts on the checked tier and 14 on native**:

- **Toward agreement.** `grammar/cfg_target_arch.lu` and
  `cfg_target_freestanding.lu` (Verdict → agreement: lupin reads `cfg`,
  #174); `memory/prov_addr_with_addr.lu` and `prov_expose_round_trip.lu`
  (unsupported → agreement) and `prov_narrow_cast_trap.lu` (Verdict →
  agreement), #184; `memory/raw_ptr_private_sig.lu` and
  `raw_ptr_mut_param.lu` (Verdict → agreement, #181);
  `memory/region_str_view_for_return.lu` (completeness → agreement, #178);
  `memory/extern_c_outside_unsafe.lu` (unsupported + completeness →
  completeness).
- **Into a hard divergence, ruled**: `comptime/layout_query_repr_c.lu` and
  `memory/packed_fields_at_offset_of.lu` on both tiers, and
  `memory/raw_repr_packed_layout.lu` and `raw_repr_align_layout.lu` on
  native (the checked machine declines those two, so they sit below its
  ledger). lupin's closed attribute set refuses `repr(c, packed)` and
  `repr(c, align(N))` by name, E0817, where 0.1.45 declined later. Its
  mirror of kw08 is still open (#188).
- `membrane/extern_libc.lu` moves Verdict → unsupported on native: lupin
  declines the bodyless C declaration by name where 0.1.45 said E1302.

Below the ledger on both tiers, B's verdict or bytes moved:
- the eleven `grammar/attr_*`, `cfg_*` and `extern_abi_interrupt.lu` rows
  (`exit(0)` → E0817/E0818);
- the five `rows/negative/first_*` codes (#175);
- `errdefer_infallible.lu` (→ E0607, #179);
- `unit_discard_if_value.lu` and the four `unit_discard_tail_if*` rows
  (ruling #34);
- `prov_narrow_cast.lu`'s stdout;
- `packed_field_lend.lu` (→ E0817).

The counts:
- **Checked:** 585 → 593 agreements, completeness 233 → 232, hard
  divergences 22 → 19, soundness 0.
- **Native:** 627 → 635, 233 → 232, 28 → 26, soundness 1 (still
  `memory/unsafe_ub_uaf.lu`).

**Dropped, 37 cases.** The gates no longer pin lupin's answer for them:
- `attr_closed_set_lanes.rs` 12 (#174)
- `c_membrane_lanes.rs` 4 (#181)
- `first_diagnostic_lanes.rs` 7 (#175)
- `region_view_for_lanes.rs`'s `PRE_RETURN_TRAP_LUPIN` 1 (#178)
- `unit_discard_lanes.rs` 6: the else-less-`if` value pin, and s208's five `LUPIN_179` cases (#179)
- `int_ptr_lanes.rs` 7 (#184, #182)

Against the published 0.1.45, exactly those thirty-seven tests go red.

**Kept, 23 cases, each carried to 0.1.46** and each naming its open lupin issue:
- `attr_closed_set_lanes.rs`'s control `attr_implemented_set.lu` 1 (lupin declines its `comptime fn`)
- `volatile_lanes.rs` 6 (#185). The L3 row now names `read_volatile` where 0.1.45 stopped at `with_addr`.
- `repr_layout_lanes.rs` 9 (#188)
- `static_lanes.rs` 7 (#190)

**Kept, verdict changed, 7 of those** (ruled at r28), each now E0817, refused by name:
- `repr_layout`'s `layout_query_repr_c.lu`, `packed_fields_at_offset_of.lu`, `packed_field_lend.lu`, `attr_repr_unlayable.lu`, `raw_repr_packed_layout.lu` and `raw_repr_align_layout.lu`
- `static`'s hosted `#[section]` program

None is widened.

### Interrupts (kw10, ruling #31 K7 = B)

- **`[abi.interrupt]`**: wolf has no interrupt calling convention, and
  the clause now says how an interrupt reaches wolf instead. An
  assembly trampoline the root `wolf.pkg` lists pushes a zero for the
  vectors whose CPU pushes no error code (every vector but 8, 10-14,
  17, 21, 29 and 30), pushes the vector, saves the fifteen general
  registers and calls an `export fn h(f: *Frame)` under the C
  convention; the frame is a `#[repr(c)]` struct of 22 `u64`s
  (`size_of` 176, `rip` at 136); when `h` returns, the trampoline
  restores the registers and returns with `iretq`. The IDT is data the
  program builds in a `.bss` table assembly reserves and wolf names
  with `extern "c" let`. No compiler change: kw02's `export fn`, kw05's
  `asm`, kw08's layout and kw09's `extern "c" let` already compose into
  it. Witness: `freestanding_interrupt.rs`, an image QEMU boots (`-kernel`,
  the PVH entry) whose IDT wolf fills; `int3` returns, `ud2` and a `#GP`
  with error code 0x1234 are resumed past, on both tiers. The linux CI
  job installs QEMU for it and sets `WOLF_INTERRUPT_REQUIRE_QEMU=1`.
  Anchors 585 → 586.
- Filed on the way: wolf-lang#577 (a store to a field of a raw element,
  `f[0].rip = v`, is refused on native, release and checked: a handler
  writes the word at `offset_of(Frame, rip) / 8` instead) and #579 (a
  `pub const` read from another module is refused on native, release
  and checked).

### Module state and linker control (kw09, ruling #31 K11 = A and K6)

- **Module `const`, `let` and `var` compile.** Before, every one was
  `unsupported` on native and release ("item-initializer lowering
  (globals)") and on the checked machine (wolf-lang#560, #529). Every
  module initializer is now evaluated at compile time
  (`[mem.static.3]`): the value is part of the image, so there is no
  initialization order to observe; an initializer that reads a `var`,
  calls into run-time code or needs its own value is **E0705**. A
  `const` is its value (no storage); a `let` is read-only data in
  `.rodata`; a `var` is writable data in `.data`, or `.bss` when its
  initial bytes are all zero. Module state holds the integers, `byte`,
  `bool` and the floats (and `str` in a `const` or `let`); any other
  type is refused by name (`module state of a type that is not static
  data`).
- **A `var` is raw-tier** (`[mem.static.2]`, K11 = A): every read and
  write of it outside `unsafe` is E1301, one per site; `let` and
  `const` read freely. The compiled tiers read and write it through
  the raw-buffer region, so another function's write is never cached
  across a call. Each module item is defined in exactly one object of
  a build, global, under its mangled name, and imported by the others.
- **`#[section(".name")]`** (`[abi.link.section]`, K6) places a
  function or a module `let`/`var` in that object section, on both
  tiers, hosted (ELF) and `x86_64-unknown-none`. On a `const` it is
  E0817 (no storage), as is Rust's `link_section`. Mach-O and COFF
  hosts refuse it by name (`section placement on a Mach-O target`).
  The native tier gives each function its own section only in an
  object that places one, and renames the placed one in the ELF.
- **`extern "c" let NAME: *T`** (`[abi.link.extern]`) names a symbol
  the link defines — a linker-script symbol, an assembly label — and
  its value is the symbol's address. A non-pointer type, an
  initializer, `var`/`const`, or a body position is the new **E0821**.
  `[abi.link.script]`: the consuming build links the image.
- **The machines.** The checked machine runs module state as ordinary
  memory started at its comptime value, and refuses `#[section]`
  (`section placement`) and `extern "c" let` (`a link-time symbol`) by
  name. lupin 0.1.45 parts on every refusal and overflows its stack on
  an initializer cycle; its half is wolf-interp#190, pinned by version
  in `static_lanes.rs`. `static_sections.rs` reads every placement
  from the objects and links a kernel with its script on both tiers.
- Hosted programs with no module state compile byte-for-byte as before.
  Anchors 577 → 585.
### Exact layout: `packed`, `align(N)`, `size_of` / `align_of` / `offset_of` (kw08, ruling #31 K4)

- **`#[repr(c, packed)]` and `#[repr(c, align(N))]`.** Both were E0817
  ("not implemented yet"). A packed struct is the C layout with every
  field at the next byte: alignment 1, no padding
  (`[abi.layout.packed]`); an aligned one has its alignment raised to
  at least `N` and its size rounded to it (`[abi.layout.align]`). They
  hold through a raw pointer at any depth — a packed struct as a field,
  an aligned one inside a packed one — and the stride of `p[i]` is the
  C `sizeof`. A packed field is loaded and stored with the alignment
  its offset guarantees (`align 1` on release, no `aligned` flag on
  native), never its natural one.
- **The queries.** `size_of(T)` answers for every `#[repr(c)]` struct
  (it was E0708 for any struct), and `align_of(T)` and
  `offset_of(T, field)` are new; all three answer at comptime from the
  clause's layout, for scalars and `#[repr(c)]` structs
  (`[abi.layout.query]`). A native-layout type stays E0708; an
  `offset_of` field the struct lacks is E0403.
- **Proven against C.** `repr_c_raw_layout.rs` holds six layouts to
  gcc and clang in both directions on native and release, and wolf's
  three queries to C's `sizeof`, `_Alignof` and `offsetof`.
- **New codes.** **E0819**: a packed field is lent (a `mut` argument
  or receiver, or an aggregate passed `read`) — its address may be
  misaligned; copy it out instead. **E0820**: a representation that
  cannot be laid out — `align(N)` with `N` not a power of two up to
  2^28, `packed` or `align` without `c`, both at once, on a generic
  struct, named twice, or an aligned struct held inside a packed one
  (the C compilers disagree there: MSVC-ABI clang keeps the alignment,
  SysV gcc and clang do not). `repr(transparent)` and
  `repr(packed(N))` stay E0817.
- **The membrane.** A packed or aligned struct by value at the C
  membrane is refused by name on the compiling tiers (its psABI
  passing is not lowered); a pointer to one crosses.
- The checked machine still refuses a whole-aggregate raw store or
  load by name; a packed field read or written as a scalar at its
  `offset_of` runs on every machine. lupin 0.1.45 has neither the
  attributes nor the queries: its half is wolf-interp#188, pinned by
  version in `repr_layout_lanes.rs`. Hosted programs that use none of
  this build byte-for-byte as before. Anchors 574 → 577.

### Volatile access: `p.read_volatile()` / `p.write_volatile(v)` (kw07, ruling #31 K3 = B)

- **Two methods on `*T`.** `p.read_volatile()` yields the `T` at `p`;
  `p.write_volatile(v)` stores `v` there. Before, no volatile spelling
  existed and `[mem.unsafe.raw.3]` promised "std intrinsics" nothing
  defined; every machine answered `unsupported`. The pointee is one of
  `u8`…`u64`, `i8`…`i64` or `byte` (`[mem.unsafe.volatile.1]`); any
  other — `int`, `uint`, `bool`, a float, an aggregate — is the new
  **E1307**. Both are raw-tier accesses: E1301 outside `unsafe`.
- **One access per call, in the object.** Each call is exactly one
  load or store of the pointee's width, aligned, never elided, split,
  merged or reordered against another volatile access
  (`[mem.unsafe.volatile.2]`), on native and release, hosted and
  `x86_64-unknown-none`. The access lowers to a width-named intrinsic
  call on the raw-buffer token, so the WIR mid-end treats it as an
  opaque effect, and both backends expand it in place to one
  instruction (LLVM `load volatile`/`store volatile`); nothing is
  imported. `volatile_disasm.rs` counts the instructions, with
  release-tier controls that prove the ordinary spelling of each shape
  IS folded (a hundred-store loop becomes one store; a poll's load is
  hoisted).
- **The machines.** On an allocation a volatile access is an ordinary
  access of the width (`[mem.unsafe.volatile.3]`): the checked machine
  runs it under the same rows as `p[0]`, and a misaligned address is
  the new §7 row **L3** (licenses O11: one aligned access, no check).
  lupin 0.1.45 has no volatile surface; its half is wolf-interp#185,
  pinned by version in `volatile_lanes.rs`.
- Hosted programs that use neither method compile byte-for-byte as
  before. Anchors 570 → 574.
### Integers and pointers (kw06, ruling #31 K9(a) and K9(c))

- **The casts and the provenance methods lower.** `int as *T`, `*T as
  int`, `p.addr()`, `p.with_addr(a)`, `p.expose()`, `p.with_exposed(a)`
  and `p.is_null()` were `unsupported` on native and release ("raw casts
  that change the machine shape", "a method call without an elaborated
  impl"); now they lower through two WIR ops, `ptr.to_int` and
  `ptr.from_int` (`ptrtoint`/`inttoptr` on release, the identity on
  native), and `with_addr` is an offset from the receiver, keeping its
  provenance. `N as *T` widens `N` by its own signedness; `*T as N` is
  the address's bits for a 64-bit `N` and the address as `uint` under
  `[type.numlit.cast.narrow]` for a narrower one (`[mem.prov.expose]`).
- **Prefix `*p`.** A read, a write and a compound write through a raw
  pointer, `p[0]` by another name, on all three tiers (it was
  `unsupported` on every one, and `*p = v` was refused at resolve).
  Outside `unsafe` it is E1301; on a non-pointer, E0409.
- **Foreign memory, `[mem.prov.device]`.** An access no allocation owns
  is UB row L2 on a hosted target, as before; on the freestanding
  target it is the platform's — `0xb8000 as *u16` stores to `0xb8000` —
  and the checked machine and lupin refuse a freestanding program by
  name, never answering UB.
- **Fixed on the way.** The checked machine read a signed raw pointee
  unsigned (`*i32` holding -6 read 4294967290; wolf-lang#561), and gave
  `*T as u8` the whole address under a `u8` type. Hosted programs that
  use none of this build byte-for-byte as before (boreutils, lobo:
  every binary identical, dev and release). Anchors 569 → 570.
- lupin 0.1.45 has no `addr`/`with_addr`/`expose`/`with_exposed`, forgets
  the address of an integer-made pointer no allocation owns, and runs
  `*p` outside `unsafe` (wolf-interp#184); pinned by version in
  `int_ptr_lanes.rs`.

### An else-less `if` at a fallible fn's tail discards its raise (s208, ruling #34, #541)

- **Ruling #34 = A: the then-block of an `if` with no `else` is a unit
  context everywhere, a fallible fn's tail included.** `fn g(c: bool)
  -> () ! {bad} { if c { boom() } }` handed `boom`'s raise to `g`'s
  caller on checked, native, release and lupin, with no W0601 — the
  one place a raise left a function with no `?` and no `return`. It is
  the warned discard now, as the same `if` one line higher already
  was, and `g` returns `()`. Before, an else-if chain with no final
  `else` at that tail split the machines (native and release
  discarded, checked and lupin raised); a value-carrying row there
  (`if c { maybe(9) }`) was E0401; a closure checked against
  `fn(bool) -> () ! {bad}` raised the same way. All three discard now.
  `?`, `return` and `else` inside the block are untouched: wolf-std's
  `move_file` (`std/fs/fs.lu`), the one such tail downstream, ends in a
  `?` and answers as before. A bare tag as that block's tail (`if c {
  bad }`) is now what it is in a statement `if`: `unsupported` by name;
  write `return bad`. Witnesses `corpus/rows/unit_discard_tail_if*.lu`,
  gate `unit_discard_lanes.rs`; lupin 0.1.45 answers the old way, pinned
  by version (wolf-interp#179).
- **Ruling #35 = share (spec text only).** `copy` of a `str` shares its
  bytes (`[mem.tier0.move.3]`), as every machine already did; s160's
  and s171's notes in `spec/02-memory-model.md` said it "materializes
  the bytes into the ambient region" and are amended. No code change.

## 0.2.22 — 2026-10-03

THE TWENTY-SECOND. wolf builds for a freestanding target:
`--target x86_64-unknown-none` emits a kernel object with no `main`
shim and no runtime, one trap hook, and everything that needs the
hosted runtime or an allocator refused by name; assembly sources
listed in `wolf.pkg` (`asm: [...]`) are assembled and linked, and on
that target a call into assembly must name the roster (E1306). The
attribute set is
closed and `cfg(target)` is evaluated against the build's target
(E0817, E0818), a `#[repr(c)]` struct through a raw pointer has the C
layout, `export fn` is a C function and a hand-declared `extern "c" fn`
is callable, `*T` crosses the C membrane, and an integer cast keeps its
value or traps (`[type.numlit.cast.narrow]`). A `for` piece cannot
outlive its region, and the checked machine discards a row nobody
consumes, as the other lanes do. Programs can seek, tell and read at an
offset, and descriptors 0, 1 and 2 are the standard streams: **the first
handle an open answers is 3**. A refusal carries one code on every lane
(`[proto.record.first]`), and the blast-radius property counts wreck
sites. The pairing moves to lupin 0.1.45, whose spec pin moves to
v0.2.21 and which heals the one parting 0.2.21 shipped pinned. Eight
lanes and 209 commits (s203, kw01, kw02, s207, kw03, s199, kw04, kw05),
then r27's 13: 222 since v0.2.21 (this line said 209 until r28 counted
from history).

### Read this before you bump the pin

**Eight programs gave a wrong answer with no diagnostic on 0.2.21.**
Each is now refused or answers the ruled bytes on its lane:

- `for w in s.words() { last = w }` inside `region scratch { }` handed
  `last` out of the region and printed from freed bytes on checked,
  native and release (#540). E1010 now; lupin trapped it.
- A `#[repr(c)]` struct stored or loaded through `*T` used a packed
  layout on native and release, `{u8, u32, u8}` at offsets 0 1 5 where
  C has 0 4 8 (#523). The C layout now.
- `p[0] += 5` through a raw pointer stored 5 on native and release; the
  checked machine sized a `*i64` element by the wrong type (#542).
- `-1 as u64` printed `-1` and `128 as u8 as i8` printed `-128` on
  native and release (#533). Both trap `overflow` now, as the checked
  machine and lupin did. Spell the bits `v as wrapping[u64]`.
- `let v = if c { maybe(n) }` bound `3` or `none` on the checked machine,
  where the value is `()`, and a discarded raising statement left
  `main` with `error: bad` there (#541).
- A `mut p: *u8` reassigned in a private callee never reached the
  caller on the checked machine (`2` where native printed `42`), and a
  write through a pointer argument was UB row P2 there (kw02).
- `#[repr(packed)]` was laid out unpacked, `#[section(…)]` landed in
  `.text`, `#[noalloc]` verified nothing, and `#[cfg(target = "…")]` was
  ignored, so target-gated code compiled for every target (#519).
- `extern "x86-interrupt" fn` compiled as an ordinary function
  returning with `ret` (#524).

**Programs that compiled are now refused.** An attribute wolf does not
know or does not implement yet (E0817, naming the lane that owns it);
any ABI string but `"c"` (E0818); a `cfg` predicate other than `target`;
a `for` piece, a `List[str]` element or `copy s` of a region-built
`str` held past its region (E1010). A sign-changing or narrowing cast
that does not fit now traps at run time where native and release
reinterpreted the bits. In the downstreams, `corpus/comptime.lu` drops
its `#[noalloc]`, and the book's `#[noalloc]` exercises become E0817 at
its next pin. wolf-std, boreutils and lobo use only implemented
attributes.

**Programs that were refused now compile.** `wolf build --target
x86_64-unknown-none --emit=obj` (it was an unknown flag); `asm:` in
the root `wolf.pkg` (it was E1502, an unknown key). Every narrowing integer
cast on native and release; `export fn` (a C symbol under the target's
C plan); a call to a hand-declared `extern "c" fn` inside `unsafe`; a
raw pointer in a module-private signature or across the C membrane
(`pub` signatures still E1302); `return boom()` in a unit fn, which
stopped native and release with an internal error.

**Handles moved.** The first handle `fs_open` answers is 3 on every
machine (it was 0 on the compiler and 1 on lupin). `fs_fstat(0)`,
`(1)` and `(2)` describe the standard streams instead of answering
`io`.

### The pairing: lupin 0.1.45

`PAIRING` names **lupin 0.1.45 at pin `dfcc2f1`** (wolf-interp release
402670966; the linux x86-64 archive's sha256 `907cfb1a…`), whose spec pin is
this compiler's v0.2.21 tag. It carries is69 (wolffe-lang/wolf-interp#176),
is68 (#126, #103 row 1, #125, #138, #169) and the mirror of s199's
`fs_seek`/`fs_tell`/`fs_read_at` and standard streams. The ritual (#87,
with #281's control against the 0.1.44 archive, same corpus, same
release `wolf`) **moved 16 ledger counts on the checked tier and 17 on
native**, each toward agreement but one:

- **Healed.** `rows/eu_bind_empty_row_handled.lu` (Verdict → agreement):
  0.2.21's one pinned parting, #176. The two s199 rows `fs/seek_tell.lu`
  and `fs/read_at.lu` (unsupported → agreement). is68's
  `memory/region_str_view_return.lu` and the nine
  `region_str_view_for_*.lu` rows (completeness → agreement: lupin traps
  `region-fault` where the compiler refuses E1010). The #169 rows
  `nested_fn_mut_moveout.lu` (→ agreement) and `nested_fn_mut_omitted.lu`
  (→ completeness), and `rows/unit_discard_unit_main.lu` (Verdict → a
  warning-only Diag).
- **One move into a hard divergence**: `grammar/cfg_target_freestanding.lu`
  (unsupported → Verdict). lupin still reads no `cfg` (#174) and keeps
  the code the freestanding target gates; is68's wider scalar pass now
  types that code and refuses it, `fail(E0401)`, where 0.1.44 declined.
- `memory/nested_fn_mut_param.lu` moves to agreement on native only: the
  checked machine declines the row, so the move sits below its ledger.

Below the ledger on both tiers: `rows/unit_discard_if_value.lu` (lupin's
stdout `3 none` → `() none`, #179) and `strings/trim_cutset_refused.lu`
(`exit(0)` → `fail(E0402)`, #125). Checked: 564 → 577 agreements, completeness 233 → 223, hard
divergences 17, soundness 0; native: 605 → 619, 233 → 223, 21, soundness 1 (still
`memory/unsafe_ub_uaf.lu`, the program's own use-after-free read on the
compiled lane).

The gates that pinned lupin's answer: **dropped, 17 cases** — the #176
pin (`fallible_bind_empty_row_lanes.rs`), `fs_std_lanes.rs`'s
`PRE_MIRROR_LUPIN` (six cases), `region_view_for_lanes.rs`'s
`PRE_VIEW_TRAP_LUPIN` (six, is68's #126),
`unit_discard_lanes.rs`'s unit-`main` pin (#103), and
`nested_fn_modes_lanes.rs`'s three moded nested-fn rows, whose oracle
said `unsupported` for every version and now asks for the ruled answers
(is68's #169). Against the published 0.1.44 exactly those seventeen go
red. **Kept, 26 cases, each carried to
0.1.45 with 0.1.44's answer**, each naming its open lupin issue:
`attr_closed_set_lanes.rs` 13 (wolf-interp#174), `c_membrane_lanes.rs` 4
(#181), `first_diagnostic_lanes.rs` 7 (#175), `region_view_for_lanes.rs`'s
`PRE_RETURN_TRAP_LUPIN` 1 (#178) and `unit_discard_lanes.rs`'s else-less
`if` 1 (#179; is68's `() none` only). **Kept, verdict changed, one case**:
`cfg_target_freestanding.lu` is pinned at 0.1.45's `fail(E0401)` (#174,
above). None is widened.

### Linked assembly: `asm` in `wolf.pkg` (kw05, ruling #31 K2 = C)

- **Assembly sources in the manifest.** `asm: ["boot/io.S"]` in the
  root `wolf.pkg` was an unknown key (E1502). Now each listed source is
  assembled for the build's target — on `x86_64-unknown-none` by the
  release tier's clang with `--target=x86_64-unknown-none-elf`, from
  any host, checked ELF x86-64; hosted, by the link's `cc` — and handed
  to the link, or written beside `--emit=obj -o K.o` as
  `K.asm-<stem>.o` for the boot code's link (`[abi.asm.link]`). A
  routine is called through a bodyless `extern "c" fn` inside `unsafe`.
  A dependency's manifest that lists `asm` is refused: an assembly
  source is not part of a package's content address.
- **The assembly roster, E1306.** The roster is the `.globl` names of
  the listed sources. On the freestanding target a package that lists
  assembly calls through `extern "c"` only into its roster or the hooks;
  any other call is E1306 naming the routine (`[abi.asm.roster]`).
- **The machines.** The checked machine refuses a call into a listed
  routine as `` a call into assembly `NAME` `` (`[abi.asm.machines]`);
  lupin already refuses a call to a bodyless declaration by name. The
  inline `asm` block stays `unsupported` (`[abi.asm.inline]`, kw13).
- A build that lists no `asm` links byte-for-byte as before (boreutils,
  lobo: every binary identical, dev and release). The manifest schema
  note now names `target` and `asm`. Anchors 564 → 569.

### The freestanding target: `--target x86_64-unknown-none` (kw04, ruling #31 K1, K8(a), K10)

- **A kernel object, with no `main` shim and no runtime.** `--target`
  was an unknown flag, and every object carried the C-entry `main` shim
  and imported the runtime's trap reporters. Now `wolf build --target
  x86_64-unknown-none --emit=obj` (or `target: "x86_64-unknown-none"`
  in the root `wolf.pkg`) builds an object for a boot stub to link, on
  both tiers and from any host: the entry is an `export fn`, `main` is
  an ordinary function, and the object imports only the hook list —
  `wolf_trap` and `memcpy`/`memmove`/`memset`/`memcmp` — plus the
  program's own `extern "c"` declarations (`[abi.target]`,
  `[abi.target.none]`, `[abi.target.entry]`, `[abi.target.none.hooks]`).
- **One trap hook, with the site, on both tiers.** On the target every
  trap calls `wolf_trap(kind, file, file_len, line, col)`, which the
  program supplies and which must not return (`ud2` follows the call);
  the native tier had called `__wolf_rt_trap_at` and the release tier
  `__wolf_rt_trap` or `__wolf_rt_trap_at`. On a hosted target
  `wolf_trap` is the runtime's own report-and-exit: a program that
  imports it links it as a link-time alias of `__wolf_rt_trap_at`.
  Compiled into `libwolf_rt.a` it moved every hosted binary (debug info
  and LLVM's `.llvm.<hash>` names), so hosted binaries that do not
  import it are byte-identical to 0.2.21's.
- **Kernel code generation.** The release tier emits
  `x86_64-unknown-none-elf` with `noredzone`, `"frame-pointer"="all"`
  and no MMX/SSE/AVX (`+soft-float`) on every function; the native
  tier emits ELF under the SysV plan with frame pointers, and both are
  small PIC (`[abi.target.none.codegen]`). The gate disassembles every
  object (no `xmm`/`ymm`, no store below `%rsp`, `ud2` after every hook
  call) and links both tiers with an assembly boot stub — no libc, no
  runtime — and runs them.
- **Refused by name, before any backend:** everything that needs the
  hosted runtime (`` `print` needs the hosted runtime (target
  x86_64-unknown-none) ``, `env_args`, a `str` comparison, `spawn`, …),
  every allocating construct (`List`, `Map`, `Pool`, string
  interpolation, a capturing closure, `region`; `[abi.target.none.alloc]`)
  and every float. `--emit=bin`, `wolf run`, `--checked` and
  `--profile-gen` refuse the target by name; `conform-run --target
  x86_64-unknown-none` is `unsupported` on every rung, never run.
- `cfg(target = "…")` reads the build's target; anchors 558 → 564.

### The narrowing cast: keep the value or trap (kw03, #533, ruling #31 K12)

- **An integer cast keeps the value, and traps `overflow` when the
  target cannot hold it**, on every machine (`[type.numlit.cast.narrow]`).
  Native and release refused every cast to a narrower integer
  ("narrowing numeric casts (range-check semantics)"); `b as int as u8`
  now compiles, with one compare and a branch to the trap.
- **A change of sign is a narrowing cast.** Native and release ran
  `-1 as u64`, `u32 as i32` and `i8 as u16` by reinterpreting the bits
  (`-1 as u64` printed `-1`, `128 as u8 as i8` printed `-128`), a
  silent wrong answer where the checked machine and lupin trap. They
  trap now. A program that wanted the bits spells it
  `v as wrapping[u64]`.
- **Truncation is `v as wrapping[T] as T`.** The checked machine
  range-checked a signed wrapping value's stored mask instead of its
  value, so `200 as wrapping[i8] as i8` trapped there and printed `-56`
  everywhere else; it keeps `-56` now. It also refuses by name a cast
  that would hold a `u64` above `i64::MAX`, which it cannot represent
  (#551), instead of trapping it.

### The export seam: C calls wolf, wolf calls C, `*T` crosses (kw02, #513, #521, #514, rulings #31 K9(b) and #32 R1)

- **`export fn` is a C function.** It was lowered as a wolf function:
  a mangled symbol (`_Wkw_x$9716…`), the wolf convention (lc00 measured
  a segfault passing a `#[repr(c)]` struct by value from C), and
  dropped by `--release`. Nothing above WIR set the flag the backends
  already honoured. Now `export fn f` (and `extern "c" fn f` with a
  body) is the global symbol `f` under the target's C plan on both
  tiers, kept whether or not wolf calls it, also from a child module
  (`[abi.c.export]`); a narrow or `bool` result is widened by its
  signedness for the C caller (WIR `export sext fn` / `export zext fn`).
  A wolf call into an export crosses under the same plan, in or out of
  its object.
- **A hand-declared `extern "c" fn` is callable.** A call was refused
  at codegen ("hand-declared externs beyond the modelled c.* set").
  It now lowers to the plain symbol with the declared prototype, inside
  `unsafe` (E1301, as `c.malloc`); a narrow integer or `bool` argument
  is widened to 32 bits by its signedness (WIR `sext`/`zext`, cranelift
  extension, LLVM `signext`/`zeroext`): before that, clang-compiled
  callees read `i8 -5` and `i16 -300` zero-extended (`[abi.c.import]`).
  `import c` beyond the modelled five is still the header importer's
  (#521's other half).
- **`*T` crosses the membrane, and a module-private signature.** E1302
  refused a raw pointer in every signature. Ruled K9(b) = B: a
  module-private fn and either side of the C membrane may carry `*T`;
  `pub` fns, methods and exported types may not (`[mem.unsafe.sig]`).
  `corpus/memory/unsafe_sig.lu`'s `peek` became `pub` to keep its E1302.
- **The checked machine** refuses a call into hand-declared C by name,
  and passes a raw pointer argument as a copy (by value) or as the
  caller's variable (`mut`). It had frozen the pointee at the call, so
  a private `poke(p)` writing through `p` was UB row P2 there and ran
  natively; a `mut p: *u8` reassigned in the callee never reached the
  caller (it printed `2` where native printed `42`).
- **What does not cross is refused by name**, not guessed: `str`,
  containers, a struct without `#[repr(c)]`, an error union, a
  `mut`/`take` parameter, a generic or `comptime` export, an export as
  a fn value, two functions under one C symbol (E1201 is not built;
  `[abi.c.types]`).
- Witnesses: `c_membrane_link.rs` builds two fixtures `--emit=obj` on
  both tiers and links them with their C half: C calls 21 exports
  (eight mixed-width integers, nine floats, every `#[repr(c)]` class by
  value both ways, `*Big` read and written back, a `*u8` string) and
  checks 36 values; wolf calls 14 C functions, two of which write
  through wolf pointers. Run under gcc 16.2.1 and clang 23.1.1 on
  kasumi. `c_membrane_lanes.rs` asserts seven corpus rows on the
  checked, native and release lanes and lupin, and seven refusals by
  name; lupin 0.1.44 is pinned pre-mirror where it parts
  (wolf-interp#181).

### Which code a refusal carries: the first diagnostic (s203, #377, ruling #28)

- **One program, one code, on every lane.** `conform-run` stopped at
  lex whenever lex had an error, wherever it sat, and otherwise took
  the parser's first error in emission order; `wolf run`/`build`
  sorted lex, parse and resolve diagnostics together by (offset, span
  end, code). A file could be E0102 on its record and E0203 on its
  screen (38 of wl21's 99 three-way rejections), or E0207 and E0301.
- Ruled by the maintainer (#28) and written as `[proto.record.first]`:
  the earliest byte offset is first; at one offset the earlier phase
  (lex, parse, resolve, then later). Lex and parse are one rung for the
  question, so a parse error before the first lex error is the refusal;
  `wolf run`/`build` stop at the entry's front end as the ladder does.
  `wolf_diag::first_error` is the one place that picks. A boundary
  (E0202) sits at its opener, so an unclosed `(` before an unterminated
  string inside it is first.
- Witnesses: six rows `corpus/rows/negative/first_*.lu` and
  `corpus/rows/first_list_literal_sum.lu` — #377's classes and the
  parse-before-lex, boundary-before-lex and parse-before-resolve shapes
  — and two driver fixtures, `tests/fixtures/first_diagnostic/` (#377's
  error-row class and kw00's top-level `union`, which no formatter can
  lay out); `first_diagnostic_lanes.rs` asserts the
  checked, native and release records and both `wolf run` screens, and
  pins lupin 0.1.43/0.1.44 by version where it parts. Over 90
  candidates the teacher regenerated for #377's four refusal classes
  (8 byte-identical to wl21's), the four wolfgang screens and records
  agree on every one (57 of 90 at trunk); lupin parts where #175 says.
  The two
  `exit(0)`-vs-E0201 programs were lupin 0.1.36 without list literals
  (closed at 0.1.37), not a refusal question.

### The blast radius counts wreck sites (s203, #367, #544, ruling #27)

- **The property counts wreck sites, not diagnostics**: two added
  diagnostics are separate sites when the mutated tree holds an intact
  declaration, statement or arm wholly between them; each site is held
  to the bound (3, 5 structural), which does not move. It now collects
  every violation instead of stopping at the first, which at budget 1000
  surfaced seventeen recoveries the first had hidden (and one more at
  300 once s202's rows joined the corpus); each is fixed in the parser
  and pinned by (file, iteration). #544's `struct` body read
  as a `let` is one site and was over the bound: the parser now says
  one thing per token, reports a valueless binder group once, resets
  the arm fold only on a clean arm, and stops recovery at a sibling
  declaration. Budgets 300 (the nightly's) and 1000 are green.

### The attribute set is closed, `cfg(target)` is evaluated, `"c"` is the only ABI (kw01, #519, #524, ruling #31 K13/K7)

- **An attribute nothing read compiled silently on every machine and
  meant nothing**: `#[frobnicate]`, `#[repr(packed)]` (laid out
  unpacked), `#[section(".text.boot")]` (landed in `.text`),
  `#[noalloc]` over a function that allocates, `#[thread_local]`. New
  **E0817** refuses, by name, an attribute wolf does not know, a known
  one nothing implements yet (the four performance contracts,
  `repr(packed | align(N) | transparent)`, `thread_local`, `section`,
  `link_section`, each naming the lane that owns it), and an
  implemented one where no pass reads it (`#[repr(c)]` on a function).
  The implemented set is `trusted`, `consttime`, `allow`, `index`,
  `budget`, `repr(c)` and `cfg(target = "…")`
  (`[gram.item.attr.set]`).
- **`#[cfg(target = "S")]` was ignored**, so target-gated code was
  compiled for every target. It now keeps its item, statement or field
  when `S` names the build's target triple or its architecture and
  drops it before resolution otherwise; a string that names no target,
  or another predicate (`cfg(unix)`), is E0817 rather than "false"
  (`[gram.item.attr.cfg]`). The build's target is the host's until
  `--target` lands.
- **`extern "x86-interrupt" fn` compiled as an ordinary function
  returning with `ret`.** New **E0818** refuses every ABI string but
  `"c"` (`[abi.c.seams]`).
- Downstream: `corpus/comptime.lu` drops its `#[noalloc]` (it claimed
  a verification nothing performs); `corpus/ffi.lu` gains an aarch64
  twin of its x86_64-gated `asm` so its phase is one truth on every
  host. wolf-std, boreutils and lobo use only implemented attributes;
  the book's `#[noalloc]` exercises (ch20 `ex20-5.lu`, `ex20-8.lu`,
  `EXERCISES.md` 20-8) become E0817 at its next pin.
- Witnesses: `corpus/grammar/attr_*.lu`, `cfg_*.lu`,
  `extern_abi_interrupt.lu`; `attr_closed_set_lanes.rs` asserts every
  wolfgang lane and pins lupin 0.1.43 by version (it reads no
  attribute; the mirror is wolf-interp#174).

### A `#[repr(c)]` struct through a raw pointer has the C layout (kw01, #523, ruling #31 K4)

- **A silent wrong answer at the C membrane on native and release**:
  a struct stored or loaded through `*T` used the packed spill layout,
  `{u8, u32, u8}` at offsets 0 1 5 with stride 6, where C has 0 4 8,
  size 12. The raw tier now loads and stores a pointee field by field
  at its psABI offsets and steps by the C `sizeof` (`[abi.layout.c]`).
  The checked machine and lupin refuse a whole-aggregate raw store by
  name, as before. `repr_c_raw_layout.rs` holds a C compiler to both
  directions (C reads what wolf wrote, wolf reads what C wrote) on
  both compiling tiers.
- **`p[i] op= v` through a raw pointer was wrong on three lanes
  (#542)**: native and release dropped the operator (`p[0] += 5`
  stored 5); the checked machine always added and sized a `*i64`
  element by the wrong type (`w[1] = 9; w[1]` read 0). All three now
  read, apply the operator with the checked arithmetic, and store, as
  lupin already did (`corpus/memory/raw_compound_assign.lu`).

### A `for` piece cannot outlive its region; a discarded row is discarded on every lane (s207, #540, #541)

- **A piece bound by a `for` over `words()`/`lines()`/`split()` is
  refused E1010 when it outlives its region (#540).** `for w in
  s.words() { last = w }` handed `last` out of `region scratch { }`
  and printed from freed bytes on checked, native and release, while
  `s.split(",")[0]` was already E1010 and lupin traps `region-fault`.
  A loop binding now carries its iterable's sites as `xs[i]` does —
  less the `List` a view call allocates, which holds no piece's bytes,
  and a channel's own allocation — so the pieces of a parameter, a
  literal, or a `str` built outside the region still leave freely. The
  same rule refuses an element of a region-built `List[str]` held past
  the region, and `copy s` of a region-built `str`, which shares its
  bytes (`[mem.tier0.move.3]`) but carried no site. Nothing changes at
  run time. Witnesses `corpus/memory/region_str_view_for_*.lu`,
  `region_str_list_for_held.lu`, `region_str_copy_return.lu`; gate
  `region_view_for_lanes.rs`, which also runs is68's 19 view escapes.
- **The checked machine discards a row nobody consumes (#541,
  `[type.unit.discard]`).** Every lane warned W0601; native, release
  and lupin then dropped the row, but the checked machine propagated
  it: a unit fn's raising tail, a raising statement, a loop body's
  tail or an else-less `if` left `main` with `error: bad`, and `let v
  = if c { maybe(n) }` bound `3` or `none` where the value is `()`. It
  discards now at every unit-context tail and non-trailing `!T`
  statement; `?` still propagates. `return boom()` in a unit fn — "the
  operand of a `return`" is a unit context — stopped native and
  release with an internal error (`ret carries 1 value(s)`); it is the
  same discard now. Witnesses `corpus/rows/unit_discard_*.lu`, gate
  `unit_discard_lanes.rs`. An else-less `if` that is the TAIL of a
  fallible fn still raises on all four machines; the clause's list
  reads otherwise, and the ruling is asked on #541.

### Seek, tell and the positional read; `fs_fstat` on descriptors 0, 1 and 2 (s199, #426, #424)

- **Nothing in wolf named an offset**: a program could only read a
  handle forward from byte zero, so boreutils `tail -n 10` of a
  256 MiB file read all of it (111 ms against GNU's 0.2 ms). Three new
  builtins, on every machine: `fs_seek(fd, off, whence) -> int !
  {invalid, io, unseekable}` (whence 0 start, 1 current, 2 end; the new
  offset from the start), `fs_tell(fd) -> int ! {io, unseekable}` and
  `fs_read_at(fd, off, max) -> List[byte] ! {eof, invalid, io,
  unseekable}` (`pread`: the cursor does not move). **New tag
  `unseekable`**: a pipe, fifo, socket or terminal (`ESPIPE`), its own
  tag because a program falls back to reading forward. Clauses
  `[os.fs.seek]`, `[os.fs.tell]`, `[os.fs.read_at]`; the host table is
  78 lines.
- **`fs_fstat(0)`, `(1)` and `(2)` answered `io` whatever the
  descriptors were**: the handle number was an index into the runtime's
  own table and the first open was 0 (lupin's was 1). **The first handle
  an open answers is now 3** on every machine, and 0, 1 and 2 are the
  process's standard streams for `fs_fstat`, `fs_seek`, `fs_tell` and
  `fs_read_at` (`[os.fs.std]`): `fs_fstat(0)` is `kind` 0 and the size
  when standard input is a file, `kind` 2 when it is a pipe.
  `fs_read`/`fs_read_chunk`/`fs_write`/`fs_write_chunk`/`fs_close`
  still answer `io` on 0..2 (#405).
- windows: handles are served through std's seek and `seek_read` (the
  pointer put back); `GetFileType` decides what a handle IS — anything
  but a disk file is `kind` 2 for `fs_fstat` and `unseekable` for the
  offset calls, because std's metadata calls an anonymous pipe a
  regular file there (measured on the runner, CI run 37062798818).
- Witnesses: `corpus/fs/seek_tell.lu`, `corpus/fs/read_at.lu`, the
  stdin fixtures under `crates/wolf_driver/tests/fixtures/fs_std/`;
  `fs_std_lanes.rs` runs them on checked, native, release and lupin
  with standard input a file and a pipe, and pins lupin 0.1.43/0.1.44
  by version until wolf-interp's mirror ships.

## 0.2.21 — 2026-10-02

THE TWENTY-FIRST. A `match` over a fallible value compiles and runs on
every lane (the maintainer's ruling #21, `[type.row.match]`): an arm
naming a tag of the scrutinee's row handles the error half, any other
pattern handles the value half, and the match must cover both. A `?`
inside a `defer` or `errdefer` is refused (ruling #19,
`[type.row.defer]`, new E0611) where native and release overflowed the
compiler's stack, and a block's `errdefer` runs when the block's value
is an error (ruling #20). An unannotated binding's literal takes
`i32` at the binding, as lupin always answered, and three native and
release compiler crashes are fixed. Release builds are reproducible
again. The pairing moves to lupin 0.1.44, whose spec pin moves to
v0.2.20 and which carries every mirror this release's gates pinned
lupin 0.1.43's answers on by version. Four lanes and 80 commits: s195, s196, s197,
s202.

### Read this before you bump the pin

**Three programs gave a wrong answer with no diagnostic on 0.2.20.**
Each is now refused or answers the ruled bytes on every lane:

- `let b = { errdefer print("errdefer blk"); defer print("defer blk");
  look(m, "zz") } else 0` printed `defer blk` alone on native and
  release: a block's `errdefer` did not run when the block's value was
  an error (#499). Both run on every lane now, as the checked machine
  and lupin already ran them.
- `defer print("deferred {key(ok)?}")` ran on the checked machine and
  dropped the `?`'s error, so the function answered `1` (#498); native
  and release overflowed the compiler's stack on the same program.
  E0611 on every lane now.
- `let big = 5000000000` then `take_int(big)` printed `5000000001` on
  checked, native and release: a later `int` use typed the literal
  (#458). The literal takes `i32` at the binding now, so it is E0415,
  which is lupin's answer.

**Programs that compiled are now refused.** A `?` anywhere inside a
deferred expression (E0611). s196 found no `?` under a defer in any
downstream. An unannotated binding whose literal (or term of
literals) does not fit `i32` (E0415); s202 found no boreutils or lobo
build that moved, and one corpus row now spells `let i: int`.

**Programs that were refused now compile.** A `!T` bound without `?`
whose row is empty, read in a hole or a `match` (a compiler panic on
native and release since 0.2.14, #393); any `str` as an assert
message (#398); a loop holding an inlined region root on release
(#425). A `match` whose scrutinee has type `T ! {row}`: `match
look(m, k) { none => -1, v => v }` (`unsupported` on every wolfgang
lane through 0.2.20); a tag that is also a variant of the value type
is E0816, never guessed.

**Release builds are reproducible.** With the mid-end on, three
release builds of one program by one compiler gave three different
binaries on 0.2.20 (#503). They are byte-identical now.

### The pairing: lupin 0.1.44

`PAIRING` names **lupin 0.1.44 at pin `cdde128`** (wolf-interp release
402215729; the linux x86-64 archive's sha256 `e44aae06…`), whose spec
pin is this compiler's v0.2.20 tag and which carries
wolffe-lang/wolf-interp#163 and the lupin halves of rulings #21
(`[type.row.match]`, E0801, E0816) and #19 (`[type.row.defer]`,
E0611). The ritual (#87, with #281's control against the 0.1.43
archive, same corpus, same release `wolf`) **moved one ledger count on
each tier** — the parting named below — and moved **eleven files below
the ledger on each tier, the same eleven**, each to the compiler's
answer: the five
`rows/match_row_*` rows (lupin's stdout now the ruled bytes: 0.1.43
took the first bare-tag arm for a value too), the three
`rows/negative/match_row_*` refusals (`exit` → `fail(E0801)` or
`fail(E0816)`) and the three `rows/negative/try_in_*` rows (`exit(0)`
→ `fail(E0611)`). Checked: 552 → 551 agreements, hard divergences
7 → 8, completeness 207, soundness 0; native: 593 → 592, 10 → 11,
207, 1. Native's one soundness finding
is still `memory/unsafe_ub_uaf.lu` (`exit` vs `ub(mem.ub)`), the
program's own use-after-free read on the compiled lane.

The gates that pinned lupin by version drop every 0.1.43 entry: the
eight `match_fallible_lanes.rs` pins and the three
`try_under_defer_refused_lanes.rs` pins are gone. The eleven cases they
held take the ruled arm against the published 0.1.44. None is widened
and none stays pinned.

One new pin, on one case: lupin 0.1.44 refuses s202's
`rows/eu_bind_empty_row_handled.lu` with E0801, reading `f`'s inferred
`-> !int` row as open where the row is empty and every other machine
(lupin 0.1.43 included) prints `43 42 42`. That is lupin's defect
(wolffe-lang/wolf-interp#176, which a lupin lane is fixing);
`fallible_bind_empty_row_lanes.rs` pins 0.1.44's `fail(E0801)` by
version until the next pairing, and it is the one ledger count the
ritual moves, on each tier.

### A literal written at an unannotated binding takes `i32` (s202, #458)

- **A later use typed the literal, not only the binding.** `let c =
  922337203685477580` followed by `if s > c` with `s: int` ran as
  64-bit on checked, native and release, and `let big = 5000000000`
  then `take_int(big)` printed `5000000001`; lupin 0.1.43 traps both
  as outside `i32`. The literal has no context of its own
  (`[type.numlit.propagate]`), so it takes `i32` and is **E0415** when
  it does not fit; a later use still decides the binding's type
  (`let n = 0; take_int(n)` is unchanged). The same holds when the
  initializer is a `+ - * / %` term made only of literals; a term with
  a value operand keeps that operand's type. `[type.numlit.value]`
  carries the rule and corrects its 2026-09-15 measurement.
- **One corpus row depended on the old reading and moved:**
  `faults/index_origin_min_overflow.lu` (`let i = 0 -
  9223372036854775807 - 1`, used as an index) is E0415 unannotated; it
  now spells `let i: int`, and all four machines trap at the index
  shift it is about (lupin used to trap it at the literal). No
  boreutils or lobo build moved (lobo's `packed / 10000000000` is a
  value term and keeps `int`).
- Witnesses `corpus/typecheck/numlit_binding_literal*.lu` and the
  control `numlit_binding_value_later_use.lu`; gate
  `numlit_binding_literal_lanes.rs`.

### Three native and release crashes and one refusal by syntax kind (s202, #393, #398, #425)

- **A `!T` bound without `?` panicked native and release when its row
  was empty (#393).** `let x = f()` with `f() -> !int` and nothing
  for `f` to raise, then `print("{x}")`, stopped the compiler at
  `build.rs:1210` ("eu.ok on a non-eu or unit-ok value") in every
  release since 0.2.14; checked and lupin printed `42`. A
  statically-empty row lowers as the plain ok value, which `else` and
  `?` already handled; the interpolation hole and `match` now do too
  (the `match` answered `unsupported`, the unit case refused as "a
  valueless union"). Witnesses `corpus/rows/eu_bind_empty_row*.lu`,
  gate `fallible_bind_empty_row_lanes.rs`.
- **An assert message is any `str`, evaluated only on the failing path
  (#398, `[conf.trap.assert]`).** Native and release decided by syntax
  kind: a bare parameter or a field read was refused as "assert
  messages with effects", while an interpolation that calls was
  accepted and dropped without running its call. The message now
  lowers into the trap block, before the trap; a literal still emits
  nothing. Witnesses `corpus/faults/assert_msg_*.lu`, gate
  `assert_message_lanes.rs`.
- **The mid-end's loop versioner no longer clones a loop that holds a
  region token root (#425).** An inlined callee that reads a lent
  `List` mints its `region.foreign` roots at the call site, inside the
  caller's loop; versioning copied them and `--release` stopped on
  `[region-root] ... two token roots` (boreutils `cut -n`'s walk).
  Such a loop keeps its checks; it did not compile before. Witnesses
  `corpus/memory/versioned_loop_inlined_root.lu` and
  `versioned_loop_cross_module/`, gate `versioned_loop_root_lanes.rs`.

### Release IR is reproducible across builds (s195, #503)

The mid-end walked a natural loop's block set in `HashSet` order, so the versioner's guard chain came out in a per-process order and three release builds of lobo were three binaries; the set is ordered now, `release_determinism.rs` builds a fixed corpus set three times and asserts byte identity (IR and binary), and the macOS native gauntlet runs as three shards (`cargo xtask ci --shard`).

### A `?` inside a `defer` or `errdefer` is refused (s196, #498, ruling #19)

- **Native and release overflowed the compiler's stack** on
  `defer print("deferred {key(ok)?}")` and on the same `?` under an
  `errdefer` (`wolf build` and `conform-run` both aborted, rc=134, no
  record): the `?`'s error edge re-lowered the defer that contains it,
  without end. The checked machine ran the deferred `?` and dropped
  its row; lupin 0.1.42 and 0.1.43 made it the function's result.
- Ruled by the maintainer (#19 = refused) and written as
  `[type.row.defer]` beside `[type.row.else]`: a deferred expression
  runs while the function is already leaving, so a second error has
  nowhere to go. **New E0611** on every machine, at the `?`, naming the
  `defer`/`errdefer` and the fix (an `else` inside the deferred
  expression, or the fallible call moved out of it). The refusal reads
  the whole deferred expression; a `?` in a closure defined under the
  defer is the closure's own and is not refused.
- Witnesses: `corpus/rows/negative/try_in_defer.lu`,
  `try_in_errdefer.lu`, `try_in_defer_block.lu` and the control
  `corpus/rows/defer_else_handles.lu`; `try_under_defer_refused_lanes.rs`
  asserts E0611 on checked, native and release, that `wolf build`
  answers with the diagnostic and never a signal, and pinned lupin
  0.1.43's measured bytes by version until this release's pairing
  (below), where 0.1.44 refuses with the same code.

### A block's `errdefer` runs when the block's value is an error (s196, #499, ruling #20)

- **Native and release skipped a block's `errdefer`** when the block's
  trailing value was a failing call handled by an `else` outside it:
  `let b = { errdefer print("errdefer blk"); defer print("defer blk");
  look(m, "zz") } else 0` printed `defer blk` only. The checked machine
  and lupin ran both, and at a function's tail all four agreed.
- Ruled by the maintainer (#20 = it runs) and written into
  `[mem.model.order]`: a scope has left on the error path when its own
  value is an error, so a block whose value is a row carrying an error
  runs its `errdefer` as a function whose tail is an error does. The
  WIR lowering now forks a block's fall-through on the value's err bit
  — only when the block holds an `errdefer` and its value is a `!T`;
  every other block's exit is unchanged. The checked machine already
  did this.
- Witnesses: `corpus/rows/errdefer_block_row.lu` (the issue's shape),
  `errdefer_block_nested.lu` (nested blocks, and a block as a
  function's tail), `errdefer_block_loop_break.lu` (a block in a loop
  whose `else` breaks) and the control `errdefer_fn_tail_control.lu`;
  `errdefer_block_row_lanes.rs` runs each on checked, native, release
  and lupin and asserts they agree on the ruled bytes.

### `match` over a fallible value (s197, #497, ruling #21)

- **`match look(m, k) { none => -1, v => v }` compiles and runs on
  every lane.** Before this, every wolfgang lane refused a `match`
  whose scrutinee has type `T ! {row}` as `unsupported`, and lupin ran
  it with no clause to say what the arms mean. Ruled by the maintainer
  (#21, "yes, as proposed") and written as `[type.row.match]` beside
  `[type.row.else]`: an arm naming a tag of the row is a **row arm**
  (`none`, `Io(e)` with its payload bound); any other pattern is a
  **value arm** over `T`, exactly as a `match` over `T`; `_` covers
  what is left on both halves. The match must cover every tag and the
  whole of `T` — E0801 names the missing tag or the uncovered value
  half. The row is consumed as `else` consumes it: a `?` that fires
  inside the scrutinee leaves the function past the arms.
- **New: E0816** — a tag of the row that is also a variant of `T`
  (`Shape ! {Line(int)}`) is refused by name, never guessed; `else |e|
  match e { … }` keeps the halves apart and keeps working.
- The checked machine dispatches a raw or `let`-bound row to the row
  arms and a value to the value arms; native and release split the
  union once (`eu.is_err`) and run one arm chain per half. The sema
  arm sorter (`fallible_arm_half`) is the one rule all three replay.
- Witnesses: six `corpus/rows/match_row_*.lu` rows (every arm kind,
  payload tags, `_` on each half, an enum value half with a nested
  match, the #492 shape, the `else |e| match e` control) and three
  refusals under `rows/negative/`. `match_fallible_lanes.rs` runs each
  on checked, native, release and lupin. lupin 0.1.43 took the first
  bare-tag arm for a value too; its answers were pinned by version as
  pre-mirror until this release's pairing (below).

## 0.2.20 — 2026-10-01

THE TWENTIETH. Arguments are two-phase (the maintainer's ruling #17,
`[mem.tier0.excl.4]`): a later argument may read what an earlier `mut`
argument claims, and may not write, move, re-claim or lend it — for a
`mut` receiver too. Seven silent wrong answers in the published 0.2.19
are fixed, and the checked machine no longer runs a slice's endpoints
or a propagating index operand twice. EG3 lands for one call's
claims: `add2(mut xs[i], mut xs[i + 1])` and a loop index against a
literal run. An `else` handles its scrutinee's own row only (ruling
#18, `[type.row.else]`). A program that inherited a signal blocked
now hears the signals it arms. The pairing moves to lupin 0.1.43, whose
spec pin moves to v0.2.19 and which carries every mirror this release's
gates pinned lupin 0.1.42's answers on by version. Ten lanes and 144 commits: s187, s186,
eg03, s190, s189, s188, s192, s194, s191, s193.

### Read this before you bump the pin

**Seven programs gave a wrong answer with no diagnostic on 0.2.19.**
Each is now refused or answers the ruled bytes on every lane:

- `var w = W { xs }; (mut w.xs).push(9); print("{xs.len} {w.xs.len}")`
  printed `2 2` on native and release: the field shorthand did not move
  `xs` (#486). It is the longhand now, so the later read is E1001.
- `(mut xs).push({ xs = [9]; 5 })` printed `1 9` on every lane and lost
  the push (#487). E1002 now.
- `(mut p).set_x({ p.z = 9; p.z })` under `fn set_x(mut self.{x}, ..)`
  printed `10 3` on native and release (#494). `10 9` on every lane now.
- `bump(mut a, { a = 5; 1 })` printed `6` on checked and `2` on native
  and release (#476). E1002 now.
- `var g = f; g(xs)` against `fn f(mut xs: List[int])` printed `2` on
  checked and `1` on native and release (#484). Refused by name now.
- A nested fn's `read` parameter written, or `f(xs)` against a nested
  `fn f(mut xs: …)`, let the callee's write reach the caller on native
  and release (#466). E1014 / E1007 now.
- `look(m, key()?) else 0` answered `0` on the checked machine when
  `key` failed (#492). It propagates `key`'s row on every lane now.

**Programs that compiled are now refused.** A write, move, re-claim or
lend of a `mut`-claimed place (or a `mut` receiver) inside a later
argument of the same call (E1002); a moded fn used as a value; a call
to a nested fn that omits the mode its parameter declares (E1007); a
read of a local after `W { local }` moved it (E1001). s186 and s192
built boreutils and lobo with no diagnostic; s190 found no shorthand
initializer in either.

**Programs that were refused now compile.** A direct read of a
claimed place in a later argument (`f(mut a, a.x)`, `grow(mut xs,
xs.len)`; E1002 through 0.2.19), and two claims in one call through an
offset or a loop index that `[mem.model.place.elem]` item 4 proves
distinct.

### The pairing: lupin 0.1.43

`PAIRING` names **lupin 0.1.43 at pin `c2401f0`** (wolf-interp release
401010971; the linux x86-64 archive's sha256 `e957c8de…`), whose spec
pin is this compiler's v0.2.19 tag and which carries
wolffe-lang/wolf-interp#155, #157, #159, #160, #162, #164, ruling #17's
two-phase arguments and the lupin halves of #487 and #494. The ritual
(#87, with #281's control against the 0.1.42 archive, same corpus, same
release `wolf`) **moved 29 ledger counts on each tier, together**, all
to agreement: the six `field_shorthand_*` rows and the fourteen refused
`recv_claim_arg_*` rows (completeness → agreement: the compiler's E1001
or E1002 now meets lupin's trap), and the eight `mut_claim_*` read rows
with `mut_read_overlap.lu` (soundness → agreement: both run them now).
Checked: 502 → 531 agreements, completeness 218 → 198, soundness 9 → 0;
native: 543 → 572, 218 → 198, 10 → 1. Twelve files moved below the
ledger, lupin's stdout now the compiler's: `ctl_index_try_once_receivers`,
`ctl_slice_try_once`, `ctl_slice_endpoints_indexed_base` (#157, #162:
each operand once) and the nine `recv_view_arg_*` rows (#494). Native's
one soundness finding is still `memory/unsafe_ub_uaf.lu` (`exit` vs
`ub(mem.ub)`), the program's own use-after-free read on the compiled
lane.

The gates that pinned lupin by version drop every 0.1.42 entry:
`PRE_TWO_PHASE_LUPIN` (with its 0.1.40 and 0.1.41), `PRE_RECEIVER_LUPIN`
and `PRE_VIEW_LUPIN` are empty, and the six `field_shorthand_lanes.rs`
pins and the three measured `LUPIN_0_1_42_*` answers are gone. The 40
cases they held take the ruled arm against the published 0.1.43. None
is widened and none stays pinned.

### A view-set receiver keeps its arguments' writes (s193, #494)

- **`(mut p).set_x({ p.z = 9; p.z })` under `fn set_x(mut self.{x}, ..)`
  lost the write to `p.z` on native and release.** They printed `10 3`
  where checked printed `10 9`, with no diagnostic; trunk and the 0.2.19
  archive alike. `[mem.tier0.excl.3]` leaves the fields outside a view
  set to the caller, and `[mem.tier0.excl.4]` runs the arguments before
  the claim, so the write stands. The two native tiers spilled a
  by-value receiver whole before the arguments ran and reloaded the
  whole slot after the call. The reload now carries the view set's
  fields only, each rebuilt into the variable's current value; a
  receiver without a view set is unchanged.
- Shapes that answered wrong before and agree with checked now: two
  fields written outside the view, a nested struct field outside it, a
  two-field view set with the third field written, the callee writing
  both view fields beside an argument's write, a field receiver with a
  view set (`(mut o.v).set_x(..)`), and a `List` field replaced in the
  argument (`4 1 5`: the replaced buffer came back). An element receiver
  and a re-lent `mut self` lend an address and were already right.
- boreutils and lobo declare no view-set receiver; their emitted code is
  unchanged.
- Witnesses: the nine `corpus/memory/recv_view_arg_*.lu` rows.
  `view_set_writeback_lanes.rs` runs each on checked, native, release
  and lupin. lupin 0.1.42 writes the receiver back whole on all nine
  (the element and re-lent forms too); it is pinned pre-mirror by
  version, and any later lupin must give checked's bytes.

### An `else` handles its scrutinee's own row only (s191, #492)

- **The checked machine's `else` caught a `?` that was propagating
  from inside its scrutinee.** `look(m, key()?) else 0` and
  `m[key()?] else 0` answered 0 when `key` failed, and the function
  ran on past the `else`. Native, release and lupin 0.1.42 leave the
  function with `key`'s row. The checked machine's `else` matched
  every error flow, whether it was the scrutinee's own raw row or one
  a `?` was already propagating. A row `return`ed from a block inside
  the scrutinee was caught the same way. An `else |e|` handler typed
  over `{none}` was handed `parse`.
- Ruled by the maintainer (#18 = A) and written as `[type.row.else]`
  beside `[type.row.operand]`: the handler runs for the scrutinee's
  own row only. The checked machine now passes a propagating row past
  the `else`. Native and release are unchanged.
- Witnesses: seven `corpus/rows/else_*.lu` rows (a `?` in a call
  argument, in a `Map` index, under three nested `else`s, under an
  `else |e|` and an `else |none|` handler, in a block with `defer`
  and `errdefer`, a `return` of a row from a block, and the
  own-failure control). `else_scrutinee_own_row_lanes.rs` runs each
  row on checked, native, release and lupin, and asserts that they
  agree on the right answer.
- Found beside it and filed: #497 (`match` straight over a `!T` is
  refused on every wolfgang lane and runs on lupin), #498 (`?` inside
  a `defer` or `errdefer` overflows the compiler's stack on native and
  release), #499 (`errdefer` in a block whose value is a raw row runs
  on checked and lupin, not on native and release).

### A scaled deref of element size 0 round-trips through the WIR text (s194, #496)

- **The fuzz target `wir_parse` found a verified module that did not
  re-parse.** `fact deref %q 00x%n` parses as element size 0 times
  `%n`, verifies, and printed as `0x%2`; the lexer took `0x` as a hex
  prefix with no digits and refused it (`bad unsigned integer \`0x\``).
  It turned CI's fuzz smoke red on any run that reached that input.
- `0x` now starts a hex integer only when a hex digit follows. A `0x`
  with no digit was refused by every number reader before, so no input
  that parsed changes meaning. The printer, the verifier and every
  lowered dump are unchanged; lowering never emits a scaled deref.
- A search of every printer format that writes a number beside another
  token found one other such format, `{lo}..={hi}`, which was already
  safe. `crates/wolf_wir/tests/fact_text_edges.rs` covers both at their
  edges: 448 facts (every kind and justification) and 29 instruction
  immediates each re-parse to themselves. The artifact joins the seed
  corpus as `fuzz/corpus/wir_parse/deref-elem0-c255f8ab.wir`.

### A `mut` receiver's own arguments (s192, #487)

- **`(mut xs).push({ xs = [9]; 5 })` ran and lost the push.** Every
  wolfgang lane printed `1 9`, the 0.2.19 archive included, with no
  diagnostic; lupin 0.1.42 answers `ub(mem.ub)`. `[mem.tier0.excl.4]`
  makes a `mut` receiver the call's first argument: its own arguments
  may read it, but a write, a move, a re-claim (`mut`) or a lend of it
  is E1002. s186's scan of a later argument's writes, moves and nested
  claims never saw the receiver's claim. It does now, on checked,
  native and release. The receiver's claims (a view set's named fields,
  when it has one) are the first entries the scan checks against.
- Shapes that ran before and are refused now: the receiver written or
  moved out and stored back; a nested call that claims it again
  (`(mut xs).push({ (mut xs).push(7); 5 })` printed `4 7`;
  `(mut xs).push(drain(mut xs))` ran to `trap(bounds)`); an element
  receiver (`(mut xs[0]).push({ xs[0] = …; 5 })`) written, moved, or its
  container replaced; a field receiver written, moved, or its struct
  replaced; and a view-set receiver's own field written
  (`mut self.{x}`: checked printed `51`, native and release `2`). The
  direct re-claim `(mut a).absorb(mut a)`, the `read` lend
  `(mut a).absorb(a)` and a closure argument capturing the receiver were
  already refused by the call surface's pairwise check.
- Reads still run with the value from before the call:
  `(mut xs).push(xs.len)`, a whole read one call down, bu12's
  `(mut out.class_off).push(out.bytes.len)`, a sibling field written, a
  different literal element written, a view set's own field read.
- The report names the receiver ("`xs` is the call's `mut` receiver
  here"). A nested claim of the same place now says "the same place
  twice is never disjoint", where it said "a path and its prefix".
- boreutils `0b273a03` and lobo `35f93a35` build with no diagnostic (46
  of 46 builds, before and after), so no downstream site used a
  refused shape.
- Witnesses: the fifteen `corpus/memory/recv_claim_arg_*.lu` rows.
  `receiver_claim_args_lanes.rs` runs each on checked, native, release
  and lupin. lupin 0.1.42 holds no receiver claim while the arguments
  run (`ub(mem.ub)` on eight rows, and it runs the other six). The gate
  pins those answers for 0.1.42 by version; any later lupin must trap
  `exclusivity`.
- Not changed: a view-set receiver whose argument writes a field
  OUTSIDE the view set is legal, and still answers `10 9` on checked
  and `10 3` on native, release and lupin (wolf-lang#494, found here).

### The struct field shorthand is the longhand (s190, #486)

- **`W { xs }` did not move `xs`.** The longhand `W { xs: xs }` moves a
  non-`Copy` value (`[mem.tier0.move.1]`) and a later read of `xs` is
  E1001. The shorthand was accepted, and wolf 0.2.19's native and
  release builds left two names on one buffer:
  `var w = W { xs }; (mut w.xs).push(9); print("{xs.len} {w.xs.len}")`
  printed `2 2`. The same hole let `return W { xs }` move a `mut`
  parameter out with only W1002, and handed the caller the moved
  buffer. The checked machine dropped a shorthand field from the struct
  and answered `unsupported` at the first field read. A shorthand
  inside a closure or a task was never seen as a capture: `unsupported`
  at lowering, and no E1002 for the borrow it takes.
- The cause was the parse: a shorthand field carried a bare identifier
  and no value, so every pass that reads a field's value read nothing.
  The shorthand now carries a `PathExpr` naming the local, the
  longhand's shape, as a shorthand struct pattern already carried an
  `IdentPat`. Every pass reads it as `x: x`. `wolf fmt` output is
  unchanged. The shorthand-only branches in the type checker, the
  lowering and comptime are gone. A shorthand naming a module-level
  `let` now resolves and lowers as the longhand does.
- No shorthand field initializer existed in the corpus, wolf-std,
  boreutils or lobo before this change, so no existing program changes
  answer.
- Witnesses: the twelve `corpus/memory/field_shorthand_*.lu` rows (the
  issue's shape, `Copy` fields, a mixed literal, nested literals, a
  `return`, a closure, a task, a closure's borrow).
  `field_shorthand_lanes.rs` runs each row and its longhand twin on
  checked, native, release and lupin. It asserts that the twins agree
  and that the wolfgang lanes give the ruled answer. lupin 0.1.42
  copies through the shorthand (wolffe-lang/wolf-interp#159), and the
  gate pins its answers for 0.1.42 by version.

### Offsets and loop indices in one call (eg03, EGC's EG3)

- **Two claims in one call through a run-time index are distinct where
  `[mem.model.place.elem]` item 4 proves it.** R1: `add2(mut xs[i],
  mut xs[i + 1])` runs, and so do `xs[i + 1]` beside `xs[i + 2]` and
  `g[i][0]` beside `g[i + 1][0]`. R2: `for i in 1..3 { add2(mut xs[0],
  mut xs[i]) }` runs, and so do `1..=2` against `0` and `0..2` against
  `2`. Through 0.2.19 each was E1002 on every wolfgang lane while lupin
  ran it. Both rules need the index local unchanged: nothing the call
  evaluates may write it, no loan may be taken on it, and the body may
  have no raw-tier statement. R2 also needs the loop's body never to
  assign the index (a `for` binding is assignable).
- Still refused, each with a row: `i` written between the claims (a
  block argument, or a nested `back(mut i)`), the loop index assigned
  in the body, a range that holds the literal (`1..=2` against `2`),
  `i + 0`, an offset against a literal, `i - 1`, `i + j` and `1..n`.
  lupin 0.1.42 traps the first six and runs the last three.
- Left out, and stated in the clause: moves, local borrows, iteration
  and s186's writes inside a later argument do not read R1 or R2.
- Witnesses: `corpus/memory/elem_offset_mut_pair.lu`,
  `elem_loop_induction_mut.lu` (eg00's parked K13 and K14),
  `elem_dyn_proofs_legs.lu`, and nine twins
  (`elem_offset_*_mut.lu`, `elem_loop_*`). `element_places_lanes.rs`
  asserts them on checked, native, release and lupin.

### An index operand that leaves early runs once (s189, #481)

- **The checked machine ran an index operand again when it left
  early.** Under `conform-run --checked`, `let v = xs[idx()?]` ran
  `idx` twice when its `?` propagated, and three times in a hole, as a
  `read` argument, as a member base, under `copy`, as a method receiver,
  as a `for` iterable and as an element store's right-hand side. Under
  a byte view (`ss[idx()?].bytes().len`) it ran six times. A `return`,
  `break` or `continue` inside an index operand, and `xs[idx() else {
  return 9 }]`, ran it twice. After a `continue`, the next iteration
  trapped use-after-move on a list the re-run had moved. In `g[a()?][b()?]`
  the operands that had already run ran again. Native, release and lupin
  0.1.42 run each operand once, and now every lane does.
- **A store target or `mut` argument whose index propagates** (`xs[idx()?]
  = v()`, `xs[idx()?] += 1`, `g[a()?][0] = v()`, `bump(mut xs[idx()?])`)
  was refused on the checked machine as "not a place". It now runs the
  index once. The right-hand side never runs and nothing is stored, as
  on the other lanes.
- The place lookup now hands the operand's flow back to its caller,
  which returns it. Before, it answered "not a place" and the caller
  evaluated the expression again.
- Witnesses: `corpus/memory/ctl_index_try_once.lu`,
  `ctl_index_try_once_receivers.lu`, `ctl_slice_try_once.lu`,
  `ctl_nested_index_try_once.lu`, `ctl_index_return_break_once.lu`,
  `ctl_index_else_once.lu`, `ctl_store_index_try_once.lu`.
  `index_flow_once_lanes.rs` asserts them on checked, native, release
  and lupin. It pins lupin 0.1.42 by version where lupin runs a
  receiver's operand twice (wolffe-lang/wolf-interp#157, #162).

### A slice's endpoints run once (s187, #479)

- **The checked machine ran a slice's endpoints two or three times**
  when the slice was not simply bound: `xs[lo()..hi()].len` ran `lo`
  and `hi` twice under `conform-run --checked`, and
  `"{xs[lo()..hi()].len}"` ran them three times. Native, release and
  lupin 0.1.42 ran them once, and now every lane does. The same fix
  covers a slice passed as a `read` argument, a slice as the receiver
  of `count` or `is_empty`, a slice as a `for` iterable (all ran
  twice), a slice nested under an index or another slice
  (`xs[lo()..hi()][0]`, `xs[lo()..hi()][0..1].len`), a slice of an
  indexed element (`g[gi()][lo()..hi()]`, which ran `gi` as often as
  the endpoints), and a `str` slice (`s[lo()..hi()].len`). The cause
  was the place lookup: it evaluated the bracket's operands before
  learning the index was a range. It now decides that a slice is not a
  place from the syntax, before any operand runs.
- Witnesses: `corpus/memory/ctl_slice_endpoints.lu` (is60's c14, c18
  and c19) and `ctl_slice_endpoints_{call,nested,indexed_base,str}.lu`.
  `slice_endpoint_once_lanes.rs` asserts them on checked, native,
  release and lupin. lupin 0.1.42 runs an indexed base's outer index
  twice or three times (wolffe-lang/wolf-interp#157), and the gate
  pins that answer for 0.1.42 by version.
- Not fixed here: when the `?` in an index operand propagates
  (`xs[idx()?]`), the checked machine still runs the operand twice
  (#481).

### The #146 gate measures again (s187, #477)

- `release_overlap_guard.rs` had compared `fail(E1014)` with
  `fail(E1014)` since the memory checker began refusing its witness.
  The witness is respelled so that it runs (`var x = copy x0`). The
  gate now asserts that native prints the known sum before it compares
  release with native.
- Safe source can no longer hand the versioned loop two overlapping
  lists (#384 fixed `copy`, and #93 keeps two live Lists off one
  buffer). So a stale overlap guard gives the same answer as a correct
  one, and the behavioural half cannot see #146. A second test reads
  the release mid-end's module (`WOLF_MIDEND_DUMP=1`) and asserts that
  every overlap guard in the witness takes its extent from a len load
  laid out after the last in-place `push`. With #146 planted back, the
  behavioural test stays green and the structural test goes red.

### An armed signal is heard under an inherited blocked mask (s188, #483)

- **A program that inherited a signal blocked never answered it.** A
  process inherits its signal mask across `exec`. lobo, started from
  fish with `ssh -f … &`, inherited SIGINT and SIGQUIT blocked on every
  thread, so the handler `os_signal_listen(QUIT)` installed never ran:
  `kill -QUIT` stayed pending (`ShdPnd 0x4`) and the graceful quit
  never came (found by ws44). nginx sets its own mask; wolf now does
  the equivalent. `os_signal_listen` has the runtime's drain thread
  (`wolf-signal`, the one thread it keeps for the program's life)
  unblock exactly the signals of the meanings the program arms, and
  returns once it has. Every other signal keeps the inherited mask, on
  every thread: an unarmed SIGHUP or SIGINT stays blocked and pending.
  A signal sent while it was still blocked arrives when the listen
  unblocks it. Written into `[os.signal.listen]`.
- Witness: `signal_blocked_mask.rs` launches the program with the set
  blocked (`sigprocmask` in the forked child), sends the signal, and
  reads every thread's mask on linux. It was red at trunk on linux and
  macOS (native and release) and is green here.
- Windows has no signal mask. Its inheritable analog, the console's
  CTRL+C-ignore flag, is not cleared yet (#490).

### A moded module fn is refused as a value (s188, #484)

- **A module fn with a `mut` or `take` parameter read as a value ran
  with its mode erased.** A fn type carries no modes, so a call through
  the value passed the argument as a plain `read`. `var g = f; g(xs)`
  against `fn f(mut xs: List[int])` printed `2` on checked (the callee
  grew the caller's list through a `read` argument) and `1` on native
  and release, with no diagnostic. Through a value a `take` parameter
  never moved (`2 2`). A `mut int` parameter's fn returned as a value
  lost its write on checked and was an ICE on native and release. It is
  now refused by name on every lane (`unsupported`, "a fn with `mut` or
  `take` parameters used as a value"): bound, passed, returned, or
  qualified from another module. Call it by name. This is s186's rule
  for a nested fn, and lupin 0.1.42 already refuses each of these. The
  spec clauses are `[gram.type]` (a fn type has no mode slot),
  `[type.fn.value]`, and `[mem.tier0.mode.mut]` (a call site must
  spell `mut`).
- Witnesses: `fn_value_modes_lanes.rs` on checked, native, release and
  lupin, with both polarities: five refused shapes, and a modeless fn
  value and the by-name calls, which run. Also a `wolf_sema`
  typecheck test.

### Fixed

- **Arguments are two-phase (s186, wolf-lang#476, ruled 2026-09-30;
  `[mem.tier0.excl.4]`).** Within one call the arguments evaluate left
  to right and a `mut` argument's claim takes effect at call entry. A
  later argument may READ the claimed place and sees the value from
  before the call: a `Copy` value (`f(mut a, a.x)`), an operand
  (`bump(mut a, a + 1)`), a header or member (`grow(mut xs, xs.len)`,
  `xs.count()`), string interpolation (`fail(mut fl, "{fl.store}")`),
  or a nested call whose result lends nothing from the place
  (`bump(mut xs[0], total(xs))`, `ring_drop(mut r, ring_len(r))`). A
  direct read (D39's rule) was E1002 through 0.2.19, and it runs now. A
  read one call down ran already, while lupin traps it; the compiler's
  answer is the ruled one, and is63 moves lupin. A later argument may
  not WRITE or MOVE the claimed place, and that is E1002 now:
  `bump(mut a, { a = 5; 1 })` printed `6` on checked and `2` on native
  and release through 0.2.19. A second claim, or a lend into the same
  call (`both(mut xs[0], xs)`, a capturing closure), stays E1002, and
  so do `take` arguments.
- **A nested fn's `mut` and `take` parameters are real (s186,
  wolf-lang#466).** `f(mut xs)` against a nested `fn f(mut xs: …)` was
  E1007 ("takes this parameter as plain `read`"), and `f(xs)` compiled —
  and on native and release the callee grew the caller's list through
  it. The call site now spells the mode the nested fn declares (E1007
  when omitted), the call runs on native and release with the callee's
  writes reaching the caller (an `int`, an element, a list grown or
  replaced, a `mut` parameter re-lent), and the memory checker holds the
  nested body to its modes: a `mut` parameter left moved-out at the
  nested fn's return is E1001 (#464's rule), a write through a `read`
  parameter is E1014, and a `read` parameter returned is E1002
  (wolf-lang#366's rule) — the last two ran through 0.2.19 with the write
  reaching the caller where lupin 0.1.42 does not. A nested fn with a
  moded parameter is called by name only: used as a value (`var g = f`,
  passed to a higher-order fn) it is refused by name, since a fn type
  carries no modes. The checked executor still declines every nested
  fn (the #12 family), and lupin 0.1.42 declines one with a moded
  parameter.

## 0.2.19 — 2026-09-30

THE NINETEENTH. `mut` claims become element-granular (EGC's EG2):
`swap(mut xs[0], mut xs[1])` and its nested, field and loan shapes
compile and run, and every one-place shape is still refused. Item 1(c)
of `[mem.model.place.elem]` holds under a claim as it does for moves
(wolf-lang#472, ruled A), and `len`, `count` and `is_empty` are header
reads beside a moved or claimed element (wolf-lang#474 and
wolffe-lang/wolf-interp#149, ruled 2026-09-30). Fixed first: a
release-tier ICE in the published 0.2.18 (#470). Also fixed:
`--deny-warnings` hid #464's refusal behind a W1002 (#469), and 37 gate
files read an ICE as an environment skip (#471). The pairing moves to
lupin 0.1.42, whose spec pin moves to v0.2.18 (it was two releases
behind) and which carries every mirror 0.2.18's gates pinned by
version. Three lanes and 100 commits: eg02, eg02b, s185.

### Read this before you bump the pin

**#470 — the release tier stopped with an internal compiler error.** On
0.2.18 this runs on the checked and native tiers (`2 12`), and the
release tier stops with `ICE: mid-end broke the module`
(`token-linearity`, "effect token consumed twice on one path"): the
inliner bound `add2`'s two token chains to the caller's one region.
Here the call stays out of line and it prints `2 12` on every lane:

```wolf
struct R {
    a: int,
    b: int,
}

fn add2(mut a: int, mut b: int) {
    a = a + 1
    b = b + 10
}

fn g(mut r: R) {
    add2(mut r.a, mut r.b)   // 0.2.18, release tier: ICE (token-linearity)
}
// main: var r = R { a: 1, b: 2 }; g(mut r); print("{r.a} {r.b}")
```

(`corpus/memory/mut_two_fields_one_region.lu`.) EG2's element pairs,
`add2(mut xs[0], mut xs[1])`, reach the same shape, which is how eg02
found it.

**Programs that were refused now compile.** Two `mut` claims on two
literal elements (E1002 or E1013 through 0.2.18), a member read
(`xs.len`) beside an element claim (E1002), and `count()`,
`is_empty()` or a `Map`/`Pool`'s header beside a moved element (E1001).

### Element-granular claims (eg02, EGC's EG2)

- **`mut` claims are element-granular (EGC's EG2, eg02).**
  `[mem.model.place.elem]`'s items 1(a)/(b) and the path rule now hold
  for every rule that asks whether two paths conflict under a claim —
  call exclusivity (`[mem.tier0.excl]`, E1002, with D39's read and
  s168's nested call inside a `mut` argument), borrows
  (`[mem.tier0.borrow]`) and iteration (`[mem.iter.excl]`, E1013) — not
  only moves. `swap(mut xs[0], mut xs[1])`, `add2(mut g[0][1], mut
  g[1][0])`, `f(mut cs[0].n, mut cs[1].n)`, `bump(mut xs[0], xs[1])`,
  `put(mut xs[0], grow(mut xs[1]))`, `for x in xs[0] { (mut
  xs[1]).push(x) }` and a store to `xs[1]` under a `dyn` loan of
  `xs[0]` compile and run on checked, native and release with lupin
  0.1.41's bytes; each was E1002 or E1013 through 0.2.18.
  `corpus/memory/mut_elem_excl.lu` now runs. Every item 2 shape stays
  refused — a run-time index against any index, the same literal
  twice, `Pool` handles, a path and its prefix — each with a corpus
  twin.
- **wolf-lang#470 fixed: a release-tier ICE.** Two `mut` arguments in
  one caller region — two fields off a `mut` parameter, `add2(mut r.a,
  mut r.b)` inside `fn g(mut r: R)` — made 0.2.18's release build stop
  with `ICE: mid-end broke the module` (`token-linearity`) once the
  callee inlined: the inliner bound the callee's two token chains to
  the caller's one. Such a call now stays out of line. EG2's element
  pairs reach the same shape. Witness
  `corpus/memory/mut_two_fields_one_region.lu`, `2 12` on every lane.

### Item 1(c) under a claim (eg02b, #472) and ICEs fail the gates (#471)

- **Item 1(c) holds under a claim too (eg02b, wolf-lang#472 ruled
  A).** An element and a member of its container are distinct places
  for claims as for moves, whatever the index: `bump(mut xs[0],
  xs.len)`, `bump(mut xs[i], xs.len)`, `bump(mut g[0][1], g[0].len)`
  and a member read before the claim compile and run on checked, native
  and release; eg02 had kept them E1002 because lupin 0.1.41 reads
  `.len` as the whole container and traps. `elem_member_read_after_mut.lu`
  now runs. The whole container under an element claim
  (`both(mut xs[0], xs)`) stays E1002. lupin through 0.1.41 still
  traps these rows; the gate names those versions and expects the ruled
  answer from any later one.
- **An internal compiler error fails the driver gates (wolf-lang#471).**
  `wolf` exits 2 both when a host cannot run a lane and on an ICE; 37
  gate files under `crates/wolf_driver/tests/` read every exit 2 as an
  environment skip, so an ICE on the native or release lane passed. They
  now ask one helper, `lane_exit::environment_refusal`, which fails the
  test when stderr carries the driver's `ICE:` line.

### Header reads (s185, #474, wolffe-lang/wolf-interp#149) and `--deny-warnings` (#469)

- **`len`, `count` and `is_empty` are header reads (s185, wolf-lang#474
  and wolffe-lang/wolf-interp#149, ruled 2026-09-30).** They read only a
  container's header, so they are allowed beside a moved element:
  `xs.count()` and `xs.is_empty()` after `move xs[0]`, a `Map`'s
  `count()`, `is_empty()` and `len()` after a value is read out, a
  `Pool`'s `len()` and `is_empty()` after `move p[h]` compile and run on
  checked, native and release (lupin 0.1.41's bytes where lupin runs
  them); each was E1001 through 0.2.18. Beside an element claim they
  already ran (`bump(mut xs[0], xs.count())`), and the ruling keeps
  that. Every other method still reads the whole container and stays
  E1001 beside a moved part — `push`, `get`, `pop`, a slice, an impl
  method's `self`, a user type's method named `count` — and a whole move
  still refuses `count()`. `[mem.model.place.elem]` 1(c) names the
  header reads. lupin through 0.1.41 traps the header methods under a
  claim and runs the whole-read methods beside a moved part; the gate
  names those versions and expects the ruled answer from any later one.
- **`--deny-warnings` no longer hides #464's refusal (wolf-lang#469).**
  A `mut` parameter moved out and never stored back is E1001, and
  W1002's "never written" stands down beside it — but under
  `--deny-warnings` the promoted W1002 stopped `wolf build`, `wolf test`
  and `conform-run` at the resolve rung, before the E1001 existed, and
  its help said to drop the `mut`. A promoted W1002 now waits for the
  mem rung; the E1001 (or #325's E0804) retires it and is the one
  error reported, `fail(E1001)` at `mem` in the record. A W1002 nothing
  retires still rejects the program, at `resolve`.

### The pairing: lupin 0.1.42

`PAIRING` names **lupin 0.1.42 at pin `ec56a08`** (wolf-interp release
400022505; the linux x86-64 archive's sha256 `9856335a…`), whose spec
pin is this compiler's v0.2.18 tag and which carries
wolffe-lang/wolf-interp#143–#146, #149, #151, #152 and the lupin halves
of #472 and #474. The ritual (#87, with #281's control against the 0.1.41
archive, same corpus, same release `wolf`) **moved 13 ledger counts on
each tier, together**, all to agreement: `elem_header_methods_under_claim`,
`elem_member_read_after_mut` and `elem_member_read_under_claims`
(soundness → agreement: 0.2.19 runs them and lupin now does too);
`elem_key_reassigned_no_revive`, the six `elem_whole_read_*_after_move`
rows and `mut_param_moveout_{map,one_path,whole}` (completeness →
agreement: the compiler's E1001 now meets lupin's `trap(use-after-move)`).
Checked: 444 → 457 agreements, completeness 200 → 190, soundness 3 → 0;
native: 485 → 498, 200 → 190, 4 → 1. One file moved below the ledger:
`ctl_store_order_nested_index.lu`, whose lupin stdout is now the
compiler's (#145, each index operand once). Native's one soundness
finding is still `memory/unsafe_ub_uaf.lu` (`exit` vs `ub(mem.ub)`), the
program's own use-after-free read on the compiled lane.

The gates that pinned lupin by version drop every 0.1.40 and 0.1.41
entry: nine lists in `element_places_lanes.rs`, `mut_param_return_lanes.rs`
and `store_order_lanes.rs` are empty, and the fourteen cases pinned at
0.1.41 (#144, #145, #146, #149, #472, #474) take the ruled arm against
the published 0.1.42. None is widened and none stays pinned.

## 0.2.18 — 2026-09-27

THE EIGHTEENTH, a point release. The published 0.2.17 gives two silent
wrong answers on the native tier, and both are fixed here and go first:
an index store revived a moved sibling element, so native and release
printed a moved buffer through the container (#460); and a function
that moved out of a `mut` parameter handed the caller the moved buffer
(#464). Each witness below compiles and runs on 0.2.17 and is
`fail(E1001)` on every wolfgang lane here; a downstream can run them
against its own pin. Beside them land the maintainer's rulings of
2026-09-26 — R1/R3 over any `Copy` local (eg01b) and index first, then
value (s183, #452) — and the clause they all stand on (eg00), and the
pairing moves to lupin 0.1.41, the first lupin whose element reads trap
on a moved element (wolffe-lang/wolf-interp#141). Five lanes and 74 commits: eg00,
eg01, eg01b, s183, s184.

### Read this before you bump the pin

Two programs 0.2.17 compiled are refused, and both were reading a moved
value on every machine that ran them.

**#460 — an index store revived a moved sibling element.** On 0.2.17
this compiles; the checked machine traps `use-after-move` at the read,
native and release print `2 2 1` (the moved list and `xs[0]` are one
buffer, so the push through `a` shows in `xs[0]`), and lupin 0.1.40
prints a third answer, `2 1 1`. Here it is `fail(E1001)` on checked,
native and release, and lupin 0.1.41 traps:

```wolf
var xs = List[List[int]]()
(mut xs).push([1])
(mut xs).push([2])
var a = move xs[0]
xs[1] = [5]
(mut a).push(7)
print("{a.len} {xs[0].len} {xs[1].len}")   // E1001: `xs[1] = …` revives only `xs[1]`
```

(`corpus/memory/elem_const_store_no_revive_heap.lu`; its three twins
store through a run-time index, hold `int` elements, or both.)

**#464 — a `mut` parameter left moved-out at return.** On 0.2.17 this
compiles with only a W1002 ("never written"), and checked, native and
release print `2`: the caller's `xs` still names the buffer `f` moved
into `t` and grew. Here it is `fail(E1001)` at the move:

```wolf
fn f(mut xs: List[int]) {
    var t = move xs   // E1001: `xs` returns to the caller moved-out
    (mut t).push(9)
}
// main: var xs = [1]; f(mut xs); print("{xs.len}")
```

(`corpus/memory/mut_param_moveout_whole.lu`; the field, element, map
value and one-path shapes beside it, and their store-back twins, which
run.)

### Element places for moves, and #460 (eg01, EGC's EG1)

- **#460 is fixed**: an index store no longer revives a moved sibling
  element. `var a = move xs[0]; xs[1] = [5]` followed by a read of
  `xs[0]` compiled on 0.2.17 and gave three answers — the checked
  machine trapped `use-after-move`, native and release printed the
  aliased `2 2 1` (the moved list and `xs[0]` were one buffer), lupin
  printed `2 1 1`. It is now `fail(E1001)` on every wolfgang lane:
  `[mem.model.place.elem]` item 3, a store revives a moved element only
  through an index that surely denotes it — the same integer literal,
  the same plain string/`bool`/`char` literal key, or (R3) the same
  plain integer local unwritten since the move
  (`var t = move xs[i]` … `xs[i] = take t` keeps compiling). A store to
  the whole container still revives every element.
- **Moves are element-granular for literal indices** (item 1): `move
  xs[0]` leaves `xs[1]` readable (`xs[0].tags` and `xs[1].tags` too),
  a value read out of `m["a"]` leaves `m["b"]` readable, and an element
  move leaves the container's `len` readable. wolf-lang#446's
  conservatism row, `corpus/memory/elem_move_one_place.lu`, now runs
  `1 2` on every machine. Run-time indices stay one place (item 2), and
  `mut` claims still treat a container's elements as one place (EG2).
- **Read this before you bump the pin:** a program that read a moved
  element because a store elsewhere in the container revived it is now
  E1001 — it was reading a moved value on every machine.
  `corpus/memory/list_session_struct.lu` was one (`let s2 = tbl[2]`
  moved a `Session`, then `for c in tbl` read it); it now says
  `copy tbl[2]` and prints the same bytes. (A value read out of a
  `Map` through a `str` local key and stored back through the same key
  was refused here too, R1/R3 then being stated over integer locals;
  eg01b below lifts it.)
- Diagnostics name the element: `xs[0]`, `m["a"]`, `xs[i]` where they
  printed `xs[_]`; E1002 between two elements says the claim treats a
  container's elements as one place rather than calling them a path and
  its prefix.
- Witnesses: the four #460 rows, EG1's four parked rows, item 3's
  positive twin and R3's failing twin under `corpus/memory/elem_*.lu`,
  asserted on checked, native, release and lupin by
  `element_places_lanes.rs` (lupin 0.1.40's pre-mirror answers pinned
  by version, wolffe-lang/wolf-interp#141).

### A `mut` parameter is initialized at every return (s184, #464)

- **A silent wrong answer in 0.2.17 is refused**: a function that moved
  out of a `mut` parameter — the whole parameter, a field, an element,
  a map value — and returned without storing a value back compiled with
  only a W1002, and native and release handed the caller the moved
  buffer (`var t = move xs; (mut t).push(9)` printed `2` for the
  caller's `xs.len`, the #460 aliasing through a parameter). It is now
  E1001 at the move on the checked, native and release lanes:
  `[mem.tier0.mode.mut]` states that a `mut` parameter is initialized at
  every return of the callee, the `?` error edge and an early `return`
  included. A store back before each return (`xs = t`, `s.tags = t`,
  `xs[0] = t`, `m[k] = take v` under `[mem.model.place.elem]`'s revival
  rules) keeps it legal.
- W1002's "this `mut` parameter is never written" no longer fires
  beside that refusal: the move out was the write its scan cannot see.
- **Read this before you bump the pin:** a function that moves out of a
  `mut` parameter on some path and never puts a value back is E1001 now
  — every caller of it was reading a moved value.
- Witnesses: `corpus/memory/mut_param_moveout_{whole,field,elem,map,
  one_path}.lu` (refused) and `mut_param_restore_{whole,field,elem,
  map}.lu` (run), asserted on checked, native, release and lupin by
  `mut_param_return_lanes.rs`.

### R3 over any `Copy` local (eg01b, the maintainer's ruling)

- **`m[k] = take v` revives again with a `str` key**: `var v = m[k]
  else …; (mut v).push(x); m[k] = take v` compiled on 0.2.17 (through
  #460's any-store revival) and was E1001 after eg01, whose R3 took
  only an integer local. `[mem.model.place.elem]`'s R1 and R3 are now
  stated over a local of any `Copy` type, and the compiler follows: a
  store through the same `str`, `char` or `bool` key local, or the
  same `Pool` handle local, unwritten since the read-out, revives it.
  A key local written in between still revives nothing — on 0.2.17
  that shape printed the moved buffer on every wolfgang lane.
- The clause's stale sentences are fixed: R3 is no longer labelled not
  implemented, item 3 names R3 as its exception, and "where the
  machines stand" describes trunk rather than 0.2.17.
- Diagnostics and CFG dumps name a pool handle (`pool[h]`) where they
  printed `pool[_]`.
- Witnesses: `corpus/memory/elem_str_key_revive.lu`,
  `elem_char_bool_key_revive.lu`, `elem_pool_handle_revive.lu` and the
  blur row `elem_key_reassigned_no_revive.lu`, asserted on checked,
  native, release and lupin by `element_places_lanes.rs`.

### Index first, then value (s183, #452, the maintainer's ruling)

- **The checked machine now evaluates a store's place operands before
  its right-hand side**: `xs[idx()] = val()` printed `val idx` under
  `conform-run --checked` through 0.2.17 and `idx val` on native,
  release and lupin; it prints `idx val` everywhere. The same holds
  for a `Map` key (`m[key()] = val()`), every level of a nested index
  (`g[i()][j()] = val()`: `i j val`), a compound store (`xs[i()] +=
  v()`) and a raw-pointer element store (`p[i()] = v()`). The index's
  value is taken first too: `ys[i] = bump(mut i)` stores at the old
  `i` on every lane (the checked machine stored at the new one).
- `[mem.model.place.rhs]` states the order: the place's operands, left
  to right and outermost first, then the right-hand side, then the
  address, then the store — the address is still located after the
  right-hand side, so `xs[x()] = grow(mut xs)` stores into the grown
  list.
- Witnesses: `corpus/memory/ctl_store_order.lu` (s182's control) and
  `ctl_store_order_{map,nested,nested_index,captured,compound,raw}.lu`,
  asserted on checked, native, release and lupin by
  `store_order_lanes.rs`. lupin 0.1.40 evaluates a multi-index store's
  outer operands twice (wolf-interp#145); the gate pins that answer
  for 0.1.40 and 0.1.41 by version.

### The element clause (eg00, #446's campaign)

- **`[mem.model.place.elem]`** (spec/02 §1) says which index shapes are
  distinct places: two integer-literal indices of a `List` with
  different values, two literal `Map` keys with different values, an
  element against its container's header member, and tuple positions
  (already distinct). Everything else stays **one place** — a run-time
  index against any other index, a `Pool` handle, a non-literal key —
  and re-initialization is a *must*: a store revives a moved element
  only through an index that provably denotes it. Three proof rules for
  run-time indices (offset from one unassigned local, a loop index
  against a literal outside its range, same-index revival) are stated
  and marked not yet implemented. No compiler behaviour changes here:
  the clause is the oracle for eg01–eg03.
- Ten witnesses green at trunk (`corpus/memory/elem_*.lu`, asserted on
  checked, native, release and lupin by `element_places_lanes.rs`);
  thirteen parked with ruled verdicts in the planning repository.
- **#460** (filed): an index store revives a moved sibling element —
  `move xs[0]; xs[1] = v` then a read of `xs[0]` compiles; the checked
  machine traps, native aliases the moved list, lupin prints a third
  answer. `corpus/memory/list_session_struct.lu` reads a moved element
  through this hole.
- **wolffe-lang/wolf-interp#141** (filed): lupin 0.1.40's index read
  does not trap on a moved element.

### The pairing: lupin 0.1.41

`PAIRING` names **lupin 0.1.41 at pin `93a5fe5`** (wolf-interp release
397709135; the linux x86-64 archive's sha256 `18848901…`), the first
lupin whose every element read traps on a moved element
(wolffe-lang/wolf-interp#141). The ritual (#87, with #281's control
against the 0.1.40 archive, same corpus, same release `wolf`) **moved 7
ledger counts on each tier, together**, all `completeness → agreement`
where the compiler's static E1001 now meets lupin's
`trap(use-after-move)`: `elem_const_store_no_revive_{heap,int}.lu`,
`elem_dyn_store_no_revive_{heap,int}.lu`, `elem_move_same_const_read.lu`,
`elem_sym_reassigned_no_revive.lu` and `mut_param_moveout_elem.lu`.
Checked: 420 → 427 agreements, completeness 197 → 190; native: 461 →
468, 197 → 190. Nothing moved below the ledger. Native's one soundness
finding is still `memory/unsafe_ub_uaf.lu` (`exit` vs `ub(mem.ub)`), the
program's own use-after-free read on the compiled lane.

The gates that pin lupin by version take 0.1.41 as follows: the seven
#141 cases in `element_places_lanes.rs` and `mut_param_return_lanes.rs`
now take the ruled arm (the trap) with no edit; the #146 list in
`mut_param_return_lanes.rs` names 0.1.41 as pre-mirror (lupin still
prints `1` when a callee moves a whole `mut` parameter); the #144 and
#145 lists already named it.

### Shipped, by name

Open at the cut:

- **#446** — `move xs[0]` on a `List[int]` refused a later read of
  `xs[1]` through 0.2.17; eg01 makes literal indices distinct places,
  so that row now runs, but run-time indices stay one place and the
  maintainer's word on the issue is still owed.
- **#455** — on the native tier a first-class `region()` created inside
  a function is not freed on return; the block form `region r { … }`
  frees.
- **#458** — an unannotated integer literal binding is typed by a later
  comparison on wolfgang and runs as `int`; lupin keeps the `i32`
  default and traps.
- **#466** — a nested fn's `mut` parameter is read as plain `read` at
  the call site (E1007).
- **#457** (the test suite, not the toolchain) — `scope_handle_param.rs`
  runs whatever `target/release/wolf` exists and never rebuilds it;
  rebuild before a release-profile run.
- On lupin 0.1.41, open and pinned by version in this release's gates:
  wolffe-lang/wolf-interp#144 (a `Map` read copies a non-`Copy` value
  out), wolffe-lang/wolf-interp#145 (a nested index store runs its
  outer operands twice) and wolffe-lang/wolf-interp#146 (a caller does
  not see its callee move a whole `mut` parameter); and
  wolffe-lang/wolf-interp#143 (a whole read of a container holding a
  moved part does not trap).

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
