#!/usr/bin/env python3
"""s286: perf3's read of b3_churn at ten times the ops: each lane's median
ns per op (the kernel's own clock) and user CPU seconds, before and after,
with bench/protocol.md's margin (3xMAD and the 2% floor).
    pgo-read.py TAKE_LOG BEFORE8 AFTER8
"""
import re
import statistics as st
import sys

log, B, A = sys.argv[1:4]
d = {}
for line in open(log):
    m = re.match(r'\s*r(\d+) (\w+) (wolf|wolf_pgo) user_s=([0-9.]+) \{"ns":(\d+),"ops":(\d+)', line)
    if m:
        _, h, lane, u, ns, ops = m.groups()
        d.setdefault((lane, h), {"ns": [], "user": []})
        d[(lane, h)]["ns"].append(int(ns) / int(ops))
        d[(lane, h)]["user"].append(float(u))


def mad(x):
    m = st.median(x)
    return st.median([abs(v - m) for v in x])


for lane in ("wolf", "wolf_pgo"):
    for k in ("ns", "user"):
        b, a = d[(lane, B)][k], d[(lane, A)][k]
        mb, ma = st.median(b), st.median(a)
        margin = max(3 * max(mad(b), mad(a)) / mb, 0.02)
        r = ma / mb
        v = "tie" if abs(r - 1) <= margin else ("faster" if r < 1 else "slower")
        print(f"b3_churn/{lane} at 7900000 ops, {k:4}: before {mb:.4f} after {ma:.4f} "
              f"ratio {r:.4f} margin {margin:.4f} {v} (n={len(b)},{len(a)})")
