#!/usr/bin/env python3
"""s275: the latency summary of series.sh's runs: per side (trunk, head) and
per run, the count, median, p99, max and the count over 50 ms of
wait_fd_wakes_for_a_signal_while_parked's 200 waits."""
import glob, os, re, sys

D = sys.argv[1] if len(sys.argv) > 1 else "/Users/mfwolffe/lanes/s275/lat"


def pct(xs, p):
    xs = sorted(xs)
    if not xs:
        return 0
    k = max(0, min(len(xs) - 1, int(round(p / 100.0 * len(xs) + 0.5)) - 1))
    return xs[k]


def line(name, xs):
    return "%-12s n=%5d median=%6.2fms p99=%7.2fms max=%8.2fms over50=%d" % (
        name, len(xs), pct(xs, 50) / 1000, pct(xs, 99) / 1000, max(xs) / 1000 if xs else 0,
        sum(1 for x in xs if x > 50000))


allx = {"trunk": [], "head": []}
for f in sorted(glob.glob(os.path.join(D, "run-*-*.log")), key=lambda p: (int(re.search(r"run-(\d+)-", p).group(1)), p)):
    m = re.search(r"run-(\d+)-(trunk|head)\.log$", f)
    i, side = m.group(1), m.group(2)
    xs = [int(v) for v in re.findall(r"S275_LAT us=(\d+)", open(f, errors="replace").read())]
    allx[side] += xs
    res = "ok" if "test result: ok" in open(f, errors="replace").read() else "FAILED"
    print(line("run %s %s" % (i, side), xs), res)
print()
for side in ("trunk", "head"):
    print(line(side + " all", allx[side]))
