#!/usr/bin/env python3
"""Network-free checks for real-linker input selection and CI provisioning."""
import contextlib
import hashlib
import io
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
FETCH = ROOT / "ci/fetch-test-link-bundle"


class LinkInputs(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="wolf-link-inputs-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.rt = self.root / "head runtime.a"
        self.rt.write_bytes(b"runtime from the compiler's head")
        self.sysroot = self.root / "verified sysroot"
        self.sysroot.mkdir()
        self.envfile = self.root / "github-env"
        self.cache = self.root / "bundle cache"
        self.env = {
            "WOLF_STATIC_ENTRY_MUSL_RT": str(self.rt),
            "WOLF_STATIC_ENTRY_SYSROOT": str(self.sysroot),
            "GITHUB_ENV": str(self.envfile),
        }

    def invoke(self, failure=None):
        def fetch(args, **kwargs):
            self.assertEqual(args, [sys.executable, str(ROOT / "ci/fetch-entry-bundle"), str(self.cache)])
            self.assertEqual(kwargs, {"check": True})
            if failure:
                raise failure
            print("bundle-sha  digest-pinned-musl-bundle")
            return subprocess.CompletedProcess(args, 0)

        output = io.StringIO()
        with patch.dict(os.environ, self.env, clear=True), patch.object(sys, "argv", [str(FETCH), str(self.cache)]), patch("subprocess.run", side_effect=fetch), contextlib.redirect_stdout(output):
            runpy.run_path(str(FETCH), run_name="__main__")
        return output.getvalue()

    def test_exports_head_runtime_and_shared_sysroot_with_both_digests(self):
        output = self.invoke()
        digest = hashlib.sha256(self.rt.read_bytes()).hexdigest()
        self.assertEqual(output.count(f"{digest}  {self.rt}"), 2)
        self.assertIn("bundle-sha", output)
        self.assertIn(f"WOLF_TEST_LIGATURE_RT={self.rt}\nLIGATURE_SYSROOT={self.sysroot}\n", self.envfile.read_text())
        self.assertNotIn("target/test-link-bundle/libwolf_rt.a", self.envfile.read_text())

    def test_missing_runtime_variable_is_final(self):
        del self.env["WOLF_STATIC_ENTRY_MUSL_RT"]
        with self.assertRaisesRegex(SystemExit, "REFUSED missing head-built musl runtime"):
            self.invoke()
        self.assertFalse(self.envfile.exists())

    def test_missing_runtime_file_is_final(self):
        self.rt.unlink()
        with self.assertRaisesRegex(SystemExit, "REFUSED missing head-built musl runtime"):
            self.invoke()

    def test_missing_sysroot_variable_is_final(self):
        del self.env["WOLF_STATIC_ENTRY_SYSROOT"]
        with self.assertRaisesRegex(SystemExit, "REFUSED missing verified musl sysroot"):
            self.invoke()

    def test_missing_sysroot_directory_is_final(self):
        self.sysroot.rmdir()
        with self.assertRaisesRegex(SystemExit, "REFUSED missing verified musl sysroot"):
            self.invoke()

    def test_digest_fetch_failure_is_final(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.invoke(subprocess.CalledProcessError(1, "ci/fetch-entry-bundle"))
        self.assertFalse(self.envfile.exists())

    def test_ci_builds_shared_runtime_before_real_linker_inputs(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        lint = workflow.split("  lint-test:", 1)[1].split("  variadic-abi:", 1)[0]
        self.assertIn("cargo xtask dist-rt --target x86_64-unknown-linux-musl", lint)
        self.assertIn("rt=$PWD/target/dist-rt/x86_64-unknown-linux-musl/release/libwolf_rt.a", lint)
        self.assertIn('echo "WOLF_STATIC_ENTRY_MUSL_RT=$rt"', lint)
        self.assertIn('sha256sum "$rt"; sha256sum "$rt"', lint)
        self.assertLess(lint.index("cargo xtask dist-rt"), lint.index("ci/fetch-test-link-bundle"))
        self.assertEqual(lint.count("path: .link-bundle-cache"), 1)
        self.assertIn("python3 ci/link-inputs-test.py", workflow)


if __name__ == "__main__":
    unittest.main()
