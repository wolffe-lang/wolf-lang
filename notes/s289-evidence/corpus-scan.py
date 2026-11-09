#!/usr/bin/env python3
"""s289: every corpus entry's checked-machine record, for the two refusals
this lane retires. usage: corpus-scan.py <wolf> <corpus-dir> <out.tsv>
Prints file, verdict, phase_reached and the construct (or the error codes).
An entry is a file with a `//! check:` or `//! phase:` header line; members
are compiled through their module's entry and skipped here."""
import json
import os
import subprocess
import sys

wolf, corpus, out = sys.argv[1], sys.argv[2], sys.argv[3]
files = []
for root, _, names in os.walk(corpus):
    for n in names:
        if n.endswith(".lu"):
            p = os.path.join(root, n)
            with open(p, encoding="utf-8", errors="replace") as f:
                head = f.read(4096)
            if "//! check:" in head or "//! phase:" in head:
                files.append(p)
files.sort()
with open(out, "w") as o:
    for p in files:
        cmd = [wolf, "conform-run", p, "--json", "--checked"]
        root = os.path.join(os.path.dirname(p), "std")
        r = subprocess.run(cmd, capture_output=True, text=True, cwd=os.path.dirname(corpus))
        try:
            rec = json.loads(r.stdout)
        except ValueError:
            o.write(f"{p}\tNO-RECORD\t-\t{r.stderr.strip()[:100]}\n")
            continue
        what = rec.get("x-unsupported-construct") or ",".join(
            d["code"] for d in rec.get("diagnostics", []) if d.get("severity") == "error")
        o.write(f"{p}\t{rec.get('verdict')}\t{rec.get('phase_reached')}\t{what}\n")
print(f"{len(files)} entries scanned")
