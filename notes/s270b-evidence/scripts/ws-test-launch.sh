set -eu
# s270b: launch the workspace test run detached (ssh hasu bash -s < ws-test-launch.sh)
base=/home/mfwolffe/lanes/s270b
rm -f "$base/wolf-lang/crates/wolf_lex/tests/snapshots/corpus_snapshots__memory__region_outer_write_push_copy_call.snap.new"
git -C "$base/wolf-lang" status --short
test ! -e "$base/records/ws-test.pid" || { echo "a ws-test pid file exists"; exit 3; }
setsid -f bash "$base/scripts/job.sh" ws-test /lane/scripts/ws-test-inner.sh > "$base/logs/ws-test-job.log" 2>&1 < /dev/null
for i in 1 2 3 4 5 6 7 8 9 10; do
  test -s "$base/records/ws-test.pid" && break
  read -r -t 1 < /dev/zero || true
done
echo "pid $(cat "$base/records/ws-test.pid")"
head -n 2 "$base/logs/ws-test-job.log"
