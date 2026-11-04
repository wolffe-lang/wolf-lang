#!/usr/bin/env python3
"""Compare the recorded before/after answers and predicted native ledger deltas."""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent


def rows(path, prefix):
    result = {}
    current = None
    for line in path.read_text().splitlines():
        if line.startswith(f'BEGIN {prefix}-'):
            current = line.split()[1].removeprefix(prefix + '-')
            assert current not in result, current
            result[current] = []
        elif current and line.startswith(f'END {prefix}-{current} '):
            assert ' exit=0 ' in line, line
            current = None
        elif current:
            result[current].append(line)
    assert current is None, 'incomplete row'
    return result


def observation(lines):
    for line in lines:
        if line.startswith('{'):
            value = json.loads(line)
            return {k: value.get(k) for k in ['verdict', 'stdout_inline', 'stdout_sha256', 'diagnostics', 'warnings']}
    raise AssertionError('missing JSON observation')


def numbers(lines):
    values = [line for line in lines if line and all(word.isdigit() for word in line.split())]
    assert len(values) == 1, values
    return [int(word) for word in values[0].split()]


base = rows(HERE / 'baseline.log', 'base')
head = rows(HERE / 'head-7d0539c9.log', 'head')
assert base.keys() == head.keys(), 'measurement rows were dropped or added'
checked = native = 0
for key in base:
    if key.startswith('checked-'):
        assert observation(base[key]) == observation(head[key]), key
        checked += 1
    elif not key.startswith('ledger-'):
        assert numbers(base[key]) == numbers(head[key]), key
        native += 1
print(f'PASS identical answers: {checked} checked observations and {native} native/release/C executions')
for n, predicted in [(10_000, 524176), (100_000, 4194192), (236_000, 8388496)]:
    for mode in ['region', 'rindex', 'rset']:
        key = f'ledger-{mode}-{n}'
        before, after = numbers(base[key]), numbers(head[key])
        assert before[0] == after[0] == n
        measured = after[1] - before[1]
        assert measured == predicted, (key, measured, predicted)
        print(f'PASS {key}: before={before[1]} after={after[1]} predicted_delta={predicted} measured_delta={measured}')
