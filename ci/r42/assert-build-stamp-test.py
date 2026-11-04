#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest

PATH = pathlib.Path(__file__).with_name("assert-build-stamp.py")
SPEC = importlib.util.spec_from_file_location("build_stamp", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BuildStamp(unittest.TestCase):
    def test_full_and_short_current_head_match(self):
        head = "1234567890abcdef" * 2 + "12345678"
        for value in (head, head[:7]):
            self.assertTrue(MODULE.matches(f"wolf 0.2.37+dev.{value} (wolfgang, pin {value})", head))

    def test_unknown_old_or_false_release_stamp_does_not_match(self):
        head = "1234567890abcdef" * 2 + "12345678"
        for line in ("wolf 0.2.37+dev.unknown (wolfgang, pin unknown)",
                     "wolf 0.2.37+dev.abcdef0 (wolfgang, pin abcdef0)",
                     "wolf 0.2.37 (wolfgang, pin 1234567)",
                     "wolf 0.2.37+dev.1234567 (wolfgang, pin abcdef0)"):
            self.assertFalse(MODULE.matches(line, head))


if __name__ == "__main__":
    unittest.main(verbosity=2)
