//! The one-truth check, cross-binary and over real stdio framing:
//! `wolf lsp`'s published diagnostics carry exactly the codes and spans
//! `wolf conform-run` reports for the same bytes. This is the assertion
//! that makes "the compiler *is* the language server" (D34) a claim
//! rather than a slogan (report 09 §conformance).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../wolf_lsp/tests/fixtures")
        .join(rel)
        .canonicalize()
        .expect("fixture exists")
}

fn corpus(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(rel)
        .canonicalize()
        .expect("corpus file exists")
}

/// `wolf conform-run --error-format=json` → the (code, lo, hi) set of
/// diagnostics whose primary span is in file 0 (the entry).
fn cli_diagnostics(path: &Path) -> Vec<(String, u32, u32)> {
    let out = Command::new(wolf())
        .args(["conform-run", path.to_str().unwrap(), "--error-format=json"])
        .output()
        .expect("conform-run runs");
    let stderr = String::from_utf8(out.stderr).unwrap();
    let mut diags = Vec::new();
    for line in stderr.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("diag_schema").is_none() {
            continue;
        }
        if v["primary"]["file"].as_u64() != Some(0) {
            continue;
        }
        let span = v["primary"]["span"].as_array().unwrap();
        diags.push((
            v["code"].as_str().unwrap().to_string(),
            span[0].as_u64().unwrap() as u32,
            span[1].as_u64().unwrap() as u32,
        ));
    }
    diags
}

// ------------------------------------------- a minimal framed client --

struct LspChild {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl LspChild {
    fn start() -> LspChild {
        let mut child = Command::new(wolf())
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("wolf lsp starts");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        LspChild {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    fn send(&mut self, msg: &serde_json::Value) {
        let body = serde_json::to_string(msg).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }));
        id
    }

    fn notify(&mut self, method: &str, params: serde_json::Value) {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0", "method": method, "params": params,
        }));
    }

    /// Read one framed message (blocking; the harness relies on the
    /// test timeout for hangs).
    fn recv(&mut self) -> serde_json::Value {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read header");
            assert!(n > 0, "server closed stdout mid-conversation");
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(v) = line.strip_prefix("Content-Length:") {
                content_length = Some(v.trim().parse().unwrap());
            }
        }
        let len = content_length.expect("Content-Length header");
        let mut body = vec![0u8; len];
        self.stdout.read_exact(&mut body).expect("read body");
        serde_json::from_slice(&body).expect("valid JSON body")
    }

    fn wait_response(&mut self, id: i64) -> serde_json::Value {
        loop {
            let msg = self.recv();
            if msg.get("id").and_then(|v| v.as_i64()) == Some(id) && msg.get("method").is_none() {
                return msg;
            }
        }
    }

    fn wait_publish(&mut self, uri: &str) -> Vec<serde_json::Value> {
        loop {
            let msg = self.recv();
            if msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics")
                && msg["params"]["uri"].as_str() == Some(uri)
            {
                return msg["params"]["diagnostics"].as_array().cloned().unwrap();
            }
        }
    }

    fn shutdown(mut self) {
        let id = self.request("shutdown", serde_json::Value::Null);
        let resp = self.wait_response(id);
        assert!(resp.get("error").is_none_or(|e| e.is_null()));
        self.notify("exit", serde_json::Value::Null);
        let status = self.child.wait().expect("wolf lsp exits");
        assert!(status.success(), "clean exit, got {status:?}");
    }
}

/// `(code, start, end)` in `(line, character)` positions.
type PosDiag = (String, (u64, u64), (u64, u64));

/// Byte offset → (line, character) with utf-8 positions negotiated —
/// characters are byte columns, so the mapping needs only line starts.
fn offset_to_utf8_pos(src: &[u8], offset: u32) -> (u64, u64) {
    let mut line = 0u64;
    let mut start = 0u32;
    for (i, &b) in src.iter().enumerate().take(offset as usize) {
        if b == b'\n' {
            line += 1;
            start = i as u32 + 1;
        }
    }
    (line, (offset - start) as u64)
}

fn one_truth_check(path: &Path) {
    let src = std::fs::read(path).unwrap();
    let text = String::from_utf8(src.clone()).unwrap();
    let expected = cli_diagnostics(path);

    let mut lsp = LspChild::start();
    let id = lsp.request(
        "initialize",
        serde_json::json!({
            "capabilities": { "general": { "positionEncodings": ["utf-8"] } },
        }),
    );
    let resp = lsp.wait_response(id);
    assert_eq!(
        resp["result"]["capabilities"]["positionEncoding"], "utf-8",
        "utf-8 negotiated so characters are byte columns"
    );
    lsp.notify("initialized", serde_json::json!({}));

    let uri = format!("file://{}", path.display());
    lsp.notify(
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": {
                "uri": uri, "languageId": "wolf", "version": 1, "text": text,
            }
        }),
    );
    let published = lsp.wait_publish(&uri);

    let got: Vec<PosDiag> = published
        .iter()
        .map(|d| {
            (
                d["code"].as_str().unwrap().to_string(),
                (
                    d["range"]["start"]["line"].as_u64().unwrap(),
                    d["range"]["start"]["character"].as_u64().unwrap(),
                ),
                (
                    d["range"]["end"]["line"].as_u64().unwrap(),
                    d["range"]["end"]["character"].as_u64().unwrap(),
                ),
            )
        })
        .collect();
    let want: Vec<PosDiag> = expected
        .iter()
        .map(|(code, lo, hi)| {
            (
                code.clone(),
                offset_to_utf8_pos(&src, *lo),
                offset_to_utf8_pos(&src, *hi),
            )
        })
        .collect();
    assert_eq!(got, want, "LSP and CLI disagree for {}", path.display());
    lsp.shutdown();
}

/// Clean file: both surfaces agree on "no diagnostics".
#[test]
fn one_truth_wordcount_clean() {
    one_truth_check(&corpus("wordcount.lu"));
}

/// Parse-broken file: same code, same span, both surfaces.
#[test]
fn one_truth_broken_fixture() {
    one_truth_check(&fixture("broken.lu"));
}

/// Resolve-tier diagnostic with a fix-it (E0305): same code, same span.
#[test]
fn one_truth_unused_import() {
    one_truth_check(&fixture("unused/main.lu"));
}
