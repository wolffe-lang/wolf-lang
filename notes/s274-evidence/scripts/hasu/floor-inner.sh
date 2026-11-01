#!/usr/bin/env bash
# s274 item 4: why the ru_maxrss siblings read the same peak at N and 2N turns.
# A child's `ru_maxrss` (wait4) starts from the high-water mark of the address
# space it replaced at exec: the parent's, copied by fork or shared by vfork. A
# parent that has touched M MiB then runs /bin/true, which needs ~1 MiB of its own;
# the child's ru_maxrss reads ~M MiB. Inside the container, as the lane uid.
# usage: floor-inner.sh
set -uo pipefail
for mib in 0 16 64; do
  python3 -I - "$mib" <<'PY'
import os, subprocess, sys
mib = int(sys.argv[1])
blob = bytearray(mib << 20)
for i in range(0, len(blob), 4096):
    blob[i] = 1
own = [l for l in open("/proc/self/status") if l.startswith("VmHWM:")][0].split()[1]
p = subprocess.Popen(["/bin/true"])
_, _, ru = os.wait4(p.pid, 0)
spawn = ru.ru_maxrss
pid = os.fork()
if pid == 0:
    os.execv("/bin/true", ["/bin/true"])
_, _, ru = os.wait4(pid, 0)
print(f"S274_FLOOR parent touched {mib} MiB, parent VmHWM {own} kB: /bin/true's ru_maxrss {spawn} kB by subprocess (vfork), {ru.ru_maxrss} kB by fork+exec")
PY
done
