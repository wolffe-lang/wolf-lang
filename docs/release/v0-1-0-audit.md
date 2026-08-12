# r01 criteria audit — wolf v0.1.0

Audited 2026-08-11 against the r01 contract's ten-row criteria list
(`sprints/release/r01-v0-1-0.md`): every row verified against current
reality, none waived. Baseline: wolf-lang trunk `e94b879` (== origin)
plus this sprint's uncommitted stamp/spec/notes work; wolf-interp trunk
`bc1018e` (lupin 0.1.6). s71 and lupin v0.1.7 run concurrently — rows
they own are **pending-named-work**, not red. A red row blocks the tag.

Verdict key: **GREEN** (verified true) · **PENDING(owner)** (named
concurrent work) · **RED(owner)** (blocks the tag; must be fixed,
filed, or ruled — never waived).

---

## Row 1 — Zero open soundness-class issues in either implementation

**VERDICT: RED (unowned: wolf-lang #27, #15/S-11) + PENDING(lupin
v0.1.7: wolf-interp #16, #17, #21, #22)**

`gh issue list` (2026-08-11), soundness-class reading: an
implementation produces a wrong answer or misses a mandated trap
without any diagnostic.

- wolf-interp **#16** "silent wrong answer: a function returning an
  ENUM through an error row always takes the miss path" — silent wrong
  answer. Owner: lupin v0.1.7 → pending.
- wolf-interp **#21** "literal-sticky container typing defaults i32
  against the locked 64-bit int (the #53 mechanism)" — misses mandated
  X3 traps (see Row 3's `overflow_elem_write.lu` divergence: lupin
  `exit(0)` where the directive and wolfc say `trap(overflow)`).
  Owner: lupin v0.1.7 → pending.
- wolf-interp **#17** (`as` with unknown target type is a silent
  no-op) and **#22** (`--explore` runs programs the same binary's
  E11xx checks refuse) — silent-acceptance class. Owner: lupin
  v0.1.7 → pending.
- wolf-lang **#27** "read-mode immutability unenforced by BOTH
  implementations (differentially invisible)" — a memory-model
  guarantee unenforced everywhere, invisible to the differ. **No
  concurrent sprint owns it.** RED until it is fixed or a human ruling
  records why it does not block v0.1.0.
- wolf-lang **#15** "mutate-while-iterating disagrees: wolfc
  fail(E1001), lupin runs it silently, `[conf.trap.map]` predicts
  exclusivity" — rides the **open S-11 spec ruling** (`for` operand
  semantics; confirmed open in wolf-interp's divergence log and
  lint-triage). A missed exclusivity trap is soundness-class. **No
  owner named.** RED until ruled.

## Row 2 — argv/env, files, stdin (checked), sockets (checked), print — natively where the ledger says run

**VERDICT: GREEN**

- argv/env: `corpus/os/args_cwd.lu`, `corpus/os/env_roundtrip.lu` —
  `phase: run` (native), corpus gate green.
- files: `corpus/fs/roundtrip.lu`, `corpus/fs/error_row.lu` —
  `phase: run` (native).
- stdin, checked lane: `read_line` is prelude surface; probe verified:
  `echo howl | wolf run stdin_probe.lu` → `line: howl`. (`conform-run`
  deliberately postures stdin at EOF for determinism, hence no
  `phase: run` stdin corpus file — the checked-lane criterion is met
  by `wolf run`.)
- sockets, checked lane: `wolf conform-run --checked
  corpus/net/echo_roundtrip.lu` → `phase_reached: run, exit(0)`,
  expected stdout (`got: ping\nreply: pong`).
- print: `corpus/hello.lu` `phase: run`, plus the three P-project
  witnesses (`corpus/projects/{count,rpn,wordtree}.lu`) all
  `phase: run`. 89 corpus files carry `phase: run` (native).

## Row 3 — Differential shows zero unexplained divergences; every filed family closed or carries its [proto.cmp] ruling

**VERDICT: PENDING(lupin v0.1.7), with an unfiled residue that is RED
until filed**

Run in-worktree: `cargo xtask differ target/debug/wolf …/lupin
--checked --triage` → `254 file(s) — 85 agreement(s), 76 completeness
note(s), 0 SOUNDNESS finding(s), 75 unsupported; 21 hard
divergence(s)`.

- **1 verdict divergence**: `corpus/faults/overflow_elem_write.lu` —
  wolf `Trap("overflow")` vs lupin `Exit(0)`. Filed: wolf-interp #21
  (the s70 X3 litmus wave postdates lupin's tenth differential round
  at pin `13b811f`). Owner: lupin v0.1.7.
- **20 warning-parity divergences** (`warnings [W…] vs []`): the
  s68/s69 lint wave (W03xx/W06xx/W08xx/W10xx/W13xx + E0802-as-warning)
  landed at `e5f7ea8`/`e94b879`, after lupin's last round; lupin emits
  a present-but-empty `warnings` array, so `[proto.cmp.warn]` set
  equality fires (honest-absent per `[proto.record.warn]` would not).
  **Not yet filed as a DIV family** — filing it (or lupin dropping to
  honest-absent / growing the lints) is required; owner: lupin v0.1.7.
- Filed families: DIV-2026-001…010, 013 closed; **DIV-2026-011** (+
  riders 012, 014: rung placement) carries its ruling —
  `[proto.cmp.rung]`, drafted s70 — and closes when lupin adopts it
  (Row 4). DIV-2026-015 realigned per the log. lupin's tenth round:
  "11 divergences, all filed, none a soundness candidate."

## Row 4 — [proto.cmp.rung] adopted by BOTH implementations

**VERDICT: PENDING(lupin v0.1.7)**

- wolfgang: adopted at s70 (`5e9ead6` — "proto.cmp.rung drafted — fail
  parity at any shared rung is agreement"); `xtask/src/protocol.rs
  compare()` compares verdicts and first-diagnostic code+span, never
  `phase_reached`, for fail/fail pairs. Spec clause present
  (spec/06 `[proto.cmp.rung]`).
- lupin: **not adopted** at HEAD `bc1018e` —
  `wolf-interp/src/compare.rs` still returns VerdictMismatch when
  `a.phase_reached != b.phase_reached` before any fail-parity check.

## Row 5 — grammar/1 declared in spec/01

**VERDICT: GREEN (drafted this sprint, in-worktree)**

spec/01 header now declares grammar/1; new §10 "Grammar versioning"
adds `[gram.version]`, `[gram.version.1]` (additive-only until v0.2),
`[gram.version.anchor]` (the anchors are the contract, citing
`[conf.anchor.stable]`), `[gram.version.enforce]` (spec-extract +
CI sync as the enforcement). `cargo xtask spec-extract` regenerated
`spec/anchors.json` (310 anchors, 4 new); `spec/grammar.ebnf`
unchanged (no EBNF was touched).

## Row 6 — Every corpus directive truthful (the ledger IS the release audit)

**VERDICT: GREEN**

In-worktree, post-stamp: `cargo xtask corpus` → `corpus: 254 file(s),
0 bad — phase ledger enforced via conform-run` (phase ledger + warning
ledger + check directives enforced per file). `cargo xtask ci` green
end-to-end, including `differ-self`: `254 file(s), 0 divergence(s),
147 in conservatism ledger (unsupported)`.

## Row 7 — Version stamps and the pairing

**VERDICT: GREEN (wolf side) / PENDING(lupin v0.1.7 + integrator pin)**

- Workspace `Cargo.toml` and `fuzz/Cargo.toml` stamped 0.1.0;
  `wolf --version` prints `wolf 0.1.0 (wolfgang)` (D38) and the
  pairing line `paired with lupin 0.1.7 (reference interpreter),
  pin UNSTAMPED` — `LUPIN_PIN_SHA` in `wolf_driver/src/main.rs` is
  the integrator's stamp at tag time; a tag never ships UNSTAMPED.
- lupin currently prints `lupin 0.1.6 (wolf-interp, pin e94b879)` —
  the pairing shape exists; the 0.1.7 bump and final pin are the
  concurrent lupin agent's.

## Row 8 — Release notes per repo, Tense discipline

**VERDICT: GREEN (wolf side) / PENDING(lupin v0.1.7 for wolf-interp's)**

`docs/release/NOTES-v0.1.0.md` written this sprint: present tense
throughout, debug tier stated as real with corpus-ledger evidence,
release tier attributed to c09 plainly, checked-lane vs native-lane
distinctions kept honest (compiled concurrent execution named as not
built), no promises.

## Row 9 — Synced annotated tags across the six repos + metarepo

**VERDICT: PENDING(integrator)**

No tags exist in any wolffe-lang repo today (verified via `git tag` /
`gh api …/tags`). Tags are the integrator's act, never the agent's
(r01 contract). Blocked behind rows 1, 3, 4, 7, 10 and the s71/lupin
landings; the metarepo pin set is stamped then.

## Row 10 — CI green on every repo at every release sha, all platforms

**VERDICT: RED(integrator: tree-sitter-wolf, metarepo) / GREEN
elsewhere today**

- wolf-lang: `ci success` at trunk `e94b879` (== origin/trunk).
  Release sha will move when this sprint + s71 land — re-verify then.
- wolf-interp: `CI success` at HEAD `bc1018e` (after two red runs
  fixed by the rung-grep repairs).
- wolf-std `ci success` @ `1123428`; wolf-book `book success` @
  `2e67446`; wolf-lsp `ci success` @ `2ae8f63`.
- **tree-sitter-wolf and the metarepo (`wolf`) have no workflows and
  no runs at all** (`gh run list` empty; `.github/workflows` 404) — a
  repo with no CI cannot be green at its release sha. RED until CI
  exists there or a human ruling amends the criterion (no waiving
  here).

---

## Summary

| Row | Verdict |
|---|---|
| 1 soundness issues | RED (#27, #15/S-11 unowned) + pending (lupin #16/#17/#21/#22) |
| 2 io surface | GREEN |
| 3 differ | PENDING (lupin v0.1.7) + RED unfiled lint-parity family |
| 4 proto.cmp.rung | PENDING (lupin v0.1.7) |
| 5 grammar/1 | GREEN (this sprint) |
| 6 corpus ledger | GREEN |
| 7 version stamps | GREEN wolf-side; pin + lupin bump pending |
| 8 release notes | GREEN wolf-side; wolf-interp pending |
| 9 tags | PENDING (integrator) |
| 10 CI | RED (tree-sitter-wolf, metarepo have no CI); others green |
