"""Where the compiler and lupin differ on W0801 (and on the select E0806),
file by file.

usage: python3 -I w0801-diff.py WOLF LUPIN STD_ROOT DIR...

Runs every `.lu` file under each DIR (skipping target/, .git/ and vendored
upstream trees) through `WOLF conform-run --json --phase=typecheck` (both
W0801 halves: the resolver's binder half and the checker's match-arm half)
and `LUPIN conform-run --json --phase resolve` (lupin's warning walk), and
compares the W0801 span sets of the two records, and whether each verdict
names E0806. Prints one line per file that differs, then the totals: files
run, files with W0801 on either side, compiler-only spans, lupin-only spans,
E0806 disagreements. Neither side's program runs; both stop at a static rung.
"""
import json
import os
import subprocess
import sys

wolf, lupin, std_root, *dirs = sys.argv[1:]
SKIP = {"target", ".git", "node_modules"}


def record(cmd):
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=120)
    except subprocess.TimeoutExpired:
        return None
    try:
        return json.loads(p.stdout)
    except Exception:
        return None


def w0801(rec):
    return {tuple(w["span"]) for w in (rec or {}).get("warnings", []) if w.get("code") == "W0801"}


def e0806(rec):
    return "E0806" in str((rec or {}).get("verdict", ""))


grand = {"files": 0, "either": 0, "wolf_only": 0, "lupin_only": 0, "e0806": 0, "no_record": 0}
for d in dirs:
    files = []
    for root, subdirs, names in os.walk(d):
        subdirs[:] = sorted(s for s in subdirs if s not in SKIP and not (s == "upstream" and os.path.basename(root) == "vendor"))
        files.extend(os.path.join(root, n) for n in sorted(names) if n.endswith(".lu"))
    for f in files:
        rel = os.path.relpath(f, d)
        rw = record([wolf, "conform-run", "--json", "--phase=typecheck", "--std-root", std_root, f])
        rl = record([lupin, "conform-run", "--json", "--phase", "resolve", "--std-root", std_root, f])
        grand["files"] += 1
        if rw is None or rl is None:
            grand["no_record"] += 1
            print(f"NO-RECORD {rel}: wolf={'ok' if rw else 'none'} lupin={'ok' if rl else 'none'}", flush=True)
        sw, sl = w0801(rw), w0801(rl)
        if sw or sl:
            grand["either"] += 1
        only_w, only_l = sorted(sw - sl), sorted(sl - sw)
        grand["wolf_only"] += len(only_w)
        grand["lupin_only"] += len(only_l)
        if only_w or only_l:
            print(f"DIFF {rel}: compiler-only {only_w} lupin-only {only_l}"
                  f" (compiler {(rw or {}).get('verdict')} at {(rw or {}).get('phase_reached')},"
                  f" lupin {(rl or {}).get('verdict')} at {(rl or {}).get('phase_reached')})", flush=True)
        elif sw:
            print(f"SAME {rel}: {sorted(sw)}", flush=True)
        if e0806(rw) != e0806(rl):
            grand["e0806"] += 1
            print(f"E0806 {rel}: compiler {(rw or {}).get('verdict')} lupin {(rl or {}).get('verdict')}", flush=True)
print("TOTAL " + " ".join(f"{k}={v}" for k, v in grand.items()))
