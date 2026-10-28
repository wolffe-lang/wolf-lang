set -uo pipefail
source /lane/scripts/s270-env.sh
wolf=${S270C_WOLF:-/lane/wolf-lang/target/debug/wolf}
cd /lane/scratch/s270c-red
mkdir -p err
show() { python3 -I -c 'import json,sys
try:
    r=json.load(sys.stdin); print(r.get("verdict"), repr(r.get("stdout_inline")))
except Exception as e:
    print("NOT JSON", e)'; }
for d in ${S270C_ONLY:-v[0-9]*/}; do
 d=${d%/}
 f=$d/main.lu
 printf '\n== %s\n' "$f"
 for mode in checked native release; do
  printf '%s: ' "$mode"
  "$wolf" conform-run --json "--$mode" "$f" 2>"err/$d.$mode.err" | show
 done
 printf 'release-off: '
 WOLF_MIDEND=0 "$wolf" conform-run --json --release "$f" 2>"err/$d.off.err" | show
 printf 'lupin: '
 "$LUPIN" conform-run --json "$f" 2>"err/$d.lupin.err" | show
done
