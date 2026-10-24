#!/bin/sh
set -eu
plain=$(grep SigIgn /proc/self/status)
wrapped=$(/usr/bin/taskset -c 0-3 grep SigIgn /proc/self/status)
printf 'default dispositions: plain=%s wrapped=%s\n' "$plain" "$wrapped"
[ "$plain" = "$wrapped" ]
plain_ignored=$(sh -c 'trap "" PIPE XFSZ; exec grep SigIgn /proc/self/status')
wrapped_ignored=$(sh -c 'trap "" PIPE XFSZ; exec /usr/bin/taskset -c 0-3 grep SigIgn /proc/self/status')
printf 'inherited ignored dispositions: plain=%s wrapped=%s\n' "$plain_ignored" "$wrapped_ignored"
[ "$plain_ignored" = "$wrapped_ignored" ]
/usr/bin/taskset -c 0-3 sh -c 'taskset -pc $$'
/usr/bin/taskset -c 0 sh -c 'taskset -pc $$'
# An option belonging to the target must not bypass affinity translation.
/usr/bin/taskset -c 0-3 sh -c 'taskset -pc $$; [ "$1" = "-p" ]; [ "$2" = "a b" ]; [ -z "$3" ]' sh -p 'a b' ''
if /usr/bin/taskset -c 0-7 true; then
    echo 'oversized affinity unexpectedly succeeded' >&2
    exit 1
else
    [ "$?" -eq 2 ]
fi
printf 'taskset selftest passed\n'
