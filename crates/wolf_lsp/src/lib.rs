//! `wolf lsp` — the transport-only LSP shim over the `wolf_query`
//! contract (s52 v0, D34: the compiler *is* the language server).
//!
//! Strictly a serialization boundary (matklad's rule, report 09): this
//! crate knows JSON-RPC, the LSP lifecycle, position encodings and
//! wire shapes; it computes nothing. Every answer comes from a
//! [`wolf_query::Snapshot`], so editor diagnostics and CLI builds are
//! one truth by construction.
//!
//! # Capability set
//!
//! `positionEncoding` (utf-8 preferred / utf-16 default / utf-32),
//! full-text sync with didSave, `publishDiagnostics` on
//! open/change(debounced)/save, `textDocument/hover` (typed subset),
//! `textDocument/documentSymbol`, `textDocument/formatting`
//! (whole-document, via the one formatter), `textDocument/codeAction`
//! (machine-applicable fix-its as quickfixes),
//! `textDocument/completion` (s122: keywords, in-scope names, member
//! completion after `.` — trigger character `.`, which is both member
//! access and the module path separator; the incomplete-buffer
//! contract lives in `wolf_query::Snapshot::completions`),
//! `textDocument/definition`, `textDocument/references`,
//! `textDocument/prepareRename` + `textDocument/rename` (s133: the
//! navigation trio, answered from the binding table the resolver and
//! checker keep — never a textual search; `LocationLink[]` when the
//! client declares `linkSupport`, `documentChanges` when it declares
//! `workspaceEdit.documentChanges`; rename refuses BY NAME with
//! `RequestFailed` for keywords, builtins, prelude names, std/`import
//! c` symbols and modules — the refusal set is `wolf_query`'s
//! `navigate` module docs), `$/cancelRequest` honored into the query
//! layer.
//!
//! **Still absent, refused by name** (MethodNotFound with the method
//! named — never faked, per the track's rule): signature help,
//! semantic tokens, inlay hints, range formatting, workspace symbols,
//! the pull-diagnostics model.
//!
//! Transport is rust-analyzer's `lsp-server` (L1: framing, handshake,
//! IO threads — the parts with sharp edges — and no policy). The main
//! loop, concurrency and cancellation policy live in [`server`];
//! UTF-16/UTF-32 translation lives in [`positions`] and nowhere else.

mod convert;
pub mod positions;
mod server;

pub use server::main_loop;

/// Serve LSP over stdio until `exit`. The driver's `wolf lsp` entry.
///
/// Returns the exit code the LSP lifecycle demands: `0` when `exit`
/// followed `shutdown`, `1` for a bare `exit` — the driver passes it to
/// `std::process::exit` unchanged.
pub fn run_stdio() -> Result<i32, Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    let code = main_loop(connection)?;
    io_threads.join()?;
    Ok(code)
}
