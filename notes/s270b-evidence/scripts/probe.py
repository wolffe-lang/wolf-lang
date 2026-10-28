"""s270b probe runner: every .lu file in a probe directory on all four machines.

usage: probe.py <records-name> <dir-under-/lane> [wolf-binary]
Prints one line per (file, machine): verdict, stdout (trimmed), the diagnostic codes.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

records = Path('/lane/records') / sys.argv[1]
d = Path('/lane') / sys.argv[2]
wolf = sys.argv[3] if len(sys.argv) > 3 else '/lane/wolf-lang/target/debug/wolf'
lupin = os.environ['LUPIN']
env = dict(os.environ)


def record(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=600)
    try:
        rec = json.loads(r.stdout.strip().splitlines()[-1])
    except Exception:
        return dict(exit=r.returncode, verdict=None, stdout=r.stdout[-300:], stderr=r.stderr[-600:])
    codes = [x.get('code') for x in rec.get('diagnostics', [])]
    return dict(exit=r.returncode, verdict=rec.get('verdict'), stdout=rec.get('stdout_inline'),
                codes=codes, stderr=r.stderr[-600:])


for f in sorted(d.glob('*.lu')):
    dump = subprocess.run([wolf, 'conform-run', '--json', '--phase=mem', '--dump=mut-clean', str(f)],
                          capture_output=True, text=True, env=env, timeout=600)
    clean = [x for x in dump.stderr.splitlines() if x.startswith('fn ') and 'main' not in x]
    print(f"{f.name:40s} mut-clean: {'; '.join(clean)}", flush=True)
    rows = {}
    for mode in ['--checked', '--native', '--release']:
        rows[mode] = record([wolf, 'conform-run', '--json', mode, str(f)])
    rows['lupin'] = record([lupin, 'conform-run', '--json', str(f)])
    for k, v in rows.items():
        line = dict(file=f.name, machine=k, **v)
        with records.open('a') as out:
            out.write(json.dumps(line) + '\n')
        so = (v.get('stdout') or '').strip().replace('\n', ' | ')[:80]
        err = (v.get('stderr') or '').strip().replace('\n', ' | ')[:300] if not v.get('verdict', '') or 'exit' not in (v.get('verdict') or '') else ''
        print(f"{f.name:40s} {k:10s} exit={v['exit']} {v.get('verdict')} codes={v.get('codes')} out=[{so}] {err}", flush=True)
