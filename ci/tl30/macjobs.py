#!/usr/bin/env python3
"""tl30: per repo, every macOS job name: runs it ran in (by the run's tier),
median and p90 minutes, and the macOS minutes a run of each tier spends.
    macjobs.py CACHE [--since ISO] [REPO...]"""
import json, os, sys, glob, statistics
from collections import defaultdict
from datetime import datetime, timezone
def ts(s): return datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
args = sys.argv[1:]; cache = args.pop(0); since = "0"
if args[:1] == ["--since"]: since = args[1]; args = args[2:]
def osof(j):
    lab = " ".join(j.get("labels") or []).lower()
    return "macos" if "macos" in lab else "windows" if "windows" in lab else "linux"
def tier(r):
    att = r.get("run_attempt") or 1
    for j in r["jobs"]:
        if (j["name"] or "").startswith("tier ") and (j.get("run_attempt") or 1) == att:
            return "(no verdict)" if "${{" in j["name"] else j["name"]
    return "untiered:" + r["event"]
for f in sorted(glob.glob(os.path.join(cache, "runs-*.json"))):
    repo = os.path.basename(f)[5:-5]
    if args and repo not in args: continue
    runs = [r for r in json.load(open(f)) if r["created_at"] >= since]
    per = defaultdict(list); pertier = defaultdict(list)
    for r in runs:
        att = r.get("run_attempt") or 1
        t = tier(r) + " (" + r["event"] + ")"
        tot = 0
        for j in r["jobs"]:
            if (j.get("run_attempt") or 1) != att or osof(j) != "macos" or not j.get("runner_name") or not j.get("started_at") or not j.get("completed_at"): continue
            m = (ts(j["completed_at"]) - ts(j["started_at"])).total_seconds() / 60
            per[(j["name"])].append((m, t, j["conclusion"])); tot += m
        if r["conclusion"] in ("success", "failure"):
            pertier[t].append(tot)
    print("=== %s (%d runs)" % (repo, len(runs)))
    for t, v in sorted(pertier.items()):
        v.sort(); print("  tier %-38s runs %3d  macOS-min/run median %6.1f  max %6.1f  sum %7.1f" % (t, len(v), v[len(v)//2], v[-1], sum(v)))
    for n, v in sorted(per.items()):
        ms = sorted(x[0] for x in v); ok = [x[0] for x in v if x[2] == "success"]; ok.sort()
        tiers = defaultdict(int)
        for x in v: tiers[x[1]] += 1
        print("  job %-55s ran %3d  median %5.1f  ok-median %5.1f  p90 %5.1f  | %s" % (n[:55], len(v), ms[len(ms)//2], ok[len(ok)//2] if ok else 0, ms[int(0.9*(len(ms)-1))], ", ".join("%s:%d" % kv for kv in sorted(tiers.items()))))
