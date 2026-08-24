//! s107 — the ONE json parser, pinned across the seam: the reference
//! implementation is `wolf_mem::json` (the checked lane's; its module
//! doc pins semantics), and `wolf_rt::json` is the hand mirror the
//! native lanes link — the two crates may not see each other (the
//! locked graph; D15), so THIS test is the single place that proves
//! they never drift. The `fmt_parity`/`net_parity` precedent, applied
//! to the query kernels.

/// The vector battery: every semantic corner the reference's own test
/// suite distinguishes — validity edges, escapes and surrogates,
/// number source-spelling, depth, path walking, container rendering,
/// and all three error kinds.
const TEXTS: &[&str] = &[
    // Validity edges.
    "null",
    "true",
    "false",
    "42",
    "-0.50",
    "1e3",
    "9223372036854775808",
    "01",
    "1.",
    ".5",
    "+1",
    "nul",
    "",
    " ",
    "[]",
    "[1,]",
    "{",
    "[1] [2]",
    "'a'",
    "\"\"",
    "\"\u{0009}\"",
    " { \"a\" : [1, 2.5, -3e+2] , \"b\" : \"x\" } ",
    // Escapes and surrogates.
    r#""a\nb\t\"\\\/""#,
    r#""Aé""#,
    r#""🐺""#,
    r#""🐺""#,
    r#""\ud83d""#,
    r#""\udc3a""#,
    r#""\x41""#,
    // Structures the paths below address.
    r#"{"users":[{"name":"lupin","tags":[1,2,3]},{"name":"ainu"}],"n":42}"#,
    r#" {"a": [1, {"b": "c d"}]} "#,
    r#"{"pack": [{"name": "lupin"}, {"name": "ainu"}], "n": 42, "a": [1, 2, 3], "b": null}"#,
];

const PATHS: &[&str] = &[
    "",
    "a",
    "b",
    "n",
    "x",
    "users",
    "users.0",
    "users.0.name",
    "users.0.tags",
    "users.1.name",
    "users.9.name",
    "n.x",
    "pack.0.name",
    "a.0",
    "a.b",
    ".",
    "0",
];

/// The two error enums as one comparable spelling.
fn mem_err(e: wolf_mem::json::JsonErr) -> &'static str {
    match e {
        wolf_mem::json::JsonErr::Parse => "parse",
        wolf_mem::json::JsonErr::Missing => "missing",
        wolf_mem::json::JsonErr::Kind => "kind",
    }
}

fn rt_err(e: wolf_rt::json::JsonErr) -> &'static str {
    match e {
        wolf_rt::json::JsonErr::Parse => "parse",
        wolf_rt::json::JsonErr::Missing => "missing",
        wolf_rt::json::JsonErr::Kind => "kind",
    }
}

#[test]
fn reference_and_mirror_agree_on_every_vector() {
    for &s in TEXTS {
        assert_eq!(
            wolf_mem::json::valid(s),
            wolf_rt::json::valid(s),
            "valid diverged on {s:?}"
        );
        for &p in PATHS {
            assert_eq!(
                wolf_mem::json::get(s, p).map_err(mem_err),
                wolf_rt::json::get(s, p).map_err(rt_err),
                "get diverged on {s:?} / {p:?}"
            );
            assert_eq!(
                wolf_mem::json::type_of(s, p).map_err(mem_err),
                wolf_rt::json::type_of(s, p).map_err(rt_err),
                "type diverged on {s:?} / {p:?}"
            );
            assert_eq!(
                wolf_mem::json::len_of(s, p).map_err(mem_err),
                wolf_rt::json::len_of(s, p).map_err(rt_err),
                "len diverged on {s:?} / {p:?}"
            );
        }
    }
}

/// The depth limit is part of the pinned semantics (RFC 8259 §9 makes
/// it implementation-defined, which is exactly why the two copies
/// must agree on the NUMBER, not just the idea).
#[test]
fn depth_limit_agrees() {
    assert_eq!(wolf_mem::json::MAX_DEPTH, wolf_rt::json::MAX_DEPTH);
    let deep =
        "[".repeat(wolf_mem::json::MAX_DEPTH + 2) + &"]".repeat(wolf_mem::json::MAX_DEPTH + 2);
    assert!(!wolf_mem::json::valid(&deep));
    assert!(!wolf_rt::json::valid(&deep));
    let ok = "[".repeat(64) + &"]".repeat(64);
    assert!(wolf_mem::json::valid(&ok));
    assert!(wolf_rt::json::valid(&ok));
}
