#!/usr/bin/env python3
"""Offline release metadata gate for wolf 0.2.39; no compilation or network."""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
VERSION = "0.2.39"
LUPIN = "0.1.61"
LUPIN_PIN = "5f1f1c949582f53121e4047bc83c86f23ff72240"
STD = "2411d868ee5c7ce6aee272c0475f049f29bab65d"


def crate_versions(lock):
    blocks = (ROOT / lock).read_text().split("[[package]]")
    return [re.search(r'^version = "([^"]+)"', b, re.M)[1] for b in blocks
            if re.search(r'^name = "(?:wolf_[^"]+|xtask)"', b, re.M)]


def release_entry():
    text = (ROOT / "CHANGELOG.md").read_text()
    m = re.search(rf"^## {re.escape(VERSION)} — \d{{4}}-\d{{2}}-\d{{2}}$", text, re.M)
    assert m, f"no `## {VERSION} — <date>` heading"
    rest = text[m.end():]
    return rest[: re.search(r"^## ", rest, re.M).start()]


class ReleasePosture(unittest.TestCase):
    def test_workspace_and_lock_versions(self):
        self.assertIn(f'version = "{VERSION}"', (ROOT / "Cargo.toml").read_text())
        versions = crate_versions("Cargo.lock")
        self.assertGreater(len(versions), 20)
        self.assertEqual(set(versions), {VERSION})

    def test_fuzz_lock_records_the_release(self):
        versions = crate_versions("fuzz/Cargo.lock")
        self.assertGreater(len(versions), 10)
        self.assertEqual(set(versions), {VERSION})

    def test_interface_snapshot_carries_the_toolchain(self):
        snap = (ROOT / "crates/wolf_sema/tests/snapshots/"
                "interface_pretty__two_mod_interfaces.snap").read_text()
        self.assertEqual(snap.count(f"toolchain {VERSION} "), 2)

    def test_published_pairing(self):
        pairing = (ROOT / "crates/wolf_driver/PAIRING").read_text()
        self.assertIn(f"lupin-version = {LUPIN}\n", pairing)
        self.assertIn(f"lupin-pin = {LUPIN_PIN}\n", pairing)

    def test_exact_bundled_std(self):
        self.assertIn(f"wolf-std-commit = {STD}\n",
                      (ROOT / "crates/wolf_driver/STD-PIN").read_text())

    def test_changelog_opens_with_the_release(self):
        text = (ROOT / "CHANGELOG.md").read_text()
        self.assertNotIn("## Unreleased", text)
        self.assertTrue(text.startswith(f"# Changelog\n\n## {VERSION} — "))

    def test_changelog_carries_the_whole_round(self):
        entry = release_entry()
        for sprint in ("s285", "s286", "s287", "s289", "s290", "s292"):
            self.assertIn(sprint, entry)
        for name in ("os_spawn_session", "os_setsid", "os_sid", "os_term_open_pty",
                     "os_term_pty_name", "os_term_set_size", "os_term_attr",
                     "os_term_set_attr"):
            self.assertIn(f"`{name}", entry)
        for word in ("E0415", "E0827", "E0828", "E0404", "E0301", "`min(a, b)`",
                     "#1051", f"lupin {LUPIN}", STD[:8], "scrypt"):
            self.assertIn(word, entry)

    def test_release_note_names_the_pairing_and_its_limits(self):
        note = (ROOT / f"docs/releases/{VERSION}.md").read_text()
        self.assertIn(f"# wolf {VERSION}", note)
        self.assertIn(f"lupin {LUPIN}", note)
        self.assertIn(LUPIN_PIN, note)
        self.assertIn(STD, note)
        # The terminal surface, by name, and lupin's answer to it.
        for name in ("os_spawn_session", "os_setsid", "os_term_open_pty",
                     "os_term_set_attr"):
            self.assertIn(name, note)
        self.assertIn("refused by name", note)
        # The race this release fixes is named with its issue.
        self.assertIn("wolf-lang/issues/1051", note)
        # The known limits at the cut are named, not claimed fixed.
        for issue in ("1088", "1092", "1093", "1094", "1095"):
            self.assertIn(f"wolf-lang/issues/{issue}", note)
        self.assertIn("wolf-std/issues/133", note)
        self.assertIn("wolf-std/issues/125", note)

    def test_readme_names_the_release(self):
        readme = (ROOT / "README.md").read_text()
        self.assertIn(f"The current release is v{VERSION}", readme)
        self.assertIn(f"docs/releases/{VERSION}.md", readme)
        self.assertIn(f"history of v{VERSION}", readme)


if __name__ == "__main__":
    unittest.main(verbosity=2)
