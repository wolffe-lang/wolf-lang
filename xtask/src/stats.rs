//! Variance-aware statistics for `bench diff` (s01, rustc-perf lineage):
//! medians for robustness, MAD for the noise floor.

/// Median of a sample. Returns `None` on an empty slice.
pub fn median(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).expect("NaN in sample"));
    let mid = v.len() / 2;
    Some(if v.len() % 2 == 0 {
        (v[mid - 1] + v[mid]) / 2.0
    } else {
        v[mid]
    })
}

/// Median absolute deviation — the robust spread estimate the noise gate
/// uses. Returns `None` on an empty slice.
pub fn mad(xs: &[f64]) -> Option<f64> {
    let m = median(xs)?;
    let devs: Vec<f64> = xs.iter().map(|x| (x - m).abs()).collect();
    median(&devs)
}

/// Verdict for one metric across baseline and candidate samples.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// Within the noise floor.
    Unchanged,
    /// Significant move; positive delta = candidate is larger.
    Significant { delta_pct: f64 },
}

/// Significance gate: a delta is real only if it clears BOTH the pooled
/// MAD noise floor (3×) and a 2% practical floor. Below either → noise.
pub fn compare(baseline: &[f64], candidate: &[f64]) -> Option<Verdict> {
    let mb = median(baseline)?;
    let mc = median(candidate)?;
    let noise = 3.0 * mad(baseline)?.max(mad(candidate)?);
    let delta = mc - mb;
    let practical = 0.02 * mb.abs();
    if delta.abs() > noise.max(practical) {
        Some(Verdict::Significant {
            delta_pct: 100.0 * delta / mb,
        })
    } else {
        Some(Verdict::Unchanged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_and_mad() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
        assert_eq!(median(&[]), None);
        assert_eq!(mad(&[1.0, 1.0, 5.0]), Some(0.0));
    }

    #[test]
    fn same_distribution_is_unchanged() {
        let a = [100.0, 101.0, 99.0, 100.5, 100.2, 99.8, 100.1, 99.9, 100.3, 100.0];
        assert_eq!(compare(&a, &a), Some(Verdict::Unchanged));
    }

    #[test]
    fn big_move_is_significant() {
        let a = [100.0, 101.0, 99.0, 100.0, 100.0, 100.0, 99.5, 100.5, 100.0, 100.0];
        let b: Vec<f64> = a.iter().map(|x| x * 1.5).collect();
        match compare(&a, &b) {
            Some(Verdict::Significant { delta_pct }) => {
                assert!((delta_pct - 50.0).abs() < 1.0, "got {delta_pct}");
            }
            other => panic!("expected significant, got {other:?}"),
        }
    }

    #[test]
    fn small_move_inside_noise_is_unchanged() {
        let a = [100.0, 110.0, 90.0, 105.0, 95.0, 102.0, 98.0, 108.0, 92.0, 100.0];
        let b: Vec<f64> = a.iter().map(|x| x + 1.0).collect();
        assert_eq!(compare(&a, &b), Some(Verdict::Unchanged));
    }
}
