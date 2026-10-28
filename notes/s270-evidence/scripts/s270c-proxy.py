#!/usr/bin/python3 -I
"""s270 differ proxy: impl A is the branch head's wolf, impl B the base
(v0.2.36, 5adf3d91) built the same way. Both sides get the same tier
flag (S270C_TIER) and every conform-run record is logged."""
import json, os, subprocess, sys

name = os.path.basename(sys.argv[0])
args = sys.argv[1:]
tools = {
    "s270c-proxy-A": "/lane/wolf-lang/target/debug/wolf",
    "s270c-proxy-B": "/lane/wolf-base/target/debug/wolf",
}
tool = tools[name]
actual = list(args)
tier = os.environ.get("S270C_TIER")
if actual[:1] == ["conform-run"] and tier:
    actual.append("--" + tier)
env = dict(os.environ)
for key in ("WOLF_CHECKED", "WOLF_NATIVE", "WOLF_RELEASE"):
    env.pop(key, None)
got = subprocess.run([tool, *actual], capture_output=True, env=env)
if args[:1] == ["conform-run"]:
    row = {"proxy": name, "tier": tier, "argv": args, "status": got.returncode}
    try:
        row["record"] = json.loads(got.stdout)
    except (ValueError, UnicodeError):
        row["raw_stdout"] = got.stdout.decode(errors="replace")
    row["stderr"] = got.stderr.decode(errors="replace")[-2000:]
    with open(os.environ["S270C_PROTOCOL_LOG"], "a") as f:
        f.write(json.dumps(row) + "\n")
sys.stdout.buffer.write(got.stdout)
sys.stderr.buffer.write(got.stderr)
raise SystemExit(got.returncode)
