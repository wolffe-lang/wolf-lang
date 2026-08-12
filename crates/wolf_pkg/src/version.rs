//! Package versions (s51): dotted numerics, no ranges, ever (D33 —
//! MVS resolves over minimums, so a version is a point, not a set).

use std::fmt;

/// A package version: `major.minor.patch`. Ordering is the derived
/// lexicographic one — exactly what MVS's max-of-minimums needs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Default)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    /// Parse `"1"`, `"1.4"`, or `"1.4.0"`. Anything else (ranges,
    /// wildcards, pre-release tags) is refused — MVS has no use for
    /// them and D33 wants the manifest boring.
    pub fn parse(s: &str) -> Option<Version> {
        let mut parts = s.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = match parts.next() {
            Some(p) => p.parse().ok()?,
            None => 0,
        };
        let patch = match parts.next() {
            Some(p) => p.parse().ok()?,
            None => 0,
        };
        if parts.next().is_some() {
            return None;
        }
        Some(Version::new(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_forms() {
        assert_eq!(Version::parse("1.4.0"), Some(Version::new(1, 4, 0)));
        assert_eq!(Version::parse("1.4"), Some(Version::new(1, 4, 0)));
        assert_eq!(Version::parse("2"), Some(Version::new(2, 0, 0)));
        assert_eq!(Version::parse(""), None);
        assert_eq!(Version::parse("1.4.0.1"), None);
        assert_eq!(Version::parse("^1.4"), None);
        assert_eq!(Version::parse("1.x"), None);
    }

    #[test]
    fn ordering_is_lexicographic() {
        assert!(Version::parse("1.10.0") > Version::parse("1.9.9"));
        assert!(Version::parse("2.0.0") > Version::parse("1.99.99"));
    }
}
