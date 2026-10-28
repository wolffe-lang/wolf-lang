set -uo pipefail
source /lane/scripts/s270-env.sh
export WOLF_STD=/lane/inputs/wolf/wolf-0.2.35-x86_64-unknown-linux-gnu/std
printf 's270c PROBE %s\n' "$(date -u +%FT%TZ)"
cd /lane/wolf-lang
git rev-parse HEAD
git status --short | head
cargo build --locked -q -p wolf_driver -p wolf_rt
printf 'CARGO_BUILD_EXIT %s\n' "$?"
cd /lane/witnesses/pack-rec
mkdir -p target
for w in head 035; do
 if test "$w" = head; then wolf=/lane/wolf-lang/target/debug/wolf; else wolf=/lane/inputs/wolf/wolf-0.2.35-x86_64-unknown-linux-gnu/wolf; fi
 "$wolf" --version | head -1
 for mode in native release-on release-off; do
  flags=; test "$mode" != native && flags=--release
  midend=1; test "$mode" = release-off && midend=0
  out="target/probe-$w-$mode"
  if test "$mode" = native; then
   "$wolf" build --no-cache src/decoder_probe.lu -o "$out" > "/lane/logs/s270c-probe-$w-$mode-build.log" 2>&1
  else
   WOLF_MIDEND="$midend" "$wolf" build --no-cache --release src/decoder_probe.lu -o "$out" > "/lane/logs/s270c-probe-$w-$mode-build.log" 2>&1
  fi
  rc=$?; printf '%s %s BUILD_EXIT %s\n' "$w" "$mode" "$rc"
  if test "$rc" != 0; then tail -n 20 "/lane/logs/s270c-probe-$w-$mode-build.log"; continue; fi
  "$out"
  printf '%s %s RUN_EXIT %s\n' "$w" "$mode" "$?"
 done
done
printf 's270c PROBE DONE %s\n' "$(date -u +%FT%TZ)"
