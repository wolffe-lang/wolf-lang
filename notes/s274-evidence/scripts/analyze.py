#!/usr/bin/env python3
"""s274: distributions from the hasu batch logs (S274_ROW, S274_PROBE, S274_SIB*, S274_HEAD).

usage: analyze.py <log> [<log> ...]

Every statistic is printed as n, median, p05..p95 (the spread), min and max (the worst).
  - the row (S274_ROW): the trunk row's own statistic per leg, max(0, b - a) x 1024 / 20000
    B/turn (growth_per_turn at 7332772f), and its red rate per run;
  - the probe (S274_PROBE): per variant and tier, the same cross-process statistic from the
    probe's final VmHWM (unclipped: b - a in kB), and the in-process differences between the
    readings at 20,000 and 40,000 turns of the 40,000-turn process, for VmHWM, VmRSS,
    smaps_rollup Rss and Anonymous + Swap (kB, and B/turn);
  - the head (S274_HEAD): the head row's per-leg statistic and red rate;
  - the siblings (S274_SIBLINE): every REPORT figure per row, tier and mode.
"""
import collections
import re
import statistics
import sys


def q(xs, p):
    xs = sorted(xs)
    if not xs:
        return float("nan")
    k = (len(xs) - 1) * p
    lo, hi = int(k), min(int(k) + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)


def dist(xs, fmt="{:.1f}"):
    if not xs:
        return "n=0"
    f = fmt.format
    return (f"n={len(xs)} median={f(statistics.median(xs))} p05..p95={f(q(xs, .05))}..{f(q(xs, .95))} "
            f"min={f(min(xs))} max={f(max(xs))}")


def kv(text):
    return dict(re.findall(r"(\w+)=(\S*)", text))


rows = collections.defaultdict(list)       # (label, rig) -> [(exit, [(a, b), ...])]
probe = collections.defaultdict(dict)      # (label, rig, v, tier, m) -> {n: (marks, hwm_end)}
head = collections.defaultdict(list)       # (label, rig) -> [(exit, [(tier, g, a, b, h1, h2)])]
sib = collections.defaultdict(list)        # (rig, key) -> [value]
sib_exit = collections.defaultdict(list)   # (rig, bin, row) -> [exit]

for path in sys.argv[1:]:
    for line in open(path, errors="replace"):
        line = line.rstrip("\n")
        if line.startswith("S274_ROW "):
            d = kv(line)
            legs = [tuple(int(x) for x in p.split(":")) for p in d.get("legs", "").split(",") if p]
            rows[(d["label"], d["rig"])].append((int(d["exit"]), legs))
        elif line.startswith("S274_PROBE "):
            d = kv(line.split(" mark ")[0])
            marks = {}
            for m in re.finditer(r"mark i=(\d+) hwm=(\S+) rss=(\S+) srss=(\S+) anon=(\S+) swap=(\S+)", line):
                i, *vals = m.groups()
                marks[int(i)] = tuple(int(v) if v.isdigit() else None for v in vals)
            end = re.search(r" hwm=(\d+)$", line)
            probe[(d["label"], d["rig"], d["v"], d["tier"], d["m"])][int(d["n"])] = (
                marks, int(end.group(1)) if end else None, int(d["exit"]))
        elif line.startswith("S274_HEAD "):
            d = kv(line.split(" legs=")[0])
            legs = []
            for m in re.finditer(r"(native|release):(-?[\d.]+):(\d+):(\d+):(\d+):(\d+)", line):
                t, g, a, b, h1, h2 = m.groups()
                legs.append((t, float(g), int(a), int(b), int(h1), int(h2)))
            head[(d["label"], d["rig"])].append((int(d["exit"]), legs))
        elif line.startswith("S274_SIB "):
            d = kv(line)
            sib_exit[(d["rig"], d["bin"], d["row"])].append(int(d["exit"]))
        elif "S274_SIBLINE" in line:
            rig = re.search(r"rig=(\S+)", line).group(1)
            body = line.split(" s=", 1)[1].split(" ", 1)[1]
            m = re.match(r"REPORT (\S+) (\S+) (\S+?)(?: \(cwd \d+ bytes\))?: .*?(-?\d+) B (?:a turn|an entry)", body)
            if m:
                sib[(rig, f"{m.group(1)} {m.group(2)} {m.group(3)}")].append(int(m.group(4)))
                nums = re.findall(r"(\d+) turns (\d+) (B|KB)", body)
                if len(nums) == 2:
                    unit = 1024 if nums[0][2] == "KB" else 1
                    diff_kb = (int(nums[1][1]) - int(nums[0][1])) * unit / 1024
                    sib[(rig, f"{m.group(1)} {m.group(2)} {m.group(3)} peak-diff-kB")].append(diff_kb)
                continue
            m = re.match(r"(copy_str|no_region): VmHWM (\d+) kB at 20000 turns, (\d+) kB at 40000", body)
            if m:
                g = max(0, int(m.group(3)) - int(m.group(2))) * 1024 / 20000
                sib[(rig, f"{m.group(1)} B/turn")].append(g)

if rows:
    print("## The trunk row (cross-process VmHWM, growth_per_turn at 7332772f)")
    for (label, rig), runs in sorted(rows.items()):
        red = sum(1 for e, _ in runs if e != 0)
        for i, tier in enumerate(("native", "release")):
            legs = [l[i] for _, l in runs if len(l) > i]
            g = [max(0, b - a) * 1024 / 20000 for a, b in legs]
            diff = [b - a for a, b in legs]
            print(f"{label} {rig} {tier}: B/turn {dist(g)} | b-a kB {dist(diff, '{:.0f}')} | >=4: {sum(1 for x in g if x >= 4)}")
        print(f"{label} {rig}: red {red}/{len(runs)} runs ({100 * red / max(1, len(runs)):.0f} %)")
    print()

if probe:
    print("## The probe (trunk wolf; one witness per variant and tier)")
    groups = collections.defaultdict(lambda: collections.defaultdict(list))
    for (label, rig, v, tier, m), byn in probe.items():
        g = groups[(label, rig, v, tier)]
        if 20000 in byn and 40000 in byn and byn[20000][1] and byn[40000][1]:
            a, b = byn[20000][1], byn[40000][1]
            g["xproc VmHWM b-a kB"].append(b - a)
            g["xproc row B/turn (clipped)"].append(max(0, b - a) * 1024 / 20000)
        if 40000 in byn:
            marks = byn[40000][0]
            if 20000 in marks and 40000 in marks:
                for j, name in enumerate(("VmHWM", "VmRSS", "smaps Rss")):
                    x, y = marks[20000][j], marks[40000][j]
                    if x is not None and y is not None:
                        g[f"inproc {name} kB"].append(y - x)
                x = marks[20000][3] + (marks[20000][4] or 0)
                y = marks[40000][3] + (marks[40000][4] or 0)
                g["inproc anon+swap kB"].append(y - x)
                g["inproc anon+swap B/turn"].append((y - x) * 1024 / 20000)
        if 20000 in byn:
            marks = byn[20000][0]
            if 10000 in marks and 20000 in marks:
                x = marks[10000][3] + (marks[10000][4] or 0)
                y = marks[20000][3] + (marks[20000][4] or 0)
                g["inproc anon+swap kB, 10k->20k, no warm-up"].append(y - x)
                g["inproc VmHWM kB, 10k->20k, no warm-up"].append(marks[20000][0] - marks[10000][0])
    for key in sorted(groups):
        print(" ".join(key))
        for stat, xs in sorted(groups[key].items()):
            print(f"    {stat}: {dist(xs)}")
    print()
    # The mechanism: how much each reading differs from PROCESS to process at the same
    # turn (20,000), for the flat witness. VmHWM = smaps Rss = Anonymous + file-backed.
    print("## Across processes at turn 20,000, the flat witness (kB): what the noise is made of")
    across = collections.defaultdict(lambda: collections.defaultdict(list))
    for (label, rig, v, tier, m), byn in probe.items():
        if v != "flat":
            continue
        for n, (marks, _, _) in byn.items():
            if 20000 in marks and None not in marks[20000]:
                hwm, rss, srss, anon, swap = marks[20000]
                a2 = across[(label, rig, tier)]
                a2["VmHWM"].append(hwm)
                a2["smaps Rss"].append(srss)
                a2["Anonymous"].append(anon)
                a2["file-backed (Rss - Anonymous)"].append(srss - anon)
    for key in sorted(across):
        print(" ".join(key))
        for stat, xs in across[key].items():
            print(f"    {stat}: {dist(xs, '{:.0f}')} spread={max(xs) - min(xs)}")
    print()

if head:
    print("## The head row (one process, smaps_rollup Anonymous + Swap)")
    for (label, rig), runs in sorted(head.items()):
        red = sum(1 for e, _ in runs if e != 0)
        for tier in ("native", "release"):
            g = [x[1] for _, legs in runs for x in legs if x[0] == tier]
            hw = [x[5] - x[4] for _, legs in runs for x in legs if x[0] == tier]
            print(f"{label} {rig} {tier}: B/turn {dist(g, '{:.2f}')} | in-process VmHWM kB {dist(hw, '{:.0f}')}")
        print(f"{label} {rig}: red {red}/{len(runs)} runs ({100 * red / max(1, len(runs)):.0f} %)")
    print()

if sib:
    print("## The siblings (trunk; each row's own REPORT figure)")
    for key in sorted(sib):
        print(f"{' '.join(key)}: {dist(sib[key])}")
    for key in sorted(sib_exit):
        ex = sib_exit[key]
        print(f"{' '.join(key)}: red {sum(1 for e in ex if e)}/{len(ex)}")
