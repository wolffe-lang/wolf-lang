"""s287: the census compared run by run, for the tree's evidence. Public trees
get a line each, with every moved run named; the private trees (PRIVATE, a
comma-separated list of directory names) are folded into one aggregate line
that names none of them, as the lane rules ask.

usage: python3 -I census-summary.py BASE.json HEAD.json PRIVATE
"""
import json
import os
import sys

base = json.load(open(sys.argv[1]))
head = json.load(open(sys.argv[2]))
private = set(sys.argv[3].split(","))


def name_of(d):
    d = d.rstrip("/")
    return "corpus" if d.endswith("/corpus") else os.path.basename(d)


B = {name_of(k): v for k, v in base.items()}
H = {name_of(k): v for k, v in head.items()}
agg = [0, 0, 0, 0]
tot = [0, 0, 0]
for name in sorted(set(B) | set(H)):
    b = B.get(name, {}).get("per", {})
    h = H.get(name, {}).get("per", {})
    files = H.get(name, {}).get("files", 0)
    moved_v, moved_d = [], []
    for f in sorted(set(b) | set(h)):
        bv = (b.get(f, {}).get("verdict"), b.get(f, {}).get("phase"))
        hv = (h.get(f, {}).get("verdict"), h.get(f, {}).get("phase"))
        if bv != hv:
            moved_v.append((f, bv, hv))
        if b.get(f, {}).get("diags") != h.get(f, {}).get("diags"):
            moved_d.append((f, b.get(f, {}).get("diags"), h.get(f, {}).get("diags")))
    tot[0] += len(h)
    tot[1] += len(moved_v)
    tot[2] += len(moved_d)
    if name in private:
        agg[0] += files
        agg[1] += len(h)
        agg[2] += len(moved_v)
        agg[3] += len(moved_d)
        continue
    for f, bv, hv in moved_v:
        print(f"  {name}/{f}: verdict {bv[0]} at {bv[1]} -> {hv[0]} at {hv[1]}")
    for f, bd, hd in moved_d:
        print(f"  {name}/{f}: errors {bd} -> {hd}")
    print(f"{name}: {files} file(s) in {len(h)} run(s); {len(moved_v)} verdict(s) moved, {len(moved_d)} error set(s) moved")
print(f"private downstream trees ({len(private & set(H))} of them): {agg[0]} file(s) in {agg[1]} run(s); {agg[2]} verdict(s) moved, {agg[3]} error set(s) moved")
print(f"TOTAL: {tot[0]} run(s); {tot[1]} verdict(s) moved; {tot[2]} error set(s) moved")
