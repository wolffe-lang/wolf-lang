#!/usr/bin/env python3
"""Offline integration regression using an explicitly supplied licensed cache."""
import pathlib
import subprocess
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
CACHE = pathlib.Path(sys.argv.pop(1)).resolve() if len(sys.argv) > 1 else None


class LicensedBundleCache(unittest.TestCase):
    def test_published_licensed_bundle_is_accepted_without_relaxing_checks(self):
        self.assertIsNotNone(CACHE, "pass a new owned cache containing the digest-verified archive")
        result = subprocess.run([sys.executable, "-I", str(ROOT / "ci/fetch-entry-bundle"), str(CACHE)],
                                cwd=ROOT, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stdout.decode() + result.stderr.decode())
        self.assertIn("38 members verified twice", result.stdout.decode())


if __name__ == "__main__":
    unittest.main(verbosity=2)
