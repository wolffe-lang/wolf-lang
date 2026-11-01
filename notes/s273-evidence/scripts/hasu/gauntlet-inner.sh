#!/usr/bin/env bash
# s273: the gauntlet at the final head. `cargo xtask ci`'s own steps, read from
# CI_STEPS in xtask/src/main.rs AT THIS HEAD and run in its order with its
# arguments, one `END <step> exit=N` line each, stopping at the first nonzero
# step (the-lane-rules-1031.md); then `cargo xtask ci --plan` for the step count.
# The exit code is the first red step's, or 0 when all are green.
set -uo pipefail
source /lane/scripts/gate-env.sh
cd /lane/wolf-lang
expected=${1:?expected final head}
actual=$(git rev-parse HEAD)
test "$actual" = "$expected" || { echo "S273_REFUSED head $actual != $expected"; exit 2; }
git diff --exit-code >/dev/null || { echo "S273_REFUSED dirty tree"; exit 2; }
git diff --cached --exit-code >/dev/null || { echo "S273_REFUSED dirty index"; exit 2; }
printf 'S273_GAUNTLET_START %s head=%s tree=%s\n' "$(date -u +%FT%TZ)" "$actual" "$(git rev-parse 'HEAD^{tree}')"
steps=$(python3 - <<'PY'
import re
s = open("xtask/src/main.rs").read()
i = s.index("const CI_STEPS")
j = s.index("];", i)
blk = re.sub(r"//[^\n]*", "", s[i:j])
for m in re.finditer(r'\(\s*"([^"]+)"\s*,\s*&\[(.*?)\]\s*,?\s*\)', blk, re.S):
    print(m.group(1) + "\t" + "\x1f".join(re.findall(r'"([^"]*)"', m.group(2))))
PY
)
n=$(printf '%s\n' "$steps" | grep -c .)
printf 'S273_GAUNTLET_STEPS %s\n' "$n"
cargo xtask ci --plan 2>&1 | tail -1
while IFS=$'\t' read -r name args; do
  IFS=$'\x1f' read -r -a argv <<< "$args"
  printf '== %s %s: cargo %s\n' "$name" "$(date -u +%FT%TZ)" "${argv[*]}"
  cargo "${argv[@]}"
  rc=$?
  printf 'END %s exit=%s %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
  if [ "$rc" -ne 0 ]; then
    printf 'S273_GAUNTLET_RED %s head=%s step=%s exit=%s\n' "$(date -u +%FT%TZ)" "$actual" "$name" "$rc"
    exit "$rc"
  fi
done <<< "$steps"
printf 'S273_GAUNTLET_GREEN %s head=%s steps=%s\n' "$(date -u +%FT%TZ)" "$actual" "$n"
