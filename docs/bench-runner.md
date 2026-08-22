# The dedicated bench runner — the human's checklist

The M2 gate is decided by `cargo xtask bench ritual` running nightly on
a machine that does nothing else (`.github/workflows/m2-ritual.yml`,
`runs-on: [self-hosted, bench]`). Nine hand-runs (#89, 2026-08-20 → -22)
established the protocol the ritual codifies; this page is the one part
no tool can do: plugging in the hardware.

The ritual **refuses** on a busy machine — load ≥ 1.0 after a bounded
wait, or any foreign cargo/rustc/clang process, is a named failure, not
a measurement. So a mislabeled shared runner fails loudly. Do not fight
that; it is the design.

## 1. Pick the machine

- Idle by construction: nothing else scheduled — no desktop session, no
  other CI, no cron builds. The quiet check will hold you to this.
- Any x86-64 Linux box is legitimate: the gate is a **ratio against
  naive clang -O3 on the same machine**, so absolute speed does not
  matter. What matters is that it is the *same* box every night — the
  layout-noise floors only mean something across runs on one host.
- The nine hand-runs ran on an i7-10870H under `powersave`. The
  governor's *choice* matters less than its *consistency*; pin one and
  the conditions file will name it every night:

  ```sh
  # pick one and make it stick (cpupower survives reboots via its service)
  sudo cpupower frequency-set -g performance
  ```

- Leave SMT and ASLR as the distribution ships them — the ritual
  records both; changing them mid-series is what invalidates a series.

## 2. Install the toolchain the lanes need

```sh
sudo apt-get install -y time util-linux clang llvm lld \
    linux-tools-common "linux-tools-$(uname -r)"
# rust via rustup (stable); the workflow uses the checkout's pin
```

`llvm-profdata` (in `llvm`) matters: all nine hand-runs skipped the PGO
scrutiny lane for want of it. The runner unskipping that lane is its
first improvement over the hand ritual, and the conditions file will
flip from "ABSENT" to "present" the first night.

## 3. Register the runner with the `bench` label

```sh
# a registration token (expires quickly; mint it right before use):
gh api -X POST repos/wolffe-lang/wolf-lang/actions/runners/registration-token --jq .token

mkdir -p ~/actions-runner && cd ~/actions-runner
# the runner's release assets are VERSIONED — no unversioned alias exists
# (found the hard way registering wolf-bench-i7: the alias URL serves an
# error page that tar rejects). Resolve the real asset:
URL=$(gh api repos/actions/runner/releases/latest \
  --jq '.assets[] | select(.name | test("^actions-runner-linux-x64-[0-9.]+\\.tar\\.gz$")) | .browser_download_url')
curl -o runner.tar.gz -L "$URL"
tar xzf runner.tar.gz
./config.sh --url https://github.com/wolffe-lang/wolf-lang \
    --token <TOKEN-FROM-ABOVE> \
    --labels bench \
    --unattended
sudo ./svc.sh install && sudo ./svc.sh start
```

The `bench` label is the contract: `m2-ritual.yml` targets
`[self-hosted, bench]` with **no hosted fallback**, so the job queues
until a machine wearing the label exists, and never silently runs on
shared iron.

## 4. What happens nightly, and what to read

At 02:47 UTC the workflow runs the ritual: quiet check → generated
conditions file → the t1 suite (10 runs/kernel) → the gate → a line
appended to `bench/ritual-ledger.jsonl` (pushed to trunk as a bot
commit) → artifacts archived to the `bench-data` branch.

- **The ledger is the s44 clock.** Its header states the tick rule: a
  HOLDS advances the count only ≥ 12 h after the previously counted
  hold; a DOES-NOT-HOLD resets it. At three consecutive holds the run
  log prints the declaration-threshold banner — the declaration itself
  is still yours to announce.
- A **red job** means the ritual could not be performed honestly (busy
  machine, dirty tree, suite failure) — read the refusal, it names
  itself. A **refuted gate is a green job**: the verdict is data, in
  the ledger and the step summary.
- First nights on a new box: expect the numbers' *texture* to differ
  from the hand-runs (different host, PGO lane now live). The gate is
  ratio-based so the verdict is comparable; the per-kernel absolute
  ns/op are not, and nothing reads them across hosts.
