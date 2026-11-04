#!/usr/bin/env python3
"""The recorded debug and release takes run in CI's gauntlet environment.

Static, offline: it reads ci/r42/validation-steps.sh. CI's linux gauntlet
(.github/workflows/ci.yml) sets no std root for either machine and finds
each compiler's runtime beside it, and it requires the static entry's
inputs. A take that exports WOLF_STD or LUPIN_STD answers the no-std rows
and the directory std roots differently from CI; one that skips a missing
sysroot hides sixteen static-entry cells.
"""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
STEPS = ROOT / "ci/r42/validation-steps.sh"


def branch(text, label):
    start = text.index(f"\n    {label})\n")
    end = text.index("\n        ;;\n", start)
    return text[start:end]


class CiMirrorEnvironment(unittest.TestCase):
    def setUp(self):
        self.text = STEPS.read_text()
        self.ci = branch(self.text, "validation-debug|validation-release")
        self.gauntlet = self.ci.index("cargo xtask ci")

    def before_gauntlet(self, pattern):
        m = re.search(pattern, self.ci, re.M)
        self.assertIsNotNone(m, pattern)
        self.assertLess(m.start(), self.gauntlet, pattern)

    def test_no_std_root_for_either_machine(self):
        self.before_gauntlet(r"^\s*unset WOLF_STD LUPIN_STD WOLF_RT_LIB$")

    def test_static_entry_inputs_are_required(self):
        self.before_gauntlet(r"^\s*export WOLF_STATIC_ENTRY_REQUIRE=1$")

    def test_nothing_restores_a_root_before_the_gauntlet(self):
        head = self.ci[:self.gauntlet]
        unset = head.rindex("unset WOLF_STD LUPIN_STD WOLF_RT_LIB")
        self.assertNotRegex(head[unset:], r"(export|^)\s*(WOLF_STD|LUPIN_STD|WOLF_RT_LIB)=")

    def test_the_std_and_differ_takes_keep_their_roots(self):
        self.assertRegex(self.text, r"(?m)^export WOLF_STD=/lane/builds/wolf-std-")
        std = branch(self.text,
                     "validation-std-tests|validation-std-docs|"
                     "validation-std-published-tests|validation-std-published-docs")
        self.assertNotIn("unset WOLF_STD", std)


if __name__ == "__main__":
    unittest.main(verbosity=2)
