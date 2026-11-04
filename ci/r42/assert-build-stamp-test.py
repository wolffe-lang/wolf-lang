#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest

PATH = pathlib.Path(__file__).with_name("assert-build-stamp.py")
SPEC = importlib.util.spec_from_file_location("build_stamp", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BuildStamp(unittest.TestCase):
    def test_seven_character_current_head_matches(self):
        head = "1234567890abcdef" * 2 + "12345678"
        value = head[:7]
        self.assertTrue(MODULE.matches(f"wolf 0.2.37+dev.{value} (wolfgang, pin {value})", head))

    def test_full_forty_character_stamp_is_refused(self):
        head = "1234567890abcdef" * 2 + "12345678"
        self.assertFalse(MODULE.matches(f"wolf 0.2.37+dev.{head} (wolfgang, pin {head})", head))

    def test_every_noncanonical_hex_width_is_refused(self):
        head = "1234567890abcdef" * 2 + "12345678"
        for width in range(1, 41):
            if width == 7:
                continue
            with self.subTest(width=width):
                value = head[:width]
                self.assertFalse(MODULE.matches(f"wolf 0.2.37+dev.{value} (wolfgang, pin {value})", head))

    def test_launcher_derives_contract_width_from_immutable_snapshot(self):
        launcher = PATH.with_name("wolf-hasu-run.sh").read_text()
        self.assertIn('stamp=$(git -C "$snapshot" rev-parse --short=7 HEAD)', launcher)
        self.assertIn('-e "WOLF_COMMIT=$stamp"', launcher)
        self.assertNotIn('-e "WOLF_COMMIT=$head"', launcher)
        self.assertNotIn('-e WOLF_RELEASE=', launcher)

    def test_unknown_old_or_false_release_stamp_does_not_match(self):
        head = "1234567890abcdef" * 2 + "12345678"
        for line in ("wolf 0.2.37+dev.unknown (wolfgang, pin unknown)",
                     "wolf 0.2.37+dev.abcdef0 (wolfgang, pin abcdef0)",
                     "wolf 0.2.37 (wolfgang, pin 1234567)",
                     "wolf 0.2.37+dev.1234567 (wolfgang, pin abcdef0)"):
            self.assertFalse(MODULE.matches(line, head))


if __name__ == "__main__":
    unittest.main(verbosity=2)
