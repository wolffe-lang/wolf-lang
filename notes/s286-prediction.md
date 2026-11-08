# s286 prediction: glibc's CPU-count read on a closed descriptor 1

Committed before the first change. Base: wolf-lang trunk `d848fbed`. Nothing here is measured yet
at this base; the cause is the one measured in #1051's comment, and every claim below names the
row or measurement that will check it.

## The cause, as measured in #1051

A native program started with descriptor 1 closed can see a descriptor on 1 that the C library
opened. A pool worker's first allocation (Rust std's `Thread` handle) gives the new thread a
glibc malloc arena. Once more threads hold arenas than glibc's test bound (8 on 64-bit), glibc
asks for the CPU count once, through `get_nprocs()`, which opens
`/sys/devices/system/cpu/online` on the lowest free descriptor, the closed 1, for a few
microseconds. `fs_fstat(1)` on main answers ok inside that window. The runtime's own descriptors
go through `off_std`, but this one is opened inside the C library. A program crosses the bound as
its workers start when it is given 8 or more CPUs (one worker per CPU, plus main and any runtime
thread), so the race row fails on such hosts and passes on CI's smaller runners.

## What the documentation says

- `mallopt(3)`: `M_ARENA_TEST` is the arena count at which "the system configuration will be
  examined to determine a hard limit"; its default is 8 where `sizeof(long)` is 8 and 2 where it is
  4. "The value of `M_ARENA_TEST` is not used when `M_ARENA_MAX` has a nonzero value."
- The glibc manual, 2.39 through 2.43 (Memory Allocation Tunables): `glibc.malloc.arena_max`
  defaults to 0, "meaning that the limit on the number of arenas is determined by the number of
  CPU cores online. For 32-bit systems the limit is twice the number of cores online and on 64-bit
  systems, it is 8 times the number of cores online." The development manual rewords this as "the
  number of CPU cores online reported by get_nprocs()" and drops the multiple, so the limit is
  checked against the host's glibc by a row (row 3), never assumed.
- `mallopt(3)`: `MALLOC_ARENA_MAX` and `MALLOC_ARENA_TEST` control the same parameters; "if the
  same parameters are adjusted via mallopt(), then the mallopt() settings take precedence"; the
  variables "are ignored in set-user-ID and set-group-ID programs".
- `get_nprocs(3)`: the processors currently available (online); the implementation opens and
  parses files in `/sys` on every call. `GLIBC_TUNABLES` is a colon-separated list of `name=value`
  items (the manual's Tunables chapter).

## Decision 1: the value, and where it is set (recommended: two steps)

**Recommended.** The arena limit is settled in two steps, both on linux-gnu only:

1. **At entry**, in `__wolf_rt_main_start` (the entry shim's first call, before `main`, with no
   other thread in the process): `mallopt(M_ARENA_MAX, 8)` (2 on a 32-bit target). Eight is the
   count at which glibc would have run its test, so a program that never has more than 8 arenas
   allocates exactly as it did. Because the limit is now nonzero, glibc never runs the test, on
   any host and for any thread: the runtime's, the pool's or a C library's. No CPU count is needed
   for this step, so it makes no system call. The one program that allocates differently is one
   whose threads outnumber 8 before any pool exists, which only a C library's threads can do: they
   share 8 arenas until the pool starts (for good, if it never does), where glibc would have
   allowed 8 × cores.
2. **At the pool's first start**, inside the pool's one-time initialization, before its first
   worker exists: `mallopt(M_ARENA_MAX, 8 * get_nprocs())` (`2 *` on 32-bit), clamped to at least
   step 1's value. This is glibc's own documented limit, from the same function glibc would have
   called, so a program that runs workers reaches the arena count glibc would have allowed it.
   `get_nprocs()`'s file lands on the lowest free descriptor, the closed 1 if it is closed, but on
   the thread that starts the pool. Before the pool exists that thread is the only one running
   program code; the signal drain and reactor threads never look at a program's descriptors. (The
   pool's initialization already reads the CPU affinity and cgroup files through Rust std's
   `available_parallelism`, at the same point, by the same argument.)

**Why this holds at any CPU count.** The race needs glibc's test to run on one thread while
another runs program code. Step 1 makes the test unreachable before any thread exists. The CPU
count then decides only the limit's size, and step 2 sets that to glibc's own value for the host.

**Not taken: one step at entry, `8 * get_nprocs()` for every program** (the plan as first
written). It gives the same limit, but every linux-gnu program would pay `get_nprocs()`'s calls at
start, including the many that never start a thread. The start-up gate (`bench/startup.json`: the
runtime's own start and exit at most 12 system calls over C's `hello`, "may not rise") would go
red and have to rise to 15 or 16.

**Not taken: step 2 alone.** Before the pool started, glibc's default would stand, so a C
library's threads could still cross the bound while main runs program code.

**Predicted:** the arena count a program reaches with the settle equals glibc's own count without
it at every CPU count tried (4, 8, 12 and all CPUs of a 20-CPU host). glibc's limit follows the
online count, not the affinity: under `taskset` on 4 of 20 CPUs its limit is 160, not 32. If either
count differs, glibc 2.39's formula is not `8 * get_nprocs()` and I stop and report before the
fix lands.

## The cost

- **Allocation throughput: unchanged.** The limit is glibc's own, so programs reach the same
  arenas. Single-threaded programs (family B's `list_alloc` and `b3_churn`) never leave the main
  arena, so step 1's cap never binds. The pool's benches (`spawn_bench`, `chan_bench`) pass
  through step 2 and reach the same arena counts. **Bound:** every row is a tie by
  `bench/protocol.md`'s rule (a delta must clear both 3×MAD and the 2% floor), and I expect each
  after/before median ratio within 0.98–1.02. The rows run interleaved, before and after, on one
  set of CPUs with nothing else running beside them.
- **Start-up: unchanged.** `mallopt` and reading the environment make no system call, so the
  start-up gate stays at 12 and the report-only wall ratio of `hello` moves less than 2%.
- **The pool's first start:** one `get_nprocs()`, predicted as `openat`, `read` and `close` of
  the online file (a fourth call if it reads to the end), once per process. glibc's own test made
  the same calls in any program past 8 arenas; a pooled program with fewer now pays them too.
- **Memory:** unchanged, by the same limit.

## Decision 2: how the skip detects a user's own setting

The environment the program started with is read once, at entry. That is the environment glibc
read, and nothing has run yet to change it. Steps 1 and 2 are both skipped when it holds either:

- `MALLOC_ARENA_MAX`, with any value, empty and 0 included. Interpreting the value is glibc's
  business; the user asked for their own policy.
- a `GLIBC_TUNABLES` item whose name is exactly `glibc.malloc.arena_max`, in any position in the
  list.

The decision is kept, so step 2 does what step 1 decided. A runtime used without the entry shim,
as in the runtime's own tests, makes the decision at the pool's start from the process
environment.

**Recommended: an arena test bound is not a limit, so it does not skip the settle.**
`MALLOC_ARENA_TEST` or `glibc.malloc.arena_test` only moves the moment glibc reads the CPU
count, and that read is what this fix removes. The documented limit is the same either way. One
edge changes: a test bound set above 8 × cores let glibc create that many arenas before it
tested; with the settle it creates 8 × cores. A user who wants more arenas sets the limit, which
is honoured. Not taken: skipping on a test bound too, which would keep the race for exactly those
users.

**Recommended: in a set-user-ID or set-group-ID program (`getauxval(AT_SECURE)` nonzero), the
variables are not the user's setting, and the settle runs.** `mallopt(3)` says glibc ignores them
there. I predict glibc 2.39 also ignores `GLIBC_TUNABLES` in such a program, measured with a
set-user-ID copy run as another user in a container. If glibc honours the tunable there, the skip
follows glibc.

## Rows, red first

1. **The CPU count is read only where no other thread runs program code** (driver, linux-gnu,
   new, both tiers). Under `strace -f -e trace=openat,clone,clone3`, a fixture holds 12 tasks
   asleep at once, so the pool's workers and their compensating extras outnumber glibc's bound on
   any CPU count. Every `openat` of `/sys/devices/system/cpu/online` must come from the main
   thread, before the first worker's clone. **Red at trunk on any Linux glibc host, CI's included:**
   a worker opens it. Green at the fix. Without `strace` it skips loudly; `WOLF_SYSCALL_REQUIRE_STRACE`
   (set on CI's Linux job) makes the skip a failure. A program that starts no pool must open the
   file not at all, which checks step 1's "no system call".
2. **#781's race row as it stands** (`a_descriptor_wait_makes_no_descriptor_while_the_program_runs`),
   run unpinned with 8, 12 and every CPU it can be given on a 20-CPU host, with descriptor 1 closed. **Red at trunk** at #1051's rate: at
   least 9 of 10 runs per tier at each count, `ok` 7 to 27 in a red run. **At the fix:** 0 of 100
   runs per CPU count per tier, each run the row's own check (`ok 0`, every wait ready), plus the
   row itself as the record.
3. **The limit is glibc's own** (runtime, linux-gnu, new, a harness-less test). Three children,
   each holding `8 * get_nprocs() + 8` threads that have allocated at once, and arenas counted from
   `malloc_stats(3)`'s report:
   - the settled child, through `__wolf_rt_main_start` and the pool's start;
   - a control child with neither, which is glibc's default;
   - a child through `__wolf_rt_main_start` alone, which must stop at 8.

   The settled count must equal the control's, and is reported beside `8 * get_nprocs()`. The
   equality holds at trunk too, where nothing is settled; it is the row that keeps the settled
   limit glibc's on whatever glibc the host has. The third child is red at trunk: glibc's default
   lets it reach the control's count.
4. **The skip**, in row 3's runtime test. With `MALLOC_ARENA_MAX=2`, with
   `GLIBC_TUNABLES=glibc.malloc.arena_max=2`, and with that item after another
   (`glibc.malloc.check=0:glibc.malloc.arena_max=2`), the settled child holds at most 2 arenas.
   With `GLIBC_TUNABLES=glibc.malloc.arena_test=1` it is settled as without it. Under row 1's
   tracer, the settled fixture with `MALLOC_ARENA_MAX=2` opens the online file nowhere. The skip
   row is shown red on a local commit of the settle without its skip, which never ships.
5. **The start-up gate** (`cargo xtask bench-gates`, unchanged): `hello` stays at 12 calls over C.

The macOS take runs the same gauntlet: the settle is linux-gnu only, so nothing there moves, and the
new rows skip by name on macOS.

## The plant

`s286-plant-no-settle`: the final head with both `mallopt` calls removed and the skip left in
place. It goes red by name at #781's race row on 8 or more CPUs, and at row 1 on any host. Each
`FAIL <row>` line is at column 0 in the record.
