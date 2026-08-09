//! X4 canonicalization fixtures (s11 Target 4).
//!
//! The sugar-preference rewrite fires exactly when it is provable from
//! syntax alone: a single `let r = region(…)` immediately followed by
//! exactly one `in r { … }` with no other mention of `r` in the
//! enclosing block. Sent, frozen, returned, re-entered, or
//! re-mentioned regions are untouched — the snapshot pairs below prove
//! it.

fn fmt(src: &str) -> String {
    let out = wolf_fmt::format_text(src.as_bytes());
    assert!(!out.fell_back, "self-check fell back for {src:?}");
    assert!(!out.partial, "unexpected syntax errors in {src:?}");
    String::from_utf8(out.text).unwrap()
}

#[track_caller]
fn check(src: &str, want: &str) {
    let got = fmt(src);
    assert_eq!(got, want, "\n== got ==\n{got}\n== want ==\n{want}");
    assert_eq!(fmt(&got), got, "not idempotent");
}

// ------------------------------------------------------ rewrite fires ---

#[test]
fn plain_value_region_rewrites_to_block_sugar() {
    check(
        "fn main() {\n    let r = region()\n    let v = in r {\n        build()\n    }\n    v\n}\n",
        "fn main() {\n    let v = region r {\n        build()\n    }\n    v\n}\n",
    );
}

#[test]
fn rc_strategy_rides_into_the_sugar() {
    check(
        "fn main() {\n    let r = region(rc)\n    let v = in r {\n        build()\n    }\n    v\n}\n",
        "fn main() {\n    let v = region r: rc {\n        build()\n    }\n    v\n}\n",
    );
}

#[test]
fn pool_strategy_rides_into_the_sugar() {
    check(
        "fn main() {\n    let r = region(pool(Node))\n    let sum = in r {\n        work()\n    }\n    sum\n}\n",
        "fn main() {\n    let sum = region r: pool(Node) {\n        work()\n    }\n    sum\n}\n",
    );
}

#[test]
fn statement_position_in_block_rewrites_too() {
    check(
        "fn main() {\n    let r = region()\n    in r {\n        work()\n    }\n    0\n}\n",
        "fn main() {\n    region r {\n        work()\n    }\n    0\n}\n",
    );
}

// -------------------------------------------------- rewrite must NOT ---

#[test]
fn frozen_region_is_untouched() {
    // corpus/regions.lu shape: `freeze r` after the `in` block.
    check(
        "fn main() {\n    let r = region(rc)\n    let config = in r {\n        build_config()\n    }\n    let frozen = freeze r\n    0\n}\n",
        "fn main() {\n    let r = region(rc)\n    let config = in r {\n        build_config()\n    }\n    let frozen = freeze r\n    0\n}\n",
    );
}

#[test]
fn sent_region_is_untouched() {
    check(
        "fn main() {\n    let r = region()\n    let data = in r {\n        build_batch()\n    }\n    results.send(move r)\n    0\n}\n",
        "fn main() {\n    let r = region()\n    let data = in r {\n        build_batch()\n    }\n    results.send(move r)\n    0\n}\n",
    );
}

#[test]
fn returned_region_is_untouched() {
    check(
        "fn make() -> region {\n    let r = region()\n    let v = in r {\n        seed()\n    }\n    r\n}\n",
        "fn make() -> region {\n    let r = region()\n    let v = in r {\n        seed()\n    }\n    r\n}\n",
    );
}

#[test]
fn re_entered_region_is_untouched() {
    check(
        "fn main() {\n    let r = region()\n    let a = in r {\n        one()\n    }\n    let b = in r {\n        two()\n    }\n    0\n}\n",
        "fn main() {\n    let r = region()\n    let a = in r {\n        one()\n    }\n    let b = in r {\n        two()\n    }\n    0\n}\n",
    );
}

#[test]
fn merely_mentioned_region_is_untouched() {
    check(
        "fn main() {\n    let r = region()\n    let v = in r {\n        build()\n    }\n    inspect(r)\n    0\n}\n",
        "fn main() {\n    let r = region()\n    let v = in r {\n        build()\n    }\n    inspect(r)\n    0\n}\n",
    );
}

#[test]
fn in_block_not_immediately_following_is_untouched() {
    check(
        "fn main() {\n    let r = region()\n    other()\n    let v = in r {\n        build()\n    }\n    0\n}\n",
        "fn main() {\n    let r = region()\n    other()\n    let v = in r {\n        build()\n    }\n    0\n}\n",
    );
}

#[test]
fn in_block_inside_a_closure_is_untouched() {
    // The closure could run later or many times; moving region
    // creation into it is not meaning-preserving.
    check(
        "fn main() {\n    let r = region()\n    let f = fn() in r {\n        build()\n    }\n    0\n}\n",
        "fn main() {\n    let r = region()\n    let f = fn() in r {\n        build()\n    }\n    0\n}\n",
    );
}

#[test]
fn annotated_or_commented_lets_are_untouched() {
    check(
        "fn main() {\n    let r: region = region()\n    let v = in r {\n        build()\n    }\n    0\n}\n",
        "fn main() {\n    let r: region = region()\n    let v = in r {\n        build()\n    }\n    0\n}\n",
    );
    check(
        "fn main() {\n    let r = region() // the arena\n    let v = in r {\n        build()\n    }\n    0\n}\n",
        "fn main() {\n    let r = region() // the arena\n    let v = in r {\n        build()\n    }\n    0\n}\n",
    );
}

// ------------------------------------------- desugar equivalence prop ---

/// The rewritten output reparses to a tree whose X4 desugaring is
/// identical: `normalize` maps `region r(…) { … }` sugar and the
/// (let + in) value pair to the same shape, so equality here is
/// exactly desugar-equivalence.
#[test]
fn desugar_equivalence_over_rewrite_fixtures() {
    let eligible = [
        "fn main() {\n    let r = region()\n    let v = in r {\n        build()\n    }\n    v\n}\n",
        "fn main() {\n    let r = region(rc)\n    let v = in r {\n        build()\n    }\n    v\n}\n",
        "fn main() {\n    let r = region(pool(Node))\n    in r {\n        work()\n    }\n    0\n}\n",
    ];
    for src in eligible {
        let out = fmt(src);
        assert!(
            out.contains("region r"),
            "rewrite did not fire for {src:?}: {out}"
        );
        assert!(!out.contains("let r"), "let survived for {src:?}: {out}");
        let n_in = parse_norm(src);
        let n_out = parse_norm(&out);
        assert_eq!(n_in, n_out, "desugar equivalence broken for {src:?}");
    }
}

fn parse_norm(src: &str) -> wolf_fmt::NTree {
    let mut sm = wolf_span::SourceMap::new();
    let f = sm.intern(std::path::Path::new("x4.lu"));
    let parse = wolf_parse::parse_file(f, src.as_bytes());
    wolf_fmt::normalize(&parse.root, src.as_bytes())
}
