#!/usr/bin/env python3
"""Exercise the real proxy subprocess protocol without compiler builds."""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
PROXY = ROOT / "ci/r42/compiler-proxy.py"


class CompilerProxy(unittest.TestCase):
    def test_both_controls_receive_every_tier_without_ambient_lane_flags(self):
        with tempfile.TemporaryDirectory(dir=ROOT.parent) as scratch:
            directory = pathlib.Path(scratch)
            binary = directory / "fake-compiler"
            binary.write_text("#!/usr/bin/env python3\nimport json, os, sys\n"
                              "print(json.dumps({'argv': sys.argv[1:], 'ambient': [k for k in "
                              "('WOLF_CHECKED', 'WOLF_NATIVE', 'WOLF_RELEASE') if k in os.environ]}))\n"
                              "print('exact diagnostic', file=sys.stderr)\n"
                              "raise SystemExit(int(os.environ.get('FAKE_EXIT', '0')))\n")
            binary.chmod(0o755)
            for role in ("A", "B"):
                for tier in ("checked", "native", "release"):
                    with self.subTest(role=role, tier=tier):
                        log = directory / f"{role}-{tier}.jsonl"
                        env = dict(os.environ, R42_DIFF_A=str(binary), R42_DIFF_B=str(binary),
                                   R42_DIFF_TIER=tier, R42_DIFF_LOG=str(log),
                                   WOLF_CHECKED="1", WOLF_NATIVE="1", WOLF_RELEASE="1")
                        result = subprocess.run([sys.executable, "-I", str(PROXY), role,
                                                 "conform-run", "row.lu", "--json"],
                                                env=env, capture_output=True)
                        self.assertEqual(result.returncode, 0, result.stderr)
                        observed = json.loads(result.stdout)
                        self.assertEqual(observed["argv"], ["conform-run", "row.lu", "--json", "--" + tier])
                        self.assertEqual(observed["ambient"], [])
                        record = json.loads(log.read_text())
                        self.assertEqual(record["record"], observed)
                        self.assertEqual(record["role"], role)
                        self.assertEqual(record["stderr"], "exact diagnostic\n")
                        self.assertEqual(result.stderr, b"exact diagnostic\n")

    def test_nonzero_and_non_json_output_are_preserved(self):
        with tempfile.TemporaryDirectory(dir=ROOT.parent) as scratch:
            directory = pathlib.Path(scratch)
            binary = directory / "fake-compiler"
            binary.write_text("#!/bin/sh\nprintf 'not JSON\\n'\nprintf 'error details\\n' >&2\nexit 7\n")
            binary.chmod(0o755)
            log = directory / "records.jsonl"
            env = dict(os.environ, R42_DIFF_A=str(binary), R42_DIFF_TIER="native", R42_DIFF_LOG=str(log))
            result = subprocess.run([sys.executable, "-I", str(PROXY), "A", "conform-run", "row.lu", "--json"],
                                    env=env, capture_output=True)
            self.assertEqual(result.returncode, 7)
            self.assertEqual(result.stdout, b"not JSON\n")
            self.assertEqual(result.stderr, b"error details\n")
            record = json.loads(log.read_text())
            self.assertEqual(record["raw_stdout"], "not JSON\n")
            self.assertEqual(record["status"], 7)


if __name__ == "__main__":
    unittest.main(verbosity=2)
