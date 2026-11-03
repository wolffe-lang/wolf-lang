#!/usr/bin/env bash
# s277: one chunk of the gauntlet at the final head, inside r41-toolchain as
# the lane uid (one lock take each, at most 2 h). Every line it prints is
# inside a `START`/`END <step> exit=N secs=S` pair (the org's
# gauntlet recorder, sourced, is the step function), and it stops at the
# first nonzero step. The CI steps are `cargo xtask ci`'s own, read from
# CI_STEPS in xtask/src/main.rs AT THIS HEAD and run in its order with its
# arguments (s272's gauntlet-inner, copied); prep builds the inputs the
# test step needs, each read twice (s272's prep-inner, copied).
# usage: gauntlet-chunk.sh <prep|lint|test|rest> <expected-sha>
set -uo pipefail
source /lane/rig/gate-env.sh
source /lane/rig/gauntlet-record.sh
chunk=${1:?chunk} expected=${2:?sha}
cd /lane/wolf-lang || exit 2
head_ok() {
  test "$(git rev-parse HEAD)" = "$expected" && test -z "$(git status --porcelain --untracked-files=no)" &&
    git rev-parse HEAD && grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status &&
    printf 'pid1 uid=%s self uid=%s block=%s tests on %s\n' "$(awk '/^Uid:/{print $2}' /proc/1/status)" "$(id -u)" "$S277_BLOCK" "$WOLF_TEST_CPUS" &&
    test "$(awk '/^Uid:/{print $2}' /proc/1/status)" != "$(id -u)"
}
gauntlet_step "$chunk-head" head_ok || exit $?
inputs() {
  local input pass
  for input in \
    '34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193  /inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz' \
    '4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a  /inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz'; do
    for pass in 1 2; do printf '%s\n' "$input" | sha256sum -c - || return 1; done
  done
  "$LUPIN" --version
}
std_pin() {
  local pin pass
  pin=$(sed -n 's/^wolf-std-commit = \([0-9a-f]\{40\}\)$/\1/p' crates/wolf_driver/STD-PIN)
  test -n "$pin" || return 1
  if ! test -d /lane/wolf-std/.git; then git clone -q https://github.com/wolffe-lang/wolf-std.git /lane/wolf-std || return 1; fi
  git -C /lane/wolf-std checkout -q --detach "$pin" || return 1
  for pass in 1 2; do test "$(git -C /lane/wolf-std rev-parse HEAD)" = "$pin" || return 1; git -C /lane/wolf-std rev-parse HEAD; done
}
dist_rt() {
  local first second
  cargo xtask dist-rt --target x86_64-unknown-linux-musl || return 1
  first=$(sha256sum "$WOLF_STATIC_ENTRY_MUSL_RT"); second=$(sha256sum "$WOLF_STATIC_ENTRY_MUSL_RT")
  printf '%s\n%s\n' "$first" "$second"
  test "$first" = "$second"
}
ligature() {
  WOLF=/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz/unpacked/wolf-0.2.35-x86_64-unknown-linux-gnu/wolf ci/build-test-ligature
}
ci_steps() {
  python3 - <<'PY'
import re
s = open("xtask/src/main.rs").read()
i = s.index("const CI_STEPS")
j = s.index("];", i)
blk = re.sub(r"//[^\n]*", "", s[i:j])
for m in re.finditer(r'\(\s*"([^"]+)"\s*,\s*&\[(.*?)\]\s*,?\s*\)', blk, re.S):
    print(m.group(1) + "\t" + "\x1f".join(re.findall(r'"([^"]*)"', m.group(2))))
PY
}
run_ci() { # the CI steps from $1 (inclusive) up to $2 (exclusive; "" = the end)
  local from=$1 to=$2 on=0 name args
  local steps
  steps=$(ci_steps) || return 1
  while IFS=$'\t' read -r name args; do
    [ "$name" = "$from" ] && on=1
    [ -n "$to" ] && [ "$name" = "$to" ] && break
    [ "$on" = 1 ] || continue
    IFS=$'\x1f' read -r -a argv <<< "$args"
    gauntlet_step "$name" cargo "${argv[@]}" || return $?
  done <<< "$steps"
}
case $chunk in
  prep)
    gauntlet_step prep-inputs inputs || exit $?
    gauntlet_step prep-std-pin std_pin || exit $?
    gauntlet_step prep-tools bash -c 'command -v clang ld.lld strace cc && clang --version && ld.lld --version && rustup target list --installed' || exit $?
    gauntlet_step prep-build cargo build --locked -p wolf_driver -p wolf_rt || exit $?
    gauntlet_step prep-entry-bundle ci/fetch-entry-bundle || exit $?
    gauntlet_step prep-dist-rt dist_rt || exit $?
    gauntlet_step prep-link-bundle ci/fetch-test-link-bundle || exit $?
    gauntlet_step prep-test-clef ci/build-test-clef || exit $?
    gauntlet_step prep-test-ligature ligature || exit $?
    gauntlet_step ci-plan bash -c 'cargo xtask ci --plan 2>&1 | tail -3' || exit $?
    ;;
  lint) run_ci fmt test || exit $? ;;
  test) run_ci test fuzz-check || exit $? ;;
  rest)
    run_ci fuzz-check "" || exit $?
    # The orchestrator's ask (2026-11-03, tl35's routing): every ci/*-test.py.
    for t in ci/*-test.py; do
      gauntlet_step "ci-$(basename "$t" .py)" python3 "$t" || exit $?
    done
    ;;
  *) exit 2 ;;
esac
gauntlet_step "$chunk-clean" bash -c 'git diff --exit-code --stat && test -z "$(git status --porcelain --untracked-files=no)"' || exit $?
exit 0
