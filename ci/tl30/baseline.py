#!/usr/bin/env python3
"""tl30: the before, per repo: the last N completed green runs of the merge
gate's shape (a labelled pull request's or a trunk push's `tier full`; yew,
untiered: every pull-request and push run), each run's macOS minutes, by job,
and its job-minutes and wall.
    baseline.py CACHE REPO [N]"""
import json, os, sys, statistics
from datetime import datetime
def ts(s): return datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ")
cache, repo = sys.argv[1], sys.argv[2]; N = int(sys.argv[3]) if len(sys.argv) > 3 else 10
runs = json.load(open(os.path.join(cache, "runs-%s.json" % repo)))
def osof(j):
    lab = " ".join(j.get("labels") or []).lower()
    return "macos" if "macos" in lab else "windows" if "windows" in lab else "linux"
def tier(r):
    for j in r["jobs"]:
        if (j["name"] or "").startswith("tier ") and j.get("run_attempt") == r["run_attempt"]:
            return j["name"]
    return None
sel = []
for r in runs:
    if r["conclusion"] != "success" or r["event"] not in ("pull_request", "push"): continue
    t = tier(r)
    if repo == "yew" or t == "tier full":
        sel.append(r)
sel = sel[:N]
rows = []
for r in sel:
    js = [j for j in r["jobs"] if j.get("run_attempt") == r["run_attempt"] and j.get("runner_name") and j.get("started_at") and j.get("completed_at")]
    mac = {j["name"]: (ts(j["completed_at"]) - ts(j["started_at"])).total_seconds() / 60 for j in js if osof(j) == "macos"}
    jm = sum((ts(j["completed_at"]) - ts(j["started_at"])).total_seconds() / 60 for j in js)
    wall = (max(ts(j["completed_at"]) for j in js) - ts(r["created_at"])).total_seconds() / 60
    mw = [(ts(j["started_at"]) - ts(j["created_at"])).total_seconds() / 60 for j in js if osof(j) == "macos"]
    rows.append((r, mac, jm, wall, mw))
    print("%s %s %-12s %-6s mac %6.1f (%d jobs, slot wait max %5.1f)  job-min %6.1f  wall %6.1f | %s" % (
        r["id"], r["created_at"][:16], r["event"][:12], r["head_branch"][:6], sum(mac.values()), len(mac), max(mw) if mw else 0, jm, wall,
        "; ".join("%s %.1f" % (k.replace("macos-latest", "mac")[:34], v) for k, v in sorted(mac.items()))))
if rows:
    med = lambda xs: statistics.median(xs)
    print("median over %d: macOS-min %.1f, macOS jobs %d, job-min %.1f, wall %.1f; runs %s .. %s" % (
        len(rows), med([sum(x[1].values()) for x in rows]), int(med([len(x[1]) for x in rows])), med([x[2] for x in rows]), med([x[3] for x in rows]), rows[-1][0]["id"], rows[0][0]["id"]))
    names = sorted({k for x in rows for k in x[1]})
    for n in names:
        v = [x[1][n] for x in rows if n in x[1]]
        print("   %-50s median %5.1f  (%d runs, min %.1f max %.1f)" % (n, med(v), len(v), min(v), max(v)))
