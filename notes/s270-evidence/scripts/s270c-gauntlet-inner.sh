#!/usr/bin/env bash
set -uo pipefail
source /lane/scripts/s270c-gauntlet-env.sh
cd /lane/wolf-lang
printf 's270c GAUNTLET START %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git status --short
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
printf 'pid1 uid: '; awk '/^Uid:/{print $2}' /proc/1/status
printf 'self uid: '; id -u
rustc --version
rustup target list --installed
command -v strace
step() {
 local name=$1; shift
 printf '\n== STEP %s %s\n' "$name" "$(date -u +%FT%TZ)"
 "$@"
 local rc=$?
 printf '== STEP %s EXIT %s %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
 echo "$rc" > "/lane/records/s270c-gauntlet-step-$name.exit"
 return "$rc"
}
step build cargo build --locked -p wolf_driver -p wolf_rt || exit 10
step dist-rt cargo xtask dist-rt --target x86_64-unknown-linux-musl || exit 11
sha256sum "$WOLF_STATIC_ENTRY_MUSL_RT" "$WOLF_STATIC_ENTRY_MUSL_RT"
step clef ci/build-test-clef || exit 12
step ligature env WOLF=/lane/inputs/wolf/wolf-0.2.35-x86_64-unknown-linux-gnu/wolf ci/build-test-ligature || exit 13
step xtask-ci cargo xtask ci
rc=$?
printf '\nXTASK_CI_EXIT %s\n' "$rc"
git status --short
git diff --exit-code
printf 's270c GAUNTLET END %s exit=%s\n' "$(date -u +%FT%TZ)" "$rc"
exit "$rc"
