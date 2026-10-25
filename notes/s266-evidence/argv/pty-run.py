"""s266 (#923): run a program on a fresh pseudo-terminal with byte argv.

`python3 -I pty-run.py PROGRAM ARGHEX...`: each argument is hex (so a
byte the shell would mangle passes intact); argv[0] is PROGRAM. Prints
the terminal's transcript (repr) and the status as the shell reports it.
"""
import os
import pty
import sys

prog = sys.argv[1]
argv = [os.fsencode(prog)] + [bytes.fromhex(a) for a in sys.argv[2:]]
pid, fd = pty.fork()
if pid == 0:
    os.execv(prog, argv)
out = b""
while True:
    try:
        chunk = os.read(fd, 4096)
    except OSError:
        break
    if not chunk:
        break
    out += chunk
_, status = os.waitpid(pid, 0)
print("argv", argv)
print("transcript", repr(out.replace(b"\r", b"")))
code = os.waitstatus_to_exitcode(status)
print("status", code if code >= 0 else f"signal {-code}")
