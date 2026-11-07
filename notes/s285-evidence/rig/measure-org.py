"""s285: every E0415 and E0207 across checkouts, on two binaries (adapted by
copy from s282's measure-org.py).

usage: python3 -I measure-org.py WOLF STD_ROOT OUT.json JOBS DIR...

Runs `WOLF conform-run --json --phase=typecheck --std-root STD_ROOT FILE`,
JOBS at a time, over each DIR (skipping target/, .git/ and vendored
upstream trees). A directory is a module (D59): a file with no `//! check:`
header is a member, and conform-run on any member compiles the whole
directory module, so the members of one directory run ONCE, through the
first of them; a file with its own `check:` header is an entry and runs on
its own. Each diagnostic is attributed to its file through the record's
file table. E0415 is the checker's (typecheck rung) and E0207 the parser's,
so the typecheck rung sees both; a module refused before the checker is
refused alike on both binaries, and the per-file syntactic census covers
its files. Run with the base binary and with the head's, the two files
compared run by run (compare.py) show every verdict and every diagnostic
the change moved. Prints one summary line per DIR.
"""
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

wolf, std_root, out_path, jobs, *dirs = sys.argv[1:]
SKIP = {"target", ".git", "node_modules"}
CODES = ("E0415", "E0207")


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
        p = subprocess.run([wolf, "conform-run", "--json", "--phase=typecheck", "--std-root", std_root, f],
                           capture_output=True, timeout=300)
    except subprocess.TimeoutExpired:
        return f, "timeout", []
    try:
        rec = json.loads(p.stdout)
    except Exception:
        return f, "no-record", []
    table = rec.get("files") or [rec.get("file", f)]
    diags = []
    for d in rec.get("diagnostics", []):
        if d.get("code") not in CODES:
            continue
        i = d.get("file", 0)
        where = table[i] if isinstance(i, int) and i < len(table) else f
        diags.append([d.get("code"), os.path.basename(str(where)), d.get("span")])
    return f, rec.get("verdict", "?"), sorted(diags)


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
        for f, v, diags in pool.map(run, sorted(runs)):
            per[os.path.relpath(f, d)] = {"verdict": v, "diags": diags}
            verdicts[v] = verdicts.get(v, 0) + 1
    result[d] = {"files": len(files), "runs": len(runs), "verdicts": verdicts, "per": per}
    n415 = sum(1 for x in per.values() for c, *_ in x["diags"] if c == "E0415")
    n207 = sum(1 for x in per.values() for c, *_ in x["diags"] if c == "E0207")
    print(f"{d}: {len(files)} file(s) in {len(runs)} run(s), {n415} E0415, {n207} E0207; "
          f"verdicts {dict(sorted(verdicts.items()))}", flush=True)
with open(out_path, "w") as out:
    json.dump(result, out, indent=1, sort_keys=True)
