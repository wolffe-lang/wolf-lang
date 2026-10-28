set -uo pipefail
source /lane/scripts/s270-env.sh
export WOLF_STD=/lane/inputs/wolf/wolf-0.2.35-x86_64-unknown-linux-gnu/std
cd /lane/wolf-lang
git status --short
cargo build --locked -q -p wolf_driver -p wolf_rt
printf 'CARGO_BUILD_EXIT %s\n' "$?"
grep -c 'wolf-lang#940' crates/wolf_wir/src/midend/interproc.rs
out=/lane/scratch/s270c-pack
mkdir -p "$out"
for w in 035 base fixed; do
 case $w in
  035) wolf=/lane/inputs/wolf/wolf-0.2.35-x86_64-unknown-linux-gnu/wolf ;;
  base) wolf=/lane/scratch/s270c-bisect/wolf ;;
  fixed) wolf=/lane/wolf-lang/target/debug/wolf ;;
 esac
 for tree in pack-full pack-rec; do
  cd "/lane/witnesses/$tree"
  bin="$out/$w-$tree"
  env -u WOLF_MIDEND_SKIP "$wolf" build --release --no-cache --deny-warnings src/pack.lu -o "$bin" > "$bin.build.log" 2>&1
  rc=$?
  printf '\n%s %s (release, mid-end on) BUILD_EXIT %s\n' "$w" "$tree" "$rc"
  if test "$rc" != 0; then tail -n 8 "$bin.build.log"; continue; fi
  if test "$tree" = pack-full; then
   "$bin" --selftest 2>&1 | tail -n 1
  fi
  python3 -I tools/pty --bin "$bin" --rows quit,ctrl-c > "$bin.pty.log" 2>&1
  printf 'PTY_EXIT %s\n' "$?"
  cat "$bin.pty.log"
 done
done
