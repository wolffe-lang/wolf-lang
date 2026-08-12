
## lsp-windows-stdio (2026-08-10)

The three `lsp_one_truth` publish-waiting tests wedge on Windows CI. The
server receives `didOpen` over piped stdio and never publishes: no panic on
inherited stderr, alive until the 60s watchdog kills it. URIs are verified
parseable (`file:///D:/…`), macOS and Linux are green, and the in-process
wolf_lsp transcript suite is green on Windows too, so the protocol logic is
answering and the stdio/threading layer is where to look. Quarantined with
`#[cfg_attr(windows, ignore)]` until there is real signal (an env-gated
server trace, or a Windows box). s57 owns it; that sprint rebuilds this
layer.

## release-tier fact channels owed to s42 (2026-08-12)

Three gaps in the s41 fact emission. Each waits on an analysis s41 does not
own.

1. Call purity and termination bits (`memory(...)`, `willreturn`, `nosync`).
   WIR carries no purity facts at v0, so the LLVM tier emits none. The
   lowering rule is a one-liner once s42's analyses mint them.
2. `deref` facts with scaled sizes (`8x%n`). `dereferenceable(n)` wants a
   constant, so scaled claims are dropped. Revisit when s42's range analysis
   can bound the count.
3. `!invariant.load` detection is direct-def only. A frozen token that
   reaches a load through a block parameter is not traced, so those loads
   lose the fact. This is sound and conservative; a small token-origin
   dataflow closes it.

All three are performance-only by construction (D2: dropping facts is always
sound).
