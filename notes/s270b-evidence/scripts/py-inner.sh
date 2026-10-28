#!/usr/bin/env bash
# run one python script from /lane/scripts with its arguments, in the lane env
set -uo pipefail
source /lane/scripts/env.sh
cd /lane
python3 -I "/lane/scripts/$1" "${@:2}"
