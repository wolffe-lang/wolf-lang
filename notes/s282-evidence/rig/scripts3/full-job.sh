#!/usr/bin/env bash
# The whole s280 gauntlet at one head on H1, in four takes of at most 2 h
# (prep, build, gates, coverage) on one block, each its own lock take,
# aggregated into one record.
#   full-job.sh TAG EXPECTED WORK BLOCK
set -uo pipefail
base=/home/mfwolffe/lanes/s282
tag=$1; expected=$2; work=$3; block=$4
short=${expected:0:8}
label=full-$tag-$short
marker=$base/records/$label.exit
export TMPDIR=$base/tmp
printf '%s\n' "$$" > "$base/records/$label.pid"
child=
on_signal() { printf '%s\n' "$1" > "$marker"; printf 'FULL_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
printf 'FULL_START %s head=%s host=H1 block=%s kernel=%s\n' "$(date -u +%FT%TZ)" "$expected" "$block" "$(uname -r)"
rc=0
records=()
for shard in prep build gates coverage; do
  take=$tag-$short-$shard
  bash "$base/scripts3/gauntlet-take.sh" run "$block" "$take" "$work" "$expected" "$shard" &
  child=$!
  wait "$child"
  rc=$?
  child=
  if [ "$rc" -ne 0 ]; then break; fi
  records+=("$base/records/gauntlet-$expected-$take.record")
done
if [ "$rc" -eq 0 ]; then
  python3 -I "$base/scripts3/aggregate-records.py" "$base/$work" "$expected" "$base/scripts3/gauntlet-record.sh" "$base/records/gauntlet-$expected-$tag.record" "${records[@]}"
  rc=$?
fi
printf 'FULL_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
exit "$rc"
