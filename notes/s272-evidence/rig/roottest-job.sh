#!/usr/bin/env bash
# s272 roottest job: one block's two locks (block name outside), one long
# wait each, held for the build container and the host's case containers.
# usage: roottest-job.sh <A|B>
set -u
block=$1
case $block in A) cpus=0-5 ;; B) cpus=6-11 ;; *) echo "block A or B"; exit 2 ;; esac
L=/home/mfwolffe/lanes
d=$L/s272/records/roottest
mkdir -p "$d"; rm -f "$d/exit.marker"; echo $$ > "$d/pid"
{
echo "S272 roottest waiting for block $block at $(date -u +%FT%TZ)"
flock -w 21600 $L/hasu-block-$block.lock flock -w 21600 $L/hasu-$block.lock env S272_CPUS=$cpus bash -c '
  export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
  echo "S272 roottest acquired the block at $(date -u +%FT%TZ), cpus $S272_CPUS"
  podman run --rm --name s272-roottest-build --memory 12g --pids-limit 16384 --userns keep-id:uid=1000,gid=1000 --user 0 -v /home/mfwolffe/lanes/s272:/lane:rw -w /lane/wolf-lang localhost/r41-toolchain:1 \
    bash -c "/usr/bin/taskset.real -c $S272_CPUS setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash /lane/scripts/roottest-build.sh; exit \$?"
  rc=$?; echo "END roottest-build exit=$rc"; [ $rc -eq 0 ] || exit $rc
  bash /home/mfwolffe/lanes/s272/scripts/roottest-run.sh
  rc=$?; echo "END roottest-run exit=$rc"; exit $rc'
echo "exit=$?" > "$d/exit.marker"
} > "$d/log" 2>&1
