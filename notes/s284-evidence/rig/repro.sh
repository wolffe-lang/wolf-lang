#!/usr/bin/env bash
# s284 job "repro": interleaved runs of drain.lu under stated conditions, on
# four of the block's CPUs, under launch.sh's three locks. The plan file has
# one "<cond> <binary>" pair a line; every round runs each pair once, the
# order rotated by one each round, until <rounds> rounds or <budget> minutes
# (no new round starts past the budget; one lock take stays under 2 h).
# Each run is run1.sh's (its own throwaway container). Prints every RUN
# line, the host's load each round, and a SUMMARY line per pair.
# usage: repro.sh <A|B> <plan> <rounds> <budget-minutes> <label>
set -uo pipefail
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
plan=${2:?plan}
rounds=${3:?rounds}
budget=${4:?budget minutes}
label=${5:?label}
case $blk in A) FOUR=0-3 ;; B) FOUR=6-9 ;; *) echo "block must be A or B"; exit 2 ;; esac
D=$L/runs/$label
mkdir -p "$D"
mapfile -t pairs < <(grep -v '^[[:space:]]*\(#\|$\)' "$plan")
n=${#pairs[@]}
[ "$n" -gt 0 ] || { echo "empty plan"; exit 2; }
for pr in "${pairs[@]}"; do
    set -- $pr
    [ -x "$2" ] || { echo "REFUSED: no binary $2"; exit 2; }
done
end=$(($(date +%s) + budget * 60))
echo "LOCKS_ACQUIRED $(date -u +%FT%TZ) block=$blk four=$FOUR label=$label pairs=$n rounds=$rounds budget=${budget}m kernel=$(uname -r)"
cp "$plan" "$D/plan.txt"
for pr in "${pairs[@]}"; do set -- $pr; echo "PAIR $1 $2 sha256=$(sha256sum "$2" | cut -c1-16)"; done
declare -a tot=() grn=() stl=() red=() oth=()
for i in $(seq 0 $((n - 1))); do tot[$i]=0; grn[$i]=0; stl[$i]=0; red[$i]=0; oth[$i]=0; done
r=0
while [ "$r" -lt "$rounds" ] && [ "$(date +%s)" -lt "$end" ]; do
    r=$((r + 1))
    echo "ROUND $r $(date -u +%FT%TZ) load=$(cut -d' ' -f1-3 /proc/loadavg)"
    for k in $(seq 0 $((n - 1))); do
        i=$(((k + r) % n))
        set -- ${pairs[$i]}
        line=$(bash "$L/rig/run1.sh" "$1" "$2" "$FOUR" "$D" "$label-r$r-p$i")
        echo "$line"
        tot[$i]=$((tot[$i] + 1))
        case $line in
        *" rc=0 "*) grn[$i]=$((grn[$i] + 1)) ;;
        *" rc=124 "*) stl[$i]=$((stl[$i] + 1)) ;;
        *" rc=1 "*) red[$i]=$((red[$i] + 1)) ;;
        *) oth[$i]=$((oth[$i] + 1)) ;;
        esac
    done
done
for i in $(seq 0 $((n - 1))); do
    set -- ${pairs[$i]}
    echo "SUMMARY $label cond=$1 bin=$(basename "$2") runs=${tot[$i]} green=${grn[$i]} stalled=${stl[$i]} red=${red[$i]} other=${oth[$i]}"
done
echo "REPRO_END $label rounds=$r $(date -u +%FT%TZ)"
