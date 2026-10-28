"""s270b: yew's scratch-once witness on all four machines, against chosen compilers.

usage: repro2.py <records-name> <witness-dir-under-/lane/witnesses> <label=wolf,...> <N,C> ...
The witness directory holds yew's fl/, util/ and unicode/ (copied from yew bf7ab4a3's src/);
main.lu is written from yew's notes/yw07b-evidence/stdlib/scratch-once/main.lu with the two
dimensions substituted exactly as s270 did: '100000' -> N, '99999' -> N-1, '0..64 +' -> '0..C +'.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

lane = Path('/lane')
text = (lane / 'yew/notes/yw07b-evidence/stdlib/scratch-once/main.lu').read_text()
records = lane / 'records' / sys.argv[1]
root = lane / 'witnesses' / sys.argv[2]
wolfs = dict(x.split('=', 1) for x in sys.argv[3].split(','))
configs = [tuple(int(x) for x in c.split(',')) for c in sys.argv[4:]]
env = dict(os.environ, WOLF_STD='/lane/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu/std')
lupin = os.environ['LUPIN']


def obs(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=900)
    try:
        rec = json.loads(r.stdout.strip().splitlines()[-1])
        codes = [d.get('code') for d in rec.get('diagnostics', [])]
        return r.returncode, rec.get('verdict'), rec.get('stdout_inline'), codes, r.stderr[-300:]
    except Exception:
        return r.returncode, None, None, None, (r.stdout + r.stderr)[-300:]


for n, count in configs:
    body = text.replace('100000', str(n)).replace('99999', str(n - 1)).replace('0..64 +', f'0..{count} +')
    (root / 'main.lu').write_text(body)
    rows = []
    for label, wolf in wolfs.items():
        for mode in ['--checked', '--native', '--release']:
            rows.append((f'{label}{mode}', obs([wolf, 'conform-run', '--json', mode, str(root / 'main.lu')])))
    rows.append(('lupin-0.1.58', obs([lupin, 'conform-run', '--json', str(root / 'main.lu')])))
    for machine, (rc, verdict, out, codes, err) in rows:
        row = dict(witness=sys.argv[2], n=n, count=count, machine=machine, exit=rc, verdict=verdict,
                   stdout=out, codes=codes, stderr=err)
        with records.open('a') as f:
            f.write(json.dumps(row) + '\n')
        tail = '' if verdict and verdict.startswith(('exit', 'fail')) else (err or '').strip().replace('\n', ' | ')[-160:]
        print(f"{sys.argv[2]} N={n} C={count} {machine:22s} {verdict} {codes or ''} {(out or '').strip()[:60]} {tail}", flush=True)
