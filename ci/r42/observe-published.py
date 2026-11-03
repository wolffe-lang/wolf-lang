#!/usr/bin/env python3
"""Record published controls from a trusted cwd; never infer unsupported stops."""
import hashlib
import json
import pathlib
import subprocess
import sys

source, inputs, records = map(pathlib.Path, sys.argv[1:])
records.mkdir(parents=True, exist_ok=False)
versions = {}
for package, version in (("wolf", "0.2.36"), ("lupin", "0.1.58"), ("lupin", "0.1.59")):
    binary = next((inputs / f"{package}-{version}").rglob(package))
    assert binary.is_file() and binary.is_absolute()
    versions[version] = binary
    out = subprocess.run([str(binary), "--version"], cwd=source, capture_output=True, timeout=30)
    (records / f"{package}-{version}-version.txt").write_bytes(out.stdout + out.stderr)
    assert out.returncode == 0, out
    print(out.stdout.decode(), flush=True)
rows = ["fs/statfs_rows.lu", "os/chroot_rows.lu", "os/priority_rows.lu",
        "os/signal_action_rows.lu", "os/term_lf_rows.lu", "fs/at_rows.lu",
        "fs/mkdir_at.lu", "fs/symlink_at.lu", "fs/link_at.lu",
        "fs/open_directory.lu", "fs/mode_fd.lu"]
observations = []
for version in ("0.1.58", "0.1.59"):
    for row in rows:
        name = row.replace("/", "-")
        cwd = records / f"cwd-{version}-{name}"
        cwd.mkdir()
        out = subprocess.run([str(versions[version]), "conform-run", str(source / "corpus" / row), "--json"],
                             cwd=cwd, capture_output=True, timeout=60)
        (records / f"{version}-{name}.stdout").write_bytes(out.stdout)
        (records / f"{version}-{name}.stderr").write_bytes(out.stderr)
        record = json.loads(out.stdout)
        assert record["impl_version"] == version, record
        observations.append({"version": version, "row": row, "exit": out.returncode, "record": record,
                             "stderr": out.stderr.decode(errors="replace")})
        print(json.dumps(observations[-1]), flush=True)
(records / "observations.json").write_text(json.dumps(observations, indent=2) + "\n")
print("END published-observations exit=0", flush=True)
