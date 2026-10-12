//! The server with a std (s235, wolf-lang#724): a host built with roots
//! resolves `std` the way `wolf build` does, so a std data type's
//! home-module methods (`[type.method.home]`) are not E0301, and hover,
//! definition and member completion answer on them. The fixture std
//! tree (`fixtures/std_tree`) holds one module, `std.map`; the package
//! (`fixtures/std_user`) calls `Map.keys` on it.
//!
//! The order itself (`--std-root`, `WOLF_STD`, `wolf.pkg`, beside the
//! binary) is the driver's, tested over the real `wolf lsp` process in
//! `wolf_driver/tests/lsp_std_root.rs`; here the resolver is a fixed
//! answer, so these rows pin what the query layer does with one.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use support::{Client, fixture, uri_of};

fn std_tree() -> PathBuf {
    fixture("std_tree").canonicalize().expect("std tree exists")
}

fn user_entry() -> PathBuf {
    fixture("std_user/main.lu")
        .canonicalize()
        .expect("package exists")
}

/// A resolver that answers the fixture std for every package.
fn with_std() -> wolf_lsp::RootsResolver {
    let root = std_tree();
    Arc::new(move |_: &Path| wolf_lsp::PackageRoots {
        std_root: Some(root.clone()),
        dep_roots: Default::default(),
    })
}

/// `(line, character)` of the first `needle` on 0-based `line`.
fn at(text: &str, line: usize, needle: &str) -> (u32, u32) {
    let l = text.lines().nth(line).expect("line exists");
    (line as u32, l.find(needle).expect("needle on the line") as u32)
}

fn position(uri: &lsp_types::Url, (line, character): (u32, u32)) -> Value {
    json!({
        "textDocument": { "uri": uri.as_str() },
        "position": { "line": line, "character": character },
    })
}

/// No roots: the stub's E0301 on `m.keys()` — the bug, kept as the
/// host's documented std-less behavior so the rows below are not
/// vacuous (the same file, the same bytes).
#[test]
fn without_roots_a_home_method_is_e0301() {
    let (mut client, _) = Client::start(&["utf-8"]);
    let uri = client.open_from_disk(&user_entry());
    let diags = client.wait_publish(&uri);
    let codes: Vec<&str> = diags.iter().filter_map(|d| d["code"].as_str()).collect();
    assert_eq!(codes, ["E0301"], "{diags:?}");
    assert!(
        diags[0]["message"]
            .as_str()
            .unwrap()
            .contains("`keys` is not a builtin method"),
        "{diags:?}"
    );
    client.shutdown();
}

/// With the std root: no diagnostics at all (`wolf build` with the same
/// root is clean).
#[test]
fn with_roots_a_home_method_resolves() {
    let (mut client, _) = Client::start_with_roots(&["utf-8"], with_std());
    let uri = client.open_from_disk(&user_entry());
    assert_eq!(client.wait_publish(&uri), Vec::<Value>::new());
    client.shutdown();
}

/// Hover on `keys` answers std.map's declaration: the module, the
/// signature, and its doc comment, over the use's own range.
#[test]
fn hover_on_a_home_method_shows_its_std_declaration() {
    let (mut client, _) = Client::start_with_roots(&["utf-8"], with_std());
    let path = user_entry();
    let text = std::fs::read_to_string(&path).unwrap();
    let uri = client.open_from_disk(&path);
    let _ = client.wait_publish(&uri);
    let pos = at(&text, 1, "keys");
    let id = client.request("textDocument/hover", position(&uri, pos));
    let hover = client.wait_response(id).expect("hover answers");
    let value = hover["contents"]["value"].as_str().expect("markdown hover");
    assert!(
        value.starts_with("```wolf\n// std.map\nfn keys(m: Map[K, V]) -> List[K]\n```"),
        "{value}"
    );
    assert!(
        value.contains("Every key, in the map's iteration order."),
        "{value}"
    );
    assert_eq!(
        hover["range"],
        json!({ "start": { "line": pos.0, "character": pos.1 },
                "end": { "line": pos.0, "character": pos.1 + 4 } })
    );
    client.shutdown();
}

/// Definition on `keys` lands in the std tree's file — a real `file://`
/// path an editor can open, never the `std://` display — on the name of
/// `pub fn keys` (line 4 of the fixture).
#[test]
fn definition_on_a_home_method_opens_the_std_file() {
    let (mut client, _) = Client::start_with_roots(&["utf-8"], with_std());
    let path = user_entry();
    let text = std::fs::read_to_string(&path).unwrap();
    let uri = client.open_from_disk(&path);
    let _ = client.wait_publish(&uri);
    let id = client.request(
        "textDocument/definition",
        position(&uri, at(&text, 1, "keys")),
    );
    let def = client.wait_response(id).expect("definition answers");
    let want = uri_of(&std_tree().join("map/map.lu"));
    assert_eq!(
        def,
        json!([{ "uri": want.as_str(),
                 "range": { "start": { "line": 4, "character": 7 },
                            "end": { "line": 4, "character": 11 } } }]),
    );
    client.shutdown();
}

/// Rename refuses a std declaration by name: the std tree is outside the
/// package, as the stub's std names were (`navigate`'s refusal set).
#[test]
fn rename_refuses_a_std_declaration() {
    let (mut client, _) = Client::start_with_roots(&["utf-8"], with_std());
    let path = user_entry();
    let text = std::fs::read_to_string(&path).unwrap();
    let uri = client.open_from_disk(&path);
    let _ = client.wait_publish(&uri);
    let mut params = position(&uri, at(&text, 1, "keys"));
    params["newName"] = json!("all_keys");
    let id = client.request("textDocument/rename", params);
    let err = client
        .wait_response(id)
        .expect_err("rename of a std item is refused");
    assert!(
        err.message.contains("`keys` is a std item"),
        "{}",
        err.message
    );
    client.shutdown();
}

/// Member completion on a `Map` receiver lists std.map's methods — its
/// `pub fn`s whose first parameter is a `Map` — and not its other
/// `pub fn`s (`fresh`), while the user types `m.` on a new line.
#[test]
fn completion_lists_the_home_modules_methods() {
    let (mut client, _) = Client::start_with_roots(&["utf-8"], with_std());
    let path = user_entry();
    let text = std::fs::read_to_string(&path).unwrap();
    let uri = client.open_from_disk(&path);
    let _ = client.wait_publish(&uri);
    let edited = text.replacen("    m.keys()\n", "    let p = m.\n    m.keys()\n", 1);
    client.change(&uri, 2, &edited);
    let _ = client.wait_publish(&uri);
    let id = client.request(
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": 1, "character": 14 },
        }),
    );
    let items = client.wait_response(id).expect("completion answers");
    let labels: Vec<(&str, &str)> = items
        .as_array()
        .expect("a list")
        .iter()
        .map(|i| (i["label"].as_str().unwrap(), i["detail"].as_str().unwrap_or("")))
        .collect();
    assert_eq!(
        labels,
        [
            ("keys", "fn keys(m: Map[K, V]) -> List[K]"),
            ("values", "fn values(m: Map[K, V]) -> List[V]"),
        ],
        "{items}"
    );
    client.shutdown();
}
