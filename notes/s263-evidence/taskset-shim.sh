#!/bin/sh
# Python computes the replacement in a CHILD. This shell execs taskset,
# preserving the caller's dispositions (including intentionally ignored
# SIGPIPE/SIGXFSZ). Python must never exec the target program itself.
mapping=$(S263_TASKSET_ARGV0="$0" /usr/bin/python3 /usr/lib/s263-taskset-translate.py "$@") || exit "$?"
index=${mapping%%:*}
replacement=${mapping#*:}
if [ "$index" -ge 0 ]; then
    position=0
    # The for-list snapshots the original argv; rotation preserves every
    # argument byte and boundary, including spaces and empty arguments.
    for argument do
        shift
        if [ "$position" -eq "$index" ]; then
            set -- "$@" "$replacement"
        else
            set -- "$@" "$argument"
        fi
        position=$((position + 1))
    done
fi
exec /usr/bin/taskset.real "$@"
