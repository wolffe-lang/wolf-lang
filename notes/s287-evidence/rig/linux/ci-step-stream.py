#!/usr/bin/env python3
"""Run an unchanged xtask CI shard and expose each step's termination.

xtask prints its next marker only after the preceding child succeeded,
and exits at the first failed child. Preserve all child output indented;
the independent gauntlet recorder/audit supplies the record verdict.
"""
import subprocess
import sys
import time

SHARDS = {
    'build': ['fmt', 'fmt-fixpoint', 'clippy', 'rt-none', 'test', 'fuzz-check', 'deps-check'],
    'gates': ['corpus', 'abi-check', 'debug-check', 'midend-rate', 'bench-gates',
              'spec-extract', 'conformance', 'print-gate', 'diag-catalog', 'doc-catalog',
              'fmt-lu', 'fmt-fuzz', 'audit-surface', 'dist-smoke', 'differ-self'],
    'coverage': ['lane-coverage'],
}


def stream(shard, command):
    expected = SHARDS[shard]
    seen = []
    bad = False
    current = f'ci-{shard}-launch'
    started = time.monotonic()
    print(f'START {current}', flush=True)
    try:
        proc = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, text=True)
    except OSError as exc:
        print(f'  launch failed: {exc}', flush=True)
        print(f'END {current} exit=1 secs={time.monotonic() - started:.3f}', flush=True)
        return 1
    for line in proc.stdout:
        if line.startswith('== xtask ci: '):
            name = line.removeprefix('== xtask ci: ').strip()
            if len(seen) >= len(expected) or name != expected[len(seen)]:
                bad = True
                print(f'  unexpected CI marker: {name!r}; seen={seen!r}', flush=True)
            else:
                print(f'END {current} exit=0 secs={time.monotonic() - started:.3f}', flush=True)
                current = name
                started = time.monotonic()
                seen.append(name)
                print(f'START {current}', flush=True)
        print('  ' + line, end='', flush=True)
    code = proc.wait()
    code = code if code >= 0 else 128 - code
    if bad or (code == 0 and seen != expected):
        print(f'  incomplete or invalid shard: seen={seen!r}, expected={expected!r}', flush=True)
        code = code or 1
    print(f'END {current} exit={code} secs={time.monotonic() - started:.3f}', flush=True)
    return code


if __name__ == '__main__':
    if len(sys.argv) != 2 or sys.argv[1] not in SHARDS:
        raise SystemExit('usage: ci-step-stream.py build|gates|coverage')
    shard = sys.argv[1]
    raise SystemExit(stream(shard, ['cargo', 'xtask', 'ci', '--shard', shard]))
