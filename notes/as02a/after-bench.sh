#!/bin/bash
# The gauntlet's steps after bench-gates (xtask ci stops at its first red),
# each alone at the checkout's head, each with its own exit code.
set -u
L=/home/mfwolffe/lanes/as02a
E=$L/evidence/after-bench
mkdir -p $E
cd $L/wolf-lang
echo "head $(git rev-parse --short HEAD)"
worst=0
run() {
  local name=$1; shift
  local t0=$(date +%s)
  cargo run --quiet -j4 --package xtask -- "$@" > $E/$name.log 2>&1
  local rc=$?
  [ $rc -gt $worst ] && worst=$rc
  echo "$name exit=$rc ($(( $(date +%s) - t0 )) s): $(tail -1 $E/$name.log | cut -c1-150)"
}
run spec-extract spec-extract --check
run conformance conformance
run print-gate print-gate
run diag-catalog diag-catalog --check
run doc-catalog doc-catalog --check
run fmt-lu fmt-lu
run fmt-fuzz fmt-fuzz --ci
run audit-surface audit-surface
run dist-smoke dist
run differ-self differ --self
run lane-coverage lane-coverage
echo "worst exit=$worst"
echo $worst > $E/worst.exit
exit $worst
