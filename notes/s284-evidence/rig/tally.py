#!/usr/bin/env python3
"""s284: every RUN line of the reproduction and proof logs, totalled by job,
condition and binary (the commit, the cargo profile, the wolf tier), with
the median wall time of the green runs. Reads the logs named on the command
line; prints a markdown table."""
import re
import statistics
import sys
from collections import OrderedDict

RUN = re.compile(r"^RUN (\S+) (\S+) (\S+) rc=(\S+) ms=(\d+)")
PAIR = re.compile(r"^PAIR (\S+) (\S+) sha256=")
SHAS = {
    "275f99040e778a703e9fedbfcc704683ec1f4b7b": "trunk 275f9904",
    "8e8ce00faa1a0b7acf96e9458aa5a62ef47ebbc9": "measurement 8e8ce00f",
    "bf843401dcf38395b2bb433ceb06330c0332cc14": "fix bf843401",
}


def main(paths):
    rows = OrderedDict()
    for path in paths:
        pairs = []
        job = None
        for line in open(path, encoding="utf-8", errors="replace"):
            m = PAIR.match(line)
            if m:
                cond, binpath = m[1], m[2]
                sha = binpath.split("/")[-2]
                pairs.append((cond, SHAS.get(sha, sha[:8]), binpath.split("/")[-1]))
                continue
            m = RUN.match(line)
            if not m:
                continue
            tag = m[1]
            job = tag.split("-r")[0]
            idx = int(tag.rsplit("-p", 1)[1])
            cond, commit, binary = pairs[idx]
            key = (job, cond, commit, binary)
            r = rows.setdefault(key, {"runs": 0, "green": 0, "stalled": 0, "other": 0, "ms": []})
            r["runs"] += 1
            rc = m[4]
            if rc == "0":
                r["green"] += 1
                r["ms"].append(int(m[5]))
            elif rc == "124":
                r["stalled"] += 1
            else:
                r["other"] += 1
    print("| job | condition | commit | binary | runs | green | stalled (rc=124) | other | median green |")
    print("|---|---|---|---|---|---|---|---|---|")
    for (job, cond, commit, binary), r in rows.items():
        med = f"{statistics.median(r['ms']) / 1000:.2f} s" if r["ms"] else "—"
        print(f"| {job} | {cond} | {commit} | `{binary}` | {r['runs']} | {r['green']} | "
              f"{r['stalled']} | {r['other']} | {med} |")


if __name__ == "__main__":
    main(sys.argv[1:])
