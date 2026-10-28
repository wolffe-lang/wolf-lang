set -eu
# s270b: launch the measurement chain detached (run as: ssh hasu bash -s < chain-launch.sh)
base=/home/mfwolffe/lanes/s270b
test ! -e "$base/records/chain.pid" || { echo "a chain pid file exists: $(cat "$base/records/chain.pid")"; exit 3; }
setsid -f bash "$base/scripts/chain-job.sh" > "$base/logs/chain.log" 2>&1 < /dev/null
for i in 1 2 3 4 5 6 7 8 9 10; do
  test -s "$base/records/chain.pid" && break
  read -r -t 1 < /dev/zero || true
done
pid=$(cat "$base/records/chain.pid")
echo "pid $pid"
ps -o pid,pgid,sid,stat,etime,cmd -p "$pid"
head -n 3 "$base/logs/chain.log"
