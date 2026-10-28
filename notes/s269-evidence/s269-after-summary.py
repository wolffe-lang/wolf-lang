import collections
import csv
import json
from pathlib import Path
import sys
root = Path(sys.argv[1])
summary = {}
for tier in ['native', 'release']:
    lines = (root / f'after-{tier}.log').read_text().splitlines()
    rows = [l.split('\t') for l in lines if l.startswith(('package\t', 'pass\t', 'unit\t'))]
    per_phase = collections.defaultdict(float)
    per_unit_phase = collections.defaultdict(float)
    for scope, unit, phase, detail, count, seconds in rows:
        if scope == 'pass': per_phase[phase] += float(seconds)
        per_unit_phase[(scope, unit, phase)] += float(seconds)
    text_header = next(l for l in lines if l.startswith('wolf build timings:'))
    wall = float(text_header.split()[4].removesuffix('s'))
    hottest = {}
    for phase in ['midend/licm', 'midend/hdrprom', 'midend/rangeopt']:
        values = sorted((r for r in rows if r[0] == 'pass' and r[2] == phase), key=lambda r: -float(r[5]))
        hottest[phase] = dict(function_rows=len(values), seconds=per_phase[phase], top_two_fraction=(sum(float(r[5]) for r in values[:2]) / per_phase[phase] if values else 0), top=[dict(unit=r[1], function=r[3], count=int(r[4]), seconds=float(r[5])) for r in values[:10]])
    summary[tier] = dict(wall_seconds=wall, pass_seconds=dict(per_phase), hottest=hottest, contributions=[dict(scope=scope, unit=unit, phase=phase, seconds=seconds) for (scope, unit, phase), seconds in sorted(per_unit_phase.items())], interpretation='Nested package scopes and parallel elapsed contributions are not additive. Per-function pass cost includes whole-module analysis performed by that invocation; no loop attribution.')
(root / 's269-after-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
