"""s270b: the list-length sweep of yew's untouched witness on the native tier.

usage: sweep.py <records-name> <label=wolf,...> <N> <C,C,...>
Bisects nothing by itself: prints each count's verdict so the boundaries are read directly.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

lane = Path('/lane')
text = (lane / 'yew/notes/yw07b-evidence/stdlib/scratch-once/main.lu').read_text()
records = lane / 'records' / sys.argv[1]
wolfs = dict(x.split('=', 1) for x in sys.argv[2].split(','))
n = int(sys.argv[3])
counts = [int(c) for c in sys.argv[4].split(',')]
root = lane / 'witnesses/yew'
env = dict(os.environ, WOLF_STD='/lane/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu/std')
for count in counts:
    body = text.replace('100000', str(n)).replace('99999', str(n - 1)).replace('0..64 +', f'0..{count} +')
    (root / 'main.lu').write_text(body)
    for label, wolf in wolfs.items():
        for mode in sys.argv[5:] or ['--native']:
            r = subprocess.run([wolf, 'conform-run', '--json', mode, str(root / 'main.lu')],
                               capture_output=True, text=True, env=env, timeout=900)
            try:
                verdict = json.loads(r.stdout.strip().splitlines()[-1]).get('verdict')
            except Exception:
                verdict = 'died: ' + r.stderr.strip().splitlines()[-1][-80:] if r.stderr.strip() else None
            row = dict(n=n, count=count, machine=label + mode, verdict=verdict)
            with records.open('a') as f:
                f.write(json.dumps(row) + '\n')
            print(json.dumps(row), flush=True)
