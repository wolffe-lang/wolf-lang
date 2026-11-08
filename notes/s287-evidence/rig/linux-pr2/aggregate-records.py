#!/usr/bin/env python3
"""Aggregate sequential same-head shard records without changing any END line.
Each original and the aggregate is checked by the independent recorder.
The first header/start and last footer/end retain the actual run interval;
source headers are preserved as indented provenance inside the first step.
"""
import re
import subprocess
import sys
from pathlib import Path

repo, head, recorder, output, *inputs = sys.argv[1:]
assert len(inputs) == 4, 'prep, build, gates and coverage records required'
headers = []
bodies = []
footers = []
for path in inputs:
    subprocess.run(['bash', recorder, '--verify', path, repo, head, 'wolf-lang'], check=True)
    lines = Path(path).read_text().splitlines()
    h = dict(word.split('=', 1) for word in lines[0].split()[1:])
    f = dict(word.split('=', 1) for word in lines[-1].split()[1:])
    assert h['head'] == head and f['worst'] == '0'
    headers.append((lines[0], h))
    footers.append(f)
    bodies.append(lines[1:-1])
for i in range(1, len(headers)):
    for key in ['repo', 'head', 'tree', 'boot']:
        assert headers[i][1][key] == headers[0][1][key], (key, 'shards differ')
    assert headers[i][1]['start'] >= footers[i - 1]['end'], 'shards overlap or are out of order'
body = []
for lines in bodies:
    body.extend(lines)
# These are data inside the first step, not new commands or verdicts.
first_start = next(i for i, line in enumerate(body) if line.startswith('START '))
provenance = [f'  aggregated source {path}: {h[0]}' for path, h in zip(inputs, headers)]
body[first_start + 1:first_start + 1] = provenance
names = [line[6:] for line in body if line.startswith('START ')]
assert len(names) == len(set(names)), 'duplicate step names across shards'
steps = sum(int(f['steps']) for f in footers)
assert len(names) == steps
footer = f"RECORD-END worst=0 steps={steps} end={footers[-1]['end']}"
with Path(output).open('x') as out:
    out.write('\n'.join([headers[0][0], *body, footer]) + '\n')
subprocess.run(['bash', recorder, '--verify', output, repo, head, 'wolf-lang'], check=True)
print(f'AGGREGATE verified {output}', flush=True)
