set -uo pipefail
source /lane/scripts/s270-env.sh
export WOLF_STD=/lane/wolf-std/std
cd /lane/wolf-lang
git rev-parse --short=8 HEAD
out=/lane/scratch/s270c-uaf
mkdir -p "$out"
for row in corpus/conc/atomic_ub_uaf.lu corpus/memory/unsafe_ub_uaf.lu corpus/memory/volatile_ub_uaf.lu corpus/conc/atomic_race_plain.lu; do
 n=$(basename "$row" .lu)
 for tier in native release; do
  flag=; test "$tier" = release && flag=--release
  for side in head base; do
   if test "$side" = head; then wolf=/lane/wolf-lang/target/debug/wolf; else wolf=/lane/wolf-base/target/debug/wolf; fi
   "$wolf" build --no-cache ${flag:+$flag} "$row" -o "$out/$n-$tier-$side" > "$out/$n-$tier-$side.build" 2>&1
   printf '%s %s %s build=%s sha256=%s\n' "$n" "$tier" "$side" "$?" "$(sha256sum "$out/$n-$tier-$side" | cut -c1-16)"
  done
  for side in head base; do
   runs=
   for i in 1 2 3 4 5; do
    "$out/$n-$tier-$side" > "$out/run.out" 2> "$out/run.err"
    rc=$?
    runs="$runs exit=$rc/out=$(head -c 24 "$out/run.out" | tr '\n' ' ')/err=$(wc -c < "$out/run.err")"
   done
   printf '%s %s %s:%s\n' "$n" "$tier" "$side" "$runs"
  done
 done
done
