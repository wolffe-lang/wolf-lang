#!/usr/bin/env python3
"""Remove only completed Rust test executables; retain compiler/runtime inputs."""
import datetime
import pathlib
import re
import sys

root, profile, log, marker, output = sys.argv[1:]
root = pathlib.Path(root).resolve()
if profile not in ("debug", "release"):
    raise SystemExit("REFUSED unknown profile")
if pathlib.Path(marker).read_text().strip() != "0":
    raise SystemExit("REFUSED pruning before a successful completed phase")
deps = root / "target" / profile / "deps"
paths = sorted(set(re.findall(r'\((target/' + re.escape(profile) + r'/deps/[^ )]+)\)',
                              pathlib.Path(log).read_text())))
removed = []
for relative in paths:
    path = root / relative
    if path.parent.resolve() != deps.resolve() or path.is_symlink():
        raise SystemExit(f"REFUSED {path}")
    if not re.fullmatch(r"[A-Za-z0-9_-]+-[0-9a-f]{16}", path.name):
        continue
    if path.is_file():
        size = path.stat().st_size
        path.unlink()
        removed.append(f"{size} {path}")
pathlib.Path(output).write_text(datetime.datetime.now(datetime.timezone.utc).isoformat()
                               + "\n" + "\n".join(removed) + "\n")
print(f"Pruned {len(removed)} completed {profile} test executables; "
      "kept root compiler/runtime, caches, source and all logs.")
