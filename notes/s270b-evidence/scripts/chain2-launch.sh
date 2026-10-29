set -eu
# s270b: launch chain 2 detached (run as: ssh hasu bash -s < chain2-launch.sh)
base=/home/mfwolffe/lanes/s270b
printf '%s\n' 822b236cc2c05b8f859e6d5a989f53ce60311ad0 > "$base/records/gauntlet-head.txt"
test ! -e "$base/records/chain2.pid" || { echo "a chain2 pid file exists: $(cat "$base/records/chain2.pid")"; exit 3; }
setsid -f bash "$base/scripts/chain2-job.sh" > "$base/logs/chain2.log" 2>&1 < /dev/null
for i in 1 2 3 4 5 6 7 8 9 10; do
  test -s "$base/records/chain2.pid" && break
  read -r -t 1 < /dev/zero || true
done
pid=$(cat "$base/records/chain2.pid")
echo "pid $pid"
ps -o pid,pgid,sid,stat,etime,cmd -p "$pid"
head -n 3 "$base/logs/chain2.log"
