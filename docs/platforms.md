# Platforms

What each tier-1 host runs today, measured by CI on that host, and
what refuses. D35 sets the matrix; this page is the state of it. Floors
(`cargo xtask lane-coverage`) are measured per platform (s59): the
linux, macOS, and windows lines in `xtask/src/main.rs` are three
separate measurements.

| host | native tier (`wolf build` / `wolf run`) | release tier (`--release`) | task layer, procs, channels, `when` | io reactor (`net` deadlines, async fs) | `os.signal` | debugger |
|---|---|---|---|---|---|---|
| linux x86-64 | yes (s28) | yes (s41) | yes (s32) | epoll (s35) | yes (s114) | gdb transcripts |
| macOS aarch64 | yes (s59) | yes (s127) | yes (s59) | kqueue (s59) | yes (s59) | lldb + dSYM |
| windows x86-64 | **yes — bring-up (s60a)** | refuses by name (s60c) | **yes (s60b)** | WSAPoll (s60b; IOCP: s60c) | **yes (s60b)** — Ctrl+C/Break/close; external reload/upgrade the named gap | none (s60c: DWARF-in-COFF + lldb) |


## windows x86-64 — the s60a bring-up, the s60b task layer

Here is the learner's bar and what meets it. On a Windows 10/11 x64
box with the release archive unpacked, `wolf build hello.lu` produces
`hello.exe`; it runs and prints; `wolf run hello.lu` does the same; a
program that traps reports `wolf-trap: <kind>` (with its `  at
file:line:col` site line) on stderr and exits 134, the same number as
every other native host (D70: the trap is a call into the runtime that
ends in `ExitProcess(134)`; nothing about it is a signal, so nothing
about the number is signal arithmetic); `wolf --version` reports the
build it came from. The pipeline behind it: cranelift
emits COFF objects under the MSVC x64 calling convention
(`WindowsFastcall`), the driver links them against `wolf_rt.lib`
(shipped in every windows archive since v0.2.1) and the import
libraries a Rust staticlib needs, and the C runtime's `mainCRTStartup`
calls wolf's `main` shim.

### What serves

- `wolf build`, `wolf run`, `wolf conform-run --native`, `wolf test`
  on the native lane; `--emit=obj|wir`; the whole static ladder and
  the checked lane (`--checked`); those never touched the host.
- The runtime surface that is plain Rust std underneath: `print`,
  strings, lists, json, `fs`, `os` (env, cwd, exe, exit, `spawn`/
  `wait`/`kill` of child processes), `time`, `random`
  (`BCryptGenRandom`), regions and the region ledger, sited traps.
- The task layer (s60b): `spawn`/scopes, `proc`, channels and
  `select`, `sync`/`when`, `region_transfer`, `wolf test --schedules`.
  Workers are Win32 threads on the kernel's own reserve-and-guard
  stacks, `CreateThread` with `STACK_SIZE_PARAM_IS_A_RESERVATION` at
  `WOLF_TASK_STACK` (8 MiB default): `VirtualAlloc(MEM_RESERVE)` plus
  the `PAGE_GUARD` page ntdll walks down on first touch, the same
  reserve-large/commit-on-fault posture the unix `mmap` spans build by
  hand. Windows offers no thread on memory the runtime mapped (there
  is no `pthread_attr_setstack`; the TEB's bounds are the kernel's),
  so the spans are not pooled across threads. A worker keeps its stack
  for life, which the pool's own lifetime already guarantees, and idle
  trim (`MEM_DECOMMIT` beneath the parked frame + a re-armed guard,
  the `_resetstkoflw` shape) is s60c's.
- Stack overflow in a task reports in wolf's voice (s60b):
  `wolf-rt: stack overflow in task '<name>'` on stderr, exit 134. A
  vectored exception handler does it (the one place this host needs a
  VEH: an overflow is the kernel raising `STATUS_STACK_OVERFLOW` on
  the guard page, never a call), installed at pool init, matching only
  that status, terminating the process through `TerminateProcess` with
  no unwinding through wolf frames and no containment (an overflow is
  process death on every host, and `stack-overflow` is not in the
  closed trap vocabulary, so it is never a proc's `fault(kind)`). The
  overflowing thread reports from the stack guarantee
  `SetThreadStackGuarantee` holds back (the altstack twin). The main
  thread reports too once the pool is up (`wolf-rt: stack overflow`,
  same number), because the handler is process-wide.
- `os.signal` (s60b) over `SetConsoleCtrlHandler`: `CTRL_C` and
  `CTRL_CLOSE` → `terminate`, `CTRL_BREAK` → `quit`, the
  `[os.signal.platform]` table. The console handler runs on a system
  thread in normal context and enqueues the meaning directly (no
  self-pipe, no drain thread); a listened meaning is consumed, an
  unlistened one is left to the console's default disposition.
  `os_signal_raise` is an in-process loopback here, since
  `GenerateConsoleCtrlEvent` is console-wide and would reach every
  process on the console, the CI shell included. That way the loopback
  witness and a program's own reload path work for every meaning.
- The reactor (s60b) behind the s35 interface, on `WSAPoll`: the
  `net` parking calls (`accept`/`read`/`write`) await readiness in the
  reactor thread with the pool compensating, kill teardown reaches
  them, and `net_deadline` arms the timer wheel so the `timeout` row
  fires (`corpus/net/read_deadline.lu` answers `timeout`, the same
  verdict as linux/macOS). Since s138 (`[os.net.accept]`) a listener
  lives non-blocking here as on the other reactor hosts (`FIONBIO`
  through std), so the accept after a readiness wake takes the
  connection or answers `WSAEWOULDBLOCK` and waits again against the
  same budget, never parks; the accepted socket inherits the mode on
  winsock and is put back to blocking, one posture on every host. The
  race that clause is about cannot be constructed on this host at this
  pin, because the inherit set is refused here, so
  `corpus/net/accept_race.lu` is vacuous here, same stdout. The poller
  keeps the armed list itself
  (`WSAPoll` has no kernel set) and rebuilds the array per wake; the
  wake token is a self-connected loopback UDP socket. Measured on
  windows-latest: 24 tasks each parked on a 200 ms read deadline
  against a silent peer resolved 24/24 `timeout` in 207 ms wall
  (4 cores; probe run 33614917814). The deadlines fired at the
  deadline, and the poller's whole cost across 24 armed sockets and 24
  timers was the 7 ms above the budget. That is the corpus's shape, a
  handful of sockets per program with deadlines that resolve at the
  deadline, and it is why the v1 is `WSAPoll`. Completion ports are
  the many-socket scale rung and the only road to async file io,
  neither of which any row needs today; they land at s60c behind the
  same seam, readiness adapted underneath as kqueue and `WSAPoll`
  were.
- Readiness over a set (`net_wait`, `[os.net.wait]`, s137):
  `WSAPoll` again, this time in the program's own hands, above the
  reactor. A spawn-free serving loop asks which of its sockets can be
  read and blocks until one can. This is the s137 surface that serves
  on this host. Every tier-1 kernel answers the question
  (`poll(2)` elsewhere), so `net_wait` is the same call with the same
  row set everywhere, on the native tier and on the checked machine
  alike, and `[os.net.wait]` names no host.
- The schedulable core count (`os_cpus`, `[os.cpus]`, s137):
  `GetSystemInfo` here, through the one call that reads the right
  thing on every host (`available_parallelism`, which honours a cgroup
  quota and an affinity mask where those exist). Like `net_wait` it
  refuses on no host, and the native lane and the checked machine
  answer the same number, so `workers auto` means one thing.
- The C membrane for scalars and pointers (`extern "c"` /
  `export` with `int`, `float`, pointer parameters and results).

### What refuses, by name

Every refusal below is a *named* one: exit 2 with a message that says
which sprint owns it, or a `refused@wir` row in `lane-coverage`. None
of them is a link error or a silent stub.

- `wolf build --release` (the LLVM tier): "this host cannot run the
  release tier", s60c (`ReleaseTarget::WindowsX64`, clang-cl/lld).
- External `reload`/`upgrade` signals: `SIGHUP`/`SIGUSR2` have no
  Windows analog, so a wws-shaped program on Windows takes reload and
  upgrade over a control channel (ws04's ungated half). Self-raise of
  either reaches a listener; an unlistened self-raise of them is
  dropped (there is no disposition to deliver to), and of
  `terminate`/`quit` ends the process as Ctrl+C would
  (`STATUS_CONTROL_C_EXIT`).
- Aggregates by value across the C membrane (`extern "c"` /
  `export` with struct parameters or results): refused by shape. The
  MSVC rules (1/2/4/8-byte composites as bits in a register, larger by
  pointer to a caller-owned copy) land with the s60 campaign's
  `cl.exe` differential. Scalars and pointers cross.
- The debugger story: the lld-link flavors keep wolf's DWARF in the PE
  (`/DEBUG:DWARF`); nothing consumes it yet. `link.exe` drops it. s60c
  (DWARF-in-COFF + lldb).
- Stack overflow in a program that never spawns dies as
  `STATUS_STACK_OVERFLOW` (0xC00000FD), outside wolf's voice: the
  reporter is installed at pool init (D15: no spawn, no handler),
  measured on the runner.
- Async file io (the reactor's fs flavor): IOCP's, s60c.
- Unix-domain sockets (`net_listen_unix`/`net_connect_unix`,
  `[os.net.unix]`, s136): the `unsupported` row, and never a bare
  `io`. `AF_UNIX` exists on Windows since 10 1803 and `wolf_rt`'s
  windows test suite measures the kernel's answer on the runner
  (`socket(AF_UNIX, SOCK_STREAM, 0)` through ws2_32,
  `shim_unix_refuses_by_name_and_measures_af_unix`), but `std::net`
  has no unix-domain surface on this host and the runtime carries no
  winsock binding beyond `WSAPoll` (D15), so this page names the
  serving rung instead of the runtime claiming it. A lobo-shaped
  program keeps its loopback-TCP + token control endpoint on this host
  (wolf-lang#227).
- `reuse_port` (`net_listen_with`'s option, `[os.net.listen.opts]`,
  s137): the `unsupported` row, and here the refusal is a choice.
  Windows has no `SO_REUSEPORT`. `SO_REUSEADDR` is spelled the same in
  a manual and means something else entirely there: it lets any
  process take a port another process already bound, and promises
  nothing about which of them a connection reaches. Aliasing the
  option to it would hand a prefork server a silent hijack where it
  asked for a shared queue, so the runtime refuses. `wolf_rt`'s
  windows test suite measures what the alias would have meant on the
  runner, and the answer is worse than the manual suggests: two ws2_32
  sockets both carrying `SO_REUSEADDR` bind one port, and every one of
  16 dials went to the first socket bound, none to the second (16/0,
  measured). So the second bind succeeds and then serves nothing: a
  worker that "joined the group" would sit idle forever with no error
  to report. `net_listen_with`'s option-less shape serves on this host
  like `net_listen`, backlog hint and all, and a held address answers
  `exists` (wolf-lang#234).
- Descriptor inheritance across a spawn (`os_spawn_with`'s inherit
  set and `net_adopt_listener`, `[os.proc.inherit]`, s137): the
  `unsupported` row, and the gap is deeper than #235 guessed. The
  runtime's windows suite measures it on the runner
  (`windows_socket_handle_inheritance_measured`): a listener's
  `SOCKET` marked `HANDLE_FLAG_INHERIT`, its value named to a
  re-executed child, is not a usable socket in that child at all.
  `std::process::Command` publishes only its stdio handles to the
  child on this host, so the descriptor does not cross the spawn the
  runtime performs. Two things are therefore missing: the crossing
  itself, and the numbering the clause promises (a `SOCKET` is not a
  small stable descriptor a parent can hand over by position, "the
  listener is 3"). Both want the same next campaign: a
  `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` spawn of the runtime's own,
  which `Command` cannot express. The serving rung is named here for
  it. `os_spawn_with` with an empty set already serves on this host,
  as `os_spawn` with the program named apart from its arguments, so a
  windows port of a prefork master needs the handoff and not the spawn
  (wolf-lang#235).

### The floor line

`cargo xtask lane-coverage` on windows-latest at 306d0fc (probe run
33614917814) over 449 entries: checked 261 · native 278 ·
release 0 (dark by design until s60c) · union 295 · all-three 0.
At the s60a bring-up the line read 259/255/0/274/0 with 21 rows
refused by construct name; at s60b 0 remain. (Checked read 259 here
until 2026-09-08. That is the s60a FLOOR, not the s60b measurement:
the run's own last line is `floors held (checked/native/release/union/
all-three >= 259/255/0/274/0)` and the line above it is `checked
executes 261 at run`. Re-read from run 33614917814's log; the 0.2.3
changelog entry's 261/278/0/295/0 was the one that agreed with it.)
The by-name table retired with its last row, and the 36 rows the native lane does not
execute here are the same 36 it does not execute on macOS (the lanes'
own scope gaps, none of them a host's).

### The toolchain a learner needs (and the linker order)

The windows archive ships `wolf.exe`, `wolf_rt.lib`, and the importer
worker. Linking still needs two things Windows does not ship: a COFF
linker, and the import libraries (`kernel32.lib`, `ws2_32.lib`, the
UCRT from the Windows SDK, plus `msvcrt.lib` from the MSVC toolset).
Both come with Visual Studio Build Tools, "Desktop development with
C++", the same requirement Rust's own `windows-msvc` toolchain
carries. Bundling the libraries so no install is needed at all is
s47's (mingw-w64 import libraries), named in the refusal. The driver
finds the MSVC environment the way rustc does (a Developer Command
Prompt's own `LIB`, else the newest Visual Studio / Build Tools install
via vswhere and the registry) and hands its `LIB` to whichever linker
wins this order:

1. `WOLF_LINKER`: an explicit path (the `CC` twin), taken as-is.
2. `lld-link` on `PATH`, else the LLVM Visual Studio bundles
   (`VC\Tools\Llvm\x64\bin\lld-link.exe`).
3. rustup's bundled `rust-lld` (`<sysroot>\lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe`,
   driven `-flavor link`); a learner with a Rust toolchain has this
   without installing anything else.
4. MSVC `link.exe`.
5. A named refusal saying what to install.

`wolf build --verbose` names the choice. The link line is the
`link.exe` dialect on every rung: `/NOLOGO /SUBSYSTEM:CONSOLE /OPT:REF
/Brepro` (section GC as on unix; a zeroed PE timestamp so cached and
`--no-cache` builds are bit-identical), the objects, `wolf_rt.lib`, and
`kernel32 ntdll userenv ws2_32 dbghelp bcrypt msvcrt`, the list
`rustc --print native-static-libs` names for the runtime.

### The road (the s60 campaign)

- s60b (landed, 2026-09-02): the runtime crossed, with the task layer
  on the kernel's reserve-and-guard stacks and the VEH reporter,
  `os.signal` over `SetConsoleCtrlHandler`, and the reactor's
  `WSAPoll` rung behind the s35 interface (`net` deadlines serve).
- s60c: what s60b named. The LLVM release tier on windows
  (`ReleaseTarget::WindowsX64`, clang-cl/lld); IOCP behind the same
  reactor seam (async fs, the many-socket rung); idle stack trim on
  windows (`MEM_DECOMMIT` + a re-armed guard) and the s36
  commit-failure injection there; then the campaign map, which is the
  in-process COFF/PE writer with `.pdata`/`.xdata`, the MSVC x64
  aggregate ABI fuzzed against `cl.exe`, `import c "windows.h"`
  through bundled mingw-w64 headers and import libraries (s47, which
  is also the day the Build Tools requirement above retires), and
  DWARF-in-COFF + lldb.
