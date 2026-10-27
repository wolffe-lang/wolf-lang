import hashlib
import json
from pathlib import Path
import os
import subprocess
lane = Path('/home/mfwolffe/lanes/s268')
repo = lane / 'wolf-lang'
out = lane / 'evidence/replay'
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, WOLF_RT_LIB=str(repo / 'target/debug/libwolf_rt.a'))
compilers = {'before':lane/'baseline/wolf', 'after':repo/'target/debug/wolf'}
rows = ['conc/atomic_ub_uaf.lu','memory/unsafe_ub_uaf.lu','memory/volatile_ub_uaf.lu']
for row in rows:
    d = out / Path(row).stem
    d.mkdir(exist_ok=True)
    src = d/'main.lu'
    src.write_bytes((repo/'corpus'/row).read_bytes())
    for tier in ['native','release']:
        hashes = {}
        observations = {}
        for variant, compiler in compilers.items():
            binary = d/f'{tier}-{variant}'
            command = [str(compiler),'build','--no-cache',str(src),'-o',str(binary)]
            if tier == 'release': command.append('--release')
            subprocess.run(command, env=env, check=True, capture_output=True)
            hashes[variant] = hashlib.sha256(binary.read_bytes()).hexdigest()
            observations[variant] = []
            for run in range(5):
                result = subprocess.run([str(binary)], env=env, capture_output=True)
                observations[variant].append({'exit':result.returncode,'stdout_hex':result.stdout.hex(),'stderr_hex':result.stderr.hex()})
        print(json.dumps({'row':row,'tier':tier,'hashes':hashes,'byte_identical':len(set(hashes.values())) == 1,'runs':observations}),flush=True)
    for variant, compiler in compilers.items():
        result = subprocess.run([str(compiler),'conform-run',str(repo/'corpus'/row),'--checked','--json'],env=env,capture_output=True,check=True)
        print(json.dumps({'row':row,'tier':'checked','variant':variant,'record':json.loads(result.stdout)}),flush=True)
