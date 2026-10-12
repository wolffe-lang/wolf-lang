//! A minimal framed LSP client over a real `wolf lsp` child's stdio —
//! shared by the cross-binary LSP gates (`lsp_one_truth.rs`, the s235
//! std rows in `std_root.rs`). Included by path (`mod lsp_client;`);
//! not a test target of its own.

#![allow(dead_code)] // each including gate uses a subset

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// `file://` URI a server can parse on every platform — windows paths
/// need slash normalization and the empty-authority third slash
/// (`file:///D:/…`); `path.display()` alone produced `file://D:\…`,
/// which never parsed, so the awaited publish never matched (the 3.7h
/// windows wedge).
pub fn file_uri(path: &std::path::Path) -> String {
    let slashed = path.display().to_string().replace('\\', "/");
    if slashed.starts_with('/') {
        format!("file://{slashed}")
    } else {
        format!("file:///{slashed}")
    }
}

// ------------------------------------------- a minimal framed client --

pub struct LspChild {
    pub child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    disarm: Option<std::sync::mpsc::Sender<()>>,
}

impl LspChild {
    /// `wolf lsp` from this build, environment inherited.
    pub fn start() -> LspChild {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_wolf"));
        cmd.arg("lsp");
        LspChild::spawn(cmd)
    }

    /// Spawn `cmd` — a `wolf lsp` invocation the caller shaped (another
    /// binary, flags, environment) — as the server.
    pub fn spawn(mut cmd: Command) -> LspChild {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("wolf lsp starts");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        // Watchdog: libtest has no per-test timeout, so a lost response
        // would otherwise block recv() until the CI job ceiling (it did:
        // 3.7h wedged on windows). Kill the child after 60s unless
        // disarmed by Drop; death turns the hang into a loud EOF assert.
        let (disarm_tx, disarm_rx) = std::sync::mpsc::channel::<()>();
        let pid = child.id();
        std::thread::spawn(move || {
            if matches!(
                disarm_rx.recv_timeout(std::time::Duration::from_secs(60)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                eprintln!("watchdog: killing wedged wolf lsp (pid {pid})");
                #[cfg(unix)]
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
                #[cfg(windows)]
                let _ = Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .status();
            }
        });
        LspChild {
            child,
            stdin,
            stdout,
            next_id: 0,
            disarm: Some(disarm_tx),
        }
    }

    /// Stand the watchdog down — call before waiting on a clean exit.
    pub fn disarm(&mut self) {
        if let Some(tx) = self.disarm.take() {
            let _ = tx.send(());
        }
    }

    pub fn send(&mut self, msg: &serde_json::Value) {
        let body = serde_json::to_string(msg).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    pub fn request(&mut self, method: &str, params: serde_json::Value) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }));
        id
    }

    pub fn notify(&mut self, method: &str, params: serde_json::Value) {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0", "method": method, "params": params,
        }));
    }

    /// Read one framed message (blocking; the harness relies on the
    /// test timeout for hangs).
    pub fn recv(&mut self) -> serde_json::Value {
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

    pub fn wait_response(&mut self, id: i64) -> serde_json::Value {
        loop {
            let msg = self.recv();
            if msg.get("id").and_then(|v| v.as_i64()) == Some(id) && msg.get("method").is_none() {
                return msg;
            }
        }
    }

    pub fn wait_publish(&mut self, uri: &str) -> Vec<serde_json::Value> {
        loop {
            let msg = self.recv();
            if msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics")
                && msg["params"]["uri"].as_str() == Some(uri)
            {
                return msg["params"]["diagnostics"].as_array().cloned().unwrap();
            }
        }
    }

    pub fn shutdown(mut self) {
        let id = self.request("shutdown", serde_json::Value::Null);
        let resp = self.wait_response(id);
        assert!(resp.get("error").is_none_or(|e| e.is_null()));
        self.notify("exit", serde_json::Value::Null);
        self.disarm();
        let status = self.child.wait().expect("wolf lsp exits");
        assert!(status.success(), "clean exit, got {status:?}");
    }
}
