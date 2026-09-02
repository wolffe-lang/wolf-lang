//! Per-request-kind LSP latency, in the s01 bench JSONL shape
//! (`{bench, track, lang, metric, value, unit, commit, config}`,
//! `track: "lsp"`). Report-only (report 09 L4): v0 runs the
//! non-resident compiler per request batch; these numbers become a CI
//! gate when s57/M3 tightens them.
//!
//! Run: `cargo run -p wolf_lsp --example lsp_bench [runs]`
//! (default 20). One in-process session over `corpus/wordcount.lu`;
//! measured shim-receive → shim-respond, per report 09's budget table.

use std::path::Path;
use std::time::{Duration, Instant};

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};

fn main() {
    let runs: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(20);

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let wordcount = manifest
        .join("../../corpus/wordcount.lu")
        .canonicalize()
        .expect("corpus/wordcount.lu");
    let text = std::fs::read_to_string(&wordcount).expect("readable");
    let uri = format!("file://{}", wordcount.display());

    let (server_side, client_side) = Connection::memory();
    let server = std::thread::spawn(move || {
        wolf_lsp::main_loop(server_side).expect("server loop");
    });
    let send = |m: Message| client_side.sender.send(m).expect("send");
    let recv = || {
        client_side
            .receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("recv")
    };
    let mut next_id = 0i32;
    let mut request = |method: &str, params: Value| -> RequestId {
        next_id += 1;
        let id = RequestId::from(next_id);
        send(Message::Request(Request::new(
            id.clone(),
            method.to_string(),
            params,
        )));
        id
    };
    let wait_response = |id: RequestId| loop {
        if let Message::Response(r) = recv()
            && r.id == id
        {
            return r;
        }
    };

    // Handshake + open (utf-8: byte columns).
    let id = request(
        "initialize",
        json!({ "capabilities": { "general": { "positionEncodings": ["utf-8"] } } }),
    );
    wait_response(id);
    send(Message::Notification(Notification::new(
        "initialized".to_string(),
        json!({}),
    )));
    send(Message::Notification(Notification::new(
        "textDocument/didOpen".to_string(),
        json!({
            "textDocument": {
                "uri": uri, "languageId": "wolf", "version": 1, "text": text,
            }
        }),
    )));
    // Drain the initial publishDiagnostics before timing.
    loop {
        if let Message::Notification(n) = recv()
            && n.method == "textDocument/publishDiagnostics"
        {
            break;
        }
    }

    let doc = json!({ "uri": uri });
    // A use of `total` after its declaration (utf-8: byte columns).
    let nav_pos = {
        let decl = text.find("var total").expect("wordcount declares `total`");
        let off = decl + text[decl + 9..].find("total").expect("a later use") + 9;
        let line = text[..off].matches('\n').count();
        let col = off - text[..off].rfind('\n').map_or(0, |i| i + 1);
        json!({ "line": line, "character": col })
    };
    let kinds: Vec<(&str, &str, Value)> = vec![
        (
            "hover",
            "textDocument/hover",
            json!({ "textDocument": doc, "position": { "line": 18, "character": 3 } }),
        ),
        (
            "documentSymbol",
            "textDocument/documentSymbol",
            json!({ "textDocument": doc }),
        ),
        (
            "formatting",
            "textDocument/formatting",
            json!({ "textDocument": doc, "options": { "tabSize": 4, "insertSpaces": true } }),
        ),
        (
            "codeAction",
            "textDocument/codeAction",
            json!({
                "textDocument": doc,
                "range": { "start": { "line": 0, "character": 0 },
                           "end": { "line": 0, "character": 0 } },
                "context": { "diagnostics": [] },
            }),
        ),
        // s122: name-position completion (warm memoized analysis —
        // the common keystroke path once diagnostics have run).
        (
            "completion",
            "textDocument/completion",
            json!({ "textDocument": doc, "position": { "line": 18, "character": 7 } }),
        ),
        // s133: the navigation trio on a local's USE (`total`, the
        // wordcount map) — a binding-table lookup on the memoized
        // analysis; rename walks every file's table for the edit set.
        (
            "definition",
            "textDocument/definition",
            json!({ "textDocument": doc, "position": nav_pos }),
        ),
        (
            "references",
            "textDocument/references",
            json!({ "textDocument": doc, "position": nav_pos,
                    "context": { "includeDeclaration": true } }),
        ),
        (
            "rename",
            "textDocument/rename",
            json!({ "textDocument": doc, "position": nav_pos, "newName": "counts" }),
        ),
    ];

    let commit = git_short_sha();
    for (name, method, params) in &kinds {
        let mut samples: Vec<f64> = Vec::with_capacity(runs);
        for _ in 0..runs {
            let started = Instant::now();
            let id = request(method, params.clone());
            wait_response(id);
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(|a, b| a.total_cmp(b));
        let p = |q: f64| samples[((samples.len() - 1) as f64 * q) as usize];
        for (metric, value) in [
            ("latency_p50_ms", p(0.5)),
            ("latency_p95_ms", p(0.95)),
            ("latency_max_ms", *samples.last().unwrap()),
        ] {
            println!(
                "{}",
                json!({
                    "bench": format!("wordcount/{name}"),
                    "track": "lsp",
                    "lang": "wolf",
                    "metric": metric,
                    "value": value,
                    "unit": "ms",
                    "commit": commit,
                    "config": "v0-nonresident",
                })
            );
        }
    }

    // s122: member-position completion under mid-edit conditions —
    // each run sends a didChange (a broken `total.` inserted in
    // `main`) and then completes after the dot, so the sample carries
    // the full keystroke path: overlay write + repaired-text ladder.
    {
        let anchor = "    var total = Map[str, int]()\n";
        let member_text = text.replace(anchor, &format!("{anchor}    total.\n"));
        assert_ne!(member_text, text, "wordcount anchor line present");
        let dot_line = member_text
            .lines()
            .position(|l| l.trim() == "total.")
            .expect("inserted line") as u64;
        let mut samples: Vec<f64> = Vec::with_capacity(runs);
        for i in 0..runs {
            send(Message::Notification(Notification::new(
                "textDocument/didChange".to_string(),
                json!({
                    "textDocument": { "uri": uri, "version": 2 + i as i64 },
                    "contentChanges": [ { "text": member_text } ],
                }),
            )));
            let started = Instant::now();
            let id = request(
                "textDocument/completion",
                json!({
                    "textDocument": doc,
                    "position": { "line": dot_line, "character": 10 },
                }),
            );
            wait_response(id);
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(|a, b| a.total_cmp(b));
        let p = |q: f64| samples[((samples.len() - 1) as f64 * q) as usize];
        for (metric, value) in [
            ("latency_p50_ms", p(0.5)),
            ("latency_p95_ms", p(0.95)),
            ("latency_max_ms", *samples.last().unwrap()),
        ] {
            println!(
                "{}",
                json!({
                    "bench": "wordcount/completionMember",
                    "track": "lsp",
                    "lang": "wolf",
                    "metric": metric,
                    "value": value,
                    "unit": "ms",
                    "commit": commit,
                    "config": "v0-nonresident",
                })
            );
        }
    }

    let id = request("shutdown", Value::Null);
    wait_response(id);
    send(Message::Notification(Notification::new(
        "exit".to_string(),
        Value::Null,
    )));
    server.join().expect("clean server exit");
}

fn git_short_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
