#!/usr/bin/env python3
"""s286: the cost rows' before/after ratios from one perf take's raw files.

t1's family B (t1-r<round>-<head>.jsonl, every run's ns_per_op per lane) and
the pool's benches (pool-benches.txt, one line per run). For each row: the
median over every run of each head, the ratio after/before, and the margin
bench/protocol.md states: a delta counts only where it clears both 3xMAD
(the larger head's) and the 2% practical floor.

    perf-ratios.py DIR BEFORE8 AFTER8
"""
import glob
import json
import os
import re
import statistics as st
import sys

d, B, A = sys.argv[1], sys.argv[2], sys.argv[3]


def mad(xs):
    m = st.median(xs)
    return st.median([abs(x - m) for x in xs])


def table(rows, title, fmt):
    print(title)
    print(f"  {'row':42} {'before':>10} {'after':>10} {'ratio':>7} {'margin':>7}  verdict")
    for name in sorted({k[0] for k in rows}):
        b, a = rows.get((name, B), []), rows.get((name, A), [])
        if not b or not a:
            continue
        mb, ma = st.median(b), st.median(a)
        margin = max(3 * max(mad(b), mad(a)) / mb, 0.02)
        ratio = ma / mb
        verdict = "tie" if abs(ratio - 1) <= margin else ("faster" if ratio < 1 else "slower")
        print(f"  {name:42} {mb:{fmt}} {ma:{fmt}} {ratio:7.4f} {margin:7.4f}  {verdict} (n={len(b)},{len(a)})")
    print()


t1 = {}
for f in sorted(glob.glob(os.path.join(d, "t1-r*.jsonl"))):
    m = re.search(r"t1-r\d+-(\w+)\.jsonl$", f)
    for line in open(f):
        rec = json.loads(line)
        if rec.get("metric") == "ns_per_op":
            t1.setdefault((f"{rec['bench']}/{rec['lang']}", m.group(1)), []).append(rec["value"])
if t1:
    table(t1, "t1 family B, ns per op (every run of every round)", "10.4f")

unit = {"ns": 1.0, "µs": 1000.0, "us": 1000.0, "ms": 1e6, "s": 1e9}
pool = {}
p = os.path.join(d, "pool-benches.txt")
if os.path.exists(p):
    for line in open(p):
        parts = line.split(" ", 3)
        if len(parts) < 4:
            continue
        _, h, bench, rest = parts
        for k, v in re.findall(r"(\w+)=([0-9.]+(?:ns|µs|us|ms|s))", rest):
            mm = re.match(r"([0-9.]+)(\D+)", v)
            pool.setdefault((f"{bench.rstrip(':')}/{k}", h), []).append(float(mm.group(1)) * unit[mm.group(2)])
if pool:
    table(pool, "the pool's benches, ns (every run)", "10.1f")
