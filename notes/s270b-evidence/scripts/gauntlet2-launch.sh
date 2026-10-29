set -eu
# s270b: move the clone to the wanted head, then launch the gauntlet detached
# (run as: ssh hasu bash -s -- <sha> < gauntlet2-launch.sh)
base=/home/mfwolffe/lanes/s270b
want=$1
bash "$base/scripts/sync-clone.sh" "$want"
mv -f "$base/records/gauntlet.pid" "$base/records/gauntlet-822b236c.pid" 2>/dev/null || true
mv -f "$base/records/gauntlet.exit" "$base/records/gauntlet-822b236c.exit" 2>/dev/null || true
mv -f "$base/logs/gauntlet.log" "$base/logs/gauntlet-822b236c.log" 2>/dev/null || true
setsid -f bash "$base/scripts/gauntlet-job.sh" > "$base/logs/gauntlet.log" 2>&1 < /dev/null
for i in 1 2 3 4 5 6 7 8 9 10; do
  test -s "$base/records/gauntlet.pid" && break
  read -r -t 1 < /dev/zero || true
done
pid=$(cat "$base/records/gauntlet.pid")
echo "pid $pid"
ps -o pid,pgid,sid,stat,etime,cmd -p "$pid"
head -n 3 "$base/logs/gauntlet.log"
