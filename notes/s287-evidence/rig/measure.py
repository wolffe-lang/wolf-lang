"""s287: one conform-run rung over checkouts, on one binary (adapted by copy
from s285's measure-org.py).

usage: python3 -I measure.py WOLF STD_ROOT PHASE CODES OUT.json JOBS DIR...

Runs `WOLF conform-run --json --phase=PHASE --std-root STD_ROOT FILE`, JOBS
at a time, over each DIR (skipping target/, .git/ and vendored upstream
trees). A directory is a module (D59): a file with no `//! check:` header is
a member, and conform-run on any member compiles the whole directory module,
so the members of one directory run ONCE, through the first of them; a file
with its own `check:` header is an entry and runs on its own. Each run keeps
its verdict, its phase reached, and every ERROR diagnostic whose code is in
CODES (comma-separated; `*` keeps every error code), attributed to its file
through the record's file table. Run with the base binary and with a head's,
the two files compared run by run (compare.py) show every verdict and every
diagnostic the change moved.
"""
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

wolf, std_root, phase, codes, out_path, jobs, *dirs = sys.argv[1:]
CODES = None if codes == "*" else set(codes.split(","))
SKIP = {"target", ".git", "node_modules"}


def is_entry(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                if not line.startswith("//!"):
                    return False
                if line.startswith("//! check:"):
                    return True
    except OSError:
        return False
    return False


def run(f):
    try:
        p = subprocess.run([wolf, "conform-run", "--json", f"--phase={phase}", "--std-root", std_root, f],
                           capture_output=True, timeout=300)
    except subprocess.TimeoutExpired:
        return f, "timeout", "?", []
    try:
        rec = json.loads(p.stdout)
    except Exception:
        return f, "no-record", "?", []
    table = rec.get("files") or [rec.get("file", f)]
    diags = []
    for d in rec.get("diagnostics", []):
        if d.get("severity") == "warning":
            continue
        if CODES is not None and d.get("code") not in CODES:
            continue
        i = d.get("file", 0)
        where = table[i] if isinstance(i, int) and i < len(table) else f
        diags.append([d.get("code"), os.path.basename(str(where)), d.get("span")])
    return f, rec.get("verdict", "?"), rec.get("phase_reached", "?"), sorted(diags)


result = {}
for d in dirs:
    files = []
    runs = []
    for root, subdirs, names in os.walk(d):
        subdirs[:] = sorted(s for s in subdirs if s not in SKIP and not (s == "upstream" and os.path.basename(root) == "vendor"))
        here = [os.path.join(root, n) for n in sorted(names) if n.endswith(".lu")]
        files.extend(here)
        members = [f for f in here if not is_entry(f)]
        runs.extend(f for f in here if is_entry(f))
        runs.extend(members[:1])
    per = {}
    verdicts = {}
    with ThreadPoolExecutor(max_workers=int(jobs)) as pool:
        for f, v, ph, diags in pool.map(run, sorted(runs)):
            per[os.path.relpath(f, d)] = {"verdict": v, "phase": ph, "diags": diags}
            verdicts[v] = verdicts.get(v, 0) + 1
    result[d] = {"files": len(files), "runs": len(runs), "verdicts": verdicts, "per": per}
    print(f"{d}: {len(files)} file(s) in {len(runs)} run(s); verdicts {dict(sorted(verdicts.items()))}", flush=True)
with open(out_path, "w") as out:
    json.dump(result, out, indent=1, sort_keys=True)
