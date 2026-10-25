#!/usr/bin/env bash
set -u
cd /work
old=/work/controls/wolf/unpacked/wolf-0.2.35-x86_64-unknown-linux-gnu/wolf
lupin=/work/controls/lupin/unpacked/lupin-0.1.56-x86_64-unknown-linux-gnu/lupin
mkdir -p notes/s263-evidence
for mode in native checked; do
    if [ "$mode" = native ]; then variable=WOLF_NATIVE; else variable=WOLF_CHECKED; fi
    printf 'Differ %s versus published wolf starts %s\n' "$mode" "$(date -Iseconds)"
    cargo xtask differ target/debug/wolf "env $variable=1 $old" "--$mode" > "notes/s263-evidence/differ-wolf-0235-$mode.log" 2>&1 &
    pid=$!
    while kill -0 "$pid" 2>/dev/null; do
        printf 'Differ %s active %s\n' "$mode" "$(date -Iseconds)"
        tail -1 "notes/s263-evidence/differ-wolf-0235-$mode.log"
        sleep 30
    done
    wait "$pid"; rc=$?
    printf '%s\n' "$rc" > "notes/s263-evidence/differ-wolf-0235-$mode.exit"
    tail -5 "notes/s263-evidence/differ-wolf-0235-$mode.log"
    printf 'Differ %s exit=%s\n' "$mode" "$rc"
done
printf 'Checked reference differ starts %s\n' "$(date -Iseconds)"
cargo xtask differ target/debug/wolf "$lupin" --checked --triage > notes/s263-evidence/differ-lupin-0156-checked.log 2>&1 &
pid=$!
while kill -0 "$pid" 2>/dev/null; do
    printf 'Reference differ active %s\n' "$(date -Iseconds)"
    tail -1 notes/s263-evidence/differ-lupin-0156-checked.log
    sleep 30
done
wait "$pid"; rc=$?
printf '%s\n' "$rc" > notes/s263-evidence/differ-lupin-0156-checked.exit
tail -5 notes/s263-evidence/differ-lupin-0156-checked.log
printf 'Reference differ exit=%s\n' "$rc"
