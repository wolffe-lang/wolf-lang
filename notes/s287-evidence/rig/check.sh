#!/bin/bash
# s287: compile-check and run named tests at a commit (by build-at.sh, behind the lock).
# usage: check.sh <cargo args...>   e.g. check.sh test -p wolf_sema --lib
set -u
echo "== $(git rev-parse --short=8 HEAD) $(date -u +%T): cargo $*"
cargo "$@"
