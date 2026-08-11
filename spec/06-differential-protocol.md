# Wolf Language Specification — 06: Differential Protocol

Status: normative, v0 (sprint s06). The two-implementation contract: the
compiler (wolf-lang) and the independent interpreter (wolf-interp) are
comparable **only** through this protocol — it is the single shared
artifact between the tracks, and it is a spec, not code.

---

## §1 Invocation `[proto.invoke]`

- `[proto.invoke.cli]` A conforming implementation exposes:
  `<impl> conform-run <file.lu> [--phase=<p>] [--seed=N] [--json]`
  where `<p>` ∈ the canonical phase ladder `none, lex, parse, resolve,
  typecheck, mem, wir, run` (stop after `<p>`; default: run as deep as
  the implementation can). `--json` is the machine mode this spec
  defines; without it, output is human-shaped and unspecified.
- `[proto.invoke.exit]` `conform-run` itself exits 0 whenever it
  produced a well-formed observation record — the *record* carries the
  program's outcome. Tool-level failures (missing file, bad flags) exit
  nonzero with no record.

## §2 The observation record `[proto.record]`

One JSON object on stdout. Schema (`"protocol": 1`):

```json
{
  "protocol": 1,
  "impl": "wolfgang",
  "impl_version": "0.0.1",
  "commit": "abc1234",
  "file": "corpus/hello.lu",
  "phase_reached": "none",
  "seeded": false,
  "diagnostics": [ { "code": "E1002", "span": [120, 133], "severity": "error" } ],
  "warnings": [ { "code": "W1301", "span": [356, 362] } ],
  "verdict": "unsupported",
  "stdout_sha256": null,
  "stdout_inline": null
}
```

- `[proto.record.phase]` `phase_reached` names the deepest phase that
  **completed**. An implementation that cannot complete a phase because a
  construct is outside its current coverage reports the last *completed*
  phase with verdict `unsupported` — never the incomplete phase. A
  `fail(CODE)` verdict reports the phase that failed as `phase_reached`.
- `[proto.record.fields]` Required: `protocol`, `impl`, `impl_version`,
  `commit`, `file`, `phase_reached`, `seeded`, `diagnostics`, `verdict`.
  `stdout_sha256`/`stdout_inline` are required when `verdict` is
  `exit(0-255)` and the program wrote output; `stdout_inline` is
  included up to 4096 bytes, the hash always.
- `[proto.record.verdict]` `verdict` is one of:
  `pass` (stopped at requested phase, clean), `fail(CODE)` (rejected;
  first diagnostic's code), `exit(N)`, `trap(kind)` (kind per
  `[conf.trap.set]`), `ub(anchor)`, `unsupported`.
- `[proto.record.diag]` Diagnostics carry `{code, span, severity}` —
  byte-offset half-open spans (s07's byte-exact contract). **Messages
  are never part of the protocol** (D22: wording is a per-implementation
  quality concern).
- `[proto.record.ub]` `ub(anchor)` cites the s04 §7 row (e.g.,
  `ub(mem.ub)` with the row id in `x-ub-row`, or the specific clause).
  It **participates in comparison**: one side reporting `ub(…)` where
  the other runs defined is a *soundness-candidate* divergence — the
  highest-severity class.
- `[proto.record.unsupported]` `unsupported` is a legal verdict: the
  feature is outside this implementation's current scope. Excluded from
  divergence counting; reported in the **conservatism ledger** so scope
  gaps stay visible, never silent.
- `[proto.record.warn]` `warnings` (added s67, additive within
  `"protocol": 1` — validators accept records with or without it) is
  the warning observations as `{code, span}` entries: every
  warning-severity diagnostic the run produced *after* source-level
  `#[allow]` suppression (the attribute is part of the program, so
  every implementation honors it; spec/01 §9.3). An implementation
  includes the array whenever it runs warning analyses and omits it
  entirely otherwise — **honest-absent**: an implementation that has
  not built a lint reports no array rather than an empty one it
  cannot stand behind. Severity is not repeated (the array is
  warnings by definition); `diagnostics` continues to carry the same
  observations with `"severity": "warning"` per `[proto.record.diag]`.
- `[proto.record.ext]` Keys beginning `x-` are implementation
  extensions. They participate in equality only when both records carry
  the same key.

## §3 Comparison semantics `[proto.cmp]`

- `[proto.cmp.phase]` At `--phase=lex|parse`: compare `phase_reached`
  and `verdict`; for `fail`, the **first** diagnostic's code and span
  must agree (the interpreter performs no recovery — is01; later
  diagnostics are a compiler-quality concern, never compared).
  At `resolve|typecheck|mem`: same, plus `fail` codes drawn from the
  E1xxx+ families. At `run`: compare `verdict`; for `exit`, compare
  status and `stdout_sha256`; for `trap`, compare kind only.
- `[proto.cmp.rung]` Rejection-rung tolerance (s70, the DIV-011
  family's ruling): when both records reject with `fail(CODE)` and the
  **first** diagnostic's code and span agree, the records AGREE even
  when `phase_reached` names different rungs of the shared ladder.
  Where on the ladder an implementation discovers a rejection is an
  architecture fact — a fused resolver rejects at `resolve` what a
  staged checker rejects at `typecheck` — not a semantic observation;
  the rejection itself (code + span) is the observation. The tolerance
  is exactly one verdict wide: it never spans `fail` vs any other
  verdict, and a full-ladder run (no `--phase`) that rejects on one
  side while the other runs to a dynamic outcome stays a divergence
  under `[proto.cmp.phase]`. At an explicit `--phase=<p>`, a side that
  reports `pass` at `<p>` while the other already rejected is likewise
  still a divergence — the tolerance compares two rejections, never a
  rejection against silence.
- `[proto.cmp.warn]` Warning parity (s67): when **both** records carry
  the `warnings` array, the sorted `{code, span}` sets must agree —
  a mismatch is a span/code-class divergence. Absent on either side is
  never a divergence (`[proto.record.warn]`'s honest-absent), so lupin
  implements the subset whose analyses it has and parity grows
  lint-by-lint.
- `[proto.cmp.defined-divergence]` Never divergences: schedule-dependent
  output *ordering* when the litmus is tagged concurrency-nondeterministic
  and `seeded` is false on either side; unspecified layout observations
  (Tier-3 address inspection); diagnostic count beyond the first;
  `x-` keys absent on one side; the `warnings` array absent on one side;
  `unsupported` on either side.
- `[proto.cmp.triage]` Everything else is a divergence and files a bug.
  Triage rule, normative: **the spec document is the defendant first** —
  an ambiguous clause is presumed the root cause until the clause is
  shown unambiguous; then the implementation that disagrees with it is
  the defendant. This is how differential testing hardens the spec
  (01 Q6).
- `[proto.cmp.severity]` Divergence classes, descending:
  soundness-candidate (`[proto.record.ub]`), verdict mismatch,
  span/code mismatch, stdout mismatch. Reports order by class.

## §4 Nondeterminism `[proto.seed]`

- `[proto.seed.flag]` `--seed=N` requests the deterministic schedule
  seeded per spec 03 §5 (`sched-ev/0`). An implementation without
  seeded scheduling declares `"seeded": false` and concurrency litmuses
  compare structurally only (`[proto.cmp.defined-divergence]`).
- `[proto.seed.equal]` For two records with `seeded: true` and equal
  seeds, comparison proceeds as if sequential: equal seeds ⇒ comparable
  observations, including output bytes (is06/is07 depend on this; the
  compiler honors it from s36).

## §5 Reference harness `[proto.harness]`

- `[proto.harness.differ]` `cargo xtask differ <implA-cmd> <implB-cmd>`
  walks `corpus/**/*.lu`, invokes both implementations' `conform-run
  --json`, validates both records against §2, applies §3, and emits a
  JSONL divergence report (one line per file:
  `{"file", "class", "a", "b"}` — empty report = green).
- `[proto.harness.fixtures]` `corpus/protocol/` holds canned observation
  records — valid, wrong-protocol-version, extension-bearing,
  missing-required-field — that every conforming schema validator must
  accept/reject exactly as named. They are the protocol's own tests.
