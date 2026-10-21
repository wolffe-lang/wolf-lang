#!/usr/bin/env python3
"""tl30: every red macOS job in the cached runs (the last 60 runs per repo in
which a macOS job ran), against its Linux counterpart in the same run attempt
(the same job with the host swapped; for wolf-lang the linux gauntlet). Prints
one TSV row a red macOS job: run, created, event, branch, attempt, job id,
job, conclusion, counterpart, counterpart's conclusion, and `MAC-ONLY` when
the counterpart ran green (or `NO-COUNTERPART` when none ran).
    macpairs.py CACHE [REPO...]"""
import json, os, sys, glob, re
from collections import defaultdict
cache = sys.argv[1]; only = sys.argv[2:]
def osof(j):
    lab = " ".join(j.get("labels") or []).lower()
    return "macos" if "macos" in lab else "windows" if "windows" in lab else "linux"
def counterpart(repo, name):
    if repo == "wolf-lang" and (name.startswith("native gauntlet") or name.startswith("build + test (macos")):
        return ["lint + test + corpus (linux x86-64)"]
    if repo == "lobo" and name == "gauntlet-macos":
        return ["gauntlet"]
    if repo == "lope": return ["linux x86-64 (every gate)"]
    if repo == "aspen": return [None]
    n = name.replace("macos-latest", "ubuntu-latest")
    c = [n]
    if repo == "boreutils": c = [n.replace(", g)", ")"), n.replace(", g)", ", )")]
    if repo == "yew" and name.startswith("oracle"): c = [n.replace("--cc clang", "--cc clang --cc gcc")]
    return c
print("\t".join("repo run created event branch attempt job_id job conclusion counterpart counterpart_conclusion verdict".split()))
for f in sorted(glob.glob(os.path.join(cache, "runs-*.json"))):
    repo = os.path.basename(f)[5:-5]
    if only and repo not in only: continue
    runs = [r for r in json.load(open(f)) if any(osof(j) == "macos" and j.get("runner_name") for j in r["jobs"])][:60]
    for r in runs:
        byatt = defaultdict(list)
        for j in r["jobs"]:
            byatt[j.get("run_attempt") or 1].append(j)
        for att, js in sorted(byatt.items()):
            red = lambda j: j.get("conclusion") in ("failure", "timed_out") or (j.get("conclusion") == "cancelled" and j.get("runner_name") and r["conclusion"] != "cancelled")
            for m in [j for j in js if osof(j) == "macos" and red(j)]:
                cands = counterpart(repo, m["name"])
                cp = [j for j in js if j["name"] in cands and j.get("runner_name")]
                if not cp:
                    # a re-run attempt carries the counterpart over from an earlier attempt
                    cp = [j for a2, js2 in byatt.items() if a2 < att for j in js2 if j["name"] in cands and j.get("runner_name")][-1:]
                if cp:
                    c = cp[0]; v = "MAC-ONLY" if c["conclusion"] == "success" else "both"
                    cn, cc = c["name"], c["conclusion"]
                else:
                    cn, cc, v = "-", "-", "NO-COUNTERPART"
                print("\t".join(str(x) for x in (repo, r["id"], r["created_at"][:16], r["event"], r["head_branch"], att, m["id"], m["name"], m["conclusion"], cn, cc, v)))
