#!/usr/bin/python3 -I
# s270b differ proxy: proxy-wolf-A is the head compiler, proxy-wolf-B the base (trunk 5adf3d91).
# The tier comes from S270B_PROXY_TIER and is appended to every conform-run on BOTH sides,
# with WOLF_CHECKED/WOLF_NATIVE/WOLF_RELEASE stripped, so A and B run the same lane (r41's
# protocol-proxy.py is the model). Every conform-run's record is logged to S270B_PROTOCOL_LOG.
import json
import os
import subprocess
import sys

name = os.path.basename(sys.argv[0])
args = sys.argv[1:]
tools = {'proxy-wolf-A': '/lane/wolf-lang/target/debug/wolf', 'proxy-wolf-B': '/lane/base/target/debug/wolf'}
tool = tools[name]
actual = list(args)
mode = os.environ.get('S270B_PROXY_TIER')
if actual[:1] == ['conform-run'] and mode:
    actual.append('--' + mode)
env = dict(os.environ)
for key in ('WOLF_CHECKED', 'WOLF_NATIVE', 'WOLF_RELEASE'):
    env.pop(key, None)
got = subprocess.run([tool, *actual], capture_output=True, env=env)
if args[:1] == ['conform-run'] and os.environ.get('S270B_PROTOCOL_LOG'):
    row = {'proxy': name, 'tier': mode, 'argv': args, 'status': got.returncode}
    try:
        row['record'] = json.loads(got.stdout)
    except (ValueError, UnicodeError):
        row['raw_stdout'] = got.stdout.decode(errors='replace')
    row['stderr'] = got.stderr.decode(errors='replace')[-2000:]
    with open(os.environ['S270B_PROTOCOL_LOG'], 'a') as f:
        f.write(json.dumps(row) + '\n')
sys.stdout.buffer.write(got.stdout)
sys.stderr.buffer.write(got.stderr)
raise SystemExit(got.returncode)
