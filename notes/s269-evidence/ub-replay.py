import hashlib
import json
import os
from pathlib import Path
import subprocess
lane = Path('/home/mfwolffe/lanes/s269')
repo = lane / 'wolf-lang'
out = lane / 'evidence/ub-replay'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, WOLF_STD=str(lane / 'wolf-std/std'), WOLF_RT_LIB=str(lane / 'baseline/libwolf_rt.a'))
for key in ['WOLF_CHECKED', 'WOLF_NATIVE', 'WOLF_RELEASE', 'S269_MEASURE']:
    env.pop(key, None)
records = []
for relative in ['conc/atomic_ub_uaf.lu', 'memory/unsafe_ub_uaf.lu', 'memory/volatile_ub_uaf.lu']:
    source = repo / 'corpus' / relative
    checked = []
    for name in ['baseline', 'candidate']:
        result = subprocess.run([str(lane / name / 'wolf'), 'conform-run', str(source), '--checked', '--json'], env=env, capture_output=True, check=True, timeout=30)
        value = json.loads(result.stdout)
        checked.append(value)
    assert checked[0] == checked[1], checked
    assert checked[0]['x-ub-row'] == 'P1', checked
    for tier in ['native', 'release']:
        pair = []
        for name in ['baseline', 'candidate']:
            binary = out / f'{source.stem}-{tier}-{name}'
            command = [str(lane / name / 'wolf'), 'build', '--no-cache', str(source), '-o', str(binary)]
            if tier == 'release': command.append('--release')
            build = subprocess.run(command, env=env, capture_output=True, timeout=60)
            assert build.returncode == 0, (command, build.returncode, build.stderr.decode())
            runs = []
            for i in range(5):
                run = subprocess.run([str(binary)], env=env, capture_output=True, timeout=10)
                runs.append(dict(code=run.returncode, stdout_hex=run.stdout.hex(), stderr_hex=run.stderr.hex()))
            pair.append(dict(compiler=name, sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), runs=runs))
        assert pair[0]['sha256'] == pair[1]['sha256'], pair
        assert all(r['stdout_hex'] == '' and r['stderr_hex'] == '' for p in pair for r in p['runs']), pair
        records.append(dict(file=relative, tier=tier, checked_verdict=checked[0]['verdict'], checked_ub_row=checked[0]['x-ub-row'], records=pair))
print(json.dumps(records, indent=2), flush=True)
print('PASS: all six compiled UB pairs are byte-identical; checked records identical P1; five replays each with empty streams. Arbitrary program exits are not build exits.', flush=True)
