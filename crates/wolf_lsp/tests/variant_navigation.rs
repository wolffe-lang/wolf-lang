//! Enum member navigation through the real JSON-RPC server loop.
mod support;

use serde_json::{Value, json};
use support::{Client, fixture, uri_of};

fn position(text: &str, needle: &str, nth: usize) -> Value {
    let off = text.match_indices(needle).nth(nth).unwrap().0;
    let line = text[..off].bytes().filter(|&b| b == b'\n').count();
    let col = off - text[..off].rfind('\n').map_or(0, |i| i + 1);
    json!({"line": line, "character": col})
}

#[test]
fn variant_navigation_matrix() {
    let root = fixture("variant/main.lu");
    let member = fixture("variant/status/status.lu");
    let member_text = std::fs::read_to_string(&member).unwrap();
    for (context, entry) in [("root", &root), ("member", &member)] {
        let (mut client, _) = Client::start(&["utf-8"]);
        let uri = client.open_from_disk(entry);
        let diagnostics = client.wait_publish(&uri);
        println!("CONTEXT {context} diagnostics={diagnostics:?}");
        let member_uri = uri_of(&member);
        // Open the member too when a package entry is already open.
        if context == "root" {
            client.open_from_disk(&member);
            let _ = client.wait_publish(&member_uri);
        }
        for nth in 0..3 {
            let params = json!({"textDocument": {"uri": member_uri}, "position": position(&member_text, "Foreign", if nth == 0 { 0 } else { nth + 1 })});
            for include in [false, true] {
                let mut p = params.clone();
                p["context"] = json!({"includeDeclaration": include});
                let id = client.request("textDocument/references", p);
                println!("MATRIX {context}/{nth}/references/{include} {}", client.wait_response(id).unwrap());
            }
            for method in ["definition", "hover", "rename"] {
                let mut p = params.clone();
                if method == "rename" { p["newName"] = json!("External"); }
                let id = client.request(&format!("textDocument/{method}"), p);
                println!("MATRIX {context}/{nth}/{method} {:?}", client.wait_response(id));
            }
        }
        client.shutdown();
    }
}
