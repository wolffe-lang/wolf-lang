#!/bin/sh
# s240: what dash's job control and GNU timeout ask of the kernel, read
# from strace of the shipped binaries (black-box; no source read). Runs
# inside the throwaway glibc container as its first process; every
# process signalled is one this script started. Writes /out/*.trace.
set -u
T="strace -f -qq -o"
CALLS=setpgid,getpgid,getpgrp,setsid,getsid,kill,tgkill,wait4,waitid,ioctl
echo "== dash: kill 0, kill -- -PGID and kill -1 as the builtin sends them"
# A session of its own for kill 0 (setsid). USR1 is ignored in this
# subshell and so in everything it starts (strace, dash, the sleeps):
# kill -1 then reaches every process here but the first, which POSIX's
# rule and Linux's pid 1 keep out, and harms none of them.
( trap '' USR1
$T /out/dash-kill.trace -e trace=kill setsid dash -c '
  sleep 30 & a=$!
  kill -USR1 0; echo "kill 0: $?"
  setsid sleep 30 & sleep 0.2; b=$(ps -o pgid= -C sleep | sort -n | tail -1 | tr -d " ")
  kill -USR1 -$b; echo "kill -$b: $?"
  kill -USR1 -1; echo "kill -1: $?"
  kill $a; kill -$b; wait' )
grep -E 'kill\((0|-)' /out/dash-kill.trace | sed 's/^[0-9]* *//' | sort | uniq -c

echo "== dash -m (a script) under a pseudo-terminal: a job, stopped, continued, killed"
printf '%s\n' 'set -m' 'sleep 30 &' 'kill -STOP %1' 'jobs' 'kill -CONT %1' 'jobs' 'kill %1' 'wait' 'echo end' > /out/jobs.sh
$T /out/dash-m.trace -e trace=$CALLS script -qec 'dash /out/jobs.sh' /dev/null > /out/dash-m.out 2>&1 < /dev/null
sed 's/^[0-9]* *//' /out/dash-m.trace | grep -E 'setpgid|kill\(|TIOCSPGRP|wait4|waitid' | sed -E 's/[0-9]{2,}/N/g' | sort | uniq -c | sort -rn | head -20
echo "-- what it printed:"; tr -d '\r' < /out/dash-m.out

echo "== dash -i -m under a pseudo-terminal: fg, bg, jobs"
printf 'sleep 2 &\njobs\nfg %%1\nsleep 30 &\nkill -STOP %%1\njobs\nbg %%1\nkill %%1\nwait\nexit\n' > /out/typed
$T /out/dash-im.trace -e trace=$CALLS script -qec 'dash -i -m' /dev/null < /out/typed > /out/dash-im.out 2>&1
sed 's/^[0-9]* *//' /out/dash-im.trace | grep -E 'setpgid|setsid|kill\(|TIOCSPGRP|TIOCGPGRP|TIOCSCTTY|wait4|waitid' \
  | sed -E 's/[0-9]{2,}/N/g' | sort | uniq -c | sort -rn | head -30
echo "-- what the session printed:"; tr -d '\r' < /out/dash-im.out | head -30

echo "== GNU timeout: its own group, and its command's children"
timeout --version | head -1
$T /out/timeout.trace -e trace=setpgid,setsid,kill \
  timeout 1 sh -c 'sleep 30 & echo child=$!; sleep 30'
echo "timeout status: $?"
grep -E 'setpgid|setsid|kill\(' /out/timeout.trace | sed 's/^[0-9]* *//' | sed -E 's/[0-9]{2,}/N/g' | sort | uniq -c
$T /out/timeout-fg.trace -e trace=setpgid,setsid,kill \
  timeout --foreground 1 sh -c 'sleep 3 & echo child=$!; sleep 3'
echo "timeout --foreground status: $?"
grep -E 'setpgid|setsid|kill\(' /out/timeout-fg.trace | sed 's/^[0-9]* *//' | sed -E 's/[0-9]{2,}/N/g' | sort | uniq -c
echo "== the same two without strace (strace -f waits for every tracee): who outlives timeout"
timeout 1 sh -c 'sleep 30 & sleep 30'; echo "timeout status: $?"
sleep 0.3; echo "-- sleeps left after timeout:"; ps -eo pid,pgid,stat,comm | grep '[s]leep' || echo none
timeout --foreground 1 sh -c 'sleep 30 & sleep 30'; echo "timeout --foreground status: $?"
sleep 0.3; echo "-- sleeps left after timeout --foreground:"; ps -eo pid,pgid,stat,comm | grep '[s]leep' || echo none
for p in $(ps -eo pid,comm | awk '$2=="sleep"{print $1}'); do kill -9 "$p"; done
echo done
