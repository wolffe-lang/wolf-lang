//! Scripted in-process LSP client: drives `wolf_lsp::main_loop` over
//! `lsp_server::Connection::memory()` — real messages, real threads,
//! real cancellation; only the byte framing is elided (the driver's
//! integration test covers stdio framing against the shipped binary).

#![allow(dead_code)] // shared across test binaries; each uses a subset

use std::path::{Path, PathBuf};
use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, RequestId, ResponseError};
use lsp_types::Url;
use serde_json::{Value, json};

pub struct Client {
    conn: Connection,
    /// Joins to the loop's lifecycle exit code (0 = shutdown→exit).
    server: Option<std::thread::JoinHandle<i32>>,
    next_id: i32,
    /// Notifications received while waiting for something else.
    pub notifications: Vec<Notification>,
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(name)
        .canonicalize()
        .expect("corpus file exists")
}

pub fn uri_of(path: &Path) -> Url {
    Url::from_file_path(path).expect("absolute fixture path")
}

impl Client {
    /// Start a server thread and complete the `initialize` handshake,
    /// offering `encodings` (e.g. `&["utf-8"]`); empty = no capability.
    pub fn start(encodings: &[&str]) -> (Client, Value) {
        let caps = if encodings.is_empty() {
            json!({})
        } else {
            json!({ "general": { "positionEncodings": encodings } })
        };
        Client::start_with(caps)
    }

    /// Start a server thread and complete the `initialize` handshake
    /// with an explicit client-capabilities document (the shape a
    /// real client declares: `linkSupport`, `documentChanges`, …).
    pub fn start_with(caps: Value) -> (Client, Value) {
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || {
            wolf_lsp::main_loop(server_side).expect("server loop failed")
        });
        let mut client = Client {
            conn: client_side,
            server: Some(server),
            next_id: 0,
            notifications: Vec::new(),
        };
        let id = client.request("initialize", json!({ "capabilities": caps }));
        let result = client.wait_response(id).expect("initialize succeeds");
        client.notify("initialized", json!({}));
        (client, result)
    }

    pub fn request(&mut self, method: &str, params: Value) -> RequestId {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        self.conn
            .sender
            .send(Message::Request(Request::new(
                id.clone(),
                method.to_string(),
                params,
            )))
            .expect("send request");
        id
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.conn
            .sender
            .send(Message::Notification(Notification::new(
                method.to_string(),
                params,
            )))
            .expect("send notification");
    }

    fn recv(&mut self) -> Message {
        self.conn
            .receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("server answered within 30s")
    }

    /// Wait for the response to `id`, buffering notifications.
    pub fn wait_response(&mut self, id: RequestId) -> Result<Value, ResponseError> {
        loop {
            match self.recv() {
                Message::Response(r) if r.id == id => return r.response_result,
                Message::Response(r) => panic!("unexpected response: {r:?}"),
                Message::Notification(n) => self.notifications.push(n),
                Message::Request(r) => panic!("unexpected server request: {r:?}"),
            }
        }
    }

    /// Wait for the next `publishDiagnostics` for `uri` (buffered or
    /// incoming) and return its `diagnostics` array.
    pub fn wait_publish(&mut self, uri: &Url) -> Vec<Value> {
        let matches = |n: &Notification| {
            n.method == "textDocument/publishDiagnostics"
                && n.params.get("uri").and_then(Value::as_str) == Some(uri.as_str())
        };
        if let Some(pos) = self.notifications.iter().position(matches) {
            let n = self.notifications.remove(pos);
            return n.params["diagnostics"].as_array().cloned().unwrap();
        }
        loop {
            match self.recv() {
                Message::Notification(n) if matches(&n) => {
                    return n.params["diagnostics"].as_array().cloned().unwrap();
                }
                Message::Notification(n) => self.notifications.push(n),
                other => panic!("expected publishDiagnostics, got {other:?}"),
            }
        }
    }

    pub fn open(&mut self, path: &Path, text: &str) -> Url {
        let uri = uri_of(path);
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri.as_str(),
                    "languageId": "wolf",
                    "version": 1,
                    "text": text,
                }
            }),
        );
        uri
    }

    pub fn open_from_disk(&mut self, path: &Path) -> Url {
        let text = std::fs::read_to_string(path).expect("fixture readable");
        self.open(path, &text)
    }

    pub fn change(&mut self, uri: &Url, version: i32, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri.as_str(), "version": version },
                "contentChanges": [ { "text": text } ],
            }),
        );
    }

    /// Clean shutdown: `shutdown` → `exit` → join the server thread,
    /// asserting the lifecycle's success code (0).
    pub fn shutdown(mut self) {
        let id = self.request("shutdown", Value::Null);
        let resp = self.wait_response(id);
        assert!(resp.is_ok(), "shutdown errored: {resp:?}");
        self.notify("exit", Value::Null);
        let code = self
            .server
            .take()
            .unwrap()
            .join()
            .expect("server thread exits cleanly");
        assert_eq!(code, 0, "shutdown→exit is the success order");
    }

    /// Bare `exit` with no `shutdown` first; returns the loop's exit
    /// code (the spec says 1).
    pub fn exit_without_shutdown(mut self) -> i32 {
        self.notify("exit", Value::Null);
        self.server
            .take()
            .unwrap()
            .join()
            .expect("server thread exits cleanly")
    }
}
