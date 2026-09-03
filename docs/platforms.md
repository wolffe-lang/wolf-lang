# Platforms

What each tier-1 host runs today, measured by CI on that host — and
what refuses, by name. D35 names the matrix; this page is the state of
it. Floors (`cargo xtask lane-coverage`) are per-platform and measured,
never inherited (s59): the linux, macOS, and windows lines in
`xtask/src/main.rs` are three separate measurements.

| host | native tier (`wolf build` / `wolf run`) | release tier (`--release`) | task layer, procs, channels, `when` | io reactor (`net` deadlines, async fs) | `os.signal` | debugger |
|---|---|---|---|---|---|---|
| linux x86-64 | yes (s28) | yes (s41) | yes (s32) | epoll (s35) | yes (s114) | gdb transcripts |
| macOS aarch64 | yes (s59) | yes (s127) | yes (s59) | kqueue (s59) | yes (s59) | lldb + dSYM |
| windows x86-64 | **yes — bring-up (s60a)** | refuses by name (s60c) | **yes (s60b)** | WSAPoll (s60b; IOCP: s60c) | **yes (s60b)** — Ctrl+C/Break/close; external reload/upgrade the named gap | none (s60c: DWARF-in-COFF + lldb) |


## windows x86-64 — the s60a bring-up, the s60b task layer

The learner's bar, and what meets it: on a Windows 10/11 x64 box with
the release archive unpacked, `wolf build hello.lu` produces
`hello.exe`; it runs and prints; `wolf run hello.lu` does the same; a
program that traps reports `wolf-trap: <kind>` (with its `  at
file:line:col` site line) on stderr and exits **134** — the same
number as every other native host (D70: the trap is a call into the
runtime that ends in `ExitProcess(134)`; nothing about it is a signal,
so nothing about the number is signal arithmetic); `wolf --version`
tells the truth about the build. The pipeline behind it: cranelift
emits COFF objects under the MSVC x64 calling convention
(`WindowsFastcall`), the driver links them against `wolf_rt.lib`
(shipped in every windows archive since v0.2.1) and the import
libraries a Rust staticlib needs, and the C runtime's `mainCRTStartup`
calls wolf's `main` shim.

### What serves

- `wolf build`, `wolf run`, `wolf conform-run --native`, `wolf test`
  on the native lane; `--emit=obj|wir`; the whole static ladder and
  the checked lane (`--checked`) — those never touched the host.
- The runtime surface that is plain Rust std underneath: `print`,
  strings, lists, json, `fs`, `os` (env, cwd, exe, exit, `spawn`/
  `wait`/`kill` of child processes), `time`, `random`
  (`BCryptGenRandom`), regions and the region ledger, sited traps.
- **The task layer (s60b)**: `spawn`/scopes, `proc`, channels and
  `select`, `sync`/`when`, `region_transfer`, `wolf test --schedules`.
  Workers are Win32 threads on the kernel's own reserve-and-guard
  stacks — `CreateThread` with `STACK_SIZE_PARAM_IS_A_RESERVATION` at
  `WOLF_TASK_STACK` (8 MiB default): `VirtualAlloc(MEM_RESERVE)` plus
  the `PAGE_GUARD` page ntdll walks down on first touch, the same
  reserve-large/commit-on-fault posture the unix `mmap` spans build by
  hand. Windows offers no thread on memory the runtime mapped (there
  is no `pthread_attr_setstack`; the TEB's bounds are the kernel's),
  so the spans are not pooled across threads — a worker keeps its
  stack for life, which the pool's own lifetime already guarantees —
  and idle trim (`MEM_DECOMMIT` beneath the parked frame + a re-armed
  guard, the `_resetstkoflw` shape) is s60c's.
- **Stack overflow in a task reports in wolf's voice (s60b)**:
  `wolf-rt: stack overflow in task '<name>'` on stderr, exit **134** —
  a vectored exception handler (the one place a VEH is genuinely
  needed on this host: an overflow is the kernel raising
  `STATUS_STACK_OVERFLOW` on the guard page, never a call) installed
  at pool init, matching only that status, terminating the process
  through `TerminateProcess` with no unwinding through wolf frames and
  no containment (an overflow is process death on every host —
  `stack-overflow` is not in the closed trap vocabulary, so it is
  never a proc's `fault(kind)`). The overflowing thread reports from
  the stack guarantee `SetThreadStackGuarantee` holds back (the
  altstack twin). The main thread reports too once the pool is up —
  `wolf-rt: stack overflow`, same number — because the handler is
  process-wide.
- **`os.signal` (s60b)** over `SetConsoleCtrlHandler`: `CTRL_C` and
  `CTRL_CLOSE` → `terminate`, `CTRL_BREAK` → `quit` — the
  `[os.signal.platform]` table. The console handler runs on a system
  thread in normal context and enqueues the meaning directly (no
  self-pipe, no drain thread); a listened meaning is consumed, an
  unlistened one is left to the console's default disposition.
  `os_signal_raise` is an **in-process** loopback here —
  `GenerateConsoleCtrlEvent` is console-wide and would reach every
  process on the console, the CI shell included — so the loopback
  witness and a program's own reload path work for every meaning.
- **The reactor (s60b)** behind the s35 interface, on `WSAPoll`: the
  `net` parking calls (`accept`/`read`/`write`) await readiness in the
  reactor thread with the pool compensating, kill teardown reaches
  them, and `net_deadline` arms the timer wheel so the `timeout` row
  fires (`corpus/net/read_deadline.lu` answers `timeout`, the same
  verdict as linux/macOS). The poller keeps the armed list itself
  (`WSAPoll` has no kernel set) and rebuilds the array per wake; the
  wake token is a self-connected loopback UDP socket. **Measured** on
  windows-latest: 24 tasks each parked on a 200 ms read deadline against a silent peer resolved **24/24 `timeout` in 207 ms wall** (4 cores; probe run 33614917814) — the deadlines fired at the deadline, and the poller's whole cost across 24 armed sockets and 24 timers was the 7 ms above the budget. That is the corpus's shape — a handful
  of sockets per program, deadlines that resolve at the deadline —
  and it is why the v1 is `WSAPoll` and not IOCP: completion ports
  are the many-socket scale rung and the only road to async file io,
  neither of which any row needs today; they land at s60c behind the
  same seam, readiness adapted underneath exactly as kqueue and
  `WSAPoll` were.
- The C membrane for scalars and pointers (`extern "c"` /
  `export` with `int`, `float`, pointer parameters and results).

### What refuses, by name

Every refusal below is a *named* one — exit 2 with a message that says
which sprint owns it, or a `refused@wir` row in `lane-coverage` — never
a link error, never a silent stub.

- **`wolf build --release`** (the LLVM tier): "this host cannot run the
  release tier" — s60c (`ReleaseTarget::WindowsX64`, clang-cl/lld).
- **External `reload`/`upgrade` signals**: `SIGHUP`/`SIGUSR2` have no
  Windows analog — a wws-shaped program on Windows takes reload and
  upgrade over a control channel (ws04's ungated half). Self-raise of
  either reaches a listener; an unlistened self-raise of them is
  dropped (there is no disposition to deliver to), and of
  `terminate`/`quit` ends the process as Ctrl+C would
  (`STATUS_CONTROL_C_EXIT`).
- **Aggregates by value across the C membrane** (`extern "c"` /
  `export` with struct parameters or results): refused by shape —
  the MSVC rules (1/2/4/8-byte composites as bits in a register,
  larger by pointer to a caller-owned copy) land with the s60
  campaign's `cl.exe` differential, never as a guess. Scalars and
  pointers cross.
- **The debugger story**: the lld-link flavors keep wolf's DWARF in
  the PE (`/DEBUG:DWARF`); nothing consumes it yet. `link.exe` drops
  it. s60c (DWARF-in-COFF + lldb).
- **Stack overflow in a program that never spawns** dies as
  `STATUS_STACK_OVERFLOW` (0xC00000FD), not in wolf's voice — the
  reporter is installed at pool init (D15: no spawn, no handler),
  measured on the runner.
- **Async file io** (the reactor's fs flavor): IOCP's — s60c.
- **Unix-domain sockets** (`net_listen_unix`/`net_connect_unix`,
  `[os.net.unix]`, s136): the `unsupported` row, by name — never a
  bare `io`. `AF_UNIX` exists on Windows since 10 1803 and
  `wolf_rt`'s windows test suite measures the kernel's answer on the
  runner (`socket(AF_UNIX, SOCK_STREAM, 0)` through ws2_32 —
  `shim_unix_refuses_by_name_and_measures_af_unix`), but `std::net`
  has no unix-domain surface on this host and the runtime carries no
  winsock binding beyond `WSAPoll` (D15), so the serving rung is
  named here rather than claimed. A lobo-shaped program keeps its
  loopback-TCP + token control endpoint on this host (wolf-lang#227).

### The floor line

`cargo xtask lane-coverage` on windows-latest at 306d0fc (probe run
33614917814) over 449 entries: checked 259 · native 278 ·
release 0 (dark by design until s60c) · union 295 · all-three 0.
At the s60a bring-up the line read 259/255/0/274/0 with 21 rows
refused by construct name; at s60b **0** remain — the by-name table retired with its last row, and the 36 rows the native lane does not execute here are exactly the 36 it does not execute on macOS (the lanes' own scope gaps, none of them a host's).

### The toolchain a learner needs (and the linker order)

The windows archive ships `wolf.exe`, `wolf_rt.lib`, and the importer
worker. Linking still needs two things Windows does not ship: a COFF
linker, and the **import libraries** (`kernel32.lib`, `ws2_32.lib`,
the UCRT — the Windows SDK — plus `msvcrt.lib` from the MSVC toolset).
Both come with **Visual Studio Build Tools, "Desktop development with
C++"** — the same requirement Rust's own `windows-msvc` toolchain
carries. Bundling the libraries so no install is needed at all is
s47's (mingw-w64 import libraries), named in the refusal. The driver
finds the MSVC environment the way rustc does (a Developer Command
Prompt's own `LIB`, else the newest Visual Studio / Build Tools install
via vswhere and the registry) and hands its `LIB` to whichever linker
wins this order:

1. `WOLF_LINKER` — an explicit path (the `CC` twin), taken as-is.
2. `lld-link` on `PATH`, else the LLVM Visual Studio bundles
   (`VC\Tools\Llvm\x64\bin\lld-link.exe`).
3. rustup's bundled `rust-lld` (`<sysroot>\lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe`,
   driven `-flavor link`) — a learner with a Rust toolchain has this
   without installing anything else.
4. MSVC `link.exe`.
5. A named refusal saying what to install.

`wolf build --verbose` names the choice. The link line is the
`link.exe` dialect on every rung: `/NOLOGO /SUBSYSTEM:CONSOLE /OPT:REF
/Brepro` (section GC as on unix; a zeroed PE timestamp so cached and
`--no-cache` builds are bit-identical), the objects, `wolf_rt.lib`, and
`kernel32 ntdll userenv ws2_32 dbghelp bcrypt msvcrt` — exactly what
`rustc --print native-static-libs` names for the runtime.

### The road (the s60 campaign)

- **s60b** (landed, 2026-09-02) — the runtime crossed: the task
  layer on the kernel's reserve-and-guard stacks with the VEH
  reporter, `os.signal` over `SetConsoleCtrlHandler`, the reactor's
  `WSAPoll` rung behind the s35 interface (`net` deadlines serve).
- **s60c** — what s60b named: the LLVM release tier on windows
  (`ReleaseTarget::WindowsX64`, clang-cl/lld); IOCP behind the same
  reactor seam (async fs, the many-socket rung); idle stack trim on
  windows (`MEM_DECOMMIT` + a re-armed guard) and the s36
  commit-failure injection there; then the campaign map — the
  in-process COFF/PE writer with `.pdata`/`.xdata`, the MSVC x64
  aggregate ABI fuzzed against `cl.exe`, `import c "windows.h"`
  through bundled mingw-w64 headers and import libraries (s47 —
  which is also the day the Build Tools requirement above retires),
  DWARF-in-COFF + lldb.
