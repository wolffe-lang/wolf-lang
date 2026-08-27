# The signal-reception seam (wolf-wws ws04 consumes this)

Frozen by s114 (c30-signals, wolf-lang#126). This is the stable surface
wolf-wws `-s reload|quit|stop` and the zero-downtime upgrade path listen
on. Normative spec: `spec/11-os.md` §1 `[os.signal]`. Runtime:
`crates/wolf_rt/src/signal.rs`. Checked lane: `crates/wolf_mem/src/
ubcheck.rs` (`os_builtin`).

## The three builtins (the whole surface)

```
os_signal_listen(set: int) -> () ! {io}   // register interest
os_signal_wait(set: int)   -> int ! {io}  // park until one arrives; returns it
os_signal_raise(sig: int)  -> () ! {io}   // deliver one meaning to THIS process
```

`set` is a bitmask of MEANINGS; `os_signal_wait` returns the single
meaning that arrived. Nothing is received without a prior `listen`.
There is no ambient global handler and **no wolf code ever runs in a
signal handler** (the runtime's async-signal-safe trampoline writes one
byte to a self-pipe; a drain thread delivers the event to the parked
task — `[os.signal.model]`).

## The meaning set (frozen; `[os.signal.set]`)

| meaning     | bit | unix signal | wws use                            |
|-------------|-----|-------------|------------------------------------|
| `RELOAD`    | 1   | `SIGHUP`    | re-read config, drain old workers  |
| `TERMINATE` | 2   | `SIGTERM`   | fast shutdown                      |
| `QUIT`      | 4   | `SIGQUIT`   | graceful shutdown after drain      |
| `UPGRADE`   | 8   | `SIGUSR2`   | binary-swap / zero-downtime upgrade |

wws wraps these bits with named constants in its own module (the surface
is the builtin ABI; the names are wws's, exactly as std.process wraps
`os_spawn`). New meanings append with the next free bit; nothing
renumbers.

## How ws04 uses it

```
os_signal_listen(RELOAD | TERMINATE | QUIT | UPGRADE)?
loop {
    let sig = os_signal_wait(RELOAD | TERMINATE | QUIT | UPGRADE)?
    // act on sig: reload / drain+quit / stop / upgrade
}
```

A supervisor task parks on `os_signal_wait` (a real thread parked with
blocking compensation — the c19 model). When the surface composes with
`select` (a later widening), ws04 can `select` a signal against its
other work; today it waits. This sprint delivers the EVENT; ws04 owns
the reload/drain/upgrade LOGIC (its ungated half).

## Blocking honesty (`[os.signal.wait]`)

- The wait parks a thread; delivery reaches it through the runtime's
  self-pipe drain and the pool's blocking compensation.
- Empty / all-unmapped set → `io` at once (never a hang).
- A KILL teardown point: a killed supervisor terminates at the wait, it
  does not hang on a signal that never comes. Plain cancellation keeps
  waiting (the `{io}` row has no cancellation row — the net kill-only
  posture).

## Platform matrix (`[os.signal.platform]`)

- **Linux:** full set, the running implementation (gated with the
  native concurrency layer — `reactor`/`net`/`task` are Linux at this
  campaign stage). macOS/BSD delivery widens with the task-layer port.
- **Windows (tier-1):** POSIX signals do not exist. The meanings map to
  `SetConsoleCtrlHandler` — CTRL_C→`TERMINATE`, CTRL_BREAK→`QUIT`,
  CTRL_CLOSE→`TERMINATE`. `RELOAD`/`UPGRADE` have **no Windows analog**
  — a NAMED gap: ws04's Windows reload/upgrade uses its control channel
  (the ungated half), not a signal. Console delivery lands with the
  Windows native backend; the mapping is spec'd now so the seam is a
  meaning seam, portable by construction.

## Determinism (`[os.signal.det]`)

Signal arrival is external non-determinism, EXCLUDED from
`--schedules`/`--replay`: it emits no `sched-ev` record. ws04 tests that
need reproducibility drive the loopback (`os_signal_raise` in-process),
whose OUTPUT is causally pinned; they do not replay real OS signals.
