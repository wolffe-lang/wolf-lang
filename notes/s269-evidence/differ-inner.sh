set -uo pipefail
lane=/home/mfwolffe/lanes/s269
export WOLF_STD="$lane/wolf-std/std"
result=0
for tier in checked native release; do
    unset WOLF_CHECKED WOLF_NATIVE WOLF_RELEASE
    case "$tier" in
        checked) export WOLF_CHECKED=1; flag=--checked ;;
        native) export WOLF_NATIVE=1; flag=--native ;;
        release) export WOLF_RELEASE=1; flag=--native ;;
    esac
    printf 's269 differ %s start %s\n' "$tier" "$(date -u +%FT%TZ)"
    cargo xtask differ "$lane/candidate/wolf" "$lane/baseline/wolf" "$flag" > "$lane/evidence/differ-$tier.log" 2>&1
    status=$?
    printf '%s\n' "$status" > "$lane/evidence/differ-$tier.exit"
    printf 's269 differ %s exit=%s %s\n' "$tier" "$status" "$(date -u +%FT%TZ)"
    if [ "$status" -ne 0 ]; then result=1; fi
done
exit "$result"
