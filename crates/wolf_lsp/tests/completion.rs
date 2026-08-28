//! Protocol-layer completion tests (s122): the capability is
//! advertised with its trigger character, name-position completion
//! answers keywords + in-scope names with kinds and details, member
//! position answers from the receiver's type, and — the normal case,
//! not the edge case — every one of the member/broken tests here runs
//! against a buffer that does NOT parse, pinning the
//! incomplete-buffer contract: answer from what the frontend
//! recovered, empty (never an error) when the receiver cannot be
//! typed.

mod support;

use serde_json::{Value, json};
use support::{Client, fixture};

/// LSP `CompletionItemKind` values asserted below.
const KIND_METHOD: u64 = 2;
const KIND_FUNCTION: u64 = 3;
const KIND_FIELD: u64 = 5;
const KIND_VARIABLE: u64 = 6;
const KIND_ENUM_MEMBER: u64 = 20;
const KIND_KEYWORD: u64 = 14;

fn complete_at(client: &mut Client, uri: &lsp_types::Url, line: u32, character: u32) -> Vec<Value> {
    let id = client.request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": line, "character": character },
        }),
    );
    client
        .wait_response(id)
        .expect("completion answers")
        .as_array()
        .expect("completion returns a list")
        .clone()
}

fn find<'a>(items: &'a [Value], label: &str) -> Option<&'a Value> {
    items.iter().find(|i| i["label"] == label)
}

/// The capability is advertised, with `.` as the trigger character
/// (member access AND the module path separator — one char, both
/// meanings).
#[test]
fn initialize_advertises_completion_with_dot_trigger() {
    let (client, init) = Client::start(&["utf-8"]);
    assert_eq!(
        init["capabilities"]["completionProvider"]["triggerCharacters"],
        json!(["."])
    );
    client.shutdown();
}

/// Name position in a clean buffer: keywords, functions (signature +
/// doc), parameters and typed locals all complete with correct kinds.
#[test]
fn scope_names_and_keywords_complete_with_kinds_and_details() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("typed.lu");
    let uri = client.open_from_disk(&path);
    let _ = client.wait_publish(&uri);

    // Inside `add`'s body, line 4 `    s` col 5 (after the `s`).
    let items = complete_at(&mut client, &uri, 4, 5);

    let s = find(&items, "s").expect("local `s` completes");
    assert_eq!(s["kind"], KIND_VARIABLE);
    assert_eq!(s["detail"], "s: int", "typed local carries its type");

    let a = find(&items, "a").expect("param `a` completes");
    assert_eq!(a["kind"], KIND_VARIABLE);
    assert_eq!(a["detail"], "a: int", "param carries its annotation");

    let add = find(&items, "add").expect("fn `add` completes");
    assert_eq!(add["kind"], KIND_FUNCTION);
    assert_eq!(add["detail"], "fn add(a: int, b: int) -> int");
    let doc = add["documentation"]["value"].as_str().unwrap();
    assert!(doc.contains("Adds two numbers."), "doc rides: {doc}");

    let kw = find(&items, "let").expect("keyword `let` completes");
    assert_eq!(kw["kind"], KIND_KEYWORD);
    // Reserved words that are not idents anywhere else still offer.
    assert!(find(&items, "errdefer").is_some(), "full reserved set");
    client.shutdown();
}

/// `.` after a `str` receiver, mid-edit: the buffer does NOT parse
/// (trailing `s.`), the server repairs and answers the builtin `str`
/// method surface. Snapshot-reviewed: this list is the s37/s120
/// method set, and any drift from `wolf_sema::check`'s table must
/// show up here as a reviewed diff.
#[test]
fn dot_after_str_receiver_offers_methods_mid_edit() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("typed.lu");
    let uri = client.open(&path, "fn f(s: str) -> str {\n    s.\n}\n");
    let diags = client.wait_publish(&uri);
    assert!(!diags.is_empty(), "the buffer is mid-edit broken");

    // Cursor right after the dot (line 1 `    s.` col 6).
    let items = complete_at(&mut client, &uri, 1, 6);
    assert!(!items.is_empty(), "str members answer on a broken buffer");
    for i in &items {
        assert_eq!(i["kind"], KIND_METHOD, "every str member is a method");
    }
    let shaped: Vec<String> = items
        .iter()
        .map(|i| {
            format!(
                "{} · {}",
                i["label"].as_str().unwrap(),
                i["detail"].as_str().unwrap()
            )
        })
        .collect();
    insta::assert_snapshot!(shaped.join("\n"));
    client.shutdown();
}

/// Member completion through a declared annotation when the repaired
/// body still has a type error: struct fields with kinds + types.
#[test]
fn struct_fields_complete_after_dot() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("symbols.lu");
    let text = "struct Point {\n    x: int\n    y: int\n}\n\n\
                enum Color {\n    Red\n    Blue\n}\n\n\
                fn dist(p: Point) -> int {\n    p.\n}\n";
    let uri = client.open(&path, text);
    let _ = client.wait_publish(&uri);

    // Line 11 `    p.` col 6.
    let items = complete_at(&mut client, &uri, 11, 6);
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert_eq!(labels, ["x", "y"], "exactly the fields: {items:?}");
    assert_eq!(items[0]["kind"], KIND_FIELD);
    assert_eq!(items[0]["detail"], "x: int");
    client.shutdown();
}

/// Member completion after an enum *type name*: the variants.
#[test]
fn enum_variants_complete_after_type_name_dot() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("symbols.lu");
    let text = "enum Color {\n    Red\n    Blue\n}\n\n\
                fn pick() -> int {\n    Color.\n}\n";
    let uri = client.open(&path, text);
    let _ = client.wait_publish(&uri);

    // Line 6 `    Color.` col 10.
    let items = complete_at(&mut client, &uri, 6, 10);
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert_eq!(labels, ["Red", "Blue"], "the variants: {items:?}");
    assert_eq!(items[0]["kind"], KIND_ENUM_MEMBER);
    client.shutdown();
}

/// Member completion after an import binding: the module's exported
/// items, with elaborated signatures.
#[test]
fn module_members_complete_after_import_binding_dot() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("unused/main.lu");
    let text = "use util\n\nfn main() -> int {\n    util.\n    0\n}\n";
    let uri = client.open(&path, text);
    let _ = client.wait_publish(&uri);

    // Line 3 `    util.` col 9.
    let items = complete_at(&mut client, &uri, 3, 9);
    let helper = find(&items, "helper").expect("util's pub fn completes");
    assert_eq!(helper["kind"], KIND_FUNCTION);
    assert_eq!(helper["detail"], "fn helper() -> int");
    client.shutdown();
}

/// A receiver nobody can type answers the EMPTY list — conservative,
/// never a guess, never an error (the track's no-garbage rule).
#[test]
fn unknown_receiver_completes_empty_not_wrong() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("typed.lu");
    let uri = client.open(&path, "fn f(x: Wumpus) -> int {\n    x.\n}\n");
    let _ = client.wait_publish(&uri);

    let items = complete_at(&mut client, &uri, 1, 6);
    assert_eq!(items, Vec::<Value>::new(), "empty, honestly");
    client.shutdown();
}

/// A buffer whose parse recovery is partial (a syntax error on the
/// ladder's parse rung) still answers: keywords always, plus whatever
/// the resilient tree recovered.
#[test]
fn broken_buffer_still_answers_keywords_and_recovered_items() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("broken.lu");
    let uri = client.open_from_disk(&path); // `lett x = 1` — parse error
    let _ = client.wait_publish(&uri);

    let items = complete_at(&mut client, &uri, 2, 4);
    let kw = find(&items, "let").expect("keywords answer on broken code");
    assert_eq!(kw["kind"], KIND_KEYWORD);
    let main = find(&items, "main").expect("recovered item answers");
    assert_eq!(main["kind"], KIND_FUNCTION);
    client.shutdown();
}

/// Even when the ladder dies at LEX (parse recovery fails), the
/// request answers a list — never an error, never a hang. This is the
/// floor of the incomplete-buffer contract.
#[test]
fn lex_dead_buffer_still_answers_a_list() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = fixture("typed.lu");
    // The emoji is not wolf syntax: E0107 stray byte, lex rung stops.
    let uri = client.open(
        &path,
        "fn main() -> int {\n    let \u{1f43a} = 1\n    0\n}\n",
    );
    let _ = client.wait_publish(&uri);

    let items = complete_at(&mut client, &uri, 2, 4);
    assert!(
        find(&items, "let").is_some(),
        "keywords are the floor: {items:?}"
    );
    client.shutdown();
}

/// Latency probe on the largest corpus file — report-only, not a CI
/// gate (the JSONL gate rides examples/lsp_bench.rs). Run manually:
/// `cargo test -p wolf_lsp --release --test completion -- --ignored --nocapture`
#[test]
#[ignore = "latency probe; report-only, run with --ignored --nocapture"]
fn member_completion_latency_probe_large_file() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let path = support::corpus("conc/spawn_cluster_split.lu");
    let mut text = std::fs::read_to_string(&path).unwrap();
    let base_lines = text.lines().count() as u32;
    text.push_str("\nfn probe_zz(s: str) -> str {\n    s.\n}\n");
    let uri = client.open(&path, &text);
    let _ = client.wait_publish(&uri);

    let mut samples: Vec<f64> = Vec::new();
    for _ in 0..30 {
        let t = std::time::Instant::now();
        let items = complete_at(&mut client, &uri, base_lines + 2, 6);
        samples.push(t.elapsed().as_secs_f64() * 1000.0);
        assert!(!items.is_empty(), "str members on the appended receiver");
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    println!(
        "member completion, {} bytes, cold repair each request: p50={:.3}ms p95={:.3}ms max={:.3}ms",
        text.len(),
        samples[samples.len() / 2],
        samples[(samples.len() - 1) * 95 / 100],
        samples.last().unwrap()
    );
    client.shutdown();
}
