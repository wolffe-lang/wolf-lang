#!/bin/sh
# s267's taskset shim (from s266's, the block moved) (RULES §4; wolf-lang#925). The lane's container runs
# on kasumi CPUs 12-15. A request that fits inside 0-3 is moved to the same
# positions inside 12-15, a request inside 12-15 passes as written, and any
# other request fails by name (exit 2). POSIX sh and no interpreter: the
# script installs no trap and changes no mask, so the dispositions and the
# mask the caller handed it reach the real taskset (and its command)
# unchanged; every other argument passes byte for byte.
real=/usr/bin/taskset.real
log=${S267_TASKSET_LOG:-/home/mfwolffe/lanes/s267/evidence/taskset-translations.log}

die() {
    printf 's267 taskset: %s\n' "$1" >&2
    printf '{"requested":"%s","refused":"%s","pid":%s}\n' "${req:-}" "$1" "$$" >>"$log"
    exit 2
}

# A shell reads its script from the lowest free descriptor, so a caller
# that closed 0, 1 or 2 (stdfd_lanes closes each in turn) would find the
# script open there in the program: the one descriptor this shim makes.
# Each exec below closes it first, in the same compound command (parsed
# whole before the script's descriptor goes).
self=$(readlink -f -- "$0")
run() {
    for n in 0 1 2; do
        if [ "$(readlink "/proc/$$/fd/$n" 2>/dev/null)" = "$self" ]; then
            eval "exec $n<&-"
        fi
    done
    exec "$real" "$@"
}

# Every request is logged, translated or not: a gate that still asks for
# CPUs outside the block shows as a "translated":true line.
record() {
    printf '{"requested":"%s","assigned":"%s","translated":%s,"pid":%s}\n' \
        "$req" "$1" "$2" "$$" >>"$log" || die "cannot append the translation log"
}

n=0
mode=mask
inline=0
idx=0
for a do
    n=$((n + 1))
    case $a in
    --pid | --help | --version) run "$@" ;;
    --)
        idx=$((n + 1))
        break
        ;;
    --cpu-list=*)
        mode=list
        inline=1
        idx=$n
        break
        ;;
    --cpu-list) mode=list ;;
    --all-tasks) ;;
    --*) run "$@" ;;
    -*p*) run "$@" ;;
    -?*)
        rest=${a#-}
        while [ -n "$rest" ]; do
            c=${rest%"${rest#?}"}
            rest=${rest#?}
            case $c in
            c) mode=list ;;
            a) ;;
            *) run "$@" ;;
            esac
        done
        ;;
    *)
        idx=$n
        break
        ;;
    esac
done
[ "$idx" -gt 0 ] && [ "$idx" -le "$#" ] || run "$@"
eval "req=\${$idx}"
[ "$inline" -eq 1 ] && req=${req#--cpu-list=}

if [ "$mode" = list ]; then
    cpus=
    rest=$req
    while :; do
        part=${rest%%,*}
        case $part in
        '' | *[!0-9:-]*) die "invalid CPU list '$req'" ;;
        esac
        case $part in
        *-*)
            low=${part%%-*}
            tail=${part#*-}
            case $tail in
            *:*)
                high=${tail%%:*}
                step=${tail#*:}
                ;;
            *)
                high=$tail
                step=1
                ;;
            esac
            ;;
        *)
            low=$part
            high=$part
            step=1
            ;;
        esac
        for v in "$low" "$high" "$step"; do
            case $v in
            '' | *[!0-9]*) die "invalid CPU range '$part'" ;;
            esac
        done
        [ "$step" -gt 0 ] && [ "$high" -ge "$low" ] || die "invalid CPU range '$part'"
        [ $(((high - low) / step + 1)) -le 4 ] || die "CPU set '$req' does not fit CPUs 12-15"
        c=$low
        while [ "$c" -le "$high" ]; do
            cpus="$cpus $c"
            c=$((c + step))
        done
        [ "$rest" = "$part" ] && break
        rest=${rest#*,}
    done
    low_ok=1
    high_ok=1
    for c in $cpus; do
        [ "$c" -le 3 ] || low_ok=0
        [ "$c" -ge 12 ] && [ "$c" -le 15 ] || high_ok=0
    done
    if [ "$high_ok" -eq 1 ]; then
        record "$req" false
        run "$@"
    fi
    [ "$low_ok" -eq 1 ] || die "CPU set '$req' does not fit CPUs 12-15"
    repl=
    for c in $cpus; do
        repl="$repl${repl:+,}$((c + 12))"
    done
else
    hex=${req#0x}
    case $hex in
    '' | *[!0-9a-fA-F]*) run "$@" ;;
    esac
    [ "${#hex}" -le 8 ] || die "affinity mask '$req' does not fit CPUs 12-15"
    mask=$((0x$hex))
    [ "$mask" -ne 0 ] || die "empty CPU set"
    if [ $((mask & ~0xf000)) -eq 0 ]; then
        record "$req" false
        run "$@"
    fi
    [ $((mask & ~0xf)) -eq 0 ] || die "affinity mask '$req' does not fit CPUs 12-15"
    repl=$(printf '%x' $((mask << 12)))
fi
record "$repl" true
[ "$inline" -eq 1 ] && repl="--cpu-list=$repl"
pos=0
for a do
    shift
    pos=$((pos + 1))
    if [ "$pos" -eq "$idx" ]; then
        set -- "$@" "$repl"
    else
        set -- "$@" "$a"
    fi
done
run "$@"
