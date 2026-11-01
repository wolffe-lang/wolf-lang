#!/usr/bin/env bash
# s275 job "proof-f0" (hasu, the block given): pelt's F0 sequence (pax#196's
# replay; pelt lane sh34's tools/wait-replay, by copy from pelt's sh34
# branch) against a pelt built with the head's wolf and runtime (head.sha).
# pelt at its trunk 81562fd, built against its pinned wolf-std 78b9d3cb (a
# copy of sh34's staged pin), the tiers in F0_TIERS (native: the tier that
# stalled in sh34's rig; static: the shape PAX runs). 100 replays a tier
# under four of the block's CPUs, watched (the shell's task count at every
# prompt). Expected: runs 100 green 100 stalled 0 failed 0.
# usage: proof-f0.sh <A|B>
set -uo pipefail
L=/home/mfwolffe/lanes/s275
blk=${1:?block}
case $blk in
A) SIX=0-5 FOUR=0-3 ;;
B) SIX=6-11 FOUR=6-9 ;;
*) echo "block must be A or B"; exit 2 ;;
esac
tiers=${F0_TIERS:-native static}
head=$(cat "$L/head.sha")
P=$L/pelt
PSHA=81562fd
STDREV=78b9d3cbc41d51db44d18c40b6694403644d1c83
STD=$L/pelt-std
TOOL=$L/pelt-tools/wait-replay
O=$L/out/f0-$head
echo "BEGIN proof-f0 $(date -Iseconds) host $(hostname) block $blk six $SIX four $FOUR head $head tiers $tiers"
C="bash $L/scripts/ctr.sh"
step() {
    name=$1
    shift
    echo "BEGIN $name $(date -Iseconds)"
    "$@"
    rc=$?
    echo "END $name exit=$rc $(date -Iseconds)"
}
step build-head bash "$L/scripts/build-at.sh" "$head" $SIX
[ -d "$P" ] || git clone -q https://github.com/wolffe-lang/pelt "$P" || { echo "REFUSED clone"; exit 2; }
git -C "$P" fetch -q origin trunk sh34 || { echo "REFUSED fetch"; exit 2; }
git -C "$P" checkout -q --detach "$PSHA" || { echo "REFUSED checkout"; exit 2; }
echo "pelt at $(git -C "$P" rev-parse HEAD)"
mkdir -p "$L/pelt-tools" "$O" "$L/scratch/staticcc"
git -C "$P" show origin/sh34:tools/wait-replay >"$TOOL" && chmod +x "$TOOL"
echo "wait-replay from pelt $(git -C "$P" rev-parse --short origin/sh34): $(sha256sum "$TOOL")"
[ -d "$STD" ] || cp -a /home/mfwolffe/lanes/sh34/pelt/.wolf-bin/std "$STD"
test "$(cat "$STD/STD-REV")" = "$STDREV" || { echo "REFUSED std $(cat "$STD/STD-REV")"; exit 2; }
printf '#!/bin/sh\nexec /usr/bin/cc -static "$@"\n' >"$L/scratch/staticcc/cc"
chmod +x "$L/scratch/staticcc/cc"
WOLF=$L/b-$head/target/release/wolf
for t in $tiers; do
    case $t in
    native) args="" envp="" ;;
    static) args="--release --no-cache" envp="PATH=$L/scratch/staticcc:/opt/s263-rust/bin:/usr/bin:/bin" ;;
    *) echo "unknown tier $t"; continue ;;
    esac
    step "pelt-build-$t" $C nonet "$P" taskset.real -c $SIX env WOLF_STD="$STD" $envp \
        "$WOLF" build $args --deny-warnings src/pelt.lu -o "$O/pelt-$t"
    [ -x "$O/pelt-$t" ] || continue
    step "replay-$t" $C nonet "$P" taskset.real -c $FOUR python3 "$TOOL" --runs 100 --watch --out "$O/replay-$t" "$O/pelt-$t"
done
echo "END proof-f0 $(date -Iseconds)"
