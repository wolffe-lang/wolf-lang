# 8. Packages

The package layer's formats, normative since s51. The covenant over
all of them is D33: **reading the dependency graph never executes
anything** — no build scripts, no hooks, no fallbacks, ever. A
manifest that asks for build-time execution is refused (E1503).

## 8.1 The manifest

- `[pkg.manifest]` A package is a versioned tree of modules (D32)
  described by a `wolf.pkg` file at its root: a single `pkg { … }`
  block in wolf-literal data syntax. Parsing it is data parsing; no
  expression in a manifest is ever evaluated.
- `[pkg.manifest.schema]` Top-level keys: `name` (scoped
  `owner/pkg` — there is no flat namespace), `version` (dotted
  numerics), `edition`, `wolf` (minimum toolchain, advisory at v0),
  `fingerprint` (immutable identity minted at init, advisory at v0),
  `deps`, `test`, `bench` (target-scoped dependency sections — a
  test-only dependency never reaches a consumer's build), `features`,
  `capabilities`, `paths`, `min_age`, `c`, `lints`, `trusted`,
  `replace`, `exclude`. Unknown keys are schema errors (E1502).
- `[pkg.manifest.dep]` A dependency entry binds a source-import
  ALIAS to a source: `{ path: "…" }` (a tree in place),
  `{ git: "…", tag: "…" }` (a pinned fetch), or
  `{ pkg: "owner/name", major: N, min: "X.Y.Z" }` (registry form —
  the entry SHAPE is stable now, the transport arrives with the
  hosted registry; X7). Aliases are one flat namespace per project at
  v0; two majors of one package coexist as two entries under two
  aliases. `lazy: true` marks a dependency fetched only if imported.
- `[pkg.manifest.replace]` `replace: { alias: { source } }` is a
  TOP-LEVEL-EXCLUSIVE power: the root manifest's replacement
  overrides any dependency entry with that alias, wherever in the
  graph it is declared; replacement `path:` sources anchor at the
  root. A non-root manifest carrying `replace`/`exclude` is parsed,
  ignored, and warned about — the build's root decides, dependencies
  advise.
- `[pkg.manifest.exclude]` `exclude: [ "name@version", … ]` refuses a
  resolved package matching an entry (E1510). At v0 transport there
  is no version list to fall back over, so exclusion refuses rather
  than silently downgrading.
- `[pkg.manifest.fmt]` A manifest round-trips through `wolf fmt` as
  an identity: the formatter validates it with the manifest parser
  and leaves the bytes alone. (A canonical reprint would be lossy
  over comments and carried-but-inert keys; a lossless literal-CST
  formatter may tighten this later without changing the schema.)

## 8.2 Resolution

- `[pkg.resolve.mvs]` Version selection is minimal version selection,
  verbatim from vgo: dependencies state MINIMUMS, a build uses the
  max-of-minimums, and a newly published version is never picked up
  until a human runs `wolf update`. No ranges, no solver, one unique
  solution. (The MVS engine is transport-independent and
  unit-pinned; it engages end-to-end with the registry transport.)
- `[pkg.resolve.offline]` Resolution is the only phase that may reach
  the network. A build fetches nothing: it uses what `wolf.sum` pins
  and the store holds (E1505/E1509 otherwise).

## 8.3 Integrity

- `[pkg.sum]` `wolf.sum` is the integrity LEDGER, not a resolution
  input: one line per non-root package —
  `alias version <multihash|-> caps=<set|->` — recording the blake3
  content address (`b3:`) of the source-filtered tree (identity
  files: `wolf.pkg`, `*.lu`, `*.wolfi`; VCS metadata, caches, and a
  consumer's `vendor/` mirror never perturb identity). Resolution
  output is a pure function of manifests; the ledger witnesses it.
- `[pkg.store]` Fetched trees live in a global content-addressed
  store, immutable and re-verified on every use: a store tree whose
  re-derived hash differs from its address is the supply-chain alarm
  (E1506), never a cache hit.
- `[pkg.vendor]` `wolf vendor` writes the store-backed slice of a
  resolution into `vendor/wolf/<multihash>/` — a store-layout mirror
  the next build prefers automatically. Because it IS a store, every
  store guarantee applies unchanged; mirrors are untrusted by
  construction.

## 8.4 The transparency log

- `[pkg.log.record]` One append-only line per published version:
  `owner/pkg@version tree=<addr> manifest=<addr> interface=<addr>`
  (`addr` = `b3:` + 64 hex). A published version is immutable — the
  same key never appears twice, even for its author.
- `[pkg.log.merkle]` The log is Merkle-hashed RFC-6962-style over
  blake3 (`H(0x00‖line)` leaves, `H(0x01‖l‖r)` nodes). A signed
  TREE HEAD (`log.head`: `head size=N root=<hex> sig=<alg>:<hex>`)
  pins the sequence; clients verify INCLUSION of a record under the
  head and CONSISTENCY between heads (append-only or alarm). The
  signature's algorithm tag is the frozen surface: v1 ships `b3k`
  (keyed blake3); an asymmetric scheme slots into the same field.
- `[pkg.log.transport]` v1 transport is dumb files (`log`,
  `log.head`) served from anywhere; client verification is
  transport-agnostic by construction, and the hosted registry (c15)
  changes only how the bytes arrive. A log with records but no head
  proves nothing and is refused.

## 8.5 Capabilities

- `[pkg.caps]` Every package declares its capability set from
  `[net, fs, exec, env, ffi, unsafe, comptime]` (I13). A package
  whose modules reach a capability-carrying std facade without
  declaring the capability fails its build (E1504) — an error, never
  a warning. `wolf audit` renders the transitive capability tree and
  diffs it across upgrades; `wolf audit --ci` exits non-zero on any
  acquisition.

## 8.6 The verb surface

- `[pkg.verbs]` Landed by s51 and permanent (D34): `add`, `rm`,
  `update`, `vendor`, `audit`, `tree`, `why`, `publish` (with `init`
  and `cache` per D45). A missing import in project mode is an error
  with a fix-it naming the `wolf add` invocation, never an implicit
  fetch.
