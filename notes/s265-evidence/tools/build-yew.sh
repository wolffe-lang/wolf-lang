#!/bin/bash
# s265: build yew cells a (0.2.34) and b (0.2.35 + compat) on the release tier.
set -uo pipefail
L=$HOME/lanes/s265
E=$L/evidence/yew; mkdir -p $E
exec > $E/build.log 2>&1
echo $$ > $E/build.pid
trap 'echo $? > $E/build.exit' EXIT
date -u
$L/ctr bash -c "
set -e
for c in a:0.2.34 b:0.2.35; do
  cell=\${c%%:*}; v=\${c#*:}
  cd $L/yew-\$cell
  echo \"BUILD \$cell \$v \$(date -u +%T)\"
  WOLF_STD=$L/tc/\$v/std /usr/bin/time -v $L/tc/\$v/wolf build --release --deny-warnings src/yew.lu -o target/release/yew 2> $E/build-\$cell.time
  echo \"BUILT \$cell \$(date -u +%T)\"
  tail -n +1 $E/build-\$cell.time | grep -E 'Elapsed|Maximum resident'
done
sha256sum $L/yew-a/target/release/yew $L/yew-b/target/release/yew
size $L/yew-a/target/release/yew $L/yew-b/target/release/yew
"
