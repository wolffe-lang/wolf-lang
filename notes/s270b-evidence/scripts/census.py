"""s270b: the downstream census at the memory check, trunk (base) against the head.

usage: census.py <out-name>
Every .lu file of every repo under /lane/census (fresh shallow clones at their trunk heads),
`conform-run --json --phase=mem` with the base compiler and with the head compiler, the std the
STD-PIN names (/lane/wolf-std/std). A row moves when the verdict, the diagnostic codes or the
warning codes differ. Six workers (block A's six cores).
"""
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

census = Path('/lane/census')
out = Path('/lane/records') / sys.argv[1]
compilers = {'base': '/lane/base/target/debug/wolf', 'head': '/lane/wolf-lang/target/debug/wolf'}
env = dict(os.environ, WOLF_STD='/lane/wolf-std/std')
files = sorted(p for p in census.rglob('*.lu') if '/.git/' not in str(p) and '/target/' not in str(p))


def observe(args):
    label, f = args
    try:
        r = subprocess.run([compilers[label], 'conform-run', '--json', '--phase=mem', str(f)],
                           capture_output=True, text=True, env=env, timeout=300)
        rec = json.loads(r.stdout.strip().splitlines()[-1])
        return (label, str(f), rec.get('verdict'), sorted(d.get('code') or '' for d in rec.get('diagnostics', [])),
                sorted(w.get('code') or '' for w in rec.get('warnings', [])), rec.get('phase_reached'))
    except Exception as e:  # a crash or a timeout is itself an answer, recorded
        return (label, str(f), f'no-record: {type(e).__name__}', [], [], None)


jobs = [(label, f) for f in files for label in compilers]
results = {}
with ThreadPoolExecutor(max_workers=6) as pool:
    for i, (label, f, verdict, codes, warns, reached) in enumerate(pool.map(observe, jobs)):
        results[(label, f)] = (verdict, codes, warns, reached)
        if i % 500 == 0:
            print(f'progress {i}/{len(jobs)}', flush=True)
moved = []
reach_mem = 0
by_repo = {}
for f in files:
    f = str(f)
    b, h = results[('base', f)], results[('head', f)]
    repo = f.split('/')[3]
    by_repo.setdefault(repo, [0, 0, 0])
    by_repo[repo][0] += 1
    if b[3] == 'mem' or (b[0] or '').startswith('fail(E10'):
        by_repo[repo][1] += 1
        reach_mem += 1
    if b[:3] != h[:3]:
        by_repo[repo][2] += 1
        moved.append(dict(file=f, base=b, head=h))
with out.open('w') as fh:
    for f in files:
        f = str(f)
        fh.write(json.dumps(dict(file=f, base=results[('base', f)], head=results[('head', f)])) + '\n')
print(f'files {len(files)}; reaching the memory check (base) {reach_mem}; moved {len(moved)}')
for repo, (n, m, mv) in sorted(by_repo.items()):
    print(f'  {repo:12s} files {n:4d} at-mem {m:4d} moved {mv}')
for m in moved:
    print('MOVED', json.dumps(m))
