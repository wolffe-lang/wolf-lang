"""Real controlling-PTY witnesses. Interrupts are terminal bytes, never external signals."""
import errno
import fcntl
import json
import os
import pathlib
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time

wolf = str(pathlib.Path(sys.argv[1]).resolve())
root = pathlib.Path(sys.argv[2]).resolve()
env = dict(os.environ, TERM='xterm', WOLF_STATUS_AFTER='1')
env.pop('WOLF_STD', None)
linker = root / 'slow-cc'
linker.write_text('#!/usr/bin/env python3\nimport os,sys,time\ntime.sleep(1.6)\nos.execvp("cc", ["cc", *sys.argv[1:]])\n')
linker.chmod(0o755)
args = ['build', '--no-cache', '--linker', str(linker), str(root / 'main.lu'), '-o', str(root / 'program')]

def checked_run(extra, command='build'):
    command_args = args if command == 'build' else ['run', '--no-cache', '--linker', str(linker), *extra, str(root / 'main.lu')]
    if command == 'build':
        command_args = [*args, *extra]
    result = subprocess.run([wolf, *command_args], env=env, capture_output=True, timeout=30)
    assert result.returncode == 0, result.stderr.decode()
    return result

result = checked_run([])
assert result.stdout == b''
assert b'wolf build: still in link of main' in result.stderr, result.stderr
assert b'wolf build: finished native in ' in result.stderr, result.stderr
assert b'\x1b' not in result.stderr and b'\r' not in result.stderr
print('PASS nonTTY WOLF_STATUS_AFTER=1 heartbeat and final line')
result = checked_run(['--quiet'])
assert result.stderr == b'', result.stderr
print('PASS slow quiet build has zero reporter bytes')
result = checked_run([], command='run')
assert result.stdout == b''
assert b'wolf run: still in link of main' in result.stderr, result.stderr
assert b'wolf run: finished native in ' in result.stderr, result.stderr
print('PASS run reporter finishes before program execution')

# Actual external-tool output resets the idle clock and keeps its original stream.
chatty = root / 'chatty-cc'
chatty.write_text('#!/usr/bin/env python3\nimport os,sys,time\ntime.sleep(0.6)\nsys.stdout.write("tool stdout\\n");sys.stdout.flush()\nsys.stderr.write("tool stderr\\n");sys.stderr.flush()\ntime.sleep(0.6)\nos.execvp("cc", ["cc", *sys.argv[1:]])\n')
chatty.chmod(0o755)
chatty_args = [str(chatty) if a == str(linker) else a for a in args]
for extra in [[], ['--quiet']]:
    result = subprocess.run([wolf, *chatty_args, *extra], env=env, capture_output=True, timeout=30)
    assert result.returncode == 0, result.stderr
    assert result.stdout == b'tool stdout\n', result.stdout
    assert result.stderr == b'tool stderr\n', result.stderr
print('PASS tool output resets idle deadline, preserves both streams, and survives quiet')

# Tiny terminal catches wrapped live-line residue; reserve room for echoed ^C.
def screen(data, width=40):
    lines = [[]]
    cursor = 0
    tokens = re.split(r'(\x1b\[[0-9;]*[A-Za-z])', data.decode(errors='replace'))
    for token in tokens:
        if token.startswith('\x1b['):
            assert token == '\x1b[2K', repr(token)
            lines[-1] = []
            continue
        for c in token:
            if c == '\r': cursor = 0
            elif c == '\n': lines.append([]); cursor = 0
            elif c == '\b': cursor = max(0, cursor - 1)
            elif c >= ' ':
                if cursor >= width:
                    lines.append([]); cursor = 0
                while len(lines[-1]) <= cursor: lines[-1].append(' ')
                lines[-1][cursor] = c
                cursor += 1
    return '\n'.join(''.join(line) for line in lines)

def terminal_row(name, argv, interrupt=False, dumb=False, expected=0):
    pid, fd = pty.fork()
    if pid == 0:
        child_env = dict(env)
        if dumb: child_env['TERM'] = 'dumb'
        os.execvpe(wolf, [wolf, *argv], child_env)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 40, 0, 0))
    data = bytearray()
    sent = False
    deadline = time.monotonic() + 30
    status = None
    eof = False
    while time.monotonic() < deadline:
        ready, _, _ = select.select([] if eof else [fd], [], [], 0.1)
        if ready:
            try: block = os.read(fd, 65536)
            except OSError as error:
                if error.errno != errno.EIO: raise
                block = b''
            if not block:
                # A PTY can report EOF just before waitpid observes the child's exit.
                # Keep polling within the real deadline instead of treating that race as timeout.
                eof = True
            data.extend(block)
            if interrupt and not sent and b'Building [' in data:
                os.write(fd, b'\x03')
                sent = True
        waited, candidate = os.waitpid(pid, os.WNOHANG)
        if waited:
            status = candidate
            # Drain bytes already queued by exit/diagnostic cleanup.
            while select.select([fd], [], [], 0)[0]:
                try: block = os.read(fd, 65536)
                except OSError as error:
                    if error.errno != errno.EIO: raise
                    break
                if not block: break
                data.extend(block)
            break
    if status is None:
        waited, candidate = os.waitpid(pid, os.WNOHANG)
        if waited: status = candidate
    if status is None:
        # A timeout is red; the only cleanup request is still a terminal interrupt byte.
        os.write(fd, b'\x03')
        _, status = os.waitpid(pid, 0)
        raise AssertionError(f'{name}: timed out; output={bytes(data)!r}')
    os.close(fd)
    if interrupt:
        assert sent and os.WIFSIGNALED(status) and os.WTERMSIG(status) == signal.SIGINT, (status, data)
    else:
        assert os.WIFEXITED(status) and os.WEXITSTATUS(status) == expected, (status, data)
    if dumb:
        assert b'Building [' not in data and b'\x1b' not in data and b'\r\x1b' not in data, data
    else:
        assert b'\r\x1b[2KBuilding [' in data, data
        assert 'Building [' not in screen(bytes(data)), (bytes(data), screen(bytes(data)))
    print(f'PASS PTY {name}: clean terminal, {len(data)} captured bytes')

terminal_row('success', args)
terminal_row('dumb', args, dumb=True)
terminal_row('SIGINT via 0x03', args, interrupt=True)
refusal = root / 'refusal'
refusal.mkdir()
source = 'fn main() -> int {\n    let r = (2..)\n    print("{r.start}")\n    0\n}\n'
source += ''.join(f'fn filler{i}() -> int {{ {i} }}\n' for i in range(1000))
assert len(source) < 100_000
(refusal / 'main.lu').write_text(source)
terminal_row('refusal', ['build', '--emit=wir', str(refusal / 'main.lu'), '-o', str(refusal / 'out')], expected=4)
