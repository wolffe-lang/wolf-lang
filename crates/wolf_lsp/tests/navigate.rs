//! Protocol-layer navigation tests (s133): `textDocument/definition`,
//! `textDocument/references`, `textDocument/prepareRename` and
//! `textDocument/rename` — advertised, served from the binding table,
//! cross-file through the module graph, shaped per the client's
//! declared capabilities (`linkSupport`, `workspaceEdit.
//! documentChanges`), refused by name where the contract says so, and
//! honoring the negotiated position encoding at every span.

mod support;

use serde_json::{Value, json};
use support::{Client, fixture};

/// (line, character) of the `nth` occurrence of `needle` in `text`,
/// in utf-8 units (byte columns) — the encoding these tests negotiate
/// unless they say otherwise.
fn pos(text: &str, needle: &str, nth: usize) -> (u64, u64) {
    let off = text.match_indices(needle).nth(nth).expect("needle").0;
    let line = text[..off].matches('\n').count() as u64;
    let line_start = text[..off].rfind('\n').map_or(0, |i| i + 1);
    (line, (off - line_start) as u64)
}

fn range(text: &str, needle: &str, nth: usize, len: u64) -> Value {
    let (l, c) = pos(text, needle, nth);
    json!({ "start": { "line": l, "character": c }, "end": { "line": l, "character": c + len } })
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

fn geo_text() -> String {
    std::fs::read_to_string(fixture("navigate/shapes/geo.lu")).unwrap()
}

/// The three providers are advertised, rename with `prepareProvider`.
#[test]
fn initialize_advertises_the_navigation_trio() {
    let (client, init) = Client::start(&["utf-8"]);
    let caps = &init["capabilities"];
    assert_eq!(caps["definitionProvider"], true);
    assert_eq!(caps["referencesProvider"], true);
    assert_eq!(caps["renameProvider"], json!({ "prepareProvider": true }));
    client.shutdown();
}

/// Definition, same file: a method call reaches `fn sum`; a field
/// through `.` reaches its declaration; a variant in a pattern reaches
/// the enum. Plain `Location[]` for a client without `linkSupport`.
#[test]
fn definition_answers_locations_in_the_same_file() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let text = main_text();
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    let _ = client.wait_publish(&uri);

    let def = |client: &mut Client, needle: &str, nth: usize, plus: u64| -> Value {
        let (l, c) = pos(&text, needle, nth);
        let id = client.request("textDocument/definition", doc_pos(&uri, (l, c + plus)));
        client.wait_response(id).expect("definition answers")
    };
    let loc = def(&mut client, "p.sum()", 0, 2);
    assert_eq!(
        loc,
        json!([{ "uri": uri.as_str(), "range": shift(range(&text, "fn sum", 0, 3), 3) }])
    );
    let loc = def(&mut client, "p.x", 0, 2);
    assert_eq!(
        loc,
        json!([{ "uri": uri.as_str(), "range": shift(range(&text, "    x: int", 0, 1), 4) }])
    );
    let loc = def(&mut client, "Rgb(r, g, b)", 0, 0);
    assert_eq!(
        loc,
        json!([{ "uri": uri.as_str(), "range": shift(range(&text, "    Rgb(int", 0, 3), 4) }])
    );
    // A prelude name and a builtin type answer null — never an error.
    assert_eq!(def(&mut client, "print(", 0, 0), Value::Null);
    assert_eq!(def(&mut client, "x: int", 0, 3), Value::Null);
    client.shutdown();
}

/// Shift a single-line range right by `n` characters.
fn shift(r: Value, n: u64) -> Value {
    json!({
        "start": { "line": r["start"]["line"], "character": r["start"]["character"].as_u64().unwrap() + n },
        "end": { "line": r["end"]["line"], "character": r["end"]["character"].as_u64().unwrap() + n },
    })
}

/// Cross-file definition: `shapes.area` reaches the sibling's `pub fn
/// area` at a `file://` URI the client never opened; with
/// `linkSupport` the answer is a `LocationLink[]` carrying the asking
/// token as `originSelectionRange`.
#[test]
fn definition_crosses_files_and_links_when_the_client_can() {
    let (mut client, _) = Client::start_with(json!({
        "general": { "positionEncodings": ["utf-8"] },
        "textDocument": { "definition": { "linkSupport": true } },
    }));
    let text = main_text();
    let geo = geo_text();
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    let _ = client.wait_publish(&uri);
    let geo_uri = support::uri_of(&fixture("navigate/shapes/geo.lu"));

    let (l, c) = pos(&text, "shapes.area(3)", 0);
    let id = client.request("textDocument/definition", doc_pos(&uri, (l, c + 7)));
    let links = client.wait_response(id).expect("definition answers");
    let target = shift(range(&geo, "pub fn area", 0, 4), 7);
    assert_eq!(
        links,
        json!([{
            "originSelectionRange": shift(range(&text, "shapes.area(3)", 0, 4), 7),
            "targetUri": geo_uri.as_str(),
            "targetRange": target,
            "targetSelectionRange": target,
        }])
    );
    client.shutdown();
}

/// References: package-wide, (file, offset) ordered, the declaration
/// only when `includeDeclaration` asks — from the use side and from
/// the declaration side of the file boundary.
#[test]
fn references_are_package_wide_and_ordered() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let text = main_text();
    let geo = geo_text();
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    let _ = client.wait_publish(&uri);
    let geo_uri = support::uri_of(&fixture("navigate/shapes/geo.lu"));

    let refs = |client: &mut Client, uri: &lsp_types::Url, p: (u64, u64), decl: bool| -> Value {
        let mut params = doc_pos(uri, p);
        params["context"] = json!({ "includeDeclaration": decl });
        let id = client.request("textDocument/references", params);
        client.wait_response(id).expect("references answer")
    };
    let use_main =
        json!({ "uri": uri.as_str(), "range": shift(range(&text, "shapes.area(3)", 0, 4), 7) });
    let decl_geo =
        json!({ "uri": geo_uri.as_str(), "range": shift(range(&geo, "pub fn area", 0, 4), 7) });
    let use_geo = json!({ "uri": geo_uri.as_str(), "range": range(&geo, "area(side)", 0, 4) });

    let (l, c) = pos(&text, "shapes.area(3)", 0);
    assert_eq!(
        refs(&mut client, &uri, (l, c + 7), false),
        json!([use_main, use_geo])
    );
    assert_eq!(
        refs(&mut client, &uri, (l, c + 7), true),
        json!([use_main, decl_geo, use_geo])
    );

    // From the sibling, opened now: the same set, same order.
    let geo_open = client.open_from_disk(&fixture("navigate/shapes/geo.lu"));
    let _ = client.wait_publish(&geo_open);
    let (l, c) = pos(&geo, "pub fn area", 0);
    assert_eq!(
        refs(&mut client, &geo_open, (l, c + 7), true),
        json!([decl_geo, use_geo]),
        "a sibling opened as its own entry is a one-file package (D32 entry rule)"
    );
    client.shutdown();
}

/// Rename: `prepareRename` answers the token and placeholder; `rename`
/// answers a `WorkspaceEdit` with `documentChanges` when the client
/// declares it (every file that names the symbol, the declaration and
/// the `use`-site segment included) and `changes` otherwise.
#[test]
fn rename_edits_every_file_in_the_clients_shape() {
    let text = main_text();
    let geo = geo_text();
    let new = "square";
    let expect_main =
        json!([{ "range": shift(range(&text, "shapes.area(3)", 0, 4), 7), "newText": new }]);
    let expect_geo = json!([
        { "range": shift(range(&geo, "pub fn area", 0, 4), 7), "newText": new },
        { "range": range(&geo, "area(side)", 0, 4), "newText": new },
    ]);

    // documentChanges declared (facsimile's shape).
    let (mut client, _) = Client::start_with(json!({
        "general": { "positionEncodings": ["utf-8"] },
        "workspace": { "workspaceEdit": { "documentChanges": true } },
    }));
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    let _ = client.wait_publish(&uri);
    let geo_uri = support::uri_of(&fixture("navigate/shapes/geo.lu"));
    let (l, c) = pos(&text, "shapes.area(3)", 0);

    let id = client.request("textDocument/prepareRename", doc_pos(&uri, (l, c + 7)));
    assert_eq!(
        client.wait_response(id).expect("prepare answers"),
        json!({ "range": shift(range(&text, "shapes.area(3)", 0, 4), 7), "placeholder": "area" })
    );

    let mut params = doc_pos(&uri, (l, c + 7));
    params["newName"] = json!(new);
    let id = client.request("textDocument/rename", params.clone());
    let edit = client.wait_response(id).expect("rename answers");
    assert_eq!(
        edit,
        json!({
            "documentChanges": [
                { "textDocument": { "uri": uri.as_str(), "version": null }, "edits": expect_main },
                { "textDocument": { "uri": geo_uri.as_str(), "version": null }, "edits": expect_geo },
            ]
        })
    );
    client.shutdown();

    // Not declared: the `changes` map.
    let (mut client, _) = Client::start(&["utf-8"]);
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    let _ = client.wait_publish(&uri);
    let id = client.request("textDocument/rename", params);
    let edit = client.wait_response(id).expect("rename answers");
    assert_eq!(
        edit,
        json!({ "changes": { uri.as_str(): expect_main, geo_uri.as_str(): expect_geo } })
    );
    client.shutdown();
}

/// Rename refuses by name — a `ResponseError` (`RequestFailed`,
/// -32803) whose message names the token and the reason; never a
/// partial edit. Keywords, builtins, prelude names, modules, and a
/// non-identifier new name.
#[test]
fn rename_refuses_by_name_with_a_response_error() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let text = main_text();
    let uri = client.open_from_disk(&fixture("navigate/main.lu"));
    let _ = client.wait_publish(&uri);

    let refuse =
        |client: &mut Client, method: &str, needle: &str, plus: u64, new: &str| -> String {
            let (l, c) = pos(&text, needle, 0);
            let mut params = doc_pos(&uri, (l, c + plus));
            if method == "textDocument/rename" {
                params["newName"] = json!(new);
            }
            let id = client.request(method, params);
            let err = client.wait_response(id).expect_err("refused");
            assert_eq!(err.code, -32803, "RequestFailed: {err:?}");
            err.message
        };
    let r = "textDocument/rename";
    let p = "textDocument/prepareRename";
    assert!(refuse(&mut client, r, "fn main", 0, "g").contains("`fn` is a keyword"));
    assert!(refuse(&mut client, p, "print(", 0, "").contains("`print` is a prelude name"));
    assert!(refuse(&mut client, r, "x: int", 3, "num").contains("`int` is a builtin type"));
    assert!(refuse(&mut client, r, "shapes.area", 0, "geo").contains("directory"));
    assert!(refuse(&mut client, r, "    x: int", 4, "let").contains("`let` is a keyword"));
    assert!(refuse(&mut client, r, "    x: int", 4, "9x").contains("not an identifier"));

    // Nothing at the position: null, not an error.
    let (l, c) = pos(&text, "\n\n", 0);
    let id = client.request(p, doc_pos(&uri, (l, c)));
    assert_eq!(client.wait_response(id).unwrap(), Value::Null);
    client.shutdown();
}

/// The negotiated encoding holds at every span: under utf-16 an
/// astral character before the token moves the request position and
/// the answered ranges by one unit less than the bytes say.
#[test]
fn navigation_honors_utf16_positions() {
    let (mut client, init) = Client::start(&[]);
    assert_eq!(init["capabilities"]["positionEncoding"], "utf-16");
    let base = main_text();
    let text = base.replace(
        "let total = p.sum() + p.x",
        "let total = \"\u{1f43a}\".len + p.sum() + p.x",
    );
    assert_ne!(text, base);
    let uri = client.open(&fixture("navigate/main.lu"), &text);
    let diags = client.wait_publish(&uri);
    assert_eq!(
        diags,
        Vec::<Value>::new(),
        "the overlay is clean: {diags:?}"
    );

    // `p.x`'s `x`: byte column minus 2 (the 4-byte wolf is 2 units).
    let (l, byte_col) = pos(&text, "p.x", 0);
    let utf16_col = byte_col + 2 - 2;
    let id = client.request("textDocument/definition", doc_pos(&uri, (l, utf16_col)));
    let loc = client.wait_response(id).expect("definition answers");
    assert_eq!(
        loc,
        json!([{ "uri": uri.as_str(), "range": shift(range(&text, "    x: int", 0, 1), 4) }])
    );

    // References on the field: the use on the astral line comes back
    // in utf-16 units.
    let (dl, dc) = pos(&text, "    x: int", 0);
    let mut params = doc_pos(&uri, (dl, dc + 4));
    params["context"] = json!({ "includeDeclaration": false });
    let id = client.request("textDocument/references", params);
    let refs = client.wait_response(id).expect("references answer");
    let on_astral_line: Vec<&Value> = refs
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["range"]["start"]["line"] == l)
        .collect();
    assert_eq!(on_astral_line.len(), 1, "{refs}");
    assert_eq!(
        on_astral_line[0]["range"],
        json!({ "start": { "line": l, "character": utf16_col },
                "end": { "line": l, "character": utf16_col + 1 } })
    );
    client.shutdown();
}
