# s178 — wolf-lang#449, the claim that outlived its arm

Working notes. §2 is re-derived against origin and the published
artifacts; §3 is written here **before** `crates/wolf_mem/src/lower.rs`
is touched, as the contract requires.

## §2 — inputs, re-derived 2026-09-24

Re-derived on **kasumi, linux x86-64**, against origin and against the
published release archives downloaded into `~/lanes/s178/archives`
(sha256 of the tarballs recorded in
`~/lanes/s178/evidence/s2-oracle-published-artifacts.log`):

| tarball | sha256 |
|---|---|
| `wolf-0.2.16-x86_64-unknown-linux-gnu.tar.gz` | `84e30c05e56b89fde2fe6e27eec8f7cec0a6aae6e1464a2993fcfc87a7a1829e` |
| `wolf-0.2.14-x86_64-unknown-linux-gnu.tar.gz` | `13b4de6d0d37fe31a719e4b89033f8e55791e2a99afe6b4c83db686f93c60d05` |
| `lupin-0.1.38-x86_64-unknown-linux-gnu.tar.gz` | `828b5c5571644107a1b123490b5e188da50020676c6b15b0bc53a899a7cc520b` |

**Holds.** `origin/trunk` is `93a5fe504593ca7642b78ba83b4986e7a03cfe71`
and `git rev-parse v0.2.16^{commit}` is the same sha — trunk *is*
v0.2.16. `check_nested_claims_after_mut` is at
`crates/wolf_mem/src/lower.rs:1759`, called once, at
`crates/wolf_mem/src/lower.rs:4670`.

**Holds — the oracle, on the published artifacts, measured not quoted:**

| witness | wolf 0.2.14 native | wolf 0.2.16 native | wolf 0.2.16 checked | lupin 0.1.38 |
|---|---|---|---|---|
| ws37's if/else | `exit(0)`, `2\n` | **`fail(E1002)`, `mem`** | **`fail(E1002)`, `mem`** | `exit(0)`, `2\n` |
| bu07's conditional write | `exit(0)`, `2\n` | **`fail(E1002)`, `mem`** | **`fail(E1002)`, `mem`** | `exit(0)`, `2\n` |
| the match-arms shape | `exit(0)`, `4\n` | **`fail(E1002)`, `mem`** | **`fail(E1002)`, `mem`** | `exit(0)`, `4\n` |

Controls, on 0.2.16 native: the same if/else with the `else` arm
removed is `exit(0)` printing `2`, and the `l2` reorder (`mut` argument
last) is `exit(0)` printing `2`. Both as §2 and the issue state.

**Drift, one, minor and reported rather than absorbed.** The contract
attributes the check to s168 `cec3d53a`. `cec3d53a` is s168's
*diag-catalog regen* (`docs: diag-catalog regen on the rebased tree —
E1002's fixture list takes the s168 witnesses`). The commit that
introduced `check_nested_claims_after_mut` is **`cd5c9b2b`**, `fix(mem):
a nested call may not claim a place an earlier mut argument holds`.
Both are s168's; the function's provenance is `cd5c9b2b`.

**Not drift, but not in the contract either.** §2 says "0.2.16
refuses"; the refusal is on **both** wolfgang lanes — it is in the mem
tier, ahead of lowering — so this is a shared-frontend regression, not
a lane divergence. And in the match shape the arm 0.2.16 flags is the
**first** arm, not the last; the contract's phrasing ("the diagnostic
names a statement that precedes the outer call") holds, and the "which
arm" detail is what pins the mechanism below.

## §3 — prediction, before the edit

**The contract offers two hypotheses and I predict both are wrong.**
There is no claim *set* with a lifetime to pop at a join, and nothing is
keyed on a call. `check_nested_claims_after_mut` re-derives the extent
every time from a *position*, and the position arithmetic is what is
broken.

**The function:** `check_nested_claims_after_mut`,
`crates/wolf_mem/src/lower.rs:1759`.
**The line:** the block-scan bound at
`crates/wolf_mem/src/lower.rs:1766-1772` —

```rust
let start = if bi == from_block {
    from_stmt
} else if bi > from_block {
    0
} else {
    continue;
};
```

**The mechanism: the scan uses block INDEX order as a proxy for program
order, and block index order is not program order once a branch
exists.** `mark` is `(cur_block, stmts.len())` taken before the
argument lowers (`lower.rs:4543`), and the scan then treats *every
block with a higher index* as "after" it. But `eval_if`
(`lower.rs:3783-3795`) allocates `then_block`, then `join`, and only
then — inside the `Some(else_node)` arm — `else_block`; so
`else_block > join`. `eval_match` (`lower.rs:3839-3842`) allocates
`join` first and every `arm_block` after it, so every arm outranks the
join. The outer call lowers *in the join*, which is why the walk reaches
backwards, in program order, into an arm that already finished — and
why it names the **first** match arm. There is no upper bound on the
scan either; it runs to the end of the function.

**This predicts each of the issue's controls without further
measurement:** `m`/`no_else` compiles because with no `else` the only
other block is `then_block < join`; `l5` compiles because the branch is
in a different function and so a different `blocks` vector; `l2`
compiles because with nothing spelled after the `mut` argument
`prior_muts` is empty and the scan never runs; `l1` and `l6` still fail
because neither touches block allocation.

**The fix, one function:** bound the scan to what *this argument's*
evaluation actually emitted. Blocks that already existed when the mark
was taken cannot hold statements this argument emitted — except the
mark's own block, from `from_stmt` on. So `mark` carries
`self.blocks.len()` as well, and the scan admits a block only if it is
the mark's block (skipping `from_stmt`) **or** was created at or after
the mark. A nested branch spelled *inside* an argument still allocates
its blocks after the mark and is still caught; s168's
`take2(mut r.a, wipe(mut r))` emits its `Stmt::Call` into the mark's own
block after `from_stmt` and is still caught.

### What falsifies this

- All **three** witnesses are red at trunk and green after the change —
  anything less than 3/3 falsifies it.
- s168's nine-case `crates/wolf_driver/tests/mut_element_place.rs` stays
  **9/9 green**, its two refusal cases still refusing. One weakened
  refusal falsifies it.
- The change touches **one** function's scan bound and the `mark` tuple
  it reads. If the fix needs a second function, the mechanism above is
  wrong.

## §4 — evidence index

Every row cites a run id or a committed path. Kasumi paths are under
`~/lanes/s178/` and the logs are `evidence/red-at-trunk.log` and
`evidence/green-at-head.log`.

### The red at trunk, per witness

Run: `cargo test --test mut_claim_extent` at `39a9fdf4` — trunk
`93a5fe50` plus the witnesses, `crates/wolf_mem/src/lower.rs`
**untouched** — on kasumi, debug profile, `LUPIN` set to the published
0.1.38 and `WOLF_PAIRING_REQUIRE_SIBLING=1`. Log:
`~/lanes/s178/evidence/red-at-trunk.log`. Result: **3 failed, 5 passed.**

| witness | at trunk `39a9fdf4` | at head |
|---|---|---|
| `both_arms_of_an_if_else_may_claim_the_same_place` | **FAILED** — checked `fail(E1002)`, wanted `exit(0)` | ok |
| `every_arm_of_a_match_may_claim_the_same_place` | **FAILED** — checked `fail(E1002)`, wanted `exit(0)` | ok |
| `a_conditional_write_then_the_shared_writer_runs` | **FAILED** — checked `fail(E1002)`, wanted `exit(0)` | ok |
| `the_same_if_without_an_else_still_runs` (control) | ok | ok |
| `a_claim_inside_a_loop_body_does_not_outlive_the_loop` | ok | ok |
| the three s168 guards | ok | ok |

The corpus gate at the same tree named the same three and not the
control: `corpus/memory/mut_claim_{cond_write,if_else,match_arms}.lu:
header claims phase 'run' but deepest passing phase is 'typecheck'`.
`mut_claim_if_no_else.lu` is absent from that list.

### The function named in §3, and whether the prediction held

Named before the edit, in `6b77f4e0`:
`check_nested_claims_after_mut`, `crates/wolf_mem/src/lower.rs:1759`,
the scan bound at lines 1766–1772.

**Held, on the mechanism and on the guard.** The contract's two offered
hypotheses — a claim set not popped at the join, or keyed on the place
rather than the call — were both wrong, as predicted: there is no
stored claim set. The cause is block-index order standing in for
program order. The fix is the one function and the `Mark` the mark
tuple became; 3 of 3 witnesses went red-to-green; s168's nine cases
stayed 9/9 with both refusals refusing.

**Wrong in one place, published rather than quietly dropped.** I added
a fifth case expecting the mechanism to predict a leak out of a loop
body. It does not: `eval_while` mints `head`, `body`, `exit` and the
code after the loop lowers in `exit`, the highest of the three, so the
body never outranked the cursor. `a_claim_inside_a_loop_body_does_not_
outlive_the_loop` **passed at trunk with the bug in place** and is kept
as a non-regression pin, with its comment corrected to say so.

### The green at head

`~/lanes/s178/evidence/green-at-head.log`, kasumi, both profiles, with
`libwolf_rt.a` built in each:

| | debug | release |
|---|---|---|
| `cargo test --workspace` | exit 0, **2400 passed, 0 failing binaries** | exit 0, **2398 passed, 0 failing binaries** |
| `mut_claim_extent` (this lane's gate) | 8/8 | 8/8 |
| `mut_element_place` (s168's gate) | **9/9** | **9/9** |

`cargo xtask corpus`: **694 files, 0 bad**. `cargo xtask lane-coverage`:
floors held.

One failure on the way was not this change: `conc_native::wolf_test_
schedules_explores_a_native_body` failed under `--release` because
`target/release/libwolf_rt.a` had never been built in this lane
directory (`libwolf_rt.a not found next to the wolf binary`). After
`cargo build --release -p wolf_rt`, 18/18. It is an artifact of running
the release suite in a fresh tree, not of #449.

### Coverage, before and after

`cargo xtask lane-coverage`, trunk `93a5fe50` against head:

| | trunk | head | Δ |
|---|---|---|---|
| non-member corpus entries | 646 | 650 | +4 |
| checked executes at run | 369 | 373 | +4 |
| native executes at run | 422 | 426 | +4 |
| release executes at run | 421 | 425 | +4 |
| UNION | 427 of 646 | 431 of 650 | +4 |
| all three lanes | 364 | 368 | +4 |
| residue `rejected` | 205 | 205 | **0** |
| residue `refused@mem` | 5 | 5 | **0** |
| residue `refused@resolve` | 7 | 7 | **0** |
| residue `refused@wir` | 1 | 1 | **0** |
| residue `forward` | 1 | 1 | **0** |

**Every moved entry, named:** `corpus/memory/mut_claim_if_else.lu`,
`mut_claim_if_no_else.lu`, `mut_claim_match_arms.lu` and
`mut_claim_cond_write.lu`. Each is new and each moves +1 on checked,
native, release, union and all-three. **No pre-existing entry changed
bucket** — nothing left `rejected`, nothing left a residue. That is the
statement that matters for a fix whose risk is widening an acceptance:
the fix accepted exactly the four programs it was written to accept and
nothing else in 646 files.

The `wolf_wir` lowering ledger records the same independently: its
snapshot gained exactly four lines, all `lowers`
(`crates/wolf_wir/tests/snapshots/lower_corpus__corpus_lowering_ledger.snap`).
