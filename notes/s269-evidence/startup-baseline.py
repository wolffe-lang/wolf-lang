import hashlib
import json
import os
from pathlib import Path
import subprocess
lane = Path('/home/mfwolffe/lanes/s269')
repo = lane / 'wolf-lang'
out = lane / 'evidence/startup-baseline'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, WOLF_RT_LIB=str(lane / 'baseline/libwolf_rt.a'))
env.pop('WOLF_STD', None)
records = {}
for name, wolf in [('before', lane / 'baseline/wolf'), ('after', lane / 'candidate/wolf')]:
    subprocess.run([str(wolf), 'build', str(repo / 'corpus/hello.lu'), '--release', '--no-cache', '-o', str(out / name)], env=env, check=True)
subprocess.run(['clang', '-O2', str(repo / 'bench/startup/hello.c'), '-o', str(out / 'c')], check=True)
for name in ['before', 'after', 'c']:
    binary = out / name
    trace = out / f'{name}.strace'
    subprocess.run(['/usr/bin/strace', '-qq', '-o', str(trace), str(binary)], env={}, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, check=True)
    process = subprocess.run([str(binary)], env={}, capture_output=True, check=True)
    lines = [s for s in trace.read_text().splitlines() if s and not s.startswith(('+++', '---', 'execve('))]
    records[name] = dict(sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), stdout_hex=process.stdout.hex(), stderr_hex=process.stderr.hex(), syscalls=len(lines), names=[s.split('(', 1)[0] for s in lines])
assert records['before']['sha256'] == records['after']['sha256']
assert records['before']['names'] == records['after']['names']
assert records['before']['stdout_hex'] == records['after']['stdout_hex']
print(json.dumps(dict(runtime_sha256=hashlib.sha256((lane / 'baseline/libwolf_rt.a').read_bytes()).hexdigest(), records=records, before_minus_c=records['before']['syscalls']-records['c']['syscalls'], after_minus_c=records['after']['syscalls']-records['c']['syscalls']), indent=2), flush=True)
