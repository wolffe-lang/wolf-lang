
## lsp-windows-stdio (2026-08-10)

The three `lsp_one_truth` publish-waiting tests wedge on Windows CI: the
server receives `didOpen` over piped stdio and never publishes — no panic
on inherited stderr, alive until the 60s watchdog kills it. URIs verified
parseable (`file:///D:/…`); macOS/Linux green; the in-process wolf_lsp
transcript suite is green ON Windows, so the fault is in the stdio/threading
layer, not the protocol logic. Quarantined `#[cfg_attr(windows, ignore)]`
pending investigation with real signal (env-gated server trace or a
Windows box). Owed by s57 at the latest — residency rebuilds this layer.

## release-tier fact channels owed to s42 (2026-08-12)

Three honest gaps in the s41 fact emission, each waiting on analyses
s41 does not own: (1) call purity/termination bits (`memory(...)`,
`willreturn`, `nosync`) — WIR carries no purity facts at v0, so the
LLVM tier emits none; s42's analyses mint them and the lowering rule
is a one-liner once they exist. (2) `deref` facts with SCALED sizes
(`8x%n`) — `dereferenceable(n)` wants a constant, so scaled claims are
dropped; revisit when s42's range analysis can bound the count.
(3) `!invariant.load` detection is direct-def only: a frozen token
that reaches a load THROUGH a block parameter is not traced, so those
loads lose the fact (sound, just conservative); a small token-origin
dataflow closes it. All three are performance-only by construction
(D2: dropping facts is always sound).
