# `.wprof` — the wolf profile format (s45)

This is the format doc the s45 contract requires: the v1 schema, the
keying rule, and the **version bump policy**. The implementation is
`wolf_wir/src/profile.rs` (reader, writer, merger) and
`wolf_rt/src/prof.rs` (the runtime writer). Both are held to this
document by tests in the same files.

A `.wprof` is plain data. It is never executable, it is never a package
artifact, and nothing in the wolf tooling requires one to exist (D4:
PGO is integrated, optional, and **never required**; D33: no build
scripts — instrument→run→use is two driver flags and a data file).

## The v1 schema

```text
wprof 1
producer instr
runs 3
samples 800005
fn <64 hex> <n> <c0> <c1> … <c[n-1]>
```

| line | meaning |
|---|---|
| `wprof <version>` | **Mandatory, first.** The reader checks it before anything else. |
| `producer <name>` | How the counts were obtained. `instr` is the only value v1 produces or accepts. |
| `runs <u32>` | How many program runs were merged into this file. |
| `samples <u64>` | Total counter increments across every record. Checked against the records; a disagreement is a corrupt file. |
| `fn <hash> <n> <counts…>` | One function body. `n` is its block count and must equal the number of counts. |

Blank lines and `#` comments are ignored. Records are written in
ascending hash order, so a `.wprof` is a pure function of the run's
counters and diffs cleanly.

### `producer`, and the sample-derived headroom

s45's non-targets say: no sampling-based PGO at v1 — instrumentation
only, *"the `.wprof` container leaves room for a sample-derived producer
later"*. This line is that room, and report 10 kept it.

It is spelled as **data, not as a version**, deliberately. A future
`producer sample` file is still a v1 container: same header, same
record shape, same keying. What changes is the meaning of the numbers —
sampled hit counts are not execution counts, and a consumer that
averaged them into an inline budget as if they were would be wrong in a
way no schema check catches. So a v1 reader handed `producer sample`
**refuses loudly** and names the producer it does not know, rather than
skipping the line and reading the counts anyway.

## Keying: the content hash, and nothing else

A record's key is the **s43 content hash** of the normalized function
body (`midend::dedup::body_hash`) — the same hash the frozen summary
index publishes, the same hash D8 dedup collapses on, the same hash the
release cluster cache key folds. Never a symbol name, never a DefPath,
never a source position.

What that buys, and what it costs:

- **A profile survives recompilation.** The hash is over content, so
  rebuilding an untouched program leaves every record applicable.
- **An edit invalidates precisely.** Change a body and exactly that
  body's record no longer matches. Its neighbours keep theirs.
- **A stale record is ignorable, not poisonous.** It names a body this
  build does not contain, so it is dropped. It cannot be applied to the
  wrong body, because the only thing that could match it is a body with
  the same hash — which is the same body. This is the failure mode
  name-keyed profiles have and this one does not
  (`refs/papers/stale-profile-matching.txt`).
- **The counts are positional and structurally valid.** They index
  `print::block_order` — the canonical reachable-RPO order the hash is
  taken over — so a record and a body agree on block identity exactly
  when they agree on the hash. A record whose length disagrees with the
  body it matched is therefore a corrupt file, not a stale one, and is
  refused.

### The granularity is *surviving bodies*, not source functions

Worth stating plainly, because it surprises: wolf compiles
whole-program and inlines aggressively, so the bodies a profile names
are the ones that **survive** optimization. Two consequences:

- a program small enough to collapse into a single `main` has exactly
  one record, and any edit anywhere invalidates all of it;
- editing a body invalidates that body **and every body that inlined a
  copy of it** — which is not over-invalidation, since those bodies did
  change and their old counts describe blocks that no longer exist.

Both are precise, both are harmless (a lost record is ignored, never
misapplied), and both mean "an edit invalidates exactly what it
changed" is a statement about post-optimization bodies. The driver test
`crates/wolf_driver/tests/pgo.rs::an_edit_invalidates_exactly_what_it_changed`
pins it at the granularity it actually has.

## The version bump policy

The rule is: **a reader of an old format must either work or refuse
loudly, never misread.** Concretely, a v1 reader —

- requires the version line, and refuses a file without one;
- refuses a version it does not implement, naming both versions;
- refuses an **unknown directive** rather than skipping it (skipping is
  how a reader misreads a newer file);
- refuses a `fn` record carrying more or fewer counts than its declared
  `n`, because v1 defines the line as *exactly* `n` counts;
- refuses a `samples` header that disagrees with the records.

Therefore **bump the version** for: a new directive; any change to the
`fn` line's shape, including appending a field after the counts;
any change to what a count means or what order the counts are in; any
change to the hash function or the normalization it runs over.

**Do not bump** for: a new `producer` value (that is what the slot is
for); a new comment; a reordering of the header lines among themselves.

`WPROF_VERSION` also rides the release build's cache key, so a bump
invalidates cached objects even when a profile file's bytes did not
move.

## The build's side of the contract

- `wolf build --release --profile-gen[=<dir>]` — instrument and write
  `<dir>/default.wprof` on exit. `WOLF_PROFILE_FILE` overrides the path
  at run time. Instrumented objects are never written to the release
  object cache, and an instrumented binary is marked by carrying the
  profile runtime's symbols (`__wolf_rt_prof_init`), which nothing else
  does.
- `wolf build --release --profile=<f.wprof>` — consume it. An
  unreadable profile is a **loud build error**; a stale one is one
  summary line and no error; a fully stale one produces a build
  byte-identical to the no-profile build.
- **The profile is a build input and keys the build (D7).** The object
  cache folds the profile file's own content hash, so changing the
  profile always rebuilds and a stale profile can never silently reuse
  an object built under a different one.
- `wolf profile show <f>` / `wolf profile merge <out> <in>…` — inspect
  and combine. Staleness *against a particular build* is reported by
  the build itself (one line, and the full coverage under
  `--codegen-report`), because that is where the body hashes exist.
