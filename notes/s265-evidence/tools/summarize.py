#!/usr/bin/env python3
"""s265: medians and spreads of a rows TSV (label, ns columns; any mode)."""
import csv, statistics, sys
rows = list(csv.DictReader(open(sys.argv[1]), delimiter='\t'))
key = lambda r: (r.get('mode', ''), r['label'])
groups = {}
for r in rows:
    groups.setdefault(key(r), []).append(int(r['ns']))
def q(s, p):
    s = sorted(s); x = p * (len(s) - 1); lo = int(x); f = x - lo
    return s[lo] if lo + 1 >= len(s) else s[lo] * (1 - f) + s[lo + 1] * f
print('mode\tlabel\tn\tmedian_us\tp10_us\tp90_us\tmin_us\tmax_us\tspread')
meds = {}
for (m, l), v in groups.items():
    med = statistics.median(v); meds[(m, l)] = med
    print(f"{m}\t{l}\t{len(v)}\t{med/1e3:.1f}\t{q(v,.1)/1e3:.1f}\t{q(v,.9)/1e3:.1f}\t{min(v)/1e3:.1f}\t{max(v)/1e3:.1f}\t{(q(v,.9)-q(v,.1))/med:.3f}")
