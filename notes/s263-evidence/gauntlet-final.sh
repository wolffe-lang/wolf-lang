#!/usr/bin/env bash
set -u
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
lane=$HOME/lanes/s263
expected=${1:?pass the final tested source head}
printf '%s\n' "$$" > "$lane/gauntlet-final.pid"
trap 'rc=$?; printf "%s\n" "$rc" > "$lane/gauntlet-final.exit.tmp"; mv "$lane/gauntlet-final.exit.tmp" "$lane/gauntlet-final.exit"; printf "s263 FINAL gauntlet exit=%s end=%s\n" "$rc" "$(date -Iseconds)"' EXIT
printf 's263 FINAL gauntlet start=%s head=%s pid=%s\n' "$(date -Iseconds)" "$(git -C "$lane" rev-parse HEAD)" "$$"
[ "$(git -C "$lane" rev-parse HEAD)" = "$expected" ] || exit 2
git -C "$lane" diff --exit-code HEAD -- || exit 2
grep -E '^Sig(Blk|Ign):' "/proc/$$/status"
ulimit -c 0
bash "$lane/container/run-final.sh" env RUST_TEST_NOCAPTURE=1 WOLF_RT_NONE_REQUIRE=1 \
 LUPIN=/work/controls/lupin/unpacked/lupin-0.1.56-x86_64-unknown-linux-gnu/lupin \
 WOLF_STD=/work/controls/wolf/unpacked/wolf-0.2.35-x86_64-unknown-linux-gnu/std \
 cargo xtask ci
exit "$?"
