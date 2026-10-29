#!/usr/bin/env bash
# s270b: move the base worktree to trunk d2753b48 for the #631 A/B (host side, no build)
set -euo pipefail
base=/home/mfwolffe/lanes/s270b
git -C "$base/base" checkout --quiet --detach d2753b48c334b844d25987be25d1d83bf1bb67dd
git -C "$base/base" rev-parse HEAD | tee "$base/records/base-head.txt"
git -C "$base/base" status --short | head -5
