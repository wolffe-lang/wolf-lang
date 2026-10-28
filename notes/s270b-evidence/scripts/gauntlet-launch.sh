set -eu
# s270b: launch the gauntlet detached (run as: ssh hasu bash -s < gauntlet-launch.sh)
base=/home/mfwolffe/lanes/s270b
test ! -e "$base/records/gauntlet.pid" || { echo "a gauntlet pid file exists: $(cat "$base/records/gauntlet.pid")"; exit 3; }
setsid -f bash "$base/scripts/gauntlet-job.sh" > "$base/logs/gauntlet.log" 2>&1 < /dev/null
for i in 1 2 3 4 5 6 7 8 9 10; do
  test -s "$base/records/gauntlet.pid" && break
  read -r -t 1 < /dev/zero || true
done
pid=$(cat "$base/records/gauntlet.pid")
echo "pid $pid"
ps -o pid,pgid,sid,stat,etime,cmd -p "$pid"
head -n 3 "$base/logs/gauntlet.log"
