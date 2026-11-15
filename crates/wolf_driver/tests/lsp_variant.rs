//! Framed stdio witness for enum navigation in an open package context.
mod lsp_client;

use lsp_client::{LspChild, file_uri};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../wolf_lsp/tests/fixtures/variant")
        .join(rel)
        .canonicalize()
        .unwrap()
}

fn location(path: &Path, nth: usize) -> Value {
    let text = std::fs::read_to_string(path).unwrap();
    let off = text.match_indices("Foreign").nth(nth).unwrap().0;
    let line = text[..off].bytes().filter(|&b| b == b'\n').count();
    let col = off - text[..off].rfind('\n').map_or(0, |i| i + 1);
    json!({"uri": file_uri(path), "range": {
        "start": {"line": line, "character": col},
        "end": {"line": line, "character": col + 7}
    }})
}

#[cfg_attr(
    windows,
    ignore = "windows: piped stdio server gap — docs/backlog.md#lsp-windows-stdio"
)]
#[test]
fn variant_references_and_rename_over_stdio() {
    let root = fixture("main.lu");
    let member = fixture("status/status.lu");
    let mut lsp = LspChild::start();
    let id = lsp.request(
        "initialize",
        json!({"capabilities": {"general": {"positionEncodings": ["utf-8"]}}}),
    );
    assert!(lsp.wait_response(id).get("error").is_none());
    lsp.notify("initialized", json!({}));
    for path in [&root, &member] {
        let uri = file_uri(path);
        lsp.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": uri, "languageId": "wolf", "version": 1,
                "text": std::fs::read_to_string(path).unwrap()
            }}),
        );
        let _ = lsp.wait_publish(&uri);
    }
    for nth in [0, 2, 3] {
        let loc = location(&member, nth);
        let params =
            json!({"textDocument": {"uri": loc["uri"]}, "position": loc["range"]["start"]});
        for include in [false, true] {
            let mut p = params.clone();
            p["context"] = json!({"includeDeclaration": include});
            let id = lsp.request("textDocument/references", p);
            let mut want = vec![location(&root, 0), location(&root, 1)];
            if include {
                want.push(location(&member, 0));
            }
            want.extend([location(&member, 2), location(&member, 3)]);
            assert_eq!(lsp.wait_response(id)["result"], json!(want));
        }
        let id = lsp.request("textDocument/definition", params.clone());
        assert_eq!(
            lsp.wait_response(id)["result"],
            json!([location(&member, 0)])
        );
        let mut p = params;
        p["newName"] = json!("External");
        let id = lsp.request("textDocument/rename", p);
        let resp = lsp.wait_response(id);
        let changes = &resp["result"]["changes"];
        assert_eq!(changes.as_object().unwrap().len(), 2);
        for (path, occurrences) in [(&root, vec![0, 1]), (&member, vec![0, 2, 3])] {
            let edits: Vec<_> = occurrences
                .into_iter()
                .map(|i| {
                    json!({
                        "range": location(path, i)["range"], "newText": "External"
                    })
                })
                .collect();
            assert_eq!(changes[file_uri(path)], json!(edits));
        }
    }
    lsp.shutdown();
}
