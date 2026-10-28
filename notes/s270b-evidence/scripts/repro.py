"""s270b item 1: yew's scratch-once witness on all four machines.

The witness text is yew bf7ab4a3's notes/yw07b-evidence/stdlib/scratch-once/main.lu,
parameterised on its two independent dimensions (byte count N, list element count C)
exactly as s270 did: '100000' -> N, '99999' -> N-1, '0..64 +' -> '0..C +'.
Machines: checked/native/release through `wolf conform-run --json` (trunk and 0.2.35),
lupin 0.1.58 through `lupin conform-run`.
"""
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

lane = Path('/lane')
src = lane / 'yew/notes/yw07b-evidence/stdlib/scratch-once/main.lu'
root = lane / 'witnesses/yew'
if not root.exists():
    root.mkdir(parents=True)
    for d in ['fl', 'util', 'unicode']:
        shutil.copytree(lane / 'yew/src' / d, root / d)
text = src.read_text()
std035 = '/lane/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu/std'
env = dict(os.environ, WOLF_STD=std035)
wolfs = {'trunk': '/lane/wolf-lang/target/debug/wolf', '035': os.environ['WOLF035']}
lupin = os.environ['LUPIN']
records = lane / 'records' / sys.argv[1]
configs = [tuple(int(x) for x in c.split(',')) for c in sys.argv[2:]]


def emit(row):
    with records.open('a') as f:
        f.write(json.dumps(row) + '\n')
    print(json.dumps(row), flush=True)


for n, count in configs:
    body = text.replace('100000', str(n)).replace('99999', str(n - 1)).replace('0..64 +', f'0..{count} +')
    (root / 'main.lu').write_text(body)
    for ver, wolf in wolfs.items():
        for mode in ['--checked', '--native', '--release']:
            r = subprocess.run([wolf, 'conform-run', '--json', mode, str(root / 'main.lu')],
                               capture_output=True, text=True, env=env, timeout=600)
            try:
                rec = json.loads(r.stdout.strip().splitlines()[-1])
                verdict = rec.get('verdict')
                out = rec.get('stdout_inline')
            except Exception:
                verdict, out = None, None
            emit(dict(n=n, count=count, machine=f'wolf-{ver}{mode}', exit=r.returncode,
                      verdict=verdict, stdout=out, stderr=r.stderr[-400:]))
    r = subprocess.run([lupin, 'conform-run', str(root / 'main.lu')],
                       capture_output=True, text=True, env=env, timeout=600)
    try:
        rec = json.loads(r.stdout.strip().splitlines()[-1])
        verdict = rec.get('verdict')
        out = rec.get('stdout_inline')
    except Exception:
        verdict, out = None, r.stdout[-200:]
    emit(dict(n=n, count=count, machine='lupin-0.1.58', exit=r.returncode,
              verdict=verdict, stdout=out, stderr=r.stderr[-400:]))
