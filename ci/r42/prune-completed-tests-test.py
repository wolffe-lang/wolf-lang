#!/usr/bin/env python3
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "ci/r42/prune-completed-tests.py"


class CompletedTestPruner(unittest.TestCase):
    def run_case(self, marker_value):
        with tempfile.TemporaryDirectory(dir=ROOT.parent) as scratch:
            root = pathlib.Path(scratch)
            deps = root / "target/debug/deps"
            deps.mkdir(parents=True)
            test = deps / "fixture-0123456789abcdef"
            test.write_text("completed executable")
            library = deps / "libfixture-0123456789abcdef.rlib"
            library.write_text("retain dependency")
            compiler = root / "target/debug/wolf"
            compiler.write_text("retain root compiler")
            marker = root / "completed.exit"
            marker.write_text(marker_value)
            log = root / "record.log"
            log.write_text("  Running (target/debug/deps/fixture-0123456789abcdef)\n"
                           "  Running (target/debug/deps/libfixture-0123456789abcdef.rlib)\n")
            output = root / "pruned.txt"
            result = subprocess.run([sys.executable, "-I", str(SCRIPT), str(root), "debug",
                                     str(log), str(marker), str(output)], capture_output=True)
            self.assertTrue(library.exists())
            self.assertTrue(compiler.exists())
            if marker_value == "0\n":
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse(test.exists())
                self.assertIn("Pruned 1 completed debug test executables", result.stdout.decode())
                self.assertIn(test.name, output.read_text())
            else:
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(test.exists())
                self.assertFalse(output.exists())

    def test_remove_only_completed_test_executable(self):
        self.run_case("0\n")

    def test_refuse_a_nonzero_phase(self):
        self.run_case("101\n")


if __name__ == "__main__":
    unittest.main(verbosity=2)
