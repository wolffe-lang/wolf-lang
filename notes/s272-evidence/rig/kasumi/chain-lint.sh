#!/usr/bin/env bash
# s272: fmt and clippy at one commit on kasumi (inside job.sh's hold of slot 8-11).
# usage: chain-lint.sh <sha>
bash /home/mfwolffe/lanes/s272/scripts/container.sh lint 0 /lane/scripts/lint-inner.sh "$1"
