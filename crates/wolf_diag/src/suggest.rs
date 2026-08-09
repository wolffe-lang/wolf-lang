//! The shared "did you mean" machinery (s10 target 4).
//!
//! One edit-distance suggester for every phase: keyword typos now
//! (`fnn` → `fn`), identifiers/fields/methods when sema arrives (s13).
//! Damerau-Levenshtein (transpositions count as one edit — `fnn`/`fn`,
//! `lte`/`let`), with a length-scaled acceptance threshold so short
//! words don't match everything and long words tolerate more noise:
//!
//! | input length | max edits |
//! |--------------|-----------|
//! | ≤ 4          | 1         |
//! | 5–8          | 2         |
//! | > 8          | 3         |
//!
//! Candidates are also tried case-insensitively (`Fn` → `fn` is one
//! *case* slip, not two edits): a case-insensitive match at a strictly
//! smaller distance wins over the case-sensitive one.

/// Damerau-Levenshtein distance (optimal string alignment variant):
/// insertions, deletions, substitutions, and adjacent transpositions
/// each cost 1. Operates on chars.
pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    // Three rolling rows: two back (transpositions), one back, current.
    let mut prev2: Vec<usize> = vec![0; m + 1];
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(prev2[j - 2] + 1);
            }
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// The length-scaled acceptance threshold (see module docs).
pub fn max_edits(input_len: usize) -> usize {
    match input_len {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    }
}

/// The best candidate within threshold, or `None`. Exact matches return
/// `None` too — there is nothing to suggest. Ties break toward the
/// earlier candidate (candidate lists are in canonical order).
pub fn best_match<'c>(input: &str, candidates: &[&'c str]) -> Option<&'c str> {
    let limit = max_edits(input.chars().count());
    let input_lower = input.to_lowercase();
    let mut best: Option<(usize, &'c str)> = None;
    for &cand in candidates {
        if cand == input {
            return None;
        }
        let d = damerau_levenshtein(input, cand);
        // The case-insensitive pass: a candidate differing mostly by
        // case ranks by its case-folded distance.
        let d = d.min(damerau_levenshtein(&input_lower, &cand.to_lowercase()));
        if d <= limit && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, cand));
        }
    }
    best.map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_basics() {
        assert_eq!(damerau_levenshtein("", ""), 0);
        assert_eq!(damerau_levenshtein("fn", "fn"), 0);
        assert_eq!(damerau_levenshtein("fnn", "fn"), 1); // deletion
        assert_eq!(damerau_levenshtein("fn", "in"), 1); // substitution
        assert_eq!(damerau_levenshtein("lte", "let"), 1); // transposition
        assert_eq!(damerau_levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn thresholds_scale_with_length() {
        assert_eq!(max_edits(2), 1);
        assert_eq!(max_edits(4), 1);
        assert_eq!(max_edits(5), 2);
        assert_eq!(max_edits(8), 2);
        assert_eq!(max_edits(9), 3);
    }

    #[test]
    fn keyword_typos_match() {
        let kws = ["fn", "let", "for", "struct", "return"];
        assert_eq!(best_match("fnn", &kws), Some("fn"));
        assert_eq!(best_match("lte", &kws), Some("let"));
        assert_eq!(best_match("strcut", &kws), Some("struct"));
        assert_eq!(best_match("retrun", &kws), Some("return"));
        // Too far for a short word: `xy` is 2 edits from `fn`.
        assert_eq!(best_match("xy", &kws), None);
        // Exact match: nothing to suggest.
        assert_eq!(best_match("fn", &kws), None);
    }

    #[test]
    fn case_insensitive_pass() {
        let kws = ["fn", "match"];
        assert_eq!(best_match("Fn", &kws), Some("fn"));
        // `Mtach` = one transposition after case folding (5 chars → 2 allowed).
        assert_eq!(best_match("Mtach", &kws), Some("match"));
    }
}
