//! Enum member navigation through the JSON-RPC server loop. The member's
//! sibling import requires the package entry's context, not its own directory.
mod support;

use serde_json::{Value, json};
use support::{Client, fixture, uri_of};

fn position(text: &str, nth: usize) -> Value {
    let off = text.match_indices("Foreign").nth(nth).unwrap().0;
    let line = text[..off].bytes().filter(|&b| b == b'\n').count();
    let col = off - text[..off].rfind('\n').map_or(0, |i| i + 1);
    json!({"line": line, "character": col})
}

fn location(rel: &str, nth: usize) -> Value {
    let path = fixture(rel);
    let text = std::fs::read_to_string(&path).unwrap();
    let start = position(&text, nth);
    let end = json!({"line": start["line"], "character": start["character"].as_u64().unwrap() + 7});
    json!({"uri": uri_of(&path), "range": {"start": start, "end": end}})
}

const MAIN: &str = "variant/main.lu";
const MEMBER: &str = "variant/status/status.lu";

fn params(rel: &str, nth: usize) -> Value {
    let loc = location(rel, nth);
    json!({"textDocument": {"uri": loc["uri"]}, "position": loc["range"]["start"]})
}

fn expected_refs(include: bool) -> Value {
    let mut refs = vec![location(MAIN, 0), location(MAIN, 1)];
    if include {
        refs.push(location(MEMBER, 0));
    }
    // Other.Foreign (declaration occurrence 1, use occurrence 4) is NOT State.Foreign.
    refs.extend([location(MEMBER, 2), location(MEMBER, 3)]);
    json!(refs)
}

fn start(root_first: bool) -> Client {
    let (mut client, _) = Client::start(&["utf-8"]);
    let order = if root_first {
        [MAIN, MEMBER]
    } else {
        [MEMBER, MAIN]
    };
    for rel in order {
        let uri = client.open_from_disk(&fixture(rel));
        let _ = client.wait_publish(&uri);
    }
    client
}

fn cursors() -> [(&'static str, usize); 5] {
    [(MEMBER, 0), (MEMBER, 2), (MEMBER, 3), (MAIN, 0), (MAIN, 1)]
}

#[test]
fn variant_references_use_the_open_package_graph() {
    for root_first in [true, false] {
        let mut client = start(root_first);
        for (rel, nth) in cursors() {
            for include in [false, true] {
                let mut p = params(rel, nth);
                p["context"] = json!({"includeDeclaration": include});
                let id = client.request("textDocument/references", p);
                assert_eq!(
                    client.wait_response(id).unwrap(),
                    expected_refs(include),
                    "{rel}/{nth}, includeDeclaration={include}, root_first={root_first}"
                );
            }
        }
        client.shutdown();
    }
}

#[test]
fn variant_definition_uses_the_open_package_graph() {
    for root_first in [true, false] {
        let mut client = start(root_first);
        for (rel, nth) in cursors() {
            let id = client.request("textDocument/definition", params(rel, nth));
            assert_eq!(
                client.wait_response(id).unwrap(),
                json!([location(MEMBER, 0)]),
                "{rel}/{nth}"
            );
        }
        client.shutdown();
    }
}

#[test]
fn variant_rename_uses_the_open_package_graph() {
    for root_first in [true, false] {
        let mut client = start(root_first);
        for (rel, nth) in cursors() {
            let p = params(rel, nth);
            let id = client.request("textDocument/prepareRename", p.clone());
            assert_eq!(
                client.wait_response(id).unwrap(),
                json!({
                    "range": location(rel, nth)["range"], "placeholder": "Foreign"
                })
            );
            let mut p = p;
            p["newName"] = json!("External");
            let id = client.request("textDocument/rename", p);
            let mut changes = serde_json::Map::new();
            for loc in expected_refs(true).as_array().unwrap() {
                changes
                    .entry(loc["uri"].as_str().unwrap())
                    .or_insert(json!([]))
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"range": loc["range"], "newText": "External"}));
            }
            assert_eq!(
                client.wait_response(id).unwrap(),
                json!({"changes": changes}),
                "{rel}/{nth}"
            );
        }
        client.shutdown();
    }
}

#[test]
fn variant_hover_uses_the_open_package_graph() {
    for root_first in [true, false] {
        let mut client = start(root_first);
        for (rel, nth) in cursors() {
            let id = client.request("textDocument/hover", params(rel, nth));
            let hover = client.wait_response(id).unwrap();
            // Variant declaration hover is still unsupported. Value hovers
            // return the enum type; pattern hovers return the enclosing match's type.
            if rel == MEMBER && nth == 0 {
                assert_eq!(hover, Value::Null);
            } else {
                let ty = if (rel == MEMBER && nth == 3) || (rel == MAIN && nth == 0) {
                    "int"
                } else {
                    "State"
                };
                assert_eq!(
                    hover["contents"]["value"],
                    format!("```wolf\n{ty}\n```"),
                    "{rel}/{nth}"
                );
            }
        }
        client.shutdown();
    }
}

#[test]
fn variant_context_is_not_inferred_from_an_unrelated_open_ancestor() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let root = fixture(MAIN);
    let uri = client.open(&root, "fn main() -> int { 0 }\n");
    let _ = client.wait_publish(&uri);
    let member = client.open_from_disk(&fixture(MEMBER));
    let ds = client.wait_publish(&member);
    assert!(ds.iter().any(|d| d["code"] == "E0301"));
    let mut p = params(MEMBER, 0);
    p["context"] = json!({"includeDeclaration": false});
    let id = client.request("textDocument/references", p);
    assert_eq!(client.wait_response(id).unwrap(), Value::Null);
    client.shutdown();
}

#[test]
fn variant_context_tracks_unsaved_import_edits_and_close() {
    let mut client = start(true);
    let mut p = params(MEMBER, 0);
    p["context"] = json!({"includeDeclaration": true});
    let id = client.request("textDocument/references", p.clone());
    assert_eq!(client.wait_response(id).unwrap(), expected_refs(true));
    let root = uri_of(&fixture(MAIN));
    client.change(&root, 2, "fn main() -> int { 0 }\n");
    let _ = client.wait_publish(&root);
    let id = client.request("textDocument/references", p.clone());
    assert_eq!(client.wait_response(id).unwrap(), Value::Null);
    let text = std::fs::read_to_string(fixture(MAIN)).unwrap();
    client.change(&root, 3, &text);
    let _ = client.wait_publish(&root);
    let id = client.request("textDocument/references", p.clone());
    assert_eq!(client.wait_response(id).unwrap(), expected_refs(true));
    client.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": root}}),
    );
    let id = client.request("textDocument/references", p);
    assert_eq!(client.wait_response(id).unwrap(), Value::Null);
    client.shutdown();
}
