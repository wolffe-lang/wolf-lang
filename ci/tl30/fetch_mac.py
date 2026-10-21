#!/usr/bin/env python3
"""tl30: per repo, the most recent completed runs of the workflows that hold macOS
jobs, each with every attempt's jobs (filter=all), until N runs in which a macOS
job ran are in hand. Cached per run once completed.
    fetch_mac.py CACHE REPO WORKFLOW_FILE[,WORKFLOW_FILE] [N] [MAXFETCH]"""
import json, os, subprocess, sys, time
ORG = "wolffe-lang"
def gh(path):
    for attempt in range(4):
        r = subprocess.run(["gh", "api", path], capture_output=True, text=True)
        if r.returncode == 0:
            return json.loads(r.stdout)
        sys.stderr.write("gh api %s failed (%s), retry\n" % (path, r.stderr.strip()[:200]))
        time.sleep(5 * (attempt + 1))
    raise SystemExit("gh api failed: " + path)
cache, repo, wfs = sys.argv[1], sys.argv[2], sys.argv[3].split(",")
N = int(sys.argv[4]) if len(sys.argv) > 4 else 60
MAXF = int(sys.argv[5]) if len(sys.argv) > 5 else 160
os.makedirs(os.path.join(cache, "jobs"), exist_ok=True)
runs = []
for wf in wfs:
    for page in (1, 2, 3):
        d = gh("repos/%s/%s/actions/workflows/%s/runs?per_page=100&page=%d&status=completed" % (ORG, repo, wf, page))
        got = d.get("workflow_runs", [])
        runs += [{k: r.get(k) for k in ("id", "name", "path", "event", "status", "conclusion", "created_at", "run_started_at", "updated_at", "run_attempt", "head_branch", "head_sha", "display_title")} for r in got]
        if len(got) < 100:
            break
runs.sort(key=lambda r: r["created_at"], reverse=True)
def mac(j):
    return "macos" in " ".join(j.get("labels") or []).lower()
from concurrent.futures import ThreadPoolExecutor
def jobs_for(r):
    jf = os.path.join(cache, "jobs", "%s-%d.json" % (repo, r["id"]))
    if os.path.exists(jf):
        c = json.load(open(jf))
        if c.get("run_attempt") == r["run_attempt"]:
            return c, 0
    jobs, page = [], 1
    while True:
        d = gh("repos/%s/%s/actions/runs/%d/jobs?per_page=100&page=%d&filter=all" % (ORG, repo, r["id"], page))
        jobs += [{k: j.get(k) for k in ("id", "name", "status", "conclusion", "created_at", "started_at", "completed_at", "labels", "runner_name", "run_attempt")} for j in d["jobs"]]
        if len(d["jobs"]) < 100:
            break
        page += 1
    c = {"run_attempt": r["run_attempt"], "jobs": jobs}
    json.dump(c, open(jf + ".tmp", "w")); os.replace(jf + ".tmp", jf)
    return c, 1
out, nmac, fetched, i = [], 0, 0, 0
with ThreadPoolExecutor(6) as ex:
    while i < len(runs) and nmac < N and fetched < MAXF:
        batch = runs[i:i + 12]; i += 12
        for r, (c, f) in zip(batch, ex.map(jobs_for, batch)):
            fetched += f
            r["jobs"] = c["jobs"]
            if nmac < N:
                out.append(r)
                if any(mac(j) and j.get("runner_name") for j in r["jobs"]):
                    nmac += 1
json.dump(out, open(os.path.join(cache, "runs-%s.json" % repo), "w"))
print(repo, "listed", len(runs), "kept", len(out), "with a macOS job that ran", nmac, "fetched", fetched)
