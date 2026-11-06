"""Every new W0801 (binder half) and E0806 (select arm) across checkouts.

usage: python3 -I measure-org.py WOLF STD_ROOT OUT.json DIR...

Runs `WOLF conform-run --json --phase=resolve --std-root STD_ROOT FILE` on
every `.lu` file under each DIR (skipping target/, .git/ and vendored
upstream trees), and records, per DIR, the files run, the verdicts, and every
W0801 and E0806 with the file it lies in and its span. Both codes are
resolve-rung in the binder half; the checker's match-arm W0801 never runs at
`--phase=resolve`, so a base binary reports none of either by construction and
a head binary reports exactly the new ones. Prints one summary line per DIR.
"""
import json
import os
import subprocess
import sys

wolf, std_root, out_path, *dirs = sys.argv[1:]
SKIP = {"target", ".git", "node_modules"}
result = {}
for d in dirs:
    files = []
    for root, subdirs, names in os.walk(d):
        subdirs[:] = sorted(s for s in subdirs if s not in SKIP and not (s == "upstream" and os.path.basename(root) == "vendor"))
        files.extend(os.path.join(root, n) for n in sorted(names) if n.endswith(".lu"))
    seen = set()
    hits = []
    verdicts = {}
    for f in files:
        try:
            p = subprocess.run([wolf, "conform-run", "--json", "--phase=resolve", "--std-root", std_root, f],
                               capture_output=True, timeout=120)
        except subprocess.TimeoutExpired:
            verdicts["timeout"] = verdicts.get("timeout", 0) + 1
            continue
        try:
            rec = json.loads(p.stdout)
        except Exception:
            verdicts["no-record"] = verdicts.get("no-record", 0) + 1
            continue
        v = rec.get("verdict", "?")
        verdicts[v] = verdicts.get(v, 0) + 1
        table = rec.get("files") or [rec.get("file", f)]
        for diag in rec.get("diagnostics", []):
            code = diag.get("code")
            if code not in ("W0801", "E0806"):
                continue
            where = table[diag.get("file", 0)] if diag.get("file", 0) < len(table) else f
            key = (code, os.path.relpath(os.path.join(os.path.dirname(f), where) if not os.path.isabs(where) else where, d), tuple(diag["span"]))
            if key in seen:
                continue
            seen.add(key)
            hits.append({"code": code, "file": key[1], "span": list(key[2]), "entry": os.path.relpath(f, d)})
    result[d] = {"files": len(files), "verdicts": verdicts, "hits": hits}
    print(f"{d}: {len(files)} file(s), {sum(1 for h in hits if h['code'] == 'W0801')} W0801, "
          f"{sum(1 for h in hits if h['code'] == 'E0806')} E0806; verdicts {dict(sorted(verdicts.items()))}", flush=True)
with open(out_path, "w") as out:
    json.dump(result, out, indent=1, sort_keys=True)
