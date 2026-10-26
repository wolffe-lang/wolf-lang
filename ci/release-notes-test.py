#!/usr/bin/env python3
"""The dry release must request the same curated entry as its version tag."""
from pathlib import Path
import os
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def dry_script():
    text = (ROOT / ".github/workflows/release.yml").read_text()
    step = text.split("- name: release notes from the workspace version (dry run)", 1)[1]
    block = step.split("        run: |\n", 1)[1]
    lines = []
    for line in block.splitlines():
        if line and not line.startswith("          "):
            break
        lines.append(line[10:])
    return "\n".join(lines)


class DryReleaseNotes(unittest.TestCase):
    def invoke(self, manifest):
        with tempfile.TemporaryDirectory(prefix="r41-notes-") as d:
            d = Path(d)
            (d / "Cargo.toml").write_text(manifest)
            cargo = d / "cargo"
            cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
            cargo.chmod(0o755)
            return subprocess.run(
                ["bash", "-euo", "pipefail", "-c", dry_script()], cwd=d,
                env=dict(os.environ, PATH=f'{d}:{os.environ["PATH"]}'),
                capture_output=True, text=True,
            )

    def test_curated_workspace_entry_is_rehearsed(self):
        manifest = (ROOT / "Cargo.toml").read_text()
        version = re.search(r'^version = "([^"]+)"$', manifest, re.M).group(1)
        changelog = (ROOT / "CHANGELOG.md").read_text()
        self.assertRegex(changelog, rf"(?m)^## {re.escape(version)}(?:\s|$)")
        got = self.invoke(manifest)
        self.assertEqual(got.returncode, 0, got.stderr)
        self.assertEqual(got.stdout.splitlines(), [
            "xtask", "release-notes", version, "--out", "target/release-notes.md",
        ])
        self.assertIn("python3 ci/release-notes-test.py",
                      (ROOT / ".github/workflows/ci.yml").read_text())

    def test_version_is_not_hardcoded_or_a_dependency_version(self):
        got = self.invoke('[workspace.package]\nversion = "9.8.7"\n'
                          '[dependencies.other]\nversion = "1.2.3"\n')
        self.assertEqual(got.returncode, 0, got.stderr)
        self.assertEqual(got.stdout.splitlines()[2], "9.8.7")

    def test_missing_version_refuses_before_extracting_notes(self):
        got = self.invoke("[workspace]\nresolver = '3'\n")
        self.assertNotEqual(got.returncode, 0)
        self.assertEqual(got.stdout, "")


if __name__ == "__main__":
    unittest.main()
