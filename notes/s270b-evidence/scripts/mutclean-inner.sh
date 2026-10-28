#!/usr/bin/env bash
# dump the ambient-clean verdicts and the mem phase record for the yew witness (N=1024, C=2)
set -uo pipefail
source /lane/scripts/env.sh
export WOLF_STD=/lane/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu/std
cd /lane/witnesses/yew
sed -e 's/100000/1024/g' -e 's/99999/1023/g' -e 's/0\.\.64 +/0..2 +/' /lane/yew/notes/yw07b-evidence/stdlib/scratch-once/main.lu > main.lu
/lane/wolf-lang/target/debug/wolf conform-run --json --phase=mem --dump=mut-clean main.lu > /lane/records/mutclean.out 2> /lane/records/mutclean.err
echo "rc=$?"
cat /lane/records/mutclean.out
grep -E 'fl_gc_alloc|fl_str_new|fl_list_new|fl_list_push|fl_empty_slot|fl_slot_reset|fl_gc_collect|fl_copy_slot|^fn main' /lane/records/mutclean.err
wc -l /lane/records/mutclean.err
