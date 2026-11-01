#!/usr/bin/env bash
# s272: a detached job: pid file, log, exit marker.
# usage: launch.sh <A|B> <name> <inner> [args...]
name=$2
d=/home/mfwolffe/lanes/s272/records/$name
mkdir -p "$d"
rm -f "$d/exit.marker"
echo $$ > "$d/pid"
/home/mfwolffe/lanes/s272/scripts/container.sh "$@" > "$d/log" 2>&1
echo "exit=$?" > "$d/exit.marker"
