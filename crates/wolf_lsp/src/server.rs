//! The main loop: lifecycle, routing, debouncing, cancellation.
//!
//! Shape per report 09 / rust-analyzer: one explicit loop over the
//! connection's receiver (no spawned-future soup — every trigger is
//! visible here), writes applied on the loop thread (so they order
//! against each other), reads dispatched to worker threads against
//! immutable [`wolf_query::Snapshot`]s.
//!
//! Cancellation is honored **into the query layer** (the tower-lsp
//! failure named in report 09): `$/cancelRequest` reaches the running
//! query's [`wolf_query::CancelToken`]; the query unwinds at its next
//! checkpoint and the request answers `-32800 RequestCancelled`. A
//! request whose snapshot was invalidated by a write instead retries
//! once on fresh state and only then answers `-32801 ContentModified`
//! — never merely because a `didChange` sat in the queue (the spec's
//! negative rule).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{Notification as _, PublishDiagnostics};
use lsp_types::{
    CodeActionOrCommand, CodeActionProviderCapability, CompletionOptions, HoverProviderCapability,
    InitializeParams, OneOf, PublishDiagnosticsParams, SaveOptions, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TextEdit, Url,
};
use wolf_query::{Cancelled, Change, QueryHost, Snapshot};
use wolf_span::{LineIndex, Span};

use crate::convert;
use crate::positions::{self, Encoding};

/// Debounce window for `didChange`-driven re-analysis. Overlay writes
/// apply immediately (position-based requests must see synced text);
/// only diagnostics *publication* waits for the keystroke storm to
/// quiet down.
const DEBOUNCE: Duration = Duration::from_millis(100);

/// `-32800` / `-32801`, from the transport crate's enum.
const REQUEST_CANCELLED: i32 = ErrorCode::RequestCanceled as i32;
const CONTENT_MODIFIED: i32 = ErrorCode::ContentModified as i32;

/// Per-in-flight-request control: the token cancels the query, the
/// flag records that the *client* asked (⇒ `-32800`, not retry).
struct Inflight {
    token: wolf_query::CancelToken,
    client_cancelled: Arc<AtomicBool>,
}

type InflightMap = Arc<Mutex<HashMap<RequestId, Inflight>>>;

/// Cancellations that raced ahead of their request's registration —
/// the worker checks this right after registering, so `$/cancelRequest`
/// is never lost to the spawn window.
type EarlyCancelSet = Arc<Mutex<std::collections::HashSet<RequestId>>>;

/// Outbound message sink — a closure over the connection's sender, so
/// this crate never has to name the transport's channel types.
type SendFn = Arc<dyn Fn(Message) + Send + Sync>;

/// Everything a worker thread needs, cloneable.
#[derive(Clone)]
struct Cx {
    host: Arc<QueryHost>,
    sender: SendFn,
    inflight: InflightMap,
    early_cancel: EarlyCancelSet,
    enc: Encoding,
    /// path → the client's own URI string for open docs (echoed
    /// byte-identically; clients compare URIs as strings).
    open_urls: Arc<Mutex<HashMap<PathBuf, Url>>>,
}

/// Serve one LSP session over `connection` until `exit`. The
/// transport-generic entry: `run_stdio` wraps it for the real server
/// and tests drive it over `Connection::memory()`.
///
/// Returns the process exit code the lifecycle demands: `0` when `exit`
/// followed a `shutdown` request, `1` otherwise (the spec's rule — a
/// bare `exit` is an abnormal end and must not look like success). A
/// client that hangs up without `exit` is scored by the same rule.
pub fn main_loop(connection: Connection) -> Result<i32, Box<dyn std::error::Error + Send + Sync>> {
    let (init_id, init_value) = connection.initialize_start()?;
    let init: InitializeParams = serde_json::from_value(init_value)?;
    let enc = positions::negotiate(&init);
    let capabilities = server_capabilities(enc);
    let result = serde_json::json!({
        "capabilities": capabilities,
        "serverInfo": { "name": "wolf-lsp", "version": env!("CARGO_PKG_VERSION") },
    });
    connection.initialize_finish(init_id, result)?;

    let sender = {
        let s = connection.sender.clone();
        Arc::new(move |m: Message| {
            let _ = s.send(m);
        }) as SendFn
    };
    let cx = Cx {
        host: Arc::new(QueryHost::new()),
        sender,
        inflight: Arc::new(Mutex::new(HashMap::new())),
        early_cancel: Arc::new(Mutex::new(std::collections::HashSet::new())),
        enc,
        open_urls: Arc::new(Mutex::new(HashMap::new())),
    };
    let mut shutdown_requested = false;
    // Docs with unpublished changes: path → deadline.
    let mut dirty: HashMap<PathBuf, Instant> = HashMap::new();

    loop {
        let timeout = dirty
            .values()
            .min()
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        let msg = match connection.receiver.recv_timeout(timeout) {
            Ok(msg) => Some(msg),
            Err(e) if e.is_timeout() => None,
            // Client hung up without `exit`: same lifecycle scoring.
            Err(_) => return Ok(i32::from(!shutdown_requested)),
        };
        // Flush every dirty doc whose debounce window elapsed.
        let now = Instant::now();
        let due: Vec<PathBuf> = dirty
            .iter()
            .filter(|(_, dl)| **dl <= now)
            .map(|(p, _)| p.clone())
            .collect();
        for path in due {
            dirty.remove(&path);
            spawn_publish(&cx, path);
        }
        let Some(msg) = msg else { continue };
        match msg {
            Message::Request(req) => {
                if shutdown_requested {
                    respond(
                        &cx,
                        Response::new_err(
                            req.id,
                            ErrorCode::InvalidRequest as i32,
                            "shutdown already requested".to_string(),
                        ),
                    );
                    continue;
                }
                if req.method == "shutdown" {
                    shutdown_requested = true;
                    respond(&cx, Response::new_ok(req.id, serde_json::Value::Null));
                    continue;
                }
                dispatch_request(&cx, req);
            }
            Message::Notification(note) => match note.method.as_str() {
                // The spec's exit-code rule: success only when a
                // `shutdown` request came first.
                "exit" => return Ok(i32::from(!shutdown_requested)),
                "$/cancelRequest" => cancel_request(&cx, &note),
                "textDocument/didOpen" => {
                    if let Some(path) = did_open(&cx, &note) {
                        dirty.remove(&path);
                        spawn_publish(&cx, path);
                    }
                }
                "textDocument/didChange" => {
                    if let Some(path) = did_change(&cx, &note) {
                        dirty.insert(path, Instant::now() + DEBOUNCE);
                    }
                }
                "textDocument/didSave" => {
                    if let Some(path) = doc_path(&note) {
                        dirty.remove(&path);
                        spawn_publish(&cx, path);
                    }
                }
                "textDocument/didClose" => {
                    if let Some(path) = did_close(&cx, &note) {
                        dirty.remove(&path);
                    }
                }
                _ => {} // unknown notifications drop, per spec
            },
            Message::Response(_) => {} // we send no server→client requests
        }
    }
}

fn server_capabilities(enc: Encoding) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(enc.kind()),
        // Explicit always — the field defaults to *no sync* if omitted
        // (report 09 §Needs work #6).
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                will_save: None,
                will_save_wait_until: None,
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
            },
        )),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        // s122: completion. `.` is wolf's member access AND its module
        // path separator (dotted paths), so one trigger character
        // serves both. No resolve step: items arrive fully formed.
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn respond(cx: &Cx, resp: Response) {
    (cx.sender)(Message::Response(resp));
}

// ------------------------------------------------------ notifications --

fn doc_uri(note: &Notification) -> Option<Url> {
    let uri = note.params.get("textDocument")?.get("uri")?.as_str()?;
    Url::parse(uri).ok()
}

fn doc_path(note: &Notification) -> Option<PathBuf> {
    doc_uri(note)?.to_file_path().ok()
}

fn did_open(cx: &Cx, note: &Notification) -> Option<PathBuf> {
    let uri = doc_uri(note)?;
    let path = uri.to_file_path().ok()?;
    let text = note
        .params
        .get("textDocument")?
        .get("text")?
        .as_str()?
        .as_bytes()
        .to_vec();
    cx.open_urls
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.clone(), uri);
    cx.host.apply_change(Change::Open {
        path: path.clone(),
        text,
    });
    Some(path)
}

fn did_change(cx: &Cx, note: &Notification) -> Option<PathBuf> {
    let path = doc_path(note)?;
    // Full sync (negotiated): the last full-text change wins.
    let text = note
        .params
        .get("contentChanges")?
        .as_array()?
        .iter()
        .rev()
        .find_map(|c| {
            if c.get("range").is_some() {
                None // incremental deltas were not negotiated; skip
            } else {
                c.get("text")?.as_str()
            }
        })?
        .as_bytes()
        .to_vec();
    cx.host.apply_change(Change::Edit {
        path: path.clone(),
        text,
    });
    Some(path)
}

fn did_close(cx: &Cx, note: &Notification) -> Option<PathBuf> {
    let uri = doc_uri(note)?;
    let path = uri.to_file_path().ok()?;
    cx.host.apply_change(Change::Close { path: path.clone() });
    cx.open_urls
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&path);
    // Ownership went back to disk; clear our published state so the
    // editor doesn't show squiggles for a buffer it no longer owns.
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics: Vec::new(),
        version: None,
    };
    (cx.sender)(Message::Notification(Notification::new(
        PublishDiagnostics::METHOD.to_string(),
        params,
    )));
    Some(path)
}

fn cancel_request(cx: &Cx, note: &Notification) {
    let id: Option<RequestId> = match note.params.get("id") {
        Some(serde_json::Value::Number(n)) => n.as_i64().map(|i| RequestId::from(i as i32)),
        Some(serde_json::Value::String(s)) => Some(RequestId::from(s.clone())),
        _ => None,
    };
    let Some(id) = id else { return };
    let inflight = cx.inflight.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = inflight.get(&id) {
        entry.client_cancelled.store(true, Ordering::SeqCst);
        entry.token.cancel();
    } else {
        // Not registered yet (the spawn window) or already answered;
        // record it — the worker checks this set after registering.
        // Entries for already-answered requests are inert; cap the set
        // so they cannot accumulate over a long session.
        let mut early = cx.early_cancel.lock().unwrap_or_else(|e| e.into_inner());
        if early.len() >= 256 {
            early.clear();
        }
        early.insert(id);
    }
}

// ------------------------------------------------------- diagnostics --

fn spawn_publish(cx: &Cx, path: PathBuf) {
    let cx = cx.clone();
    std::thread::spawn(move || {
        let snapshot = cx.host.snapshot();
        let requesting_uri = {
            let urls = cx.open_urls.lock().unwrap_or_else(|e| e.into_inner());
            match urls.get(&path) {
                Some(u) => u.clone(),
                None => return, // closed meanwhile
            }
        };
        // An unreadable file has nothing to publish; a cancelled batch
        // was superseded by a newer write whose own publish is coming.
        if let Ok(Some(batch)) = snapshot.diagnostics(&path) {
            let open_urls = cx
                .open_urls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            // Publish per FILE of the analyzed set, keyed by each
            // diagnostic's own primary span — not only for the
            // requesting document. D32 makes every `.lu` in a directory
            // one module, so a duplicate definition's primary span can
            // land in a `member: true` sibling; a server that publishes
            // only entry-primary diagnostics shows two clean files for a
            // package that does not build. LSP permits publishing for
            // documents the client never opened; unopened files get a
            // `file://` URI for their overlay/disk path. A file with
            // nothing to say gets an empty publish, which also clears
            // its previously published state after a fix.
            for file in &batch.files {
                let uri = if file.path == path {
                    requesting_uri.clone() // echo the client's own bytes
                } else {
                    match convert::url_for(&file.path, &open_urls) {
                        Some(u) => u,
                        None => continue,
                    }
                };
                let diagnostics =
                    convert::diagnostics_for_file(&batch, &file.path, cx.enc, &open_urls);
                let params = PublishDiagnosticsParams {
                    uri,
                    diagnostics,
                    version: None,
                };
                (cx.sender)(Message::Notification(Notification::new(
                    PublishDiagnostics::METHOD.to_string(),
                    params,
                )));
            }
        }
    });
}

// ----------------------------------------------------------- requests --

fn dispatch_request(cx: &Cx, req: Request) {
    let cx = cx.clone();
    std::thread::spawn(move || {
        let client_cancelled = Arc::new(AtomicBool::new(false));
        // One retry on write-invalidation (never on client cancel).
        for attempt in 0..2 {
            let snapshot = cx.host.snapshot();
            let token = snapshot.cancel_token();
            cx.inflight
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(
                    req.id.clone(),
                    Inflight {
                        token: token.clone(),
                        client_cancelled: Arc::clone(&client_cancelled),
                    },
                );
            // A `$/cancelRequest` may have raced ahead of registration.
            if cx
                .early_cancel
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&req.id)
            {
                client_cancelled.store(true, Ordering::SeqCst);
                token.cancel();
            }
            let outcome = handle_request(&cx, &snapshot, &req);
            drop(snapshot);
            match outcome {
                Ok(resp) => {
                    cx.inflight
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&req.id);
                    respond(&cx, resp);
                    return;
                }
                Err(Cancelled) => {
                    if client_cancelled.load(Ordering::SeqCst) {
                        cx.inflight
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(&req.id);
                        respond(
                            &cx,
                            Response::new_err(
                                req.id.clone(),
                                REQUEST_CANCELLED,
                                "the request was cancelled".to_string(),
                            ),
                        );
                        return;
                    }
                    if attempt == 1 {
                        cx.inflight
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(&req.id);
                        respond(
                            &cx,
                            Response::new_err(
                                req.id.clone(),
                                CONTENT_MODIFIED,
                                "analysis state changed while answering".to_string(),
                            ),
                        );
                        return;
                    }
                    // else: retry on the post-write snapshot.
                }
            }
        }
    });
}

/// Route one request against one snapshot. `Err(Cancelled)` propagates
/// from the query layer's single catch point.
fn handle_request(cx: &Cx, snapshot: &Snapshot, req: &Request) -> Result<Response, Cancelled> {
    let id = req.id.clone();
    match req.method.as_str() {
        "textDocument/hover" => hover(cx, snapshot, id, &req.params),
        "textDocument/documentSymbol" => document_symbol(cx, snapshot, id, &req.params),
        "textDocument/formatting" => formatting(cx, snapshot, id, &req.params),
        "textDocument/codeAction" => code_action(cx, snapshot, id, &req.params),
        "textDocument/completion" => completion(cx, snapshot, id, &req.params),
        _ => Ok(Response::new_err(
            id,
            ErrorCode::MethodNotFound as i32,
            format!("wolf lsp v0 does not implement {}", req.method),
        )),
    }
}

/// Extract (path, byte offset) from any `{textDocument, position}`
/// params, reading the document through the snapshot.
fn text_position(
    cx: &Cx,
    snapshot: &Snapshot,
    params: &serde_json::Value,
) -> Option<(PathBuf, Arc<Vec<u8>>, LineIndex, u32)> {
    let uri = params.get("textDocument")?.get("uri")?.as_str()?;
    let path = Url::parse(uri).ok()?.to_file_path().ok()?;
    let text = snapshot.file_text(&path)?;
    let index = LineIndex::new(&text);
    let pos = params.get("position")?;
    let position = lsp_types::Position {
        line: pos.get("line")?.as_u64()? as u32,
        character: pos.get("character")?.as_u64()? as u32,
    };
    let offset = positions::position_to_offset(&text, &index, position, cx.enc);
    Some((path, text, index, offset))
}

fn hover(
    cx: &Cx,
    snapshot: &Snapshot,
    id: RequestId,
    params: &serde_json::Value,
) -> Result<Response, Cancelled> {
    let Some((path, text, index, offset)) = text_position(cx, snapshot, params) else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let result = snapshot.type_at(&path, offset)?;
    let Some(h) = result else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let mut value = format!("```wolf\n{}\n```", h.text);
    if let Some(doc) = &h.doc {
        value.push_str("\n\n---\n\n");
        value.push_str(doc);
    }
    let hover = lsp_types::Hover {
        contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value,
        }),
        range: Some(convert::range_of(&text, &index, h.span, cx.enc)),
    };
    Ok(Response::new_ok(id, hover))
}

/// s122 — `textDocument/completion`. The query owns the semantics
/// (the incomplete-buffer contract included); this fn is pure wire
/// shape. The response is a plain `CompletionItem[]`, always complete
/// (no server-side truncation, so no `isIncomplete` list wrapper).
fn completion(
    cx: &Cx,
    snapshot: &Snapshot,
    id: RequestId,
    params: &serde_json::Value,
) -> Result<Response, Cancelled> {
    let Some((path, _text, _index, offset)) = text_position(cx, snapshot, params) else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let result = snapshot.completions(&path, offset)?;
    let Some(items) = result else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    Ok(Response::new_ok(id, convert::completion_items(&items)))
}

fn document_symbol(
    cx: &Cx,
    snapshot: &Snapshot,
    id: RequestId,
    params: &serde_json::Value,
) -> Result<Response, Cancelled> {
    let Some(path) = params
        .get("textDocument")
        .and_then(|t| t.get("uri"))
        .and_then(|u| u.as_str())
        .and_then(|u| Url::parse(u).ok())
        .and_then(|u| u.to_file_path().ok())
    else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let Some(text) = snapshot.file_text(&path) else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let syms = snapshot.document_symbols(&path)?;
    let Some(syms) = syms else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let index = LineIndex::new(&text);
    let converted = convert::document_symbols(&text, &index, &syms, cx.enc);
    Ok(Response::new_ok(id, converted))
}

fn formatting(
    cx: &Cx,
    snapshot: &Snapshot,
    id: RequestId,
    params: &serde_json::Value,
) -> Result<Response, Cancelled> {
    let Some(path) = params
        .get("textDocument")
        .and_then(|t| t.get("uri"))
        .and_then(|u| u.as_str())
        .and_then(|u| Url::parse(u).ok())
        .and_then(|u| u.to_file_path().ok())
    else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let Some(old) = snapshot.file_text(&path) else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let result = snapshot.format(&path)?;
    let Some(fmt) = result else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    if fmt.text == *old {
        // Byte-stable: canonical input produces zero edits.
        return Ok(Response::new_ok(id, Vec::<TextEdit>::new()));
    }
    let Ok(new_text) = String::from_utf8(fmt.text) else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    // One whole-document edit (v0; range formatting is out of scope).
    let index = LineIndex::new(&old);
    let end = positions::offset_to_position(&old, &index, old.len() as u32, cx.enc);
    let edit = TextEdit {
        range: lsp_types::Range {
            start: lsp_types::Position {
                line: 0,
                character: 0,
            },
            end,
        },
        new_text,
    };
    Ok(Response::new_ok(id, vec![edit]))
}

fn code_action(
    cx: &Cx,
    snapshot: &Snapshot,
    id: RequestId,
    params: &serde_json::Value,
) -> Result<Response, Cancelled> {
    let Some(uri) = params
        .get("textDocument")
        .and_then(|t| t.get("uri"))
        .and_then(|u| u.as_str())
        .and_then(|u| Url::parse(u).ok())
    else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let Some(path) = uri.to_file_path().ok() else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let Some(text) = snapshot.file_text(&path) else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let index = LineIndex::new(&text);
    let range = params.get("range");
    let to_pos = |v: &serde_json::Value| -> Option<lsp_types::Position> {
        Some(lsp_types::Position {
            line: v.get("line")?.as_u64()? as u32,
            character: v.get("character")?.as_u64()? as u32,
        })
    };
    let (lo, hi) = match range {
        Some(r) => {
            let start = r.get("start").and_then(to_pos);
            let end = r.get("end").and_then(to_pos);
            match (start, end) {
                (Some(s), Some(e)) => (
                    positions::position_to_offset(&text, &index, s, cx.enc),
                    positions::position_to_offset(&text, &index, e, cx.enc),
                ),
                _ => (0, text.len() as u32),
            }
        }
        None => (0, text.len() as u32),
    };
    let batch = snapshot.diagnostics(&path)?;
    let Some(batch) = batch else {
        return Ok(Response::new_ok(id, serde_json::Value::Null));
    };
    let Some(file) = batch.files.iter().find(|f| f.path == path) else {
        return Ok(Response::new_ok(id, Vec::<CodeActionOrCommand>::new()));
    };
    let span = Span::new(file.file, lo.min(hi), hi.max(lo));
    let open_urls = cx
        .open_urls
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let actions: Vec<CodeActionOrCommand> =
        convert::code_actions_for(&batch, &path, span, cx.enc, &open_urls)
            .into_iter()
            .map(CodeActionOrCommand::CodeAction)
            .collect();
    Ok(Response::new_ok(id, actions))
}
