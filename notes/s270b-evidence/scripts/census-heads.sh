#!/usr/bin/env bash
# s270b: the census clones' heads, one line each
cd /home/mfwolffe/lanes/s270b/census
for r in */; do r=${r%/}; printf '%s %s\n' "$r" "$(git -C "$r" rev-parse HEAD)"; done
