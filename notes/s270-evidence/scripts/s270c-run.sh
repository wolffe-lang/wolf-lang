set -eu
# usage: ssh hasu bash -s -- PHASE INNER < s270c-run.sh
PHASE=$1
INNER=$2
base=/home/mfwolffe/lanes/s270
log=$base/logs/s270c-$PHASE.log
mark=$base/records/s270c-$PHASE.exit
rm -f "$mark"
set +e
bash "$base/scripts/s270-container.sh" "c-$PHASE" "/lane/scripts/$INNER" > "$log" 2>&1
rc=$?
set -e
echo "$rc" > "$mark"
cat "$log"
echo "CONTAINER_EXIT $rc"
