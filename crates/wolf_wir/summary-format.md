# WIR module summaries — the frozen whole-program format (s43)

A **summary** is what the whole-program phase knows about a function
without loading its body. Summaries are the thin-LTO spine of wolf's
release tier: they drive codegen clustering, the cross-cluster import
decision, and the cluster object cache's key. This file is to the
summary index what `text.md` is to the WIR textual format.

The format is **frozen**. c12 (the resident compiler) and s45 (PGO)
key on this text, so the schema below changes only with a bump of
`wolf_wir::midend::summary::SUMMARY_FORMAT_VERSION`. The version is
folded into every release-tier cache key, so a bump invalidates
cluster objects and nothing else — debug-tier module objects are
deliberately untouched (D4's honest tiers: the dev loop is Tier-F's).

- Producer: `wolf_wir::midend::summary::summarize`
- Serialization: `ProgramSummary::render` (this format)
- Digest: `ProgramSummary::digest` = SHA-256 of the rendered text
- Driver surface: `wolf build --release --codegen-report`

## Schema (v1)

Line-oriented text, one function per line, deterministic field order,
sorted by function name. No floats, no wall clock, no thread order, no
hash-map iteration order — the whole index is a pure function of the
module plus the home map.

```text
summary-format 2
fn <name> home=<module> size=<insts> blocks=<n> flags=<EMTSRKA|-> \
   hash=<sha256> facts=<region>/<noalias>/<range>/<deref>/<frozen> \
   hot=<-|u32> calls=[<callee>@<depth>/<sites>/<constargs>,...] impls=[]
```

| Field | Meaning |
| --- | --- |
| `name` | WIR function name (the linker-visible identity for exports). |
| `home` | Defining source module; `root` for the package root, `-` when the producer supplied no home map (fixtures). |
| `size` | WIR instruction count over the reachable layout — the unit every threshold in `Thresholds` is denominated in. |
| `blocks` | Reachable block count. |
| `flags` | Fixed letter order, `-` where absent: `E` exported (D19 C membrane), `M` program entry, `T` effect-token parameters, `S` single-block body, `R` recursive (reaches itself), `K` carries a task seam, `A` address-taken (`func.addr`). |
| `hash` | D8 content hash of the NORMALIZED body — equivalence up to local names only (I11). See below. |
| `facts` | D2 fact digest: counts per kind, in the fixed order region/noalias/range/deref/frozen. |
| `hot` | Hotness hint. `-` = **unknown** (no profile, or no record for this body) — the default in every build. Otherwise a normalized **0..=1000 rank**. Reserved by s43, filled by s45 **without a format bump**; see below. |
| `calls` | Outgoing edges to MODULE functions, sorted by callee. External `decl` callees — including every `__wolf_rt_*` seam — are not edges: they are opaque by construction. The three fields after `@` are the deepest loop depth of any site, the site count, and the constant-argument count, i.e. exactly the inputs the inliner's budget consults, so import decisions are makeable from summaries alone. |
| `impls` | **Reserved (D42). Always `[]` at v1.** See below. |

### The `hot` slot, filled (s45)

s43 froze this format with a `ret=`/`stores=` (v2, s99: the interprocedural range half — the provable return range and the store-meet per local container allocation site; `-`/absent when unproven) and `hot=` slot in it so profile data could
arrive without a bump. s45 fills that slot — **and adds no field beside
it**, which is what having reserved it was for. The line shape, field
order, and `SUMMARY_FORMAT_VERSION` are unchanged; a consumer written
against s43 reads an s45 index correctly.

- `hot=-` — **unknown**. No profile was supplied, or the supplied one
  has no record for this body. This is the value in every default
  build, and it must never be read as "cold": treating unknown as cold
  is what would let a stale or partial profile pessimize a build below
  the no-profile one.
- `hot=<0..=1000>` — a **rank**: `1000 × this body's peak block count ÷
  the hottest peak block count in the program`, integer arithmetic.
  `hot=0` is *proven cold* — a record exists and the body never ran —
  and is real information the inliner acts on.

The rank is relative and bounded on purpose. This index's digest rides
the release cluster cache key, and a raw execution count would make
that key depend on how long the training run happened to be; two
training runs of the same shape and different lengths produce the same
index. The scale is over the **peak block** count rather than the entry
count, because a function called once around a million-iteration loop
is hot and its entry count says otherwise.

Producer: `wolf_wir::midend::summary::apply_profile`, matching by
content hash against a `.wprof` (`wolf_wir/wprof-format.md`).
Consumers: the cluster partition and import ranking
(`Thresholds::cluster_hot_boost`) and the inliner's budget
(`Thresholds::hot_rank` / `inline_hot_bonus` / `inline_cold_max`).

### The reserved `impls` slot (D42)

Per-trait possible-implementation sets — the input a singleton-`dyn`
devirtualization pass would need. Report 10 ruled that pass **POST-M2
backlog** (no T1 kernel is dyn-bound), and ruled that the summary
format nevertheless freezes **with headroom for the sets**, so c12 and
the tooling track can rely on the slot existing. Rendered form, when
it is eventually filled:

```text
impls=[<trait>:<impl>|<impl>|...,<trait>:<impl>,...]
```

Nothing in s43 writes it and no pass reads it. It is emitted empty on
every line, which is what makes filling it a non-breaking change.

### The content hash

`hash` is SHA-256 over the canonical print of the body with the
function's own name and linkage erased
(`wolf_wir::midend::dedup::normal_form`). The canonical printer
already normalizes exactly what I11 licenses: blocks in reverse
postorder, values numbered positionally in definition order, facts
sorted, source spans and debug-variable names excluded. Everything
else hashes — opcodes, operand order, types (region ids included:
region identity lives in the type), signatures, callee names, data
references, error tags.

Two bodies differing in any instruction, type, or fact hash apart. The
hash is taken POST-mid-end (D8), so instantiations converge only after
their type-dependent differences have been folded away.

## Clusters

Clusters are derived from summaries, not stored in them. A cluster's
content key — what the driver's object cache addresses objects by — is
SHA-256 over:

```text
summary-format <version>
cluster <name>
member <name> <body hash>      ; sorted
import <name> <body hash>      ; sorted
```

A cluster object is therefore addressed by the exact bodies the
backend will see, plus the summary format that described them: a stale
object is unaddressable rather than merely unused.
