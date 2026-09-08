//! `README.md` is a SHIPPED file, not just a repo page (#252).
//!
//! `cargo xtask dist` stages it into the release archive, and the
//! Homebrew formula and the AUR PKGBUILD install it to
//! `share/doc/wolf/README.md` — where a repo-relative link resolves to
//! nothing. Nine of them did, so a packaged reader got a document whose
//! every onward pointer was broken. An absolute link renders
//! identically in the repo view, so the rule is simply: every link in
//! the shipped README is absolute.
//!
//! `<img src=…>` is checked the same way, for the same reason.

/// Every markdown link target and `<img src=…>` in `text` that is not
/// absolute (`https:`, `http:`, `mailto:`) and not a same-page anchor.
///
/// Deliberately dumb: it scans for `](…)` and `src="…"` rather than
/// parsing markdown, which is enough for a hand-written README and has
/// no false negatives on the shape that broke.
pub fn relative_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |target: &str| {
        let t = target.trim();
        let absolute = t.starts_with("https://")
            || t.starts_with("http://")
            || t.starts_with("mailto:")
            || t.starts_with('#');
        if !absolute && !t.is_empty() {
            out.push(t.to_string());
        }
    };
    // Markdown inline links: `](target)`. A target may carry a title
    // (`](url "t")`); the URL is the first whitespace-free run.
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        rest = &rest[i + 2..];
        if let Some(end) = rest.find(')') {
            let target = &rest[..end];
            push(target.split_whitespace().next().unwrap_or(""));
        }
    }
    // Raw HTML images (the logo).
    let mut rest = text;
    while let Some(i) = rest.find("src=\"") {
        rest = &rest[i + 5..];
        if let Some(end) = rest.find('"') {
            push(&rest[..end]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_readme() -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask sits in the repo root")
            .join("README.md");
        std::fs::read_to_string(&p).expect("README.md is readable")
    }

    /// The regression (#252): the file `dist` ships and the packages
    /// install may not carry a link that only works in a checkout.
    #[test]
    fn shipped_readme_has_no_relative_links() {
        let bad = relative_links(&shipped_readme());
        assert!(
            bad.is_empty(),
            "README.md is installed to share/doc, where these resolve to nothing — \
             use https://github.com/wolffe-lang/wolf-lang/blob/trunk/…: {bad:?}"
        );
    }

    /// The README still points somewhere: an empty scan would pass the
    /// test above for the wrong reason.
    #[test]
    fn shipped_readme_still_links() {
        let text = shipped_readme();
        let links = text.matches("](").count();
        assert!(links >= 9, "README.md carries its pointers ({links} found)");
    }

    #[test]
    fn scanner_separates_absolute_from_relative() {
        let bad = relative_links(
            "[a](docs/x.md) [b](https://e.com/y) [c](#anchor) <img src=\"assets/l.svg\"/>",
        );
        assert_eq!(bad, vec!["docs/x.md", "assets/l.svg"]);
    }

    /// `WOLF_STD` is the one mechanism a packaged reader cannot guess
    /// (#251): the diagnostic names it, and so must the shipped doc.
    #[test]
    fn shipped_readme_names_the_std_mechanism() {
        let text = shipped_readme();
        assert!(text.contains("WOLF_STD"), "README.md names WOLF_STD");
        assert!(
            text.contains("wolf-std"),
            "README.md names the wolf-std repo"
        );
    }
}
