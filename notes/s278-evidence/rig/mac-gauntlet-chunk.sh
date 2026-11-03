#!/bin/bash
# s278: one chunk of the macOS record at the final head, run behind the Mac
# lock (one take per chunk, at most 2 h) in the build worktree. Every line it
# prints is inside a `START`/`END <step> exit=N secs=S` pair (the org's
# gauntlet recorder, sourced, is the step function), each step's own output
# indented two spaces, and it stops at the first nonzero step.
#   rows   the build shard's steps that read the changed files: fmt,
#          fmt-fixpoint, the corpus snapshots of wolf_lex, wolf_parse and
#          wolf_wir, the two os_surface_lanes rows, clippy on that test;
#   gates  the gates shard of `cargo xtask ci` (the macOS job
#          `native gauntlet (macOS aarch64, gates)`), read from CI_STEPS and
#          CI_SHARDS in xtask/src/main.rs AT THIS HEAD, run in its order with
#          its arguments.
# The corpus step's own refusal lines (`corpus: <file>: …`) are repeated at
# column 0 as `FAIL <file> …`, so a red row is named in the record.
# usage: mac-gauntlet-chunk.sh <rows|gates> <expected-sha>
set -u
exec 2>&1
L=/Users/mfwolffe/lanes/s278
source "$L/rig/gauntlet-record.sh"
chunk=${1:?chunk} expected=${2:?sha}
export LUPIN=/Users/mfwolffe/.local/bin/lupin
export WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1 WOLF_PAIRING_REQUIRE_SIBLING=1
indent() { "$@" 2>&1 | sed 's/^/  /'; return "${PIPESTATUS[0]}"; }
head_ok() {
  test "$(git rev-parse HEAD)" = "$expected" || { echo "head is not $expected"; return 1; }
  test -z "$(git status --porcelain --untracked-files=no)" || { echo "tracked checkout is dirty"; return 1; }
  git rev-parse HEAD && git log --oneline -1 && sw_vers && uname -a && rustc -V && cargo -V &&
    cc --version | head -1 && "$LUPIN" --version && id &&
    ps -o pid,sig,sigmask -p $$ && df -g /Users/mfwolffe | tail -1 &&
    echo "CARGO_INCREMENTAL=$CARGO_INCREMENTAL CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS TMPDIR=$TMPDIR CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-<unset>}"
}
gauntlet_step "$chunk-head" indent head_ok || exit $?
corpus_step() {
  local log rc
  log=$(mktemp "$TMPDIR/s278-corpus.XXXXXX") || return 1
  cargo xtask corpus > "$log" 2>&1; rc=$?
  sed 's/^/  /' "$log"
  grep -E '^corpus: [^ ]+\.lu: ' "$log" | sed -E 's/^corpus: ([^ ]+\.lu): (.*)$/FAIL \1 \2/'
  rm -f "$log"
  return $rc
}
ci_steps() {
  python3 - <<'PY'
import re
s = open("xtask/src/main.rs").read()
def block(name):
    i = s.index("const " + name)
    j = s.index("\n];", i)
    return re.sub(r"//[^\n]*", "", s[i:j])
steps = {}
order = []
for m in re.finditer(r'\(\s*"([^"]+)"\s*,\s*&\[(.*?)\]\s*,?\s*\)', block("CI_STEPS"), re.S):
    steps[m.group(1)] = re.findall(r'"([^"]*)"', m.group(2))
    order.append(m.group(1))
shards = block("CI_SHARDS")
m = re.search(r'\(\s*"gates"\s*,\s*&\[(.*?)\]\s*,?\s*\)', shards, re.S)
gates = re.findall(r'"([^"]*)"', m.group(1))
for name in order:
    if name in gates:
        print(name + "\t" + "\x1f".join(steps[name]))
PY
}
run_gates() {
  local steps name args n=0
  steps=$(ci_steps) || return 1
  while IFS=$'\t' read -r name args; do
    n=$((n + 1))
    if [ "$name" = corpus ]; then
      gauntlet_step corpus corpus_step || return $?
      continue
    fi
    IFS=$'\x1f' read -r -a argv <<< "$args"
    gauntlet_step "$name" indent cargo "${argv[@]}" || return $?
  done <<< "$steps"
  [ "$n" -eq 15 ] || { printf 'START gates-count\n  the gates shard has %s steps here, not 15\nEND gates-count exit=1 secs=0\n' "$n"; return 1; }
}
case $chunk in
  rows)
    gauntlet_step fmt indent cargo fmt --all --check || exit $?
    gauntlet_step fmt-fixpoint indent cargo xtask fmt-fixpoint || exit $?
    gauntlet_step snap-lex indent cargo test --locked -p wolf_lex --test corpus_snapshots || exit $?
    gauntlet_step snap-parse indent cargo test --locked -p wolf_parse --test corpus_decls || exit $?
    gauntlet_step snap-wir indent cargo test --locked -p wolf_wir --test lower_corpus || exit $?
    gauntlet_step os-surface-rows indent cargo test --locked -p wolf_driver --test os_surface_lanes -- --nocapture --test-threads=1 the_root_rows_on_every_host the_signal_action_rows_on_every_host || exit $?
    gauntlet_step clippy-os-surface indent cargo clippy --locked -p wolf_driver --test os_surface_lanes -- -D warnings || exit $?
    ;;
  gates) run_gates || exit $? ;;
  plant)
    gauntlet_step plant-build indent cargo build --locked -p wolf_driver -p wolf_rt || exit $?
    gauntlet_step plant-corpus corpus_step || exit $?
    ;;
  *) exit 2 ;;
esac
gauntlet_step "$chunk-clean" indent bash -c 'git diff --exit-code --stat && test -z "$(git status --porcelain --untracked-files=no)"' || exit $?
exit 0
