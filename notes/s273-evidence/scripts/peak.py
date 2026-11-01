#!/usr/bin/env python3
"""s273: run one program, report its peak resident set from wait4(2).

usage: peak.py GUARD_MB [TIMEOUT_S] -- prog args...

Polls the child's RSS every 20 ms and SIGKILLs exactly that pid when it
passes GUARD_MB (a memory guard for the Mac). Prints one line:
  PEAK <bytes> MB=<mb> wall=<s> status=<exit|killed> stdout=<first line>
"""
import os
import subprocess
import sys
import time


def rss_kb(pid):
    try:
        out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)],
                             capture_output=True, text=True).stdout.strip()
        return int(out) if out else 0
    except ValueError:
        return 0


def main():
    guard_mb = int(sys.argv[1])
    timeout = 300.0
    rest = sys.argv[2:]
    if rest[0] != "--":
        timeout = float(rest[0])
        rest = rest[1:]
    assert rest[0] == "--"
    argv = rest[1:]
    t0 = time.time()
    p = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    killed = False
    while True:
        pid, status, ru = os.wait4(p.pid, os.WNOHANG)
        if pid == p.pid:
            break
        if rss_kb(p.pid) > guard_mb * 1024 or time.time() - t0 > timeout:
            os.kill(p.pid, 9)
            killed = True
            pid, status, ru = os.wait4(p.pid, 0)
            break
        time.sleep(0.02)
    wall = time.time() - t0
    out = p.stdout.read().decode(errors="replace").strip().splitlines()
    err = p.stderr.read().decode(errors="replace").strip().splitlines()
    peak = ru.ru_maxrss if sys.platform == "darwin" else ru.ru_maxrss * 1024
    if killed:
        st = "killed"
    elif os.WIFEXITED(status):
        st = "exit%d" % os.WEXITSTATUS(status)
    else:
        st = "signal%d" % os.WTERMSIG(status)
    print("PEAK %d MB=%.1f wall=%.2f status=%s stdout=%s stderr=%s" % (
        peak, peak / 1048576.0, wall, st, out[0][:300] if out else "",
        err[-1] if err else ""))


if __name__ == "__main__":
    main()
