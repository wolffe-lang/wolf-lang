#!/usr/bin/env bash
# s270b: move the hasu clone to origin/s270b at the expected sha; refuse anything else
set -euo pipefail
base=/home/mfwolffe/lanes/s270b
want=$1
cd "$base/wolf-lang"
git status --short
git fetch --quiet origin s270b
git reset --quiet --hard FETCH_HEAD
git clean -fdq -e target
have=$(git rev-parse HEAD)
printf 'clone at %s (want %s), branch %s\n' "$have" "$want" "$(git branch --show-current)"
git status --short
test "$have" = "$want"
