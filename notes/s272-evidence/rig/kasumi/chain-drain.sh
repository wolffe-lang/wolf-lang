#!/usr/bin/env bash
# s272: s275's drain witness at two commits on kasumi (inside job.sh's hold of a
# slot), the scratch on btrfs.
# usage: chain-drain.sh <slot> <runs> <label> <sha> <label> <sha>
slot=$1; shift
bash /home/mfwolffe/lanes/s272/scripts/container.sh "$slot" drain 0 /lane/scripts/drain-inner.sh "$@"
