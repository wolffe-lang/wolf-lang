//! `.wprof` — the wolf-owned profile container (s45 target 2).
//!
//! A profile is **plain data**: a versioned, line-oriented text file
//! that records how often each block of each function executed. It is a
//! build INPUT like any other (D4 reproducibility: same source + same
//! `.wprof` → byte-identical binary), it is never executable, and it
//! carries no manifest or registry implications (D33/s51).
//!
//! # Keying: the content hash, and nothing else
//!
//! Every record is keyed by the **s43 content hash** of the NORMALIZED
//! function body ([`crate::midend::dedup::body_hash`]) — never by
//! symbol name, never by a DefPath, never by a source position. That is
//! the whole design, and it is confirmed by evidence rather than taste
//! (`refs/papers/stale-profile-matching.txt`):
//!
//! - a profile taken on commit A applies to commit B wherever bodies
//!   are hash-identical, so recompiling, renaming a local, or editing a
//!   NEIGHBOURING function leaves a record perfectly applicable;
//! - editing a body changes its hash, so exactly that record goes
//!   stale — invalidation is precise instead of program-wide;
//! - a stale record is **ignorable, not poisonous**: it names a body
//!   that does not exist in this build, so it is dropped. It cannot be
//!   misapplied to a body whose block structure it does not describe,
//!   which is the failure mode name-keyed profiles have.
//!
//! The counts are **positional** over [`crate::print::block_order`] —
//! the same canonical reachable-RPO order the printer uses, hence the
//! same order the hash is taken over. A record and a body agree on
//! block identity precisely when they agree on the hash, so "valid
//! against the hashed body" is structural rather than hoped-for. A
//! record whose length disagrees with the body's block count is
//! therefore a corrupt file, not a stale one, and is refused.
//!
//! # Never required (D4)
//!
//! Nothing here is on the default path. A build with no profile is a
//! normal build: no warning, no degradation, no "consider PGO" nag. A
//! build with a FULLY stale profile is byte-identical to that build,
//! plus one summary line saying the profile did not apply.
//!
//! # The v1 schema
//!
//! ```text
//! wprof 1
//! producer instr
//! runs <u32>
//! samples <u64>
//! fn <hash> <n> <c0> <c1> … <c[n-1]>
//! ```
//!
//! - `wprof <version>` — MUST be the first line. A reader that does not
//!   know the version REFUSES; it never guesses. See the version
//!   discipline below.
//! - `producer` — how the counts were obtained. `instr` is the only
//!   value v1 produces or accepts. **This is the reserved
//!   sample-producer headroom** the sprint's non-targets leave open
//!   (`sample` for a future perf/LBR ingest): the slot exists at v1 so
//!   a later producer needs no container change, and a v1 reader
//!   handed a `sample` profile refuses LOUDLY rather than reading
//!   sample counts as execution counts.
//! - `runs` — how many program runs were merged into this file.
//! - `samples` — total counter increments across all records. The
//!   staleness denominator: "this build matched 3% of the profile's
//!   samples" is the number `wolf profile show` reports.
//! - `fn` — one record per function body. `n` is the block count and
//!   must equal the number of counts that follow. `c0` is the entry
//!   block's count, i.e. the function's entry count.
//!
//! Records are emitted in ascending hash order, so a `.wprof` is a pure
//! function of the run's counters and diffs cleanly.
//!
//! # Version discipline (the format doc's rule, enforced here)
//!
//! A reader of an old format must **either work or refuse loudly, never
//! misread**. Concretely:
//!
//! - the version line is mandatory and is checked before anything else;
//! - a version this reader does not implement is [`ProfileError`], with
//!   the file's version and this compiler's in the message;
//! - an unknown DIRECTIVE (a line whose first token is not one of the
//!   known keywords) is an error, not a skipped line — silently
//!   ignoring a directive is exactly how a reader misreads a newer
//!   file;
//! - a `fn` record with a count-length mismatch is an error;
//! - **the bump policy**: additive per-record fields APPENDED to a `fn`
//!   line after the counts would require a bump, because v1 defines the
//!   line as exactly `n` counts. New DIRECTIVES require a bump for the
//!   same reason. The one thing that does not is a new `producer`
//!   value, which is why that slot is spelled as data rather than as a
//!   version.

use std::collections::BTreeMap;
use std::fmt;

/// The `.wprof` container version this compiler reads and writes.
/// Bump ONLY with a schema change; a bump makes every older profile
/// refuse loudly (never misread), and rides the build cache key so a
/// bump cannot silently reuse an object built under the old reader.
pub const WPROF_VERSION: u32 = 1;

/// The v1 producer: counters inserted at the WIR level and dumped by
/// the runtime. The container reserves the slot for a sample-derived
/// producer (the sprint's stated non-target), which is why this is a
/// string in the file and not an implicit property of the version.
pub const PRODUCER_INSTR: &str = "instr";

/// What went wrong reading a `.wprof`. Every variant is LOUD: this type
/// exists so that no unreadable profile is ever quietly treated as an
/// empty one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.msg)
        } else {
            write!(f, "line {}: {}", self.line, self.msg)
        }
    }
}

impl std::error::Error for ProfileError {}

fn err(line: usize, msg: impl Into<String>) -> ProfileError {
    ProfileError {
        line,
        msg: msg.into(),
    }
}

/// One function body's counts, keyed by its content hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Block execution counts, positional over
    /// [`crate::print::block_order`]. `blocks[0]` is the entry block.
    pub blocks: Vec<u64>,
}

impl Record {
    /// The function's entry count — how many times it was called.
    pub fn entry(&self) -> u64 {
        self.blocks.first().copied().unwrap_or(0)
    }

    /// The hottest block's count: the denominator for branch weights
    /// and the honest answer to "how hot is this body really", since a
    /// function called once around a million-iteration loop is hot.
    pub fn peak(&self) -> u64 {
        self.blocks.iter().copied().max().unwrap_or(0)
    }

    /// Total counter increments in this record.
    pub fn total(&self) -> u64 {
        self.blocks.iter().copied().fold(0u64, u64::saturating_add)
    }
}

/// A parsed `.wprof`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// How many program runs were merged in.
    pub runs: u32,
    /// Records by content hash, ascending (the canonical order).
    pub funcs: BTreeMap<String, Record>,
}

impl Profile {
    /// Total counter increments across every record — the staleness
    /// denominator.
    pub fn samples(&self) -> u64 {
        self.funcs
            .values()
            .map(Record::total)
            .fold(0u64, u64::saturating_add)
    }

    /// The record for a body hash, if this profile has one.
    pub fn get(&self, body_hash: &str) -> Option<&Record> {
        self.funcs.get(body_hash)
    }

    /// The canonical serialization. Deterministic: hash order, no
    /// floats, no wall clock, no host facts.
    pub fn render(&self) -> String {
        let mut out = format!("wprof {WPROF_VERSION}\n");
        out.push_str(&format!("producer {PRODUCER_INSTR}\n"));
        out.push_str(&format!("runs {}\n", self.runs));
        out.push_str(&format!("samples {}\n", self.samples()));
        for (hash, r) in &self.funcs {
            out.push_str("fn ");
            out.push_str(hash);
            out.push(' ');
            out.push_str(&r.blocks.len().to_string());
            for c in &r.blocks {
                out.push(' ');
                out.push_str(&c.to_string());
            }
            out.push('\n');
        }
        out
    }

    /// Parse a `.wprof`. Refuses loudly on anything it does not fully
    /// understand — see the module's version discipline.
    pub fn parse(text: &str) -> Result<Profile, ProfileError> {
        let mut lines = text.lines().enumerate();
        // ---- the version line is mandatory and comes first ----------
        let (vno, vline) = loop {
            let Some((i, l)) = lines.next() else {
                return Err(err(
                    0,
                    "empty profile: a `.wprof` must open with a `wprof <version>` line",
                ));
            };
            let t = l.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            break (i + 1, t.to_string());
        };
        let Some(vtext) = vline.strip_prefix("wprof ") else {
            return Err(err(
                vno,
                format!(
                    "not a wolf profile: expected a `wprof <version>` header, found `{}`",
                    truncate(&vline)
                ),
            ));
        };
        let version: u32 = vtext.trim().parse().map_err(|_| {
            err(
                vno,
                format!("`wprof {}` is not a version number", truncate(vtext.trim())),
            )
        })?;
        if version != WPROF_VERSION {
            return Err(err(
                vno,
                format!(
                    "profile format v{version}, but this compiler reads v{WPROF_VERSION} — \
                     refusing rather than guessing at a format it does not know (re-record the \
                     profile with `--profile-gen`)"
                ),
            ));
        }
        let mut producer: Option<String> = None;
        let mut runs: Option<u32> = None;
        let mut declared_samples: Option<u64> = None;
        let mut funcs: BTreeMap<String, Record> = BTreeMap::new();
        for (i, l) in lines {
            let no = i + 1;
            let t = l.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let mut it = t.split_whitespace();
            let kw = it.next().unwrap_or("");
            match kw {
                "producer" => {
                    let p = it.next().unwrap_or("");
                    if p != PRODUCER_INSTR {
                        return Err(err(
                            no,
                            format!(
                                "producer `{}`: this compiler only consumes `{PRODUCER_INSTR}` \
                                 profiles (the container reserves other producers; a reader that \
                                 does not know one refuses rather than reading its numbers as \
                                 execution counts)",
                                truncate(p)
                            ),
                        ));
                    }
                    producer = Some(p.to_string());
                }
                "runs" => {
                    runs = Some(parse_u32(it.next(), no, "runs")?);
                }
                "samples" => {
                    declared_samples = Some(parse_u64(it.next(), no, "samples")?);
                }
                "fn" => {
                    let hash = it
                        .next()
                        .ok_or_else(|| err(no, "`fn` record without a body hash"))?;
                    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(err(
                            no,
                            format!(
                                "`{}` is not a content hash (64 lowercase hex digits)",
                                truncate(hash)
                            ),
                        ));
                    }
                    let n = parse_u32(it.next(), no, "block count")? as usize;
                    let mut blocks = Vec::with_capacity(n);
                    for _ in 0..n {
                        blocks.push(parse_u64(it.next(), no, "block count")?);
                    }
                    if it.next().is_some() {
                        return Err(err(
                            no,
                            format!(
                                "`fn` record declares {n} block(s) but carries more counts than \
                                 that — v{WPROF_VERSION} defines the line as exactly `n` counts, \
                                 so extra fields mean a format this reader does not know"
                            ),
                        ));
                    }
                    if funcs.insert(hash.to_string(), Record { blocks }).is_some() {
                        return Err(err(
                            no,
                            format!("duplicate record for body {}", &hash[..16]),
                        ));
                    }
                }
                other => {
                    return Err(err(
                        no,
                        format!(
                            "unknown directive `{}`: a v{WPROF_VERSION} reader refuses a line it \
                             does not understand instead of skipping it (skipping is how a reader \
                             misreads a newer file)",
                            truncate(other)
                        ),
                    ));
                }
            }
        }
        if producer.is_none() {
            return Err(err(0, "profile has no `producer` line"));
        }
        let p = Profile {
            runs: runs.unwrap_or(1),
            funcs,
        };
        if let Some(d) = declared_samples
            && d != p.samples()
        {
            return Err(err(
                0,
                format!(
                    "`samples {d}` disagrees with the {} counted in the records — the file is \
                     corrupt or truncated",
                    p.samples()
                ),
            ));
        }
        Ok(p)
    }

    /// Merge `other` into `self`: compatible records (same hash, same
    /// block count) sum; records only one side has are taken as they
    /// are. Multi-run and multi-shard workloads are exactly this.
    ///
    /// Two records with the same hash and DIFFERENT block counts cannot
    /// both be right — the hash determines the block structure — so
    /// that is a refusal, not a silent pick.
    pub fn merge(&mut self, other: &Profile) -> Result<(), ProfileError> {
        for (hash, r) in &other.funcs {
            match self.funcs.get_mut(hash) {
                Some(mine) => {
                    if mine.blocks.len() != r.blocks.len() {
                        return Err(err(
                            0,
                            format!(
                                "body {} appears with {} block(s) in one profile and {} in the \
                                 other; one of the files is corrupt (the content hash fixes the \
                                 block structure)",
                                &hash[..16],
                                mine.blocks.len(),
                                r.blocks.len()
                            ),
                        ));
                    }
                    for (a, b) in mine.blocks.iter_mut().zip(&r.blocks) {
                        *a = a.saturating_add(*b);
                    }
                }
                None => {
                    self.funcs.insert(hash.clone(), r.clone());
                }
            }
        }
        self.runs = self.runs.saturating_add(other.runs);
        Ok(())
    }
}

/// How much of a profile applied to a build — the number that answers
/// "PGO did nothing, why?".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Records in the profile.
    pub records: usize,
    /// Records whose hash names a body in this build.
    pub matched: usize,
    /// Samples carried by the matched records.
    pub matched_samples: u64,
    /// Samples in the profile overall.
    pub total_samples: u64,
    /// Bodies in this build with no record at all.
    pub unprofiled_bodies: usize,
}

impl Coverage {
    /// Combine two scorings of the SAME profile against two moments of
    /// one build (the whole-program phase matches twice; see
    /// `midend::wp`). A record that applied at either moment applied,
    /// so `matched` takes the larger count and `matched_samples` the
    /// larger total — the union, not the last word.
    ///
    /// This is not bookkeeping pedantry. Reporting only the LAST match
    /// would let a build say "nothing applied, this is the no-profile
    /// build" about a build whose inlining the profile had already
    /// changed — which is precisely the shape of untrue statement this
    /// sprint exists to avoid.
    pub fn union(self, other: Coverage) -> Coverage {
        Coverage {
            records: self.records.max(other.records),
            matched: self.matched.max(other.matched),
            matched_samples: self.matched_samples.max(other.matched_samples),
            total_samples: self.total_samples.max(other.total_samples),
            unprofiled_bodies: self.unprofiled_bodies.min(other.unprofiled_bodies),
        }
    }

    /// Stale records: present in the profile, absent from this build.
    pub fn stale(&self) -> usize {
        self.records - self.matched
    }

    /// The fraction of the profile's samples this build could use.
    /// `None` when the profile carries no samples at all.
    pub fn applied_fraction(&self) -> Option<f64> {
        (self.total_samples > 0).then(|| self.matched_samples as f64 / self.total_samples as f64)
    }

    /// Nothing at all applied: the build is byte-identical to a
    /// no-profile build, and the driver says so once.
    pub fn fully_stale(&self) -> bool {
        self.records > 0 && self.matched == 0
    }
}

impl fmt::Display for Coverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{} record(s) matched this build ({} stale), {}/{} sample(s) applied{}",
            self.matched,
            self.records,
            self.stale(),
            self.matched_samples,
            self.total_samples,
            match self.applied_fraction() {
                Some(x) => format!(" ({:.1}%)", x * 100.0),
                None => String::new(),
            }
        )
    }
}

/// Score `profile` against the body hashes this build actually has.
pub fn coverage<'a>(
    profile: &Profile,
    build_hashes: impl IntoIterator<Item = &'a str>,
) -> Coverage {
    let mut c = Coverage {
        records: profile.funcs.len(),
        total_samples: profile.samples(),
        ..Coverage::default()
    };
    let mut seen: std::collections::BTreeSet<&str> = Default::default();
    for h in build_hashes {
        if !seen.insert(h) {
            continue;
        }
        match profile.get(h) {
            Some(r) => {
                c.matched += 1;
                c.matched_samples = c.matched_samples.saturating_add(r.total());
            }
            None => c.unprofiled_bodies += 1,
        }
    }
    c
}

fn parse_u32(tok: Option<&str>, line: usize, what: &str) -> Result<u32, ProfileError> {
    let t = tok.ok_or_else(|| err(line, format!("missing {what}")))?;
    t.parse()
        .map_err(|_| err(line, format!("{what}: `{}` is not a count", truncate(t))))
}

fn parse_u64(tok: Option<&str>, line: usize, what: &str) -> Result<u64, ProfileError> {
    let t = tok.ok_or_else(|| err(line, format!("missing {what}")))?;
    t.parse()
        .map_err(|_| err(line, format!("{what}: `{}` is not a count", truncate(t))))
}

fn truncate(s: &str) -> String {
    if s.chars().count() <= 32 {
        return s.to_string();
    }
    let head: String = s.chars().take(32).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: u8) -> String {
        std::iter::repeat_n(format!("{byte:02x}"), 32).collect()
    }

    fn sample() -> Profile {
        let mut funcs = BTreeMap::new();
        funcs.insert(
            h(0xaa),
            Record {
                blocks: vec![10, 90, 5],
            },
        );
        funcs.insert(h(0x0b), Record { blocks: vec![7] });
        Profile { runs: 2, funcs }
    }

    #[test]
    fn round_trip_is_a_fixpoint() {
        let p = sample();
        let text = p.render();
        let back = Profile::parse(&text).expect("parses");
        assert_eq!(p, back);
        assert_eq!(text, back.render(), "render is canonical");
    }

    #[test]
    fn records_render_in_hash_order() {
        let text = sample().render();
        let order: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("fn "))
            .map(|l| l.split(' ').next().unwrap_or(""))
            .collect();
        assert_eq!(order, vec![h(0x0b), h(0xaa)], "ascending hash order");
    }

    #[test]
    fn samples_is_the_sum_of_every_count() {
        assert_eq!(sample().samples(), 10 + 90 + 5 + 7);
    }

    // ---- version discipline: work, or refuse loudly ------------------

    #[test]
    fn a_future_version_refuses_loudly() {
        let e = Profile::parse("wprof 2\nproducer instr\n").expect_err("refuses");
        assert!(e.msg.contains("v2"), "{e}");
        assert!(e.msg.contains("refusing"), "{e}");
    }

    #[test]
    fn a_missing_version_header_refuses() {
        let e = Profile::parse("producer instr\nruns 1\n").expect_err("refuses");
        assert!(e.msg.contains("wprof <version>"), "{e}");
    }

    #[test]
    fn an_empty_file_refuses() {
        assert!(Profile::parse("").is_err());
        assert!(Profile::parse("\n\n# just a comment\n").is_err());
    }

    #[test]
    fn an_unknown_directive_refuses_instead_of_skipping() {
        let text = format!("wprof 1\nproducer instr\nedges 4\nfn {} 1 3\n", h(1));
        let e = Profile::parse(&text).expect_err("refuses");
        assert!(e.msg.contains("unknown directive `edges`"), "{e}");
    }

    #[test]
    fn the_reserved_sample_producer_refuses_at_v1() {
        // The headroom the sprint's non-targets leave open: the slot
        // exists, and a v1 reader handed one says so instead of reading
        // sample counts as execution counts.
        let text = format!("wprof 1\nproducer sample\nfn {} 1 3\n", h(1));
        let e = Profile::parse(&text).expect_err("refuses");
        assert!(e.msg.contains("producer `sample`"), "{e}");
        assert!(e.msg.contains("instr"), "{e}");
    }

    #[test]
    fn extra_counts_on_a_record_refuse() {
        let text = format!("wprof 1\nproducer instr\nfn {} 2 3 4 5\n", h(1));
        let e = Profile::parse(&text).expect_err("refuses");
        assert!(e.msg.contains("more counts"), "{e}");
    }

    #[test]
    fn too_few_counts_refuse() {
        let text = format!("wprof 1\nproducer instr\nfn {} 3 1 2\n", h(1));
        assert!(Profile::parse(&text).is_err());
    }

    #[test]
    fn a_bad_hash_refuses() {
        let e = Profile::parse("wprof 1\nproducer instr\nfn main 1 3\n").expect_err("refuses");
        assert!(e.msg.contains("not a content hash"), "{e}");
    }

    #[test]
    fn a_lying_samples_header_refuses() {
        let text = format!("wprof 1\nproducer instr\nsamples 99\nfn {} 1 3\n", h(1));
        let e = Profile::parse(&text).expect_err("refuses");
        assert!(e.msg.contains("corrupt or truncated"), "{e}");
    }

    #[test]
    fn a_missing_producer_refuses() {
        let text = format!("wprof 1\nfn {} 1 3\n", h(1));
        assert!(Profile::parse(&text).is_err());
    }

    // ---- merging -----------------------------------------------------

    #[test]
    fn merge_sums_compatible_records_and_keeps_the_rest() {
        let mut a = sample();
        let mut funcs = BTreeMap::new();
        funcs.insert(
            h(0xaa),
            Record {
                blocks: vec![1, 2, 3],
            },
        );
        funcs.insert(h(0xcc), Record { blocks: vec![4] });
        a.merge(&Profile { runs: 1, funcs }).expect("merges");
        assert_eq!(a.funcs[&h(0xaa)].blocks, vec![11, 92, 8]);
        assert_eq!(a.funcs[&h(0x0b)].blocks, vec![7]);
        assert_eq!(a.funcs[&h(0xcc)].blocks, vec![4]);
        assert_eq!(a.runs, 3);
    }

    #[test]
    fn merging_incompatible_records_refuses() {
        let mut a = sample();
        let mut funcs = BTreeMap::new();
        funcs.insert(h(0xaa), Record { blocks: vec![1] });
        let e = a.merge(&Profile { runs: 1, funcs }).expect_err("refuses");
        assert!(e.msg.contains("corrupt"), "{e}");
    }

    #[test]
    fn merge_is_commutative_in_its_counts() {
        let mut funcs = BTreeMap::new();
        funcs.insert(h(0xcc), Record { blocks: vec![4] });
        let other = Profile { runs: 1, funcs };
        let mut a = sample();
        a.merge(&other).expect("merges");
        let mut b = other.clone();
        b.merge(&sample()).expect("merges");
        assert_eq!(a.funcs, b.funcs);
    }

    // ---- coverage ----------------------------------------------------

    #[test]
    fn coverage_scores_matched_stale_and_unprofiled() {
        let p = sample();
        let build = [h(0xaa), h(0xdd)];
        let c = coverage(&p, build.iter().map(String::as_str));
        assert_eq!(c.records, 2);
        assert_eq!(c.matched, 1);
        assert_eq!(c.stale(), 1);
        assert_eq!(c.matched_samples, 105);
        assert_eq!(c.total_samples, 112);
        assert_eq!(c.unprofiled_bodies, 1);
        assert!(!c.fully_stale());
    }

    #[test]
    fn a_profile_that_matches_nothing_is_fully_stale() {
        let c = coverage(&sample(), [h(0xdd).as_str()]);
        assert!(c.fully_stale());
        assert_eq!(c.applied_fraction(), Some(0.0));
    }

    #[test]
    fn an_empty_profile_is_never_fully_stale() {
        let c = coverage(&Profile::default(), [h(0xdd).as_str()]);
        assert!(!c.fully_stale(), "no records is not a stale profile");
    }

    #[test]
    fn record_accessors_read_the_positional_counts() {
        let r = Record {
            blocks: vec![10, 90, 5],
        };
        assert_eq!(r.entry(), 10, "block 0 is the entry block");
        assert_eq!(r.peak(), 90);
        assert_eq!(r.total(), 105);
    }
}
