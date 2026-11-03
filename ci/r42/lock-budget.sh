#!/usr/bin/env bash
# One deadline begins AFTER outer acquisition and BEFORE the single inner wait.
# plan/work modes are pure arithmetic checks; they never submit a workload.
set -uo pipefail
mode=${1:?outer, inner, plan or work}
shift
case "$mode" in
    plan|work)
        now=${1:?current epoch}; deadline=${2:?outer-held deadline}
        [[ "$now" =~ ^[0-9]+$ && "$deadline" =~ ^[0-9]+$ ]] || exit 2
        remaining=$((10#$deadline - 10#$now))
        if (( remaining <= 120 )); then
            echo "Insufficient outer-held budget: remaining=${remaining}s" >&2
            exit 124
        fi
        if [[ "$mode" = work ]]; then
            printf '%s\n' "$((remaining - 90))"
        else
            printf '%s %s %s\n' "$((remaining - 30))" "$((remaining - 60))" "$((remaining - 90))"
        fi
        ;;
    outer)
        inner_lock=${1:?inner lock}; shift
        export R42_LOCK_DEADLINE=$(( $(date +%s) + 7000 ))
        echo "R42_OUTER_ACQUIRED deadline=$R42_LOCK_DEADLINE $(date -u +%FT%TZ)"
        # This timeout includes the inner wait. --close prevents workload
        # descendants from retaining either parent's flock descriptor.
        # Foreground mode escalates only its direct child, never a group.
        exec timeout --foreground --kill-after=10 7000 \
            flock -w 21600 --close "$inner_lock" bash "$0" inner "$@"
        ;;
    inner)
        podman=${1:?podman executable}; shift
        plan=$(bash "$0" plan "$(date +%s)" "${R42_LOCK_DEADLINE:?}") || exit $?
        read -r cli_budget container_budget work_budget <<< "$plan"
        echo "R42_INNER_ACQUIRED cli=${cli_budget}s container=${container_budget}s deadline=$R42_LOCK_DEADLINE"
        # Native conmon lifetime enforcement does not rely on a responsive
        # CLI. The bounded CLI also has hard escalation for hung teardown.
        exec timeout --foreground --kill-after=10 "$cli_budget" \
            "$podman" run --timeout "$container_budget" "$@"
        ;;
    *) echo "Unknown lock-budget mode: $mode" >&2; exit 2 ;;
esac
