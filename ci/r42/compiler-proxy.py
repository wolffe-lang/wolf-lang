#!/usr/bin/env python3
"""Give both compiler controls the same lane and preserve each raw observation."""
import json
import os
import subprocess
import sys

role, *args = sys.argv[1:]
if role not in ("A", "B"):
    raise SystemExit("compiler proxy requires role A or B")
binary = os.environ[f"R42_DIFF_{role}"]
if not os.path.isabs(binary):
    raise SystemExit("compiler proxy requires an absolute executable path")
tier = os.environ["R42_DIFF_TIER"]
if tier not in ("checked", "native", "release", "passthrough"):
    raise SystemExit("compiler proxy requires checked, native, release or passthrough")
actual = list(args)
if actual[:1] == ["conform-run"] and tier != "passthrough":
    actual.append("--" + tier)
env = dict(os.environ)
for key in ("WOLF_CHECKED", "WOLF_NATIVE", "WOLF_RELEASE"):
    env.pop(key, None)
runtime = os.environ.get(f"R42_DIFF_RT_{role}")
if runtime:
    env["WOLF_RT_LIB"] = runtime
result = subprocess.run([binary, *actual], capture_output=True, env=env)
if args[:1] == ["conform-run"]:
    record = {"role": role, "tier": tier, "argv": args, "status": result.returncode,
              "stderr": result.stderr.decode(errors="replace")}
    try:
        record["record"] = json.loads(result.stdout)
    except (ValueError, UnicodeError):
        record["raw_stdout"] = result.stdout.decode(errors="replace")
    with open(os.environ["R42_DIFF_LOG"], "a") as stream:
        stream.write(json.dumps(record) + "\n")
sys.stdout.buffer.write(result.stdout)
sys.stderr.buffer.write(result.stderr)
raise SystemExit(result.returncode)
