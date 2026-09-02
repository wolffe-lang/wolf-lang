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
| windows x86-64 | **yes — bring-up (s60a)** | refuses by name (s60b) | refuses by name (s60b) | blocking sockets, no deadlines (s60b: IOCP) | refuses by name (s60b) | none (s60: DWARF-in-COFF + lldb) |

## windows x86-64 — the s60a bring-up

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
- `net` in its **blocking** posture — the v0 fallback the module
  documents for hosts without a reactor: `listen`/`accept`/`connect`/
  `read`/`write` block in the syscall; `net_deadline` answers `io`
  (never an inert deadline).
- The C membrane for scalars and pointers (`extern "c"` /
  `export` with `int`, `float`, pointer parameters and results).

### What refuses, by name

Every refusal below is a *named* one — exit 2 with a message that says
which sprint owns it, or a `refused@wir` row in `lane-coverage` — never
a link error, never a silent stub.

- **`wolf build --release`** (the LLVM tier): "this host cannot run the
  release tier" — s60b.
- **The task layer**: `spawn`/scopes, `proc`, channels and `select`,
  `sync`/`when`, `region_transfer`. `wolf_rt` compiles for windows
  without its task layer (pooled `mmap` stacks, the pthread pool, the
  guard-page fault reporter are POSIX); the codegen refuses the
  construct by name so the row counts. s60b: the task layer on
  `VirtualAlloc(MEM_RESERVE)` stacks and the IOCP reactor.
- **`os.signal`** listen/wait/raise: no POSIX signals; the
  `SetConsoleCtrlHandler` mapping is spec'd (`[os.signal.platform]`)
  and lands with the task layer.
- **Aggregates by value across the C membrane** (`extern "c"` /
  `export` with struct parameters or results): refused by shape —
  the MSVC rules (1/2/4/8-byte composites as bits in a register,
  larger by pointer to a caller-owned copy) land with the s60
  campaign's `cl.exe` differential, never as a guess. Scalars and
  pointers cross.
- **The debugger story**: the lld-link flavors keep wolf's DWARF in
  the PE (`/DEBUG:DWARF`); nothing consumes it yet. `link.exe` drops
  it. s60 (DWARF-in-COFF + lldb).
- **Stack overflow in the main thread** dies as
  `STATUS_STACK_OVERFLOW` (0xC00000FD), not in wolf's voice — the
  guard-page reporter is the task layer's.

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

- **s60b** — the runtime crosses: the task layer on `VirtualAlloc`
  stacks, the IOCP reactor behind the s35 interface (`net` deadlines,
  async fs), `os.signal` over `SetConsoleCtrlHandler`, the LLVM
  release tier on windows.
- **s60c** — the compiler owns the image: the in-process COFF/PE
  writer with `.pdata`/`.xdata`, the MSVC x64 aggregate ABI fuzzed
  against `cl.exe`, `import c "windows.h"` through bundled mingw-w64
  headers and import libraries (s47 — which is also the day the
  Build Tools requirement above retires), DWARF-in-COFF + lldb.
