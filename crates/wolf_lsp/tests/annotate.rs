//! Protocol-layer tests for the annotating rungs (s134):
//! `textDocument/signatureHelp`, `textDocument/semanticTokens/full` and
//! `/range`, `textDocument/inlayHint` — advertised, answered from the
//! binding table and the checker's call/local records, shaped per the
//! client's declarations (documentation format, the session's hint
//! configuration), honoring the negotiated encoding at every position.

mod support;

use serde_json::{Value, json};
use support::{Client, fixture};

fn pos(text: &str, needle: &str, nth: usize) -> (u64, u64) {
    let off = text.match_indices(needle).nth(nth).expect("needle").0;
    let line = text[..off].matches('\n').count() as u64;
    let line_start = text[..off].rfind('\n').map_or(0, |i| i + 1);
    (line, (off - line_start) as u64)
}

fn doc_pos(uri: &lsp_types::Url, (line, character): (u64, u64)) -> Value {
    json!({
        "textDocument": { "uri": uri.as_str() },
        "position": { "line": line, "character": character },
    })
}

fn main_text() -> String {
    std::fs::read_to_string(fixture("navigate/main.lu")).unwrap()
}

/// The three providers are advertised; the semantic-token legend is
/// the closed set in its fixed order; hints need no resolve.
#[test]
fn initialize_advertises_the_annotating_trio() {
    let (client, init) = Client::start(&["utf-8"]);
    let caps = &init["capabilities"];
    assert_eq!(
        caps["signatureHelpProvider"],
        json!({ "triggerCharacters": ["(", ","], "retriggerCharacters": [","] })
    );
    assert_eq!(
        caps["semanticTokensProvider"]["legend"]["tokenTypes"],
        json!([
            "namespace",
            "type",
            "parameter",
            "variable",
            "property",
            "enumMember",
            "function",
            "keyword"
        ])
    );
    assert_eq!(
        caps["semanticTokensProvider"]["legend"]["tokenModifiers"],
        json!(["declaration", "readonly"])
    );
    assert_eq!(caps["semanticTokensProvider"]["full"], true);
    assert_eq!(caps["semanticTokensProvider"]["range"], true);
    assert_eq!(
        caps["inlayHintProvider"],
        json!({ "resolveProvider": false })
    );
    client.shutdown();
}

/// Signature help inside `shapes.area(3)`: the cross-file callee's
/// declared parameter, its return type, its `///` doc, active
/// parameter 0; inside `Color.Rgb(1, 2, 3)` after the second comma:
/// the variant's payload types, active parameter 2; outside any
/// argument list: null.
#[test]
fn signature_help_names_the_declared_parameters_and_counts_commas() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let text = main_text();
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    client.wait_publish(&uri);

    // `shapes.area(3)` — the cursor on the `3`.
    let id = client.request(
        "textDocument/signatureHelp",
        doc_pos(&uri, pos(&text, "area(3)", 0).into_after(5)),
    );
    let help = client.wait_response(id).unwrap();
    let sig = &help["signatures"][0];
    assert_eq!(sig["label"], "area(side: int) -> int");
    assert_eq!(sig["parameters"], json!([{ "label": [5, 14] }]));
    assert_eq!(help["activeParameter"], 0);
    assert!(
        sig["documentation"]
            .as_str()
            .unwrap_or("")
            .contains("square's area"),
        "the callee's /// doc rides along: {sig}"
    );

    // `Color.Rgb(1, 2, 3)` — the cursor on the `3`, two commas before it.
    let id = client.request(
        "textDocument/signatureHelp",
        doc_pos(&uri, pos(&text, "Rgb(1, 2, 3)", 0).into_after(10)),
    );
    let help = client.wait_response(id).unwrap();
    assert_eq!(help["signatures"][0]["label"], "Color.Rgb(int, int, int)");
    assert_eq!(help["activeParameter"], 2);

    // `p.sum()` — a receiver is not a parameter the parentheses spell.
    let id = client.request(
        "textDocument/signatureHelp",
        doc_pos(&uri, pos(&text, "sum()", 0).into_after(4)),
    );
    let help = client.wait_response(id).unwrap();
    assert_eq!(help["signatures"][0]["label"], "sum() -> int");
    assert_eq!(help["signatures"][0]["parameters"], json!([]));
    assert!(help["activeParameter"].is_null());

    // On the `let` keyword: no argument list here.
    let id = client.request(
        "textDocument/signatureHelp",
        doc_pos(&uri, pos(&text, "let total", 0)),
    );
    assert!(client.wait_response(id).unwrap().is_null());
    client.shutdown();
}

trait After {
    fn into_after(self, n: u64) -> (u64, u64);
}
impl After for (u64, u64) {
    fn into_after(self, n: u64) -> (u64, u64) {
        (self.0, self.1 + n)
    }
}

/// Decode the relative integer stream into (line, char, len, type, mods).
fn decode(data: &Value) -> Vec<(u64, u64, u64, u64, u64)> {
    let d: Vec<u64> = data
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    let (mut line, mut start) = (0u64, 0u64);
    d.chunks(5)
        .map(|c| {
            line += c[0];
            start = if c[0] == 0 { start + c[1] } else { c[1] };
            (line, start, c[2], c[3], c[4])
        })
        .collect()
}

/// Full semantic tokens on the navigation fixture: keywords, a struct
/// name (`type`, declaration), a field (`property`), a method
/// (`function`), a variant (`enumMember`), a parameter, a `let` local
/// (`variable`, readonly, declaration at its binder), the module
/// (`namespace`), `print` (a prelude `function`).
#[test]
fn semantic_tokens_classify_through_the_binding_table() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let text = main_text();
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    client.wait_publish(&uri);
    let id = client.request(
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri.as_str() } }),
    );
    let toks = decode(&client.wait_response(id).unwrap()["data"]);
    // legend: namespace 0, type 1, parameter 2, variable 3, property 4,
    // enumMember 5, function 6, keyword 7; modifiers: declaration 1, readonly 2.
    let at = |needle: &str, nth: usize| {
        let (l, c) = pos(&text, needle, nth);
        toks.iter()
            .find(|t| t.0 == l && t.1 == c)
            .copied()
            .unwrap_or_else(|| panic!("no token at {needle:?} #{nth} ({l}:{c}); tokens: {toks:?}"))
    };
    assert_eq!(at("struct Point", 0).3, 7, "`struct` is a keyword");
    assert_eq!(
        at("Point {", 0),
        (
            pos(&text, "Point {", 0).0,
            pos(&text, "Point {", 0).1,
            5,
            1,
            1
        ),
        "the struct name declares a type"
    );
    assert_eq!(at("x: int", 0).3, 4, "a field is a property");
    assert_eq!(at("x: int", 0).4, 1, "…declared here");
    assert_eq!(at("sum(self)", 0).3, 6, "a method is a function");
    assert_eq!(at("Rgb(int", 0).3, 5, "a variant is an enumMember");
    assert_eq!(at("c: Color", 0).3, 2, "a parameter");
    assert_eq!(at("c {", 0).3, 2, "…and its use");
    let total = at("total = p", 0);
    assert_eq!(
        (total.3, total.4),
        (3, 3),
        "a `let` binder: variable, declaration + readonly"
    );
    assert_eq!(at("total}", 0).3, 3, "…and its use");
    assert_eq!(at("shapes.area", 0).3, 0, "the module is a namespace");
    assert_eq!(at("area(3)", 0).3, 6, "a cross-file fn is a function");
    assert_eq!(
        at("print(", 0).3,
        6,
        "a prelude name in callee position is a function"
    );
    assert_eq!(at("p.x +", 0).3, 3, "a local use is a variable");
    assert_eq!(at("x +", 0).3, 4, "a field through `.` is a property");
    // In source order, never overlapping.
    for w in toks.windows(2) {
        assert!(
            (w[0].0, w[0].1 + w[0].2) <= (w[1].0, w[1].1),
            "overlap: {w:?}"
        );
    }
    // The range request is the same stream, clipped.
    let (l, _) = pos(&text, "fn main", 0);
    let id = client.request("textDocument/semanticTokens/range", json!({
        "textDocument": { "uri": uri.as_str() },
        "range": { "start": { "line": l, "character": 0 }, "end": { "line": l + 1, "character": 0 } },
    }));
    let ranged = decode(&client.wait_response(id).unwrap()["data"]);
    assert!(ranged.iter().all(|t| t.0 == l), "{ranged:?}");
    assert_eq!(ranged.len(), toks.iter().filter(|t| t.0 == l).count());
    client.shutdown();
}

/// Inlay hints: `let p = Point { … }` gets `: Point` after `p`,
/// `let total = …` gets `: int`; `shapes.area(3)` gets `side:` before
/// the `3`; a variant constructor's payload has no names to offer; the
/// hint classes are filterable through `initializationOptions`.
#[test]
fn inlay_hints_infer_binder_types_and_name_parameters() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let text = main_text();
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    client.wait_publish(&uri);
    let whole = json!({
        "textDocument": { "uri": uri.as_str() },
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 100, "character": 0 } },
    });
    let id = client.request("textDocument/inlayHint", whole.clone());
    let hints = client.wait_response(id).unwrap();
    let hints = hints.as_array().unwrap();
    let find = |label: &str| {
        hints
            .iter()
            .find(|h| h["label"] == label)
            .unwrap_or_else(|| panic!("no hint {label:?}: {hints:?}"))
    };
    let p = find(": Point");
    let (l, c) = pos(&text, "p = Point", 0);
    assert_eq!(p["position"], json!({ "line": l, "character": c + 1 }));
    assert_eq!(p["kind"], 1);
    assert_eq!(p["paddingLeft"], true);
    let total = find(": int");
    let (l, c) = pos(&text, "total = p", 0);
    assert_eq!(total["position"], json!({ "line": l, "character": c + 5 }));
    let side = find("side:");
    let (l, c) = pos(&text, "area(3)", 0);
    assert_eq!(side["position"], json!({ "line": l, "character": c + 5 }));
    assert_eq!(side["kind"], 2);
    assert_eq!(side["paddingRight"], true);
    // Exactly these four: the variant constructor's payload offers no
    // names, `p.sum()` has no positional argument, and `print`'s
    // argument reaches no declared parameter (a prelude name).
    let labels: Vec<&str> = hints.iter().map(|h| h["label"].as_str().unwrap()).collect();
    assert_eq!(labels, [": Point", ": int", "side:", "c:"], "{hints:?}");
    // A range that covers only `fn brightness`: no hints from `main`.
    let (l, _) = pos(&text, "fn brightness", 0);
    let id = client.request("textDocument/inlayHint", json!({
        "textDocument": { "uri": uri.as_str() },
        "range": { "start": { "line": l, "character": 0 }, "end": { "line": l + 5, "character": 0 } },
    }));
    let inside = client.wait_response(id).unwrap();
    assert!(
        inside
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["position"]["line"].as_u64().unwrap() >= l),
        "{inside}"
    );
    client.shutdown();

    // Parameter names off, types on, through initializationOptions.
    let (mut client, _) = Client::start_with_options(
        json!({ "general": { "positionEncodings": ["utf-8"] } }),
        json!({ "inlayHints": { "parameterNames": false } }),
    );
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    client.wait_publish(&uri);
    let id = client.request("textDocument/inlayHint", whole);
    let hints = client.wait_response(id).unwrap();
    let labels: Vec<&str> = hints
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&": Point"), "{labels:?}");
    assert!(
        !labels.iter().any(|l| l.ends_with(':')),
        "no parameter-name hints: {labels:?}"
    );
    client.shutdown();
}

/// Positions in the negotiated encoding: a UTF-16 client sees
/// semantic-token columns in code units past an astral character.
#[test]
fn semantic_tokens_count_columns_in_the_negotiated_encoding() {
    let (mut client, _) = Client::start(&["utf-16"]);
    let path = fixture("navigate/main.lu");
    let text = "fn main() -> !int {\n    let s = \"😀\"\n    let n = 1\n    n\n}\n";
    let uri = client.open(&path, text);
    client.wait_publish(&uri);
    let id = client.request(
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri.as_str() } }),
    );
    let toks = decode(&client.wait_response(id).unwrap()["data"]);
    // Line 2's `let` keyword sits at utf-16 column 4 either way; `n` at 8.
    assert!(toks.contains(&(2, 4, 3, 7, 0)), "{toks:?}");
    assert!(toks.contains(&(2, 8, 1, 3, 3)), "{toks:?}");
    client.shutdown();
}
