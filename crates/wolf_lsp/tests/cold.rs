//! The v0 courtesy bound (sprint acceptance): a cold server answers
//! hover on `corpus/wordcount.lu` in under 2 s. Own test binary so no
//! sibling test's work shares the clock.

mod support;

use serde_json::json;
use support::{Client, corpus};

#[test]
fn cold_hover_on_wordcount_under_two_seconds() {
    let started = std::time::Instant::now();
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = corpus("wordcount.lu");
    let uri = client.open_from_disk(&path);
    // Hover over `count` in `fn count(text: str)` (line 18, col 3).
    let id = client.request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": 18, "character": 3 },
        }),
    );
    let resp = client.wait_response(id);
    assert!(resp.is_ok(), "hover errored: {resp:?}");
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "cold hover took {elapsed:?} (bound: 2 s)"
    );
    client.shutdown();
}
