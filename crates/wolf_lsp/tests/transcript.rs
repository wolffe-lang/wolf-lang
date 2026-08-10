//! Transcript smoke tests: scripted JSON-RPC sessions against the shim
//! (in-process over `Connection::memory()` — see `support`). The
//! cancellation and cold-latency transcripts live in their own test
//! binaries (`cancel.rs`, `cold.rs`) because they control process-wide
//! state (the slow-query env knob, wall-clock bounds).

mod support;

use serde_json::{Value, json};
use support::{Client, fixture};

/// initialize → open the canonical corpus program → its diagnostics are
/// EMPTY (wordcount is clean at every implemented rung) → shutdown.
#[test]
fn wordcount_publishes_no_diagnostics() {
    let (mut client, init) = Client::start(&["utf-8"]);
    assert_eq!(init["capabilities"]["positionEncoding"], "utf-8");
    let path = support::corpus("wordcount.lu");
    let uri = client.open_from_disk(&path);
    let diags = client.wait_publish(&uri);
    assert_eq!(diags, Vec::<Value>::new());
    client.shutdown();
}

/// A broken fixture publishes the same code at the same span the CLI
/// reports for the same bytes (`wolf conform-run` probed: E0201 at
/// bytes [28,28] = line 1, cols 9..9 — the in-process half of the
/// one-truth check; the cross-binary half lives in wolf_driver/tests).
#[test]
fn broken_fixture_matches_cli_codes_and_spans() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("broken.lu");
    let uri = client.open_from_disk(&path);
    let diags = client.wait_publish(&uri);
    assert_eq!(diags.len(), 1, "one diagnostic: {diags:?}");
    assert_eq!(diags[0]["code"], "E0201");
    assert_eq!(diags[0]["severity"], 1); // error
    assert_eq!(
        diags[0]["range"],
        json!({ "start": { "line": 1, "character": 9 },
                "end": { "line": 1, "character": 9 } })
    );
    client.shutdown();
}

/// Overlays are real: didOpen text overrides disk, didChange with
/// unsaved edits changes diagnostics, and the disk file never changes.
#[test]
fn overlay_edits_change_diagnostics_without_touching_disk() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("typed.lu");
    let disk_before = std::fs::read(&path).unwrap();

    // Open with broken text (disk is clean).
    let uri = client.open(&path, "fn main() -> int {\n    lett x = 1\n    0\n}\n");
    let diags = client.wait_publish(&uri);
    assert!(!diags.is_empty(), "overlay text is broken: {diags:?}");

    // Unsaved edit back to clean text → diagnostics clear.
    let clean = String::from_utf8(disk_before.clone()).unwrap();
    client.change(&uri, 2, &clean);
    let diags = client.wait_publish(&uri);
    assert_eq!(diags, Vec::<Value>::new());

    assert_eq!(std::fs::read(&path).unwrap(), disk_before, "disk untouched");
    client.shutdown();
}

/// Formatting is byte-stable: format → apply → format again → zero
/// edits. Full-document only, via the one formatter.
#[test]
fn formatting_round_trip_is_byte_stable() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("unformatted.lu");
    let uri = client.open_from_disk(&path);
    let _ = client.wait_publish(&uri);

    let id = client.request(
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri.as_str() },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    );
    let edits = client.wait_response(id).expect("formatting answers");
    let edits = edits.as_array().expect("edit list");
    assert_eq!(edits.len(), 1, "non-canonical file gets one edit");
    let new_text = edits[0]["newText"].as_str().unwrap().to_string();
    assert_ne!(new_text.as_bytes(), std::fs::read(&path).unwrap());

    // Apply (whole-document edit) and format again: zero edits.
    client.change(&uri, 2, &new_text);
    let id = client.request(
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri.as_str() },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    );
    assert_eq!(client.wait_response(id).unwrap(), json!([]));
    client.shutdown();
}

/// documentSymbol answers from the parse tree; snapshot-reviewed.
#[test]
fn document_symbols_snapshot() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("symbols.lu");
    let uri = client.open_from_disk(&path);
    let id = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri.as_str() } }),
    );
    let syms = client.wait_response(id).expect("symbols answer");
    // Compact shape: one `name [kind] @line` per symbol, indented.
    fn shape(v: &Value, depth: usize, out: &mut String) {
        out.push_str(&format!(
            "{}{} [kind {}] @{}\n",
            "  ".repeat(depth),
            v["name"].as_str().unwrap_or("?"),
            v["kind"],
            v["selectionRange"]["start"]["line"],
        ));
        if let Some(children) = v["children"].as_array() {
            for c in children {
                shape(c, depth + 1, out);
            }
        }
    }
    let mut shaped = String::new();
    for s in syms.as_array().unwrap() {
        shape(s, 0, &mut shaped);
    }
    insta::assert_snapshot!(shaped);
    client.shutdown();
}

/// Hover on the typed subset: a local binding, an expression, and an
/// item header with its doc comment.
#[test]
fn hover_answers_types_and_docs() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("typed.lu");
    let uri = client.open_from_disk(&path);
    let _ = client.wait_publish(&uri);

    let hover_at = |client: &mut Client, line: u32, character: u32| -> Value {
        let id = client.request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri.as_str() },
                "position": { "line": line, "character": character },
            }),
        );
        client.wait_response(id).expect("hover answers")
    };

    // Line 3 `    let s = a + b` — the local `s` (col 8).
    let h = hover_at(&mut client, 3, 8);
    let text = h["contents"]["value"].as_str().unwrap();
    assert!(
        text.contains("s: int"),
        "local hover shows its type: {text}"
    );

    // Line 4 `    s` — the expression (col 4).
    let h = hover_at(&mut client, 4, 4);
    let text = h["contents"]["value"].as_str().unwrap();
    assert!(text.contains("int"), "expr hover shows its type: {text}");

    // Line 2 `fn add(…)` — the item name (col 3) carries the doc.
    let h = hover_at(&mut client, 2, 3);
    let text = h["contents"]["value"].as_str().unwrap();
    assert!(text.contains("fn add"), "item hover: {text}");
    assert!(
        text.contains("Adds two numbers."),
        "doc comment rides the hover: {text}"
    );
    client.shutdown();
}

/// Machine-applicable fix-its surface as quickfix code actions with
/// fully-resolved edits (E0305's delete-the-import, probed from the
/// CLI: edit replaces bytes [0,9] with "").
#[test]
fn code_action_surfaces_machine_applicable_fix() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("unused/main.lu");
    let uri = client.open_from_disk(&path);
    let diags = client.wait_publish(&uri);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0]["code"], "E0305");

    let id = client.request(
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri.as_str() },
            "range": diags[0]["range"],
            "context": { "diagnostics": [ diags[0] ] },
        }),
    );
    let actions = client.wait_response(id).expect("code actions answer");
    let actions = actions.as_array().unwrap();
    assert_eq!(actions.len(), 1, "one quickfix: {actions:?}");
    let action = &actions[0];
    assert_eq!(action["title"], "delete the unused import");
    assert_eq!(action["kind"], "quickfix");
    let edits = &action["edit"]["changes"][uri.as_str()];
    assert_eq!(
        edits,
        &json!([{
            "range": { "start": { "line": 0, "character": 0 },
                       "end": { "line": 1, "character": 0 } },
            "newText": "",
        }])
    );
    client.shutdown();
}

/// The utf-16 default: no client encodings advertised ⇒ utf-16, and
/// positions on an astral-character line translate per the spec's own
/// `a𐐀b` rule.
#[test]
fn utf16_default_and_translation() {
    let (mut client, init) = Client::start(&[]);
    assert_eq!(init["capabilities"]["positionEncoding"], "utf-16");
    let path = fixture("unicode.lu");
    // `let 🐺 = 1` — the emoji is not wolf syntax (E0107, probed:
    // bytes [27,31]); its range must come back in utf-16 units
    // (astral = 2), i.e. cols 8..10, where utf-8 would say 8..12.
    let uri = client.open(
        &path,
        "fn main() -> int {\n    let \u{1f43a} = 1\n    0\n}\n",
    );
    let diags = client.wait_publish(&uri);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "E0107");
    assert_eq!(
        diags[0]["range"],
        json!({ "start": { "line": 1, "character": 8 },
                "end": { "line": 1, "character": 10 } })
    );
    client.shutdown();
}

/// A diagnostic whose PRIMARY span lives in a `member: true` sibling of
/// the opened document reaches the SIBLING's URI (D32: every `.lu` in a
/// directory is one module, so `wolf build` fails this package —
/// resolve/dupdef, E0302 — and the editor must not show two clean
/// files). The sibling is never opened; LSP permits publishing for
/// documents the client does not have open.
#[test]
fn cross_file_diagnostic_reaches_the_siblings_uri() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let main = support::corpus("resolve/dupdef/main.lu");
    let sibling = support::corpus("resolve/dupdef/twice.lu");
    let sib_uri = support::uri_of(&sibling);

    let main_uri = client.open_from_disk(&main);

    // The duplicate definition's primary span is in twice.lu: the E0302
    // publish must arrive for the sibling's URI...
    let sib_diags = client.wait_publish(&sib_uri);
    assert_eq!(sib_diags.len(), 1, "{sib_diags:?}");
    assert_eq!(sib_diags[0]["code"], "E0302");
    assert_eq!(sib_diags[0]["severity"], 1);
    // ...with the first definition site (in main.lu) riding as related
    // information, so the entry document is still one hop away.
    let related = sib_diags[0]["relatedInformation"]
        .as_array()
        .expect("cross-file related information");
    assert!(
        related
            .iter()
            .any(|r| r["location"]["uri"].as_str() == Some(main_uri.as_str())),
        "related info points into the entry file: {related:?}"
    );

    // The entry file itself gets its own (empty) publish — no
    // diagnostic's primary lands there.
    let main_diags = client.wait_publish(&main_uri);
    assert_eq!(main_diags, Vec::<Value>::new());
    client.shutdown();
}

/// The lifecycle exit-code rule (LSP spec): `exit` without a prior
/// `shutdown` request exits 1. The success order (shutdown → exit = 0)
/// is asserted by `Client::shutdown` in every other test.
#[test]
fn bare_exit_without_shutdown_exits_one() {
    let (client, _) = Client::start(&["utf-8"]);
    assert_eq!(client.exit_without_shutdown(), 1);
}

/// Requests after shutdown are refused with InvalidRequest, unknown
/// methods with MethodNotFound — every request gets *some* response.
#[test]
fn lifecycle_edges() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let id = client.request("workspace/executeCommand", json!({ "command": "x" }));
    let resp = client.wait_response(id);
    assert_eq!(resp.unwrap_err().code, -32601);

    let id = client.request("shutdown", Value::Null);
    let resp = client.wait_response(id);
    assert!(resp.is_ok());
    let id = client.request("textDocument/hover", json!({}));
    let resp = client.wait_response(id);
    assert_eq!(resp.unwrap_err().code, -32600);
    client.notify("exit", Value::Null);
}
