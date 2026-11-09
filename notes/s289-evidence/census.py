#!/usr/bin/env python3
"""s289: census every ambient name for the "a std/prelude stub without a
signature" refusal, on the checked machine (conform-run --json --checked).

usage: census.py <wolf> <outdir>
Three uses per name: a value (`let f = NAME`), a call (`NAME()`), and a
member receiver (`NAME.member()`), each its own one-file program in its own
directory. Prints one TSV line per (name, use): name, kind, use, verdict,
construct-or-code. A summary at the end counts the stub refusals.
"""
import json
import os
import subprocess
import sys

wolf, out = sys.argv[1], sys.argv[2]
names = json.loads(subprocess.run([wolf, "prelude", "--json"], capture_output=True,
                                  check=True).stdout)["names"]
USES = {
    "value": "fn main() -> int {{\n    let f = {n}\n    0\n}}\n",
    "call": "fn main() -> int {{\n    {n}()\n    0\n}}\n",
    "member": "fn main() -> int {{\n    {n}.member()\n    0\n}}\n",
}
STUB = "a std/prelude stub without a signature"
os.makedirs(out, exist_ok=True)
stub = []
rows = 0
for row in names:
    n, kind = row["name"], row["kind"]
    for use, tmpl in USES.items():
        d = os.path.join(out, f"{n}.{use}")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "main.lu"), "w") as f:
            f.write(tmpl.format(n=n))
        r = subprocess.run([wolf, "conform-run", "main.lu", "--json", "--checked"], cwd=d,
                           capture_output=True, text=True)
        try:
            rec = json.loads(r.stdout)
        except ValueError:
            print(f"{n}\t{kind}\t{use}\tNO-RECORD\t{r.stderr.strip()[:120]}")
            continue
        verdict = rec.get("verdict", "")
        what = rec.get("x-unsupported-construct") or ",".join(
            dd["code"] for dd in rec.get("diagnostics", []) if dd.get("severity") == "error")
        rows += 1
        print(f"{n}\t{kind}\t{use}\t{verdict}\t{what}")
        if what == STUB:
            stub.append((n, kind, use))
print(f"# census: {len(names)} names, {rows} uses, {len(stub)} answer \"{STUB}\"")
by = {}
for n, kind, use in stub:
    by.setdefault(use, []).append(n)
for use, ns in by.items():
    print(f"# stub {use}: {len(ns)}: {' '.join(ns)}")
