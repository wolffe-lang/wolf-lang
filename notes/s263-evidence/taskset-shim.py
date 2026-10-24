#!/usr/bin/python3
"""Lane-only affinity translation; never installed on the host."""
import json
import os
import re
import sys

REAL = "/usr/bin/taskset.real"
ALLOWED = set(range(6, 12))
args = sys.argv[1:]


def fail(message):
    print("s263 taskset: " + message, file=sys.stderr)
    sys.exit(2)


def parse_list(value):
    cpus = set()
    for part in value.split(","):
        match = re.fullmatch(r"(\d+)(?:-(\d+)(?::(\d+))?)?", part)
        if not match:
            fail("invalid CPU list " + repr(value))
        low = int(match[1])
        high = int(match[2]) if match[2] else low
        step = int(match[3]) if match[3] else 1
        if step == 0 or high < low:
            fail("invalid CPU range " + repr(part))
        # Reject oversized ranges without allocating an unbounded set.
        if (high - low) // step + 1 > 6:
            fail("requested CPU set does not fit block B")
        cpus.update(range(low, high + 1, step))
    return cpus


# Existing-pid/query forms are untouched, as the orchestration ruling asks.
if any(a in ("--pid", "--help", "--version") or
       (a.startswith("-") and not a.startswith("--") and "p" in a[1:])
       for a in args):
    os.execv(REAL, [REAL] + args)

index = 0
list_mode = False
inline = False
while index < len(args) and args[index].startswith("-"):
    option = args[index]
    if option in ("-c", "--cpu-list"):
        list_mode = True
    elif option.startswith("--cpu-list="):
        list_mode = True
        inline = True
        break
    elif option not in ("-a", "--all-tasks"):
        fail("unsupported affinity option " + repr(option))
    index += 1
if index >= len(args):
    os.execv(REAL, [REAL] + args)
requested = args[index].split("=", 1)[1] if inline else args[index]
if list_mode:
    cpus = parse_list(requested)
else:
    try:
        mask = int(requested, 16)
    except ValueError:
        fail("invalid affinity mask " + repr(requested))
    if mask.bit_count() > 6:
        fail("requested CPU mask does not fit block B")
    cpus = {i for i in range(mask.bit_length()) if mask & (1 << i)}
if not cpus or len(cpus) > 6:
    fail("requested CPU set does not fit block B")
assigned = cpus if cpus <= ALLOWED else set(range(6, 6 + len(cpus)))
replacement = ",".join(map(str, sorted(assigned))) if list_mode else format(sum(1 << i for i in assigned), "x")
if assigned != cpus:
    record = {"requested": requested, "assigned": sorted(assigned), "argv": sys.argv, "pid": os.getpid()}
    fd = os.open("/work/notes/s263-evidence/taskset-translations.log", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
    try:
        os.write(fd, (json.dumps(record, separators=(",", ":")) + "\n").encode())
    finally:
        os.close(fd)
args[index] = "--cpu-list=" + replacement if inline else replacement
os.execv(REAL, [REAL] + args)
