#!/usr/bin/env bash
# s272: s275's drain witness in three loads on kasumi (inside job.sh's hold of a
# slot), with the binaries drain-inner.sh built.
# usage: chain-drain2.sh <slot> <runs> <label>...
slot=$1; shift
bash /home/mfwolffe/lanes/s272/scripts/container.sh "$slot" drain2 0 /lane/scripts/drain2-inner.sh "$@"
