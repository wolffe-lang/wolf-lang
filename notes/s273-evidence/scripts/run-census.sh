#!/bin/bash
# s273: the --phase=mem census, base vs head, every tree under src/.
# usage: run-census.sh <base-wolf> <head-wolf>   (run behind locked.sh)
set -u
C=/Users/mfwolffe/lanes/s273/census
roots=$(ls -d $C/src/*/)
roots=$(echo $roots | sed 's#/ # #g; s#/$##')
$C/census.sh "$1" $C/base.tsv $roots
echo "END census-base exit=$?"
$C/census.sh "$2" $C/head.tsv $roots
echo "END census-head exit=$?"
if cmp -s $C/base.tsv $C/head.tsv; then
  echo "census: byte-identical ($(wc -l < $C/base.tsv) files)"
  echo "END census-diff exit=0"
else
  diff $C/base.tsv $C/head.tsv | head -50
  echo "END census-diff exit=1"
fi
