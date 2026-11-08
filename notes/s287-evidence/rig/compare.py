"""s287: compare two measure.py records (base, head) file by file (adapted by
copy from s285's compare.py).

usage: python3 -I compare.py BASE.json HEAD.json

Prints, per DIR, the files whose verdict or phase moved and the files whose
kept diagnostics moved, with both sides; then one line per DIR: files, moved
verdicts, moved diagnostics. Exits 0 always: the lines are the measurement.
"""
import json
import os
import sys

base = json.load(open(sys.argv[1]))
head = json.load(open(sys.argv[2]))
total = [0, 0, 0]
for d in base:
    b, h = base[d]["per"], head[d]["per"]
    moved_v = []
    moved_d = []
    for f in sorted(set(b) | set(h)):
        bv = (b.get(f, {}).get("verdict"), b.get(f, {}).get("phase"))
        hv = (h.get(f, {}).get("verdict"), h.get(f, {}).get("phase"))
        bd, hd = b.get(f, {}).get("diags"), h.get(f, {}).get("diags")
        if bv != hv:
            moved_v.append((f, bv, hv))
        if bd != hd:
            moved_d.append((f, bd, hd))
    name = os.path.basename(d.rstrip("/"))
    for f, bv, hv in moved_v:
        print(f"  {name}/{f}: verdict {bv} -> {hv}")
    for f, bd, hd in moved_d:
        print(f"  {name}/{f}: diagnostics {bd} -> {hd}")
    print(f"{name}: {base[d].get('files', len(b))} file(s) in {len(b)} run(s) base, {len(h)} head; {len(moved_v)} verdict(s) moved, "
          f"{len(moved_d)} diagnostic set(s) moved")
    total[0] += len(h)
    total[1] += len(moved_v)
    total[2] += len(moved_d)
print(f"TOTAL: {total[0]} file(s); {total[1]} verdict(s) moved; {total[2]} diagnostic set(s) moved")
