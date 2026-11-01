#!/bin/bash
# s275: one turn on the Mac build lock (lane rules): wait for the lock
# directory (a dead holder's lock is removed and retried at once), write
# pid and lane, the 8 GiB gate (free + inactive + speculative pages x 16384),
# then the steps at <sha> in ~/lanes/s275/build (a worktree pinned to that
# commit, never the live tree), each with END <step> exit=N, and the lock
# removed on the way out. Takes the lock once per call.
# usage: mac-run.sh <sha> <step-file>   (step-file: lines "name<TAB>command")
set -u
L=/Users/mfwolffe/lanes/s275
LOCK=/Users/mfwolffe/lanes/mac-build.lock
sha=$(git -C "$L/wolf-lang" rev-parse "${1:?sha}") || exit 2
steps=${2:?step file}
B=$L/build
[ -d "$B" ] || git -C "$L/wolf-lang" worktree add -q --detach "$B" "$sha" || exit 2
git -C "$B" checkout -q --detach "$sha" || exit 2
test "$(git -C "$B" rev-parse HEAD)" = "$sha" || { echo "REFUSED head"; exit 2; }
git -C "$B" diff --quiet || { echo "REFUSED dirty"; exit 2; }
until mkdir "$LOCK" 2>/dev/null; do
    holder=$(cat "$LOCK/pid" 2>/dev/null)
    if [ -n "$holder" ] && ! ps -p "$holder" >/dev/null 2>&1; then
        echo "the lock's holder $holder is gone; removing it $(date +%T)"
        rm -rf "$LOCK"
        continue
    fi
    echo "waiting for the Mac lock (holder $(cat "$LOCK/lane" 2>/dev/null) pid $holder) $(date +%T)"
    sleep 30
done
echo $$ >"$LOCK/pid"
echo s275 >"$LOCK/lane"
trap 'rm -rf "$LOCK"; echo "released the Mac lock $(date +%T)"' EXIT
echo "took the Mac lock $(date +%T), at $sha"
while :; do
    bytes=$(vm_stat | awk '/Pages free/ {f=$3} /Pages inactive/ {i=$3} /Pages speculative/ {s=$3} END {print (f+i+s)*16384}')
    [ "$bytes" -ge $((8 * 1024 * 1024 * 1024)) ] && break
    echo "8 GiB gate: $((bytes / 1048576)) MiB free+inactive+speculative; waiting $(date +%T)"
    sleep 30
done
echo "8 GiB gate passed: $((bytes / 1048576)) MiB $(date +%T)"
cd "$B" || exit 2
export CARGO_INCREMENTAL=0
while IFS=$'\t' read -r name cmd; do
    [ -z "$name" ] && continue
    echo "BEGIN $name $(date +%T)"
    bash -c "$cmd"
    echo "END $name exit=$? $(date +%T)"
done <"$steps"
