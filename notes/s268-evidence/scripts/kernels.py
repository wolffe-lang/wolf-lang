#!/usr/bin/env python3
"""Paired s268 before/after measurements on every compiler kernel, both tiers."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import statistics

lane = Path('/home/mfwolffe/lanes/s268')
repo = lane / 'wolf-lang'
out = lane / 'evidence' / 'kernels'
out.mkdir(parents=True, exist_ok=True)
manifest = json.loads((repo / 'bench/kernels/manifest.json').read_text())
env = dict(os.environ, WOLF_RT_LIB=str(repo / 'target/release/libwolf_rt.a'))
compilers = {'before': lane / 'baseline/wolf', 'after': repo / 'target/debug/wolf'}
rows = []
with (out / 'samples.jsonl').open('w') as samples:
    for k in manifest['kernels']:
        name = k['name']
        source_dir = out / 'source' / name
        source_dir.mkdir(parents=True, exist_ok=True)
        source = source_dir / 'main.lu'
        source.write_bytes((repo / 'bench/kernels' / name / 'kernel.lu').read_bytes())
        for tier in ('native', 'release'):
            bins = {}
            for variant, wolf in compilers.items():
                binary = out / f'{name}-{tier}-{variant}'
                cmd = [str(wolf), 'build', '--no-cache', str(source), '-o', str(binary)]
                if tier == 'release':
                    cmd.append('--release')
                with (out / f'{name}-{tier}-{variant}.build.log').open('w') as log:
                    subprocess.run(cmd, env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=300)
                bins[variant] = binary
            times = {'before': [], 'after': []}
            sink = None
            for run in range(11):
                order = ('before', 'after') if run % 2 == 0 else ('after', 'before')
                for variant in order:
                    result = subprocess.run([str(bins[variant]), str(k['ops_wolf'])], env=env, check=True, capture_output=True, text=True, timeout=120)
                    value = json.loads(result.stdout)
                    if sink is None:
                        sink = value['sink']
                    if value['sink'] != sink:
                        raise RuntimeError(f'sink mismatch: {name} {tier} {variant}: {value["sink"]} != {sink}')
                    row = dict(kernel=name, tier=tier, variant=variant, run=run, warmup=run == 0, **value)
                    samples.write(json.dumps(row) + '\n')
                    samples.flush()
                    if run:
                        times[variant].append(value['ns'] / value['ops'])
            stats = {}
            for variant, values in times.items():
                median = statistics.median(values)
                stats[variant] = dict(median=median, mad=statistics.median(abs(v-median) for v in values))
            before, after = stats['before'], stats['after']
            floor = max(before['median'] * .02, 3 * before['mad'], 3 * after['mad'])
            same = hashlib.sha256(bins['before'].read_bytes()).digest() == hashlib.sha256(bins['after'].read_bytes()).digest()
            row = dict(kernel=name, tier=tier, stats=stats, byte_identical=same, noise_floor_ns_per_op=floor, regression=after['median']-before['median'] > floor)
            rows.append(row)
            print(json.dumps(row), flush=True)
            (out / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
