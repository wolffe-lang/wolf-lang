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

/// The legend's type names, in the order `initialize` advertises them.
const TYPES: [&str; 8] = [
    "namespace",
    "type",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "function",
    "keyword",
];

/// One decoded token, rendered `line:col len type[+modifiers] 'text'`
/// — the stream as a reader of the issue's table would write it.
fn render(text: &str, toks: &[(u64, u64, u64, u64, u64)]) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    toks.iter()
        .map(|&(l, c, len, ty, m)| {
            let s: String = lines[l as usize]
                .chars()
                .skip(c as usize)
                .take(len as usize)
                .collect();
            let mut mods = String::new();
            if m & 1 != 0 {
                mods.push_str("+declaration");
            }
            if m & 2 != 0 {
                mods.push_str("+readonly");
            }
            format!("{l}:{c} {len} {}{mods} {s:?}", TYPES[ty as usize])
        })
        .collect()
}

/// wolf-lang#356 — the contextual `then` got **no token at all**: a
/// hole in the stream, not a miscolouring.
///
/// `is_keyword_kind` in the semantic-token walk carried its own
/// `AsKw..=WhileKw` range with `SelfKw` special-cased past the end, so
/// `ThenKw` — a kind declared after `SelfKw` by s151 — matched neither
/// that test nor the `Ident` test under it, and `classify_token`
/// returned `None`. The editor rendered `if c then a else b` with `if`
/// and `else` coloured and `then` as plain text.
///
/// The whole decoded stream is pinned, not the one token, and that is
/// the point: the defect survived `v0.2.5..v0.2.12` because every
/// existing assertion looks a token up by position and an absent token
/// is only ever a count nobody took. A hole is what a count missed.
#[test]
fn the_contextual_then_is_a_keyword_and_the_whole_stream_is_pinned() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = support::corpus("grammar/if_then_ident.lu");
    let text = std::fs::read_to_string(&path).unwrap();
    let uri = client.open_from_disk(&path);
    client.wait_publish(&uri);
    let id = client.request(
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri.as_str() } }),
    );
    let toks = decode(&client.wait_response(id).unwrap()["data"]);
    // The whole stream, in source order. `then` appears four times in
    // this file and each one is classified for what it is in its own
    // position: the binder, two identifier reads, and — at 11:20,
    // between the `if` at 11:12 and the `else` at 11:27 — the keyword.
    // The fifth `then`, inside the string literal, is not a token.
    assert_eq!(
        render(&text, &toks),
        [
            "8:0 2 keyword \"fn\"",
            "8:3 4 function+declaration \"main\"",
            "8:14 3 type \"int\"",
            "9:4 3 keyword \"let\"",
            "9:8 4 variable+declaration+readonly \"then\"",
            "9:15 4 keyword \"true\"",
            "10:4 2 keyword \"if\"",
            "10:7 4 variable+readonly \"then\"",
            "10:14 5 function \"print\"",
            "11:4 3 keyword \"let\"",
            "11:8 1 variable+declaration+readonly \"n\"",
            "11:12 2 keyword \"if\"",
            "11:15 4 variable+readonly \"then\"",
            "11:20 4 keyword \"then\"",
            "11:27 4 keyword \"else\"",
            "12:4 5 function \"print\"",
            "12:12 1 variable+readonly \"n\"",
        ]
    );

    // Said again as a property, because the pin above is a list a
    // future edit can "fix" by deleting a row: every `then` this file
    // spells outside the string literal reaches the stream, and the
    // one between `if` and `else` is the keyword.
    let rendered = render(&text, &toks);
    let thens: Vec<&String> = rendered
        .iter()
        .filter(|r| r.ends_with("\"then\""))
        .collect();
    assert_eq!(thens.len(), 4, "four `then` tokens, no hole: {thens:?}");
    assert_eq!(
        thens.iter().filter(|r| r.contains(" keyword ")).count(),
        1,
        "exactly one of them is the contextual keyword: {thens:?}"
    );

    // The `/range` request over line 11 is the same stream, clipped —
    // the hole was not a full-document artifact, so neither is the fix.
    let id = client.request("textDocument/semanticTokens/range", json!({
        "textDocument": { "uri": uri.as_str() },
        "range": { "start": { "line": 11, "character": 0 }, "end": { "line": 12, "character": 0 } },
    }));
    let ranged = decode(&client.wait_response(id).unwrap()["data"]);
    assert_eq!(
        render(&text, &ranged),
        [
            "11:4 3 keyword \"let\"",
            "11:8 1 variable+declaration+readonly \"n\"",
            "11:12 2 keyword \"if\"",
            "11:15 4 variable+readonly \"then\"",
            "11:20 4 keyword \"then\"",
            "11:27 4 keyword \"else\"",
        ]
    );
    client.shutdown();
}

/// The same hole, one kind over, and nobody had measured it:
/// `ErrorKw` — the contextual `error` of an error-set alias, s158's
/// `[gram.item.error]` — was declared right after `ThenKw` and fell
/// through the same two tests, so `error IoErrors = {none, parse}`
/// opened with a word the server classified as nothing at all.
///
/// s158 did think about this item's tokens: it taught the walk that
/// the alias NAME highlights as a `type`. The keyword introducing it
/// was still a hole, because a lookup-by-position assertion for the
/// name passes either way.
#[test]
fn the_contextual_error_keyword_is_a_keyword_too() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = support::corpus("rows/error_alias_row.lu");
    let text = std::fs::read_to_string(&path).unwrap();
    let uri = client.open_from_disk(&path);
    client.wait_publish(&uri);
    let id = client.request(
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri.as_str() } }),
    );
    let toks = decode(&client.wait_response(id).unwrap()["data"]);
    let rendered = render(&text, &toks);
    // The alias declaration line, whole: the keyword, then the name as
    // a `type` declaration. The tags inside the row are not names the
    // resolver binds, so they carry no token — that is by design and
    // is pinned here so it stays a decision rather than a hole.
    let line: Vec<&String> = rendered.iter().filter(|r| r.starts_with("12:")).collect();
    assert_eq!(
        line,
        [
            "12:0 5 keyword \"error\"",
            "12:6 8 type+declaration \"IoErrors\"",
        ],
        "whole stream: {rendered:?}"
    );
    // And `error` as an identifier stays an identifier: the `else`
    // spelling below binds no `error`, but the tag `parse` in
    // `return parse` is a tag, not a keyword, either way.
    assert!(
        !rendered.iter().any(|r| r.contains(" keyword \"parse\"")),
        "a tag is not a keyword: {rendered:?}"
    );
    client.shutdown();
}

/// wolf-lang#379 — the alias NAME was a `type` where it was declared and
/// nothing where it was used: the resolver records no reference for an
/// error row's entries (they are structural), so the walk had nothing
/// to classify at `{IoErrors, closed}` or at `-> int ! ConfigErrors`,
/// while `bool` and `int` on the same line were `type`. An entry that
/// names an error-set alias is bound now, to the same item its
/// declaration is; the TAGS beside it (`closed`) still carry no token.
/// wolf-lsp's `transcripts/annotate/semanticTokens-error.lsps` sees
/// exactly these two new tokens at its next pin.
#[test]
fn an_error_set_alias_is_a_type_where_it_is_used_too() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = support::corpus("rows/error_alias_union.lu");
    let text = std::fs::read_to_string(&path).unwrap();
    let uri = client.open_from_disk(&path);
    client.wait_publish(&uri);
    let id = client.request(
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri.as_str() } }),
    );
    let toks = decode(&client.wait_response(id).unwrap()["data"]);
    let rendered = render(&text, &toks);
    let line = |n: &str| -> Vec<String> {
        rendered
            .iter()
            .filter(|r| r.starts_with(n))
            .cloned()
            .collect()
    };
    assert_eq!(
        line("13:"),
        [
            "13:0 5 keyword \"error\"",
            "13:6 12 type+declaration \"ConfigErrors\"",
            "13:22 8 type \"IoErrors\"",
        ],
        "whole stream: {rendered:?}"
    );
    assert_eq!(
        line("15:"),
        [
            "15:0 2 keyword \"fn\"",
            "15:3 4 function+declaration \"load\"",
            "15:8 2 parameter+declaration \"ok\"",
            "15:12 4 type \"bool\"",
            "15:21 3 type \"int\"",
            "15:27 12 type \"ConfigErrors\"",
        ],
        "whole stream: {rendered:?}"
    );
    client.shutdown();
}
