//! `$/cancelRequest` reaches the query layer (the tower-lsp failure
//! named in report 09): with an injected slow query, a cancelled hover
//! answers `-32800 RequestCancelled` — promptly, and never never-at-all.
//!
//! Own test binary: the slow-query knob is process-wide env state.

mod support;

use serde_json::json;
use support::{Client, fixture};

#[test]
fn cancelled_request_answers_request_cancelled() {
    // Every query in this process now dawdles at its first checkpoint
    // (in 5 ms cancellable slices).
    // SAFETY: single-threaded at this point; this test binary owns the
    // process and no query has started yet.
    unsafe { std::env::set_var(wolf_query::TEST_SLOW_ENV, "10000") };

    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("typed/typed.lu");
    let text = std::fs::read_to_string(&path).unwrap();
    let uri = client.open(&path, &text);

    let started = std::time::Instant::now();
    let id = client.request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": 3, "character": 8 },
        }),
    );
    // The handshake consumed id 1; this hover is id 2.
    client.notify("$/cancelRequest", json!({ "id": 2 }));
    let err = client
        .wait_response(id)
        .expect_err("cancelled request answers with an error");
    assert_eq!(err.code, -32800, "RequestCancelled: {err:?}");
    // Promptly: nowhere near the 10 s the query would have dawdled.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "cancellation reached the compute, not just the transport"
    );

    // The pipeline is not poisoned: clear the knob via a fresh value
    // of zero and ask again — the same request now succeeds.
    unsafe { std::env::set_var(wolf_query::TEST_SLOW_ENV, "0") };
    let id = client.request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": 3, "character": 8 },
        }),
    );
    let result = client
        .wait_response(id)
        .expect("post-cancel hover succeeds");
    let text = result["contents"]["value"].as_str().unwrap().to_string();
    assert!(text.contains("s: int"), "{text}");
    client.shutdown();
}
