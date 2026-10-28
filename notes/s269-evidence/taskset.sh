#!/bin/sh
# Disposition-neutral CPU affinity translation for s269, based on s268.
real=/usr/bin/taskset.real
log=/home/mfwolffe/lanes/s269/evidence/taskset.log
die() {
    printf 's269 taskset: %s\n' "$1" >&2
    exit 2
}
self=$(readlink -f -- "$0")
run() {
    for n in 0 1 2; do
        if [ "$(readlink "/proc/$$/fd/$n" 2>/dev/null)" = "$self" ]; then eval "exec $n<&-"; fi
    done
    exec "$real" "$@"
}
record() {
    printf '{"requested":"%s","assigned":"%s","translated":%s,"pid":%s}\n' "$req" "$1" "$2" "$$" >>"$log" || die 'cannot append translation log'
}
n=0
mode=mask
inline=0
idx=0
for a do
    n=$((n + 1))
    case $a in
        --pid|--help|--version) run "$@" ;;
        --) idx=$((n + 1)); break ;;
        --cpu-list=*) mode=list; inline=1; idx=$n; break ;;
        --cpu-list) mode=list ;;
        --all-tasks) ;;
        --*) run "$@" ;;
        -*p*) run "$@" ;;
        -?*)
            rest=${a#-}
            while [ -n "$rest" ]; do
                c=${rest%"${rest#?}"}; rest=${rest#?}
                case $c in c) mode=list ;; a) ;; *) run "$@" ;; esac
            done ;;
        *) idx=$n; break ;;
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
        case $part in ''|*[!0-9:-]*) die "invalid CPU list '$req'" ;; esac
        case $part in
            *-*) low=${part%%-*}; tail=${part#*-}; case $tail in *:*) high=${tail%%:*}; step=${tail#*:} ;; *) high=$tail; step=1 ;; esac ;;
            *) low=$part; high=$part; step=1 ;;
        esac
        for v in "$low" "$high" "$step"; do case $v in ''|*[!0-9]*) die "invalid CPU range '$part'" ;; esac; done
        [ "$step" -gt 0 ] && [ "$high" -ge "$low" ] || die "invalid CPU range '$part'"
        [ $(((high - low) / step + 1)) -le 4 ] || die "CPU set '$req' does not fit CPUs 8-11"
        c=$low
        while [ "$c" -le "$high" ]; do cpus="$cpus $c"; c=$((c + step)); done
        [ "$rest" = "$part" ] && break
        rest=${rest#*,}
    done
    low_ok=1
    high_ok=1
    for c in $cpus; do
        [ "$c" -le 3 ] || low_ok=0
        [ "$c" -ge 8 ] && [ "$c" -le 11 ] || high_ok=0
    done
    if [ "$high_ok" -eq 1 ]; then record "$req" false; run "$@"; fi
    [ "$low_ok" -eq 1 ] || die "CPU set '$req' does not fit CPUs 8-11"
    repl=
    for c in $cpus; do repl="$repl${repl:+,}$((c + 8))"; done
else
    hex=${req#0x}
    case $hex in ''|*[!0-9a-fA-F]*) run "$@" ;; esac
    [ "${#hex}" -le 8 ] || die "mask '$req' does not fit CPUs 8-11"
    mask=$((0x$hex))
    [ "$mask" -ne 0 ] || die 'empty CPU set'
    if [ $((mask & ~0xf00)) -eq 0 ]; then record "$req" false; run "$@"; fi
    [ $((mask & ~0xf)) -eq 0 ] || die "mask '$req' does not fit CPUs 8-11"
    repl=$(printf '%x' $((mask << 8)))
fi
record "$repl" true
[ "$inline" -eq 1 ] && repl="--cpu-list=$repl"
pos=0
for a do
    shift
    pos=$((pos + 1))
    if [ "$pos" -eq "$idx" ]; then set -- "$@" "$repl"; else set -- "$@" "$a"; fi
done
run "$@"
