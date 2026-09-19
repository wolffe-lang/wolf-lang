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
  `pass` (the ladder stopped clean and nothing executed —
  `[proto.record.pass]`), `fail(CODE)` (rejected;
  first diagnostic's code), `exit(N)`, `trap(kind)` (kind per
  `[conf.trap.set]`), `ub(anchor)`, `unsupported`.
- `[proto.record.pass]` `pass` is the ladder's CLEAN STOP: every rung
  through `phase_reached` completed and found nothing to report, and
  nothing was executed. It arises two ways and they are one fact —
  the run stopped at an explicit `--phase=<p>`, or the run named no
  phase and reached the deepest rung THIS LANE implements. An
  implementation (or a lane of one) whose deepest rung is static
  answers `pass` there. (Amended 2026-09-18, s169 — wolf-lang#150 and
  #343.)

  **`pass` is not `unsupported`, and that distinction is the clause's
  whole content.** `unsupported` is a statement about the PROGRAM:
  this implementation's coverage does not reach a construct in it, and
  `[proto.record.unsupported]` files it in the conservatism ledger.
  `pass` is a statement about the RUN: the implementation has no
  complaint about the program, and this lane executes nothing. A third
  fact, `fail(CODE)`, is the language declining the program. Three
  facts, three verdicts.

  Before s169 wolfgang's default lane spelled the first and the second
  the same way — `unsupported` at `wir` — so no reader could tell a
  scope gap from a clean program, and wolf-book's sample runner had
  written the workaround into its own source ("the bare verdict is
  useless here — `unsupported` is also what a perfectly good program
  reports"). The witness that forced the amendment is wolf-lang#343,
  filed as five `List` slice forms and a `str` slice the reference
  lowering supposedly did not cover: measured at 0.2.15 all six lower
  CLEAN (no `x-unsupported-construct`, empty stderr) and run to
  `exit(0)` on the checked and native lanes. The verdict was never
  about slices.

  `pass` carries no program outcome. It is never coverage
  (`[proto.cmp.coverage]`), never carries `stdout_sha256`, and a lane
  that answers `pass` has compared nothing dynamically.
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
  gaps stay visible, never silent. It is a claim about the program's
  CONSTRUCTS and never about the lane's engines: a lane that completed
  its ladder clean and simply does not execute answers `pass`
  (`[proto.record.pass]`), never `unsupported`. An implementation that
  can name the refused construct SHOULD, as `x-unsupported-construct`
  (`[proto.record.ext]`) — a bare `unsupported` costs its reader a
  bisect.
- `[proto.record.trap]` `trap_message` (added s169, additive within
  `"protocol": 1` — validators accept records with or without it) is
  the human text the PROGRAM supplied for its own fault: today exactly
  the second argument of `assert(cond, msg)` (`[conf.trap.assert]`),
  evaluated only on the failing path. Present only on a `trap(kind)`
  verdict and only where the implementation holds the text;
  honest-absent otherwise.

  **It is never compared.** `[proto.record.diag]` rules that wording is
  a per-implementation quality concern and the same rule governs here,
  with one difference worth stating: this wording is the *program's*,
  not the implementation's, so two conforming implementations will
  usually agree — and the protocol still declines to require it,
  because an implementation that drops the message
  (`[conf.trap.assert]` permits that until formatting lands) must not
  become a divergence for it.

  It exists because `stdout_inline` was the only channel a runner had
  for a trap's message, which is the program's OUTPUT field — so a
  runner either read a program's own words out of its output stream or
  went without (wolf-lang#150, wolf-book ch21/ch27).
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
  status and `stdout_sha256`; for `trap`, compare kind, **and compare
  `stdout_sha256` when both records carry it** — the output a program
  wrote before its fault is a program observation exactly as an
  exiting program's is. Absence of the field on either side is
  `[proto.record.fields]`'s honest-absent and is never a divergence, as
  `[proto.cmp.warn]` treats a missing `warnings` array. For `ub`,
  compare the finding; output bytes are not compared on a program with
  undefined behaviour. (The trap sentence ruled 2026-09-11, BACKLOG
  B25, as is35 drafted it on wolf-lang#216, and written by s163. At
  its writing every tier that runs a writing trap returned one digest:
  63 trap records in lupin 0.1.24's bundle, 61 writing nothing and
  both writers byte-identical on `--checked`, `--native`, `--release`
  and lupin. **The cost, stated:** none to any program — the digest is
  already in both records — and one string comparison per trap record
  to the comparator.)
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
  `trap_message` on either side (`[proto.record.trap]`);
  `unsupported` on either side.
- `[proto.cmp.pass]` `pass` on a FULL-LADDER run (added s169). On a run
  that named no `--phase`, a side reporting `pass`
  (`[proto.record.pass]`) against a DYNAMIC verdict — `exit`, `trap`,
  `ub` — is **never a divergence**: the passing side stopped without
  executing, so the two records are not two answers to one question.
  Everything else `pass` can meet on a full-ladder run **is** compared,
  and the carve-out is exactly one verdict class wide:

  - `pass` against `fail(CODE)` **is a divergence** (verdict-mismatch
    class). One implementation completed the static ladder clean and
    the other rejected the program statically; that is a disagreement
    about the language, and it is the disagreement the protocol most
    wants to see. Note what this buys: before s169 wolfgang answered
    `unsupported` here, which the clause above excuses on either side,
    so this pair was invisible. The amendment does not only rename a
    verdict — it **opens a hole the old spelling kept shut**.
  - `pass` against `pass` agrees, at whatever rungs the two lanes
    stopped (`[proto.cmp.rung]`'s reasoning: where a lane runs out of
    engine is an architecture fact).
  - `pass` against `unsupported` is not a divergence, by the clause
    above.

  At an explicit `--phase=<p>` nothing changes: `[proto.cmp.rung]`'s
  last sentence already rules `pass` there, and it still rules it.
- `[proto.cmp.coverage]` Coverage, the conservatism ledger's sibling
  (s82, wolf-lang#90). A report states, per lane and as their **union**,
  how many corpus entries the lane *executed*: `phase_reached` is `run`
  and the verdict is a dynamic observation — `exit`, `trap` or `ub`.
  Nothing else is coverage. `unsupported` is a refusal
  (`[proto.record.unsupported]`) and two lanes refusing the same entry
  is not agreement about it; `fail(CODE)` is a rejection compared at its
  own rung under `[proto.cmp.rung]`, which is comparison but not
  run-rung coverage; `pass` answers a `--phase` request and carries no
  program outcome. An implementation with several execution engines
  publishes the **union and the intersection both**: the lanes are not
  required to nest, and where they do not, the union is the honest
  figure while any single lane's count overstates what one run compares.
  A conforming report never lets the number rise by widening what counts
  as compared — the divergence count and the coverage it was drawn from
  are read together or not at all.
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
