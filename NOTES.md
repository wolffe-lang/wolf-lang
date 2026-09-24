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
