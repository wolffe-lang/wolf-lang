//! The keyword run is closed by NAME, not only by count (s175, the
//! ruling on wolf-lang#370's residual).
//!
//! `SyntaxKind::is_keyword` is a half-open discriminant range
//! `AsKw..LParen`, and `kind.rs` pins both ends at build time. What
//! that cannot see is a keyword kind declared OUTSIDE the run — the
//! addition mode that actually bit at #356 — and #370 left two ways to
//! close it: an exhaustive ~200-arm `match` kept in step by hand, or
//! nothing. This is the third way, and the ruling: every variant whose
//! name ends in `Kw` must sit inside the run, and every variant inside
//! the run must end in `Kw`, read from the enum's own source text. It
//! costs one test, no arms, and no edit when a keyword is added in the
//! right place. It is a test and not a `const` because a name is not a
//! discriminant; the `const` block keeps the ends, this keeps the
//! membership.
//!
//! Seen red before it was trusted (s175's report, kasumi): a planted
//! `ZzzKw` after `LParen` fails here with `outside the run`. A planted
//! `Zzz` between `AsKw` and `LParen` never reaches this test — kind.rs's
//! own width assertion refuses the build first — so the intruder arm
//! below is the belt for a swap that keeps the count, and has not been
//! seen red on its own.

const KIND_RS: &str = include_str!("../src/kind.rs");

/// The variant names of `pub enum SyntaxKind`, in declaration order.
fn variants() -> Vec<String> {
    let start = KIND_RS
        .find("pub enum SyntaxKind {")
        .expect("kind.rs declares `pub enum SyntaxKind {`");
    let body = &KIND_RS[start + "pub enum SyntaxKind {".len()..];
    let end = body.find("\n}\n").expect("the enum closes at column 0");
    let body = &body[..end];
    let mut out = Vec::new();
    for line in body.lines() {
        // Comments — doc or plain — carry no variants.
        let code = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        for piece in code.split(',') {
            let name = piece.trim();
            // `#[…]` attributes and `Name = N` discriminants: the name
            // is the leading identifier, if any.
            if name.is_empty() || name.starts_with('#') {
                continue;
            }
            let ident: String = name
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !ident.is_empty() && ident.chars().next().unwrap().is_ascii_uppercase() {
                out.push(ident);
            }
        }
    }
    out
}

#[test]
fn every_keyword_kind_is_inside_the_run_and_nothing_else_is() {
    let names = variants();
    let lo = names
        .iter()
        .position(|n| n == "AsKw")
        .expect("AsKw declared");
    let hi = names
        .iter()
        .position(|n| n == "LParen")
        .expect("LParen declared");
    assert!(lo < hi, "AsKw must precede LParen");
    let mut outside = Vec::new();
    let mut intruders = Vec::new();
    for (i, n) in names.iter().enumerate() {
        let in_run = i >= lo && i < hi;
        let is_kw = n.ends_with("Kw");
        if is_kw && !in_run {
            outside.push(n.clone());
        }
        if in_run && !is_kw {
            intruders.push(n.clone());
        }
    }
    assert!(
        outside.is_empty(),
        "keyword kind(s) declared outside the run `AsKw..LParen`, so `is_keyword` cannot \
         see them (#356's shape): {outside:?}"
    );
    assert!(
        intruders.is_empty(),
        "non-keyword kind(s) inside the run but not a keyword, so `is_keyword` claims them: \
         {intruders:?}"
    );
    // The run's width is what kind.rs's `const` block pins; agree with it.
    assert_eq!(
        hi - lo,
        53,
        "the run holds 50 reserved + 3 contextual keywords"
    );
}
