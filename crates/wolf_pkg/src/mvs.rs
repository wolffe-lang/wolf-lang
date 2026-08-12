//! Minimal version selection, verbatim from vgo (s51 Target 2;
//! reports/07 §Done well #1).
//!
//! Deps state *minimums*; the build uses the max of the minimums
//! reachable in the module graph. No SAT, no ranges, a unique
//! solution, and a newly published version is never picked up until a
//! human asks (`wolf update` adds the new minimum; resolution itself
//! is a pure function of manifests). Two majors of one package are two
//! distinct modules — semantic import versioning without `/v2` path
//! ugliness — so the selection key is `(name, major)`.

use std::collections::{BTreeMap, BTreeSet};

use crate::version::Version;

/// One requirement edge: "I need `name` at major `major`, version
/// `min` or newer".
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Req {
    pub name: String,
    pub major: u32,
    pub min: Version,
}

impl Req {
    pub fn new(name: &str, major: u32, min: Version) -> Req {
        Req {
            name: name.to_string(),
            major,
            min,
        }
    }
}

/// The universe of manifests: given a module at an exact version, its
/// requirement list. v0 backends: the in-memory test universe here,
/// and the disk resolver's fetched manifests ([`crate::project`]).
pub trait Universe {
    fn requirements(
        &mut self,
        name: &str,
        major: u32,
        version: Version,
    ) -> Result<Vec<Req>, String>;
}

/// A resolved module: the MVS answer for one `(name, major)`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Selected {
    pub name: String,
    pub major: u32,
    pub version: Version,
}

/// MVS proper: walk the module graph from the root requirements,
/// reading each *required minimum version's own manifest* (this is
/// vgo's lazy graph — versions nobody requires are never visited),
/// and select, per `(name, major)`, the maximum minimum reached.
/// Deterministic by construction; cycles are fine (visited-set).
pub fn resolve(root: &[Req], universe: &mut dyn Universe) -> Result<Vec<Selected>, String> {
    let mut selected: BTreeMap<(String, u32), Version> = BTreeMap::new();
    let mut visited: BTreeSet<(String, u32, Version)> = BTreeSet::new();
    let mut work: Vec<Req> = root.to_vec();
    while let Some(req) = work.pop() {
        let key = (req.name.clone(), req.major);
        let cur = selected.entry(key).or_insert(req.min);
        if req.min > *cur {
            *cur = req.min;
        }
        if !visited.insert((req.name.clone(), req.major, req.min)) {
            continue;
        }
        let deps = universe.requirements(&req.name, req.major, req.min)?;
        work.extend(deps);
    }
    Ok(selected
        .into_iter()
        .map(|((name, major), version)| Selected {
            name,
            major,
            version,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory universe: `(name, major, version) -> reqs`. Asking
    /// for an unlisted manifest is an error — the lazy-graph tests
    /// assert un-required versions are never fetched.
    #[derive(Default)]
    struct Mem {
        manifests: BTreeMap<(String, u32, Version), Vec<Req>>,
        fetched: Vec<(String, Version)>,
    }

    impl Mem {
        fn add(&mut self, name: &str, major: u32, v: &str, reqs: &[(&str, u32, &str)]) {
            self.manifests.insert(
                (name.to_string(), major, Version::parse(v).unwrap()),
                reqs.iter()
                    .map(|(n, m, v)| Req::new(n, *m, Version::parse(v).unwrap()))
                    .collect(),
            );
        }
    }

    impl Universe for Mem {
        fn requirements(
            &mut self,
            name: &str,
            major: u32,
            version: Version,
        ) -> Result<Vec<Req>, String> {
            self.fetched.push((name.to_string(), version));
            self.manifests
                .get(&(name.to_string(), major, version))
                .cloned()
                .ok_or_else(|| format!("no manifest for {name}@{version}"))
        }
    }

    fn req(name: &str, v: &str) -> Req {
        Req::new(name, 1, Version::parse(v).unwrap())
    }

    fn assert_build_list(got: &[Selected], want: &[(&str, &str)]) {
        let got: Vec<(String, String)> = got
            .iter()
            .map(|s| (s.name.clone(), s.version.to_string()))
            .collect();
        let want: Vec<(String, String)> = want
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect();
        assert_eq!(got, want);
    }

    /// The vgo paper's headline example: A needs B1.2 and C1.2;
    /// B1.2 needs D1.3; C1.2 needs D1.4; D needs E1.2.
    /// Build list: max-of-minimums ⇒ D1.4, E1.2.
    #[test]
    fn vgo_worked_example() {
        let mut u = Mem::default();
        u.add("B", 1, "1.2.0", &[("D", 1, "1.3.0")]);
        u.add("C", 1, "1.2.0", &[("D", 1, "1.4.0")]);
        u.add("D", 1, "1.3.0", &[("E", 1, "1.2.0")]);
        u.add("D", 1, "1.4.0", &[("E", 1, "1.2.0")]);
        u.add("E", 1, "1.2.0", &[]);
        let got = resolve(&[req("B", "1.2.0"), req("C", "1.2.0")], &mut u).unwrap();
        assert_build_list(
            &got,
            &[
                ("B", "1.2.0"),
                ("C", "1.2.0"),
                ("D", "1.4.0"),
                ("E", "1.2.0"),
            ],
        );
    }

    /// A newer D1.5 exists in the universe but nobody requires it:
    /// MVS never picks it up (and, lazily, never even reads it) until
    /// a human raises a minimum (`wolf update`'s act).
    #[test]
    fn new_versions_are_never_picked_up_silently() {
        let mut u = Mem::default();
        u.add("B", 1, "1.2.0", &[("D", 1, "1.3.0")]);
        u.add("D", 1, "1.3.0", &[]);
        u.add("D", 1, "1.5.0", &[]); // published, unrequired
        let got = resolve(&[req("B", "1.2.0")], &mut u).unwrap();
        assert_build_list(&got, &[("B", "1.2.0"), ("D", "1.3.0")]);
        assert!(
            !u.fetched
                .contains(&("D".to_string(), Version::parse("1.5.0").unwrap())),
            "lazy graph: the unrequired version's manifest was never read"
        );
    }

    /// Upgrade = raising a root minimum: re-resolving with the new
    /// requirement pulls the deeper graph the new version demands.
    #[test]
    fn upgrade_raises_minimums() {
        let mut u = Mem::default();
        u.add("B", 1, "1.2.0", &[("D", 1, "1.3.0")]);
        u.add("B", 1, "1.3.0", &[("D", 1, "1.6.0")]);
        u.add("D", 1, "1.3.0", &[]);
        u.add("D", 1, "1.6.0", &[]);
        let before = resolve(&[req("B", "1.2.0")], &mut u).unwrap();
        assert_build_list(&before, &[("B", "1.2.0"), ("D", "1.3.0")]);
        let after = resolve(&[req("B", "1.3.0")], &mut u).unwrap();
        assert_build_list(&after, &[("B", "1.3.0"), ("D", "1.6.0")]);
    }

    /// Downgrade case: dropping the root minimum back down releases
    /// the raised transitive minimum — resolution is a pure function
    /// of the manifests in force, with no memory of past builds.
    #[test]
    fn downgrade_is_pure_recomputation() {
        let mut u = Mem::default();
        u.add("B", 1, "1.3.0", &[("D", 1, "1.6.0")]);
        u.add("B", 1, "1.2.0", &[("D", 1, "1.3.0")]);
        u.add("D", 1, "1.6.0", &[]);
        u.add("D", 1, "1.3.0", &[]);
        let high = resolve(&[req("B", "1.3.0")], &mut u).unwrap();
        assert_build_list(&high, &[("B", "1.3.0"), ("D", "1.6.0")]);
        let low = resolve(&[req("B", "1.2.0")], &mut u).unwrap();
        assert_build_list(&low, &[("B", "1.2.0"), ("D", "1.3.0")]);
    }

    /// Two majors of one package coexist as two modules (semantic
    /// import versioning): the selection key is `(name, major)`.
    #[test]
    fn two_majors_coexist() {
        let mut u = Mem::default();
        u.add("B", 1, "1.2.0", &[("D", 1, "1.1.0")]);
        u.add("C", 1, "1.0.0", &[("D", 2, "2.3.0")]);
        u.add("D", 1, "1.1.0", &[]);
        u.add("D", 2, "2.3.0", &[]);
        let got = resolve(&[req("B", "1.2.0"), req("C", "1.0.0")], &mut u).unwrap();
        let d_versions: Vec<(u32, String)> = got
            .iter()
            .filter(|s| s.name == "D")
            .map(|s| (s.major, s.version.to_string()))
            .collect();
        assert_eq!(
            d_versions,
            [(1, "1.1.0".to_string()), (2, "2.3.0".to_string())]
        );
    }

    /// Import cycles between packages resolve fine: the visited set
    /// terminates the walk; both minimums are honored.
    #[test]
    fn cycles_terminate() {
        let mut u = Mem::default();
        u.add("B", 1, "1.1.0", &[("C", 1, "1.2.0")]);
        u.add("C", 1, "1.2.0", &[("B", 1, "1.1.0")]);
        let got = resolve(&[req("B", "1.1.0")], &mut u).unwrap();
        assert_build_list(&got, &[("B", "1.1.0"), ("C", "1.2.0")]);
    }

    /// Diamond with a deep raise: the max-of-minimums is taken over
    /// the whole reachable graph, not just direct deps.
    #[test]
    fn deep_diamond_raises() {
        let mut u = Mem::default();
        u.add("B", 1, "1.0.0", &[("D", 1, "1.0.0")]);
        u.add("C", 1, "1.0.0", &[("E", 1, "1.0.0")]);
        u.add("E", 1, "1.0.0", &[("D", 1, "1.9.0")]);
        u.add("D", 1, "1.0.0", &[]);
        u.add("D", 1, "1.9.0", &[]);
        let got = resolve(&[req("B", "1.0.0"), req("C", "1.0.0")], &mut u).unwrap();
        assert_build_list(
            &got,
            &[
                ("B", "1.0.0"),
                ("C", "1.0.0"),
                ("D", "1.9.0"),
                ("E", "1.0.0"),
            ],
        );
    }
}
