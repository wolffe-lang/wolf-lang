"""s270b: does the head compiler build #631's copy_int witness any differently from trunk?
Extracts WITNESS and STEP_COPY_INT from region_copyout_lanes.rs, builds it with both compilers on
both tiers (--no-cache), and prints each binary's sha256 and the regions dump's digest."""
import hashlib
import os
import re
import subprocess
from pathlib import Path

src = Path('/lane/wolf-lang/crates/wolf_driver/tests/region_copyout_lanes.rs').read_text()
consts = dict(re.findall(r'const (\w+): &str = r#"(.*?)"#;', src, re.S))
prog = consts['WITNESS'].replace('STEP', consts['STEP_COPY_INT'])
d = Path('/lane/scratch/tmp/ab631')
d.mkdir(parents=True, exist_ok=True)
(d / 'main.lu').write_text(prog)
env = dict(os.environ)
for label, wolf in [('head', '/lane/wolf-lang/target/debug/wolf'), ('trunk', '/lane/base/target/debug/wolf')]:
    for tier in ['native', 'release']:
        out = d / f'{label}-{tier}'
        args = [wolf, 'build', str(d / 'main.lu'), '-o', str(out), '--no-cache'] + (['--release'] if tier == 'release' else [])
        r = subprocess.run(args, capture_output=True, text=True, env=env)
        h = hashlib.sha256(out.read_bytes()).hexdigest() if out.exists() else None
        print(f'{label} {tier} build rc={r.returncode} sha256={h}', flush=True)
    dump = subprocess.run([wolf, 'conform-run', '--json', '--phase=mem', '--dump=regions', str(d / 'main.lu')],
                          capture_output=True, text=True, env=env)
    print(f'{label} regions-dump sha256={hashlib.sha256(dump.stderr.encode()).hexdigest()}', flush=True)
    print(dump.stderr.strip()[:1500])
